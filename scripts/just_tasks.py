#!/usr/bin/env python3
"""Cross-platform implementations of stateful and compound just recipes."""

from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes
import os
from pathlib import Path
import secrets
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]
MODULE_DIR = ROOT / "crates" / "adventuresim-stdb-module"
STRATEGIC_STATIC = MODULE_DIR / "static"
WEB_STATIC = ROOT / "crates" / "strategic-web" / "static"
SPACETIME_VERSION = "2.6.1"
SPACETIME_URL = "http://localhost:3000"
SPACETIME_DATABASE = "adventuresim-stdb-module"


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
) -> dict[str, str]:
    environment = os.environ.copy()
    environment.update({
        "SPACETIMEDB_HOST": spacetime_url,
        "SPACETIMEDB_DATABASE": database,
        "BIND_ADDRESS": bind_address,
        "STATIC_DIR": str(static_dir.resolve()),
        "TACTICAL_STATIC_DIR": str(tactical_static_dir.resolve()),
    })
    return environment


def web(args: argparse.Namespace) -> int:
    environment = web_environment(
        args.spacetime_url, args.database, args.bind_address,
        Path(args.static_dir), Path(args.tactical_static_dir),
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


def strategic_sim(
    seed: str, population: str, cycles: str, duration_days: str, party_size: str,
    spacetime_url: str, module_dir: Path,
) -> int:
    token = secrets.token_hex(32)
    nonce = f"{time.time_ns()}-{os.getpid()}-{secrets.token_hex(4)}"
    database = f"adventuresim-sim-{nonce}"
    environment = os.environ.copy()
    environment["ADVENTURESIM_SIM_BOOTSTRAP_TOKEN"] = token
    try:
        code = run(
            [executable("spacetime"), "publish", "--server", spacetime_url, database],
            cwd=module_dir, env=environment,
        )
        if code:
            return code
        return run([
            executable("cargo"), "run", "-p", "adventuresim-strategic-sim", "--", "core-loop",
            "--host", spacetime_url, "--database", database, "--run-nonce", nonce,
            "--seed", seed, "--population", population, "--cycles", cycles,
            "--duration-days", duration_days, "--party-size", party_size,
        ], env=environment)
    finally:
        subprocess.run(
            [executable("spacetime"), "delete", "--yes", "--server", spacetime_url, database],
            cwd=ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )


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
            "--mission-id", "test-mission", "--scene-key", "hills", "--spacetimedb-url",
            SPACETIME_URL, "--spacetimedb-module", SPACETIME_DATABASE, "--bots", "3", "--no-timeout",
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
    refusal = commands.add_parser("refuse")
    refusal.add_argument("message")
    simulation = commands.add_parser("strategic-sim-core-loop")
    simulation.add_argument("seed")
    simulation.add_argument("population")
    simulation.add_argument("cycles")
    simulation.add_argument("duration_days")
    simulation.add_argument("party_size")
    simulation.add_argument("--spacetime-url", default=SPACETIME_URL)
    simulation.add_argument("--module-dir", default=str(MODULE_DIR))
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
        if args.command == "refuse":
            return refuse(args.message)
        if args.command == "strategic-sim-core-loop":
            return strategic_sim(
                args.seed, args.population, args.cycles, args.duration_days, args.party_size,
                args.spacetime_url, Path(args.module_dir),
            )
        if args.command == "win-dev":
            return win_dev()
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(error, file=sys.stderr)
        return 1
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
