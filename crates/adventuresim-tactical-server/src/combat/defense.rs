use super::*;

fn resolve_requested_dodge(
    pending: Option<&PendingDefenderResponse>,
    time: &Time<()>,
    config: &DefenseAuthorityConfig,
) -> Option<DefenderResponse> {
    let pending = pending?;
    let elapsed = CombatInstant::from_elapsed(time).elapsed_since(pending.set_at);
    let reflex_window = std::time::Duration::from_secs_f32(config.reflex_window_seconds);
    if elapsed > reflex_window {
        return None;
    }
    let input_reflex = (1.0 - elapsed.as_secs_f32() / reflex_window.as_secs_f32()).clamp(0.0, 1.0);
    Some(match pending.choice {
        DefendRequest::Dodge { .. } => DefenderResponse::Dodge { input_reflex },
        DefendRequest::Roll => DefenderResponse::Dodge {
            input_reflex: roll_dodge_reflex(input_reflex, config.roll_dodge_effectiveness),
        },
    })
}

pub(super) fn resolve_passive_block(
    pending: Option<&PendingDefenderResponse>,
    time: &Time<()>,
    defender_view: &TacticalPlayerView,
    defender_skeleton: &SkeletonState,
    config: &DefenseAuthorityConfig,
) -> DefenderResponse {
    if let Some(response) = resolve_requested_dodge(pending, time, config) {
        return response;
    }
    passive_block_response(
        defender_skeleton.weapon_guard(),
        defender_skeleton.action_kind(),
        !defender_view.weapon_is_unarmed() || defender_view.shield_block_bonus() > 0.0,
    )
}

fn passive_block_response(
    guard: WeaponGuardState,
    action: SkeletonAction,
    has_blocking_item: bool,
) -> DefenderResponse {
    if guard == WeaponGuardState::Raised && action == SkeletonAction::None && has_blocking_item {
        DefenderResponse::Block
    } else {
        DefenderResponse::None
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the defense boundary compares both combatants' authoritative attack state"
)]
pub(super) fn resolve_melee_defender_response(
    pending: Option<&PendingDefenderResponse>,
    time: &Time<()>,
    defender_view: &TacticalPlayerView,
    defender_skeleton: &SkeletonState,
    defender_attack: Option<&MeleeAttackAuthority>,
    attacker: Entity,
    incoming_started_at: CombatInstant,
    config: &DefenseAuthorityConfig,
) -> DefenderResponse {
    if let Some(response) = resolve_requested_dodge(pending, time, config) {
        return response;
    }
    let can_intercept =
        !defender_view.weapon_is_unarmed() || defender_view.shield_block_bonus() > 0.0;
    if can_intercept
        && defender_skeleton.action_kind() == SkeletonAction::Attack
        && let Some((input_reflex, precision)) = defender_attack.and_then(|authority| {
            authority.reciprocal_parry(
                attacker,
                incoming_started_at,
                CombatDuration::from_secs_f32(config.reflex_window_seconds),
            )
        })
    {
        return DefenderResponse::Parry {
            input_reflex,
            precision: precision.get(),
        };
    }
    resolve_passive_block(pending, time, defender_view, defender_skeleton, config)
}

pub(super) fn roll_dodge_reflex(input_reflex: f32, effectiveness: f32) -> f32 {
    input_reflex.clamp(0.0, 1.0) * effectiveness
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raised_item_blocks_only_during_neutral_action_state() {
        assert_eq!(
            passive_block_response(WeaponGuardState::Raised, SkeletonAction::None, true),
            DefenderResponse::Block
        );
        for (guard, action, has_blocking_item) in [
            (WeaponGuardState::Lowered, SkeletonAction::None, true),
            (WeaponGuardState::Raised, SkeletonAction::Attack, true),
            (WeaponGuardState::Raised, SkeletonAction::Dodge, true),
            (WeaponGuardState::Raised, SkeletonAction::None, false),
        ] {
            assert_eq!(
                passive_block_response(guard, action, has_blocking_item),
                DefenderResponse::None
            );
        }
    }
}
