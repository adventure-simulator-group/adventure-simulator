//! Knee-pole, anatomical-yaw, slope, airborne-foot, and final rotation policy.

use super::*;

pub(super) const KNEE_POLE_MAX_FOOT_FACING_OFFSET_RADIANS: f32 = std::f32::consts::FRAC_PI_8;
// A 576 degree/second cap is nine degrees at the 64 Hz presentation cadence,
// retaining numeric margin below the ten-degree review gate. Contact and swing
// orientation share this boundary so terrain alignment can never introduce
// the old one-frame ankle snap.
pub(super) const AIRBORNE_FOOT_ROTATION_SPEED_DEGREES: f32 = 576.0;
pub(super) const FIRST_RUN_RELEASE_FOOT_ROTATION_SPEED_DEGREES: f32 = 0.0;

pub(in crate::animation::procedural) fn authored_knee_pole_world(
    hip: Vec3,
    authored_knee: Vec3,
    target: Vec3,
    canonical: Vec3,
) -> Option<Vec3> {
    let target_direction = (target - hip).try_normalize()?;
    let bend = (authored_knee - hip).reject_from_normalized(target_direction);
    bend.try_normalize()
        .filter(|pole| pole.dot(canonical) > 0.2)
}

pub(in crate::animation::procedural) fn retained_terrain_pole(
    remembered: Vec3,
    canonical: Vec3,
) -> Option<Vec3> {
    let remembered = remembered.try_normalize()?;
    // The old 0.2 cutoff discarded a still-valid shallow bend during the
    // support-confidence ramp and rebuilt the knee from authored FK one tick
    // later. Owner/mode discontinuities explicitly clear this cache, so any
    // finite pole in the anatomical hemisphere remains authoritative here.
    (remembered.dot(canonical) > 0.0).then_some(remembered)
}

pub(in crate::animation::procedural) fn transported_terrain_pole(
    remembered: Option<Vec3>,
    previous_end_direction: Option<Vec3>,
    next_end_direction: Vec3,
    canonical: Vec3,
) -> Option<Vec3> {
    let remembered = remembered?.try_normalize()?;
    let Some(previous) = previous_end_direction else {
        return retained_terrain_pole(remembered, canonical);
    };
    let previous = previous.try_normalize()?;
    let next = next_end_direction.try_normalize()?;
    (Quat::from_rotation_arc(previous, next) * remembered).try_normalize()
}

/// Keeps a leg's authored bend plane attached to the hip-to-foot direction.
///
/// Overgrowth's leg solve rotates the animated knee, ankle, and foot together
/// when the IK target moves, which transports the authored knee plane instead
/// of selecting a fresh world-space pole every frame. Our analytic solver does
/// the equivalent explicitly: parallel-transport the last rendered bend, fall
/// back to the current authored bend, and reject either if it crosses the
/// anatomical hemisphere. The canonical pole is only the final singularity
/// fallback.
pub(in crate::animation::procedural) fn stabilized_knee_pole(
    remembered_bend: Option<Vec3>,
    previous_end_direction: Option<Vec3>,
    hip: Vec3,
    authored_knee: Vec3,
    target: Vec3,
    canonical_world: Vec3,
    foot_facing: Option<Vec3>,
) -> Option<Vec3> {
    let next_end_direction = (target - hip).try_normalize()?;
    let canonical_bend = canonical_world
        .reject_from_normalized(next_end_direction)
        .try_normalize()
        .or_else(|| canonical_world.try_normalize())?;
    let in_anatomical_hemisphere = |bend: Vec3| {
        let bend = bend
            .reject_from_normalized(next_end_direction)
            .try_normalize()?;
        let alignment = bend.dot(canonical_bend);
        if alignment >= 0.05 {
            Some(bend)
        } else {
            // Correct continuously at the boundary instead of discarding the
            // remembered pole and selecting an unrelated fallback next tick.
            (bend + canonical_bend * (0.05 - alignment)).try_normalize()
        }
    };

    let transported = remembered_bend
        .and_then(|bend| {
            let bend = bend.try_normalize()?;
            previous_end_direction.map_or(Some(bend), |previous| {
                let previous = previous.try_normalize()?;
                (Quat::from_rotation_arc(previous, next_end_direction) * bend).try_normalize()
            })
        })
        .and_then(in_anatomical_hemisphere);
    let authored = (authored_knee - hip)
        .reject_from_normalized(next_end_direction)
        .try_normalize()
        .and_then(in_anatomical_hemisphere);

    let selected = transported.or(authored).unwrap_or(canonical_bend);
    foot_facing
        .and_then(|facing| {
            constrain_knee_pole_to_foot_facing(
                selected,
                next_end_direction,
                facing,
                KNEE_POLE_MAX_FOOT_FACING_OFFSET_RADIANS,
            )
        })
        .or(Some(selected))
}

