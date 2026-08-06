use super::*;

mod body_response;
mod hands;
mod solver;

pub(in crate::animation) use body_response::apply_locomotion_body_response;
#[cfg(test)]
pub(super) use body_response::body_response_target;
pub(super) use body_response::presentation_tick_delta;
pub(in crate::animation) use hands::apply_arm_and_weapon_constraints;
#[cfg(test)]
pub(super) use hands::secondary_grip_world;
pub(super) use solver::*;

#[derive(Debug, Clone, Copy, Default)]
struct LegIkMemory {
    left_leg: Option<Vec3>,
    right_leg: Option<Vec3>,
    left_terrain_pole_world: Option<Vec3>,
    right_terrain_pole_world: Option<Vec3>,
    left_foot_plant: Option<Vec3>,
    right_foot_plant: Option<Vec3>,
    left_foot_target: Option<Vec3>,
    right_foot_target: Option<Vec3>,
    left_foot_world_target: Option<Vec3>,
    right_foot_world_target: Option<Vec3>,
    left_support_weight: Option<f32>,
    right_support_weight: Option<f32>,
    left_release_active: bool,
    right_release_active: bool,
    pelvis_shift: f32,
    raised_pelvis_shift: f32,
    terrain_blend: f32,
    rig_origin: Option<Vec3>,
    rig_rotation: Option<Quat>,
    evaluation_tick: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default)]
struct ArmIkMemory {
    left_arm: Option<Vec3>,
    right_arm: Option<Vec3>,
}

/// Optional deterministic clock for tools that render the same simulation
/// tick more than once. Gameplay leaves the override unset and advances from
/// Bevy's render delta.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub(crate) struct ProceduralAnimationClock {
    fixed_tick: Option<(u64, f32)>,
}

impl ProceduralAnimationClock {
    #[allow(dead_code)] // Used by the standalone animation viewer and unit fixtures.
    pub(crate) fn set_fixed_tick(&mut self, tick: u64, delta_seconds: f32) {
        self.fixed_tick = Some((tick, delta_seconds.max(0.0)));
    }

    pub(crate) fn fixed_step(&self) -> Option<(u64, f32)> {
        self.fixed_tick
    }
}

pub(super) const MIN_INTER_FOOT_SEPARATION: f32 = 0.16;
// Cascadeur's final ankle bones sit about 15 mm inside analytic targets after
// the complete hierarchy solve. Keep a measured planning allowance so the
// rendered bones, not merely abstract targets, retain the 0.16 m contract.
pub(super) const GUARD_TARGET_INTER_FOOT_SEPARATION: f32 = MIN_INTER_FOOT_SEPARATION + 0.04;
pub(super) const FOOT_TRACK_INNER: f32 = MIN_INTER_FOOT_SEPARATION * 0.5;
pub(super) const FOOT_TRACK_OUTER: f32 = 0.55;
const MAX_PLANT_DISCONTINUITY: f32 = 2.0;
const MAX_OWNER_TRANSLATION_PER_TICK: f32 = 0.5;
// A player can legitimately snap-turn by 90 degrees in one input sample. Only
// discard retained plants for rotations that are unmistakably teleport-like.
const MAX_OWNER_ROTATION_PER_TICK_DEGREES: f32 = 120.0;
const MAX_FOOT_TARGET_SPEED: f32 = 5.0;
pub(super) const MAX_FOOT_TARGET_STEP: f32 = 0.2;
const PELVIS_CORRECTION_SPEED: f32 = 1.6;
pub(super) const MAX_PELVIS_CORRECTION_STEP: f32 = 0.05;
const TERRAIN_IK_BLEND_SPEED: f32 = 4.0;
const MIN_KNEE_FLEXION: f32 = 20.0_f32.to_radians();
const MIN_TERRAIN_KNEE_FLEXION: f32 = 8.0_f32.to_radians();
// Keep the normal knee reserve while a landing visibly carries weight, then
// release it before the pelvis reaches the authored height. The released
// reach remains capped at the authored leg extension, preventing a final
// recovery-frame foot lift or snap without introducing a straight-leg target.
const LANDING_KNEE_RESERVE_RELEASE_COMPRESSION: f32 = 0.012;
const LANDING_KNEE_RESERVE_FULL_COMPRESSION: f32 = 0.04;
const RAISED_GUARD_PELVIS_DROP: f32 = 0.14;
/// Measured vertical distance from the Cascadeur ankle bone to its sole.
pub(crate) const MEASURED_ANKLE_SOLE_OFFSET_METRES: f32 = 0.085;
const SWING_SOLE_CLEARANCE_METRES: f32 = 0.02;

#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct LegIkState(LegIkMemory);

#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct ArmIkState(ArmIkMemory);

/// Client-only world-space plants for combat-stance locomotion. The replicated
/// skeleton chooses cadence and direction; exact feet remain presentation
/// state so they never become tactical authority.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct RaisedFootworkState {
    initialized: bool,
    half_step: u8,
    lead: LeadFoot,
    swing_left: bool,
    step_origin: Vec3,
    step_rotation: Quat,
    swing_stance_local: Vec3,
    swing_start: Vec3,
    swing_end: Vec3,
    left_plant: Vec3,
    right_plant: Vec3,
    evaluation_tick: Option<u64>,
    step_sequence: u32,
    pub(crate) left_solve_target: Option<Vec3>,
    pub(crate) right_solve_target: Option<Vec3>,
}

