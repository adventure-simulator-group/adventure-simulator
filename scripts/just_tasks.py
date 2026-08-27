#!/usr/bin/env python3
"""Cross-platform implementations of stateful and compound just recipes."""

from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlparse


ROOT = Path(__file__).resolve().parents[1]
MODULE_DIR = ROOT / "crates" / "adventuresim-stdb-module"
STRATEGIC_STATIC = MODULE_DIR / "static"
WEB_STATIC = ROOT / "crates" / "strategic-web" / "static"
SPACETIME_VERSION = "2.6.1"
SPACETIME_URL = "http://localhost:3000"
SPACETIME_DATABASE = "adventuresim-stdb-module"
JWT_RE = re.compile(r"[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+")


def executable(name: str, message: str | None = None) -> str:
    resolved = shutil.which(name)
    if resolved is None:
        raise RuntimeError(message or f"Missing required executable: {name}")
    return resolved


def run(command: list[str], *, cwd: Path = ROOT, env: dict[str, str] | None = None) -> int:
    return subprocess.run(command, cwd=cwd, env=env).returncode


def port_is_open(port: int, host: str = "127.0.0.1") -> bool:
    with socket.socket() as connection:
        connection.settimeout(0.2)
        return connection.connect_ex((host, port)) == 0


def canonical_run_dir() -> Path:
    return Path(tempfile.gettempdir()) / "adventure-simulator-1"


def wait_for_port(port: int, timeout: float, process: subprocess.Popen[object] | None = None) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if port_is_open(port):
            return True
        if process is not None and process.poll() is not None:
            return False
        time.sleep(0.1)
    return port_is_open(port)


def spacetime_version_check(version: str = SPACETIME_VERSION) -> int:
    spacetime = executable(
        "spacetime",
        f"Missing 'spacetime' CLI. Install version {version} before running.",
    )
    result = subprocess.run(
        [spacetime, "--version"], cwd=ROOT, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    required = (
        f"spacetimedb tool version {version};",
        f"spacetimedb-lib version {version};",
    )
    if result.returncode or any(item not in result.stdout for item in required):
        print(f"Expected SpacetimeDB CLI and library {version}, but found:", file=sys.stderr)
        print(result.stdout, file=sys.stderr, end="")
        return 1
    return 0


def spacetime_start(bind: str, port: int) -> int:
    url = f"http://localhost:{port}"
    if port_is_open(port):
        print(f"SpacetimeDB already running on {url}")
        return 0
    run_dir = canonical_run_dir()
    run_dir.mkdir(parents=True, exist_ok=True)
    pid_file = run_dir / "spacetime.pid"
    pid_file.unlink(missing_ok=True)
    log_path = run_dir / "spacetime.log"
    flags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
    with log_path.open("a", encoding="utf-8") as log:
        process = subprocess.Popen(
            [executable("spacetime"), "start", "--listen-addr", f"{bind}:{port}"],
            cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
            start_new_session=os.name != "nt", creationflags=flags,
        )
    pid_file.write_text(str(process.pid), encoding="ascii")
    if not wait_for_port(port, 2.0, process):
        print(f"SpacetimeDB failed to start. See {log_path}", file=sys.stderr)
        return 1
    print(f"SpacetimeDB started on {url}")
    return 0


def process_exists(pid: int) -> bool:
    if os.name == "nt":
        # os.kill(pid, 0) terminates rather than probes on Windows. Query the
        # process handle and require the STILL_ACTIVE status instead.
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
        kernel32.OpenProcess.restype = wintypes.HANDLE
        kernel32.GetExitCodeProcess.argtypes = (wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD))
        kernel32.GetExitCodeProcess.restype = wintypes.BOOL
        kernel32.CloseHandle.argtypes = (wintypes.HANDLE,)
        kernel32.CloseHandle.restype = wintypes.BOOL
        handle = kernel32.OpenProcess(0x1000, False, pid)
        if not handle:
            return False
        try:
            exit_code = wintypes.DWORD()
            return bool(kernel32.GetExitCodeProcess(handle, ctypes.byref(exit_code))) and exit_code.value == 259
        finally:
            kernel32.CloseHandle(handle)
    try:
        os.kill(pid, 0)
        return True
    except (OSError, ProcessLookupError):
        return False


