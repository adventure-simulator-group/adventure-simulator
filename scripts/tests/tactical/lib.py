"""Shared infrastructure for the tactical E2E pytest suite (see `conftest.py`
for the session fixtures built on top of these, and `test_*.py` for the
actual tests).

Unlike the rest of `scripts/tests/`, these are not unit tests: fixtures and
tests here spawn real subprocesses - an isolated SpacetimeDB instance, the
tactical server, headless tactical clients (via `just tactical-isolated` /
`just tactical` / `just client-headless`) - then drive them over the Bevy
Remote Protocol using `scripts/tactical_brp.py`.

A full run pays real compile/bootstrap cost (single-digit minutes on a warm
`target/` cache, much longer cold) - run explicitly via `just
tactical-test`, not as part of the fast unit-test recipes.
"""

from __future__ import annotations

import contextlib
import os
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import Callable

REPO_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO_ROOT / "scripts"))
import tactical_brp  # noqa: E402

ENV_FILE = REPO_ROOT / ".env.tactical"

# Cold builds (SpacetimeDB module + tactical server + tactical client, each a
# sizeable Bevy dependency graph) can take well over ten minutes; a warm
# `target/` cache is closer to a minute per stage.
BOOTSTRAP_TIMEOUT = 1200.0
SERVER_TIMEOUT = 1200.0
CLIENT_TIMEOUT = 1200.0
MOVEMENT_TIMEOUT = 15.0
MOVEMENT_MIN_DELTA = 0.1
# Separate from MOVEMENT_TIMEOUT (which is also reused by test_lifecycle.py
# for entity-count convergence): the displacement-polling loop in
# test_movement.py has been observed to starve for well over 15s when other
# tests in the suite are hogging the machine, producing exactly 0.0
# displacement rather than a slow-but-nonzero one - so this window is wide
# on purpose, not tuned to any expected movement speed.
MOVEMENT_DETECT_TIMEOUT = 60.0


class SpawnedProcess:
    """A background `just` recipe (or direct `cargo` invocation) plus the
    log file its output is captured to."""

    def __init__(self, process: subprocess.Popen, log_path: Path):
        self.process = process
        self.log_path = log_path

    def log_text(self) -> str:
        return self.log_path.read_text(errors="ignore") if self.log_path.exists() else ""

    def terminate(self) -> None:
        if self.process.poll() is not None:
            return
        with contextlib.suppress(ProcessLookupError):
            os.killpg(os.getpgid(self.process.pid), signal.SIGTERM)
        try:
            self.process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            with contextlib.suppress(ProcessLookupError):
                os.killpg(os.getpgid(self.process.pid), signal.SIGKILL)


def spawn(cmd: list[str], log_path: Path, env: dict[str, str] | None = None) -> SpawnedProcess:
    log_file = log_path.open("w")
    process = subprocess.Popen(
        cmd,
        cwd=REPO_ROOT,
        stdout=log_file,
        stderr=subprocess.STDOUT,
        env=env,
        # Each `just` recipe (or direct cargo invocation) forks cargo/
        # spacetime children; put them in their own process group so
        # teardown can kill the whole tree at once.
        start_new_session=True,
    )
    log_file.close()
    return SpawnedProcess(process, log_path)


def wait_for(
    spawned: SpawnedProcess,
    timeout: float,
    *,
    ready: Callable[[], bool],
    what: str,
) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if ready():
            return
        if spawned.process.poll() is not None:
            raise RuntimeError(
                f"process for {what} exited early (code {spawned.process.returncode}):\n"
                f"{spawned.log_text()[-4000:]}"
            )
        time.sleep(1)
    raise TimeoutError(f"timed out after {timeout}s waiting for {what}:\n{spawned.log_text()[-4000:]}")


def report_phase(label: str, start: float) -> None:
    print(f"[timing] {label}: {time.monotonic() - start:.1f}s", file=sys.stderr)


def read_env_file(path: Path) -> dict[str, str]:
    """Parses a `KEY=value`-per-line file like `.env.tactical`."""
    values: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key] = value
    return values