impl Default for RaisedFootworkState {
    fn default() -> Self {
        Self {
            initialized: false,
            half_step: 0,
            lead: LeadFoot::Left,
            swing_left: false,
            step_origin: Vec3::ZERO,
            step_rotation: Quat::IDENTITY,
            swing_stance_local: Vec3::ZERO,
            swing_start: Vec3::ZERO,
            swing_end: Vec3::ZERO,
            left_plant: Vec3::ZERO,
            right_plant: Vec3::ZERO,
            evaluation_tick: None,
            step_sequence: 0,
            left_solve_target: None,
            right_solve_target: None,
        }
    }
}

/// Client-only world-space target for a hand. It is presentation data and is
/// deliberately absent from replicated `SkeletonState`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HandIkTarget {
    pub translation: Vec3,
    pub rotation: Option<Quat>,
    pub weight: f32,
}

/// Optional client-only direct hand targets.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct HumanoidIkTargets {
    pub left: Option<HandIkTarget>,
    pub right: Option<HandIkTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Public input for optional held-item constraints.
pub(crate) enum HandSide {
    Left,
    Right,
}

/// Constrains a client-side held item to an authored weapon socket. The
/// optional point is in weapon-local space and becomes an off-hand IK target.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct HeldWeaponConstraint {
    pub owner: Entity,
    pub primary_hand: HandSide,
    pub secondary_grip_local: Option<Vec3>,
}

