"""Parry/dodge defense tests: loads a pre-built world-dump fixture (one
party-member "template" character facing one enemy bot at melee range - see
`fixtures/combat_scenario.scn.ron`) in standalone mode (no SpacetimeDB at
all, see `--world-dump`), connects a client, mocks a real melee attack via
`bevy_enhanced_input`'s own `ActionMock` (see
`adventuresim_tactical_client::debug::register_input_mock_types` - this
drives the actual input-processing pipeline, `Fire<Attack>` and all, not a
debug bypass of it), and checks whether the bot's `DefenseChances`
(mutated via BRP) affects whether it takes damage.

The fixture was captured from a real, normally-seeded mission: one bot given
`OffensiveCombatAi`/`DefenseChances` and positioned at melee range facing the
party-member template, which was itself positioned and faced via the same
BRP techniques this test uses live (see the tactical-testing wiki page for
how - `CharacterLook` is client-reported and won't survive a fresh client
connecting, which is why this test re-applies a facing override below rather
than trusting the fixture's captured client-side facing).
"""

from __future__ import annotations

import math
import os
import time

import pytest

from lib import CLIENT_TIMEOUT, REPO_ROOT, SERVER_TIMEOUT, SpawnedProcess, spawn, tactical_brp, wait_for

FIXTURE = REPO_ROOT / "scripts" / "tests" / "tactical" / "fixtures" / "combat_scenario.scn.ron"

# Each scenario gets its own port triplet (game, server BRP, client BRP)
# rather than sharing one: a previous scenario's client can survive its
# process-group SIGTERM (cargo's child occasionally escapes the group) while
# still holding its BRP port, and a successor reusing that port would then
# silently drive the *stale* client - whose attacks go to an already-dead
# server - while its own freshly-spawned client can't bind the port at all.
# Observed live as one scenario passing and the next seeing zero server-side
# combat traffic despite every BRP call succeeding.
COMBAT_PORTS = {
    "combat-dodge": ("23900", 15706, 15707),
    "combat-no-defense": ("23901", 15708, 15709),
}

# The fixture positions the bot at +X from the template's position, same Z
# (see fixtures/combat_scenario.scn.ron - template ~(-5.59, ..., 0.51), bot
# ~(-3.09, ..., 0.51)), so a fixed yaw always faces it. Bevy's `Quat` yaw
# convention turns out to point *away* from +X as the angle increases from
# identity (identity itself looks down -Z) - confirmed live by reading back
# `Camera3d`'s own forward vector over BRP after applying candidate yaws -
# so this is negative, not the "positive turns toward +X" a naive right-hand
# derivation from identity=-Z would suggest.
CAMERA_YAW_TO_FACE_BOT = -math.pi / 2

# The fixture's captured ~2.5m separation is within camera/selection range
# but past actual melee contact range (`HANDS_REACH` (1.5) + weapon reach
# (0.8 for the katzbalger both characters spawn with) = 2.3, checked
# server-side in `validate_melee_intent_cheap`) - repositioned here rather
# than in the fixture so the fixture keeps representing a bot that's merely
# *aware* of the party (its captured `OffensiveCombatAi.phase` is
# `Pursuing`, not already attacking).
BOT_X_WITHIN_MELEE_RANGE = -4.5

# Well above what any single hit can realistically deplete (limbs start at
# 1.0 in the fixture) - see `_toughen_bot`.
BOT_LIMB_HEALTH = 1000.0

# The server validates each attack's windup against the weapon's authored
# `windup_secs` minus a delivery-jitter tolerance (`WINDUP_JITTER_TOLERANCE`
# in server combat.rs), so a correctly-paced attack should essentially
# always clear the check now; the bot's reaction delay
# (`REACTION_DELAY_SECS`, bot/defense.rs) is likewise tuned to commit just
# before that same windup elapses, so a rolled dodge/parry is fresh at
# impact. Retries remain as insurance against residual scheduling jitter,
# not as the primary mechanism.
ATTACK_ATTEMPTS = 20

