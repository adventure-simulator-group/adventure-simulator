"""Character-count lifecycle test: boots its own isolated mission/server/
client (distinct profile and ports from `conftest.py`'s shared fixtures) so
it can freely despawn a bot and kill the client mid-test without disturbing
the other test files' session-scoped fixtures.
"""

from __future__ import annotations

import os

import pytest

from lib import (
    BOOTSTRAP_TIMEOUT,
    CLIENT_TIMEOUT,
    ENV_FILE,
    MOVEMENT_TIMEOUT,
    SERVER_TIMEOUT,
    read_env_file,
    spawn,
    tactical_brp,
    wait_for,
)

LIFECYCLE_PROFILE = "tactical-dev-lifecycle"
LIFECYCLE_BASE_PORT = "23300"
LIFECYCLE_MISSION_ID = "mission:lifecycle-test"
LIFECYCLE_SERVER_BRP_PORT = 15704
LIFECYCLE_CLIENT_BRP_PORT = 15705
LIFECYCLE_ENEMY_COUNT = "2"
# Custody of any carried item (including the generated weapon-carry loadout)
# requires a nonzero character identity - see `CustodyCharacterId`.
LIFECYCLE_CHARACTER_ID = "1"

# A real client disconnect isn't detected instantly - the transport needs to
# notice the connection is gone - so this gets a longer timeout than the
# other checks below, which all follow from an immediate, local BRP
# operation (a fresh spawn or an explicit despawn).
DISCONNECT_TIMEOUT = 60.0


def _brp_count(client: tactical_brp.BrpClient, expected: int) -> bool:
    """True once `client` reports exactly `expected` `CharacterId` entities.

    Swallows `BrpError` so this is safe to use directly as a `wait_for`
    `ready` callback before the BRP endpoint has come up yet - `wait_for`
    doesn't retry on an exception from `ready()`, only on it returning
    `False`.
    """
    try:
        return len(client.query(with_=[tactical_brp.CharacterId])) == expected
    except tactical_brp.BrpError:
        return False


