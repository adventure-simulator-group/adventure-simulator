"""Drives the joined client's local player over BRP and asserts it moves.
See `conftest.py` for the `tactical_client` fixture this depends on.
"""

from __future__ import annotations

import math
import time

from lib import MOVEMENT_DETECT_TIMEOUT, MOVEMENT_MIN_DELTA, MOVEMENT_TIMEOUT, tactical_brp


def test_movement_input_moves_player(tactical_client: tactical_brp.BrpClient) -> None:
    entity = tactical_brp.wait_for_entity_with_component(
        tactical_client, tactical_brp.ClientPlayer, MOVEMENT_TIMEOUT
    )
    before = tactical_client.get_components(entity, [tactical_brp.Transform])[tactical_brp.Transform]
    before_pos = before.translation

    tactical_client.insert_resource(
        tactical_brp.PlayerInputOverride(
            value=tactical_brp.PlayerInputRequest(
                movement=[0.0, 1.0],
                look=[0.0, 0.0],
                jump=tactical_brp.JumpCommand(sequence=0),
                jump_charge=False,
                downed_align=False,
                posture=tactical_brp.PostureCommand(sequence=0, action=None),
                pace="Walk",
                weapon_guard="Lowered",
            )
        )
    )
    try:
        deadline = time.monotonic() + MOVEMENT_DETECT_TIMEOUT
        after_pos = before_pos
        moved = 0.0
        while time.monotonic() < deadline:
            time.sleep(0.25)
            after = tactical_client.get_components(entity, [tactical_brp.Transform])[tactical_brp.Transform]
            after_pos = after.translation
            # Horizontal (X/Z) displacement, not a fixed axis: spawn facing
            # varies between mission instances, and "move forward" moves the
            # character along whichever world axis it happens to face -
            # checking translation.x specifically is flaky by spawn luck,
            # not by anything BRP-related. Y is up (falling/jumping), so it's
            # excluded here on purpose, not just an oversight.
            moved = math.hypot(after_pos[0] - before_pos[0], after_pos[2] - before_pos[2])
            if moved >= MOVEMENT_MIN_DELTA:
                break
    finally:
        tactical_client.insert_resource(tactical_brp.PlayerInputOverride(value=None))

    assert moved >= MOVEMENT_MIN_DELTA, f"horizontal displacement only {moved:.4f} ({before_pos} -> {after_pos})"
