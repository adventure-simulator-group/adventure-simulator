use super::*;

pub(super) fn on_defender_response(
    event: On<FromClient<DefendRequest>>,
    mut cmd: Commands,
    time: Res<Time<()>>,
    states: Query<&TacticalCombatState>,
    mut skeletons: Query<&mut SkeletonState>,
) {
    let Some(entity) = event.client_id.entity() else {
        warn!(
            "Got defender response from an unknown client: {:?}",
            event.client_id
        );
        return;
    };

    let Ok(combat_state) = states.get(entity) else {
        return;
    };
    if combat_state.is_incapacitated() {
        return;
    }

    if let Ok(mut skeleton) = skeletons.get_mut(entity) {
        let start = animation_tick(&time);
        match **event {
            DefendRequest::Dodge => skeleton.begin_dodge(DodgeSpec::default(), start, start + 8),
            DefendRequest::Parry => skeleton.begin_block(BlockSpec::default(), start, start + 8),
        }
    }

    cmd.entity(entity).insert(PendingDefenderResponse {
        choice: **event,
        set_at: CombatInstant::from_elapsed(&time),
    });
}

pub(super) fn on_melee_attack_started(
    event: On<MeleeAttackStartedIntent>,
    mut authorities: Query<&mut MeleeAttackAuthority>,
    mut skeletons: Query<&mut SkeletonState>,
    time: Res<Time<()>>,
) {
    let Ok(mut authority) = authorities.get_mut(event.attacker) else {
        return;
    };
    authority.observe(
        Some(event.target),
        CombatInstant::from_elapsed(&time),
        event.windup,
        MELEE_WINDUP_NETWORK_ALLOWANCE,
    );
    if let Ok(mut skeleton) = skeletons.get_mut(event.attacker) {
        let start = animation_tick(&time);
        let attack = AttackSpec::melee_from_local_velocity(skeleton.local_velocity);
        skeleton.begin_attack(attack, start, start + duration_ticks(event.windup));
    }
}

pub(super) fn resolve_defender_response(
    pending: Option<&PendingDefenderResponse>,
    time: &Time<()>,
    defender_view: &TacticalPlayerView,
) -> DefenderResponse {
    let Some(pending) = pending else {
        return DefenderResponse::None;
    };

    let elapsed = CombatInstant::from_elapsed(time).elapsed_since(pending.set_at);
    if elapsed > MAX_REFLEX_WINDOW {
        return DefenderResponse::None;
    }

    let input_reflex =
        (1.0 - elapsed.as_secs_f32() / MAX_REFLEX_WINDOW.as_secs_f32()).clamp(0.0, 1.0);

    match pending.choice {
        DefendRequest::Dodge => DefenderResponse::Dodge { input_reflex },
        DefendRequest::Parry => {
            if defender_view.shield_block_bonus() > 0.0 {
                DefenderResponse::Parry { input_reflex }
            } else {
                DefenderResponse::None
            }
        }
    }
}

pub(super) fn on_melee_action_request(
    event: On<FromClient<MeleeActionRequest>>,
    mut cmd: Commands,
    time: Res<Time<()>>,
    mut authorities: Query<&mut MeleeAttackAuthority>,
    mut skeletons: Query<&mut SkeletonState>,
) {
    let Some(attacker) = event.client_id.entity() else {
        debug!(
            "Ignoring melee action from unknown client: {:?}",
            event.client_id
        );
        return;
    };
    match **event {
        MeleeActionRequest::Start => {
            let Ok(mut authority) = authorities.get_mut(attacker) else {
                return;
            };
            authority.observe(
                None,
                CombatInstant::from_elapsed(&time),
                CLIENT_MELEE_WINDUP,
                MELEE_WINDUP_NETWORK_ALLOWANCE,
            );
            if let Ok(mut skeleton) = skeletons.get_mut(attacker) {
                let start = animation_tick(&time);
                let attack = AttackSpec::melee_from_local_velocity(skeleton.local_velocity);
                skeleton.begin_attack(attack, start, start + duration_ticks(CLIENT_MELEE_WINDUP));
            }
        }
        MeleeActionRequest::Complete {
            target,
            body_part,
            reported_precision,
        } => {
            let Some(reported_precision) = ReportedPrecision::new(reported_precision) else {
                debug!("Ignoring non-finite melee precision from {attacker:?}");
                return;
            };
            // Finite precision is intentionally accepted as reported. Full
            // animation and secondary physics remain client-owned.
            cmd.trigger(MeleeAttackIntent {
                attacker,
                target,
                body_part,
                reported_precision,
            });
        }
    }
}

