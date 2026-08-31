#!/usr/bin/env python3
"""Safety checks used by the local SpacetimeDB development recipes."""

from __future__ import annotations

import argparse
import base64
from contextlib import AbstractContextManager
from dataclasses import dataclass
from enum import Enum
import hashlib
import http.client
import json
import math
import os
import secrets
from pathlib import Path, PureWindowsPath
import re
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time
import urllib.request
from urllib.parse import urlparse
from urllib.parse import urlparse
import zlib


PROFILE_RE = re.compile(r"^[a-z][a-z0-9-]{0,31}$")
JWT_RE = re.compile(r"[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+")
ROOT = Path(__file__).resolve().parents[1]
MODULE_DIR = ROOT / "crates" / "adventuresim-stdb-module"
CLIENT_DIR = ROOT / "crates" / "adventuresim-stdb-client" / "src"
TACTICAL_ENV_FILE = ROOT / ".env.tactical"
ENEMY_FIXTURE_MAX_BYTES = 16 * 1024
ANIMATION_ENEMY_FIXTURE = "animation-demo"
PASSIVE_ENEMY_FIXTURE = "passive-bandit"
STANDARD_ENEMY_FIXTURE = "standard-bandit"
DEFAULT_SCENE_INPUT = "dense-woodland"


@dataclass
class StartupBenchmark:
    """Bounded phase timings for the supervised tactical launcher."""

    started_at: float
    events: list[dict[str, object]]
    output_path: Path | None = None

    @classmethod
    def start(cls) -> "StartupBenchmark":
        return cls(time.monotonic(), [])

    def attach(self, output_path: Path) -> None:
        self.output_path = output_path
        if output_path.is_symlink():
            raise ValueError(f"refusing symlink startup benchmark target: {output_path}")
        with secure_log(output_path) as stream:
            for event in self.events:
                stream.write(json.dumps(event, sort_keys=True) + "\n")

    def record(
        self,
        phase: str,
        phase_started_at: float,
        **details: object,
    ) -> None:
        event = {
            "phase": phase,
            "duration_seconds": round(time.monotonic() - phase_started_at, 3),
            "elapsed_seconds": round(time.monotonic() - self.started_at, 3),
            **details,
        }
        self.events.append(event)
        detail_text = " ".join(
            f"{key}={value!r}" for key, value in details.items()
        )
        print(
            "[startup] "
            f"phase={phase!r} duration={event['duration_seconds']:.3f}s "
            f"elapsed={event['elapsed_seconds']:.3f}s"
            + (f" {detail_text}" if detail_text else "")
        )
        if self.output_path is not None:
            if self.output_path.is_symlink():
                raise ValueError(
                    f"refusing symlink startup benchmark target: {self.output_path}"
                )
            with self.output_path.open("a", encoding="utf-8") as stream:
                stream.write(json.dumps(event, sort_keys=True) + "\n")


class ProfileMode(str, Enum):
    """The three shapes an isolated profile can run in."""

    STRATEGIC = "strategic"  # strategic-web + tactical dispatcher (full stack)
    BARE_STRATEGIC = "bare-strategic"  # strategic-web only, no dispatcher
    TACTICAL = "tactical"  # isolated DB + seeded standalone mission only


class TacticalPlayMode(str, Enum):
    """Safe fixtures exposed by the supervised native tactical launcher."""

    ANIMATION = "animation"
    DIAGNOSTIC = "diagnostic"
    COMBAT = "combat"
    NETWORKING = "networking"


def default_enemy_fixture(mode: TacticalPlayMode) -> str:
    if mode is TacticalPlayMode.ANIMATION:
        return ANIMATION_ENEMY_FIXTURE
    if mode is TacticalPlayMode.COMBAT:
        return STANDARD_ENEMY_FIXTURE
    return PASSIVE_ENEMY_FIXTURE


def resolve_fixture_path(
    selector: str, directory: str, extension: str,
) -> Path:
    fixture_path = Path(selector)
    if len(fixture_path.parts) == 1 and not fixture_path.suffix:
        fixture_path = Path(directory) / f"{selector}.{extension}"
    if not fixture_path.is_absolute():
        fixture_path = ROOT / fixture_path
    return fixture_path


def read_enemy_fixture(path: str) -> str:
    fixture_path = resolve_fixture_path(path, "assets/tactical-enemies", "yaml")
    try:
        length = fixture_path.stat().st_size
    except OSError as error:
        raise ValueError(f"could not inspect enemy fixture {path!r}: {error}") from error
    if length == 0 or length > ENEMY_FIXTURE_MAX_BYTES:
        raise ValueError("enemy fixture must contain between 1 byte and 16 KiB")
    try:
        return fixture_path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        raise ValueError(f"could not read enemy fixture {path!r}: {error}") from error


