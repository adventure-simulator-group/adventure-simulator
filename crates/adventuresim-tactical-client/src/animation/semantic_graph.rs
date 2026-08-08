use std::collections::{BTreeMap, HashSet};

use adventuresim_tactical_core::prelude::*;
use bevy::{
    animation::AnimationTargetId,
    asset::{Assets, Handle},
    platform::collections::HashMap,
    prelude::*,
};
use bevy_animation_graph::core::{
    animation_graph::{
        AnimationGraph as DependencyAnimationGraph, DEFAULT_OUTPUT_POSE, GraphInputPin, NodeId,
        TimeUpdate,
    },
    animation_node::AnimationNode,
    context::{
        deferred_gizmos::DeferredGizmos, graph_context_arena::GraphContextArena,
        io_env::IoOverrides, system_resources::SystemResources,
    },
    edge_data::{DataSpec, DataValue},
    id::BoneId,
    pose::{BonePose, Pose},
};

use crate::animation_graph_nodes::SparseSemanticBlendNode;

use super::PresentedSkeleton;

pub(crate) const MAX_GRAPH_ANCHORS: usize = 16;
const WEIGHT_EPSILON: f32 = 0.0001;

fn pose_pin(index: usize) -> String {
    format!("semantic_pose_{index}")
}

fn factor_pin(index: usize) -> String {
    format!("semantic_factor_{index}")
}

/// Read-only coordinates presented to the semantic graph bridge. Every field
/// comes from client presentation state or its pure semantic evaluation.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct SemanticGraphInputs {
    pub speed: f32,
    pub direction: Vec2,
    pub gait_phase: f32,
    pub action: SkeletonAction,
    pub crouch: f32,
    pub airborne: bool,
    pub target_height: f32,
    pub lead: LeadFoot,
    pub support: LeadFoot,
    pub contact_sequence: u64,
    pub pack: String,
    pub captured_step: AttackStep,
    pub captured_step_direction: Vec2,
    pub captured_step_speed: f32,
}

