#!/usr/bin/env python3
"""Safety checks used by the local SpacetimeDB development recipes."""

from __future__ import annotations

import argparse
from contextlib import AbstractContextManager
from dataclasses import dataclass
from enum import Enum
import hashlib
import json
import os
import secrets
from pathlib import Path
import re
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
from urllib.parse import urlparse


PROFILE_RE = re.compile(r"^[a-z][a-z0-9-]{0,31}$")
JWT_RE = re.compile(r"[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+")
ROOT = Path(__file__).resolve().parents[1]
MODULE_DIR = ROOT / "crates" / "adventuresim-stdb-module"
CLIENT_DIR = ROOT / "crates" / "adventuresim-stdb-client" / "src"
TACTICAL_ENV_FILE = ROOT / ".env.tactical"


class ProfileMode(str, Enum):
    """The three shapes an isolated profile can run in."""

    STRATEGIC = "strategic"  # strategic-web + tactical dispatcher (full stack)
    BARE_STRATEGIC = "bare-strategic"  # strategic-web only, no dispatcher
    TACTICAL = "tactical"  # isolated DB + seeded standalone mission only


class TacticalPlayMode(str, Enum):
    """Safe fixtures exposed by the supervised native tactical launcher."""

    ANIMATION = "animation"
    COMBAT = "combat"
    NETWORKING = "networking"


def cargo_target_dir() -> Path:
    configured = os.environ.get("CARGO_TARGET_DIR")
    if configured:
        return Path(configured)
    try:
        result = subprocess.run(
            ["cargo", "-Z", "unstable-options", "config", "get", "build.target-dir"],
            capture_output=True, text=True, cwd=ROOT, timeout=5,
        )
        if result.returncode == 0:
            line = result.stdout.strip()
            if line.startswith("build.target-dir"):
                value = line.split("=", 1)[1].strip().strip('"').strip("'")
                return Path(value)
    except (subprocess.SubprocessError, OSError):
        pass
    return ROOT / "target"


def worktree_fingerprint(root: Path = ROOT) -> str:
    return hashlib.sha256(str(root.resolve()).encode("utf-8")).hexdigest()[:12]


def runtime_root() -> Path:
    if os.name == "nt":
        base = os.environ.get("LOCALAPPDATA")
        if not base:
            raise ValueError("LOCALAPPDATA is required for isolated profiles")
        return Path(base) / "AdventureSimulator" / "runtime"
    base = os.environ.get("XDG_RUNTIME_DIR") or os.environ.get("XDG_CACHE_HOME")
    if base:
        return Path(base) / "adventure-simulator"
    return Path.home() / ".cache" / "adventure-simulator" / "runtime"


def ensure_secure_directory(path: Path, containment_root: Path) -> Path:
    root_input = Path(os.path.abspath(containment_root))
    candidate_input = Path(os.path.abspath(path))
    if candidate_input != root_input and root_input not in candidate_input.parents:
        raise ValueError("profile path escapes the runtime root")
    if root_input.is_symlink():
        raise ValueError(f"refusing symlink runtime root: {root_input}")
    current_input = root_input
    for part in candidate_input.relative_to(root_input).parts:
        current_input = current_input / part
        if current_input.is_symlink():
            raise ValueError(f"refusing symlink in profile state path: {current_input}")
    root = root_input.resolve(strict=False)
    candidate = candidate_input.resolve(strict=False)
    if candidate != root and root not in candidate.parents:
        raise ValueError("profile path escapes the runtime root")
    current = root
    current.mkdir(mode=0o700, parents=True, exist_ok=True)
    for part in candidate.relative_to(root).parts:
        current = current / part
        if current.is_symlink():
            raise ValueError(f"refusing symlink in profile state path: {current}")
        current.mkdir(mode=0o700, exist_ok=True)
        if not current.is_dir():
            raise ValueError(f"profile state component is not a directory: {current}")
        if os.name != "nt":
            if current.stat().st_uid != os.getuid():
                raise ValueError(f"profile state is not owned by the current user: {current}")
            current.chmod(0o700)
    return candidate


def profile_values(
    name: str,
    base_port: int,
    *,
    root: Path = ROOT,
    state_root: Path | None = None,
) -> dict[str, object]:
    if not PROFILE_RE.fullmatch(name):
        raise ValueError("profile must match [a-z][a-z0-9-]{0,31}")
    if not 1024 <= base_port <= 65530:
        raise ValueError("base port must be between 1024 and 65530")
    fingerprint = worktree_fingerprint(root)
    state_root = state_root or runtime_root()
    profile_dir = state_root / fingerprint / name
    return {
        "profile": name,
        "worktree_fingerprint": fingerprint,
        "database": f"adventuresim-dev-{name}-{fingerprint}",
        "profile_dir": str(profile_dir),
        "data_dir": str(profile_dir / "spacetimedb-data"),
        "run_dir": str(profile_dir / "run"),
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
    return subprocess.run(
        command,
        cwd=cwd,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )


def write_console(output: str) -> None:
    encoding = getattr(sys.stdout, "encoding", None) or "utf-8"
    safe_output = output.encode(encoding, errors="replace").decode(encoding)
    sys.stdout.write(safe_output)


def publish(server: str, database: str) -> int:
    command = ["spacetime", "publish", "--server", server, database]
    result = run_checked(command, MODULE_DIR)
    write_console(result.stdout)
    if result.returncode:
        print("\nSpacetimeDB rejected the module before any client or spawner was launched.", file=sys.stderr)
        print(f"Server: {server}\nDatabase: {database}", file=sys.stderr)
        print("The published database ABI is incompatible with this checkout, or publication failed.", file=sys.stderr)
        print("Data was not deleted by this command. Choose one recovery path:", file=sys.stderr)
        print("  * preserve the database and implement an explicit migration; or", file=sys.stderr)
        print("  * use `just web-isolated <name> <base-port>` for disposable demo data.", file=sys.stderr)
    return result.returncode


def seed(server: str, database: str, bootstrap_token: str, include_damaged_demo: bool = False) -> int:
    result = run_checked([
        "spacetime", "call", "--server", server, database,
        "bootstrap_development_world", bootstrap_token,
        "true" if include_damaged_demo else "false",
    ])
    write_console(result.stdout)
    if result.returncode:
        print("development bootstrap failed; refusing to hide the reducer error.", file=sys.stderr)
    return result.returncode


def spacetime_auth_token() -> str:
    """Return the CLI's authenticated token without printing or persisting it."""
    result = run_checked(["spacetime", "login", "show", "--token"])
    if result.returncode:
        raise RuntimeError(
            "SpacetimeDB login is required for the isolated strategic gateway; "
            "run `spacetime login` and retry"
        )
    tokens = JWT_RE.findall(result.stdout)
    if len(tokens) != 1:
        raise RuntimeError(
            "SpacetimeDB CLI did not return exactly one authenticated gateway token"
        )
    return tokens[0]


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
            write_console(result.stdout)
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


def atomic_write_json(path: Path, value: dict[str, object]) -> None:
    if path.is_symlink():
        raise ValueError(f"refusing symlink metadata target: {path}")
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    temporary = path.with_name(f".{path.name}.{os.getpid()}.{time.time_ns()}.tmp")
    fd = os.open(temporary, flags, 0o600)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            json.dump(value, stream, sort_keys=True)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def secure_log(path: Path):
    if path.is_symlink():
        raise ValueError(f"refusing symlink log target: {path}")
    nofollow = getattr(os, "O_NOFOLLOW", 0)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | nofollow, 0o600)
    return os.fdopen(fd, "w", encoding="utf-8", errors="replace")