# See player.rs's `actions!(Player[Movement, Jump, RotateCamera, Attack,
# Dodge, Parry])` - `Actions<Player>.entities` preserves spawn order, and
# `Action<Attack>` itself can't be queried directly over BRP (it's a
# generic external type with no `Reflect` impl), so this index is the only
# way to find the right action entity. Keep in sync with that binding list.
ATTACK_ACTION_INDEX = 3

# From client join to a scripted attack actually landing (or missing): mock
# processed -> Fire<Attack> -> AttackState inserted, MeleeActionRequest::Start
# sent -> ~0.3s client-side windup (the weapon's `windup_secs`) -> camera
# raycast picks the target -> MeleeActionRequest::Complete -> server
# validates the windup and resolves the hit. Divided across
# `ATTACK_ATTEMPTS` retries below.
ATTACK_RESOLUTION_TIMEOUT = 20.0


def _total_limb_health(limbs: tactical_brp.Limbs) -> float:
    return limbs.left_arm + limbs.right_arm + limbs.left_leg + limbs.right_leg + limbs.chest + limbs.stomach + limbs.head


def _find_bot(server_brp: tactical_brp.BrpClient) -> int:
    bots = server_brp.query(with_=[tactical_brp.MissionEnemy])
    assert len(bots) == 1, f"expected exactly 1 bot in the fixture, found {bots}"
    return bots[0]["entity"]


def _client_brp_reachable(client_brp: tactical_brp.BrpClient) -> bool:
    try:
        client_brp.call("rpc.discover")
        return True
    except tactical_brp.BrpError:
        return False


def _face_bot(client_brp: tactical_brp.BrpClient) -> None:
    """Rotates the local player's follow camera to face the bot.

    `PlayerInputOverride.look` only substitutes what gets sent to the
    *server* (see `send_player_input` in
    `adventuresim-tactical-netcode/src/client.rs`) - it never touches the
    client's own `CharacterLook`, which is instead derived every frame
    *from* the camera's current rotation (`copy_camera_to_character_look` in
    `bevy_ahoy`). The attack raycast (`update_attack_state_system` in
    `adventuresim-tactical-client/src/player.rs`) reads the camera's
    transform directly, so the camera itself is what needs to move; setting
    its rotation directly over BRP sticks (verified live), since
    `copy_character_look_to_camera` just re-derives the same rotation from
    the `CharacterLook` that was itself just derived from this new rotation.
    """
    cameras = client_brp.query(with_=[tactical_brp.Camera3d])
    assert len(cameras) == 1, f"expected exactly 1 local camera, found {cameras}"
    camera_entity = cameras[0]["entity"]
    transform = client_brp.get_components(camera_entity, [tactical_brp.Transform])[tactical_brp.Transform]
    half_yaw = CAMERA_YAW_TO_FACE_BOT / 2.0
    transform.rotation = [0.0, math.sin(half_yaw), 0.0, math.cos(half_yaw)]
    client_brp.call("world.insert_components", {"entity": camera_entity, "components": {tactical_brp.Transform.type_path: transform.to_brp()}})


def _move_bot_within_melee_range(server_brp: tactical_brp.BrpClient, bot_entity: int) -> None:
    transform = server_brp.get_components(bot_entity, [tactical_brp.Transform])[tactical_brp.Transform]
    transform.translation = [BOT_X_WITHIN_MELEE_RANGE, transform.translation[1], transform.translation[2]]
    server_brp.call("world.insert_components", {"entity": bot_entity, "components": {tactical_brp.Transform.type_path: transform.to_brp()}})