/// Places the planted foot on the terrain with an analytic two-bone solve,
/// then lowers the hips by the bounded residual. Existing weapon/hand
/// constraints run at the same final-pose seam.
pub(in crate::animation) fn apply_terrain_leg_ik(
    enabled: Res<super::super::TerrainIkEnabled>,
    time: Res<Time>,
    clock: Res<ProceduralAnimationClock>,
    terrain: Query<&SceneTerrain>,
    owners: Query<&PresentedSkeleton>,
    rigs: Query<(Entity, &HumanoidRig)>,
    parents: Query<&ChildOf>,
    mut ik_states: Query<&mut LegIkState>,
    mut raised_states: Query<&mut RaisedFootworkState>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
    mut commands: Commands,
) {
    let terrain = terrain.single().ok();
    for (owner, rig) in &rigs {
        let Ok(skeleton) = owners.get(owner) else {
            continue;
        };
        if !terrain_ik_posture_is_valid(skeleton) {
            if let Ok(mut state) = ik_states.get_mut(owner) {
                state.0 = LegIkMemory::default();
            }
            if let Ok(mut state) = raised_states.get_mut(owner) {
                *state = RaisedFootworkState::default();
            }
            continue;
        }
        let raised_guard_follower = raised_footwork_posture_is_valid(skeleton)
            && skeleton.weapon_guard() == WeaponGuardState::Raised
            && skeleton.action_kind() == SkeletonAction::None
            && skeleton.raised_locomotion().is_moving();
        if !raised_guard_follower && let Ok(mut raised) = raised_states.get_mut(owner) {
            *raised = RaisedFootworkState::default();
        }
        let (left_weight, right_weight) = locomotion_support_weights(skeleton);
        let legs = [
            (
                BoneRole::ThighLeft,
                BoneRole::ShinLeft,
                BoneRole::FootLeft,
                left_weight,
                true,
            ),
            (
                BoneRole::ThighRight,
                BoneRole::ShinRight,
                BoneRole::FootRight,
                right_weight,
                false,
            ),
        ];
        let (mut memory, memory_was_missing) = match ik_states.get_mut(owner) {
            Ok(state) => (state.0, false),
            Err(_) => (
                // Startup is not a toggle transition: establish the configured
                // mode immediately so the first supported frame can plant.
                LegIkMemory {
                    terrain_blend: if enabled.0 { 1.0 } else { 0.0 },
                    ..default()
                },
                true,
            ),
        };
        let state_delta_seconds = match clock.fixed_tick {
            Some((tick, _)) if memory.evaluation_tick == Some(tick) => 0.0,
            Some((tick, delta_seconds)) => {
                memory.evaluation_tick = Some(tick);
                delta_seconds
            }
            None => time.delta_secs(),
        };
        if state_delta_seconds > 0.0 {
            let desired = if enabled.0 { 1.0 } else { 0.0 };
            memory.terrain_blend += (desired - memory.terrain_blend).clamp(
                -TERRAIN_IK_BLEND_SPEED * state_delta_seconds,
                TERRAIN_IK_BLEND_SPEED * state_delta_seconds,
            );
        }
        let terrain_blend = memory.terrain_blend.clamp(0.0, 1.0);
        // Plant and pelvis reach belong to the server-owned authored-body
        // frame. Terrain knee poles retain their world bend plane separately
        // so a sharp owner turn cannot corkscrew a planted knee.
        let (rig_origin, rig_rotation) = rig
            .rig_scene()
            .and_then(|entity| transforms.p0().compute_global_transform(entity).ok())
            .map(|global| (global.translation(), global.rotation()))
            .unwrap_or((Vec3::ZERO, Quat::IDENTITY));
        if state_delta_seconds > 0.0 {
            let owner_discontinuous = memory.rig_origin.is_some_and(|previous| {
                previous.distance(rig_origin) > MAX_OWNER_TRANSLATION_PER_TICK
            }) || memory.rig_rotation.is_some_and(|previous| {
                previous.angle_between(rig_rotation).to_degrees()
                    > MAX_OWNER_ROTATION_PER_TICK_DEGREES
            });
            if owner_discontinuous {
                memory.left_foot_plant = None;
                memory.right_foot_plant = None;
                memory.left_foot_target = None;
                memory.right_foot_target = None;
                memory.left_foot_world_target = None;
                memory.right_foot_world_target = None;
                memory.left_support_weight = None;
                memory.right_support_weight = None;
                memory.left_terrain_pole_world = None;
                memory.right_terrain_pole_world = None;
                memory.left_release_active = false;
                memory.right_release_active = false;
                memory.pelvis_shift = 0.0;
            }
            memory.rig_origin = Some(rig_origin);
            memory.rig_rotation = Some(rig_rotation);
        }
        let desired_raised_pelvis_shift = if raised_guard_follower {
            -RAISED_GUARD_PELVIS_DROP
        } else {
            0.0
        };
        if state_delta_seconds > 0.0 {
            memory.raised_pelvis_shift = advance_pelvis_shift(
                memory.raised_pelvis_shift,
                desired_raised_pelvis_shift,
                state_delta_seconds,
            );
        }
        let raised_pelvis_shift = memory.raised_pelvis_shift;
        if raised_pelvis_shift < -0.001
            && let Some(&pelvis) = rig.get(&BoneRole::Pelvis)
        {
            let local_delta = parents
                .get(pelvis)
                .ok()
                .and_then(|parent| {
                    transforms
                        .p0()
                        .compute_global_transform(parent.parent())
                        .ok()
                })
                .map(|parent| {
                    parent
                        .affine()
                        .inverse()
                        .transform_vector3(Vec3::Y * raised_pelvis_shift)
                })
                .unwrap_or(Vec3::Y * raised_pelvis_shift);
            if let Ok(mut transform) = transforms.p1().get_mut(pelvis) {
                transform.translation += local_delta;
            }
        }
        if raised_guard_follower {
            // The authored guard is nearly straight-legged. Smoothly lower its
            // pelvis so a world-planted support foot remains within physical
            // reach without a one-frame stance-height snap at starts or stops.
            let left = (
                rig.get(&BoneRole::ThighLeft),
                rig.get(&BoneRole::ShinLeft),
                rig.get(&BoneRole::FootLeft),
            );
            let right = (
                rig.get(&BoneRole::ThighRight),
                rig.get(&BoneRole::ShinRight),
                rig.get(&BoneRole::FootRight),
            );
            let (Some(&left_upper), Some(&left_lower), Some(&left_foot)) = left else {
                continue;
            };
            let (Some(&right_upper), Some(&right_lower), Some(&right_foot)) = right else {
                continue;
            };
            let Some((_, _, left_foot_snapshot)) = snapshot_chain(
                left_upper,
                left_lower,
                left_foot,
                &parents,
                &transforms.p0(),
            ) else {
                continue;
            };
            let Some((_, _, right_foot_snapshot)) = snapshot_chain(
                right_upper,
                right_lower,
                right_foot,
                &parents,
                &transforms.p0(),
            ) else {
                continue;
            };
            let mut footwork = raised_states
                .get_mut(owner)
                .map(|state| *state)
                .unwrap_or_default();
            let tick = clock.fixed_tick.map(|(tick, _)| tick);
            let advances = match tick {
                Some(tick) => footwork.evaluation_tick != Some(tick),
                None => state_delta_seconds > 0.0,
            };
            if let Some(tick) = tick {
                footwork.evaluation_tick = Some(tick);
            }
            let phase = skeleton.gait_phase.rem_euclid(1.0);
            let half_step = (phase >= 0.5) as u8;
            let swing_left = skeleton.raised_locomotion().swing_foot() == Some(LeadFoot::Left);
            // Pelvis lowering must not lower the semantic movement plane.
            // Recover the pre-drop authored ankle positions for persistent
            // flat plants; the analytic solve bends the lowered legs to them.
            let left_authored =
                left_foot_snapshot.global.translation() + Vec3::Y * -raised_pelvis_shift;
            let right_authored =
                right_foot_snapshot.global.translation() + Vec3::Y * -raised_pelvis_shift;
            let visible_left = memory.left_foot_world_target.unwrap_or(left_authored);
            let visible_right = memory.right_foot_world_target.unwrap_or(right_authored);
            let discontinuous =
                footwork.initialized && rig_origin.distance_squared(footwork.step_origin) > 4.0;
            let sequence_delta = guard_step_sequence_delta(
                footwork.step_sequence,
                skeleton.raised_locomotion().step_sequence(),
            );
            let skipped_handoff = footwork.initialized && sequence_delta > 1;
            if !footwork.initialized
                || footwork.lead != skeleton.lead_foot
                || discontinuous
                || skipped_handoff
            {
                footwork = RaisedFootworkState {
                    initialized: true,
                    half_step,
                    lead: skeleton.lead_foot,
                    swing_left,
                    step_origin: rig_origin,
                    step_rotation: rig_rotation,
                    swing_stance_local: rig_rotation.inverse()
                        * ((if swing_left {
                            left_authored
                        } else {
                            right_authored
                        }) - rig_origin),
                    swing_start: if swing_left {
                        visible_left
                    } else {
                        visible_right
                    },
                    swing_end: if swing_left {
                        left_authored
                    } else {
                        right_authored
                    },
                    left_plant: visible_left,
                    right_plant: visible_right,
                    evaluation_tick: tick,
                    step_sequence: skeleton.raised_locomotion().step_sequence(),
                    left_solve_target: None,
                    right_solve_target: None,
                };
            } else if advances && sequence_delta == 1 {
                if footwork.swing_left {
                    footwork.left_plant = footwork.left_solve_target.unwrap_or(footwork.swing_end);
                } else {
                    footwork.right_plant =
                        footwork.right_solve_target.unwrap_or(footwork.swing_end);
                }
                footwork.half_step = half_step;
                footwork.step_sequence = skeleton.raised_locomotion().step_sequence();
                footwork.swing_left = swing_left;
                footwork.step_origin = rig_origin;
                footwork.step_rotation = rig_rotation;
                footwork.swing_stance_local = rig_rotation.inverse()
                    * ((if swing_left {
                        left_authored
                    } else {
                        right_authored
                    }) - rig_origin);
                footwork.swing_start = if swing_left {
                    footwork.left_plant
                } else {
                    footwork.right_plant
                };
            }
            let local_direction = skeleton
                .raised_locomotion()
                .local_direction()
                .normalize_or_zero();
            // Semantic controller axes are opposite the authored rig's X/Z
            // axes. The owner carries the single 180-degree body conversion.
            let rig_local_direction = -local_direction;
            let step_length = guard_step_length(skeleton.raised_locomotion().speed());
            let opposite_plant = if footwork.swing_left {
                footwork.right_plant
            } else {
                footwork.left_plant
            };
            footwork.swing_end = plan_guard_step_endpoint(
                footwork.step_origin,
                footwork.step_rotation,
                footwork.swing_stance_local,
                rig_local_direction,
                step_length,
                footwork.swing_left,
                opposite_plant,
            );
            let step_progress = (phase * 2.0).fract();
            let horizontal_progress = smoothstep(0.0, 1.0, step_progress);
            let mut swing_target = footwork
                .swing_start
                .lerp(footwork.swing_end, horizontal_progress);
            let mut left_target = footwork.left_plant;
            let mut right_target = footwork.right_plant;
            let support_target = if footwork.swing_left {
                right_target
            } else {
                left_target
            };
            swing_target = constrain_guard_swing_to_live_corridor(
                swing_target,
                support_target,
                rig_origin,
                rig_rotation,
                footwork.swing_stance_local.x.signum(),
            );
            let mut terrain_swing_end = footwork.swing_end;
            if enabled.0
                && let Some(terrain) = terrain
            {
                left_target = terrain_conformed_guard_target(
                    left_target,
                    terrain.height_at(left_target.xz()),
                );
                right_target = terrain_conformed_guard_target(
                    right_target,
                    terrain.height_at(right_target.xz()),
                );
                terrain_swing_end = terrain_conformed_guard_target(
                    terrain_swing_end,
                    terrain.height_at(terrain_swing_end.xz()),
                );
                swing_target.y = footwork
                    .swing_start
                    .y
                    .lerp(terrain_swing_end.y, horizontal_progress);
            }
            swing_target.y += (std::f32::consts::PI * step_progress).sin() * 0.10;
            if footwork.swing_left {
                left_target = swing_target;
            } else {
                right_target = swing_target;
            }

            for (upper, lower, foot, target, left, support) in [
                (
                    left_upper,
                    left_lower,
                    left_foot,
                    left_target,
                    true,
                    !footwork.swing_left,
                ),
                (
                    right_upper,
                    right_lower,
                    right_foot,
                    right_target,
                    false,
                    footwork.swing_left,
                ),
            ] {
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
                let side = anatomical_side(
                    rig_rotation,
                    rig_origin,
                    upper_snapshot.global.translation(),
                    left,
                );
                let remembered = if left {
                    memory.left_leg
                } else {
                    memory.right_leg
                };
                let canonical_pole = canonical_knee_pole(side);
                let remembered = remembered.filter(|pole| pole.dot(canonical_pole) > 0.2);
                let pole = pole_to_world(rig_rotation, remembered.unwrap_or(canonical_pole));
                if let Some(solution) = solve_two_bone_with_reach(
                    upper_snapshot.global.translation(),
                    lower_snapshot.global.translation(),
                    foot_snapshot.global.translation(),
                    target,
                    upper_length,
                    lower_length,
                    pole,
                    (upper_length + lower_length) * 0.999,
                ) {
                    apply_two_bone_solution(
                        upper,
                        lower,
                        foot,
                        solution,
                        &parents,
                        &mut transforms,
                    );
                    let bend = (solution.knee - upper_snapshot.global.translation())
                        .reject_from_normalized(solution.end_direction);
                    if state_delta_seconds > 0.0
                        && let Some(valid) = bend.try_normalize()
                    {
                        if left {
                            memory.left_leg = Some(pole_to_owner(rig_rotation, valid));
                        } else {
                            memory.right_leg = Some(pole_to_owner(rig_rotation, valid));
                        }
                    }
                }
                if enabled.0
                    && support
                    && let Some(terrain) = terrain
                    && let Some(normal) = terrain.normal_at(target.xz())
                    && let Some(sole_axis) = rig.sole_axis(left)
                {
                    align_foot_to_slope(foot, sole_axis, normal, 1.0, &parents, &mut transforms);
                }
                if left {
                    footwork.left_solve_target = Some(target);
                    memory.left_foot_world_target = Some(target);
                    memory.left_support_weight = Some(support as u8 as f32);
                } else {
                    footwork.right_solve_target = Some(target);
                    memory.right_foot_world_target = Some(target);
                    memory.right_support_weight = Some(support as u8 as f32);
                }
            }
            if let Ok(mut state) = raised_states.get_mut(owner) {
                *state = footwork;
            } else {
                commands.entity(owner).insert(footwork);
            }
            if let Ok(mut state) = ik_states.get_mut(owner) {
                state.0 = memory;
            } else {
                commands.entity(owner).insert(LegIkState(memory));
            }
            continue;
        }

        if !enabled.0
            && terrain_blend <= 0.001
            && !memory.left_release_active
            && !memory.right_release_active
        {
            // Once the bounded release finishes, clear leg targets so a later
            // re-enable cannot resurrect stale plants. Arm pole continuity is
            // unrelated.
            memory.left_foot_plant = None;
            memory.right_foot_plant = None;
            memory.left_foot_target = None;
            memory.right_foot_target = None;
            memory.left_foot_world_target = None;
            memory.right_foot_world_target = None;
            memory.left_support_weight = None;
            memory.right_support_weight = None;
            memory.left_terrain_pole_world = None;
            memory.right_terrain_pole_world = None;
            memory.left_release_active = false;
            memory.right_release_active = false;
            memory.pelvis_shift = 0.0;
            if let Ok(mut state) = ik_states.get_mut(owner) {
                state.0 = memory;
            } else {
                commands.entity(owner).insert(LegIkState(memory));
            }
            continue;
        }
        let Some(terrain) = terrain else {
            continue;
        };
        let mut desired_hip_shift = 0.0_f32;
        for (upper_role, lower_role, foot_role, weight, left) in legs {
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
            let position = foot_snapshot.global.translation();
            if let Some(height) = terrain.height_at(position.xz()) {
                let desired_ankle = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
                desired_hip_shift = desired_hip_shift
                    .min(((desired_ankle - position.y) * weight).clamp(-0.18, 0.0));
            }
            let plant = if left {
                memory.left_foot_plant
            } else {
                memory.right_foot_plant
            };
            let Some(plant) = plant else { continue };
            // A remembered plant is world-space. Do not reproject it through
            // the rotating/moving anatomical corridor every frame: that made
            // a visibly planted foot skate during turns. New contacts are
            // constrained when acquired, and reach limiting below remains the
            // only reason an established plant may yield.
            let horizontal_target = plant;
            let Some(height) = terrain.height_at(horizontal_target.xz()) else {
                continue;
            };
            let target_y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
            let upper_length = upper_snapshot
                .global
                .translation()
                .distance(lower_snapshot.global.translation());
            let lower_length = lower_snapshot
                .global
                .translation()
                .distance(foot_snapshot.global.translation());
            let reach = terrain_maximum_reach(upper_length, lower_length);
            let horizontal_distance = (horizontal_target - upper_snapshot.global.translation())
                .xz()
                .length();
            let maximum_vertical = (reach * reach - horizontal_distance * horizontal_distance)
                .max(0.0)
                .sqrt();
            let reach_shift = target_y + maximum_vertical - upper_snapshot.global.translation().y;
            desired_hip_shift = desired_hip_shift.min((reach_shift * weight).clamp(-0.25, 0.0));
        }
        desired_hip_shift *= terrain_blend;
        // Couple both legs through one bounded, continuous pelvis correction.
        // The authored pose is restored each frame, so this retained scalar is
        // the only temporal state and cannot accumulate transform drift.
        if memory_was_missing {
            memory.pelvis_shift = desired_hip_shift;
        } else if state_delta_seconds > 0.0 {
            memory.pelvis_shift =
                advance_pelvis_shift(memory.pelvis_shift, desired_hip_shift, state_delta_seconds);
        }
        let hip_shift = memory.pelvis_shift;
        if hip_shift < -0.001
            && let Some(&pelvis) = rig.get(&BoneRole::Pelvis)
        {
            let local_delta = parents
                .get(pelvis)
                .ok()
                .and_then(|parent| {
                    transforms
                        .p0()
                        .compute_global_transform(parent.parent())
                        .ok()
                })
                .map(|parent| {
                    parent
                        .affine()
                        .inverse()
                        .transform_vector3(Vec3::Y * hip_shift)
                })
                .unwrap_or(Vec3::Y * hip_shift);
            if local_delta.is_finite()
                && let Ok(mut transform) = transforms.p1().get_mut(pelvis)
            {
                transform.translation += local_delta;
            }
        }
        for (upper_role, lower_role, foot_role, weight, left) in legs {
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
            let mut plant = if left {
                memory.left_foot_plant
            } else {
                memory.right_foot_plant
            };
            let side = anatomical_side(
                rig_rotation,
                rig_origin,
                upper_snapshot.global.translation(),
                left,
            );
            let upper_length = upper_snapshot
                .global
                .translation()
                .distance(lower_snapshot.global.translation());
            let lower_length = lower_snapshot.global.translation().distance(foot_position);
            if weight <= 0.05
                || plant.is_some_and(|position| !plant_is_continuous(position, foot_position))
            {
                plant = None;
            }
            if !terrain_leg_has_support(weight) {
                // Continue the bounded release all the way to authored swing.
                // Clearing the retained target at the old 0.05 threshold made
                // the foot teleport on the first nominally unloaded frame.
                let mut desired_target = foot_position;
                if let Some(height) = terrain.height_at(foot_position.xz()) {
                    let minimum_ankle_y =
                        height + MEASURED_ANKLE_SOLE_OFFSET_METRES + SWING_SOLE_CLEARANCE_METRES;
                    desired_target.y = desired_target
                        .y
                        .max(foot_position.y.lerp(minimum_ankle_y, terrain_blend));
                }
                let desired_owner_target = rig_rotation.inverse() * (desired_target - rig_origin);
                let previous_owner_target = if left {
                    memory.left_foot_target
                } else {
                    memory.right_foot_target
                };
                let owner_target = advance_foot_target(
                    previous_owner_target,
                    desired_owner_target,
                    state_delta_seconds,
                );
                let mut target = rig_origin + rig_rotation * owner_target;
                if let Some(height) = terrain.height_at(target.xz()) {
                    target.y = target.y.max(
                        height
                            + MEASURED_ANKLE_SOLE_OFFSET_METRES
                            + SWING_SOLE_CLEARANCE_METRES * terrain_blend,
                    );
                }
                let canonical_pole = canonical_knee_pole(side);
                let canonical_world = pole_to_world(rig_rotation, canonical_pole);
                let remembered = if left {
                    memory.left_terrain_pole_world
                } else {
                    memory.right_terrain_pole_world
                }
                .filter(|pole| pole.dot(canonical_world) > 0.2);
                let pole = remembered.unwrap_or(canonical_world);
                if let Some(solution) = solve_two_bone(
                    upper_snapshot.global.translation(),
                    lower_snapshot.global.translation(),
                    foot_position,
                    target,
                    upper_length,
                    lower_length,
                    pole,
                ) {
                    apply_two_bone_solution(
                        upper,
                        lower,
                        foot,
                        solution,
                        &parents,
                        &mut transforms,
                    );
                }
                let release_active = owner_target.distance_squared(desired_owner_target) > 0.000001;
                if left {
                    memory.left_foot_plant = None;
                    memory.left_foot_target = Some(owner_target);
                    memory.left_foot_world_target = Some(target);
                    memory.left_support_weight = Some(0.0);
                    memory.left_release_active = release_active;
                } else {
                    memory.right_foot_plant = None;
                    memory.right_foot_target = Some(owner_target);
                    memory.right_foot_world_target = Some(target);
                    memory.right_support_weight = Some(0.0);
                    memory.right_release_active = release_active;
                }
                continue;
            }
            // Do not memorize a footprint while the swing foot is merely
            // approaching the ground. Capturing that stale position early
            // makes the pelvis outrun it, forcing the reach limiter to drag a
            // fully weighted foot and drive the knee toward extension.
            if weight >= 0.95 && plant.is_none() && !raised_guard_follower {
                let visible_contact = if left {
                    memory.left_foot_world_target
                } else {
                    memory.right_foot_world_target
                }
                .unwrap_or(foot_position);
                plant = Some(constrain_foot_to_track(
                    visible_contact,
                    rig_origin,
                    rig_rotation,
                    side,
                ));
            }
            let mut horizontal_target = plant.unwrap_or_else(|| {
                constrain_foot_to_track(foot_position, rig_origin, rig_rotation, side)
            });
            let plant_local = rig_rotation.inverse() * (horizontal_target - rig_origin);
            if plant_local.x * side < FOOT_TRACK_INNER {
                // A retained world plant can rotate through the body's center
                // during an exact reversal. Move only the offending lateral
                // component back to its anatomical corridor; target velocity
                // limiting below keeps that correction continuous.
                horizontal_target =
                    constrain_foot_to_track(horizontal_target, rig_origin, rig_rotation, side);
                plant = plant.map(|_| horizontal_target);
            }
            let Some(height) = terrain.height_at(horizontal_target.xz()) else {
                continue;
            };
            let sole_offset = MEASURED_ANKLE_SOLE_OFFSET_METRES;
            let mut planted_target = Vec3::new(
                horizontal_target.x,
                height + sole_offset,
                horizontal_target.z,
            );
            // A turning or advancing pelvis can make an otherwise valid plant
            // unreachable before its support weight releases. Slide that
            // target only as far as anatomical reach requires instead of
            // dropping and reacquiring it in one frame. Re-store the adjusted
            // target so successive turns follow the side corridor continuously.
            planted_target = constrain_target_to_reach(
                planted_target,
                upper_snapshot.global.translation(),
                terrain_maximum_reach(upper_length, lower_length),
            );
            horizontal_target.x = planted_target.x;
            horizontal_target.z = planted_target.z;
            // Reach limiting may have moved the target into another triangle.
            // Resample that actual point instead of retaining a height from the
            // old XZ and a normal from the new one.
            let Some(height) = terrain.height_at(horizontal_target.xz()) else {
                continue;
            };
            planted_target.y = height + sole_offset;
            plant = plant.map(|_| horizontal_target);
            if left {
                memory.left_foot_plant = plant;
            } else {
                memory.right_foot_plant = plant;
            }
            // Sparse authored locomotion poses can move the swing foot much
            // farther than one rendered frame should permit when support is
            // released. Follow that desired pose at a bounded velocity so the
            // final IK target cannot teleport, while still converging all the
            // way back to the unconstrained authored swing during flight.
            // Keep the foot fully pinned throughout the viewer's supported
            // interval, then ease it into authored swing. Blending directly by
            // the raw support weight began dragging a nominally planted foot
            // as soon as confidence dipped below one.
            let solve_weight = smoothstep(0.05, 0.9, weight) * terrain_blend;
            let mut desired_target = foot_position.lerp(planted_target, solve_weight);
            // An unloaded sparse swing pose can dip below uneven terrain,
            // especially when the forward gait is reused in reverse. Preserve
            // exact stance contact while giving the free foot a small
            // support-weighted clearance floor.
            desired_target.y = desired_target
                .y
                .max(planted_target.y + 0.05 * (1.0 - solve_weight));
            let desired_owner_target = rig_rotation.inverse() * (desired_target - rig_origin);
            let (previous_owner_target, previous_support, mut release_active) = if left {
                (
                    memory.left_foot_target,
                    memory.left_support_weight,
                    memory.left_release_active,
                )
            } else {
                (
                    memory.right_foot_target,
                    memory.right_support_weight,
                    memory.right_release_active,
                )
            };
            if let Some(previous_support) = previous_support {
                if weight + 0.001 < previous_support {
                    release_active = true;
                } else if weight > previous_support + 0.001 {
                    // Normal contact acquisition is already close enough to
                    // lock in one tick. A hard stop can instead change both
                    // support weights from zero to one while the authored idle
                    // foot is far away; keep that exceptional acquisition
                    // bounded rather than teleporting to the new plant.
                    let maximum_step = MAX_FOOT_TARGET_SPEED * state_delta_seconds.max(0.0);
                    release_active = previous_owner_target.is_some_and(|previous| {
                        previous.distance(desired_owner_target) > maximum_step + 0.001
                    });
                }
            }
            let maximum_step = MAX_FOOT_TARGET_SPEED * state_delta_seconds.max(0.0);
            if previous_owner_target.is_some_and(|previous| {
                previous.distance(desired_owner_target) > maximum_step + 0.001
            }) {
                // Reach correction can move a nominally planted target when a
                // sharp turn carries the hip past it. Bound that correction
                // just like a sparse authored swing or hard-stop acquisition.
                release_active = true;
            }
            let owner_target = if release_active {
                advance_foot_target(
                    previous_owner_target,
                    desired_owner_target,
                    state_delta_seconds,
                )
            } else {
                desired_owner_target
            };
            if owner_target.distance_squared(desired_owner_target) <= 0.000001 {
                release_active = false;
            }
            if left {
                memory.left_foot_target = Some(owner_target);
                if state_delta_seconds > 0.0 || memory.left_support_weight.is_none() {
                    memory.left_support_weight = Some(weight);
                }
                memory.left_release_active = release_active;
            } else {
                memory.right_foot_target = Some(owner_target);
                if state_delta_seconds > 0.0 || memory.right_support_weight.is_none() {
                    memory.right_support_weight = Some(weight);
                }
                memory.right_release_active = release_active;
            }
            let mut target = rig_origin + rig_rotation * owner_target;
            if let Some(height) = terrain.height_at(target.xz()) {
                target.y = target.y.max(
                    height
                        + MEASURED_ANKLE_SOLE_OFFSET_METRES
                        + SWING_SOLE_CLEARANCE_METRES * (1.0 - solve_weight),
                );
            }
            if left {
                memory.left_foot_world_target = Some(target);
            } else {
                memory.right_foot_world_target = Some(target);
            }
            let canonical_pole = canonical_knee_pole(side);
            let canonical_world = pole_to_world(rig_rotation, canonical_pole);
            let remembered = if left {
                memory.left_terrain_pole_world
            } else {
                memory.right_terrain_pole_world
            }
            .filter(|pole| pole.dot(canonical_world) > 0.2);
            let pole = remembered.unwrap_or(canonical_world);
            let solution = if skeleton.posture() == Posture::Crouched {
                solve_two_bone_preserving_with_reach(
                    upper_snapshot.global.translation(),
                    lower_snapshot.global.translation(),
                    foot_position,
                    target,
                    upper_length,
                    lower_length,
                    pole,
                    terrain_maximum_reach(upper_length, lower_length),
                )
            } else {
                solve_two_bone(
                    upper_snapshot.global.translation(),
                    lower_snapshot.global.translation(),
                    foot_position,
                    target,
                    upper_length,
                    lower_length,
                    pole,
                )
            };
            if let Some(solution) = solution {
                apply_two_bone_solution(upper, lower, foot, solution, &parents, &mut transforms);
                let bend = (solution.knee - upper_snapshot.global.translation())
                    .reject_from_normalized(solution.end_direction);
                if state_delta_seconds > 0.0
                    && let Some(valid) = bend.try_normalize()
                {
                    if left {
                        memory.left_terrain_pole_world = Some(valid);
                    } else {
                        memory.right_terrain_pole_world = Some(valid);
                    }
                }
            }
            if solve_weight > 0.001
                && let Some(normal) = terrain.normal_at(horizontal_target.xz())
                && let Some(sole_axis) = rig.sole_axis(left)
            {
                align_foot_to_slope(
                    foot,
                    sole_axis,
                    normal,
                    solve_weight,
                    &parents,
                    &mut transforms,
                );
            }
        }
        if let Ok(mut state) = ik_states.get_mut(owner) {
            state.0 = memory;
        } else {
            commands.entity(owner).insert(LegIkState(memory));
        }
    }
}