pub(super) fn on_ranged_action_request(
    event: On<FromClient<RangedActionRequest>>,
    mut cmd: Commands,
    time: Res<Time<()>>,
    mut skeletons: Query<&mut SkeletonState>,
) {
    let Some(attacker) = event.client_id.entity() else {
        debug!(
            "Ignoring ranged action from unknown client: {:?}",
            event.client_id
        );
        return;
    };
    match **event {
        RangedActionRequest::Start => {
            if let Ok(mut skeleton) = skeletons.get_mut(attacker) {
                let start = animation_tick(&time);
                skeleton.begin_attack(
                    AttackSpec::default(),
                    start,
                    start + duration_ticks(CLIENT_RANGED_WINDUP),
                );
            }
            cmd.trigger(RangedAttackStartedIntent {
                attacker,
                target: None,
                windup: CLIENT_RANGED_WINDUP,
            });
        }
        RangedActionRequest::CompleteMiss => {
            cmd.trigger(RangedAttackIntent {
                attacker,
                target: None,
                body_part: BodyPart::Chest,
                reported_precision: ReportedPrecision::new(0.0).expect("zero is finite"),
            });
        }
        RangedActionRequest::CompleteHit {
            target,
            body_part,
            reported_precision,
        } => {
            let Some(reported_precision) = ReportedPrecision::new(reported_precision) else {
                debug!("Ignoring non-finite ranged precision from {attacker:?}");
                return;
            };
            // Finite precision is deliberately trusted. Animation and
            // secondary physics remain client-owned and non-deterministic.
            cmd.trigger(RangedAttackIntent {
                attacker,
                target: Some(target),
                body_part,
                reported_precision,
            });
        }
    }
}

pub(super) fn on_ranged_attack_started(
    event: On<RangedAttackStartedIntent>,
    mut authorities: Query<&mut RangedAttackAuthority>,
    mut skeletons: Query<&mut SkeletonState>,
    time: Res<Time<()>>,
) {
    let Ok(mut authority) = authorities.get_mut(event.attacker) else {
        return;
    };
    authority.observe(
        CombatInstant::from_elapsed(&time),
        event.windup,
        RANGED_NETWORK_ALLOWANCE,
    );
    if let Ok(mut skeleton) = skeletons.get_mut(event.attacker) {
        let start = animation_tick(&time);
        skeleton.begin_attack(
            AttackSpec::default(),
            start,
            start + duration_ticks(event.windup),
        );
    }
}

fn animation_tick(time: &Time<()>) -> u64 {
    (time.elapsed_secs_f64() * LOCOMOTION_SAMPLE_HZ as f64).round() as u64
}

fn duration_ticks(duration: CombatDuration) -> u64 {
    (duration.as_secs_f32() * LOCOMOTION_SAMPLE_HZ)
        .round()
        .max(1.0) as u64
}

pub(super) fn authoritative_line_of_sight(
    spatial: &SpatialQuery,
    attacker: Entity,
    target: Entity,
    origin: Vec3,
    target_position: Vec3,
) -> bool {
    let offset = target_position - origin;
    let distance = offset.length();
    let Ok(direction) = Dir3::new(offset) else {
        return false;
    };
    let filter = SpatialQueryFilter::from_excluded_entities([attacker]);
    spatial
        .cast_ray(origin, direction, distance, true, &filter)
        .is_some_and(|hit| hit.entity == target)
}
