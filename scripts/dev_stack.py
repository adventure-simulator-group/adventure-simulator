#!/usr/bin/env python3
"""Safety checks used by the local SpacetimeDB development recipes."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from urllib.parse import urlparse


PROFILE_RE = re.compile(r"^[a-z][a-z0-9-]{0,31}$")
ROOT = Path(__file__).resolve().parents[1]
MODULE_DIR = ROOT / "crates" / "adventuresim-stdb-module"
CLIENT_DIR = ROOT / "crates" / "adventuresim-stdb-client" / "src"


def profile_values(name: str, base_port: int) -> dict[str, object]:
    if not PROFILE_RE.fullmatch(name):
        raise ValueError("profile must match [a-z][a-z0-9-]{0,31}")
    if not 1024 <= base_port <= 65530:
        raise ValueError("base port must be between 1024 and 65530")
    # Recipes run under bash even on Windows, so keep these as POSIX paths.
    run_dir = f"/tmp/adventure-simulator-{name}"
    return {
        "profile": name,
        "database": f"adventuresim-dev-{name}",
        "data_dir": f"{run_dir}/spacetimedb-data",
        "run_dir": f"{run_dir}/run",
        "spacetime_port": base_port,
        "web_port": base_port + 1,
        "tactical_port": base_port + 2,
    }


def validate_loopback_server(server: str, expected_port: int | None = None) -> None:
    parsed = urlparse(server)
    if parsed.scheme not in {"http", "https"} or parsed.hostname not in {"localhost", "127.0.0.1", "::1"}:
        raise ValueError("destructive local operations require an http(s) loopback server")
    if expected_port is not None and parsed.port != expected_port:
        raise ValueError(f"server port must equal isolated profile port {expected_port}")
    if parsed.username or parsed.password or parsed.path not in {"", "/"} or parsed.query or parsed.fragment:
        raise ValueError("server must be a bare loopback origin")


def run_checked(command: list[str], cwd: Path = ROOT) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, cwd=cwd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)


def publish(server: str, database: str, reset_profile: str | None, base_port: int) -> int:
    command = ["spacetime", "publish"]
    if reset_profile is not None:
        values = profile_values(reset_profile, base_port)
        validate_loopback_server(server, int(values["spacetime_port"]))
        if database != values["database"]:
            raise ValueError("reset database does not match the isolated profile identity")
        command.extend(["--delete-data=always", "--yes"])
    command.extend(["--server", server, database])
    result = run_checked(command, MODULE_DIR)
    sys.stdout.write(result.stdout)
    if result.returncode:
        print("\nSpacetimeDB rejected the module before any client or spawner was launched.", file=sys.stderr)
        print(f"Server: {server}\nDatabase: {database}", file=sys.stderr)
        print("The published database ABI is incompatible with this checkout, or publication failed.", file=sys.stderr)
        print("Data was not deleted by this command. Choose one recovery path:", file=sys.stderr)
        print("  * preserve the database and implement an explicit migration; or", file=sys.stderr)
        print("  * use `just web-isolated <name> <base-port>` for disposable demo data.", file=sys.stderr)
    return result.returncode


def seed(server: str, database: str) -> int:
    result = run_checked(["spacetime", "call", "--server", server, database, "seed_world"])
    sys.stdout.write(result.stdout)
    if result.returncode:
        print("seed_world failed; refusing to hide the reducer error.", file=sys.stderr)
    return result.returncode


def binding_differences(expected: Path, actual: Path) -> list[str]:
    expected_files = {p.name for p in expected.glob("*.rs")}
    # lib.rs is the crate's handwritten facade; SpacetimeDB owns mod.rs and
    # every other file in this directory.
    expected_files.discard("lib.rs")
    actual_files = {p.name for p in actual.glob("*.rs")}
    actual_files.discard("lib.rs")
    differences = sorted(expected_files ^ actual_files)
    for name in sorted(expected_files & actual_files):
        if expected.joinpath(name).read_bytes().replace(b"\r\n", b"\n") != actual.joinpath(name).read_bytes().replace(b"\r\n", b"\n"):
            differences.append(name)
    return differences


def verify_bindings() -> int:
    with tempfile.TemporaryDirectory(prefix="adventuresim-bindings-") as temp:
        temp_root = Path(temp)
        generated = temp_root / "src"
        generated.mkdir()
        result = run_checked([
            "spacetime", "generate", "--lang", "rust", "--out-dir", str(generated),
            "--module-path", str(MODULE_DIR), "--yes",
        ])
        if result.returncode:
            sys.stdout.write(result.stdout)
            return result.returncode
        generated.joinpath("lib.rs").write_bytes(CLIENT_DIR.joinpath("lib.rs").read_bytes())
        temp_root.joinpath("Cargo.toml").write_text(
            '[package]\nname = "binding-freshness-check"\nversion = "0.0.0"\nedition = "2024"\n'
        )
        formatted = subprocess.run(
            ["cargo", "fmt", "--manifest-path", str(temp_root / "Cargo.toml")], text=True
        )
        if formatted.returncode:
            return formatted.returncode
        differences = binding_differences(CLIENT_DIR, generated)
    if differences:
        print("Generated SpacetimeDB bindings are stale:", file=sys.stderr)
        for name in differences[:20]:
            print(f"  {name}", file=sys.stderr)
        if len(differences) > 20:
            print(f"  ... and {len(differences) - 20} more", file=sys.stderr)
        print("Run `just generate-db-client`, review, and commit the generated changes.", file=sys.stderr)
        return 1
    print("SpacetimeDB client bindings match the current module schema.")
    return 0


def spawner_identity(profile: str, server: str, database: str, host: str, base_port: int) -> dict[str, object]:
    dispatcher = ROOT / "target" / "debug" / ("adventuresim-tactical-server-dispatcher.exe" if os.name == "nt" else "adventuresim-tactical-server-dispatcher")
    tactical = ROOT / "target" / "debug" / ("adventuresim-tactical-server.exe" if os.name == "nt" else "adventuresim-tactical-server")
    binaries = {}
    for name, path in (("dispatcher", dispatcher), ("tactical_server", tactical)):
        if not path.is_file():
            raise ValueError(f"{name} binary is missing; run just build-tactical")
        binaries[name] = {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
    return {
        "repository": str(ROOT),
        "profile": profile,
        "server": server,
        "database": database,
        "host": host,
        "base_port": base_port,
        "binaries": binaries,
    }


def process_is_running(pid: int) -> bool:
    if os.name != "nt":
        try:
            os.kill(pid, 0)
            return True
        except OSError:
            return False
    import ctypes
    from ctypes import wintypes

    process_query_limited_information = 0x1000
    still_active = 259
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel32.OpenProcess.restype = wintypes.HANDLE
    handle = kernel32.OpenProcess(process_query_limited_information, False, pid)
    if not handle:
        return False
    try:
        exit_code = wintypes.DWORD()
        if not kernel32.GetExitCodeProcess(handle, ctypes.byref(exit_code)):
            return False
        return exit_code.value == still_active
    finally:
        kernel32.CloseHandle(handle)


def check_spawner_identity(identity_file: Path, pid_file: Path, expected: dict[str, object]) -> int:
    if not pid_file.exists():
        return 1
    try:
        pid = int(pid_file.read_text().strip())
    except ValueError:
        pid_file.unlink(missing_ok=True)
        identity_file.unlink(missing_ok=True)
        return 1
    if not process_is_running(pid):
        pid_file.unlink(missing_ok=True)
        identity_file.unlink(missing_ok=True)
        return 1
    try:
        actual = json.loads(identity_file.read_text())
    except (OSError, json.JSONDecodeError):
        actual = None
    if actual != expected:
        print(f"Refusing to reuse tactical spawner pid {pid}: identity belongs to another checkout/profile.", file=sys.stderr)
        return 2
    print(f"Tactical spawner already running (pid {pid}) with matching identity.")
    return 0


def write_spawner_identity(identity_file: Path, identity: dict[str, object]) -> None:
    identity_file.parent.mkdir(parents=True, exist_ok=True)
    identity_file.write_text(json.dumps(identity, sort_keys=True) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    profile_parser = sub.add_parser("profile")
    profile_parser.add_argument("--name", required=True)
    profile_parser.add_argument("--base-port", type=int, required=True)
    publish_parser = sub.add_parser("publish")
    publish_parser.add_argument("--server", required=True)
    publish_parser.add_argument("--database", required=True)
    publish_parser.add_argument("--reset-profile")
    publish_parser.add_argument("--base-port", type=int, default=0)
    seed_parser = sub.add_parser("seed")
    seed_parser.add_argument("--server", required=True)
    seed_parser.add_argument("--database", required=True)
    sub.add_parser("verify-bindings")
    identity = sub.add_parser("check-spawner")
    identity.add_argument("--identity-file", type=Path, required=True)
    identity.add_argument("--pid-file", type=Path, required=True)
    writer = sub.add_parser("write-spawner")
    writer.add_argument("--identity-file", type=Path, required=True)
    for command_parser in (identity, writer):
        command_parser.add_argument("--profile", required=True)
        command_parser.add_argument("--server", required=True)
        command_parser.add_argument("--database", required=True)
        command_parser.add_argument("--host", required=True)
        command_parser.add_argument("--base-port", type=int, required=True)
    args = parser.parse_args()
    try:
        if args.command == "profile":
            print(json.dumps(profile_values(args.name, args.base_port), sort_keys=True))
            return 0
        if args.command == "publish":
            return publish(args.server, args.database, args.reset_profile, args.base_port)
        if args.command == "seed":
            return seed(args.server, args.database)
        if args.command == "verify-bindings":
            return verify_bindings()
        if args.command == "check-spawner":
            expected = spawner_identity(args.profile, args.server, args.database, args.host, args.base_port)
            return check_spawner_identity(args.identity_file, args.pid_file, expected)
        if args.command == "write-spawner":
            identity_value = spawner_identity(args.profile, args.server, args.database, args.host, args.base_port)
            write_spawner_identity(args.identity_file, identity_value)
            return 0
    except (ValueError, RuntimeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