pub(in crate::animation::procedural) fn constrain_knee_pole_to_foot_facing(
    pole: Vec3,
    leg_direction: Vec3,
    foot_facing: Vec3,
    maximum_offset_radians: f32,
) -> Option<Vec3> {
    let leg_direction = leg_direction.try_normalize()?;
    let facing_yaw = foot_facing.xz().try_normalize()?;
    let pole_yaw = pole.xz().try_normalize().unwrap_or(facing_yaw);
    let signed_offset = facing_yaw
        .perp_dot(pole_yaw)
        .atan2(facing_yaw.dot(pole_yaw));
    let clamped_offset = signed_offset.clamp(-maximum_offset_radians, maximum_offset_radians);
    let (sin, cos) = clamped_offset.sin_cos();
    let clamped_yaw = Vec2::new(
        facing_yaw.x * cos - facing_yaw.y * sin,
        facing_yaw.x * sin + facing_yaw.y * cos,
    );

    // Preserve the clamped ground-plane yaw exactly, then choose the vertical
    // component that makes the pole perpendicular to the hip-to-foot axis.
    // Clamping only after projecting into that plane does not bound yaw for a
    // diagonal leg and was the source of visibly sideways knees.
    if leg_direction.y.abs() <= 0.0001 {
        return None;
    }
    let vertical = -clamped_yaw.dot(leg_direction.xz()) / leg_direction.y;
    Vec3::new(clamped_yaw.x, vertical, clamped_yaw.y).try_normalize()
}

/// Applies the anatomical knee-yaw invariant at the final leg-solve boundary.
///
/// Individual pose owners may transport, preserve, or reconstruct their pole
/// differently, but every valid humanoid leg has the same hard constraint:
/// its effective pole stays within the foot-facing cone. Keeping this wrapper
/// beside the raw constraint prevents ordinary terrain and landing paths from
/// bypassing the combat-specific stabilizer.
#[expect(
    clippy::too_many_arguments,
    reason = "the final leg-pole constraint keeps the anatomical geometry and hierarchy readers explicit"
)]
pub(in crate::animation::procedural) fn constrain_rendered_leg_pole(
    rig: &HumanoidRig,
    left: bool,
    hip: Vec3,
    foot_position: Vec3,
    target: Vec3,
    pole: Vec3,
    parents: &Query<&ChildOf>,
    transforms: &TransformHelper,
) -> Vec3 {
    rendered_foot_facing(rig, left, foot_position, parents, transforms)
        .and_then(|facing| {
            constrain_knee_pole_to_foot_facing(
                pole,
                target - hip,
                facing,
                KNEE_POLE_MAX_FOOT_FACING_OFFSET_RADIANS,
            )
        })
        // Sparse/non-humanoid rigs may not expose a toe direction. Preserve
        // their previous graceful fallback; canonical humanoids always take
        // the constrained branch.
        .unwrap_or(pole)
}