class ObsWebSocket:
    """Small obs-websocket v5 client used only by the supervised capture path."""

    def __init__(self, port: int, password: str, timeout: float = 15.0):
        self.socket = socket.create_connection(("127.0.0.1", port), timeout=timeout)
        self.socket.settimeout(timeout)
        key = base64.b64encode(secrets.token_bytes(16)).decode("ascii")
        request = (
            f"GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n"
            "Upgrade: websocket\r\nConnection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )
        self.socket.sendall(request.encode("ascii"))
        response = b""
        while b"\r\n\r\n" not in response:
            response += self.socket.recv(4096)
        headers, self._buffer = response.split(b"\r\n\r\n", 1)
        if not headers.startswith(b"HTTP/1.1 101"):
            self.close()
            raise RuntimeError("OBS rejected its local WebSocket connection")
        hello = self.receive()
        if hello.get("op") != 0:
            raise RuntimeError("OBS did not send the expected WebSocket greeting")
        identify: dict[str, object] = {"rpcVersion": 1}
        authentication = hello.get("d", {}).get("authentication")
        if authentication:
            secret = base64.b64encode(hashlib.sha256(
                (password + authentication["salt"]).encode("utf-8")
            ).digest()).decode("ascii")
            identify["authentication"] = base64.b64encode(hashlib.sha256(
                (secret + authentication["challenge"]).encode("utf-8")
            ).digest()).decode("ascii")
        self.send({"op": 1, "d": identify})
        if self.receive().get("op") != 2:
            raise RuntimeError("OBS WebSocket authentication failed")

    def close(self) -> None:
        try:
            self.socket.close()
        except OSError:
            pass

    def _send_frame(self, opcode: int, payload: bytes) -> None:
        mask = secrets.token_bytes(4)
        length = len(payload)
        header = bytearray([0x80 | opcode])
        if length < 126:
            header.append(0x80 | length)
        elif length <= 0xFFFF:
            header.append(0x80 | 126)
            header.extend(struct.pack("!H", length))
        else:
            header.append(0x80 | 127)
            header.extend(struct.pack("!Q", length))
        masked = bytes(value ^ mask[index % 4] for index, value in enumerate(payload))
        self.socket.sendall(bytes(header) + mask + masked)

    def send(self, value: dict[str, object]) -> None:
        self._send_frame(1, json.dumps(value, separators=(",", ":")).encode("utf-8"))

    def _receive_exact(self, length: int) -> bytes:
        value = self._buffer[:length]
        self._buffer = self._buffer[length:]
        while len(value) < length:
            chunk = self.socket.recv(length - len(value))
            if not chunk:
                raise RuntimeError("OBS closed its WebSocket connection")
            value += chunk
        return value

    def receive(self) -> dict[str, object]:
        fragments = bytearray()
        while True:
            first, second = self._receive_exact(2)
            opcode = first & 0x0F
            length = second & 0x7F
            if length == 126:
                length = struct.unpack("!H", self._receive_exact(2))[0]
            elif length == 127:
                length = struct.unpack("!Q", self._receive_exact(8))[0]
            mask = self._receive_exact(4) if second & 0x80 else None
            payload = self._receive_exact(length)
            if mask:
                payload = bytes(
                    value ^ mask[index % 4] for index, value in enumerate(payload)
                )
            if opcode == 8:
                raise RuntimeError("OBS closed its WebSocket connection")
            if opcode == 9:
                self._send_frame(10, payload)
                continue
            if opcode in {0, 1}:
                fragments.extend(payload)
                if first & 0x80:
                    return json.loads(fragments.decode("utf-8"))

    def request(self, request_type: str, data: dict[str, object] | None = None) -> dict[str, object]:
        request_id = secrets.token_hex(8)
        self.send({
            "op": 6,
            "d": {
                "requestType": request_type,
                "requestId": request_id,
                "requestData": data or {},
            },
        })
        while True:
            message = self.receive()
            body = message.get("d", {})
            if message.get("op") != 7 or body.get("requestId") != request_id:
                continue
            status = body.get("requestStatus", {})
            if not status.get("result"):
                raise RuntimeError(
                    f"OBS {request_type} failed: {status.get('comment', 'unknown error')}"
                )
            return body.get("responseData", {})


@dataclass
class ObsCapture:
    process: subprocess.Popen[str]
    websocket: ObsWebSocket
    metadata_file: Path


def tactical_window_capture_geometry(process_id: int) -> dict[str, object]:
    if os.name != "nt":
        raise RuntimeError("cropped display capture is currently supported only on Windows")
    import ctypes
    from ctypes import wintypes

    class Rect(ctypes.Structure):
        _fields_ = [
            ("left", wintypes.LONG), ("top", wintypes.LONG),
            ("right", wintypes.LONG), ("bottom", wintypes.LONG),
        ]

    class MonitorInfo(ctypes.Structure):
        _fields_ = [
            ("cbSize", wintypes.DWORD),
            ("rcMonitor", Rect),
            ("rcWork", Rect),
            ("dwFlags", wintypes.DWORD),
            ("szDevice", wintypes.WCHAR * 32),
        ]

    class DisplayDevice(ctypes.Structure):
        _fields_ = [
            ("cb", wintypes.DWORD),
            ("DeviceName", wintypes.WCHAR * 32),
            ("DeviceString", wintypes.WCHAR * 128),
            ("StateFlags", wintypes.DWORD),
            ("DeviceID", wintypes.WCHAR * 128),
            ("DeviceKey", wintypes.WCHAR * 128),
        ]

    user32 = ctypes.windll.user32
    matching: list[int] = []
    callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

    @callback_type
    def find_window(window: int, _parameter: int) -> bool:
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(window, ctypes.byref(owner))
        if owner.value != process_id or not user32.IsWindowVisible(window):
            return True
        title_length = user32.GetWindowTextLengthW(window)
        title = ctypes.create_unicode_buffer(title_length + 1)
        user32.GetWindowTextW(window, title, len(title))
        if title.value == "Fabelgeist - Tactical":
            matching.append(window)
        return True

    user32.EnumWindows(find_window, 0)
    if len(matching) != 1:
        raise RuntimeError(
            f"expected one visible tactical window for pid {process_id}, found {len(matching)}"
        )
    window = matching[0]
    client = Rect()
    if not user32.GetClientRect(window, ctypes.byref(client)):
        raise RuntimeError("could not read the tactical client rectangle")
    origin = wintypes.POINT(client.left, client.top)
    if not user32.ClientToScreen(window, ctypes.byref(origin)):
        raise RuntimeError("could not map the tactical client rectangle to the monitor")
    width = client.right - client.left
    height = client.bottom - client.top
    monitor = user32.MonitorFromWindow(window, 2)
    info = MonitorInfo(cbSize=ctypes.sizeof(MonitorInfo))
    if not monitor or not user32.GetMonitorInfoW(monitor, ctypes.byref(info)):
        raise RuntimeError("could not identify the tactical client's monitor")
    monitor_width = info.rcMonitor.right - info.rcMonitor.left
    monitor_height = info.rcMonitor.bottom - info.rcMonitor.top
    monitor_device = DisplayDevice(cb=ctypes.sizeof(DisplayDevice))
    monitor_name = ""
    if user32.EnumDisplayDevicesW(
        info.szDevice, 0, ctypes.byref(monitor_device), 1
    ):
        monitor_name = monitor_device.DeviceString
    left = origin.x - info.rcMonitor.left
    top = origin.y - info.rcMonitor.top
    if (
        width <= 0 or height <= 0 or left < 0 or top < 0
        or left + width > monitor_width or top + height > monitor_height
    ):
        raise RuntimeError("tactical client rectangle lies outside its monitor")
    return {
        "window_handle": window,
        "left": left,
        "top": top,
        "right": monitor_width - left - width,
        "bottom": monitor_height - top - height,
        "width": width,
        "height": height,
        "monitor_width": monitor_width,
        "monitor_height": monitor_height,
        "monitor_left": info.rcMonitor.left,
        "monitor_top": info.rcMonitor.top,
        "monitor_name": monitor_name,
    }


def select_obs_monitor_id(
    property_items: list[dict[str, object]],
    geometry: dict[str, object],
    configured_id: str | None = None,
) -> str:
    available = [item for item in property_items if item.get("itemEnabled", True)]
    if configured_id:
        exact = [
            item for item in available
            if str(item.get("itemValue", "")) == configured_id
        ]
        if len(exact) == 1:
            return str(exact[0]["itemValue"])
        raise RuntimeError("OBS_MONITOR_ID does not identify an available display")

    monitor_name = str(geometry.get("monitor_name", "")).strip().casefold()
    resolution = f"{geometry['monitor_width']}x{geometry['monitor_height']}".casefold()
    origin = f"{geometry['monitor_left']},{geometry['monitor_top']}".casefold()
    matches = []
    for item in available:
        name = str(item.get("itemName", "")).casefold()
        if monitor_name and monitor_name in name:
            matches.append(item)
        elif not monitor_name and resolution in name and origin in name:
            matches.append(item)
    if len(matches) == 1:
        return str(matches[0]["itemValue"])
    descriptions = ", ".join(str(item.get("itemName", "?")) for item in available)
    raise RuntimeError(
        "could not uniquely match the tactical window's monitor in OBS; "
        f"set OBS_MONITOR_ID. Available displays: {descriptions or 'none'}"
    )


def obs_screenshot_has_visible_pixels(image_data: str) -> bool:
    try:
        encoded = image_data.split(",", 1)[1]
        payload = base64.b64decode(encoded, validate=True)
        if payload[:8] != b"\x89PNG\r\n\x1a\n":
            return False
        offset = 8
        idat = bytearray()
        width = height = bit_depth = color_type = interlace = None
        while offset + 12 <= len(payload):
            length = struct.unpack_from(">I", payload, offset)[0]
            kind = payload[offset + 4:offset + 8]
            data_start = offset + 8
            data_end = data_start + length
            if data_end + 4 > len(payload):
                return False
            data = payload[data_start:data_end]
            if kind == b"IHDR":
                width, height, bit_depth, color_type, compression, filtering, interlace = (
                    struct.unpack(">IIBBBBB", data)
                )
                if compression != 0 or filtering != 0:
                    return False
            elif kind == b"IDAT":
                idat.extend(data)
            elif kind == b"IEND":
                break
            offset = data_end + 4
        channels = {0: 1, 2: 3, 4: 2, 6: 4}.get(color_type)
        if (
            width is None or height is None or width <= 0 or height <= 0
            or bit_depth != 8 or interlace != 0 or channels is None
        ):
            return False
        decoded = zlib.decompress(bytes(idat))
        stride = width * channels
        if len(decoded) != height * (stride + 1):
            return False
        previous = bytearray(stride)
        cursor = 0
        for _ in range(height):
            filter_kind = decoded[cursor]
            cursor += 1
            filtered = decoded[cursor:cursor + stride]
            cursor += stride
            scanline = bytearray(stride)
            for index, value in enumerate(filtered):
                left = scanline[index - channels] if index >= channels else 0
                above = previous[index]
                upper_left = previous[index - channels] if index >= channels else 0
                if filter_kind == 0:
                    predictor = 0
                elif filter_kind == 1:
                    predictor = left
                elif filter_kind == 2:
                    predictor = above
                elif filter_kind == 3:
                    predictor = (left + above) // 2
                elif filter_kind == 4:
                    estimate = left + above - upper_left
                    distances = (
                        abs(estimate - left), abs(estimate - above),
                        abs(estimate - upper_left),
                    )
                    predictor = (left, above, upper_left)[distances.index(min(distances))]
                else:
                    return False
                scanline[index] = (value + predictor) & 0xFF
            for pixel in range(0, stride, channels):
                if color_type in {0, 4}:
                    brightness = scanline[pixel]
                    alpha = scanline[pixel + 1] if color_type == 4 else 255
                else:
                    brightness = max(scanline[pixel:pixel + 3])
                    alpha = scanline[pixel + 3] if color_type == 6 else 255
                if alpha > 0 and brightness > 8:
                    return True
            previous = scanline
        return False
    except (IndexError, TypeError, ValueError, struct.error, zlib.error):
        return False


def wait_for_obs_source_ready(
    websocket: ObsWebSocket,
    source_name: str,
    timeout_seconds: float = 10.0,
) -> None:
    deadline = time.monotonic() + timeout_seconds
    last_error: RuntimeError | None = None
    while True:
        try:
            screenshot = websocket.request("GetSourceScreenshot", {
                "sourceName": source_name,
                "imageFormat": "png",
                "imageWidth": 64,
                "imageHeight": 36,
                "imageCompressionQuality": -1,
            })
            if obs_screenshot_has_visible_pixels(str(screenshot.get("imageData", ""))):
                return
        except RuntimeError as error:
            last_error = error
        if time.monotonic() >= deadline:
            detail = f": {last_error}" if last_error else ""
            raise RuntimeError(f"OBS capture source remained black{detail}")
        time.sleep(0.1)


def set_window_topmost(window_handle: int, topmost: bool) -> None:
    if os.name != "nt":
        return
    import ctypes
    from ctypes import wintypes

    is_window = ctypes.windll.user32.IsWindow
    is_window.argtypes = [wintypes.HWND]
    is_window.restype = wintypes.BOOL
    if not is_window(window_handle):
        return

    # Keep the diagnostic visible in the monitor duplication stream without
    # activating it, so unrelated mouse/keyboard work remains uninterrupted.
    set_window_pos = ctypes.windll.user32.SetWindowPos
    set_window_pos.argtypes = [
        wintypes.HWND,
        wintypes.HWND,
        ctypes.c_int,
        ctypes.c_int,
        ctypes.c_int,
        ctypes.c_int,
        wintypes.UINT,
    ]
    set_window_pos.restype = wintypes.BOOL
    insert_after = -1 if topmost else -2  # HWND_TOPMOST / HWND_NOTOPMOST
    flags = 0x0001 | 0x0002 | 0x0010  # NOSIZE | NOMOVE | NOACTIVATE
    if not set_window_pos(
        window_handle, insert_after, 0, 0, 0, 0, flags
    ):
        raise RuntimeError("could not update the tactical window's topmost state")


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
    override = os.environ.get("ADVENTURESIM_RUNTIME_ROOT")
    if override:
        path = Path(override)
        if not path.is_absolute():
            raise ValueError("ADVENTURESIM_RUNTIME_ROOT must be absolute")
        return path
    if os.name == "nt":
        base = os.environ.get("LOCALAPPDATA")
        if not base:
            raise ValueError("LOCALAPPDATA is required for isolated profiles")
        return resolve_writable_directory(Path(base) / "AdventureSimulator" / "runtime")
    base = os.environ.get("XDG_RUNTIME_DIR") or os.environ.get("XDG_CACHE_HOME")
    if base:
        return Path(base) / "adventure-simulator"
    return Path.home() / ".cache" / "adventure-simulator" / "runtime"


def resolve_writable_directory(path: Path) -> Path:
    """Return where this Python process actually creates children of `path`.

    Microsoft Store Python can virtualize writes below ``LOCALAPPDATA`` into
    the package's ``LocalCache``. Resolving an existing parent still reports
    the unvirtualized path, while resolving a newly created child reports its
    real redirected location. Canonicalize through a private probe directory
    so containment checks and child processes use the same physical root.
    """
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    probe = path / f".path-resolution-{os.getpid()}-{time.time_ns()}"
    probe.mkdir(mode=0o700)
    try:
        return probe.resolve(strict=True).parent
    finally:
        probe.rmdir()


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


def file_tree_digest(paths: list[Path], *, relative_to: Path = ROOT) -> str:
    digest = hashlib.sha256()
    for path in sorted({path.resolve() for path in paths}, key=lambda item: str(item).lower()):
        label = path.relative_to(relative_to.resolve()).as_posix().encode("utf-8")
        digest.update(len(label).to_bytes(4, "big"))
        digest.update(label)
        contents = path.read_bytes()
        digest.update(len(contents).to_bytes(8, "big"))
        digest.update(contents)
    return digest.hexdigest()


def spacetime_build_identity() -> str:
    result = run_checked(["spacetime", "--version"])
    if result.returncode:
        raise RuntimeError("could not identify the SpacetimeDB build tool")
    return " ".join(result.stdout.split())


def workspace_module_files(
    root: Path = ROOT,
    module_dir: Path = MODULE_DIR,
) -> list[Path]:
    result = run_checked([
        "cargo", "metadata", "--format-version", "1", "--no-deps", "--offline",
    ], root)
    if result.returncode:
        raise RuntimeError(f"cargo metadata failed while identifying module inputs:\n{result.stdout}")
    metadata = json.loads(result.stdout)
    packages = {package["id"]: package for package in metadata["packages"]}
    packages_by_root = {
        Path(package["manifest_path"]).parent.resolve(): package
        for package in packages.values()
    }
    module_manifest = str((module_dir / "Cargo.toml").resolve())
    module = next(
        (package for package in packages.values()
         if str(Path(package["manifest_path"]).resolve()) == module_manifest),
        None,
    )
    if module is None:
        raise RuntimeError("SpacetimeDB module is absent from cargo metadata")
    pending = [module["id"]]
    workspace_ids: set[str] = set()
    while pending:
        package_id = pending.pop()
        if package_id in workspace_ids:
            continue
        workspace_ids.add(package_id)
        for dependency in packages[package_id]["dependencies"]:
            dependency_path = dependency.get("path")
            if dependency_path is None:
                continue
            dependency_package = packages_by_root.get(Path(dependency_path).resolve())
            if dependency_package is not None:
                pending.append(dependency_package["id"])

    inputs = [root / "Cargo.toml", root / "Cargo.lock"]
    for optional in (root / "rust-toolchain.toml", root / ".cargo" / "config.toml"):
        if optional.is_file():
            inputs.append(optional)
    ignored_parts = {".git", "target", "node_modules", "__pycache__"}
    for package_id in workspace_ids:
        package_root = Path(packages[package_id]["manifest_path"]).parent
        inputs.extend(
            path for path in package_root.rglob("*")
            if path.is_file() and not ignored_parts.intersection(path.parts)
        )
    return inputs


def module_input_digest(
    root: Path = ROOT,
    module_dir: Path = MODULE_DIR,
) -> str:
    digest = hashlib.sha256()
    digest.update(spacetime_build_identity().encode("utf-8"))
    digest.update(file_tree_digest(
        workspace_module_files(root, module_dir), relative_to=root
    ).encode("ascii"))
    return digest.hexdigest()


def generated_bindings_digest(client_dir: Path = CLIENT_DIR) -> str:
    files = sorted(path for path in client_dir.glob("*.rs") if path.name != "lib.rs")
    return file_tree_digest(files, relative_to=client_dir)


def read_json_object(path: Path) -> dict[str, object] | None:
    if path.is_symlink() or not path.is_file():
        return None
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    return value if isinstance(value, dict) else None


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


class _SeedHttpClient:
    """One keep-alive HTTP connection to the local SpacetimeDB, reused for every
    staged-bootstrap reducer call and row-count query.

    The staged seed makes dozens of small reducer calls. Doing each as a
    separate `spacetime` CLI invocation costs seconds of process start-up and
    reconnect per call (minutes total) while the actual work is milliseconds.
    The CLI is just an HTTP client to the local node, so we issue the same
    requests directly over a single persistent connection instead: the seed then
    runs in roughly its real compute time.
    """

    def __init__(self, server: str, database: str, bootstrap_token: str) -> None:
        parsed = urlparse(server)
        self._host = parsed.hostname or "127.0.0.1"
        self._port = parsed.port or 80
        self._database = database
        self._bootstrap_token = bootstrap_token
        self._headers = {"Content-Type": "application/json"}
        token_result = run_checked(["spacetime", "login", "show", "--token"])
        tokens = JWT_RE.findall(token_result.stdout)
        if not token_result.returncode and len(tokens) == 1:
            self._headers["Authorization"] = f"Bearer {tokens[0]}"
        self._conn = http.client.HTTPConnection(self._host, self._port, timeout=180)

    def _request(self, path: str, body: str, headers: dict[str, str]) -> tuple[int, str]:
        # Reconnect once if the keep-alive connection was dropped mid-run.
        for attempt in range(2):
            try:
                self._conn.request("POST", path, body=body, headers=headers)
                response = self._conn.getresponse()
                return response.status, response.read().decode("utf-8", "replace")
            except (http.client.HTTPException, OSError):
                self._conn.close()
                if attempt == 1:
                    raise
                self._conn = http.client.HTTPConnection(
                    self._host, self._port, timeout=180
                )
        raise RuntimeError("unreachable")

    def call(self, reducer: str, *args: object) -> int:
        """Invoke one split-bootstrap reducer as its own transaction. `args` are
        the reducer arguments after the bootstrap token (strings stay JSON
        strings, ints stay JSON numbers). Returns 0 on success, 1 on failure
        (surfacing the reducer error rather than hiding it)."""
        body = json.dumps([self._bootstrap_token, *args])
        path = f"/v1/database/{self._database}/call/{reducer}"
        try:
            status, text = self._request(path, body, self._headers)
        except OSError as error:
            print(f"reducer {reducer!r} HTTP call failed: {error}", file=sys.stderr)
            return 1
        if status // 100 != 2:
            print(
                f"development bootstrap reducer {reducer!r} failed "
                f"(HTTP {status}): {text.strip()}",
                file=sys.stderr,
            )
            return 1
        return 0

    def count(self, table: str) -> int:
        """Return a table's row count via the SQL endpoint over the same
        connection, so the caller can drive per-settlement / per-gallery-item
        seeding and stop when a call adds nothing."""
        headers = dict(self._headers)
        headers["Content-Type"] = "text/plain"
        path = f"/v1/database/{self._database}/sql"
        status, text = self._request(
            path, f"SELECT COUNT(*) AS count FROM {table}", headers
        )
        if status // 100 != 2:
            raise RuntimeError(f"sql count on {table} failed (HTTP {status}): {text.strip()}")
        try:
            value = json.loads(text)[0]["rows"][0][0]
        except (ValueError, IndexError, KeyError, TypeError) as error:
            raise RuntimeError(
                f"unexpected sql response counting {table}: {text}"
            ) from error
        return int(value)

    def close(self) -> None:
        self._conn.close()


def seed(server: str, database: str, bootstrap_token: str) -> int:
    # The monolithic `bootstrap_development_world` exceeds SpacetimeDB's
    # per-reducer compute budget (HTTP 402). The same world is instead seeded
    # across many small transactions whose union is byte-identical: base
    # geography/settlements, per-settlement strategic activity (quest generation
    # batched so no single call blows the budget), demo characters, then the
    # scenario gallery. All the calls share one persistent HTTP connection so
    # the fine-grained split costs ~its compute time, not a CLI spawn per call.
    client = _SeedHttpClient(server, database, bootstrap_token)
    try:
        print("[seed] staged bootstrap (base -> settlements -> demos -> gallery)", flush=True)
        returncode = client.call("dev_bootstrap_base")
        if returncode:
            return returncode

        # One call per settlement materializes its full activity (all quests) in a
        # single transaction; that is well under the per-reducer budget and avoids
        # re-running the idempotent population/incident work once per quest.
        for index in range(client.count("settlement")):
            returncode = client.call("dev_bootstrap_settlement_activity", index)
            if returncode:
                return returncode

        # All demo characters in one call (each is individually cheap).
        returncode = client.call("dev_bootstrap_finalize")
        if returncode:
            return returncode

        # The scenario gallery can't fit in one transaction, but a batch of a few
        # items does. Materialize it in batches, stopping once a batch adds no new
        # scenario row (fresh DBs add >=1 per item; an over-long final batch just
        # no-ops past the end). Then run the postcondition check once.
        print("[seed] materializing development scenario gallery", flush=True)
        GALLERY_BATCH = 8
        GALLERY_OFFSET_CAP = 256  # backstop against a non-terminating loop
        previous_scenarios = client.count("development_scenario")
        offset = 0
        while offset < GALLERY_OFFSET_CAP:
            returncode = client.call("dev_bootstrap_gallery", offset, GALLERY_BATCH)
            if returncode:
                return returncode
            current_scenarios = client.count("development_scenario")
            if current_scenarios == previous_scenarios:
                break
            previous_scenarios = current_scenarios
            offset += GALLERY_BATCH
        returncode = client.call("dev_bootstrap_gallery_validate")
        if returncode:
            return returncode
        print("[seed] staged bootstrap complete", flush=True)
        return 0
    finally:
        client.close()


def seed_standalone_tactical_mission(
    server: str,
    database: str,
    bootstrap_token: str,
    character_id: int,
    mission_id: str,
    scene_key: str,
    enemy_fixture_yaml: str,
    tactical_claim: str,
) -> int:
    """Call the tactical seed reducer with a JSON body so multiline YAML is
    escaped as one string instead of being parsed as raw CLI argument text."""
    client = _SeedHttpClient(server, database, bootstrap_token)
    try:
        return client.call(
            "seed_standalone_tactical_mission",
            character_id,
            mission_id,
            scene_key,
            enemy_fixture_yaml,
            tactical_claim,
        )
    finally:
        client.close()


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


def dev_bootstrap_token(root: Path = ROOT, state_root: Path | None = None) -> str:
    """Return a stable per-worktree ADVENTURESIM_DEV_BOOTSTRAP_TOKEN, generating
    and persisting one on first use.

    This token is baked into the SpacetimeDB module at compile time via
    `option_env!`, gating the dev-only seed reducers. Generating a fresh one
    on every isolated run forces `spacetime publish` to fully recompile the
    module each time (cargo correctly treats the changed env var as a cache
    miss) - a real ~90s tax even when nothing in the module source changed.
    Reusing one token per worktree keeps the compiled bytes stable across
    restarts so the build cache actually helps, while keeping the token
    off git and local to this machine.
    """
    fingerprint = worktree_fingerprint(root)
    state_root = state_root or runtime_root()
    token_dir = ensure_secure_directory(state_root / fingerprint, state_root)
    token_path = token_dir / "dev-bootstrap-token"
    if not token_path.is_symlink() and token_path.is_file():
        existing = token_path.read_text(encoding="utf-8").strip()
        if len(existing) == 64 and all(c in "0123456789abcdef" for c in existing):
            return existing
    token = secrets.token_hex(32)
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
    temporary = token_path.with_name(f".{token_path.name}.{os.getpid()}.{time.time_ns()}.tmp")
    fd = os.open(temporary, flags, 0o600)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            stream.write(token)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, token_path)
    finally:
        temporary.unlink(missing_ok=True)
    return token


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


