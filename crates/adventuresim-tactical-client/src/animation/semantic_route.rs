//! Direct deterministic semantic routing for pose-buffer playback.
//!
//! Authoritative presentation state is converted directly into the semantic
//! pose samples consumed by the pose buffer.

use std::collections::BTreeMap;

use adventuresim_tactical_core::prelude::*;
use bevy::prelude::*;

use super::{AnimationRuntime, PresentedSkeleton};

/// Read-only coordinates presented to the semantic pose router. Every field
/// comes from client presentation state or its pure semantic evaluation.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct SemanticRouteInputs {
    pub speed: f32,
    pub direction: Vec2,
    pub gait_phase: f32,
    pub action: SkeletonAction,
    pub airborne: bool,
    pub target_height: f32,
    pub lead: LeadFoot,
    pub support: LeadFoot,
    pub contact_sequence: u64,
    pub pack: String,
}

impl SemanticRouteInputs {
    pub(crate) fn from_presented(
        skeleton: &PresentedSkeleton,
        evaluation: &AnimationEvaluation,
    ) -> Self {
        Self {
            speed: evaluation.movement_speed,
            direction: skeleton.animation_local_velocity().xz().normalize_or_zero(),
            gait_phase: evaluation.gait_phase,
            action: skeleton.action_kind(),
            airborne: !skeleton.is_grounded(),
            target_height: evaluation.attack_target_height,
            lead: skeleton.lead_foot,
            support: skeleton.contact_foot,
            contact_sequence: skeleton.contact_sequence,
            pack: skeleton.animation_pack.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SemanticRoutePath {
    GeneralPose,
    OrdinaryLocomotion,
    RaisedGuardAttack,
}

impl SemanticRoutePath {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::GeneralPose => "general_pose",
            Self::OrdinaryLocomotion => "ordinary_locomotion",
            Self::RaisedGuardAttack => "raised_guard_attack",
        }
    }
}

#[derive(Component, Debug, Clone)]
pub(crate) struct SemanticRouteTrace {
    pub inputs: SemanticRouteInputs,
    pub requested_path: SemanticRoutePath,
    pub path: SemanticRoutePath,
    pub evaluation: AnimationEvaluation,
    pub runtime_evaluated: bool,
}

#[derive(Resource, Debug, Default)]
pub(crate) struct SemanticRouteTelemetry {
    counts: BTreeMap<SemanticRoutePath, u64>,
}

fn requested_path(skeleton: &PresentedSkeleton) -> SemanticRoutePath {
    if skeleton.action_kind() == SkeletonAction::Attack
        || skeleton.weapon_guard() == WeaponGuardState::Raised
    {
        SemanticRoutePath::RaisedGuardAttack
    } else if skeleton.is_grounded() && skeleton.posture() == Posture::Upright {
        SemanticRoutePath::OrdinaryLocomotion
    } else {
        SemanticRoutePath::GeneralPose
    }
}

pub(super) fn route_semantic_pose(skeleton: &PresentedSkeleton) -> SemanticRouteTrace {
    let evaluation = AnimationEvaluation::from_skeleton(skeleton);
    let inputs = SemanticRouteInputs::from_presented(skeleton, &evaluation);
    let path = requested_path(skeleton);
    SemanticRouteTrace {
        inputs,
        requested_path: path,
        path,
        evaluation,
        runtime_evaluated: true,
    }
}

pub(super) fn evaluate_semantic_route_paths(
    mut commands: Commands,
    mut telemetry: ResMut<SemanticRouteTelemetry>,
    runtime: Res<AnimationRuntime>,
    players: Query<(Entity, &PresentedSkeleton, Option<&InventoryItems>), With<Player>>,
    items: Query<&ItemProperties, With<WeaponItem>>,
) {
    for (entity, skeleton, inventory) in &players {
        let mut resolved = skeleton.clone();
        resolved.state.animation_pack =
            super::equipped_animation_pack(inventory, &items).to_owned();
        resolved.state.attack_animations = runtime
            .library
            .attack_animations(&resolved.state.animation_pack);
        let trace = route_semantic_pose(&resolved);
        *telemetry.counts.entry(trace.path).or_default() += 1;
        commands.entity(entity).insert(trace);
    }
}

#[cfg(test)]
pub(super) fn route_semantic_pose_for_test(
    In(skeleton): In<PresentedSkeleton>,
) -> SemanticRouteTrace {
    route_semantic_pose(&skeleton)
}
