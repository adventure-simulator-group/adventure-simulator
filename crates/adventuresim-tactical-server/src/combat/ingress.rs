use super::*;

pub(super) fn on_defender_response_request(
    event: On<FromClient<DefendRequest>>,
    mut cmd: Commands,
) {
    let Some(defender) = event.client_id.entity() else {
        warn!(
            "Got defender response from an unknown client: {:?}",
            event.client_id
        );
        return;
    };
    cmd.trigger(DefendIntent {
        defender,
        choice: **event,
    });
}

pub(super) fn apply_defend_intent(
    event: On<DefendIntent>,
    mut cmd: Commands,
    time: Res<Time<()>>,
    states: Query<&TacticalCombatState>,
    mut skeletons: Query<&mut SkeletonState>,
) {
    let Ok(combat_state) = states.get(event.defender) else {
        return;
    };
    if combat_state.is_incapacitated() {
        return;
    }

    let Ok(mut skeleton) = skeletons.get_mut(event.defender) else {
        return;
    };
    let start = animation_tick(&time);
    let accepted = match event.choice {
        DefendRequest::Dodge if skeleton.action_kind() == SkeletonAction::Dodge => true,
        DefendRequest::Dodge => skeleton
            .begin_dodge(DodgeSpec::default(), start, start + 8)
            .is_ok(),
        DefendRequest::Roll if !accepts_roll_dodge(&skeleton) => return,
        DefendRequest::Roll => true,
        DefendRequest::Parry => skeleton
            .begin_block(BlockSpec::default(), start, start + 8)
            .is_ok(),
    };
    if !accepted {
        return;
    }

    cmd.entity(event.defender).insert(PendingDefenderResponse {
        choice: event.choice,
        set_at: CombatInstant::from_elapsed(&time),
    });
}

pub(super) fn on_melee_attack_started(
    event: On<MeleeAttackStartedIntent>,
    mut authorities: Query<&mut MeleeAttackAuthority>,
    mut skeletons: Query<&mut SkeletonState>,
    time: Res<Time<()>>,
) {
    let Ok(mut skeleton) = skeletons.get_mut(event.attacker) else {
        return;
    };
    let Some(strike_family) = skeleton.available_strike_family(event.strike_family) else {
        return;
    };
    let Some(spec) = (match event.hand {
        AttackHand::Main => skeleton.select_main_attack(strike_family),
        AttackHand::Offhand => skeleton.select_offhand_attack(strike_family),
    }) else {
        return;
    };
    let Ok(mut authority) = authorities.get_mut(event.attacker) else {
        return;
    };
    let start = animation_tick(&time);
    if skeleton
        .begin_attack(spec, start, start + duration_ticks(event.windup))
        .is_err()
    {
        return;
    }
    authority.observe(
        Some(event.target),
        CombatInstant::from_elapsed(&time),
        event.windup,
        MELEE_WINDUP_NETWORK_ALLOWANCE,
    );
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
        DefendRequest::Roll => DefenderResponse::Dodge {
            input_reflex: roll_dodge_reflex(input_reflex),
        },
        DefendRequest::Parry => {
            if defender_view.shield_block_bonus() > 0.0 {
                DefenderResponse::Parry { input_reflex }
            } else {
                DefenderResponse::None
            }
        }
    }
}

const ROLL_DODGE_EFFECTIVENESS: f32 = 0.35;

fn roll_dodge_reflex(input_reflex: f32) -> f32 {
    input_reflex.clamp(0.0, 1.0) * ROLL_DODGE_EFFECTIVENESS
}

fn accepts_roll_dodge(skeleton: &SkeletonState) -> bool {
    skeleton.body().is_downed()
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod roll_tests {
    use super::*;

    #[test]
    fn roll_is_a_bounded_fraction_of_an_ordinary_dodge() {
        assert!((roll_dodge_reflex(1.0) - 0.35).abs() < f32::EPSILON);
        assert_eq!(roll_dodge_reflex(-1.0), 0.0);
        assert_eq!(roll_dodge_reflex(2.0), 0.35);
    }

    #[test]
    fn roll_defense_is_restricted_to_prone_and_supine() {
        assert!(!accepts_roll_dodge(&SkeletonState::default()));
        assert!(accepts_roll_dodge(
            &SkeletonState::default().with_body_state(BodyState::Prone)
        ));
        assert!(accepts_roll_dodge(
            &SkeletonState::default().with_body_state(BodyState::Supine)
        ));
    }

    #[test]
    fn packet_tolerance_does_not_shorten_attack_animation_timing() {
        let authored = CombatDuration::from_duration(Duration::from_millis(300));
        let (animation_windup, minimum_windup) = player_attack_windups(authored);

        assert_eq!(duration_ticks(animation_windup), 19);
        assert_eq!(duration_ticks(minimum_windup), 16);
    }

    #[test]
    fn clients_and_server_ai_share_authoritative_defense_transitions() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .add_observer(on_defender_response_request)
            .add_observer(apply_defend_intent);
        let player = app
            .world_mut()
            .spawn((TacticalCombatState::default(), SkeletonState::default()))
            .id();
        let bot = app
            .world_mut()
            .spawn((TacticalCombatState::default(), SkeletonState::default()))
            .id();

        app.world_mut().trigger(FromClient {
            client_id: adventuresim_tactical_netcode::bevy_replicon::prelude::ClientId::Client(
                player,
            ),
            message: DefendRequest::Parry,
        });
        app.world_mut().trigger(DefendIntent {
            defender: bot,
            choice: DefendRequest::Dodge,
        });
        app.world_mut().flush();

        assert_eq!(
            app.world().get::<SkeletonState>(player).unwrap().action_kind(),
            SkeletonAction::Block
        );
        assert_eq!(
            app.world().get::<SkeletonState>(bot).unwrap().action_kind(),
            SkeletonAction::Dodge
        );
        assert!(matches!(
            app.world().get::<PendingDefenderResponse>(player),
            Some(PendingDefenderResponse {
                choice: DefendRequest::Parry,
                ..
            })
        ));
        assert!(matches!(
            app.world().get::<PendingDefenderResponse>(bot),
            Some(PendingDefenderResponse {
                choice: DefendRequest::Dodge,
                ..
            })
        ));
    }
}