pub(super) fn raised_footwork_posture_is_valid(skeleton: &SkeletonState) -> bool {
    skeleton.is_grounded() && skeleton.posture() == Posture::Upright
}

pub(super) fn terrain_ik_posture_is_valid(skeleton: &SkeletonState) -> bool {
    skeleton.is_grounded()
        && matches!(skeleton.posture(), Posture::Upright | Posture::Crouched)
        && skeleton.action_kind() == SkeletonAction::None
}

pub(super) fn terrain_leg_has_support(weight: f32) -> bool {
    weight > 0.05
}

fn terrain_maximum_reach(upper_length: f32, lower_length: f32) -> f32 {
    (upper_length * upper_length
        + lower_length * lower_length
        + 2.0 * upper_length * lower_length * MIN_TERRAIN_KNEE_FLEXION.cos())
    .sqrt()
}

/// World-space plant confidence used by diagnostics. Procedural guard movement
/// has exactly one support foot while the other follows its clearance arc.
pub(crate) fn locomotion_support_weights(skeleton: &SkeletonState) -> (f32, f32) {
    let speed = skeleton.animation_speed();
    if !skeleton.is_grounded() || skeleton.action_kind() != SkeletonAction::None {
        return (0.0, 0.0);
    }
    if speed <= 0.05 {
        return (1.0, 1.0);
    }
    if skeleton.weapon_guard() == WeaponGuardState::Raised
        && skeleton.action_kind() == SkeletonAction::None
        && skeleton.raised_locomotion().is_moving()
    {
        let swing_left = skeleton.raised_locomotion().swing_foot() == Some(LeadFoot::Left);
        ((!swing_left) as u8 as f32, swing_left as u8 as f32)
    } else {
        let (left, right) = gait_support_weights(locomotion_profile(skeleton), skeleton.gait_phase);
        let moving = smoothstep(0.05, 0.75, speed);
        (1.0 - (1.0 - left) * moving, 1.0 - (1.0 - right) * moving)
    }
}

