use super::*;

#[expect(
    clippy::too_many_arguments,
    reason = "active defense bridges live authority, animation, and physiology"
)]
pub(super) fn resolve_active_defense(
    attack: &AuthorizedMeleeAttack,
    attacker_view: &TacticalPlayerView<'_, '_, '_>,
    defender_view: &TacticalPlayerView<'_, '_, '_>,
    attacker_performance: f32,
    attack_style: MeleeAttackStyle,
    contact_sample: f32,
    q_pending: &Query<&PendingDefenderResponse>,
    q_authorities: &mut Query<&mut MeleeAttackAuthority>,
    q_skeletons: &mut Query<&mut SkeletonState>,
    q_states: &mut Query<&mut TacticalCombatState>,
    time: &Time<()>,
    config: &TacticalCombatConfig,
) -> Option<DefenderResponse> {
    let defender_skeleton = q_skeletons.get(attack.target()).ok()?;
    let pending = q_pending.get(attack.target()).ok();
    let (incapacitation, performance) = defender_condition(attack, q_states);
    let response = {
        let mut authority = q_authorities.get_mut(attack.target()).ok();
        resolve_melee_defender_response(
            pending,
            time,
            defender_view,
            defender_skeleton,
            authority.as_deref_mut(),
            incapacitation,
            performance,
            attack.attacker(),
            attack.started_at(),
            &config.realtime_authority.defense,
        )
    }
    .scaled_for_performance(performance);
    let attacker_side = attacker_view.weapon_holding_side()?;
    let preview = attacker_view.melee_contact_location(
        attacker_side,
        attack_style,
        defender_view,
        attack.reported_precision().get() * attacker_performance,
        contact_sample,
    );
    let response = shield_aligned_response(response, defender_view.shield_holding_side(), preview);
    charge_defense_work(response, attack, defender_view, q_states, config);
    Some(response)
}

fn defender_condition(
    attack: &AuthorizedMeleeAttack,
    states: &Query<&mut TacticalCombatState>,
) -> (f32, f32) {
    states.get(attack.target()).map_or((0.0, 1.0), |state| {
        (
            state.incapacitation,
            combat_incapacitation_performance(state.incapacitation),
        )
    })
}

fn charge_defense_work(
    response: DefenderResponse,
    attack: &AuthorizedMeleeAttack,
    defender: &TacticalPlayerView<'_, '_, '_>,
    states: &mut Query<&mut TacticalCombatState>,
    config: &TacticalCombatConfig,
) {
    if !matches!(
        response,
        DefenderResponse::Block { .. } | DefenderResponse::Parry { .. }
    ) {
        return;
    }
    let Ok(mut state) = states.get_mut(attack.target()) else {
        return;
    };
    let state = &mut *state;
    let endurance = defender.raw_single_body_part_attr(SimpleAttribute::Endurance);
    let workload = combat_action_workload(
        CombatActionWork::WeaponDefense,
        config.realtime_authority.defense.reflex_window_seconds,
        defender.weapon_weight(),
        defender.weapon_moment_of_inertia(),
        defender.inventory_weight(),
        defender.body_weight(),
        config.resolution.fatigue,
    );
    state.charge_work(workload, endurance, config.resolution.fatigue);
}

#[expect(
    clippy::too_many_arguments,
    reason = "committed defense bridges attack authority, animation, and implement ownership"
)]
pub(super) fn commit_defense_during_attack(
    cmd: &mut Commands,
    incoming: &AuthorizedMeleeAttack,
    attempted: DefenderResponse,
    effective: DefenderResponse,
    engagement: f32,
    defender_view: &TacticalPlayerView<'_, '_, '_>,
    authorities: &mut Query<&mut MeleeAttackAuthority>,
    skeletons: &mut Query<&mut SkeletonState>,
) {
    if matches!(attempted, DefenderResponse::Parry { .. })
        && let Ok(mut authority) = authorities.get_mut(incoming.target())
        && let Some(canceled_attack_key) = authority.commit_attack_to_defense()
    {
        if let Ok(mut skeleton) = skeletons.get_mut(incoming.target()) {
            skeleton.commit_attack_to_defense();
        }
        cmd.entity(incoming.target())
            .remove::<PendingMeleeContact>()
            .remove::<MeleeLungeMovement>();
        cmd.trigger(MeleeAttackCommittedToDefense {
            defender: incoming.target(),
            incoming_attacker: incoming.attacker(),
            canceled_attack_key,
            response: attempted,
            engagement,
        });
        return;
    }
    if defender_view.shield_block_bonus() > 0.0
        && let DefenderResponse::Block { effectiveness } = effective
        && let Ok(mut authority) = authorities.get_mut(incoming.target())
        && let Some((attack_key, retained_power)) =
            authority.transform_attack_for_offhand_defense(effectiveness)
    {
        cmd.trigger(MeleeAttackTransformedByDefense {
            defender: incoming.target(),
            incoming_attacker: incoming.attacker(),
            attack_key,
            retained_power,
            response: attempted,
            engagement,
        });
    }
}
