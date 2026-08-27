use super::*;

const MAX_HIP_DROP_METRES: f32 = 0.18;
const SOLE_CONTACT_MARGIN_METRES: f32 = 0.001;

#[derive(Component, Debug, Clone, Copy, Default)]
pub(in crate::animation) struct OrdinaryLocomotionIkState {
    initialized: bool,
    pelvis_shift: f32,
    evaluation_tick: Option<u64>,
}

pub(super) fn owns(skeleton: &SkeletonState) -> bool {
    skeleton.is_grounded()
        && !skeleton.is_posture_transitioning()
        && skeleton.posture() == Posture::Upright
        && skeleton.action_kind() == SkeletonAction::None
        && (skeleton.weapon_guard() == WeaponGuardState::Lowered
            || skeleton.guarded_sprint_locomotion())
}

/// Overgrowth-style ordinary locomotion IK: semantic evaluation supplies the
/// complete FK pose and authored foot weights; this pass only conforms weighted ankles
/// to terrain, applies one shared hip correction, and solves each leg once.
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each independently borrowed locomotion IK resource and query as a system parameter"
)]
pub(in crate::animation) fn apply(
    enabled: Res<super::super::super::TerrainIkEnabled>,
    time: Res<Time>,
    clock: Res<ProceduralAnimationClock>,
    terrain: Query<&SceneTerrain>,
    owners: Query<(&PresentedSkeleton, &AnimationPlayback)>,
    rigs: Query<(Entity, &HumanoidRig)>,
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
        if !owns(skeleton) {
            continue;
        }

        let mut state = states
            .get_mut(owner)
            .map(|state| *state)
            .unwrap_or_default();
        let delta_seconds = match clock.fixed_step() {
            Some((tick, _)) if state.evaluation_tick == Some(tick) => 0.0,
            Some((tick, delta)) => {
                state.evaluation_tick = Some(tick);
                delta.max(0.0)
            }
            None => time.delta_secs().max(0.0),
        };
        let authored_weights = playback.foot_ik_weights;
        let solve_weights = if enabled.0 && terrain.is_some() {
            authored_weights
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

        for (index, (upper_role, lower_role, foot_role, weight, _)) in
            legs.iter().copied().enumerate()
        {
            let (Some(&upper), Some(&lower), Some(&foot), Some(terrain)) = (
                rig.get(&upper_role),
                rig.get(&lower_role),
                rig.get(&foot_role),
                terrain,
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
            let Some(height) = terrain.height_at(foot_position.xz()) else {
                continue;
            };
            let terrain_target = foot_position
                .with_y(height + MEASURED_ANKLE_SOLE_OFFSET_METRES - SOLE_CONTACT_MARGIN_METRES);
            let target = foot_position.lerp(terrain_target, weight.clamp(0.0, 1.0));
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

        state.pelvis_shift = if !state.initialized {
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
            let pole = authored_knee_pole_world(
                upper_snapshot.global.translation(),
                lower_snapshot.global.translation(),
                target,
                canonical,
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
                TwoBoneChain::new(
                    upper_snapshot.global.translation(),
                    lower_snapshot.global.translation(),
                    foot_snapshot.global.translation(),
                    upper_length,
                    lower_length,
                    pole,
                ),
                target,
                maximum_reach(upper_length, lower_length),
            ) {
                apply_two_bone_solution(upper, lower, foot, solution, &parents, &mut transforms);
            }
            if weight > 0.001
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

        let mut memory = LegIkMemory {
            left_authored_world_target: authored[0],
            right_authored_world_target: authored[1],
            left_foot_world_target: targets[0],
            right_foot_world_target: targets[1],
            left_support_weight: Some(authored_weights.x),
            right_support_weight: Some(authored_weights.y),
            pelvis_shift: state.pelvis_shift,
            evaluation_tick: state.evaluation_tick,
            ..default()
        };
        memory.left_foot_target = targets[0];
        memory.right_foot_target = targets[1];
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
}
