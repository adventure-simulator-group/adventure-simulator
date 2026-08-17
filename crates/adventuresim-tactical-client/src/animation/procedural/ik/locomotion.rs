use super::*;

const MAX_HIP_DROP_METRES: f32 = 0.18;
const SOLE_CONTACT_MARGIN_METRES: f32 = 0.001;

#[derive(Component, Debug, Clone, Copy, Default)]
pub(in crate::animation) struct OrdinaryLocomotionIkState {
    initialized: bool,
    pelvis_shift: f32,
    pelvis_shift_velocity: f32,
    pelvis_shift_acceleration: f32,
    pelvis_recovery: Option<PelvisRecoverySegment>,
    evaluation_tick: Option<u64>,
    last_owner_tick: Option<u64>,
    ownership_weight: f32,
    observed_non_ownership: bool,
    left_presented_target: Option<Vec3>,
    right_presented_target: Option<Vec3>,
    had_presented_solve: bool,
    posture_release_progress: Option<f32>,
    posture_release_duration_seconds: f32,
    posture_release_pelvis_start: f32,
    posture_release_left_start: Option<Vec3>,
    posture_release_right_start: Option<Vec3>,
    left_presented_rotation_chain: Option<LegRotationChain>,
    right_presented_rotation_chain: Option<LegRotationChain>,
    presented_root_transform: Option<Transform>,
    presented_pelvis_transform: Option<Transform>,
    posture_release_left_rotation_start: Option<LegRotationChain>,
    posture_release_right_rotation_start: Option<LegRotationChain>,
    posture_release_pelvis_transform_start: Option<Transform>,
    left_knee_bend_world: Option<Vec3>,
    right_knee_bend_world: Option<Vec3>,
    left_end_direction: Option<Vec3>,
    right_end_direction: Option<Vec3>,
}

const POSTURE_RELEASE_MINIMUM_SECONDS: f32 = 0.18;
// q(s) = 10s^3 - 15s^4 + 6s^5. Sizing the ownership epoch from the exact
// maxima of q'' and q''' makes explosive seams take longer without imposing
// an absolute velocity ceiling or filtering the final pose.
const QUINTIC_MAXIMUM_NORMALIZED_ACCELERATION: f32 = 5.773_503;
const QUINTIC_MAXIMUM_NORMALIZED_JERK: f32 = 60.0;

fn posture_release_duration(maximum_linear_distance: f32, maximum_angle: f32) -> f32 {
    let maximum_distance = maximum_linear_distance.max(maximum_angle).max(0.0);
    let acceleration_duration = (maximum_distance * QUINTIC_MAXIMUM_NORMALIZED_ACCELERATION
        / FOOT_FOLLOWER_MAXIMUM_ACCELERATION)
        .sqrt();
    let jerk_duration =
        (maximum_distance * QUINTIC_MAXIMUM_NORMALIZED_JERK / FOOT_FOLLOWER_MAXIMUM_JERK).cbrt();
    POSTURE_RELEASE_MINIMUM_SECONDS
        .max(acceleration_duration)
        .max(jerk_duration)
}

fn retained_posture_target(start: Vec3, authored: Vec3, progress: f32) -> Vec3 {
    start.lerp(authored, quintic_progress(progress))
}

fn blend_posture_transform(start: Transform, authored: Transform, progress: f32) -> Transform {
    let weight = quintic_progress(progress);
    Transform {
        translation: start.translation.lerp(authored.translation, weight),
        rotation: start.rotation.slerp(authored.rotation, weight).normalize(),
        scale: start.scale.lerp(authored.scale, weight),
    }
}