pub(super) fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn anatomical_side(rig_rotation: Quat, rig_origin: Vec3, hip: Vec3, left: bool) -> f32 {
    let hip_x = (rig_rotation.inverse() * (hip - rig_origin)).x;
    if hip_x.abs() > 0.001 {
        hip_x.signum()
    } else if left {
        -1.0
    } else {
        1.0
    }
}

pub(super) fn constrain_foot_to_track(
    world: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
    side: f32,
) -> Vec3 {
    let mut local = rig_rotation.inverse() * (world - rig_origin);
    let signed_x = (local.x * side).clamp(FOOT_TRACK_INNER, FOOT_TRACK_OUTER);
    local.x = signed_x * side;
    rig_origin + rig_rotation * local
}

pub(super) fn plan_guard_step_endpoint(
    step_origin: Vec3,
    step_rotation: Quat,
    mut stance_local: Vec3,
    local_direction: Vec2,
    step_length: f32,
    left: bool,
    opposite_plant: Vec3,
) -> Vec3 {
    // Cascadeur's authored lateral axis is opposite the conventional Bevy
    // anatomical assumption. Derive the corridor from the actual pose rather
    // than assigning a sign from the semantic bone name.
    let side = if stance_local.x.abs() > 0.001 {
        stance_local.x.signum()
    } else if left {
        -1.0
    } else {
        1.0
    };
    let lateral_travel = local_direction.x * step_length;
    let authored_track = (stance_local.x * side)
        .abs()
        .clamp(FOOT_TRACK_INNER, FOOT_TRACK_OUTER);
    let moving_toward_side = lateral_travel * side > 0.001;
    let mut track = if lateral_travel.abs() <= 0.001 {
        authored_track
    } else if moving_toward_side {
        (lateral_travel.abs() + FOOT_TRACK_INNER).min(FOOT_TRACK_OUTER)
    } else {
        FOOT_TRACK_INNER
    };
    let future_origin = step_origin
        + step_rotation * Vec3::new(local_direction.x, 0.0, local_direction.y) * step_length;
    let opposite_local = step_rotation.inverse() * (opposite_plant - future_origin);
    // Separation is an anatomical lateral-track contract. Fore/aft spacing
    // must not be credited toward it or feet can converge onto one tightrope.
    let separation_track = opposite_local.x * side + GUARD_TARGET_INTER_FOOT_SEPARATION;
    track = track
        .max(separation_track)
        .clamp(FOOT_TRACK_INNER, FOOT_TRACK_OUTER);
    stance_local.x = track * side;
    future_origin + step_rotation * stance_local
}