def test_character_count_reflects_bot_and_client_connect_disconnect_lifecycle(
    tmp_path_factory: pytest.TempPathFactory,
) -> None:
    """Bots, server, and a client are launched, joined, and torn down one at
    a time, asserting the `CharacterId` count visible over BRP on both the
    server and (once joined) the client at each step: 2 bots only, then 3
    once the client joins, then 2 again after a bot is despawned, then 1
    after the client disconnects.

    Bots have no network connection to sever, so "disconnecting" one is
    expressed as despawning its entity over BRP.
    """
    log_dir = tmp_path_factory.mktemp("tactical-lifecycle")

    ENV_FILE.unlink(missing_ok=True)
    mission = spawn(
        [
            "just",
            "tactical-isolated",
            LIFECYCLE_PROFILE,
            LIFECYCLE_BASE_PORT,
            LIFECYCLE_MISSION_ID,
            "hills",
            LIFECYCLE_CHARACTER_ID,
            LIFECYCLE_ENEMY_COUNT,
        ],
        log_dir / "tactical-isolated.log",
    )
    try:
        wait_for(mission, BOOTSTRAP_TIMEOUT, ready=ENV_FILE.exists, what="'.env.tactical' to be written")
        # The `tactical` recipe's first line unconditionally runs
        # `reseed-tactical-mission --if-live` against the hardcoded default
        # profile/port ("tactical-dev"/23200, matching `tactical-isolated`'s
        # own defaults) as a convenience for the common one-profile
        # workflow. `conftest.py`'s shared fixtures use exactly that default
        # profile and may still be alive at this point (session-scoped,
        # torn down only at the very end) - going through the recipe here
        # would reseed *their* mission and then, via its "re-read
        # .env.tactical after reseeding" logic, launch this test's server
        # against that clobbered mission id instead of its own. Reading the
        # values straight out of `.env.tactical` and invoking cargo directly
        # sidesteps that recipe's implicit profile coupling entirely.
        env_values = read_env_file(ENV_FILE)

        server_cmd = [
            "cargo", "run", "--package", "adventuresim-tactical-server", "--features", "debug", "--",
            "--addr", f"0.0.0.0:{env_values['TACTICAL_PORT']}",
            "--mission-id", env_values["TACTICAL_MISSION_ID"],
            "--scene-key", env_values["TACTICAL_SCENE_KEY"],
            "--spacetimedb-url", env_values["TACTICAL_SPACETIMEDB_URL"],
            "--spacetimedb-module", env_values["TACTICAL_SPACETIMEDB_MODULE"],
            "--expected-party-members", "1",
            "--required-enemy-kills", LIFECYCLE_ENEMY_COUNT,
            # Skips inserting `OffensiveCombatAi` on the bots entirely (see
            # `spawn_connected_player`), leaving them inert. Without that,
            # both bots land a lethal hit on the joining party member within
            # about half a second of connecting, ending the mission (defeat)
            # and shutting the server down before this test's assertions
            # can run.
            "--enemy-combat-scale-bps", "0",
            "--no-timeout",
            "--brp-port", str(LIFECYCLE_SERVER_BRP_PORT),
        ]
        server_env = {**os.environ, "ADVENTURESIM_TACTICAL_CLAIM": env_values["ADVENTURESIM_TACTICAL_CLAIM"]}
        server = spawn(server_cmd, log_dir / "server.log", env=server_env)
        try:
            wait_for(
                server,
                SERVER_TIMEOUT,
                ready=lambda: "Server opened on" in server.log_text(),
                what="the tactical server to open",
            )
            server_brp = tactical_brp.BrpClient(LIFECYCLE_SERVER_BRP_PORT)

            wait_for(server, MOVEMENT_TIMEOUT, ready=lambda: _brp_count(server_brp, 2), what="the server to report 2 characters (the bots)")

            client_env = {**os.environ, "TACTICAL_CLIENT_BRP_PORT": str(LIFECYCLE_CLIENT_BRP_PORT)}
            client = spawn(["just", "client-headless"], log_dir / "client.log", env=client_env)
            try:
                client_brp = tactical_brp.BrpClient(LIFECYCLE_CLIENT_BRP_PORT)

                def client_brp_reachable() -> bool:
                    try:
                        client_brp.call("rpc.discover")
                        return True
                    except tactical_brp.BrpError:
                        return False

                wait_for(client, CLIENT_TIMEOUT, ready=client_brp_reachable, what="the client BRP endpoint to come up")
                tactical_brp.wait_for_entity_with_component(client_brp, tactical_brp.ClientPlayer, CLIENT_TIMEOUT)

                wait_for(
                    server, MOVEMENT_TIMEOUT, ready=lambda: _brp_count(server_brp, 3), what="the server to report 3 characters (2 bots + the joined client)"
                )
                wait_for(client, MOVEMENT_TIMEOUT, ready=lambda: _brp_count(client_brp, 3), what="the client to report 3 characters (2 bots + itself)")

                bots = server_brp.query(with_=[tactical_brp.MissionEnemy])
                assert len(bots) == 2, f"expected 2 bots, found {bots}"
                server_brp.despawn(bots[0]["entity"])

                wait_for(
                    server, MOVEMENT_TIMEOUT, ready=lambda: _brp_count(server_brp, 2), what="the server to report 2 characters after despawning a bot"
                )
                wait_for(
                    client, MOVEMENT_TIMEOUT, ready=lambda: _brp_count(client_brp, 2), what="the client to report 2 characters after the bot despawn replicates"
                )
            finally:
                client.terminate()

            wait_for(
                server, DISCONNECT_TIMEOUT, ready=lambda: _brp_count(server_brp, 1), what="the server to report 1 character after the client disconnects"
            )
        finally:
            server.terminate()
    finally:
        mission.terminate()