pub(in crate::animation::procedural) fn rendered_foot_facing(
    rig: &HumanoidRig,
    left: bool,
    foot_position: Vec3,
    parents: &Query<&ChildOf>,
    transforms: &TransformHelper,
) -> Option<Vec3> {
    let foot = *rig.get(if left {
        &BoneRole::FootLeft
    } else {
        &BoneRole::FootRight
    })?;
    let toe = *rig.get(if left {
        &BoneRole::ToeLeft
    } else {
        &BoneRole::ToeRight
    })?;
    let foot_rotation = snapshot(foot, parents, transforms)?.global.rotation();
    let toe_position = snapshot(toe, parents, transforms)?.global.translation();
    let toe_direction = (toe_position - foot_position).try_normalize()?;

    // Toe-to-ankle projected directly onto the ground reverses yaw when a
    // running or slope-aligned foot pitches through vertical. Recover yaw
    // from the pitch-stable lateral axis instead: forward cross sole-up gives
    // anatomical right, and world-up cross right gives horizontal forward.
    // This preserves the direction the foot is facing even at heel/toe roll.
    if let Some(sole_up) = rig
        .sole_axis(left)
        .map(|axis| foot_rotation * axis)
        .and_then(Vec3::try_normalize)
        && let Some(lateral) = toe_direction.cross(sole_up).try_normalize()
        && let Some(facing) = Vec3::Y.cross(lateral).xz().try_normalize()
    {
        return Some(Vec3::new(facing.x, 0.0, facing.y));
    }

    toe_direction
        .xz()
        .try_normalize()
        .map(|facing| Vec3::new(facing.x, 0.0, facing.y))
}

pub(in crate::animation::procedural) fn projected_body_center(
    rig: &HumanoidRig,
    transforms: &TransformHelper,
) -> Option<Vec3> {
    let mut weighted = Vec3::ZERO;
    let mut total = 0.0;
    for (role, weight) in [
        (BoneRole::Pelvis, 0.45),
        (BoneRole::Chest, 0.35),
        (BoneRole::Head, 0.20),
    ] {
        let Some(&bone) = rig.get(&role) else {
            continue;
        };
        let Ok(global) = transforms.compute_global_transform(bone) else {
            continue;
        };
        weighted += global.translation() * weight;
        total += weight;
    }
    (total > 0.0).then_some(weighted / total)
}

pub(in crate::animation::procedural) fn align_foot_to_slope(
    foot: Entity,
    sole_up_local: Vec3,
    normal: Vec3,
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    let Some(snapshot) = snapshot(foot, parents, &transforms.p0()) else {
        return;
    };
    let world = slope_aligned_world_rotation(snapshot.global.rotation(), sole_up_local, normal);
    let Some(world) = world else { return };
    let Some(local) = local_rotation_for_world(snapshot.parent_rotation, world) else {
        return;
    };
    if let Ok(mut transform) = transforms.p1().get_mut(foot) {
        transform.rotation = local;
    }
}

pub(in crate::animation::procedural) fn advance_airborne_foot_rotation(
    previous: Option<Quat>,
    desired: Quat,
    delta_seconds: f32,
    maximum_speed_degrees: f32,
) -> Quat {
    let Some(previous) = previous.filter(|rotation| rotation.is_finite()) else {
        return desired;
    };
    if !desired.is_finite() {
        return previous;
    }
    let angle = previous.angle_between(desired);
    let maximum_step = maximum_speed_degrees.max(0.0).to_radians() * delta_seconds.max(0.0);
    if maximum_step <= f32::EPSILON {
        return previous;
    }
    if angle <= maximum_step || angle <= f32::EPSILON {
        desired
    } else {
        previous.slerp(desired, maximum_step / angle).normalize()
    }
}

pub(in crate::animation::procedural) fn previous_airborne_foot_orientation(
    analytic_previous: Option<Quat>,
    propagated_previous: Option<Quat>,
    just_released: bool,
) -> Option<Quat> {
    if just_released {
        // The pre-propagation analytic orientation can differ from the foot
        // orientation that the player saw after the full hierarchy settled.
        // Toe-off begins from that propagated pose so a nominally stationary
        // ankle cannot lever the toe through the continuity budget.
        propagated_previous.or(analytic_previous)
    } else {
        analytic_previous
    }
}