pub(super) fn guard_step_sequence_delta(previous: u32, current: u32) -> u32 {
    current.wrapping_sub(previous)
}

pub(super) fn constrain_guard_swing_to_live_corridor(
    target: Vec3,
    support: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
    side: f32,
) -> Vec3 {
    let mut local = rig_rotation.inverse() * (target - rig_origin);
    let support_local = rig_rotation.inverse() * (support - rig_origin);
    let required_track = support_local.x * side + GUARD_TARGET_INTER_FOOT_SEPARATION;
    let signed_track = (local.x * side)
        .max(FOOT_TRACK_INNER)
        .max(required_track)
        .min(FOOT_TRACK_OUTER);
    local.x = signed_track * side;
    rig_origin + rig_rotation * local
}

pub(super) fn terrain_conformed_guard_target(
    mut flat_target: Vec3,
    terrain_height: Option<f32>,
) -> Vec3 {
    if let Some(height) = terrain_height {
        flat_target.y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
    }
    flat_target
}

fn align_foot_to_slope(
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
    let Some(normal) = normal.try_normalize() else {
        return;
    };
    let current_up = snapshot.global.rotation() * sole_up_local;
    let angle = current_up.angle_between(normal).min(28.0_f32.to_radians()) * weight;
    let axis = current_up.cross(normal).try_normalize();
    let Some(axis) = axis else { return };
    let world = Quat::from_axis_angle(axis, angle) * snapshot.global.rotation();
    let local = snapshot.parent_rotation.inverse() * world;
    if local.is_finite()
        && let Ok(mut transform) = transforms.p1().get_mut(foot)
    {
        transform.rotation = local.normalize();
    }
}