fn player_attack_windups(authored: CombatDuration) -> (CombatDuration, CombatDuration) {
    (authored, authored.saturating_sub(WINDUP_JITTER_TOLERANCE))
}

pub(super) fn on_melee_action_request(
    event: On<FromClient<MeleeActionRequest>>,
    mut cmd: Commands,
    time: Res<Time<()>>,
    viewer: TacticalPlayerViewer,
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
        MeleeActionRequest::Start {
            strike_family,
            hand,
        } => {
            let Ok(mut skeleton) = skeletons.get_mut(attacker) else {
                return;
            };
            let Some(strike_family) = skeleton.available_strike_family(strike_family) else {
                return;
            };
            let Some(spec) = (match hand {
                AttackHand::Main => skeleton.select_main_attack(strike_family),
                AttackHand::Offhand => skeleton.select_offhand_attack(strike_family),
            }) else {
                return;
            };
            let Ok(mut authority) = authorities.get_mut(attacker) else {
                return;
            };
            // The same authored per-weapon value the client paces its own
            // swing by, minus a delivery-jitter tolerance - see
            // `WINDUP_JITTER_TOLERANCE`. Unarmed attackers use the shared
            // authored hands timing; a genuinely viewless attacker still
            // falls back to zero and is rejected by later weapon checks.
            let authored_windup = viewer
                .get_for_attack(attacker, hand)
                .map(|view| CombatDuration::from_secs_f32(view.weapon_windup_secs()))
                .unwrap_or_default();
            let (animation_windup, minimum_windup) = player_attack_windups(authored_windup);
            let start = animation_tick(&time);
            if skeleton
                .begin_attack(spec, start, start + duration_ticks(animation_windup))
                .is_err()
            {
                return;
            }
            authority.observe(
                None,
                CombatInstant::from_elapsed(&time),
                minimum_windup,
                MELEE_WINDUP_NETWORK_ALLOWANCE,
            );
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
            let (strike_family, hand) = skeletons
                .get_mut(attacker)
                .map(|skeleton| (skeleton.strike_family(), skeleton.attack_hand()))
                .unwrap_or((StrikeFamily::Thrust, AttackHand::Main));
            cmd.trigger(MeleeAttackIntent {
                attacker,
                target,
                body_part,
                reported_precision,
                strike_family,
                hand,
            });
        }
    }
}

pub(super) fn on_ranged_action_request(
    event: On<FromClient<RangedActionRequest>>,
    mut cmd: Commands,
    viewer: TacticalPlayerViewer,
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
            // Same per-weapon windup + jitter-tolerance treatment as the
            // melee path - see `on_melee_action_request`.
            let authored_windup = viewer
                .get(attacker)
                .map(|view| CombatDuration::from_secs_f32(view.weapon_windup_secs()))
                .unwrap_or_default();
            let (animation_windup, minimum_windup) = player_attack_windups(authored_windup);
            cmd.trigger(RangedAttackStartedIntent {
                attacker,
                target: None,
                animation_windup,
                minimum_windup,
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
    let Ok(mut skeleton) = skeletons.get_mut(event.attacker) else {
        return;
    };
    let start = animation_tick(&time);
    if skeleton
        .begin_attack(
            AttackSpec::default(),
            start,
            start + duration_ticks(event.animation_windup),
        )
        .is_err()
    {
        return;
    }
    authority.observe(
        CombatInstant::from_elapsed(&time),
        event.minimum_windup,
        RANGED_NETWORK_ALLOWANCE,
    );
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
    scene_items: &Query<Entity, With<TacticalSceneItem>>,
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
    let excluded: Vec<_> = scene_items.iter().chain([attacker]).collect();
    let filter = SpatialQueryFilter::from_excluded_entities(excluded);
    spatial
        .cast_ray(origin, direction, distance, true, &filter)
        .is_some_and(|hit| hit.entity == target)
}