/// Phase-aware sagittal foot roll for running. Negative phase is the approach
/// to this foot's contact and positive phase is its stance/release. The curve
/// arrives with a modest dorsiflexed heel presentation, flattens early in
/// stance, then plantar-flexes into toe-off before returning to neutral during
/// swing. Terrain-normal alignment remains the base orientation.
pub(in crate::animation::procedural) fn run_foot_roll_degrees(
    skeleton: &SkeletonState,
    left: bool,
) -> f32 {
    if locomotion_profile(skeleton).gait != LocomotionGait::Run
        || skeleton.action_kind() != SkeletonAction::None
        || skeleton.weapon_guard() != WeaponGuardState::Lowered
        || skeleton.animation_speed() <= 0.05
    {
        return 0.0;
    }
    let contact = if left { 0.0 } else { 0.5 };
    let signed = (skeleton.gait_phase - contact + 0.5).rem_euclid(1.0) - 0.5;
    let radius = locomotion_profile(skeleton).support_phase_radius;
    if signed < -radius {
        // Prepare the heel during the latter half of flight.
        8.0 * smoothstep(-0.25, -radius, signed)
    } else if signed < -0.05 {
        8.0 * (1.0 - smoothstep(-radius, -0.05, signed))
    } else if signed <= 0.06 {
        0.0
    } else if signed <= radius {
        -8.0 * smoothstep(0.06, radius, signed)
    } else {
        // Release the toe smoothly instead of carrying a pointed foot through
        // the whole airborne arc.
        -8.0 * (1.0 - smoothstep(radius, 0.25, signed))
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the final leg rotation boundary keeps pose state, airborne transitions, and hierarchy access explicit"
)]
pub(in crate::animation::procedural) fn finalize_leg_rotation_chains(
    rig: &HumanoidRig,
    skeleton: &SkeletonState,
    rig_rotation: Quat,
    memory: &mut LegIkMemory,
    evaluation_advances: bool,
    delta_seconds: f32,
    airborne_orientation_owned: [bool; 2],
    airborne_just_released: [bool; 2],
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    for (leg_index, (upper_role, lower_role, foot_role, left)) in [
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
    ]
    .into_iter()
    .enumerate()
    {
        let (Some(&upper), Some(&lower), Some(&foot)) = (
            rig.get(&upper_role),
            rig.get(&lower_role),
            rig.get(&foot_role),
        ) else {
            continue;
        };
        let current = {
            let query = transforms.p1();
            let (Ok(upper), Ok(lower), Ok(foot)) =
                (query.get(upper), query.get(lower), query.get(foot))
            else {
                continue;
            };
            LegRotationChain {
                upper: upper.rotation,
                lower: lower.rotation,
                foot: foot.rotation,
            }
        };
        let cached = if left {
            memory.left_rotation_chain
        } else {
            memory.right_rotation_chain
        };
        let contact_blend_active = if left {
            memory.left_contact_orientation_blend_active
        } else {
            memory.right_contact_orientation_blend_active
        };
        let mut resolved = final_leg_rotation_chain(cached, current, evaluation_advances);
        {
            let mut query = transforms.p1();
            if let Ok(mut transform) = query.get_mut(upper) {
                transform.rotation = resolved.upper;
            }
            if let Ok(mut transform) = query.get_mut(lower) {
                transform.rotation = resolved.lower;
            }
            if let Ok(mut transform) = query.get_mut(foot) {
                transform.rotation = resolved.foot;
            }
        }
        if evaluation_advances
            && let Some(foot_snapshot) = snapshot(foot, parents, &transforms.p0())
        {
            let base_world = foot_snapshot.global.rotation();
            let roll_degrees = run_foot_roll_degrees(skeleton, left);
            let desired_world = if roll_degrees.abs() > f32::EPSILON {
                let lateral = (rig_rotation * Vec3::X).normalize_or_zero();
                Quat::from_axis_angle(lateral, roll_degrees.to_radians()) * base_world
            } else {
                base_world
            };
            let previous_world = if left {
                previous_airborne_foot_orientation(
                    memory.left_foot_orientation_world,
                    memory.left_last_rendered_foot_rotation_world,
                    airborne_just_released[leg_index],
                )
            } else {
                previous_airborne_foot_orientation(
                    memory.right_foot_orientation_world,
                    memory.right_last_rendered_foot_rotation_world,
                    airborne_just_released[leg_index],
                )
            };
            let final_world = if airborne_orientation_owned[leg_index] || contact_blend_active {
                let angular_speed = if locomotion_profile(skeleton).gait == LocomotionGait::Run
                    && airborne_just_released[leg_index]
                {
                    FIRST_RUN_RELEASE_FOOT_ROTATION_SPEED_DEGREES
                } else {
                    AIRBORNE_FOOT_ROTATION_SPEED_DEGREES
                };
                let bounded_world = advance_airborne_foot_rotation(
                    previous_world,
                    desired_world,
                    delta_seconds,
                    angular_speed,
                );
                if let Some(local) =
                    local_rotation_for_world(foot_snapshot.parent_rotation, bounded_world)
                {
                    if let Ok(mut transform) = transforms.p1().get_mut(foot) {
                        transform.rotation = local;
                    }
                    resolved.foot = local;
                }
                bounded_world
            } else {
                desired_world
            };
            if contact_blend_active
                && final_world.angle_between(desired_world) <= 0.001_f32.to_radians()
            {
                if left {
                    memory.left_contact_orientation_blend_active = false;
                } else {
                    memory.right_contact_orientation_blend_active = false;
                }
            }
            if left {
                memory.left_foot_orientation_world = Some(final_world);
            } else {
                memory.right_foot_orientation_world = Some(final_world);
            }
        }
        if left {
            memory.left_rotation_chain = Some(resolved);
        } else {
            memory.right_rotation_chain = Some(resolved);
        }
    }
}