def _toughen_bot(server_brp: tactical_brp.BrpClient, bot_entity: int) -> None:
    """Raises every limb's health to `BOT_LIMB_HEALTH` and resets
    `TacticalCombatState` to a fresh, non-incapacitated baseline.

    Proving a dodge/parry works reliably needs several retried attacks (see
    `_run_scripted_attack`'s docstring on the reaction-delay timing race),
    and `validate_melee_intent_cheap` rejects every further attempt once the
    target is incapacitated - silently: no further "hit"/"Rejected" log
    lines at all once that happens (confirmed live), which is what a plain
    "no resolution" failure looks like from this test's side too. An
    undefended full-roll hit punches ~50 damage through the bot's padded
    armor (~92 J of force against ~42 effective chest resistance) - this
    isn't from anything this test boosts on the attacker's side, the
    character's own trained combat skill and the weapon's base force
    already clamp the attack roll to maximum. Limb health alone isn't
    enough to survive that: `blood_loss_fraction` accumulates independently
    of it (see `apply_transient_attack_result` in `combat/consequence.rs`)
    and saturates to 1.0 after a single hit regardless of how much health
    is left, which contributes to incapacitation on its own - confirmed
    live: raising only limb health still stopped every attempt after the
    first landed hit. Called before every attempt (not just once) so it
    also undoes whatever landed *this* attempt for the next retry.
    """
    limbs = server_brp.get_components(bot_entity, [tactical_brp.Limbs])[tactical_brp.Limbs]
    for field in ("left_arm", "right_arm", "left_leg", "right_leg", "chest", "stomach", "head"):
        setattr(limbs, field, BOT_LIMB_HEALTH)
    server_brp.call("world.insert_components", {"entity": bot_entity, "components": {tactical_brp.Limbs.type_path: limbs.to_brp()}})
    server_brp.call(
        "world.insert_components",
        {
            "entity": bot_entity,
            "components": {
                tactical_brp.TacticalCombatState.type_path: tactical_brp.TacticalCombatState(
                    starting_incapacitation=0.0, starting_blood_fraction=1.0, blood_loss_fraction=0.0, imbalance=0.0, incapacitation=0.0
                ).to_brp()
            },
        },
    )


def _disarm_bot(server_brp: tactical_brp.BrpClient, bot_entity: int) -> None:
    """Removes `EquipSlot` from the bot's own weapon so it can't independently
    attack the party member back.

    Repositioning the bot within melee range (see `BOT_X_WITHIN_MELEE_RANGE`)
    also puts it within its *own* `OffensiveCombatAi`'s engagement range, and
    that AI re-acquires the party member as a target every tick regardless
    of the dump's captured (and by-then-stale) target - it isn't gated on
    `DefenseChances` at all, only on having a melee-capable weapon (see
    `drive_offensive_combat_ai` in `bot/offense.rs`). Removing
    `OffensiveCombatAi` entirely would've been simpler, but the defense
    reactions this test is actually exercising are *also* gated on
    `With<OffensiveCombatAi>` (see `on_targeted_attack_started` in
    `bot/defense.rs`) - disarming leaves defense intact while making offense
    impossible (`weapon_is_melee()`/`weapon_reach()` both come from the same
    `equipped_weapon()` lookup `EquipSlot` drives).
    """
    weapon_items = server_brp.query(components=[tactical_brp.ItemOf], with_=[tactical_brp.WeaponItem, tactical_brp.EquipSlot])
    for entry in weapon_items:
        item_of = tactical_brp.ItemOf.from_brp(entry["components"][tactical_brp.ItemOf.type_path])
        if item_of.value != bot_entity:
            continue
        server_brp.call("world.remove_components", {"entity": entry["entity"], "components": [tactical_brp.EquipSlot.type_path]})


def _mock_attack(client_brp: tactical_brp.BrpClient, player_entity: int) -> None:
    """Inserts `ActionMock` on the client's Attack action entity, exactly as
    `bevy_enhanced_input::action::mock` documents for automated testing -
    the framework processes it for real (`Fire<Attack>` fires, drives the
    same code a mouse click would), it just skips reading the actual input
    device.
    """
    actions = client_brp.get_components(player_entity, [tactical_brp.ActionsPlayer])[tactical_brp.ActionsPlayer]
    attack_entity = actions.entities[ATTACK_ACTION_INDEX]
    mock = tactical_brp.ActionMock(state="Fired", value={"Axis1D": 1.0}, span={"Updates": 3}, enabled=True)
    client_brp.call("world.insert_components", {"entity": attack_entity, "components": {tactical_brp.ActionMock.type_path: mock.to_brp()}})


