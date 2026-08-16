use super::*;

const MAX_HIP_DROP_METRES: f32 = 0.18;
const SOLE_CONTACT_MARGIN_METRES: f32 = 0.001;

#[derive(Component, Debug, Clone, Copy, Default)]
pub(in crate::animation) struct OrdinaryLocomotionIkState {
    initialized: bool,
    pelvis_shift: f32,
    evaluation_tick: Option<u64>,
    last_owner_tick: Option<u64>,
    ownership_weight: f32,
    observed_non_ownership: bool,
    left_presented_target: Option<Vec3>,
    right_presented_target: Option<Vec3>,
    had_presented_solve: bool,
    posture_release_progress: Option<f32>,
    posture_release_pelvis_start: f32,
    posture_release_left_start: Option<Vec3>,
    posture_release_right_start: Option<Vec3>,
    left_knee_bend_world: Option<Vec3>,
    right_knee_bend_world: Option<Vec3>,
    left_end_direction: Option<Vec3>,
    right_end_direction: Option<Vec3>,
}

const POSTURE_RELEASE_SECONDS: f32 = 0.125;

fn retained_posture_target(start: Vec3, authored: Vec3, progress: f32) -> Vec3 {
    start.lerp(authored, quintic_progress(progress))
}

fn ordinary_leg_solve_is_owned(weight: f32, posture_release: bool) -> bool {
    posture_release || weight > 0.001
}

fn ordinary_pose_ownership_weight(solve_ownership_weight: f32, posture_release: bool) -> f32 {
    if posture_release {
        1.0
    } else {
        solve_ownership_weight
    }
}

fn posture_release_is_owned(state: OrdinaryLocomotionIkState) -> bool {
    state.initialized
        && state.had_presented_solve
        && state.left_presented_target.is_some()
        && state.right_presented_target.is_some()
}

pub(super) fn owns(skeleton: &SkeletonState) -> bool {
    skeleton.is_grounded()
        && !skeleton.is_posture_transitioning()
        && matches!(skeleton.posture(), Posture::Upright | Posture::Crouched)
        && skeleton.action_kind() == SkeletonAction::None
        && (skeleton.weapon_guard() == WeaponGuardState::Lowered
            || skeleton.guarded_sprint_locomotion())
}

