"""Session-scoped fixtures shared by the tactical E2E tests: an isolated
SpacetimeDB + mission, the tactical server, and a headless client, all on
the default profile/ports. See `lib.py` for the process-spawning helpers
these are built on.

Tests that need to freely despawn/disconnect things mid-test (and so can't
share this session-scoped state with everyone else) boot their own isolated
instance instead - see `test_lifecycle.py`.
"""

from __future__ import annotations

import os
import time
from pathlib import Path
from typing import Iterator

import pytest

import lib
from lib import tactical_brp

SERVER_BRP_PORT = 15702
CLIENT_BRP_PORT = 15703


def pytest_collection_modifyitems(items: list[pytest.Item]) -> None:
    """`test_lifecycle.py` boots its own isolated mission/server/client and
    tears it all down again within a single test, which takes well over a
    minute. If that runs while the shared `tactical_client` fixture above is
    sitting idle waiting for a later test, the connection is left degraded
    enough that `test_movement.py`'s tight movement-detection window starts
    failing - observed directly: alphabetical collection order runs
    `test_lifecycle.py` between `test_connectivity.py` and
    `test_movement.py`, and the client barely twitches once `test_movement`
    finally drives it. Force `test_lifecycle.py` to collect last regardless
    of file name, so the shared fixtures are done with everyone needing them
    before it ties up the machine.
    """
    items.sort(key=lambda item: item.module.__name__ == "test_lifecycle")


@pytest.fixture(scope="session")
def tactical_mission(tmp_path_factory: pytest.TempPathFactory) -> Iterator[Path]:
    lib.ENV_FILE.unlink(missing_ok=True)
    log_path = tmp_path_factory.mktemp("tactical-brp") / "tactical-isolated.log"
    start = time.monotonic()
    spawned = lib.spawn(["just", "tactical-isolated"], log_path)
    try:
        lib.wait_for(spawned, lib.BOOTSTRAP_TIMEOUT, ready=lib.ENV_FILE.exists, what="'.env.tactical' to be written")
        lib.report_phase("tactical_mission (build + spacetimedb start + publish + world/mission seed)", start)
        yield lib.ENV_FILE
    finally:
        spawned.terminate()


@pytest.fixture(scope="session")
def tactical_server(tactical_mission: Path, tmp_path_factory: pytest.TempPathFactory) -> Iterator[tactical_brp.BrpClient]:
    log_path = tmp_path_factory.mktemp("tactical-brp") / "server.log"
    env = {**os.environ, "TACTICAL_BRP_PORT": str(SERVER_BRP_PORT)}
    start = time.monotonic()
    spawned = lib.spawn(["just", "tactical"], log_path, env=env)
    try:
        lib.wait_for(
            spawned,
            lib.SERVER_TIMEOUT,
            ready=lambda: "Server opened on" in spawned.log_text(),
            what="the tactical server to open",
        )
        lib.report_phase("tactical_server (build + start)", start)
        yield tactical_brp.BrpClient(SERVER_BRP_PORT)
    finally:
        spawned.terminate()


@pytest.fixture(scope="session")
def tactical_client(tactical_server: tactical_brp.BrpClient, tmp_path_factory: pytest.TempPathFactory) -> Iterator[tactical_brp.BrpClient]:
    log_path = tmp_path_factory.mktemp("tactical-brp") / "client.log"
    env = {**os.environ, "TACTICAL_CLIENT_BRP_PORT": str(CLIENT_BRP_PORT)}
    start = time.monotonic()
    spawned = lib.spawn(["just", "client-headless"], log_path, env=env)
    client = tactical_brp.BrpClient(CLIENT_BRP_PORT)

    def brp_reachable() -> bool:
        try:
            client.call("rpc.discover")
            return True
        except tactical_brp.BrpError:
            return False

    try:
        lib.wait_for(spawned, lib.CLIENT_TIMEOUT, ready=brp_reachable, what="the client BRP endpoint to come up")
        lib.report_phase("tactical_client (build + start + BRP reachable)", start)
        joined = time.monotonic()
        tactical_brp.wait_for_entity_with_component(client, tactical_brp.ClientPlayer, lib.CLIENT_TIMEOUT)
        lib.report_phase("tactical_client (join mission, ClientPlayer entity appears)", joined)
        yield client
    finally:
        spawned.terminate()