def spacetime_stop() -> int:
    pid_file = canonical_run_dir() / "spacetime.pid"
    try:
        pid = int(pid_file.read_text(encoding="ascii").strip())
    except (FileNotFoundError, ValueError):
        pid_file.unlink(missing_ok=True)
        print("SpacetimeDB not running")
        return 0
    if process_exists(pid):
        try:
            os.kill(pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        print("SpacetimeDB stopped")
    else:
        print("SpacetimeDB not running")
    pid_file.unlink(missing_ok=True)
    return 0


def spacetime_status(port: int) -> int:
    state = "running" if port_is_open(port) else "not running"
    suffix = f" (http://localhost:{port})" if state == "running" else ""
    print(f"SpacetimeDB: {state}{suffix}")
    return 0


def web_environment(
    spacetime_url: str = SPACETIME_URL,
    database: str = SPACETIME_DATABASE,
    bind_address: str = "127.0.0.1:8080",
    static_dir: Path = WEB_STATIC,
    tactical_static_dir: Path = STRATEGIC_STATIC,
    spacetime_token: str | None = None,
) -> dict[str, str]:
    environment = os.environ.copy()
    environment.update({
        "SPACETIMEDB_HOST": spacetime_url,
        "SPACETIMEDB_DATABASE": database,
        "BIND_ADDRESS": bind_address,
        "STATIC_DIR": str(static_dir.resolve()),
        "TACTICAL_STATIC_DIR": str(tactical_static_dir.resolve()),
    })
    if spacetime_token is not None:
        environment["SPACETIMEDB_TOKEN"] = spacetime_token
    return environment


def spacetime_auth_token() -> str:
    result = subprocess.run(
        [executable("spacetime"), "login", "show", "--token"],
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if result.returncode:
        raise RuntimeError(
            "SpacetimeDB login is required for the strategic gateway; "
            "run `spacetime login` and retry"
        )
    tokens = JWT_RE.findall(result.stdout)
    if len(tokens) != 1:
        raise RuntimeError(
            "SpacetimeDB CLI did not return exactly one authenticated gateway token"
        )
    return tokens[0]


def web(args: argparse.Namespace) -> int:
    spacetime_token = os.environ.get("SPACETIMEDB_TOKEN") or spacetime_auth_token()
    environment = web_environment(
        args.spacetime_url, args.database, args.bind_address,
        Path(args.static_dir), Path(args.tactical_static_dir),
        spacetime_token,
    )
    if args.strategic_only:
        if run([executable("just"), "_spawner-stop"]):
            return 1
        print("\nStarting strategic-web server (strategic-only mode)...")
        print(f"Open: http://{args.bind_address}\nTactical server spawning is disabled.\n")
    else:
        if run([executable("just"), "_spawner-start"]):
            return 1
        if args.secure:
            caddy = executable(os.environ.get("CADDY_BIN", "caddy"))
            if run([caddy, "start", "--config", args.caddy_config, "--adapter", "caddyfile"]):
                return 1
            print("\nStarting strategic-web behind HTTPS...")
            print(f"Open: https://localhost:{args.secure_port}\nBackend: http://{args.bind_address}\n")
            try:
                return run([executable("cargo"), "run", "-p", "strategic-web"], env=environment)
            finally:
                subprocess.run(
                    [caddy, "stop", "--address", "127.0.0.1:2020"], cwd=ROOT,
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                )
        print("\nStarting strategic-web server...")
        print(f"Open: http://{args.bind_address}\nTactical servers bind on 127.0.0.1:{args.tactical_web_port}+\n")
    return run([executable("cargo"), "run", "-p", "strategic-web"], env=environment)


def caddy(action: str, config: str) -> int:
    name = os.environ.get("CADDY_BIN", "caddy")
    binary = executable(
        name,
        f"Missing Caddy executable '{name}'. Install it or set CADDY_BIN to its full path.",
    )
    if action == "check":
        return 0
    return run([binary, "trust", "--config", config, "--adapter", "caddyfile"])


def build_strategic(module_dir: Path) -> int:
    return run([executable("spacetime"), "build"], cwd=module_dir)


def generate_bindings(module_dir: Path) -> int:
    print("Generating SpacetimeDB client bindings...")
    code = run([
        executable("spacetime"), "generate", "--lang", "rust", "--out-dir",
        "crates/adventuresim-stdb-client/src", "--module-path", str(module_dir),
    ])
    if code:
        return code
    code = run([executable("cargo"), "fmt", "--package", "adventuresim-stdb-client"])
    if not code:
        print("Bindings generated in crates/adventuresim-stdb-client/src/")
    return code


def run_spawner(spacetime_url: str, database: str, base_port: str) -> int:
    server = ROOT / "target" / "debug" / (
        "adventuresim-tactical-server.exe" if os.name == "nt" else "adventuresim-tactical-server"
    )
    return run([
        executable("cargo"), "run", "--package", "adventuresim-tactical-server-dispatcher", "--",
        "--spacetimedb-url", spacetime_url, "--spacetimedb-module", database,
        "--tactical-server-bin", str(server), "--base-port", base_port,
    ])


def refuse(message: str) -> int:
    print(message, file=sys.stderr)
    return 2


def validate_destructive_world_target(server: str, database: str) -> None:
    parsed = urlparse(server)
    if (
        parsed.scheme not in {"http", "https"}
        or parsed.hostname not in {"localhost", "127.0.0.1", "::1"}
        or parsed.port is None
        or parsed.username
        or parsed.password
        or parsed.path not in {"", "/"}
        or parsed.query
        or parsed.fragment
    ):
        raise RuntimeError(
            "destructive world loading requires a bare loopback server URL with an explicit port"
        )
    if (
        not database.startswith("adventuresim-")
        or len(database) > 128
        or any(character not in "abcdefghijklmnopqrstuvwxyz0123456789-" for character in database)
    ):
        raise RuntimeError(
            "destructive world loading requires a lowercase adventuresim-* database name"
        )


def recreate_world_database(server: str, database: str, module_dir: Path) -> int:
    validate_destructive_world_target(server, database)
    print(
        f"Recreating disposable local database {database!r} on {server}; "
        "all existing data will be discarded.",
        flush=True,
    )
    return run([
        executable("spacetime"), "publish", "--delete-data=always", "--yes",
        "--server", server, database,
    ], cwd=module_dir)


def verified_world_identity(world_input: Path) -> tuple[str, str]:
    expected_world = (ROOT / "target" / "world-1544.json").resolve()
    if world_input.resolve() != expected_world:
        raise RuntimeError(
            f"full-world simulation requires authoritative artifact {expected_world}"
        )
    lock = json.loads((ROOT / "world-runtime-release.lock.json").read_text(encoding="utf-8"))
    record = next(
        (
            item
            for item in lock.get("files", [])
            if item.get("destination") == "target/world-1544.json"
        ),
        None,
    )
    if record is None:
        raise RuntimeError("world runtime lock does not pin target/world-1544.json")
    artifact = expected_world.read_bytes()
    sha256 = hashlib.sha256(artifact).hexdigest()
    if len(artifact) != record.get("size") or sha256 != record.get("sha256"):
        raise RuntimeError("target/world-1544.json does not match the pinned runtime release")
    document = json.loads(artifact)
    manifest_digest = document.get("metadata", {}).get("manifest_digest")
    if not isinstance(manifest_digest, str) or len(manifest_digest) != 64:
        raise RuntimeError("pinned world artifact has no valid manifest digest")
    return sha256, manifest_digest


def strategic_sim(
    seed: str, population: str, cycles: str, duration_days: str | None, party_size: str,
    spacetime_url: str, module_dir: Path, output_dir: Path,
    world_input: Path | None = None,
    require_quest_coverage: bool = False,
) -> int:
    output_dir = output_dir.resolve()
    if output_dir.exists():
        return refuse(f"Refusing to overwrite simulation output directory: {output_dir}")
    output_dir.mkdir(parents=True)
    token = secrets.token_hex(32)
    nonce = f"{time.time_ns()}-{os.getpid()}-{secrets.token_hex(4)}"
    database = f"adventuresim-sim-{nonce}"
    metadata_path = output_dir / "launcher.json"
    metadata = {
        "format_version": 1,
        "database": database,
        "run_nonce": nonce,
        "spacetime_url": spacetime_url,
        "world_mode": "compiled_world_1544" if world_input else "fixture",
        "world_input": str(world_input.resolve()) if world_input else None,
        "status": "starting",
    }
    world_sha256 = None
    world_manifest_digest = None
    metadata_path.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    environment = os.environ.copy()
    environment["ADVENTURESIM_SIM_BOOTSTRAP_TOKEN"] = token
    import_environment = os.environ.copy()
    import_environment.pop("ADVENTURESIM_SIM_BOOTSTRAP_TOKEN", None)
    result_code = 1
    cleanup_failed = False
    spacetime_executable = None
    stage = "world_validation" if world_input is not None else "publish"
    try:
        if world_input is not None:
            world_sha256, world_manifest_digest = verified_world_identity(world_input)
            metadata["world_sha256"] = world_sha256
            metadata["expected_world_manifest_digest"] = world_manifest_digest
        stage = "publish"
        spacetime_executable = executable("spacetime")
        code = run(
            [spacetime_executable, "publish", "--server", spacetime_url, database],
            cwd=module_dir, env=environment,
        )
        if code:
            metadata["status"] = "publish_failed"
            result_code = code
        elif world_input is not None:
            stage = "world_import"
            expected_world = world_input.resolve()
            code = run([
                executable("cargo"), "run", "--package", "adventuresim-world-import",
                "--bin", "adventuresim-world-import", "--", "--input", str(expected_world),
                "--load", "--server", spacetime_url, "--database", database,
            ], env=import_environment)
            if code:
                metadata["status"] = "world_import_failed"
                result_code = code
            else:
                stage = "simulator"
                command = [
                    executable("cargo"), "run", "-p", "adventuresim-strategic-sim", "--",
                    "core-loop", "--host", spacetime_url, "--database", database,
                    "--run-nonce", nonce, "--seed", seed, "--population", population,
                    "--cycles", cycles, "--party-size", party_size,
                    "--output", str(output_dir / "report.json"),
                    "--failure-output", str(output_dir / "failure.json"),
                    "--imported-world", "--expected-world-manifest-digest",
                    world_manifest_digest,
                ]
                if duration_days is not None:
                    command.extend(["--duration-days", duration_days])
                result_code = run(command, env=environment)
                metadata["status"] = (
                    "completed" if result_code == 0 else "simulator_failed"
                )
        else:
            stage = "simulator"
            command = [
                executable("cargo"), "run", "-p", "adventuresim-strategic-sim", "--",
                "core-loop", "--host", spacetime_url, "--database", database,
                "--run-nonce", nonce, "--seed", seed, "--population", population,
                "--cycles", cycles, "--party-size", party_size,
                "--output", str(output_dir / "report.json"),
                "--failure-output", str(output_dir / "failure.json"),
            ]
            if duration_days is not None:
                command.extend(["--duration-days", duration_days])
            if require_quest_coverage:
                command.append("--require-quest-coverage")
            result_code = run(command, env=environment)
            metadata["status"] = "completed" if result_code == 0 else "simulator_failed"
        failure_path = output_dir / "failure.json"
        if result_code != 0 and failure_path.is_file():
            failure = json.loads(failure_path.read_text(encoding="utf-8"))
            category = failure.get("category")
            if isinstance(category, str) and category:
                metadata["failure_artifact"] = failure_path.name
                metadata["failure_category"] = category
    except Exception:
        metadata["status"] = f"{stage}_failed"
        raise
    finally:
        try:
            cleanup_executable = spacetime_executable or executable("spacetime")
            cleanup = subprocess.run(
                [
                    cleanup_executable,
                    "delete",
                    "--yes",
                    "--server",
                    spacetime_url,
                    database,
                ],
                cwd=ROOT,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            cleanup_failed = cleanup.returncode != 0
        except (OSError, RuntimeError, subprocess.SubprocessError):
            cleanup_failed = True
        if cleanup_failed:
            metadata["run_status"] = metadata["status"]
            metadata["status"] = "cleanup_failed"
        metadata_path.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    return 1 if cleanup_failed and result_code == 0 else result_code


def sync_tree(source: Path, destination: Path, *, clear: bool) -> None:
    if clear and destination.exists():
        shutil.rmtree(destination)
    if source.is_dir():
        shutil.copytree(source, destination, dirs_exist_ok=True)


def kill_windows_tactical_processes() -> None:
    cmd = shutil.which("cmd.exe")
    if not cmd:
        return
    for image in ("adventuresim-tactical-server.exe", "adventuresim-tactical-client.exe"):
        subprocess.run(
            [cmd, "/C", "taskkill", "/IM", image, "/F"],
            cwd=Path("/mnt/c") if Path("/mnt/c").is_dir() else ROOT,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )


def win_dev() -> int:
    if not Path("/mnt/c").is_dir() or shutil.which("cmd.exe") is None:
        raise RuntimeError("win-dev must run inside WSL with Windows interop enabled")
    executable("x86_64-w64-mingw32-gcc", "Missing MinGW linker: install gcc-mingw-w64-x86-64")
    target = "x86_64-pc-windows-gnu"
    installed = subprocess.run(
        [executable("rustup"), "target", "list", "--installed"], text=True,
        stdout=subprocess.PIPE, check=True,
    ).stdout.splitlines()
    if target not in installed:
        print(f"Installing Rust target {target}...")
        if run([executable("rustup"), "target", "add", target]):
            return 1
    kill_windows_tactical_processes()
    time.sleep(0.5)
    print("Starting strategic development stack...")
    dev = subprocess.Popen([executable("just"), "dev"], cwd=ROOT, start_new_session=True)
    try:
        if not wait_for_port(8080, 300, dev):
            raise RuntimeError("Strategic development stack exited or timed out before becoming ready")
        for package in ("adventuresim-tactical-server", "adventuresim-tactical-client"):
            print(f"Building {package} (Windows)...")
            if run([executable("cargo"), "build", "-p", package, "--features", "debug", "--target", target, "--profile", "win-dev"]):
                return 1
        stage = Path("/mnt/e/adventure-sim-dev")
        stage.mkdir(parents=True, exist_ok=True)
        output = ROOT / "target" / target / "win-dev"
        shutil.copy2(output / "adventuresim-tactical-server.exe", stage)
        shutil.copy2(output / "adventuresim-tactical-client.exe", stage)
        sync_tree(ROOT / "assets", stage / "assets", clear=True)
        for assets in (ROOT / "crates").glob("*/assets"):
            sync_tree(assets, stage / "assets", clear=False)
        server = subprocess.Popen([
            str(stage / "adventuresim-tactical-server.exe"), "--addr", "0.0.0.0:6000",
            "--mission-id", "test-mission", "--scene-key", "woodland", "--spacetimedb-url",
            SPACETIME_URL, "--spacetimedb-module", SPACETIME_DATABASE,
            "--expected-party-members", "1", "--bots", "3", "--no-timeout",
        ], cwd=stage)
        time.sleep(3)
        client = subprocess.Popen([
            str(stage / "adventuresim-tactical-client.exe"), "--id", "0", "--server-addr", "127.0.0.1:6000",
        ], cwd=stage)
        return client.wait() if server.poll() is None else server.returncode or 1
    finally:
        print("\nShutting down...")
        kill_windows_tactical_processes()
        if dev.poll() is None:
            os.killpg(dev.pid, signal.SIGTERM)
        dev.wait()


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    commands = result.add_subparsers(dest="command", required=True)
    commands.add_parser("preflight")
    version = commands.add_parser("spacetime-version-check")
    version.add_argument("--version", default=SPACETIME_VERSION)
    start = commands.add_parser("spacetime-start")
    start.add_argument("--bind", default="127.0.0.1")
    start.add_argument("--port", type=int, default=3000)
    commands.add_parser("spacetime-stop")
    status = commands.add_parser("spacetime-status")
    status.add_argument("--port", type=int, default=3000)
    web_parser = commands.add_parser("web")
    web_parser.add_argument("--strategic-only", action="store_true")
    web_parser.add_argument("--secure", action="store_true")
    web_parser.add_argument("--spacetime-url", default=SPACETIME_URL)
    web_parser.add_argument("--database", default=SPACETIME_DATABASE)
    web_parser.add_argument("--bind-address", default="127.0.0.1:8080")
    web_parser.add_argument("--static-dir", default=str(WEB_STATIC))
    web_parser.add_argument("--tactical-static-dir", default=str(STRATEGIC_STATIC))
    web_parser.add_argument("--tactical-web-port", default="6001")
    web_parser.add_argument("--secure-port", default="8443")
    web_parser.add_argument("--caddy-config", default="Caddyfile.dev")
    caddy_parser = commands.add_parser("caddy")
    caddy_parser.add_argument("action", choices=("check", "trust"))
    caddy_parser.add_argument("--config", default="Caddyfile.dev")
    build = commands.add_parser("build-strategic")
    build.add_argument("--module-dir", default=str(MODULE_DIR))
    bindings = commands.add_parser("generate-bindings")
    bindings.add_argument("--module-dir", default=str(MODULE_DIR))
    spawner = commands.add_parser("spawner")
    spawner.add_argument("--spacetime-url", default=SPACETIME_URL)
    spawner.add_argument("--database", default=SPACETIME_DATABASE)
    spawner.add_argument("--base-port", default="6000")
    recreate = commands.add_parser("recreate-world-database")
    recreate.add_argument("--server", required=True)
    recreate.add_argument("--database", required=True)
    recreate.add_argument("--module-dir", default=str(MODULE_DIR))
    refusal = commands.add_parser("refuse")
    refusal.add_argument("message")
    simulation = commands.add_parser("strategic-sim-core-loop")
    simulation.add_argument("seed")
    simulation.add_argument("population")
    simulation.add_argument("cycles")
    simulation.add_argument("party_size")
    simulation.add_argument("output_dir")
    simulation.add_argument("--duration-days")
    simulation.add_argument("--spacetime-url", default=SPACETIME_URL)
    simulation.add_argument("--module-dir", default=str(MODULE_DIR))
    simulation.add_argument("--world-input")
    simulation.add_argument("--require-quest-coverage", action="store_true")
    commands.add_parser("win-dev")
    return result


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        if args.command == "preflight":
            return 0
        if args.command == "spacetime-version-check":
            return spacetime_version_check(args.version)
        if args.command == "spacetime-start":
            return spacetime_start(args.bind, args.port)
        if args.command == "spacetime-stop":
            return spacetime_stop()
        if args.command == "spacetime-status":
            return spacetime_status(args.port)
        if args.command == "web":
            return web(args)
        if args.command == "caddy":
            return caddy(args.action, args.config)
        if args.command == "build-strategic":
            return build_strategic(Path(args.module_dir))
        if args.command == "generate-bindings":
            return generate_bindings(Path(args.module_dir))
        if args.command == "spawner":
            return run_spawner(args.spacetime_url, args.database, args.base_port)
        if args.command == "recreate-world-database":
            return recreate_world_database(
                args.server, args.database, Path(args.module_dir)
            )
        if args.command == "refuse":
            return refuse(args.message)
        if args.command == "strategic-sim-core-loop":
            return strategic_sim(
                args.seed, args.population, args.cycles, args.duration_days, args.party_size,
                args.spacetime_url, Path(args.module_dir), Path(args.output_dir),
                Path(args.world_input) if args.world_input else None,
                args.require_quest_coverage,
            )
        if args.command == "win-dev":
            return win_dev()
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(error, file=sys.stderr)
        return 1
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