class ProfileLock(AbstractContextManager["ProfileLock"]):
    def __init__(self, path: Path):
        self.path = path
        self.stream = None

    @property
    def held(self) -> bool:
        return self.stream is not None

    def __enter__(self) -> "ProfileLock":
        if self.path.is_symlink():
            raise ValueError("refusing symlink lifecycle lock")
        fd = os.open(self.path, os.O_RDWR | os.O_CREAT, 0o600)
        if os.name != "nt":
            info = os.fstat(fd)
            if info.st_uid != os.getuid():
                os.close(fd)
                raise ValueError("lifecycle lock is not owned by the current user")
            os.fchmod(fd, 0o600)
        self.stream = os.fdopen(fd, "r+b", buffering=0)
        self.stream.seek(0, os.SEEK_END)
        if self.stream.tell() == 0:
            self.stream.write(b"0")
            self.stream.flush()
        self.stream.seek(0)
        try:
            if os.name == "nt":
                import msvcrt

                msvcrt.locking(self.stream.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl

                fcntl.flock(self.stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as error:
            self.stream.close()
            self.stream = None
            raise ValueError("isolated profile is already owned by another process") from error
        return self

    def __exit__(self, *args) -> None:
        if self.stream is None:
            return
        self.stream.seek(0)
        if os.name == "nt":
            import msvcrt

            msvcrt.locking(self.stream.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl

            fcntl.flock(self.stream.fileno(), fcntl.LOCK_UN)
        self.stream.close()
        self.stream = None


@dataclass(frozen=True)
class ResetCapability:
    profile: str
    base_port: int
    server: str
    database: str
    lock: ProfileLock
    listener: dict[str, object]


def reset_publish(capability: ResetCapability) -> int:
    if not capability.lock.held:
        raise ValueError("isolated reset requires the held profile lifecycle lock")
    values = profile_values(capability.profile, capability.base_port)
    validate_loopback_server(capability.server, int(values["spacetime_port"]))
    if capability.database != values["database"]:
        raise ValueError("reset database does not match the isolated profile identity")
    current = listener_process_snapshot(int(values["spacetime_port"]))
    if current is None or current != capability.listener or not identity_matches(capability.listener):
        raise ValueError("isolated reset ownership capability is missing or changed")
    command = [
        "spacetime", "publish", "--delete-data=always", "--yes",
        "--server", capability.server, capability.database,
    ]
    result = run_checked(command, MODULE_DIR)
    write_console(result.stdout)
    if result.returncode:
        print("\nIsolated reset publication failed.", file=sys.stderr)
        print(f"Server: {capability.server}\nDatabase: {capability.database}", file=sys.stderr)
        print("Deletion may already have occurred; this profile is disposable.", file=sys.stderr)
        print("Fix the error and rerun the isolated profile to recreate its data.", file=sys.stderr)
    return result.returncode


def ports_in_use(ports: list[int]) -> list[int]:
    occupied = []
    for port in ports:
        with socket.socket() as candidate:
            candidate.settimeout(0.2)
            if candidate.connect_ex(("127.0.0.1", port)) == 0:
                occupied.append(port)
    return occupied


def profile_ports(values: dict[str, object], mode: ProfileMode) -> list[int]:
    keys = ["spacetime_port", "web_port"]
    if mode is ProfileMode.STRATEGIC:
        keys.append("tactical_port")
    return [int(values[key]) for key in keys]


def write_tactical_env_file(
    *,
    url: str,
    database: str,
    port: int,
    mission_id: str,
    scene_key: str,
    character_id: int,
    enemy_count: int,
    tactical_claim: str | None,
    profile: str | None = None,
    worktree_fingerprint_value: str | None = None,
    run_dir: Path | None = None,
    session_id: str | None = None,
    play_mode: str | None = None,
) -> None:
    values = [
            f"TACTICAL_SPACETIMEDB_URL={url}",
            f"TACTICAL_SPACETIMEDB_MODULE={database}",
            f"TACTICAL_PORT={port}",
            f"TACTICAL_MISSION_ID={mission_id}",
            f"TACTICAL_SCENE_KEY={scene_key}",
            f"TACTICAL_CHARACTER_ID={character_id}",
            f"TACTICAL_BOTS={enemy_count}",
    ]
    if tactical_claim is not None:
        # The advanced/manual workflow needs the one-use claim. The supervised
        # launcher deliberately retains it only in memory.
        values.append(f"ADVENTURESIM_TACTICAL_CLAIM={tactical_claim}")
    optional = {
        "TACTICAL_PROFILE": profile,
        "TACTICAL_WORKTREE_FINGERPRINT": worktree_fingerprint_value,
        # python-dotenv treats backslashes as escapes even in unquoted values.
        "TACTICAL_RUN_DIR": run_dir.as_posix() if run_dir is not None else None,
        "TACTICAL_SESSION_ID": session_id,
        "TACTICAL_PLAY_MODE": play_mode,
    }
    values.extend(f"{key}={value}" for key, value in optional.items() if value is not None)
    TACTICAL_ENV_FILE.write_text("\n".join([*values, ""]), encoding="utf-8")


def read_tactical_env_file() -> dict[str, str]:
    if not TACTICAL_ENV_FILE.is_file():
        return {}
    values: dict[str, str] = {}
    for line in TACTICAL_ENV_FILE.read_text(encoding="utf-8").splitlines():
        if not line or line.lstrip().startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key] = value
    return values


def remove_tactical_env_file(expected_session_id: str | None = None) -> None:
    if expected_session_id is not None:
        current = read_tactical_env_file()
        if current.get("TACTICAL_SESSION_ID") != expected_session_id:
            return
    TACTICAL_ENV_FILE.unlink(missing_ok=True)


def listener_process_snapshot(port: int) -> dict[str, object] | None:
    if os.name == "nt":
        result = subprocess.run(
            [
                "powershell.exe", "-NoProfile", "-NonInteractive", "-Command",
                f"$c=Get-NetTCPConnection -State Listen -LocalPort {port} -ErrorAction SilentlyContinue; if($c){{$c[0].OwningProcess}}",
            ],
            text=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        )
        try:
            pid = int(result.stdout.strip())
        except ValueError:
            return None
        return process_snapshot(pid)
    proc_tcp = Path("/proc/net/tcp")
    if proc_tcp.exists():
        inode = None
        for line in proc_tcp.read_text().splitlines()[1:]:
            fields = line.split()
            if len(fields) > 9 and fields[3] == "0A" and int(fields[1].split(":")[1], 16) == port:
                inode = fields[9]
                break
        if inode is None:
            return None
        target = f"socket:[{inode}]"
        for process_dir in Path("/proc").glob("[0-9]*"):
            try:
                if any(os.readlink(fd) == target for fd in (process_dir / "fd").iterdir()):
                    return process_snapshot(int(process_dir.name))
            except (OSError, PermissionError):
                continue
        return None
    try:
        result = subprocess.run(
            ["lsof", "-nP", f"-iTCP:{port}", "-sTCP:LISTEN", "-t"],
            text=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        )
    except FileNotFoundError:
        return None
    try:
        return process_snapshot(int(result.stdout.splitlines()[0]))
    except (ValueError, IndexError, FileNotFoundError):
        return None


def process_snapshot(pid: int) -> dict[str, object] | None:
    if pid <= 0:
        return None
    if os.name != "nt":
        proc = Path("/proc") / str(pid)
        try:
            executable = str((proc / "exe").resolve(strict=True))
            fields = (proc / "stat").read_text().split()
            return {"pid": pid, "executable": executable, "start_token": fields[21]}
        except (OSError, IndexError):
            return None
    import ctypes
    from ctypes import wintypes

    query = 0x1000
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel32.OpenProcess.restype = wintypes.HANDLE
    handle = kernel32.OpenProcess(query, False, pid)
    if not handle:
        return None
    try:
        size = wintypes.DWORD(32768)
        buffer = ctypes.create_unicode_buffer(size.value)
        if not kernel32.QueryFullProcessImageNameW(handle, 0, buffer, ctypes.byref(size)):
            return None
        creation = wintypes.FILETIME()
        exit_time = wintypes.FILETIME()
        kernel = wintypes.FILETIME()
        user = wintypes.FILETIME()
        if not kernel32.GetProcessTimes(handle, ctypes.byref(creation), ctypes.byref(exit_time), ctypes.byref(kernel), ctypes.byref(user)):
            return None
        token = (creation.dwHighDateTime << 32) | creation.dwLowDateTime
        return {"pid": pid, "executable": str(Path(buffer.value).resolve()), "start_token": str(token)}
    finally:
        kernel32.CloseHandle(handle)


def executable_identity_matches(expected: object, actual: object) -> bool:
    expected_path = str(expected)
    actual_path = str(actual)
    if os.path.normcase(expected_path) == os.path.normcase(actual_path):
        return True
    expected_name = Path(expected_path).stem.lower()
    actual_name = Path(actual_path).stem.lower()
    return (
        expected_name in {"spacetime", "spacetimedb-cli"}
        and actual_name in {"spacetime-standalone", "spacetimedb-standalone"}
    )


def identity_matches(expected: dict[str, object]) -> bool:
    actual = process_snapshot(int(expected.get("pid", 0)))
    if actual is None:
        return False
    if actual["start_token"] != expected.get("start_token"):
        return False
    if not executable_identity_matches(expected.get("executable", ""), actual["executable"]):
        print(
            f"note: unexpected executable path change: {expected.get('executable')} -> {actual['executable']}",
            file=sys.stderr,
        )
        return False
    return True


def terminate_verified(expected: dict[str, object]) -> None:
    if not identity_matches(expected):
        raise ValueError("refusing to stop process: executable/start identity mismatch")
    pid = int(expected["pid"])
    if os.name != "nt":
        import signal

        os.kill(pid, signal.SIGTERM)
        return
    import ctypes
    from ctypes import wintypes

    terminate = 0x0001
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel32.OpenProcess.restype = wintypes.HANDLE
    handle = kernel32.OpenProcess(terminate, False, pid)
    if not handle:
        raise ValueError("unable to open verified process for termination")
    try:
        if not kernel32.TerminateProcess(handle, 0):
            raise ValueError("unable to terminate verified process")
    finally:
        kernel32.CloseHandle(handle)


def spawner_identity(profile: str, server: str, database: str, host: str, base_port: int) -> dict[str, object]:
    target = cargo_target_dir()
    dispatcher = target / "debug" / ("adventuresim-tactical-server-dispatcher.exe" if os.name == "nt" else "adventuresim-tactical-server-dispatcher")
    tactical = target / "debug" / ("adventuresim-tactical-server.exe" if os.name == "nt" else "adventuresim-tactical-server")
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


def check_spawner_identity(identity_file: Path, expected: dict[str, object]) -> int:
    if not identity_file.exists():
        return 1
    try:
        actual = json.loads(identity_file.read_text())
    except (OSError, json.JSONDecodeError):
        return 2
    process = actual.get("process", {})
    pid = process.get("pid", 0)
    if not isinstance(pid, int) or pid <= 0:
        print("Refusing invalid tactical spawner PID metadata.", file=sys.stderr)
        return 2
    if process_snapshot(pid) is None:
        identity_file.unlink()
        return 1
    if actual.get("config") != expected or not identity_matches(process):
        print(f"Refusing to reuse tactical spawner pid {pid}: process/config identity mismatch.", file=sys.stderr)
        return 2
    print(f"Tactical spawner already running (pid {pid}) with matching identity.")
    return 0


def spawn_recorded(
    command: list[str],
    metadata_file: Path,
    log_file: Path,
    config: dict[str, object],
    *,
    environment: dict[str, str] | None = None,
) -> subprocess.Popen[str]:
    if metadata_file.exists():
        try:
            previous = json.loads(metadata_file.read_text()).get("process", {})
        except (OSError, json.JSONDecodeError) as error:
            raise ValueError("refusing start: unreadable existing process metadata") from error
        if process_snapshot(int(previous.get("pid", 0))) is not None:
            raise ValueError("refusing start: recorded process is still live")
        metadata_file.unlink()
    log = secure_log(log_file)
    try:
        process = subprocess.Popen(
            command,
            cwd=ROOT,
            env=environment,
            stdout=log,
            stderr=subprocess.STDOUT,
            text=True,
        )
    finally:
        log.close()
    snapshot = process_snapshot(process.pid)
    if snapshot is None:
        process.terminate()
        raise RuntimeError("could not record child process identity")
    atomic_write_json(metadata_file, {"config": config, "process": snapshot})
    return process


def stop_recorded(metadata_file: Path, expected_config: dict[str, object] | None = None) -> None:
    if not metadata_file.exists():
        return
    try:
        metadata = json.loads(metadata_file.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError("refusing stop: unreadable process metadata") from error
    if expected_config is not None and metadata.get("config") != expected_config:
        raise ValueError("refusing stop: process configuration mismatch")
    process = metadata.get("process", {})
    if process_snapshot(int(process.get("pid", 0))) is not None:
        terminate_verified(process)
    metadata_file.unlink()


def stop_spacetime(metadata_file: Path, expected_config: dict[str, object]) -> None:
    if not metadata_file.exists():
        return
    metadata = json.loads(metadata_file.read_text())
    if metadata.get("config") != expected_config:
        raise ValueError("refusing SpacetimeDB stop: configuration mismatch")
    listener = metadata.get("listener")
    if isinstance(listener, dict) and process_snapshot(int(listener.get("pid", 0))) is not None:
        terminate_verified(listener)
    launcher = metadata.get("process", {})
    if process_snapshot(int(launcher.get("pid", 0))) is not None:
        terminate_verified(launcher)
    metadata_file.unlink()


def start_spawner(run_dir: Path, config: dict[str, object], gateway_token: str) -> subprocess.Popen[str] | None:
    metadata_file = run_dir / "spawner.identity.json"
    status = check_spawner_identity(metadata_file, config)
    if status == 0:
        return None
    if status == 2:
        raise ValueError("existing tactical spawner metadata cannot be safely reused")
    dispatcher = Path(str(config["binaries"]["dispatcher"]["path"]))
    tactical = str(config["binaries"]["tactical_server"]["path"])
    command = [
        str(dispatcher), "--spacetimedb-url", str(config["server"]),
        "--spacetimedb-module", str(config["database"]), "--tactical-server-bin", tactical,
        "--base-port", str(config["base_port"]), "--host", str(config["host"]),
    ]
    previous = os.environ.get("SPACETIMEDB_TOKEN")
    os.environ["SPACETIMEDB_TOKEN"] = gateway_token
    try:
        process = spawn_recorded(command, metadata_file, run_dir / "spawner.log", config)
    finally:
        if previous is None:
            os.environ.pop("SPACETIMEDB_TOKEN", None)
        else:
            os.environ["SPACETIMEDB_TOKEN"] = previous
    time.sleep(0.5)
    metadata = json.loads(metadata_file.read_text())
    if process.poll() is not None or not identity_matches(metadata["process"]):
        raise RuntimeError("tactical spawner exited during startup")
    return process


def wait_for_spacetime(process: subprocess.Popen[str], metadata_file: Path, log_file: Path, port: int) -> dict[str, object]:
    marker = f"Starting SpacetimeDB listening on 127.0.0.1:{port}"
    for _ in range(200):
        metadata = json.loads(metadata_file.read_text())
        if process.poll() is not None or not identity_matches(metadata["process"]):
            raise RuntimeError("isolated SpacetimeDB child exited during readiness")
        try:
            text = log_file.read_text(errors="replace")
        except OSError:
            text = ""
        data_marker = f"database running in data directory {metadata['config']['data_dir']}"
        if marker in text and data_marker in text and ports_in_use([port]) == [port]:
            listener = listener_process_snapshot(port)
            if listener is None:
                time.sleep(0.05)
                continue
            if "spacetimedb-standalone" not in Path(str(listener["executable"])).stem.lower():
                raise RuntimeError("isolated port listener is not SpacetimeDB standalone")
            if int(listener["start_token"]) < int(metadata["process"]["start_token"]):
                raise RuntimeError("isolated port listener predates the recorded launcher")
            metadata["listener"] = listener
            atomic_write_json(metadata_file, metadata)
            return listener
        time.sleep(0.05)
    raise RuntimeError(f"isolated SpacetimeDB did not become ready; see {log_file}")


def run_profile(
    name: str,
    base_port: int,
    mode: ProfileMode = ProfileMode.STRATEGIC,
    verify_http: bool = False,
    mission_id: str = "mission:test-mission",
    scene_key: str = "hills",
    character_id: int = 0,
    enemy_count: int = 3,
) -> int:
    values = profile_values(name, base_port)
    state_root = runtime_root()
    profile_dir = ensure_secure_directory(Path(str(values["profile_dir"])), state_root)
    run_dir = ensure_secure_directory(profile_dir / "run", state_root)
    data_dir = ensure_secure_directory(profile_dir / "spacetimedb-data", state_root)
    with ProfileLock(profile_dir / "lifecycle.lock") as lifecycle:
        ports = profile_ports(values, mode)
        occupied = ports_in_use(ports)
        if occupied:
            raise ValueError(f"isolated profile ports already occupied: {occupied}")
        server = f"http://127.0.0.1:{values['spacetime_port']}"
        database = str(values["database"])
        stdb_config = {
            "role": "spacetimedb", "profile": name,
            "worktree_fingerprint": values["worktree_fingerprint"], "server": server,
            "database": database, "data_dir": str(data_dir),
        }
        stdb_metadata = run_dir / "spacetime.identity.json"
        stdb_log = run_dir / "spacetime.log"
        stdb = spawn_recorded([
            "spacetime", "start", "--non-interactive", "--listen-addr",
            f"127.0.0.1:{values['spacetime_port']}", "--data-dir", str(data_dir),
        ], stdb_metadata, stdb_log, stdb_config)
        web = None
        web_config = None
        spawner = None
        spawner_config = None
        wrote_tactical_env = False
        try:
            listener = wait_for_spacetime(stdb, stdb_metadata, stdb_log, int(values["spacetime_port"]))
            if not identity_matches(listener):
                raise RuntimeError("SpacetimeDB ownership changed before destructive publish")
            capability = ResetCapability(
                profile=name,
                base_port=base_port,
                server=server,
                database=database,
                lock=lifecycle,
                listener=listener,
            )
            bootstrap_token = secrets.token_hex(32)
            previous_token = os.environ.get("ADVENTURESIM_DEV_BOOTSTRAP_TOKEN")
            os.environ["ADVENTURESIM_DEV_BOOTSTRAP_TOKEN"] = bootstrap_token
            try:
                code = reset_publish(capability)
            finally:
                if previous_token is None:
                    os.environ.pop("ADVENTURESIM_DEV_BOOTSTRAP_TOKEN", None)
                else:
                    os.environ["ADVENTURESIM_DEV_BOOTSTRAP_TOKEN"] = previous_token
            if code:
                return code
            code = seed(
                server,
                database,
                bootstrap_token,
                include_damaged_demo=mode is not ProfileMode.TACTICAL,
            )
            if code:
                return code

            if mode is ProfileMode.TACTICAL:
                tactical_claim = secrets.token_hex(32)
                result = run_checked([
                    "spacetime", "call", "--server", server, database,
                    "seed_standalone_tactical_mission", bootstrap_token,
                    str(character_id), mission_id, scene_key, str(enemy_count),
                    tactical_claim,
                ])
                write_console(result.stdout)
                if result.returncode:
                    print("standalone tactical mission seed failed; refusing to hide the reducer error.", file=sys.stderr)
                    return result.returncode
                write_tactical_env_file(
                    url=server, database=database, port=int(values["tactical_port"]),
                    mission_id=mission_id, scene_key=scene_key,
                    character_id=character_id, enemy_count=enemy_count,
                    tactical_claim=tactical_claim,
                )
                wrote_tactical_env = True
                print("")
                print(f"Isolated tactical database ready: {server} (database {database})")
                print("Strategic layer and WASM client are not built or running.")
                print("Run `just tactical` and `just client` in other terminals (no arguments needed).")
                print("Press Ctrl+C to stop the isolated database.")
                return stdb.wait()

            gateway_token = spacetime_auth_token()
            if mode is ProfileMode.STRATEGIC:
                spawner_config = spawner_identity(
                    name,
                    server,
                    database,
                    "127.0.0.1",
                    int(values["tactical_port"]),
                )
                spawner = start_spawner(run_dir, spawner_config, gateway_token)
            built = subprocess.run(["cargo", "build", "-p", "strategic-web"], cwd=ROOT)
            if built.returncode:
                return built.returncode
            environment = os.environ.copy()
            environment.update({
                "SPACETIMEDB_HOST": server, "SPACETIMEDB_DATABASE": database,
                "SPACETIMEDB_TOKEN": gateway_token,
                "STRATEGIC_SESSION_SECRET": secrets.token_urlsafe(32),
                "BIND_ADDRESS": f"127.0.0.1:{values['web_port']}",
                "STATIC_DIR": str(ROOT / "crates" / "strategic-web" / "static"),
                "TACTICAL_STATIC_DIR": str(ROOT / "crates" / "adventuresim-stdb-module" / "static"),
            })
            print(
                f"Starting isolated {mode.value} profile {name!r} "
                f"at http://127.0.0.1:{values['web_port']}"
            )
            executable = cargo_target_dir() / "debug" / ("strategic-web.exe" if os.name == "nt" else "strategic-web")
            web_config = {"role": "strategic-web", "profile": name, "executable": str(executable), "server": server}
            log = secure_log(run_dir / "web.log")
            try:
                web = subprocess.Popen([str(executable)], cwd=ROOT, env=environment, stdout=log, stderr=subprocess.STDOUT, text=True)
            finally:
                log.close()
            snapshot = process_snapshot(web.pid)
            if snapshot is None:
                raise RuntimeError("could not record strategic-web process identity")
            atomic_write_json(run_dir / "web.identity.json", {"config": web_config, "process": snapshot})
            if verify_http:
                for _ in range(200):
                    if web.poll() is not None or not identity_matches(snapshot):
                        raise RuntimeError("strategic-web exited before HTTP verification")
                    try:
                        with urllib.request.urlopen(f"http://127.0.0.1:{values['web_port']}/", timeout=0.5) as response:
                            if response.status == 200:
                                print(f"Verified isolated HTTP 200 ({len(response.read())} bytes).")
                                return 0
                    except OSError:
                        time.sleep(0.05)
                raise RuntimeError("strategic-web did not return HTTP 200")
            return web.wait()
        finally:
            if wrote_tactical_env:
                remove_tactical_env_file()
            if web_config is not None and (run_dir / "web.identity.json").exists():
                stop_recorded(run_dir / "web.identity.json", web_config)
            try:
                if spawner_config is not None:
                    stop_recorded(run_dir / "spawner.identity.json", spawner_config)
            finally:
                stop_spacetime(stdb_metadata, stdb_config)


def tactical_executable(package: str) -> Path:
    suffix = ".exe" if os.name == "nt" else ""
    target = cargo_target_dir()
    if not target.is_absolute():
        target = ROOT / target
    return (target / "debug" / f"{package}{suffix}").resolve()


def build_tactical_play(launch_client: bool) -> int:
    commands = [[
        "cargo", "build", "--package", "adventuresim-tactical-server",
        "--bin", "adventuresim-tactical-server", "--features", "debug",
    ]]
    if launch_client:
        commands.append([
            "cargo", "build", "--package", "adventuresim-tactical-client",
            "--bin", "adventuresim-tactical-client", "--features", "debug",
        ])
    for command in commands:
        result = subprocess.run(command, cwd=ROOT)
        if result.returncode:
            return result.returncode
    return 0


def sql_mission_row_exists(
    server: str,
    database: str,
    table: str,
    mission_id: str,
) -> bool:
    if table not in {
        "tactical_server_authority",
        "tactical_server_claim",
        "tactical_server_request_authority",
    }:
        raise ValueError("unsupported tactical readiness table")
    literal = mission_id.replace("'", "''")
    result = run_checked([
        "spacetime", "sql", "--server", server, database,
        f"SELECT mission_id FROM {table} WHERE mission_id = '{literal}'",
    ])
    if result.returncode:
        raise RuntimeError(f"database readiness query failed for {table}")
    return any(
        line.strip().strip('"') == mission_id for line in result.stdout.splitlines()
    )


def log_tail(path: Path, line_count: int = 30) -> str:
    try:
        return "\n".join(path.read_text(encoding="utf-8", errors="replace").splitlines()[-line_count:])
    except OSError:
        return "(log unavailable)"


def wait_for_tactical_server(
    process: subprocess.Popen[str],
    metadata_file: Path,
    log_file: Path,
    server: str,
    database: str,
    mission_id: str,
    port: int,
) -> dict[str, object]:
    stage = "server process startup"
    for _ in range(300):
        metadata = json.loads(metadata_file.read_text(encoding="utf-8"))
        recorded = metadata["process"]
        if process.poll() is not None or not identity_matches(recorded):
            raise RuntimeError(
                f"tactical readiness failed during {stage}; server exited.\n"
                f"Log: {log_file}\n{log_tail(log_file)}"
            )
        listener = listener_process_snapshot(port)
        if listener is None:
            time.sleep(0.1)
            continue
        stage = "listener ownership verification"
        if listener != recorded:
            raise RuntimeError(
                f"tactical port {port} is owned by an unrecorded process; refusing client launch"
            )
        stage = "claim consumption and server authority registration"
        if sql_mission_row_exists(server, database, "tactical_server_authority", mission_id):
            if sql_mission_row_exists(server, database, "tactical_server_claim", mission_id):
                time.sleep(0.1)
                continue
            if sql_mission_row_exists(
                server, database, "tactical_server_request_authority", mission_id
            ):
                time.sleep(0.1)
                continue
            return listener
        time.sleep(0.1)
    raise RuntimeError(
        f"tactical readiness timed out during {stage}.\n"
        f"Log: {log_file}\n{log_tail(log_file)}"
    )


def tactical_session_config(
    values: dict[str, object],
    mode: TacticalPlayMode,
    mission_id: str,
    character_id: int,
    enemy_count: int,
    session_id: str,
) -> dict[str, object]:
    return {
        "repository": str(ROOT.resolve()),
        "profile": values["profile"],
        "worktree_fingerprint": values["worktree_fingerprint"],
        "database": values["database"],
        "spacetime_port": values["spacetime_port"],
        "tactical_port": values["tactical_port"],
        "mission_id": mission_id,
        "character_id": character_id,
        "enemy_count": enemy_count,
        "play_mode": mode.value,
        "combat_enabled": mode is TacticalPlayMode.COMBAT,
        "native_client": mode is not TacticalPlayMode.NETWORKING,
        "browser_client": False,
        "session_id": session_id,
    }


def tactical_combat_scale(mode: TacticalPlayMode) -> int:
    return 10_000 if mode is TacticalPlayMode.COMBAT else 0


def launch_recorded_tactical_client(
    run_dir: Path,
    config: dict[str, object],
) -> subprocess.Popen[str]:
    executable = tactical_executable("adventuresim-tactical-client")
    if not executable.is_file():
        raise RuntimeError("native tactical client is not built; run `just tactical-play animation`")
    client_config = {
        "role": "native-client",
        "repository": str(ROOT.resolve()),
        "worktree_fingerprint": config["worktree_fingerprint"],
        "session_id": config["session_id"],
        "executable": str(executable),
        "character_id": config["character_id"],
        "server_addr": f"127.0.0.1:{config['tactical_port']}",
    }
    process = spawn_recorded(
        [
            str(executable), "--id", str(config["character_id"]),
            "--server-addr", str(client_config["server_addr"]),
        ],
        run_dir / "client.identity.json",
        run_dir / "client.log",
        client_config,
    )
    time.sleep(0.5)
    if process.poll() is not None:
        raise RuntimeError(
            f"native client exited during launch.\nLog: {run_dir / 'client.log'}\n"
            f"{log_tail(run_dir / 'client.log')}"
        )
    return process


def tactical_play(mode: TacticalPlayMode, base_port: int) -> int:
    launch_client = mode is not TacticalPlayMode.NETWORKING
    code = build_tactical_play(launch_client)
    if code:
        return code

    profile = f"tactical-play-{mode.value}"
    values = profile_values(profile, base_port)
    state_root = runtime_root()
    profile_dir = ensure_secure_directory(Path(str(values["profile_dir"])), state_root)
    run_dir = ensure_secure_directory(profile_dir / "run", state_root)
    data_dir = ensure_secure_directory(profile_dir / "spacetimedb-data", state_root)
    session_id = secrets.token_hex(16)
    mission_id = f"mission:{mode.value}-{session_id[:12]}"
    character_id = 0
    enemy_count = 1
    config = tactical_session_config(
        values, mode, mission_id, character_id, enemy_count, session_id
    )
    session_file = run_dir / "tactical-session.json"

    with ProfileLock(profile_dir / "lifecycle.lock") as lifecycle:
        occupied = ports_in_use([
            int(values["spacetime_port"]), int(values["tactical_port"]),
        ])
        if occupied:
            raise ValueError(f"tactical-play profile ports already occupied: {occupied}")
        atomic_write_json(session_file, config)
        server_url = f"http://127.0.0.1:{values['spacetime_port']}"
        database = str(values["database"])
        stdb_config = {
            "role": "spacetimedb", "profile": profile,
            "worktree_fingerprint": values["worktree_fingerprint"],
            "server": server_url, "database": database, "data_dir": str(data_dir),
            "session_id": session_id,
        }
        stdb_metadata = run_dir / "spacetime.identity.json"
        stdb_log = run_dir / "spacetime.log"
        stdb = spawn_recorded([
            "spacetime", "start", "--non-interactive", "--listen-addr",
            f"127.0.0.1:{values['spacetime_port']}", "--data-dir", str(data_dir),
        ], stdb_metadata, stdb_log, stdb_config)
        server_process = None
        wrote_env = False
        try:
            listener = wait_for_spacetime(
                stdb, stdb_metadata, stdb_log, int(values["spacetime_port"])
            )
            capability = ResetCapability(
                profile, base_port, server_url, database, lifecycle, listener
            )
            bootstrap_token = secrets.token_hex(32)
            previous_token = os.environ.get("ADVENTURESIM_DEV_BOOTSTRAP_TOKEN")
            os.environ["ADVENTURESIM_DEV_BOOTSTRAP_TOKEN"] = bootstrap_token
            try:
                code = reset_publish(capability)
            finally:
                if previous_token is None:
                    os.environ.pop("ADVENTURESIM_DEV_BOOTSTRAP_TOKEN", None)
                else:
                    os.environ["ADVENTURESIM_DEV_BOOTSTRAP_TOKEN"] = previous_token
            if code:
                return code
            if seed(server_url, database, bootstrap_token):
                return 1

            tactical_claim = secrets.token_hex(32)
            result = run_checked([
                "spacetime", "call", "--server", server_url, database,
                "seed_standalone_tactical_mission", bootstrap_token,
                str(character_id), mission_id, "hills", str(enemy_count), tactical_claim,
            ])
            if result.returncode:
                write_console(result.stdout)
                raise RuntimeError("standalone tactical mission seed failed")
            write_tactical_env_file(
                url=server_url,
                database=database,
                port=int(values["tactical_port"]),
                mission_id=mission_id,
                scene_key="hills",
                character_id=character_id,
                enemy_count=enemy_count,
                tactical_claim=None,
                profile=profile,
                worktree_fingerprint_value=str(values["worktree_fingerprint"]),
                run_dir=run_dir,
                session_id=session_id,
                play_mode=mode.value,
            )
            wrote_env = True

            server_executable = tactical_executable("adventuresim-tactical-server")
            server_config = {
                "role": "tactical-server",
                "repository": str(ROOT.resolve()),
                "worktree_fingerprint": values["worktree_fingerprint"],
                "session_id": session_id,
                "executable": str(server_executable),
                "mission_id": mission_id,
                "port": values["tactical_port"],
            }
            environment = os.environ.copy()
            environment["ADVENTURESIM_TACTICAL_CLAIM"] = tactical_claim
            combat_scale = tactical_combat_scale(mode)
            server_log = run_dir / "server.log"
            server_metadata = run_dir / "server.identity.json"
            server_process = spawn_recorded([
                str(server_executable), "--addr", f"0.0.0.0:{values['tactical_port']}",
                "--mission-id", mission_id, "--scene-key", "hills",
                "--spacetimedb-url", server_url, "--spacetimedb-module", database,
                "--expected-party-members", "1", "--required-enemy-kills", str(enemy_count),
                "--enemy-combat-scale-bps", str(combat_scale), "--no-timeout",
            ], server_metadata, server_log, server_config, environment=environment)
            wait_for_tactical_server(
                server_process, server_metadata, server_log, server_url, database,
                mission_id, int(values["tactical_port"]),
            )
            if launch_client:
                launch_recorded_tactical_client(run_dir, config)

            print("")
            print(f"Database: ready at {server_url} ({database})")
            print(f"Mission: {mission_id}")
            print("Claim: consumed successfully")
            print(f"Server: listening at ws://127.0.0.1:{values['tactical_port']}")
            if launch_client:
                print(f"Client: launched, character {character_id} (native)")
            else:
                print("Client: not launched (networking profile)")
            print(f"Combat: {'enabled' if combat_scale else 'disabled'}")
            print("Browser client: unavailable in tactical-only mode")
            print(f"Logs: {run_dir}")
            print("Press Ctrl+C to stop this profile's recorded processes.")
            while server_process.poll() is None:
                time.sleep(0.25)
            raise RuntimeError(
                f"recorded tactical server exited; see {server_log}\n{log_tail(server_log)}"
            )
        except KeyboardInterrupt:
            print("\nStopping supervised tactical profile...")
            return 0
        finally:
            try:
                stop_recorded(run_dir / "client.identity.json")
                stop_recorded(run_dir / "server.identity.json", None)
            finally:
                if wrote_env:
                    remove_tactical_env_file(session_id)
                stop_spacetime(stdb_metadata, stdb_config)


def supervised_tactical_state() -> tuple[dict[str, str], dict[str, object], Path]:
    environment = read_tactical_env_file()
    if not environment:
        raise ValueError(".env.tactical is absent; run `just tactical-play animation`")
    required = {
        "TACTICAL_PROFILE", "TACTICAL_WORKTREE_FINGERPRINT", "TACTICAL_RUN_DIR",
        "TACTICAL_SESSION_ID", "TACTICAL_SPACETIMEDB_URL",
        "TACTICAL_SPACETIMEDB_MODULE", "TACTICAL_PORT",
    }
    missing = sorted(required - environment.keys())
    if missing:
        raise ValueError(
            "legacy or incomplete .env.tactical is not a supervised session; "
            "run `just tactical-play animation`"
        )
    if environment["TACTICAL_WORKTREE_FINGERPRINT"] != worktree_fingerprint():
        raise ValueError(
            ".env.tactical belongs to another worktree; run `just tactical-play animation` here"
        )
    run_dir = Path(environment["TACTICAL_RUN_DIR"]).resolve()
    expected_root = runtime_root().resolve() / worktree_fingerprint()
    if expected_root != run_dir and expected_root not in run_dir.parents:
        raise ValueError(".env.tactical run directory escapes this worktree's runtime profile")
    session_file = run_dir / "tactical-session.json"
    try:
        config = json.loads(session_file.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError("supervised tactical session metadata is unavailable or corrupt") from error
    if (
        config.get("session_id") != environment["TACTICAL_SESSION_ID"]
        or config.get("repository") != str(ROOT.resolve())
        or config.get("worktree_fingerprint") != worktree_fingerprint()
        or config.get("profile") != environment["TACTICAL_PROFILE"]
        or config.get("database") != environment["TACTICAL_SPACETIMEDB_MODULE"]
        or str(config.get("tactical_port")) != environment["TACTICAL_PORT"]
        or config.get("mission_id") != environment.get("TACTICAL_MISSION_ID")
    ):
        raise ValueError("stale .env.tactical does not match the recorded session identity")
    validate_loopback_server(
        environment["TACTICAL_SPACETIMEDB_URL"], int(config["spacetime_port"])
    )
    return environment, config, run_dir


def tactical_status() -> int:
    try:
        environment, config, run_dir = supervised_tactical_state()
    except ValueError as error:
        print(f"Tactical session: stale or unavailable ({error})")
        print("Recovery: just tactical-play animation")
        return 1
    server_url = environment["TACTICAL_SPACETIMEDB_URL"]
    database = environment["TACTICAL_SPACETIMEDB_MODULE"]
    mission_id = str(config["mission_id"])
    db_metadata = run_dir / "spacetime.identity.json"
    server_metadata = run_dir / "server.identity.json"
    database_ready = False
    server_ready = False
    client_ready = False
    if db_metadata.is_file():
        try:
            metadata = json.loads(db_metadata.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            metadata = {}
        listener = metadata.get("listener", {})
        database_ready = isinstance(listener, dict) and identity_matches(listener)
    if server_metadata.is_file():
        try:
            metadata = json.loads(server_metadata.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            metadata = {}
        recorded = metadata.get("process", {})
        listener = listener_process_snapshot(int(config["tactical_port"]))
        server_ready = isinstance(recorded, dict) and identity_matches(recorded) and listener == recorded
    client_metadata = run_dir / "client.identity.json"
    if client_metadata.is_file():
        try:
            metadata = json.loads(client_metadata.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            metadata = {}
        recorded = metadata.get("process", {})
        client_ready = isinstance(recorded, dict) and identity_matches(recorded)
    authority = claim = request = False
    if database_ready:
        authority = sql_mission_row_exists(server_url, database, "tactical_server_authority", mission_id)
        claim = sql_mission_row_exists(server_url, database, "tactical_server_claim", mission_id)
        request = sql_mission_row_exists(
            server_url, database, "tactical_server_request_authority", mission_id
        )
    print(f"Profile: {config['profile']} ({config['worktree_fingerprint']})")
    print(f"Environment: {TACTICAL_ENV_FILE}")
    print(f"Database: {'ready' if database_ready else 'unreachable'} at {server_url}")
    print(f"Mission: {mission_id}")
    claim_state = (
        "unknown" if not database_ready
        else "available" if claim
        else "consumed" if authority or not request
        else "unknown"
    )
    print(f"Claim: {claim_state}")
    print(f"Server authority: {'registered' if authority else 'missing'}")
    print(f"Tactical listener: {'owned by recorded server' if server_ready else 'missing or unowned'}")
    if config["native_client"]:
        print(f"Client: {'running' if client_ready else 'stopped; run just tactical-client'}")
    else:
        print("Client: not launched by networking profile")
    print("Browser client: unavailable in tactical-only mode")
    print(f"Logs: {run_dir}")
    if not database_ready or not server_ready or not authority:
        if database_ready and not server_ready and claim_state == "consumed":
            print("This mission's server claim was already consumed and no server is listening.")
        print("Recovery: just tactical-play animation")
        return 1
    return 0


def tactical_client_relaunch() -> int:
    environment, config, run_dir = supervised_tactical_state()
    if tactical_status():
        raise RuntimeError("refusing native client launch because the supervised server is not ready")
    metadata_file = run_dir / "client.identity.json"
    if metadata_file.is_file():
        metadata = json.loads(metadata_file.read_text(encoding="utf-8"))
        recorded = metadata.get("process", {})
        if isinstance(recorded, dict) and identity_matches(recorded):
            print(f"Native client is already running (pid {recorded['pid']}).")
            return 0
    launch_recorded_tactical_client(run_dir, config)
    print(
        f"Native client relaunched for character {config['character_id']} at "
        f"127.0.0.1:{environment['TACTICAL_PORT']}"
    )
    return 0


def canonical_spawner(action: str) -> int:
    state_root = runtime_root()
    profile_dir = ensure_secure_directory(
        state_root / worktree_fingerprint() / "canonical", state_root
    )
    run_dir = ensure_secure_directory(profile_dir / "run", state_root)
    with ProfileLock(profile_dir / "lifecycle.lock"):
        identity_file = run_dir / "spawner.identity.json"
        if action == "stop":
            stop_recorded(identity_file)
            return 0
        config = spawner_identity(
            "canonical", "http://localhost:23100", "adventuresim-stdb-module", "127.0.0.1", 6001
        )
        if action == "start":
            start_spawner(run_dir, config, spacetime_auth_token())
        else:
            status = check_spawner_identity(identity_file, config)
            if status == 1:
                print("Tactical spawner: not running")
            return 0 if status in {0, 1} else status
    return 0


def create_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    profile_parser = sub.add_parser("profile")
    profile_parser.add_argument("--name", required=True)
    profile_parser.add_argument("--base-port", type=int, required=True)
    publish_parser = sub.add_parser("publish")
    publish_parser.add_argument("--server", required=True)
    publish_parser.add_argument("--database", required=True)
    seed_parser = sub.add_parser("seed")
    seed_parser.add_argument("--server", required=True)
    seed_parser.add_argument("--database", required=True)
    seed_parser.add_argument("--token", required=True)
    sub.add_parser("verify-bindings")
    runner = sub.add_parser("run-profile")
    runner.add_argument("--mode", choices=[m.value for m in ProfileMode], default=ProfileMode.STRATEGIC.value)
    runner.add_argument("--mission-id", default="mission:test-mission")
    runner.add_argument("--scene-key", default="hills")
    runner.add_argument("--character-id", type=int, default=0)
    runner.add_argument("--enemy-count", type=int, default=3)
    runner.add_argument("name")
    runner.add_argument("base_port", type=int)
    verifier = sub.add_parser("verify-profile")
    verifier.add_argument(
        "--mode",
        choices=(ProfileMode.STRATEGIC.value, ProfileMode.BARE_STRATEGIC.value),
        default=ProfileMode.STRATEGIC.value,
    )
    verifier.add_argument("name")
    verifier.add_argument("base_port", type=int)
    canonical = sub.add_parser("canonical-spawner")
    canonical.add_argument("action", choices=("start", "stop", "status"))
    tactical_play_parser = sub.add_parser("tactical-play")
    tactical_play_parser.add_argument(
        "mode", choices=[mode.value for mode in TacticalPlayMode]
    )
    tactical_play_parser.add_argument("base_port", type=int, nargs="?", default=24920)
    sub.add_parser("tactical-status")
    sub.add_parser("tactical-client")
    return parser


def main() -> int:
    args = create_parser().parse_args()
    try:
        if args.command == "profile":
            print(json.dumps(profile_values(args.name, args.base_port), sort_keys=True))
            return 0
        if args.command == "publish":
            return publish(args.server, args.database)
        if args.command == "seed":
            return seed(args.server, args.database, args.token)
        if args.command == "verify-bindings":
            return verify_bindings()
        if args.command == "run-profile":
            return run_profile(
                args.name, args.base_port, mode=ProfileMode(args.mode),
                mission_id=args.mission_id, scene_key=args.scene_key,
                character_id=args.character_id, enemy_count=args.enemy_count,
            )
        if args.command == "verify-profile":
            return run_profile(
                args.name,
                args.base_port,
                mode=ProfileMode(args.mode),
                verify_http=True,
            )
        if args.command == "canonical-spawner":
            return canonical_spawner(args.action)
        if args.command == "tactical-play":
            return tactical_play(TacticalPlayMode(args.mode), args.base_port)
        if args.command == "tactical-status":
            return tactical_status()
        if args.command == "tactical-client":
            return tactical_client_relaunch()
    except (ValueError, RuntimeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