pub(in crate::animation::procedural) fn final_leg_rotation_chain(
    cached: Option<LegRotationChain>,
    current: LegRotationChain,
    evaluation_advances: bool,
) -> LegRotationChain {
    if evaluation_advances {
        current
    } else {
        cached.unwrap_or(current)
    }
}

pub(in crate::animation::procedural) fn local_rotation_for_world(
    parent_world: Quat,
    desired_world: Quat,
) -> Option<Quat> {
    let local = parent_world.inverse() * desired_world;
    if local.is_finite() {
        Some(local.normalize())
    } else {
        None
    }
}

pub(in crate::animation::procedural) fn clear_slope_rotation_cache(memory: &mut LegIkMemory) {
    memory.left_rotation_chain = None;
    memory.right_rotation_chain = None;
    memory.slope_alignment_mode = None;
}

pub(in crate::animation::procedural) fn prepare_slope_rotation_cache(
    memory: &mut LegIkMemory,
    mode: SlopeAlignmentMode,
) {
    if memory.slope_alignment_mode != Some(mode) {
        clear_slope_rotation_cache(memory);
        memory.slope_alignment_mode = Some(mode);
    }
}

pub(in crate::animation::procedural) fn slope_aligned_world_rotation(
    current_world: Quat,
    sole_up_local: Vec3,
    terrain_normal: Vec3,
) -> Option<Quat> {
    let normal = terrain_normal.try_normalize()?;
    let tilt_angle = Vec3::Y.angle_between(normal).min(28.0_f32.to_radians());
    let bounded_normal = Vec3::Y
        .cross(normal)
        .try_normalize()
        .map_or(Vec3::Y, |axis| {
            Quat::from_axis_angle(axis, tilt_angle) * Vec3::Y
        });
    let current_up = (current_world * sole_up_local).try_normalize()?;
    let correction = Quat::from_rotation_arc(current_up, bounded_normal);
    Some((correction * current_world).normalize())
}