def verify_bindings(
    *,
    cache_path: Path | None = None,
    current_module_digest: str | None = None,
) -> int:
    started_at = time.monotonic()
    current_module_digest = current_module_digest or module_input_digest()
    current_bindings_digest = generated_bindings_digest()
    if cache_path is None:
        state_root = runtime_root()
        cache_dir = ensure_secure_directory(
            state_root / worktree_fingerprint(), state_root
        )
        cache_path = cache_dir / "verify-bindings-cache.json"
    expected_cache = {
        "format": 1,
        "module_input_digest": current_module_digest,
        "generated_bindings_digest": current_bindings_digest,
    }
    if read_json_object(cache_path) == expected_cache:
        print("SpacetimeDB client bindings match the cached module schema verification.")
        print(
            "[startup] phase='database client binding verification' "
            f"duration={time.monotonic() - started_at:.3f}s cache=hit"
        )
        return 0
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
    atomic_write_json(cache_path, expected_cache)
    print("SpacetimeDB client bindings match the current module schema.")
    print(
        "[startup] phase='database client binding verification' "
        f"duration={time.monotonic() - started_at:.3f}s cache=miss"
    )
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


def tactical_profile_identity(
    values: dict[str, object],
    current_module_digest: str,
    bootstrap_token: str,
) -> dict[str, object]:
    return {
        "format": 1,
        "profile": values["profile"],
        "worktree_fingerprint": values["worktree_fingerprint"],
        "database": values["database"],
        "module_input_digest": current_module_digest,
        "bootstrap_token_digest": hashlib.sha256(
            bootstrap_token.encode("utf-8")
        ).hexdigest(),
    }