fn blend_posture_rotation_chain(
    start: LegRotationChain,
    authored: LegRotationChain,
    progress: f32,
) -> LegRotationChain {
    let weight = quintic_progress(progress);
    LegRotationChain {
        upper: start.upper.slerp(authored.upper, weight).normalize(),
        lower: start.lower.slerp(authored.lower, weight).normalize(),
        foot: start.foot.slerp(authored.foot, weight).normalize(),
    }
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

fn retained_exit_handoff_is_owned(ordinary_owns: bool, state: OrdinaryLocomotionIkState) -> bool {
    ordinary_owns && (state.observed_non_ownership || state.posture_release_progress.is_some())
}

fn reset_unretained_reacquisition(
    state: &mut OrdinaryLocomotionIkState,
    reacquired: bool,
    retained_exit_handoff: bool,
) {
    if reacquired && !retained_exit_handoff {
        state.pelvis_shift = 0.0;
        state.pelvis_shift_velocity = 0.0;
        state.pelvis_shift_acceleration = 0.0;
        state.pelvis_recovery = None;
        state.left_knee_bend_world = None;
        state.right_knee_bend_world = None;
        state.left_end_direction = None;
        state.right_end_direction = None;
    }
}

fn posture_release_scalar_pelvis_start(state: OrdinaryLocomotionIkState) -> f32 {
    if state.presented_pelvis_transform.is_some() {
        0.0
    } else {
        state.pelvis_shift
    }
}

fn seed_from_raised_release(state: &mut OrdinaryLocomotionIkState, memory: LegIkMemory) {
    state.left_presented_target = memory
        .left_foot_world_target
        .or(memory.left_last_rendered_world);
    state.right_presented_target = memory
        .right_foot_world_target
        .or(memory.right_last_rendered_world);
    state.pelvis_shift = memory.raised_pelvis_shift;
    state.pelvis_shift_velocity = memory.raised_pelvis_shift_velocity;
    state.pelvis_shift_acceleration = memory.raised_pelvis_shift_acceleration;
    // A recovery segment belongs to the prior ordinary owner. The raised
    // handoff supplies a new exact p/v/a boundary and must never be sampled
    // through that stale segment.
    state.pelvis_recovery = None;
    state.left_knee_bend_world = memory.left_terrain_pole_world;
    state.right_knee_bend_world = memory.right_terrain_pole_world;
    state.left_end_direction = memory.left_terrain_end_direction;
    state.right_end_direction = memory.right_terrain_end_direction;
    state.left_presented_rotation_chain = memory.left_rotation_chain;
    state.right_presented_rotation_chain = memory.right_rotation_chain;
    state.had_presented_solve =
        state.left_presented_target.is_some() && state.right_presented_target.is_some();
    state.initialized = state.had_presented_solve;
}

fn seed_presented_pelvis_from_raised(
    state: &mut OrdinaryLocomotionIkState,
    raised: &RaisedFootworkState,
    local_scalar_shift: Option<Vec3>,
) {
    if let Some(visible_transform) = raised.visible_pelvis_local_transform {
        // The retained local Transform already contains the raised scalar
        // pelvis shift. Ordinary locomotion carries that scalar and its p/v/a
        // separately, so remove exactly that world-space contribution from
        // the full-transform owner before applying the scalar again.
        state.presented_pelvis_transform = Some(decompose_raised_pelvis_transform(
            visible_transform,
            local_scalar_shift,
        ));
    }
}

fn decompose_raised_pelvis_transform(
    mut visible_transform: Transform,
    local_scalar_shift: Option<Vec3>,
) -> Transform {
    if let Some(local_scalar_shift) = local_scalar_shift {
        visible_transform.translation -= local_scalar_shift;
    }
    visible_transform
}

pub(super) fn publish_raised_release_handoff(
    state: &mut OrdinaryLocomotionIkState,
    memory: LegIkMemory,
    pelvis_transform: Option<Transform>,
    local_scalar_shift: Option<Vec3>,
) {
    seed_from_raised_release(state, memory);
    if let Some(pelvis_transform) = pelvis_transform {
        state.presented_pelvis_transform = Some(decompose_raised_pelvis_transform(
            pelvis_transform,
            local_scalar_shift,
        ));
    }
    state.posture_release_progress = None;
    state.posture_release_duration_seconds = 0.0;
    state.posture_release_left_start = None;
    state.posture_release_right_start = None;
    state.posture_release_left_rotation_start = None;
    state.posture_release_right_rotation_start = None;
    state.posture_release_pelvis_transform_start = None;
    state.observed_non_ownership = true;
    state.last_owner_tick = None;
}

pub(super) fn apply_retained_raised_lower_body(
    rig: &HumanoidRig,
    memory: LegIkMemory,
    pelvis_transform: Option<Transform>,
    transforms: &mut Query<&mut Transform>,
) {
    if let (Some(&pelvis), Some(transform)) = (rig.get(&BoneRole::Pelvis), pelvis_transform)
        && let Ok(mut presented) = transforms.get_mut(pelvis)
    {
        *presented = transform;
    }
    for (upper_role, lower_role, foot_role, chain) in [
        (
            BoneRole::ThighLeft,
            BoneRole::ShinLeft,
            BoneRole::FootLeft,
            memory.left_rotation_chain,
        ),
        (
            BoneRole::ThighRight,
            BoneRole::ShinRight,
            BoneRole::FootRight,
            memory.right_rotation_chain,
        ),
    ] {
        let (Some(chain), Some(&upper), Some(&lower), Some(&foot)) = (
            chain,
            rig.get(&upper_role),
            rig.get(&lower_role),
            rig.get(&foot_role),
        ) else {
            continue;
        };
        if let Ok(mut transform) = transforms.get_mut(upper) {
            transform.rotation = chain.upper;
        }
        if let Ok(mut transform) = transforms.get_mut(lower) {
            transform.rotation = chain.lower;
        }
        if let Ok(mut transform) = transforms.get_mut(foot) {
            transform.rotation = chain.foot;
        }
    }
}

fn apply_posture_chain_handoff(
    rig: &HumanoidRig,
    state: OrdinaryLocomotionIkState,
    progress: f32,
    transforms: &mut Query<&mut Transform>,
) {
    if let (Some(&pelvis), Some(start)) = (
        rig.get(&BoneRole::Pelvis),
        state.posture_release_pelvis_transform_start,
    ) && let Ok(mut authored) = transforms.get_mut(pelvis)
    {
        *authored = blend_posture_transform(start, *authored, progress);
    }
    for (upper_role, lower_role, foot_role, start) in [
        (
            BoneRole::ThighLeft,
            BoneRole::ShinLeft,
            BoneRole::FootLeft,
            state.posture_release_left_rotation_start,
        ),
        (
            BoneRole::ThighRight,
            BoneRole::ShinRight,
            BoneRole::FootRight,
            state.posture_release_right_rotation_start,
        ),
    ] {
        let (Some(start), Some(&upper), Some(&lower), Some(&foot)) = (
            start,
            rig.get(&upper_role),
            rig.get(&lower_role),
            rig.get(&foot_role),
        ) else {
            continue;
        };
        let authored = {
            let (Ok(upper), Ok(lower), Ok(foot)) = (
                transforms.get(upper),
                transforms.get(lower),
                transforms.get(foot),
            ) else {
                continue;
            };
            LegRotationChain {
                upper: upper.rotation,
                lower: lower.rotation,
                foot: foot.rotation,
            }
        };
        let blended = blend_posture_rotation_chain(start, authored, progress);
        if let Ok(mut transform) = transforms.get_mut(upper) {
            transform.rotation = blended.upper;
        }
        if let Ok(mut transform) = transforms.get_mut(lower) {
            transform.rotation = blended.lower;
        }
        if let Ok(mut transform) = transforms.get_mut(foot) {
            transform.rotation = blended.foot;
        }
    }
}

fn capture_presented_lower_body(
    rig: &HumanoidRig,
    state: &mut OrdinaryLocomotionIkState,
    transforms: &Query<&mut Transform>,
) {
    state.presented_root_transform = rig
        .get(&BoneRole::Root)
        .and_then(|root| transforms.get(*root).ok())
        .map(|transform| *transform);
    state.presented_pelvis_transform = rig
        .get(&BoneRole::Pelvis)
        .and_then(|pelvis| transforms.get(*pelvis).ok())
        .map(|transform| *transform);
    for (upper_role, lower_role, foot_role, left) in [
        (
            BoneRole::ThighLeft,
            BoneRole::ShinLeft,
            BoneRole::FootLeft,
            true,
        ),
        (
            BoneRole::ThighRight,
            BoneRole::ShinRight,
            BoneRole::FootRight,
            false,
        ),
    ] {
        let chain = match (
            rig.get(&upper_role),
            rig.get(&lower_role),
            rig.get(&foot_role),
        ) {
            (Some(upper), Some(lower), Some(foot)) => {
                match (
                    transforms.get(*upper),
                    transforms.get(*lower),
                    transforms.get(*foot),
                ) {
                    (Ok(upper), Ok(lower), Ok(foot)) => Some(LegRotationChain {
                        upper: upper.rotation,
                        lower: lower.rotation,
                        foot: foot.rotation,
                    }),
                    _ => None,
                }
            }
            _ => None,
        };
        if left {
            state.left_presented_rotation_chain = chain;
        } else {
            state.right_presented_rotation_chain = chain;
        }
    }
}

/// Re-presents the already-solved ordinary lower body for another view of the
/// same fixed sample. This deliberately does not advance or rewrite solver
/// state: authored input is restored before the procedural chain, then the
/// exact first-view IK result is published again for an honest output compare.
fn represent_fixed_tick_lower_body(
    rig: &HumanoidRig,
    state: OrdinaryLocomotionIkState,
    transforms: &mut Query<&mut Transform>,
) {
    if let (Some(&root), Some(presented)) =
        (rig.get(&BoneRole::Root), state.presented_root_transform)
        && let Ok(mut transform) = transforms.get_mut(root)
    {
        *transform = presented;
    }
    if let (Some(&pelvis), Some(presented)) =
        (rig.get(&BoneRole::Pelvis), state.presented_pelvis_transform)
        && let Ok(mut transform) = transforms.get_mut(pelvis)
    {
        *transform = presented;
    }
    for (upper_role, lower_role, foot_role, chain) in [
        (
            BoneRole::ThighLeft,
            BoneRole::ShinLeft,
            BoneRole::FootLeft,
            state.left_presented_rotation_chain,
        ),
        (
            BoneRole::ThighRight,
            BoneRole::ShinRight,
            BoneRole::FootRight,
            state.right_presented_rotation_chain,
        ),
    ] {
        let (Some(chain), Some(&upper), Some(&lower), Some(&foot)) = (
            chain,
            rig.get(&upper_role),
            rig.get(&lower_role),
            rig.get(&foot_role),
        ) else {
            continue;
        };
        if let Ok(mut transform) = transforms.get_mut(upper) {
            transform.rotation = chain.upper;
        }
        if let Ok(mut transform) = transforms.get_mut(lower) {
            transform.rotation = chain.lower;
        }
        if let Ok(mut transform) = transforms.get_mut(foot) {
            transform.rotation = chain.foot;
        }
    }
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
    clock: Res<ProceduralAnimationClock>,
    terrain: Query<&SceneTerrain>,
    owners: Query<(&PresentedSkeleton, &AnimationPlayback)>,
    rigs: Query<(Entity, &HumanoidRig)>,
    mut raised_states: Query<&mut RaisedFootworkState>,
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
        let direct_raised_posture_handoff = skeleton
            .posture_transition()
            .is_some_and(|transition| transition.kind() == PostureTransitionKind::UprightToProne)
            && raised_states
                .get(owner)
                .is_ok_and(|raised| raised.initialized);
        if direct_raised_posture_handoff {
            if let Ok(current) = diagnostics.get_mut(owner) {
                seed_from_raised_release(&mut state, current.0);
            }
            if let Ok(raised) = raised_states.get(owner) {
                let local_scalar_shift = rig
                    .get(&BoneRole::Pelvis)
                    .and_then(|pelvis| parents.get(*pelvis).ok())
                    .and_then(|parent| {
                        transforms
                            .p0()
                            .compute_global_transform(parent.parent())
                            .ok()
                    })
                    .map(|parent_global| {
                        parent_global
                            .affine()
                            .inverse()
                            .transform_vector3(Vec3::Y * state.pelvis_shift)
                    });
                seed_presented_pelvis_from_raised(&mut state, raised, local_scalar_shift);
            }
        }
        let raised_release_owned = raised_release_owns_ik(skeleton, raised_states.get(owner).ok());
        let ordinary_owns = owns(skeleton);
        let retained_exit_handoff = retained_exit_handoff_is_owned(ordinary_owns, state);
        let posture_release_owns = posture_release_is_owned(state)
            && (skeleton.is_posture_transitioning() || retained_exit_handoff);
        if (!ordinary_owns && !posture_release_owns) || raised_release_owned {
            if raised_release_owned {
                if let Ok(current) = diagnostics.get_mut(owner) {
                    seed_from_raised_release(&mut state, current.0);
                }
                // Ordinary IK runs before the raised solver. The transforms at
                // this point are restored FK, so the final visible leg chain
                // must remain the one published by last tick's raised solver.
                // The pelvis is published in its actual parent-local frame;
                // the older owner-local point is intentionally not assigned
                // to Transform::translation because those frames can differ.
                if let Ok(raised) = raised_states.get(owner) {
                    let local_scalar_shift = rig
                        .get(&BoneRole::Pelvis)
                        .and_then(|pelvis| parents.get(*pelvis).ok())
                        .and_then(|parent| {
                            transforms
                                .p0()
                                .compute_global_transform(parent.parent())
                                .ok()
                        })
                        .map(|parent_global| {
                            parent_global
                                .affine()
                                .inverse()
                                .transform_vector3(Vec3::Y * state.pelvis_shift)
                        });
                    seed_presented_pelvis_from_raised(&mut state, raised, local_scalar_shift);
                }
                state.posture_release_progress = None;
                state.posture_release_duration_seconds = 0.0;
                state.posture_release_left_start = None;
                state.posture_release_right_start = None;
                state.posture_release_left_rotation_start = None;
                state.posture_release_right_rotation_start = None;
                state.posture_release_pelvis_transform_start = None;
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
        let (tick, semantic_delta) = clock.semantic_step();
        let evaluation_advances = state.evaluation_tick != Some(tick);
        let delta_seconds = if evaluation_advances {
            state.evaluation_tick = Some(tick);
            semantic_delta.max(0.0)
        } else {
            0.0
        };
        if repeated_fixed_tick_skips_ik(true, evaluation_advances) {
            // Multi-view capture restores authored input before each
            // procedural pass. Publish the first-view lower body again, but
            // do not rebuild or advance hidden solver state.
            represent_fixed_tick_lower_body(rig, state, &mut transforms.p1());
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
        reset_unretained_reacquisition(&mut state, reacquired, retained_exit_handoff);
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
            state.posture_release_pelvis_start = posture_release_scalar_pelvis_start(state);
            state.posture_release_left_start = state.left_presented_target;
            state.posture_release_right_start = state.right_presented_target;
            state.posture_release_left_rotation_start = state.left_presented_rotation_chain;
            state.posture_release_right_rotation_start = state.right_presented_rotation_chain;
            state.posture_release_pelvis_transform_start = state.presented_pelvis_transform;
            let mut maximum_linear_distance = 0.0_f32;
            let mut maximum_angle = 0.0_f32;
            for (index, (upper_role, lower_role, foot_role, _, _)) in
                legs.iter().copied().enumerate()
            {
                let (Some(&upper), Some(&lower), Some(&foot)) = (
                    rig.get(&upper_role),
                    rig.get(&lower_role),
                    rig.get(&foot_role),
                ) else {
                    continue;
                };
                if let Some((_, _, authored_foot)) =
                    snapshot_chain(upper, lower, foot, &parents, &transforms.p0())
                    && let Some(start) = if index == 0 {
                        state.posture_release_left_start
                    } else {
                        state.posture_release_right_start
                    }
                {
                    maximum_linear_distance = maximum_linear_distance
                        .max(start.distance(authored_foot.global.translation()));
                }
                let start_chain = if index == 0 {
                    state.posture_release_left_rotation_start
                } else {
                    state.posture_release_right_rotation_start
                };
                if let Some(start_chain) = start_chain {
                    let query = transforms.p1();
                    if let (Ok(authored_upper), Ok(authored_lower), Ok(authored_foot)) =
                        (query.get(upper), query.get(lower), query.get(foot))
                    {
                        maximum_angle = maximum_angle
                            .max(start_chain.upper.angle_between(authored_upper.rotation))
                            .max(start_chain.lower.angle_between(authored_lower.rotation))
                            .max(start_chain.foot.angle_between(authored_foot.rotation));
                    }
                }
            }
            if let (Some(&pelvis), Some(start)) = (
                rig.get(&BoneRole::Pelvis),
                state.posture_release_pelvis_transform_start,
            ) && let Ok(authored) = transforms.p1().get(pelvis)
            {
                maximum_linear_distance =
                    maximum_linear_distance.max(start.translation.distance(authored.translation));
                maximum_angle = maximum_angle.max(start.rotation.angle_between(authored.rotation));
            }
            state.posture_release_duration_seconds =
                posture_release_duration(maximum_linear_distance, maximum_angle);
        }
        let posture_release_progress = state.posture_release_progress.unwrap_or(0.0);
        let posture_release_blend = quintic_progress(posture_release_progress);
        if posture_release_owns {
            let mut query = transforms.p1();
            apply_posture_chain_handoff(rig, state, posture_release_progress, &mut query);
        }

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
                // The full-chain handoff above is the sole lower-body pose
                // owner. Its FK endpoint is already the desired ankle path;
                // blending that moving endpoint and solving IK again compounds
                // the quintic and creates a late-transition acceleration burst.
                foot_position
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

        let desired_pelvis_shift = if posture_release_owns {
            state.posture_release_pelvis_start * (1.0 - posture_release_blend)
        } else {
            desired_pelvis_shift
        };
        if !state.initialized {
            state.pelvis_shift = desired_pelvis_shift;
            state.pelvis_shift_velocity = 0.0;
            state.pelvis_shift_acceleration = 0.0;
        } else if delta_seconds > 0.0 {
            let followed = advance_pelvis_follower_with_recovery(
                PelvisFollowerState {
                    position: state.pelvis_shift,
                    velocity: state.pelvis_shift_velocity,
                    acceleration: state.pelvis_shift_acceleration,
                },
                &mut state.pelvis_recovery,
                desired_pelvis_shift,
                delta_seconds,
            );
            state.pelvis_shift = followed.position;
            state.pelvis_shift_velocity = followed.velocity;
            state.pelvis_shift_acceleration = followed.acceleration;
        }
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
            if posture_release_owns {
                continue;
            }
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
            pelvis_shift_velocity: state.pelvis_shift_velocity,
            pelvis_shift_acceleration: state.pelvis_shift_acceleration,
            raised_pelvis_follower_valid: false,
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
        if evaluation_advances {
            let query = transforms.p1();
            capture_presented_lower_body(rig, &mut state, &query);
        }
        if ordinary_owns {
            state.had_presented_solve = targets.iter().all(Option::is_some);
        }
        if posture_release_owns && evaluation_advances {
            let next = (posture_release_progress
                + delta_seconds
                    / state
                        .posture_release_duration_seconds
                        .max(POSTURE_RELEASE_MINIMUM_SECONDS))
            .min(1.0);
            state.posture_release_progress = Some(next);
            if next >= 1.0 {
                state.initialized = false;
                state.left_presented_target = None;
                state.right_presented_target = None;
                state.had_presented_solve = false;
                state.posture_release_left_start = None;
                state.posture_release_right_start = None;
                state.posture_release_left_rotation_start = None;
                state.posture_release_right_rotation_start = None;
                state.posture_release_pelvis_transform_start = None;
                state.posture_release_duration_seconds = 0.0;
            }
        } else if ordinary_owns {
            state.posture_release_progress = None;
            state.posture_release_left_start = None;
            state.posture_release_right_start = None;
            state.posture_release_left_rotation_start = None;
            state.posture_release_right_rotation_start = None;
            state.posture_release_pelvis_transform_start = None;
            state.posture_release_duration_seconds = 0.0;
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
        if posture_release_owns
            && skeleton.posture_transition().is_some_and(|transition| {
                transition.kind() == PostureTransitionKind::UprightToProne
            })
            && let Ok(mut raised) = raised_states.get_mut(owner)
        {
            // The first ordinary full-chain sample has now been presented
            // from the retained raised pelvis/legs. Retire the previous owner
            // only after that sample, so the later terrain system cannot
            // shadow it or drop its additive pelvis on this fixed tick.
            *raised = RaisedFootworkState::default();
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
        let duration = posture_release_duration(retained.distance(authored), 0.0);
        assert_eq!(retained_posture_target(retained, authored, 0.0), retained);
        assert_eq!(retained_posture_target(retained, authored, 1.0), authored);
        let first = retained_posture_target(retained, authored, 1.0 / 64.0 / duration);
        assert!(first.distance(retained) < 0.01);
        let last = retained_posture_target(retained, authored, 1.0 - 1.0 / 64.0 / duration);
        assert!(last.distance(authored) < 0.01);
    }

    #[test]
    fn posture_transition_first_sample_retains_the_presented_pelvis_and_hip_chain() {
        let start_pelvis = Transform::from_translation(Vec3::new(0.0, -0.02, 0.0))
            .with_rotation(Quat::from_rotation_y(0.05));
        let authored_pelvis = Transform::from_translation(Vec3::new(0.0, -0.27, 0.0))
            .with_rotation(Quat::from_rotation_y(0.4));
        let first_pelvis = blend_posture_transform(start_pelvis, authored_pelvis, 0.0);
        assert_eq!(first_pelvis.translation, start_pelvis.translation);
        assert!(first_pelvis.rotation.angle_between(start_pelvis.rotation) < 0.000001);

        let start_chain = LegRotationChain {
            upper: Quat::IDENTITY,
            lower: Quat::from_rotation_x(0.2),
            foot: Quat::from_rotation_x(-0.1),
        };
        let authored_chain = LegRotationChain {
            upper: Quat::from_rotation_z(105.79_f32.to_radians()),
            lower: Quat::from_rotation_x(-1.0),
            foot: Quat::from_rotation_y(0.8),
        };
        let first_chain = blend_posture_rotation_chain(start_chain, authored_chain, 0.0);
        assert!(first_chain.upper.angle_between(start_chain.upper) < 0.000001);
        assert!(first_chain.lower.angle_between(start_chain.lower) < 0.000001);
        assert!(first_chain.foot.angle_between(start_chain.foot) < 0.000001);

        let visible_ankle = Vec3::new(2.2413998, 1.9726762, -10.548973);
        let authored_ankle = visible_ankle + Vec3::new(-0.69, 0.71, 0.05);
        let duration = posture_release_duration(
            visible_ankle.distance(authored_ankle),
            start_chain.upper.angle_between(authored_chain.upper),
        );
        let next_progress = (1.0 / 64.0) / duration;
        let next_chain = blend_posture_rotation_chain(start_chain, authored_chain, next_progress);
        assert!(
            start_chain
                .upper
                .angle_between(next_chain.upper)
                .to_degrees()
                < 2.0,
            "the 105.79 degree authored hip seam must enter through the quintic handoff"
        );
        let next_pelvis = blend_posture_transform(start_pelvis, authored_pelvis, next_progress);
        assert!(next_pelvis.translation.distance(start_pelvis.translation) < 0.01);
        let start_knee = start_chain.upper * Vec3::NEG_Y * 0.48;
        let next_knee = next_chain.upper * Vec3::NEG_Y * 0.48;
        assert!(next_knee.distance(start_knee) < 0.10);

        let next_ankle = retained_posture_target(visible_ankle, authored_ankle, next_progress);
        assert!(next_ankle.distance(visible_ankle) < 0.10);
        let mut previous_ankle = visible_ankle;
        let mut previous_upper = start_chain.upper;
        let sample_count = (duration * 64.0).ceil() as usize;
        for sample in 1..=sample_count {
            let progress = (sample as f32 / sample_count as f32).min(1.0);
            let ankle = retained_posture_target(visible_ankle, authored_ankle, progress);
            let chain = blend_posture_rotation_chain(start_chain, authored_chain, progress);
            assert!(ankle.distance(previous_ankle) < 0.10);
            assert!(previous_upper.angle_between(chain.upper).to_degrees() < 10.0);
            previous_ankle = ankle;
            previous_upper = chain.upper;
        }
    }

    #[test]
    fn raised_release_seeds_the_complete_ordinary_transition_handoff() {
        let left = Vec3::new(1.7458137, 1.9343445, -10.977574);
        let right = Vec3::new(2.2413998, 1.9726762, -10.548973);
        let left_chain = LegRotationChain {
            upper: Quat::from_rotation_z(0.4),
            lower: Quat::from_rotation_x(-0.7),
            foot: Quat::from_rotation_y(0.2),
        };
        let memory = LegIkMemory {
            left_foot_world_target: Some(left),
            right_foot_world_target: Some(right),
            raised_pelvis_shift: -0.25,
            raised_pelvis_shift_velocity: -0.4,
            raised_pelvis_shift_acceleration: 1.5,
            raised_pelvis_follower_valid: true,
            left_terrain_pole_world: Some(Vec3::Z),
            right_terrain_pole_world: Some(Vec3::NEG_Z),
            left_terrain_end_direction: Some(Vec3::NEG_Y),
            right_terrain_end_direction: Some(Vec3::NEG_Y),
            left_rotation_chain: Some(left_chain),
            ..default()
        };
        let mut state = OrdinaryLocomotionIkState::default();

        seed_from_raised_release(&mut state, memory);
        let local_pelvis = Transform::from_translation(Vec3::new(0.0, -0.25, 0.04));
        let raised = RaisedFootworkState {
            // Deliberately different coordinates: owner-local is not a valid
            // pelvis-parent-local Transform translation.
            visible_pelvis_owner_local: Some(Vec3::new(4.0, 2.0, -7.0)),
            visible_pelvis_local_transform: Some(local_pelvis),
            ..default()
        };
        seed_presented_pelvis_from_raised(&mut state, &raised, Some(Vec3::Y * -0.25));

        assert!(posture_release_is_owned(state));
        assert_eq!(state.left_presented_target, Some(left));
        assert_eq!(state.right_presented_target, Some(right));
        assert_eq!(state.pelvis_shift, -0.25);
        assert_eq!(state.pelvis_shift_velocity, -0.4);
        assert_eq!(state.pelvis_shift_acceleration, 1.5);
        assert_eq!(state.left_knee_bend_world, Some(Vec3::Z));
        assert_eq!(state.right_knee_bend_world, Some(Vec3::NEG_Z));
        assert_eq!(state.left_presented_rotation_chain, Some(left_chain));
        assert_eq!(
            state.presented_pelvis_transform,
            Some(Transform::from_translation(Vec3::new(0.0, 0.0, 0.04)))
        );
        assert_eq!(posture_release_scalar_pelvis_start(state), 0.0);
        let restored_authored = LegRotationChain {
            upper: Quat::from_rotation_z(105.79_f32.to_radians()) * left_chain.upper,
            lower: Quat::IDENTITY,
            foot: Quat::IDENTITY,
        };
        assert!(
            state
                .left_presented_rotation_chain
                .unwrap()
                .upper
                .angle_between(restored_authored.upper)
                .to_degrees()
                > 100.0
        );
        let first = blend_posture_rotation_chain(
            state.left_presented_rotation_chain.unwrap(),
            restored_authored,
            0.0,
        );
        assert_eq!(first, left_chain);
    }

    #[test]
    fn downed_and_invalid_exit_handoffs_keep_one_pelvis_decomposition_until_reacquired() {
        let memory = LegIkMemory {
            left_foot_world_target: Some(Vec3::new(-0.2, 0.0, 0.1)),
            right_foot_world_target: Some(Vec3::new(0.2, 0.0, 0.1)),
            raised_pelvis_shift: -0.18,
            raised_pelvis_shift_velocity: 0.25,
            raised_pelvis_shift_acceleration: -0.5,
            raised_pelvis_follower_valid: true,
            left_terrain_pole_world: Some(Vec3::X),
            right_terrain_pole_world: Some(Vec3::NEG_X),
            ..default()
        };
        let raw_visible = Transform::from_translation(Vec3::new(0.03, -0.22, 0.07));
        let mut state = OrdinaryLocomotionIkState::default();
        let mut stale_recovery = None;
        let _ = advance_pelvis_follower_with_recovery(
            PelvisFollowerState {
                position: -0.25,
                velocity: 0.0,
                acceleration: 0.0,
            },
            &mut stale_recovery,
            0.0,
            1.0 / CONTINUITY_SAMPLE_HZ,
        );
        assert!(stale_recovery.is_some());
        state.pelvis_recovery = stale_recovery;
        publish_raised_release_handoff(
            &mut state,
            memory,
            Some(raw_visible),
            Some(Vec3::Y * -0.18),
        );

        assert_eq!(state.pelvis_shift, -0.18);
        assert_eq!(state.pelvis_shift_velocity, 0.25);
        assert_eq!(state.pelvis_shift_acceleration, -0.5);
        assert!(state.pelvis_recovery.is_none());
        let residual = state.presented_pelvis_transform.unwrap();
        assert!(
            (residual.translation + Vec3::Y * state.pelvis_shift).distance(raw_visible.translation)
                < 0.000001
        );
        assert!(retained_exit_handoff_is_owned(true, state));
        reset_unretained_reacquisition(&mut state, true, true);
        assert_eq!(state.pelvis_shift, -0.18);
        assert_eq!(state.pelvis_shift_velocity, 0.25);
        assert_eq!(state.pelvis_shift_acceleration, -0.5);
        assert_eq!(state.left_knee_bend_world, Some(Vec3::X));
        assert_eq!(state.right_knee_bend_world, Some(Vec3::NEG_X));

        state.observed_non_ownership = false;
        state.posture_release_progress = Some(0.0);
        assert!(retained_exit_handoff_is_owned(true, state));
        assert!(!retained_exit_handoff_is_owned(false, state));
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
        assert!(!raised_release_owns_ik(&upright_release, Some(&raised)));
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
    fn repeated_fixed_tick_republishes_active_ik_without_advancing_state() {
        let mut world = World::new();
        let root = world.spawn(Transform::IDENTITY).id();
        let pelvis = world.spawn(Transform::IDENTITY).id();
        let left_upper = world.spawn(Transform::IDENTITY).id();
        let left_lower = world.spawn(Transform::IDENTITY).id();
        let left_foot = world.spawn(Transform::IDENTITY).id();
        let right_upper = world.spawn(Transform::IDENTITY).id();
        let right_lower = world.spawn(Transform::IDENTITY).id();
        let right_foot = world.spawn(Transform::IDENTITY).id();
        let rig = HumanoidRig::with_test_bones(&[
            (BoneRole::Root, root),
            (BoneRole::Pelvis, pelvis),
            (BoneRole::ThighLeft, left_upper),
            (BoneRole::ShinLeft, left_lower),
            (BoneRole::FootLeft, left_foot),
            (BoneRole::ThighRight, right_upper),
            (BoneRole::ShinRight, right_lower),
            (BoneRole::FootRight, right_foot),
        ]);
        let left = LegRotationChain {
            upper: Quat::from_rotation_z(0.31),
            lower: Quat::from_rotation_x(-0.72),
            foot: Quat::from_rotation_y(0.18),
        };
        let right = LegRotationChain {
            upper: Quat::from_rotation_z(-0.27),
            lower: Quat::from_rotation_x(-0.61),
            foot: Quat::from_rotation_y(-0.14),
        };
        let cached = OrdinaryLocomotionIkState {
            evaluation_tick: Some(91),
            pelvis_shift: -0.19,
            presented_root_transform: Some(Transform::from_translation(Vec3::new(0.0, -0.19, 0.0))),
            presented_pelvis_transform: Some(Transform::from_translation(Vec3::new(
                0.03, -0.22, 0.04,
            ))),
            left_presented_rotation_chain: Some(left),
            right_presented_rotation_chain: Some(right),
            ..default()
        };
        let state_before_repeat = cached;

        // Authored input has been restored. Re-presentation must reproduce
        // the first-view lower body without changing its retained state.
        let mut system_state: bevy::ecs::system::SystemState<Query<&mut Transform>> =
            bevy::ecs::system::SystemState::new(&mut world);
        {
            let mut query = system_state.get_mut(&mut world).unwrap();
            represent_fixed_tick_lower_body(&rig, cached, &mut query);
        }

        assert_eq!(
            *world.get::<Transform>(root).unwrap(),
            cached.presented_root_transform.unwrap()
        );
        assert_eq!(
            *world.get::<Transform>(pelvis).unwrap(),
            cached.presented_pelvis_transform.unwrap()
        );
        assert_eq!(
            world.get::<Transform>(left_upper).unwrap().rotation,
            left.upper
        );
        assert_eq!(
            world.get::<Transform>(left_lower).unwrap().rotation,
            left.lower
        );
        assert_eq!(
            world.get::<Transform>(left_foot).unwrap().rotation,
            left.foot
        );
        assert_eq!(
            world.get::<Transform>(right_upper).unwrap().rotation,
            right.upper
        );
        assert_eq!(
            world.get::<Transform>(right_lower).unwrap().rotation,
            right.lower
        );
        assert_eq!(
            world.get::<Transform>(right_foot).unwrap().rotation,
            right.foot
        );
        assert_eq!(cached.evaluation_tick, state_before_repeat.evaluation_tick);
        assert_eq!(cached.pelvis_shift, state_before_repeat.pelvis_shift);
        assert_eq!(
            cached.presented_root_transform,
            state_before_repeat.presented_root_transform
        );
        assert_eq!(
            cached.presented_pelvis_transform,
            state_before_repeat.presented_pelvis_transform
        );
        assert_eq!(
            cached.left_presented_rotation_chain,
            state_before_repeat.left_presented_rotation_chain
        );
        assert_eq!(
            cached.right_presented_rotation_chain,
            state_before_repeat.right_presented_rotation_chain
        );
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
