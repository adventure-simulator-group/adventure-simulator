"""Basic BRP reachability and join smoke tests. See `conftest.py` for the
`tactical_server`/`tactical_client` fixtures these depend on.
"""

from __future__ import annotations

from lib import tactical_brp


def test_server_brp_reachable(tactical_server: tactical_brp.BrpClient) -> None:
    result = tactical_server.call("rpc.discover")
    assert result["info"]["title"] == "Bevy Remote Protocol"


def test_client_brp_reachable(tactical_client: tactical_brp.BrpClient) -> None:
    result = tactical_client.call("rpc.discover")
    assert result["info"]["title"] == "Bevy Remote Protocol"


def test_client_player_entity_exists(tactical_client: tactical_brp.BrpClient) -> None:
    entity = tactical_brp.find_entity_with_component(tactical_client, tactical_brp.ClientPlayer)
    assert entity is not None