/// Overgrowth-style ordinary locomotion IK: semantic evaluation supplies the
/// complete FK pose and authored foot weights; this pass only conforms weighted ankles
/// to terrain, applies one shared hip correction, and solves each leg once.
pub(in crate::animation) fn apply(
    enabled: Res<super::super::super::TerrainIkEnabled>,
    time: Res<Time>,
    clock: Res<ProceduralAnimationClock>,
    terrain: Query<&SceneTerrain>,
    owners: Query<(&PresentedSkeleton, &AnimationPlayback)>,
    rigs: Query<(Entity, &HumanoidRig)>,
    raised_states: Query<&RaisedFootworkState>,
    parents: Query<&ChildOf>,
    mut states: Query<&mut OrdinaryLocomotionIkState>,
    mut diagnostics: Query<&mut LegIkState>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
    mut commands: Commands,
) {
    let terrain = terrain.single().ok();
    for (owner, rig) in &rigs {
        let Ok((skeleton, playback)) = owners.get(owner) else {
            continue;
        };
        let mut state = states
            .get_mut(owner)
            .map(|state| *state)
            .unwrap_or_default();
        let raised_release_owned = raised_release_owns_ik(skeleton, raised_states.get(owner).ok());
        let ordinary_owns = owns(skeleton);
        let posture_release_owns =
            skeleton.is_posture_transitioning() && posture_release_is_owned(state);
        if (!ordinary_owns && !posture_release_owns) || raised_release_owned {
            if raised_release_owned {
                state.initialized = false;
                state.left_presented_target = None;
                state.right_presented_target = None;
                state.had_presented_solve = false;
                state.posture_release_progress = None;
                state.posture_release_left_start = None;
                state.posture_release_right_start = None;
            }
            state.observed_non_ownership = true;
            state.last_owner_tick = None;
            if let Ok(mut current) = states.get_mut(owner) {
                *current = state;
            } else {
                commands.entity(owner).insert(state);
            }
            continue;
        }
        let owner_tick = skeleton.locomotion_sample_tick;
        let reacquired = ordinary_owns
            && (state.observed_non_ownership
                || state
                    .last_owner_tick
                    .is_some_and(|previous| owner_tick.wrapping_sub(previous) > 1));
        state.observed_non_ownership = false;
        state.last_owner_tick = Some(owner_tick);
        let fixed_step = clock.fixed_step();
        let (delta_seconds, evaluation_advances) = match fixed_step {
            Some((tick, _)) if state.evaluation_tick == Some(tick) => (0.0, false),
            Some((tick, delta)) => {
                state.evaluation_tick = Some(tick);
                (delta.max(0.0), true)
            }
            None => {
                let delta = time.delta_secs().max(0.0);
                (delta, delta > 0.0)
            }
        };
        if repeated_fixed_tick_skips_ik(fixed_step.is_some(), evaluation_advances) {
            // Multi-view capture evaluates one simulation tick repeatedly.
            // The first pass owns both the solve and LegIk diagnostics; a
            // later view starts from restored FK and must not rebuild hidden
            // solver state from that different input chain.
            continue;
        }
        state.ownership_weight = if ordinary_owns {
            advance_ownership_weight(
                state.ownership_weight,
                state.initialized,
                reacquired,
                delta_seconds,
            )
        } else {
            state.ownership_weight
        };
        if reacquired {
            state.pelvis_shift = 0.0;
            state.left_knee_bend_world = None;
            state.right_knee_bend_world = None;
            state.left_end_direction = None;
            state.right_end_direction = None;
        }
        let authored_weights = playback.foot_ik_weights;
        let solve_ownership_weight = smooth_ownership_weight(state.ownership_weight);
        let solve_weights = if posture_release_owns {
            Vec2::ONE
        } else if enabled.0 && terrain.is_some() {
            authored_weights * solve_ownership_weight
        } else {
            Vec2::ZERO
        };

        let legs = [
            (
                BoneRole::ThighLeft,
                BoneRole::ShinLeft,
                BoneRole::FootLeft,
                solve_weights.x,
                true,
            ),
            (
                BoneRole::ThighRight,
                BoneRole::ShinRight,
                BoneRole::FootRight,
                solve_weights.y,
                false,
            ),
        ];
        let mut targets = [None, None];
        let mut authored = [None, None];
        let mut desired_pelvis_shift = 0.0_f32;
        if posture_release_owns && state.posture_release_progress.is_none() {
            state.posture_release_progress = Some(0.0);
            state.posture_release_pelvis_start = state.pelvis_shift;
            state.posture_release_left_start = state.left_presented_target;
            state.posture_release_right_start = state.right_presented_target;
        }
        let posture_release_progress = state.posture_release_progress.unwrap_or(0.0);
        let posture_release_blend = quintic_progress(posture_release_progress);

        for (index, (upper_role, lower_role, foot_role, weight, _)) in
            legs.iter().copied().enumerate()
        {
            let (Some(&upper), Some(&lower), Some(&foot)) = (
                rig.get(&upper_role),
                rig.get(&lower_role),
                rig.get(&foot_role),
            ) else {
                continue;
            };
            let Some((upper_snapshot, lower_snapshot, foot_snapshot)) =
                snapshot_chain(upper, lower, foot, &parents, &transforms.p0())
            else {
                continue;
            };
            let foot_position = foot_snapshot.global.translation();
            authored[index] = Some(foot_position);
            let target = if posture_release_owns {
                let start = if index == 0 {
                    state.posture_release_left_start
                } else {
                    state.posture_release_right_start
                }
                .unwrap_or(foot_position);
                retained_posture_target(start, foot_position, posture_release_progress)
            } else {
                let Some(height) =
                    terrain.and_then(|terrain| terrain.height_at(foot_position.xz()))
                else {
                    continue;
                };
                let terrain_target = foot_position.with_y(
                    height + MEASURED_ANKLE_SOLE_OFFSET_METRES - SOLE_CONTACT_MARGIN_METRES,
                );
                foot_position.lerp(terrain_target, weight.clamp(0.0, 1.0))
            };
            if !ordinary_leg_solve_is_owned(weight, posture_release_owns) {
                continue;
            }
            targets[index] = Some(target);

            let upper_length = upper_snapshot
                .global
                .translation()
                .distance(lower_snapshot.global.translation());
            let lower_length = lower_snapshot.global.translation().distance(foot_position);
            let reach = maximum_reach(upper_length, lower_length);
            desired_pelvis_shift = desired_pelvis_shift.min(
                required_hip_shift_for_reach(upper_snapshot.global.translation(), target, reach)
                    .clamp(-MAX_HIP_DROP_METRES, 0.0)
                    * weight,
            );
        }

        state.pelvis_shift = if posture_release_owns {
            state.posture_release_pelvis_start * (1.0 - posture_release_blend)
        } else if !state.initialized {
            desired_pelvis_shift
        } else if delta_seconds > 0.0 {
            advance_pelvis_shift(state.pelvis_shift, desired_pelvis_shift, delta_seconds)
        } else {
            state.pelvis_shift
        };
        state.initialized = true;
        apply_root_vertical_shift(rig, state.pelvis_shift, &parents, &mut transforms);
        let rig_rotation = rig
            .rig_scene()
            .and_then(|entity| transforms.p0().compute_global_transform(entity).ok())
            .map(|global| global.rotation())
            .unwrap_or(Quat::IDENTITY);

        for (index, (upper_role, lower_role, foot_role, weight, left)) in
            legs.iter().copied().enumerate()
        {
            let (Some(target), Some(&upper), Some(&lower), Some(&foot)) = (
                targets[index],
                rig.get(&upper_role),
                rig.get(&lower_role),
                rig.get(&foot_role),
            ) else {
                continue;
            };
            let Some((upper_snapshot, lower_snapshot, foot_snapshot)) =
                snapshot_chain(upper, lower, foot, &parents, &transforms.p0())
            else {
                continue;
            };
            let upper_length = upper_snapshot
                .global
                .translation()
                .distance(lower_snapshot.global.translation());
            let lower_length = lower_snapshot
                .global
                .translation()
                .distance(foot_snapshot.global.translation());
            let canonical = pole_to_world(
                rig_rotation,
                canonical_knee_pole(if left { -1.0 } else { 1.0 }),
            );
            let remembered = if left {
                state.left_knee_bend_world
            } else {
                state.right_knee_bend_world
            };
            let previous_end_direction = if left {
                state.left_end_direction
            } else {
                state.right_end_direction
            };
            let foot_facing = rendered_foot_facing(
                rig,
                left,
                foot_snapshot.global.translation(),
                &parents,
                &transforms.p0(),
            );
            let pole = stabilized_knee_pole(
                remembered,
                previous_end_direction,
                upper_snapshot.global.translation(),
                lower_snapshot.global.translation(),
                target,
                canonical,
                foot_facing,
            )
            .unwrap_or(canonical);
            let pole = constrain_rendered_leg_pole(
                rig,
                left,
                upper_snapshot.global.translation(),
                foot_snapshot.global.translation(),
                target,
                pole,
                &parents,
                &transforms.p0(),
            );
            if let Some(solution) = solve_two_bone_with_reach(
                upper_snapshot.global.translation(),
                lower_snapshot.global.translation(),
                foot_snapshot.global.translation(),
                target,
                upper_length,
                lower_length,
                pole,
                maximum_reach(upper_length, lower_length),
            ) {
                let pose_weight =
                    ordinary_pose_ownership_weight(solve_ownership_weight, posture_release_owns);
                apply_two_bone_solution_weighted(
                    upper,
                    lower,
                    foot,
                    solution,
                    pose_weight,
                    &parents,
                    &mut transforms,
                );
                // Pole memory describes the pose that was actually presented.
                // Remembering the full analytic solution during a fractional
                // get-up handoff would reintroduce a one-tick pole discontinuity.
                if let Some((presented_upper, presented_lower, presented_foot)) =
                    snapshot_chain(upper, lower, foot, &parents, &transforms.p0())
                    && let Some(end_direction) = (presented_foot.global.translation()
                        - presented_upper.global.translation())
                    .try_normalize()
                {
                    let bend = (presented_lower.global.translation()
                        - presented_upper.global.translation())
                    .reject_from_normalized(end_direction)
                    .try_normalize();
                    if left {
                        if bend.is_some() {
                            state.left_knee_bend_world = bend;
                        }
                        state.left_end_direction = Some(end_direction);
                    } else {
                        if bend.is_some() {
                            state.right_knee_bend_world = bend;
                        }
                        state.right_end_direction = Some(end_direction);
                    }
                }
            }
            if !posture_release_owns
                && weight > 0.001
                && let (Some(_), Some(normal), Some(sole_axis)) = (
                    terrain,
                    terrain.and_then(|terrain| terrain.normal_at(target.xz())),
                    rig.sole_axis(left),
                )
            {
                align_foot_to_slope_weighted(
                    foot,
                    sole_axis,
                    normal,
                    weight,
                    &parents,
                    &mut transforms,
                );
            }
        }

        let retained_anatomical_poles = diagnostics
            .get_mut(owner)
            .ok()
            .map(|state| {
                (
                    state.0.left_anatomical_pole_world,
                    state.0.right_anatomical_pole_world,
                )
            })
            .unwrap_or((None, None));
        let mut memory = LegIkMemory {
            left_authored_world_target: authored[0],
            right_authored_world_target: authored[1],
            left_foot_world_target: targets[0],
            right_foot_world_target: targets[1],
            left_support_weight: Some(if posture_release_owns {
                0.0
            } else {
                solve_weights.x
            }),
            right_support_weight: Some(if posture_release_owns {
                0.0
            } else {
                solve_weights.y
            }),
            pelvis_shift: state.pelvis_shift,
            posture_handoff_weight: (solve_ownership_weight < 1.0)
                .then_some(solve_ownership_weight),
            left_anatomical_pole_world: retained_anatomical_poles.0,
            right_anatomical_pole_world: retained_anatomical_poles.1,
            ..default()
        };
        memory.left_foot_target = targets[0];
        memory.right_foot_target = targets[1];
        // Zero-weight legs remain authored and must not be reported as IK
        // solves, but their displayed FK endpoints are still the continuity
        // seed if a whole-body posture transition begins next tick.
        state.left_presented_target = targets[0].or(authored[0]);
        state.right_presented_target = targets[1].or(authored[1]);
        if ordinary_owns {
            state.had_presented_solve = targets.iter().all(Option::is_some);
        }
        if posture_release_owns && evaluation_advances {
            let next = (posture_release_progress
                + delta_seconds / POSTURE_RELEASE_SECONDS.max(f32::EPSILON))
            .min(1.0);
            state.posture_release_progress = Some(next);
            if next >= 1.0 {
                state.initialized = false;
                state.left_presented_target = None;
                state.right_presented_target = None;
                state.had_presented_solve = false;
                state.posture_release_left_start = None;
                state.posture_release_right_start = None;
            }
        } else if ordinary_owns {
            state.posture_release_progress = None;
            state.posture_release_left_start = None;
            state.posture_release_right_start = None;
        }
        if let Ok(mut current) = diagnostics.get_mut(owner) {
            current.0 = memory;
        } else {
            commands.entity(owner).insert(LegIkState(memory));
        }
        if let Ok(mut current) = states.get_mut(owner) {
            *current = state;
        } else {
            commands.entity(owner).insert(state);
        }
    }
}