/// Final lower-body invariant pass. Pose owners and terrain alignment may
/// choose different targets and foot rotations, but no later presentation
/// stage may leave a rendered knee outside the foot-facing anatomical cone.
pub(in crate::animation) fn enforce_anatomical_knee_yaw(
    owners: Query<&PresentedSkeleton>,
    rigs: Query<(Entity, &HumanoidRig)>,
    parents: Query<&ChildOf>,
    mut states: Query<&mut LegIkState>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    for (owner, rig) in &rigs {
        let Ok(skeleton) = owners.get(owner) else {
            continue;
        };
        // Downed and posture-transition poses deliberately use knee bend
        // planes that need not follow a standing foot-facing cone. Preserve
        // their authored thigh/shin relationship exactly; terrain IK already
        // rejects these postures independently.
        if !anatomical_knee_yaw_posture_is_valid(skeleton) {
            continue;
        }
        let mut final_offsets = [0.0; 2];
        let (rig_origin, rig_rotation) = rig
            .rig_scene()
            .and_then(|entity| transforms.p0().compute_global_transform(entity).ok())
            .map(|global| (global.translation(), global.rotation()))
            .unwrap_or((Vec3::ZERO, Quat::IDENTITY));
        for (leg_index, (upper_role, lower_role, foot_role, left)) in [
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
        ]
        .into_iter()
        .enumerate()
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
            let hip = upper_snapshot.global.translation();
            let knee = lower_snapshot.global.translation();
            let target = foot_snapshot.global.translation();
            let upper_length = hip.distance(knee);
            let lower_length = knee.distance(target);
            let leg_direction = (target - hip).normalize_or_zero();
            let side = anatomical_side(rig_rotation, rig_origin, hip, left);
            let canonical = pole_to_world(rig_rotation, canonical_knee_pole(side));
            let current_bend = (knee - hip)
                .reject_from_normalized(leg_direction)
                .try_normalize()
                .unwrap_or(canonical);
            let pole = constrain_rendered_leg_pole(
                rig,
                left,
                hip,
                target,
                target,
                current_bend,
                &parents,
                &transforms.p0(),
            );
            if let Some(solution) = solve_two_bone_with_reach(
                TwoBoneChain::new(hip, knee, target, upper_length, lower_length, pole),
                target,
                maximum_reach(upper_length, lower_length),
            ) {
                apply_two_bone_solution(upper, lower, foot, solution, &parents, &mut transforms);
                if let Some((final_upper, final_lower, final_foot)) =
                    snapshot_chain(upper, lower, foot, &parents, &transforms.p0())
                    && let Some(end_direction) = (final_foot.global.translation()
                        - final_upper.global.translation())
                    .try_normalize()
                    && let Some(bend) = (final_lower.global.translation()
                        - final_upper.global.translation())
                    .reject_from_normalized(end_direction)
                    .xz()
                    .try_normalize()
                    && let Some(facing) = rendered_foot_facing(
                        rig,
                        left,
                        final_foot.global.translation(),
                        &parents,
                        &transforms.p0(),
                    )
                    .and_then(|facing| facing.xz().try_normalize())
                {
                    final_offsets[leg_index] = bend.angle_to(facing).abs().to_degrees();
                }
            }
        }
        if let Ok(mut state) = states.get_mut(owner) {
            state.0.left_knee_foot_yaw_offset_degrees = final_offsets[0];
            state.0.right_knee_foot_yaw_offset_degrees = final_offsets[1];
        }
    }
}

pub(in crate::animation::procedural) fn anatomical_knee_yaw_posture_is_valid(
    skeleton: &SkeletonState,
) -> bool {
    !skeleton.body().is_downed() && !skeleton.is_posture_transitioning() && !skeleton.is_quickstep()
}