def tactical_profile_database_is_ready(server: str, database: str) -> bool:
    result = run_checked([
        "spacetime", "sql", "--server", server, database,
        "SELECT id FROM settlement WHERE id = 'riverdale'",
    ])
    return result.returncode == 0 and any(
        line.strip().strip('"') == "riverdale" for line in result.stdout.splitlines()
    )


def tactical_profile_cache_is_valid(
    state_file: Path,
    expected_identity: dict[str, object],
    server: str,
    database: str,
) -> bool:
    return (
        read_json_object(state_file) == expected_identity
        and tactical_profile_database_is_ready(server, database)
    )


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
    enemy_fixture: str,
    tactical_claim: str | None,
    scene_input: str | None = None,
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
            f"TACTICAL_ENEMY_FIXTURE={enemy_fixture}",
    ]
    if tactical_claim is not None:
        # The advanced/manual workflow needs the one-use claim. The supervised
        # launcher deliberately retains it only in memory.
        values.append(f"ADVENTURESIM_TACTICAL_CLAIM={tactical_claim}")
    optional = {
        "TACTICAL_SCENE_INPUT": scene_input,
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
    # PureWindowsPath recognizes both slash styles, so launcher metadata remains
    # testable without weakening the exact executable-name allowlist on POSIX.
    expected_name = PureWindowsPath(expected_path).stem.casefold()
    actual_name = PureWindowsPath(actual_path).stem.casefold()
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


def terminate_verified_or_accept_exit(expected: dict[str, object]) -> None:
    """Terminate an owned process, accepting a concurrent natural exit."""

    try:
        terminate_verified(expected)
    except ValueError:
        pid = int(expected.get("pid", 0))
        for _ in range(20):
            if process_snapshot(pid) is None:
                return
            time.sleep(0.025)
        raise


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
    working_directory: Path = ROOT,
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
            cwd=working_directory,
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
        terminate_verified_or_accept_exit(process)
    metadata_file.unlink()


def stop_spacetime(metadata_file: Path, expected_config: dict[str, object]) -> None:
    if not metadata_file.exists():
        return
    metadata = json.loads(metadata_file.read_text())
    if metadata.get("config") != expected_config:
        raise ValueError("refusing SpacetimeDB stop: configuration mismatch")
    listener = metadata.get("listener")
    if isinstance(listener, dict) and process_snapshot(int(listener.get("pid", 0))) is not None:
        terminate_verified_or_accept_exit(listener)
    launcher = metadata.get("process", {})
    if process_snapshot(int(launcher.get("pid", 0))) is not None:
        terminate_verified_or_accept_exit(launcher)
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
    scene_key: str = "woodland",
    character_id: int = 0,
    enemy_fixture: str = STANDARD_ENEMY_FIXTURE,
    scene_input: str | None = None,
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
            bootstrap_token = dev_bootstrap_token()
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
            # Only strategic profiles need the full development world. A tactical
            # host is self-contained: `seed_standalone_tactical_mission` seeds the
            # light host world, the player character + inventory, the party, and
            # the mission on its own, so an isolated tactical database skips the
            # (much heavier) strategic bootstrap entirely.
            if mode is ProfileMode.STRATEGIC:
                code = seed(server, database, bootstrap_token)
                if code:
                    return code

            if mode is ProfileMode.TACTICAL:
                tactical_claim = secrets.token_hex(32)
                enemy_fixture_yaml = read_enemy_fixture(enemy_fixture)
                result = seed_standalone_tactical_mission(
                    server, database, bootstrap_token, character_id, mission_id,
                    scene_key, enemy_fixture_yaml, tactical_claim,
                )
                if result:
                    print("standalone tactical mission seed failed; refusing to hide the reducer error.", file=sys.stderr)
                    return result
                write_tactical_env_file(
                    url=server, database=database, port=int(values["tactical_port"]),
                    mission_id=mission_id, scene_key=scene_key,
                    character_id=character_id, enemy_fixture=enemy_fixture,
                    tactical_claim=tactical_claim,
                    scene_input=scene_input,
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


def live_spacetime_for_profile(name: str, base_port: int) -> dict[str, str] | None:
    """Return {server, database} for an already-running, ownership-verified
    isolated SpacetimeDB instance for this profile, or None if none is live.

    Mirrors the config-match + identity_matches check `stop_spacetime` uses,
    so this only ever "finds" an instance this same tooling started for this
    exact profile - never an unrelated process that happens to hold the port.
    """
    values = profile_values(name, base_port)
    run_dir = Path(str(values["profile_dir"])) / "run"
    stdb_metadata = run_dir / "spacetime.identity.json"
    if not stdb_metadata.exists():
        return None
    try:
        metadata = json.loads(stdb_metadata.read_text())
    except (OSError, json.JSONDecodeError):
        return None
    server = f"http://127.0.0.1:{values['spacetime_port']}"
    database = str(values["database"])
    expected_config = {
        "role": "spacetimedb", "profile": name,
        "worktree_fingerprint": values["worktree_fingerprint"], "server": server,
        "database": database, "data_dir": str(values["data_dir"]),
    }
    if metadata.get("config") != expected_config or not identity_matches(metadata.get("process", {})):
        return None
    return {"server": server, "database": database}


def reseed_tactical_mission(
    profile: str,
    base_port: int,
    mission_id_prefix: str = "mission:test-mission",
    scene_key: str = "hills",
    character_id: int = 0,
    enemy_fixture: str = STANDARD_ENEMY_FIXTURE,
    if_live: bool = False,
) -> int:
    """Seed a fresh standalone tactical mission against an already-running
    isolated SpacetimeDB instance, without rebuilding or republishing the
    module.

    `bootstrap_development_world` is idempotent (every fixture it seeds is
    guarded by a find-before-insert check), so it's safe to call again on a
    database that already has world data. `seed_standalone_tactical_mission`
    is not safe to call twice with the same mission_id once a previous
    mission has bound a tactical_server_authority row for it (it silently
    no-ops rather than erroring, and a prior mission that never shut down
    cleanly leaves that row orphaned forever) - so this always mints a fresh
    randomized mission_id instead of reusing `mission_id_prefix` verbatim.

    `if_live=True` (used when this is an automatic step ahead of `just
    tactical`, not a standalone user-facing call) makes a missing live
    instance a silent no-op (exit 0) instead of an error, so it doesn't
    block plain `cargo run`-style usage against a non-isolated database.
    """
    live = live_spacetime_for_profile(profile, base_port)
    if live is None:
        if if_live:
            return 0
        print(
            f"No live, verified SpacetimeDB instance found for profile {profile!r}.\n"
            "Run `just tactical-isolated` first (once), then `just tactical-reseed` "
            "to get a fresh mission against it without rebuilding/republishing the module.",
            file=sys.stderr,
        )
        return 1
    server, database = live["server"], live["database"]
    bootstrap_token = dev_bootstrap_token()
    # Reseeding a tactical mission needs no strategic world:
    # `seed_standalone_tactical_mission` is self-contained (host world, player
    # character + inventory, party, and mission), so skip the strategic bootstrap.
    mission_id = f"{mission_id_prefix}-{secrets.token_hex(4)}"
    tactical_claim = secrets.token_hex(32)
    enemy_fixture_yaml = read_enemy_fixture(enemy_fixture)
    result = seed_standalone_tactical_mission(
        server, database, bootstrap_token, character_id, mission_id, scene_key,
        enemy_fixture_yaml, tactical_claim,
    )
    if result:
        print("standalone tactical mission seed failed; refusing to hide the reducer error.", file=sys.stderr)
        return result
    values = profile_values(profile, base_port)
    write_tactical_env_file(
        url=server, database=database, port=int(values["tactical_port"]),
        mission_id=mission_id, scene_key=scene_key,
        character_id=character_id, enemy_fixture=enemy_fixture,
        tactical_claim=tactical_claim,
    )
    print("")
    print(f"Reseeded tactical mission {mission_id!r} against the already-running isolated database.")
    print("Run `just tactical` and `just client` in other terminals (no arguments needed).")
    return 0


def tactical_executable(package: str, build_profile: str = "dev") -> Path:
    if build_profile not in {"dev", "release"}:
        raise ValueError(f"unsupported tactical build profile: {build_profile}")
    suffix = ".exe" if os.name == "nt" else ""
    target = cargo_target_dir()
    if not target.is_absolute():
        target = ROOT / target
    directory = "release" if build_profile == "release" else "debug"
    return (target / directory / f"{package}{suffix}").resolve()


def build_tactical_play(launch_client: bool, client_profile: str = "dev") -> int:
    if client_profile not in {"dev", "release"}:
        raise ValueError(f"unsupported tactical client profile: {client_profile}")
    commands = [[
        "cargo", "build", "--package", "adventuresim-tactical-server",
        "--bin", "adventuresim-tactical-server", "--features", "debug",
    ]]
    if launch_client:
        client_command = [
            "cargo", "build", "--package", "adventuresim-tactical-client",
            "--bin", "adventuresim-tactical-client", "--features", "debug",
        ]
        if client_profile == "release":
            client_command.insert(2, "--release")
        commands.append(client_command)
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


def wait_for_tactical_client(
    process: subprocess.Popen[str],
    client_log_file: Path,
    server_log_file: Path,
) -> None:
    """Wait until the server receives input from the fully loaded client."""

    for _ in range(600):
        if process.poll() is not None:
            raise RuntimeError(
                f"native client exited before its world was ready.\n"
                f"Log: {client_log_file}\n{log_tail(client_log_file)}"
            )
        try:
            text = server_log_file.read_text(encoding="utf-8", errors="replace")
        except OSError:
            text = ""
        if "first server input received" in text:
            return
        time.sleep(0.1)
    raise RuntimeError(
        "server did not receive input from the native client within 60 seconds.\n"
        f"Client log: {client_log_file}\n{log_tail(client_log_file)}\n"
        f"Server log: {server_log_file}\n{log_tail(server_log_file)}"
    )


def tactical_session_config(
    values: dict[str, object],
    mode: TacticalPlayMode,
    mission_id: str,
    character_id: int,
    enemy_fixture: str,
    session_id: str,
    scene_input: str | None = None,
    graphics_config: str = "assets/config/tactical-graphics.yaml",
    window_capture: str = "auto",
    capture_source: str = "window",
    render_backend: str = "auto",
    input_script: str | None = None,
    client_profile: str = "dev",
    frame_timing_seconds: float | None = None,
    frame_timing_warmup_seconds: float = 5.0,
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
        "enemy_fixture": enemy_fixture,
        "play_mode": mode.value,
        "combat_enabled": mode in (TacticalPlayMode.ANIMATION, TacticalPlayMode.COMBAT),
        "native_client": mode is not TacticalPlayMode.NETWORKING,
        "browser_client": False,
        "session_id": session_id,
        "scene_input": scene_input,
        "graphics_config": graphics_config,
        "window_capture": window_capture,
        "capture_source": capture_source,
        "render_backend": render_backend,
        "input_script_source": input_script,
        "client_profile": client_profile,
        "frame_timing_seconds": frame_timing_seconds,
        "frame_timing_warmup_seconds": frame_timing_warmup_seconds,
    }


def tactical_combat_scale(mode: TacticalPlayMode) -> int:
    return 10_000 if mode in (TacticalPlayMode.ANIMATION, TacticalPlayMode.COMBAT) else 0


def launch_recorded_tactical_client(
    run_dir: Path,
    config: dict[str, object],
) -> subprocess.Popen[str]:
    executable = tactical_executable(
        "adventuresim-tactical-client", str(config.get("client_profile", "dev"))
    )
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
        "render_backend": config.get("render_backend", "auto"),
    }
    command = [
        str(executable), "--id", str(config["character_id"]),
        "--server-addr", str(client_config["server_addr"]),
        "--graphics-config", str(
            config.get("graphics_config", "assets/config/tactical-graphics.yaml")
        ),
    ]
    suffix = str(config["session_id"])[:12]
    if config.get("frame_timing_seconds") is not None:
        frame_timing_log = run_dir / f"frame-timing-{suffix}.jsonl"
        command.extend([
            "--frame-timing-log", str(frame_timing_log),
            "--frame-timing-seconds", str(config["frame_timing_seconds"]),
            "--frame-timing-warmup-seconds",
            str(config.get("frame_timing_warmup_seconds", 5.0)),
        ])
        client_config["frame_timing_log"] = str(frame_timing_log)
        config["frame_timing_log"] = str(frame_timing_log)
    if config["play_mode"] == TacticalPlayMode.DIAGNOSTIC.value:
        animation_log = run_dir / f"animation-state-{suffix}.jsonl"
        command.extend(["--animation-log", str(animation_log)])
        client_config["animation_log"] = str(animation_log)
        config["animation_log"] = str(animation_log)
        input_script = run_dir / f"animation-input-script-{suffix}.json"
        attack_screenshot = run_dir / f"animation-attack-{suffix}.png"
        commands: list[dict[str, object]] = []
        if config.get("window_capture") != "off":
            capture_ready = run_dir / f"capture-ready-{suffix}.json"
            commands.append({"type": "wait_for_signal", "path": str(capture_ready)})
            config["capture_ready_signal"] = str(capture_ready)
        default_commands: list[dict[str, object]] = [
            {"type": "rotate", "degrees_right": 90.0},
            {"type": "guard", "raised": False},
            {
                "type": "move", "direction": "forward",
                "input_speed": 0.5, "duration_seconds": 2.0,
            },
            {"type": "guard", "raised": True},
            {"type": "wait", "duration_seconds": 0.5},
            {"type": "attack", "duration_seconds": 0.25},
            {"type": "screenshot", "path": str(attack_screenshot)},
            {"type": "wait", "duration_seconds": 0.75},
            {"type": "guard", "raised": False},
            {
                "type": "move", "direction": "forward",
                "input_speed": 1.0, "duration_seconds": 2.0,
            },
            {
                "type": "slide", "direction": "forward",
                "duration_seconds": 1.5,
            },
            {"type": "toggle_posture", "duration_seconds": 1.2},
            {
                "type": "dive", "direction": "forward",
                "duration_seconds": 1.5,
            },
            {"type": "toggle_posture", "duration_seconds": 1.2},
            {
                "type": "dive", "direction": "backward",
                "duration_seconds": 1.5,
            },
            {"type": "toggle_posture", "duration_seconds": 1.2},
            {
                "type": "dive", "direction": "left",
                "duration_seconds": 1.5,
            },
            {"type": "toggle_posture", "duration_seconds": 1.2},
            {
                "type": "dive", "direction": "right",
                "duration_seconds": 1.5,
            },
            {"type": "guard", "raised": False},
            {"type": "wait", "duration_seconds": 0.5},
        ]
        input_script_source = config.get("input_script_source")
        if input_script_source:
            source_path = Path(str(input_script_source))
            if not source_path.is_absolute():
                source_path = ROOT / source_path
            try:
                source = json.loads(source_path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError) as error:
                raise ValueError(
                    f"failed to read diagnostic input script {source_path}: {error}"
                ) from error
            source_commands = source.get("commands") if isinstance(source, dict) else None
            if not isinstance(source_commands, list) or not source_commands:
                raise ValueError(
                    f"diagnostic input script must contain a non-empty commands list: {source_path}"
                )
            if not all(isinstance(command, dict) for command in source_commands):
                raise ValueError(
                    f"diagnostic input script commands must be objects: {source_path}"
                )
            commands.extend(source_commands)
            client_config["input_script_source"] = str(source_path.resolve())
            config["input_script_source"] = str(source_path.resolve())
        else:
            commands.extend(default_commands)
        atomic_write_json(input_script, {
            "commands": commands
        })
        command.extend([
            "--input-script", str(input_script),
            "--exit-after-script",
        ])
        client_config["input_script"] = str(input_script)
        config["input_script"] = str(input_script)
        if not input_script_source:
            client_config["animation_attack_screenshot"] = str(attack_screenshot)
            config["animation_attack_screenshot"] = str(attack_screenshot)
    environment = os.environ.copy()
    if config.get("render_backend", "auto") == "auto":
        environment.pop("WGPU_BACKEND", None)
    else:
        environment["WGPU_BACKEND"] = str(config["render_backend"])
    process = spawn_recorded(
        command,
        run_dir / "client.identity.json",
        run_dir / "client.log",
        client_config,
        environment=environment,
    )
    config["client_pid"] = process.pid
    time.sleep(0.5)
    if process.poll() is not None:
        raise RuntimeError(
            f"native client exited during launch.\nLog: {run_dir / 'client.log'}\n"
            f"{log_tail(run_dir / 'client.log')}"
        )
    return process


def release_capture_gate(config: dict[str, object]) -> None:
    signal = config.get("capture_ready_signal")
    if signal:
        atomic_write_json(Path(str(signal)), {"ready": True})


def find_presentmon() -> Path | None:
    configured = os.environ.get("PRESENTMON_PATH")
    program_files = Path(os.environ.get("ProgramFiles", r"C:\Program Files"))
    candidates = [
        Path(configured) if configured else None,
        Path(found) if (found := shutil.which("PresentMon")) else None,
        program_files / "AMD" / "CNext" / "CNext" / "PresentMon-x64.exe",
        program_files / "PresentMon" / "PresentMon.exe",
    ]
    return next((candidate for candidate in candidates if candidate and candidate.is_file()), None)


def launch_presentmon(
    run_dir: Path,
    client_process: subprocess.Popen[str],
    config: dict[str, object],
    mode: str,
) -> subprocess.Popen[str] | None:
    if mode == "off":
        return None
    executable = find_presentmon()
    if executable is None:
        if mode == "required":
            raise RuntimeError(
                "presentation tracing requested but PresentMon was not found; "
                "set PRESENTMON_PATH"
            )
        print("Presentation trace: PresentMon not found; continuing without ETW capture")
        return None
    suffix = str(config["session_id"])[:12]
    output_file = run_dir / f"presentmon-{suffix}.csv"
    trace_config = {
        "role": "presentmon",
        "repository": str(ROOT.resolve()),
        "worktree_fingerprint": config["worktree_fingerprint"],
        "session_id": config["session_id"],
        "executable": str(executable),
        "client_pid": client_process.pid,
        "output_file": str(output_file),
    }
    process = spawn_recorded([
        str(executable),
        "--process_id", str(client_process.pid),
        "--output_file", str(output_file),
        "--date_time",
        "--track_gpu_video",
        "--no_console_stats",
        "--terminate_on_proc_exit",
        "--session_name", f"AdventureSimulator-{suffix}",
    ], run_dir / "presentmon.identity.json", run_dir / "presentmon.log", trace_config)
    time.sleep(0.25)
    if process.poll() is not None:
        message = f"PresentMon exited during launch:\n{log_tail(run_dir / 'presentmon.log')}"
        if mode == "required":
            raise RuntimeError(message)
        print(f"Presentation trace unavailable: {message}")
        return None
    config["presentmon_csv"] = str(output_file)
    return process


def find_obs() -> Path | None:
    configured = os.environ.get("OBS_PATH")
    program_files = Path(os.environ.get("ProgramFiles", r"C:\Program Files"))
    candidates = [
        Path(configured) if configured else None,
        Path(found) if (found := shutil.which("obs64")) else None,
        Path(found) if (found := shutil.which("obs")) else None,
        program_files / "obs-studio" / "bin" / "64bit" / "obs64.exe",
    ]
    return next((candidate for candidate in candidates if candidate and candidate.is_file()), None)


def unused_loopback_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def obs_websocket_config() -> Path:
    configured = os.environ.get("OBS_WEBSOCKET_CONFIG")
    if configured:
        return Path(configured)
    roaming = os.environ.get("APPDATA")
    if not roaming:
        raise RuntimeError("APPDATA is required to locate OBS settings")
    return Path(roaming) / "obs-studio" / "plugin_config" / "obs-websocket" / "config.json"


def replace_file_bytes(path: Path, content: bytes) -> None:
    temporary = path.with_name(f".{path.name}.{os.getpid()}.{time.time_ns()}.tmp")
    temporary.write_bytes(content)
    os.replace(temporary, path)


def configure_obs_workspace(
    websocket: ObsWebSocket,
    run_dir: Path,
    config: dict[str, object],
) -> None:
    profile_name = os.environ.get("OBS_PROFILE", "Fabelgeist Diagnostics")
    collection_name = os.environ.get(
        "OBS_COLLECTION", "Fabelgeist Diagnostics"
    )
    profiles = websocket.request("GetProfileList")
    original_profile = str(profiles.get("currentProfileName", ""))
    available_profiles = {str(profile) for profile in profiles.get("profiles", [])}
    config["obs_original_profile"] = original_profile
    config["obs_capture_profile"] = profile_name
    if profile_name not in available_profiles:
        websocket.request("CreateProfile", {"profileName": profile_name})
    elif original_profile != profile_name:
        websocket.request("SetCurrentProfile", {"profileName": profile_name})
    time.sleep(0.25)
    websocket.request("SetRecordDirectory", {"recordDirectory": str(run_dir)})

    collections = websocket.request("GetSceneCollectionList")
    original_collection = str(collections.get("currentSceneCollectionName", ""))
    available_collections = {
        str(collection) for collection in collections.get("sceneCollections", [])
    }
    config["obs_original_collection"] = original_collection
    config["obs_capture_collection"] = collection_name
    if collection_name not in available_collections:
        websocket.request(
            "CreateSceneCollection", {"sceneCollectionName": collection_name}
        )
    elif original_collection != collection_name:
        websocket.request(
            "SetCurrentSceneCollection", {"sceneCollectionName": collection_name}
        )
    time.sleep(0.25)


def restore_obs_workspace(
    websocket: ObsWebSocket,
    config: dict[str, object],
    *,
    remove_capture_scene: bool,
) -> None:
    scene_name = config.get("obs_capture_scene")
    original_scene = config.get("obs_original_scene")
    if original_scene:
        websocket.request("SetCurrentProgramScene", {"sceneName": original_scene})
    if remove_capture_scene and scene_name:
        websocket.request("RemoveScene", {"sceneName": scene_name})

    original_collection = config.get("obs_original_collection")
    capture_collection = config.get("obs_capture_collection")
    if original_collection and original_collection != capture_collection:
        websocket.request(
            "SetCurrentSceneCollection",
            {"sceneCollectionName": original_collection},
        )
        time.sleep(0.25)
    original_profile = config.get("obs_original_profile")
    capture_profile = config.get("obs_capture_profile")
    if original_profile and original_profile != capture_profile:
        websocket.request("SetCurrentProfile", {"profileName": original_profile})
        time.sleep(0.25)


def launch_obs_capture(
    run_dir: Path,
    config: dict[str, object],
    mode: str,
) -> ObsCapture | None:
    if mode == "off":
        return None
    executable = find_obs()
    if executable is None:
        if mode == "required":
            raise RuntimeError("window capture requested but OBS Studio was not found; set OBS_PATH")
        print("Window capture: OBS Studio not found; continuing without video")
        return None
    port = unused_loopback_port()
    password = secrets.token_urlsafe(24)
    suffix = str(config["session_id"])[:12]
    capture_source = str(config.get("capture_source", "window"))
    scene_name = f"Fabelgeist diagnostic {suffix}"
    input_name = f"Tactical client {suffix}"
    capture_config = {
        "role": f"obs-{capture_source}-capture",
        "repository": str(ROOT.resolve()),
        "worktree_fingerprint": config["worktree_fingerprint"],
        "session_id": config["session_id"],
        "executable": str(executable),
        "websocket_port": port,
        "scene_name": scene_name,
        "capture_source": capture_source,
    }
    metadata_file = run_dir / "obs.identity.json"
    try:
        websocket_config = obs_websocket_config()
        original_config = websocket_config.read_bytes()
        temporary_config = json.loads(original_config)
    except (OSError, json.JSONDecodeError, RuntimeError) as error:
        location = locals().get("websocket_config", "the configured OBS settings")
        message = (
            f"OBS WebSocket configuration is unavailable at {location}; "
            "start OBS Studio once or set OBS_WEBSOCKET_CONFIG"
        )
        if mode == "required":
            raise RuntimeError(message) from error
        print(f"Window capture unavailable: {message}")
        return None
    temporary_config.update({
        "alerts_enabled": False,
        "auth_required": True,
        "first_load": False,
        "server_enabled": True,
        "server_password": password,
        "server_port": port,
    })
    replace_file_bytes(
        websocket_config,
        (json.dumps(temporary_config, sort_keys=True) + "\n").encode("utf-8"),
    )
    try:
        process = spawn_recorded([
            str(executable), "--multi", "--minimize-to-tray",
            "--disable-missing-files-check", "--only-bundled-plugins",
            "--websocket_ipv4_only", "--websocket_port", str(port),
            "--websocket_password", password,
        ], metadata_file, run_dir / "obs-launch.log", capture_config,
            working_directory=executable.parent)
        websocket = None
        deadline = time.monotonic() + 12.0
        while time.monotonic() < deadline and process.poll() is None:
            try:
                websocket = ObsWebSocket(port, password)
                break
            except (ConnectionError, OSError, RuntimeError, socket.timeout):
                time.sleep(0.25)
    finally:
        replace_file_bytes(websocket_config, original_config)
    if websocket is None:
        stop_recorded(metadata_file, capture_config)
        message = f"OBS did not expose its control socket; see {run_dir / 'obs-launch.log'}"
        if mode == "required":
            raise RuntimeError(message)
        print(f"Window capture unavailable: {message}")
        return None
    # The socket becomes available slightly before the OBS frontend finishes
    # accepting scene mutations on slower integrated-GPU startup paths.
    time.sleep(1.0)
    try:
        configure_obs_workspace(websocket, run_dir, config)
        original_scene = websocket.request("GetCurrentProgramScene").get("currentProgramSceneName")
        scenes = websocket.request("GetSceneList").get("scenes", [])
        scene_names = {str(scene.get("sceneName", "")) for scene in scenes}
        idle_scene = "Fabelgeist Diagnostics"
        if idle_scene not in scene_names:
            websocket.request("CreateScene", {"sceneName": idle_scene})
            scene_names.add(idle_scene)
        stale_scenes = [
            scene.get("sceneName") for scene in scenes
            if str(scene.get("sceneName", "")).startswith("Fabelgeist diagnostic ")
        ]
        if original_scene in stale_scenes:
            original_scene = idle_scene
            websocket.request(
                "SetCurrentProgramScene", {"sceneName": original_scene}
            )
        for stale_scene in stale_scenes:
            websocket.request("RemoveScene", {"sceneName": stale_scene})
        websocket.request("CreateScene", {"sceneName": scene_name})
        config["obs_original_scene"] = original_scene
        config["obs_capture_scene"] = scene_name
        if capture_source == "window":
            created = websocket.request("CreateInput", {
                "sceneName": scene_name,
                "inputName": input_name,
                "inputKind": "window_capture",
                "inputSettings": {
                    "window": (
                        "Fabelgeist - Tactical:Window Class:"
                        "adventuresim-tactical-client.exe"
                    ),
                    # OBS automatic selection chooses BitBlt for Bevy's winit
                    # window class, which can return the same stale Vulkan frame
                    # for seconds. METHOD_WGC is value 2 in win-capture.
                    "method": 2,
                    "client_area": True,
                    "cursor": False,
                    "capture_audio": False,
                },
                "sceneItemEnabled": True,
            })
            crop = None
        elif capture_source == "display":
            geometry = tactical_window_capture_geometry(int(config["client_pid"]))
            set_window_topmost(geometry["window_handle"], True)
            config["display_capture_window"] = geometry["window_handle"]
            created = websocket.request("CreateInput", {
                "sceneName": scene_name,
                "inputName": input_name,
                "inputKind": "monitor_capture",
                "inputSettings": {},
                "sceneItemEnabled": True,
            })
            properties = websocket.request("GetInputPropertiesListPropertyItems", {
                "inputName": input_name,
                "propertyName": "monitor_id",
            })
            monitor_id = select_obs_monitor_id(
                properties.get("propertyItems", []),
                geometry,
                os.environ.get("OBS_MONITOR_ID"),
            )
            websocket.request("SetInputSettings", {
                "inputName": input_name,
                "inputSettings": {"monitor_id": monitor_id},
                "overlay": True,
            })
            crop = {
                "cropLeft": geometry["left"],
                "cropTop": geometry["top"],
                "cropRight": geometry["right"],
                "cropBottom": geometry["bottom"],
            }
            config["display_capture_geometry"] = geometry
            config["display_capture_monitor_id"] = monitor_id
        else:
            raise RuntimeError(f"unsupported OBS capture source: {capture_source}")
        video = websocket.request("GetVideoSettings")
        transform = {
            "boundsType": "OBS_BOUNDS_SCALE_INNER",
            "boundsWidth": float(video["baseWidth"]),
            "boundsHeight": float(video["baseHeight"]),
            "alignment": 5,
        }
        if crop:
            transform.update(crop)
        websocket.request("SetSceneItemTransform", {
            "sceneName": scene_name,
            "sceneItemId": created["sceneItemId"],
            "sceneItemTransform": transform,
        })
        websocket.request("SetCurrentProgramScene", {"sceneName": scene_name})
        wait_for_obs_source_ready(websocket, input_name)
        config["obs_source_ready"] = True
        websocket.request("StartRecord")
    except Exception as error:
        try:
            restore_obs_workspace(
                websocket,
                config,
                remove_capture_scene=bool(config.get("obs_capture_scene")),
            )
        except Exception as cleanup_error:
            print(f"Window capture cleanup warning: {cleanup_error}", file=sys.stderr)
        display_window = config.pop("display_capture_window", None)
        if display_window is not None:
            try:
                set_window_topmost(int(display_window), False)
            except RuntimeError as cleanup_error:
                print(f"Window capture cleanup warning: {cleanup_error}", file=sys.stderr)
        websocket.close()
        stop_recorded(metadata_file, capture_config)
        if mode == "required":
            raise
        print(f"Window capture unavailable: OBS setup failed: {error}")
        return None
    print(f"Window capture: OBS recording via {capture_source} source")
    return ObsCapture(process, websocket, metadata_file)


def stop_obs_capture(capture: ObsCapture, run_dir: Path, config: dict[str, object]) -> Path:
    output_path: Path | None = None
    try:
        try:
            result = capture.websocket.request("StopRecord")
            recorded = result.get("outputPath")
            if recorded:
                output_path = Path(str(recorded))
        finally:
            restore_obs_workspace(
                capture.websocket,
                config,
                remove_capture_scene=True,
            )
        # StopRecord returns before Hybrid MP4 finishes flushing and before
        # frontend scene changes are durably saved. Give both bounded time.
        time.sleep(2.0)
    finally:
        capture.websocket.close()
        stop_recorded(capture.metadata_file)
        display_window = config.pop("display_capture_window", None)
        if display_window is not None:
            try:
                set_window_topmost(int(display_window), False)
            except RuntimeError as error:
                print(f"Window capture cleanup warning: {error}", file=sys.stderr)
    if output_path is None or not output_path.is_file():
        raise RuntimeError("OBS stopped without returning a finalized recording")
    suffix = output_path.suffix or ".mp4"
    capture_source = str(config.get("capture_source", "window"))
    destination = run_dir / (
        f"{capture_source}-capture-{str(config['session_id'])[:12]}{suffix}"
    )
    deadline = time.monotonic() + 10.0
    while True:
        try:
            shutil.move(str(output_path), destination)
            break
        except PermissionError:
            if time.monotonic() >= deadline:
                raise RuntimeError(f"OBS recording remained locked: {output_path}")
            time.sleep(0.25)
    config["window_capture_file"] = str(destination)
    return destination


def effective_presentation_trace(
    mode: TacticalPlayMode,
    requested: str,
) -> str:
    if mode is not TacticalPlayMode.DIAGNOSTIC and requested == "auto":
        return "off"
    return requested


def tactical_play(
    mode: TacticalPlayMode,
    base_port: int,
    graphics_config: str = "assets/config/tactical-graphics.yaml",
    presentation_trace: str = "auto",
    window_capture: str = "auto",
    capture_source: str = "window",
    render_backend: str = "auto",
    scene_input: str | None = None,
    enemy_fixture: str | None = None,
    input_script: str | None = None,
    client_profile: str = "dev",
    frame_timing_seconds: float | None = None,
    frame_timing_warmup_seconds: float = 5.0,
) -> int:
    benchmark = StartupBenchmark.start()
    enemy_fixture = enemy_fixture or default_enemy_fixture(mode)
    enemy_fixture_yaml = read_enemy_fixture(enemy_fixture)
    if input_script and mode is not TacticalPlayMode.DIAGNOSTIC:
        raise ValueError("--input-script is only valid for tactical-play diagnostic")
    if frame_timing_seconds is not None:
        if mode is not TacticalPlayMode.ANIMATION:
            raise ValueError("--frame-timing-seconds is only valid for tactical-play animation")
        if not math.isfinite(frame_timing_seconds) or frame_timing_seconds <= 0:
            raise ValueError("--frame-timing-seconds must be finite and greater than zero")
    if not math.isfinite(frame_timing_warmup_seconds) or frame_timing_warmup_seconds < 0:
        raise ValueError("--frame-timing-warmup-seconds must be finite and non-negative")
    launch_client = mode is not TacticalPlayMode.NETWORKING
    phase_started_at = time.monotonic()
    code = build_tactical_play(launch_client, client_profile)
    benchmark.record("native tactical binary build", phase_started_at)
    if code:
        return code

    profile = f"tactical-play-{mode.value}"
    values = profile_values(profile, base_port)
    state_root = runtime_root()
    profile_dir = ensure_secure_directory(Path(str(values["profile_dir"])), state_root)
    run_dir = ensure_secure_directory(profile_dir / "run", state_root)
    data_dir = ensure_secure_directory(profile_dir / "spacetimedb-data", state_root)
    session_id = secrets.token_hex(16)
    benchmark.attach(run_dir / f"startup-timing-{session_id[:12]}.jsonl")
    phase_started_at = time.monotonic()
    current_module_digest = module_input_digest()
    bootstrap_token = dev_bootstrap_token()
    profile_identity = tactical_profile_identity(
        values, current_module_digest, bootstrap_token
    )
    profile_state_file = profile_dir / "tactical-profile-state.json"
    benchmark.record("tactical profile identity", phase_started_at)
    mission_id = f"mission:{mode.value}-{session_id[:12]}"
    # Physical custody deliberately reserves zero as an invalid identity.
    character_id = 1
    config = tactical_session_config(
        values, mode, mission_id, character_id, enemy_fixture, session_id, scene_input,
        graphics_config,
        window_capture, capture_source, render_backend,
        input_script, client_profile, frame_timing_seconds,
        frame_timing_warmup_seconds,
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
        client_process = None
        presentmon_process = None
        obs_capture = None
        wrote_env = False
        try:
            phase_started_at = time.monotonic()
            listener = wait_for_spacetime(
                stdb, stdb_metadata, stdb_log, int(values["spacetime_port"])
            )
            benchmark.record("SpacetimeDB process readiness", phase_started_at)
            capability = ResetCapability(
                profile, base_port, server_url, database, lifecycle, listener
            )
            phase_started_at = time.monotonic()
            profile_cache_hit = tactical_profile_cache_is_valid(
                profile_state_file, profile_identity, server_url, database
            )
            benchmark.record(
                "persistent tactical profile validation",
                phase_started_at,
                cache="hit" if profile_cache_hit else "miss",
            )
            if profile_state_file.is_symlink():
                raise ValueError(
                    f"refusing symlink tactical profile state: {profile_state_file}"
                )
            if not profile_cache_hit:
                profile_state_file.unlink(missing_ok=True)
                previous_token = os.environ.get("ADVENTURESIM_DEV_BOOTSTRAP_TOKEN")
                os.environ["ADVENTURESIM_DEV_BOOTSTRAP_TOKEN"] = bootstrap_token
                phase_started_at = time.monotonic()
                try:
                    code = reset_publish(capability)
                finally:
                    if previous_token is None:
                        os.environ.pop("ADVENTURESIM_DEV_BOOTSTRAP_TOKEN", None)
                    else:
                        os.environ["ADVENTURESIM_DEV_BOOTSTRAP_TOKEN"] = previous_token
                benchmark.record(
                    "SpacetimeDB module reset and publish",
                    phase_started_at,
                    cache="miss",
                )
                if code:
                    return code
            else:
                phase_started_at = time.monotonic()
                benchmark.record(
                    "SpacetimeDB module reset and publish",
                    phase_started_at,
                    cache="hit",
                )

            tactical_claim = secrets.token_hex(32)
            phase_started_at = time.monotonic()
            result = seed_standalone_tactical_mission(
                server_url, database, bootstrap_token, character_id, mission_id,
                "woodland", enemy_fixture_yaml, tactical_claim,
            )
            if result:
                raise RuntimeError("standalone tactical mission seed failed")
            benchmark.record("standalone tactical mission seed", phase_started_at)
            if not profile_cache_hit:
                atomic_write_json(profile_state_file, profile_identity)
            write_tactical_env_file(
                url=server_url,
                database=database,
                port=int(values["tactical_port"]),
                mission_id=mission_id,
                scene_key="woodland",
                character_id=character_id,
                enemy_fixture=enemy_fixture,
                tactical_claim=None,
                scene_input=scene_input,
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
            server_command = [
                str(server_executable), "--addr", f"0.0.0.0:{values['tactical_port']}",
                "--mission-id", mission_id, "--scene-key", "woodland",
                "--spacetimedb-url", server_url, "--spacetimedb-module", database,
                "--expected-party-members", "1", "--required-enemy-kills", "1",
                "--enemy-combat-scale-bps", str(combat_scale), "--no-timeout",
                "--enemy-fixture", enemy_fixture,
            ]
            if scene_input:
                server_command.extend(["--scene-input", scene_input])
            server_process = spawn_recorded(
                server_command, server_metadata, server_log, server_config,
                environment=environment,
            )
            phase_started_at = time.monotonic()
            wait_for_tactical_server(
                server_process, server_metadata, server_log, server_url, database,
                mission_id, int(values["tactical_port"]),
            )
            benchmark.record("tactical server readiness", phase_started_at)
            if launch_client:
                phase_started_at = time.monotonic()
                client_process = launch_recorded_tactical_client(run_dir, config)
                benchmark.record("native client process launch", phase_started_at)
                phase_started_at = time.monotonic()
                wait_for_tactical_client(
                    client_process, run_dir / "client.log", server_log
                )
                benchmark.record(
                    "native client interactive readiness", phase_started_at
                )
                presentmon_process = launch_presentmon(
                    run_dir,
                    client_process,
                    config,
                    effective_presentation_trace(mode, presentation_trace),
                )
                if mode is TacticalPlayMode.DIAGNOSTIC:
                    try:
                        obs_capture = launch_obs_capture(run_dir, config, window_capture)
                    finally:
                        release_capture_gate(config)

            print("")
            print(f"Database: ready at {server_url} ({database})")
            print(f"Mission: {mission_id}")
            print("Claim: consumed successfully")
            print(f"Server: listening at ws://127.0.0.1:{values['tactical_port']}")
            if launch_client:
                print(f"Client: launched, character {character_id} (native)")
                if presentmon_process is not None:
                    print(f"Presentation trace: {config['presentmon_csv']}")
                if obs_capture is not None:
                    print("Window capture: active (final path reported after the script)")
            else:
                print("Client: not launched (networking profile)")
            print(f"Combat: {'enabled' if combat_scale else 'disabled'}")
            print("Browser client: unavailable in tactical-only mode")
            print(f"Logs: {run_dir}")
            print(f"Startup timings: {benchmark.output_path}")
            if mode is TacticalPlayMode.DIAGNOSTIC:
                print("Waiting for the bounded diagnostic client to finish...")
            else:
                print("Press Ctrl+C to stop this profile's recorded processes.")
            while server_process.poll() is None:
                bounded_client = mode is TacticalPlayMode.DIAGNOSTIC or (
                    mode is TacticalPlayMode.ANIMATION
                    and frame_timing_seconds is not None
                )
                if client_process is not None and bounded_client:
                    client_code = client_process.poll()
                    if client_code is not None:
                        if client_code:
                            raise RuntimeError(
                                f"diagnostic client exited with code {client_code}; "
                                f"see {run_dir / 'client.log'}\n"
                                f"{log_tail(run_dir / 'client.log')}"
                            )
                        if obs_capture is not None:
                            video_path = stop_obs_capture(obs_capture, run_dir, config)
                            obs_capture = None
                            print(f"Window capture complete: {video_path}")
                        if mode is TacticalPlayMode.DIAGNOSTIC:
                            print(f"Diagnostic capture complete: {config['animation_log']}")
                        else:
                            print(f"Frame timing capture complete: {config['frame_timing_log']}")
                        return 0
                time.sleep(0.25)
            raise RuntimeError(
                f"recorded tactical server exited; see {server_log}\n{log_tail(server_log)}"
            )
        except KeyboardInterrupt:
            print("\nStopping supervised tactical profile...")
            return 0
        finally:
            try:
                if obs_capture is not None:
                    try:
                        stop_obs_capture(obs_capture, run_dir, config)
                    except (OSError, RuntimeError) as error:
                        print(f"Window capture cleanup warning: {error}", file=sys.stderr)
                stop_recorded(run_dir / "client.identity.json")
                stop_recorded(run_dir / "presentmon.identity.json")
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
    runner.add_argument("--scene-key", default="woodland")
    runner.add_argument("--scene-input", default=DEFAULT_SCENE_INPUT)
    runner.add_argument("--enemy-fixture", default=STANDARD_ENEMY_FIXTURE)
    runner.add_argument("--character-id", type=int, default=1)
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
    tactical_play_parser.add_argument(
        "--graphics-config", default="assets/config/tactical-graphics.yaml"
    )
    tactical_play_parser.add_argument(
        "--presentation-trace", choices=("off", "auto", "required"), default="auto"
    )
    tactical_play_parser.add_argument(
        "--window-capture", choices=("off", "auto", "required"), default="auto"
    )
    tactical_play_parser.add_argument(
        "--capture-source", choices=("window", "display"), default="window"
    )
    tactical_play_parser.add_argument(
        "--render-backend", choices=("auto", "vulkan", "dx12"), default="auto"
    )
    tactical_play_parser.add_argument(
        "--scene-input", default=DEFAULT_SCENE_INPUT
    )
    tactical_play_parser.add_argument("--enemy-fixture")
    tactical_play_parser.add_argument("--input-script")
    tactical_play_parser.add_argument(
        "--client-profile", choices=("dev", "release"), default="dev"
    )
    tactical_play_parser.add_argument("--frame-timing-seconds", type=float)
    tactical_play_parser.add_argument(
        "--frame-timing-warmup-seconds", type=float, default=5.0
    )
    sub.add_parser("tactical-status")
    sub.add_parser("tactical-client")
    reseeder = sub.add_parser("reseed-tactical-mission")
    reseeder.add_argument("--mission-id-prefix", default="mission:test-mission")
    reseeder.add_argument("--scene-key", default="hills")
    reseeder.add_argument("--character-id", type=int, default=1)
    reseeder.add_argument("--enemy-fixture", default=STANDARD_ENEMY_FIXTURE)
    reseeder.add_argument(
        "--if-live", action="store_true",
        help="Exit 0 without printing an error if no live instance is found, "
             "instead of failing - for use as an automatic pre-step.",
    )
    reseeder.add_argument("name")
    reseeder.add_argument("base_port", type=int)
    return parser


def main() -> int:
    if os.name != "nt":
        # `run_profile`'s tactical branch and `tactical_play` both clean up
        # (including `.env.tactical`, see `remove_tactical_env_file`) in a
        # `finally` reached by letting `KeyboardInterrupt` propagate out of
        # a blocking wait - which SIGINT already does by default. SIGTERM
        # doesn't, so a `just`-recipe teardown (or any supervisor that sends
        # SIGTERM instead of Ctrl+C) would otherwise skip that cleanup and
        # leave a stale `.env.tactical` behind.
        import signal

        def _sigterm_as_keyboard_interrupt(signum: int, frame: object) -> None:
            raise KeyboardInterrupt

        signal.signal(signal.SIGTERM, _sigterm_as_keyboard_interrupt)

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
                character_id=args.character_id, enemy_fixture=args.enemy_fixture,
                scene_input=args.scene_input,
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
            return tactical_play(
                TacticalPlayMode(args.mode), args.base_port, args.graphics_config,
                args.presentation_trace, args.window_capture,
                args.capture_source, args.render_backend, args.scene_input,
                args.enemy_fixture, args.input_script, args.client_profile, args.frame_timing_seconds,
                args.frame_timing_warmup_seconds,
            )
        if args.command == "tactical-status":
            return tactical_status()
        if args.command == "tactical-client":
            return tactical_client_relaunch()
        if args.command == "reseed-tactical-mission":
            return reseed_tactical_mission(
                args.name, args.base_port, mission_id_prefix=args.mission_id_prefix,
                scene_key=args.scene_key, character_id=args.character_id,
                enemy_fixture=args.enemy_fixture, if_live=args.if_live,
            )
    except (ValueError, RuntimeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