fn advance_ownership_weight(
    current: f32,
    initialized: bool,
    reacquired: bool,
    delta_seconds: f32,
) -> f32 {
    if reacquired {
        0.0
    } else if !initialized {
        1.0
    } else {
        (current + delta_seconds.max(0.0) * 8.0).clamp(0.0, 1.0)
    }
}

fn smooth_ownership_weight(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    progress * progress * progress * (progress * (progress * 6.0 - 15.0) + 10.0)
}

fn apply_root_vertical_shift(
    rig: &HumanoidRig,
    shift: f32,
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    if shift.abs() <= 0.0001 {
        return;
    }
    let Some(&root) = rig.get(&BoneRole::Root) else {
        return;
    };
    let local_delta = parents
        .get(root)
        .ok()
        .and_then(|parent| {
            transforms
                .p0()
                .compute_global_transform(parent.parent())
                .ok()
        })
        .map(|parent| parent.affine().inverse().transform_vector3(Vec3::Y * shift))
        .unwrap_or(Vec3::Y * shift);
    if local_delta.is_finite()
        && let Ok(mut transform) = transforms.p1().get_mut(root)
    {
        transform.translation += local_delta;
    }
}

fn align_foot_to_slope_weighted(
    foot: Entity,
    sole_up_local: Vec3,
    normal: Vec3,
    weight: f32,
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    let Some(snapshot) = snapshot(foot, parents, &transforms.p0()) else {
        return;
    };
    let Some(aligned_world) =
        slope_aligned_world_rotation(snapshot.global.rotation(), sole_up_local, normal)
    else {
        return;
    };
    let desired_world = snapshot
        .global
        .rotation()
        .slerp(aligned_world, weight.clamp(0.0, 1.0));
    let Some(local) = local_rotation_for_world(snapshot.parent_rotation, desired_world) else {
        return;
    };
    if let Ok(mut transform) = transforms.p1().get_mut(foot) {
        transform.rotation = local;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_ownership_selects_guarded_sprint_but_excludes_guard_steps_and_actions() {
        let ordinary = SkeletonState::default();
        assert!(owns(&ordinary));

        let guard_step = ordinary.clone().with_weapon_guard(WeaponGuardState::Raised);
        assert!(!owns(&guard_step));

        let guarded_sprint = guard_step.with_guarded_sprint_locomotion(true);
        assert!(owns(&guarded_sprint));

        let mut attacking = ordinary.clone();
        attacking.begin_attack(AttackSpec::default(), 0, 1).unwrap();
        assert!(!owns(&attacking));
    }

    #[test]
    fn authored_posture_transition_excludes_ordinary_locomotion_ik() {
        let mut skeleton = SkeletonState::default();
        assert!(owns(&skeleton));
        assert!(skeleton.begin_posture_transition(PostureTransitionKind::UprightToProne, 0, 10,));
        assert!(!owns(&skeleton));
    }

    #[test]
    fn posture_transition_releases_retained_foot_target_with_zero_endpoint_derivatives() {
        let retained = Vec3::new(-0.3, 0.1, 0.2);
        let authored = Vec3::new(0.2, 0.0, -0.1);
        assert_eq!(retained_posture_target(retained, authored, 0.0), retained);
        assert_eq!(retained_posture_target(retained, authored, 1.0), authored);
        let first =
            retained_posture_target(retained, authored, 1.0 / 64.0 / POSTURE_RELEASE_SECONDS);
        assert!(first.distance(retained) < 0.01);
        let last = retained_posture_target(
            retained,
            authored,
            1.0 - 1.0 / 64.0 / POSTURE_RELEASE_SECONDS,
        );
        assert!(last.distance(authored) < 0.01);
    }

    #[test]
    fn zero_weight_ordinary_ik_never_claims_or_resolves_a_leg_target() {
        assert!(!ordinary_leg_solve_is_owned(0.0, false));
        assert!(!ordinary_leg_solve_is_owned(0.001, false));
        assert!(ordinary_leg_solve_is_owned(0.0011, false));
        assert!(ordinary_leg_solve_is_owned(0.0, true));

        let authored_only = OrdinaryLocomotionIkState {
            initialized: true,
            left_presented_target: Some(Vec3::NEG_X),
            right_presented_target: Some(Vec3::X),
            ..default()
        };
        assert!(!posture_release_is_owned(authored_only));
        assert!(posture_release_is_owned(OrdinaryLocomotionIkState {
            had_presented_solve: true,
            ..authored_only
        }));
    }

    #[test]
    fn ordinary_ik_yields_for_the_complete_raised_release_handoff() {
        let mut raised = RaisedFootworkState {
            initialized: true,
            release_handoff_active: true,
            ..default()
        };
        let mut skeleton = SkeletonState::default();
        assert!(raised_release_owns_ik(&skeleton, Some(&raised)));
        assert!(!raised_release_uses_transition_authored_target(&skeleton));

        raised.release_handoff_progress = 1.0;
        assert!(raised_release_owns_ik(&skeleton, Some(&raised)));

        let mut upright_release = SkeletonState::default();
        assert!(upright_release.begin_posture_transition(
            PostureTransitionKind::UprightToProne,
            4,
            12,
        ));
        assert!(raised_release_owns_ik(&upright_release, Some(&raised)));
        assert!(raised_release_uses_transition_authored_target(
            &upright_release
        ));

        skeleton
            .begin_dodge(DodgeSpec { direction: Vec2::Y }, 4, 5)
            .unwrap();
        assert!(!raised_release_owns_ik(&skeleton, Some(&raised)));

        skeleton.transition_body(BodyState::Prone);
        assert!(!raised_release_owns_ik(&skeleton, Some(&raised)));

        raised.release_handoff_active = false;
        assert!(!raised_release_owns_ik(&skeleton, Some(&raised)));
        assert!(!raised_release_owns_ik(&skeleton, None));
    }

    #[test]
    fn get_up_reacquisition_ramps_ik_ownership_from_zero() {
        assert_eq!(advance_ownership_weight(0.0, false, false, 0.0), 1.0);
        assert_eq!(advance_ownership_weight(0.0, false, true, 1.0 / 64.0), 0.0);
        let reacquired = advance_ownership_weight(1.0, true, true, 1.0 / 64.0);
        assert_eq!(reacquired, 0.0);
        let next = advance_ownership_weight(reacquired, true, false, 1.0 / 64.0);
        assert!((next - 0.125).abs() <= f32::EPSILON);
        let first_solve_weight = smooth_ownership_weight(next);
        assert!(first_solve_weight < 0.02);
        assert_eq!(
            ordinary_pose_ownership_weight(first_solve_weight, false),
            first_solve_weight,
            "get-up reacquisition must transfer pose authority at the same bounded weight as its target"
        );
        assert_eq!(
            ordinary_pose_ownership_weight(first_solve_weight, true),
            1.0
        );
        assert_eq!(smooth_ownership_weight(0.0), 0.0);
        assert_eq!(smooth_ownership_weight(1.0), 1.0);
    }

    #[test]
    fn cold_start_downed_postures_mark_the_first_get_up_solve_as_reacquired() {
        for body in [BodyState::Prone, BodyState::Supine] {
            let mut skeleton = SkeletonState::default();
            skeleton.transition_body(body);
            assert!(!owns(&skeleton));
            assert_eq!(advance_ownership_weight(0.0, false, true, 1.0 / 64.0), 0.0);
        }
    }
}