class _AttackOutcome:
    def __init__(self, bot_health_before: float, bot_health_after: float, resolved: bool):
        self.bot_health_before = bot_health_before
        self.bot_health_after = bot_health_after
        self.resolved = resolved


def _resolved_since(server: SpawnedProcess, offset: int) -> bool:
    # `resolve_melee_attack` (combat/melee.rs) logs unconditionally on
    # *both* branches of the outcome - "{attacker} hit {target} on {part}
    # for {damage} damage (...) and {N} balance damage" when it connects,
    # "{attacker} failed to hit {target} on {part} and receiver {N} balance
    # damage" when the defender fully evades it (a successful dodge reports
    # 0.0 damage and 0.0 balance damage on either branch, so the bot's own
    # state genuinely never changes - the log line is the only signal that
    # distinguishes "the attack landed but did nothing" from "the attack
    # never reached the server at all"). Sliced to content appended *since*
    # `offset` - the substring persists in the full log forever once it
    # first appears, so checking the whole log on every retry would make
    # this true forever after the first resolved attempt.
    return "balance damage" in server.log_text()[offset:]


def _run_scripted_attack(
    tmp_path_factory: pytest.TempPathFactory, log_name: str, chances: tactical_brp.DefenseChances, retry_while_damaged: bool
) -> _AttackOutcome:
    """Boots the fixture standalone, sets the bot's `DefenseChances`, mocks
    melee attacks against it until one resolves, and reports what happened
    to the bot's health (see `_AttackOutcome`).

    `retry_while_damaged` controls what counts as "done" versus "worth
    retrying": a bot's defensive reaction is a *two-stage* random process,
    not one - `roll_defend_choice` (bot/defense.rs) rolls whether it reacts
    at all, but a reaction that does trigger then commits after its own
    independent random delay (`REACTION_DELAY_SECS`, see
    `try_start_reaction`), *separate* from `DefenseChances`. That delay is
    tuned to land just before the attack's own resolution (~300ms after
    `Start`, the weapon's `windup_secs`), but both are real-time timers with
    their own jitter, so a rolled reaction can still occasionally miss the
    impact and the attack lands as if undefended.
    `retry_while_damaged=True` (used for asserting a defense actually *can*
    prevent damage) treats a damaged resolution as "that attempt lost the
    unrelated timing race, try again" rather than a final answer; `False`
    (used for asserting an undefended hit *does* land) stops at the first
    resolution, damaged or not.
    """
    log_dir = tmp_path_factory.mktemp(log_name)
    game_port, server_brp_port, client_brp_port = COMBAT_PORTS[log_name]

    server_env = {
        **os.environ,
        "TACTICAL_BRP_PORT": str(server_brp_port),
        "TACTICAL_WORLD_DUMP": str(FIXTURE),
        "TACTICAL_PORT": game_port,
        # Required by clap but genuinely unused in standalone/world-dump mode
        # (no SpacetimeDB connection is ever made to check it against).
        "ADVENTURESIM_TACTICAL_CLAIM": "standalone-test",
    }
    server = spawn(["just", "tactical"], log_dir / "server.log", env=server_env)
    try:
        wait_for(server, SERVER_TIMEOUT, ready=lambda: "Server opened on" in server.log_text(), what="the tactical server to open")
        server_brp = tactical_brp.BrpClient(server_brp_port)
        bot_entity = _find_bot(server_brp)

        server_brp.call("world.insert_components", {"entity": bot_entity, "components": {tactical_brp.DefenseChances.type_path: chances.to_brp()}})
        _move_bot_within_melee_range(server_brp, bot_entity)
        # The bot keeps its fixture armor deliberately: a dodge alone only
        # *reduces* the attack roll, but the reduced force then lands below
        # the padded armor's resistance/padding thresholds and deals exactly
        # zero damage, while an undefended full roll still punches ~50
        # through. Dodge-plus-armor preventing damage (and dodge alone
        # merely mitigating) is the intended combat design, and it's what
        # makes the two scenarios distinguishable by health at all.
        _toughen_bot(server_brp, bot_entity)
        _disarm_bot(server_brp, bot_entity)
        bot_before = server_brp.get_components(bot_entity, [tactical_brp.Limbs])[tactical_brp.Limbs]

        client_env = {**os.environ, "TACTICAL_CLIENT_BRP_PORT": str(client_brp_port), "TACTICAL_PORT": game_port}
        client = spawn(["just", "client-headless"], log_dir / "client.log", env=client_env)
        try:
            client_brp = tactical_brp.BrpClient(client_brp_port)
            wait_for(client, CLIENT_TIMEOUT, ready=lambda: _client_brp_reachable(client_brp), what="the client BRP endpoint to come up")
            player_entity_client = tactical_brp.wait_for_entity_with_component(client_brp, tactical_brp.ClientPlayer, CLIENT_TIMEOUT)

            _face_bot(client_brp)
            time.sleep(0.3)

            resolved = False
            bot_after = bot_before
            for _attempt in range(ATTACK_ATTEMPTS):
                log_offset = len(server.log_text())
                _mock_attack(client_brp, player_entity_client)

                # A single attempt (windup + resolution) takes well under a
                # second; this is a per-attempt budget, not the overall test
                # timeout, so a rejected/no-op attempt doesn't eat into the
                # next retry's own window. Polled quickly (not just once at
                # the end) because an undefended hit needs a fast reaction
                # below - see the `_toughen_bot` call right after.
                deadline = time.monotonic() + ATTACK_RESOLUTION_TIMEOUT / ATTACK_ATTEMPTS
                resolved = False
                while time.monotonic() < deadline:
                    time.sleep(0.05)
                    resolved = _resolved_since(server, log_offset)
                    if resolved:
                        break
                if not resolved:
                    continue

                bot_after = server_brp.get_components(bot_entity, [tactical_brp.Limbs])[tactical_brp.Limbs]
                # Restore the bot to full health/combat-readiness right
                # away, not at the top of the next loop iteration ~0.6s from
                # now: a landed undefended hit crosses this fixture's
                # incapacitation threshold from blood loss alone (see
                # `_toughen_bot`'s docstring), and the resulting despawn's
                # grace period is only 0.3s - waiting for the next iteration
                # loses the race and leaves nothing left to retry against
                # (confirmed live: subsequent BRP calls against the bot
                # started failing with "Entity ... not found").
                _toughen_bot(server_brp, bot_entity)
                if retry_while_damaged and bot_after != bot_before:
                    continue
                break
        finally:
            client.terminate()
    finally:
        server.terminate()

    return _AttackOutcome(_total_limb_health(bot_before), _total_limb_health(bot_after), resolved)


def test_bot_takes_no_damage_when_defense_chances_force_a_dodge(tmp_path_factory: pytest.TempPathFactory) -> None:
    outcome = _run_scripted_attack(
        tmp_path_factory, "combat-dodge", tactical_brp.DefenseChances(parry_chance=0.0, dodge_chance=1.0), retry_while_damaged=True
    )
    assert outcome.resolved, "attack never reached the server at all - can't tell a dodge from a no-op"
    assert outcome.bot_health_after == outcome.bot_health_before, (
        f"bot with a forced 100% dodge chance still lost health: {outcome.bot_health_before} -> {outcome.bot_health_after}"
    )


def test_bot_takes_damage_when_defense_chances_force_no_reaction(tmp_path_factory: pytest.TempPathFactory) -> None:
    outcome = _run_scripted_attack(
        tmp_path_factory, "combat-no-defense", tactical_brp.DefenseChances(parry_chance=0.0, dodge_chance=0.0), retry_while_damaged=False
    )
    assert outcome.bot_health_after < outcome.bot_health_before, (
        f"bot with a forced 0% defense chance should have taken damage: {outcome.bot_health_before} -> {outcome.bot_health_after}"
    )