impl SemanticGraphInputs {
    pub(crate) fn from_presented(
        skeleton: &PresentedSkeleton,
        evaluation: &AnimationEvaluation,
    ) -> Self {
        let (captured_step_direction, captured_step_speed) =
            skeleton.attack_movement().unwrap_or((Vec2::ZERO, 0.0));
        Self {
            speed: evaluation.movement_speed,
            direction: skeleton.animation_local_velocity().xz().normalize_or_zero(),
            gait_phase: evaluation.gait_phase,
            action: skeleton.action_kind(),
            crouch: evaluation.crouch_amount,
            airborne: !skeleton.is_grounded(),
            target_height: evaluation.attack_target_height,
            lead: skeleton.lead_foot,
            support: skeleton.contact_foot,
            contact_sequence: skeleton.contact_sequence,
            pack: skeleton.animation_pack.clone(),
            captured_step: skeleton.attack_step(),
            captured_step_direction,
            captured_step_speed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SemanticGraphPath {
    LegacyFallback,
    OrdinaryLocomotion,
    RaisedGuardAttack,
}

impl SemanticGraphPath {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::LegacyFallback => "legacy_fallback",
            Self::OrdinaryLocomotion => "ordinary_locomotion",
            Self::RaisedGuardAttack => "raised_guard_attack",
        }
    }
}

#[derive(Component, Debug, Clone)]
pub(crate) struct SemanticGraphTrace {
    pub inputs: SemanticGraphInputs,
    pub requested_path: SemanticGraphPath,
    pub path: SemanticGraphPath,
    pub evaluation: AnimationEvaluation,
    pub runtime_evaluated: bool,
}

#[derive(Resource, Debug, Default)]
pub(crate) struct SemanticGraphTelemetry {
    counts: BTreeMap<SemanticGraphPath, u64>,
}

#[derive(Resource)]
pub(crate) struct SemanticGraphLibrary {
    pub(super) ordinary: Handle<DependencyAnimationGraph>,
    pub(super) raised: Handle<DependencyAnimationGraph>,
    contexts: HashMap<(Entity, SemanticGraphPath), GraphContextArena>,
    marker_ids: Vec<BoneId>,
    #[cfg(test)]
    pub(super) factor_override: Option<[f32; MAX_GRAPH_ANCHORS - 1]>,
    #[cfg(test)]
    pub(super) corrupt_last_marker: bool,
}

impl FromWorld for SemanticGraphLibrary {
    fn from_world(world: &mut World) -> Self {
        let mut assets = world.resource_mut::<Assets<DependencyAnimationGraph>>();
        Self {
            ordinary: assets.add(build_semantic_route_graph(
                SemanticGraphPath::OrdinaryLocomotion,
            )),
            raised: assets.add(build_semantic_route_graph(
                SemanticGraphPath::RaisedGuardAttack,
            )),
            contexts: HashMap::new(),
            marker_ids: (0..MAX_GRAPH_ANCHORS).map(marker_id).collect(),
            #[cfg(test)]
            factor_override: None,
            #[cfg(test)]
            corrupt_last_marker: false,
        }
    }
}

/// Central graph definition used by gameplay, tests, and the native editor
/// preflight. Each node performs a genuine dependency Pose linear blend.
pub(crate) fn build_semantic_route_graph(path: SemanticGraphPath) -> DependencyAnimationGraph {
    let mut graph = DependencyAnimationGraph::new();
    for index in 0..MAX_GRAPH_ANCHORS {
        graph.add_input_data(GraphInputPin::passthrough(pose_pin(index)), DataSpec::Pose);
    }
    for index in 1..MAX_GRAPH_ANCHORS {
        graph.add_input_data(GraphInputPin::passthrough(factor_pin(index)), DataSpec::F32);
    }
    graph.add_output_data(DEFAULT_OUTPUT_POSE.into(), DataSpec::Pose);

    let mut previous: Option<NodeId> = None;
    for index in 1..MAX_GRAPH_ANCHORS {
        let node = AnimationNode::new(
            format!("{}-blend-{index}", path.as_str()),
            SparseSemanticBlendNode,
        );
        let node_id = node.id;
        graph.add_node(node);
        match previous {
            Some(previous) => graph.add_node_parameter_edge(
                previous,
                SparseSemanticBlendNode::OUTPUT,
                node_id,
                SparseSemanticBlendNode::POSE_A,
            ),
            None => graph.add_input_data_edge(
                GraphInputPin::passthrough(pose_pin(0)),
                node_id,
                SparseSemanticBlendNode::POSE_A,
            ),
        }
        graph.add_input_data_edge(
            GraphInputPin::passthrough(pose_pin(index)),
            node_id,
            SparseSemanticBlendNode::POSE_B,
        );
        graph.add_input_data_edge(
            GraphInputPin::passthrough(factor_pin(index)),
            node_id,
            SparseSemanticBlendNode::FACTOR,
        );
        previous = Some(node_id);
    }
    graph.add_output_data_edge(
        previous.expect("semantic graph has blend nodes"),
        SparseSemanticBlendNode::OUTPUT,
        DEFAULT_OUTPUT_POSE,
    );
    graph
}

fn marker_id(index: usize) -> BoneId {
    AnimationTargetId::from_names([Name::new(format!("semantic-sample-{index}"))].iter()).into()
}

fn selected_samples(evaluation: &AnimationEvaluation) -> &[PoseSample] {
    if evaluation.action.is_empty() {
        &evaluation.base
    } else {
        &evaluation.action
    }
}

fn selected_samples_mut(evaluation: &mut AnimationEvaluation) -> &mut [PoseSample] {
    if evaluation.action.is_empty() {
        &mut evaluation.base
    } else {
        &mut evaluation.action
    }
}

#[derive(Debug, Clone, Copy)]
enum FlatAnchorRole {
    Anchor { sample: usize },
    SpanStart { sample: usize },
    SpanEnd { sample: usize },
}

fn encode_sparse_inputs(
    evaluation: &AnimationEvaluation,
    marker_ids: &[BoneId],
) -> Option<(Vec<Pose>, [f32; MAX_GRAPH_ANCHORS - 1], Vec<FlatAnchorRole>)> {
    let samples = selected_samples(evaluation);
    let mut flattened = Vec::new();
    for (sample_index, sample) in samples.iter().enumerate() {
        match sample.sampling {
            PoseSampling::Anchor | PoseSampling::Cycle { .. } => flattened.push((
                FlatAnchorRole::Anchor {
                    sample: sample_index,
                },
                sample.weight,
            )),
            PoseSampling::Span { progress, .. } => {
                let progress = progress.clamp(0.0, 1.0);
                flattened.push((
                    FlatAnchorRole::SpanStart {
                        sample: sample_index,
                    },
                    sample.weight * (1.0 - progress),
                ));
                flattened.push((
                    FlatAnchorRole::SpanEnd {
                        sample: sample_index,
                    },
                    sample.weight * progress,
                ));
            }
        }
    }
    if flattened.is_empty()
        || flattened.len() > MAX_GRAPH_ANCHORS
        || flattened
            .iter()
            .any(|(_, weight)| !weight.is_finite() || *weight < 0.0)
    {
        return None;
    }

    let mut poses = Vec::with_capacity(MAX_GRAPH_ANCHORS);
    for active_index in 0..MAX_GRAPH_ANCHORS {
        let mut pose = Pose::default();
        for index in 0..flattened.len() {
            pose.add_bone(
                BonePose {
                    translation: Some(Vec3::X * (index == active_index) as u8 as f32),
                    ..default()
                },
                marker_ids[index],
            );
        }
        poses.push(pose);
    }

    let mut factors = [0.0; MAX_GRAPH_ANCHORS - 1];
    let mut accumulated = flattened[0].1;
    for index in 1..flattened.len() {
        let weight = flattened[index].1;
        let total = accumulated + weight;
        factors[index - 1] = if total > f32::EPSILON {
            weight / total
        } else {
            0.0
        };
        accumulated = total;
    }
    Some((
        poses,
        factors,
        flattened.into_iter().map(|(role, _)| role).collect(),
    ))
}

fn decode_graph_pose(
    legacy: &AnimationEvaluation,
    pose: &Pose,
    marker_ids: &[BoneId],
    roles: &[FlatAnchorRole],
) -> Option<AnimationEvaluation> {
    let mut decoded = legacy.clone();
    let samples = selected_samples_mut(&mut decoded);
    if samples.is_empty() || pose.bones.len() != roles.len() {
        return None;
    }
    let mut start_weights = vec![0.0; samples.len()];
    let mut end_weights = vec![0.0; samples.len()];
    let mut weight_sum = 0.0;
    for (index, role) in roles.iter().enumerate() {
        let marker = pose
            .get_bone(marker_ids[index])
            .and_then(|bone| bone.translation)?;
        if !marker.is_finite()
            || marker.x < -WEIGHT_EPSILON
            || marker.x > 1.0 + WEIGHT_EPSILON
            || marker.y.abs() > WEIGHT_EPSILON
            || marker.z.abs() > WEIGHT_EPSILON
        {
            return None;
        }
        let weight = marker.x.clamp(0.0, 1.0);
        match *role {
            FlatAnchorRole::Anchor { sample } | FlatAnchorRole::SpanStart { sample } => {
                *start_weights.get_mut(sample)? = weight;
            }
            FlatAnchorRole::SpanEnd { sample } => *end_weights.get_mut(sample)? = weight,
        }
        weight_sum += weight;
    }
    if (weight_sum - 1.0).abs() > WEIGHT_EPSILON {
        return None;
    }
    for (index, sample) in samples.iter_mut().enumerate() {
        match &mut sample.sampling {
            PoseSampling::Anchor | PoseSampling::Cycle { .. } => {
                sample.weight = start_weights[index]
            }
            PoseSampling::Span { progress, .. } => {
                let total = start_weights[index] + end_weights[index];
                if total <= f32::EPSILON {
                    return None;
                }
                sample.weight = total;
                *progress = (end_weights[index] / total).clamp(0.0, 1.0);
            }
        }
    }
    Some(decoded)
}

fn evaluate_dependency_graph(
    graph_handle: &Handle<DependencyAnimationGraph>,
    arena: &mut GraphContextArena,
    poses: Vec<Pose>,
    factors: [f32; MAX_GRAPH_ANCHORS - 1],
    absolute_time: f32,
    resources: &SystemResources,
) -> Option<Pose> {
    let graph = resources.animation_graph_assets.get(graph_handle)?;
    let mut gizmos = DeferredGizmos::default();
    let entity_map = HashMap::new();
    let global_inputs = HashMap::new();
    let mut io = IoOverrides::default();
    for (index, pose) in poses.into_iter().enumerate() {
        io.data.insert(
            GraphInputPin::passthrough(pose_pin(index)),
            DataValue::Pose(pose),
        );
    }
    for (index, factor) in factors.into_iter().enumerate() {
        io.data.insert(
            GraphInputPin::passthrough(factor_pin(index + 1)),
            DataValue::F32(factor),
        );
    }
    graph
        .query_with_env(
            TimeUpdate::Absolute(absolute_time),
            arena,
            resources,
            &io,
            Entity::PLACEHOLDER,
            &entity_map,
            &mut gizmos,
            &global_inputs,
        )
        .ok()?
        .remove(DEFAULT_OUTPUT_POSE)?
        .into_pose()
        .ok()
}

fn requested_path(skeleton: &PresentedSkeleton) -> SemanticGraphPath {
    if skeleton.action_kind() == SkeletonAction::Attack
        || skeleton.weapon_guard() == WeaponGuardState::Raised
    {
        SemanticGraphPath::RaisedGuardAttack
    } else if skeleton.is_grounded()
        && matches!(skeleton.posture(), Posture::Upright | Posture::Crouched)
    {
        SemanticGraphPath::OrdinaryLocomotion
    } else {
        SemanticGraphPath::LegacyFallback
    }
}

pub(super) fn route_semantic_graph(
    graphs: &mut SemanticGraphLibrary,
    resources: &SystemResources,
    entity: Entity,
    skeleton: &PresentedSkeleton,
) -> SemanticGraphTrace {
    let legacy = AnimationEvaluation::from_skeleton(skeleton);
    let inputs = SemanticGraphInputs::from_presented(skeleton, &legacy);
    let requested = requested_path(skeleton);
    let graph_handle = match requested {
        SemanticGraphPath::OrdinaryLocomotion => Some(graphs.ordinary.clone()),
        SemanticGraphPath::RaisedGuardAttack => Some(graphs.raised.clone()),
        SemanticGraphPath::LegacyFallback => None,
    };
    let mut evaluation = None;
    if let Some(graph_handle) = graph_handle
        && let Some((poses, factors, roles)) = encode_sparse_inputs(&legacy, &graphs.marker_ids)
    {
        #[cfg(test)]
        let factors = graphs.factor_override.unwrap_or(factors);
        let arena = graphs
            .contexts
            .entry((entity, requested))
            .or_insert_with(|| GraphContextArena::new(graph_handle.id()));
        let absolute_time = if skeleton.action_kind() == SkeletonAction::Attack {
            legacy.action_phase
        } else {
            legacy.gait_phase
        };
        if let Some(pose) = evaluate_dependency_graph(
            &graph_handle,
            arena,
            poses,
            factors,
            absolute_time,
            resources,
        ) {
            #[cfg(test)]
            let pose = {
                let mut pose = pose;
                if graphs.corrupt_last_marker
                    && let Some(last) = pose.bones.last_mut()
                {
                    last.translation = Some(Vec3::splat(f32::NAN));
                }
                pose
            };
            evaluation = decode_graph_pose(&legacy, &pose, &graphs.marker_ids, &roles);
        }
    }
    let runtime_evaluated = evaluation.is_some();
    SemanticGraphTrace {
        inputs,
        requested_path: requested,
        path: if runtime_evaluated {
            requested
        } else {
            SemanticGraphPath::LegacyFallback
        },
        evaluation: evaluation.unwrap_or(legacy),
        runtime_evaluated,
    }
}

pub(super) fn evaluate_semantic_graph_paths(
    mut commands: Commands,
    mut graphs: ResMut<SemanticGraphLibrary>,
    resources: SystemResources,
    mut telemetry: ResMut<SemanticGraphTelemetry>,
    players: Query<(Entity, &PresentedSkeleton), With<Player>>,
) {
    let mut live = HashSet::new();
    for (entity, skeleton) in &players {
        live.insert(entity);
        let trace = route_semantic_graph(&mut graphs, &resources, entity, skeleton);
        *telemetry.counts.entry(trace.path).or_default() += 1;
        commands.entity(entity).insert(trace);
    }
    graphs
        .contexts
        .retain(|(entity, _), _| live.contains(entity));
}

#[cfg(test)]
pub(super) fn route_semantic_graph_for_test(
    In((skeleton, entity)): In<(PresentedSkeleton, Entity)>,
    mut graphs: ResMut<SemanticGraphLibrary>,
    resources: SystemResources,
) -> SemanticGraphTrace {
    route_semantic_graph(&mut graphs, &resources, entity, &skeleton)
}

#[cfg(any(test, feature = "animation-graph-editor"))]
#[derive(Debug, Clone, Copy)]
pub(crate) struct EditorGraphPreflightRoute {
    pub(crate) label: &'static str,
    pub(crate) requested_path: SemanticGraphPath,
    pub(crate) selected_path: SemanticGraphPath,
    pub(crate) sample_count: usize,
}

/// Validate and query the exact graph assets installed by
/// `SemanticGraphLibrary`, using representative states for both migrated
/// gameplay routes. The editor calls this before it installs its UI plugin so
/// graph asset, schema, or query failures cannot be hidden by a successful
/// catalog-only preflight.
#[cfg(any(test, feature = "animation-graph-editor"))]
pub(crate) fn editor_graph_preflight(
    mut graphs: ResMut<SemanticGraphLibrary>,
    resources: SystemResources,
) -> Result<Vec<EditorGraphPreflightRoute>, Vec<String>> {
    let graph_assets = [
        (
            SemanticGraphPath::OrdinaryLocomotion,
            graphs.ordinary.clone(),
        ),
        (SemanticGraphPath::RaisedGuardAttack, graphs.raised.clone()),
    ];
    let mut errors = Vec::new();
    for (path, handle) in &graph_assets {
        match resources.animation_graph_assets.get(handle) {
            Some(graph) => {
                if let Err(error) = graph.validate() {
                    errors.push(format!(
                        "{} graph schema is invalid: {error}",
                        path.as_str()
                    ));
                }
            }
            None => errors.push(format!("{} graph asset did not load", path.as_str())),
        }
    }

    let mut ordinary = SkeletonState::default()
        .with_local_velocity(Vec3::NEG_Z * 3.75)
        .with_world_velocity(Vec3::NEG_Z * 3.75);
    ordinary.gait_phase = 0.25;

    let mut right_attack = SkeletonState::default()
        .with_weapon_guard(WeaponGuardState::Raised)
        .with_lead_foot(LeadFoot::Right);
    right_attack.begin_attack(AttackSpec::default(), 10, 20);
    right_attack.advance_action(15);

    let cases = [
        (
            "ordinary locomotion at 3.75 m/s",
            ordinary,
            SemanticGraphPath::OrdinaryLocomotion,
        ),
        (
            "raised right-lead attack at contact approach",
            right_attack,
            SemanticGraphPath::RaisedGuardAttack,
        ),
    ];
    let mut routes = Vec::new();
    for (label, skeleton, expected_path) in cases {
        let presented = PresentedSkeleton::new(skeleton, None);
        let legacy = AnimationEvaluation::from_skeleton(&presented);
        let trace = route_semantic_graph(&mut graphs, &resources, Entity::PLACEHOLDER, &presented);
        if trace.requested_path != expected_path
            || trace.path != expected_path
            || !trace.runtime_evaluated
            || trace.evaluation != legacy
        {
            errors.push(format!(
                "{label} graph query failed: requested {}, selected {}, runtime success {}, legacy parity {}",
                trace.requested_path.as_str(),
                trace.path.as_str(),
                trace.runtime_evaluated,
                trace.evaluation == legacy
            ));
            continue;
        }
        routes.push(EditorGraphPreflightRoute {
            label,
            requested_path: trace.requested_path,
            selected_path: trace.path,
            sample_count: selected_samples(&trace.evaluation).len(),
        });
    }

    if errors.is_empty() {
        Ok(routes)
    } else {
        Err(errors)
    }
}
