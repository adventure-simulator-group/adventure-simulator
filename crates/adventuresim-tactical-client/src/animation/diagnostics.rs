#[cfg(not(target_family = "wasm"))]
use super::*;
#[cfg(not(target_family = "wasm"))]
use std::io::{BufWriter, Write};
#[cfg(not(target_family = "wasm"))]
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

#[cfg(not(target_family = "wasm"))]
#[derive(Resource, Debug, Clone, Default, serde::Serialize)]
pub(crate) struct DiagnosticInputStatus {
    pub command_index: usize,
    pub command_kind: String,
    pub command_elapsed_seconds: f32,
    pub request: PlayerInputRequest,
}

#[cfg(not(target_family = "wasm"))]
#[derive(Resource)]
pub(crate) struct AnimationDiagnosticLog {
    pub(crate) writer: BufWriter<std::fs::File>,
    pub(crate) frame: u64,
}

#[cfg(not(target_family = "wasm"))]
#[derive(Resource, Clone, Debug)]
pub(crate) struct RenderScheduleTelemetry(Arc<RenderScheduleShared>);

#[cfg(not(target_family = "wasm"))]
#[derive(Debug)]
struct RenderScheduleShared {
    started: Instant,
    count: AtomicU64,
    elapsed_micros: AtomicU64,
}

#[cfg(not(target_family = "wasm"))]
impl RenderScheduleTelemetry {
    pub(crate) fn new() -> Self {
        Self(Arc::new(RenderScheduleShared {
            started: Instant::now(),
            count: AtomicU64::new(0),
            elapsed_micros: AtomicU64::new(0),
        }))
    }

    pub(crate) fn record_completion(&self) {
        let elapsed = self.0.started.elapsed().as_micros();
        self.0
            .elapsed_micros
            .store(elapsed.min(u64::MAX as u128) as u64, Ordering::Release);
        self.0.count.fetch_add(1, Ordering::Release);
    }

    pub(crate) fn snapshot(&self) -> (u64, u64) {
        (
            self.0.count.load(Ordering::Acquire),
            self.0.elapsed_micros.load(Ordering::Acquire),
        )
    }
}

#[cfg(not(target_family = "wasm"))]
impl AnimationDiagnosticLog {
    pub(crate) fn write(&mut self, mut value: serde_json::Value) {
        value["frame"] = self.frame.into();
        serde_json::to_writer(&mut self.writer, &value)
            .expect("animation diagnostic log should remain writable");
        self.writer
            .write_all(b"\n")
            .expect("animation diagnostic log should remain writable");
        if self.frame % 60 == 59 {
            self.writer
                .flush()
                .expect("animation diagnostic log should remain writable");
        }
        self.frame += 1;
    }
}

#[cfg(not(target_family = "wasm"))]
#[expect(
    clippy::type_complexity,
    reason = "the diagnostic Bevy query reads one exact snapshot of player animation components"
)]
pub(super) fn log_animation_diagnostics(
    time: Res<Time>,
    mut log: Option<ResMut<AnimationDiagnosticLog>>,
    input: Option<Res<DiagnosticInputStatus>>,
    render_schedule: Option<Res<RenderScheduleTelemetry>>,
    terrains: Query<&SceneTerrain>,
    players: Query<
        (
            Entity,
            &Transform,
            &GlobalTransform,
            Option<&Rotation>,
            &SkeletonState,
            &PresentedSkeleton,
            &AnimationPlayback,
            Option<&semantic_route::SemanticRouteTrace>,
        ),
        (With<Player>, With<crate::player::ClientPlayer>),
    >,
    bones: Query<(
        &AnimationTargetId,
        &AuthoredBindTransform,
        Option<&Name>,
        &GlobalTransform,
    )>,
) {
    let Some(log) = log.as_mut() else {
        return;
    };
    let render_schedule_completion = render_schedule.as_deref().map(|telemetry| {
        let (count, elapsed_micros) = telemetry.snapshot();
        serde_json::json!({
            "count": count,
            "elapsed_micros": elapsed_micros,
        })
    });
    let wall_clock_unix_micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_micros().min(u64::MAX as u128) as u64)
        .unwrap_or_default();
    let terrain = terrains.iter().next();
    for (
        player,
        transform,
        global_transform,
        physics_rotation,
        authoritative,
        presented,
        playback,
        semantic_route,
    ) in &players
    {
        let diagnostic_frame = log.frame;
        let global_translation = global_transform.translation();
        let terrain_height = terrain.and_then(|terrain| terrain.height_at(global_translation.xz()));
        let evaluation = AnimationEvaluation::from_skeleton(presented);
        let clips = playback
            .clips
            .iter()
            .map(|clip| {
                serde_json::json!({
                    "weight": clip.weight,
                    "time_seconds": clip.time_seconds,
                    "mirrored_weight": clip.mirrored_weight,
                })
            })
            .collect::<Vec<_>>();
        let mut bone_transforms = bones
            .iter()
            .filter(|(_, bind, _, _)| bind.owner == player)
            .map(|(target, _, name, transform)| {
                let (scale, rotation, translation) = transform.to_scale_rotation_translation();
                let terrain_clearance_metres = terrain
                    .and_then(|terrain| terrain.height_at(translation.xz()))
                    .map(|height| translation.y - height);
                serde_json::json!({
                    "name": name.map_or("<unnamed>", Name::as_str),
                    "target_id": format!("{target:?}"),
                    "translation": translation.to_array(),
                    "rotation_xyzw": rotation.to_array(),
                    "scale": scale.to_array(),
                    "terrain_clearance_metres": terrain_clearance_metres,
                })
            })
            .collect::<Vec<_>>();
        bone_transforms.sort_by(|left, right| {
            let left_name = left["name"].as_str().unwrap_or_default();
            let right_name = right["name"].as_str().unwrap_or_default();
            let left_target = left["target_id"].as_str().unwrap_or_default();
            let right_target = right["target_id"].as_str().unwrap_or_default();
            (left_name, left_target).cmp(&(right_name, right_target))
        });
        log.write(serde_json::json!({
            "trace_format": "real-client-animation-v1",
            "scenario": "real-client-script",
            "scenario_frame": diagnostic_frame,
            "elapsed_seconds": time.elapsed_secs_f64(),
            "wall_clock_unix_micros": wall_clock_unix_micros,
            "render_delta_seconds": time.delta_secs(),
            "render_schedule_completion": render_schedule_completion,
            "input": input.as_deref(),
            "controller_transform": {
                "translation": transform.translation.to_array(),
                "rotation_xyzw": transform.rotation.to_array(),
            },
            "controller_global_transform": {
                "translation": global_translation.to_array(),
                "rotation_xyzw": global_transform.compute_transform().rotation.to_array(),
            },
            "controller_physics_rotation_xyzw": physics_rotation
                .map(|rotation| rotation.0.to_array()),
            "subject_translation": global_translation.to_array(),
            "subject_rotation_xyzw": global_transform.compute_transform().rotation.to_array(),
            "action": presented.state.action_kind(),
            "action_phase": presented.state.action_phase(),
            "bones": bone_transforms,
            "terrain_height": terrain_height,
            "controller_height_above_terrain": terrain_height
                .map(|height| global_translation.y - height),
            "authoritative": authoritative,
            "presented": &presented.state,
            "presentation_phase_error_remaining": presented.phase_error_remaining,
            "presentation_phase_prediction_delta": presented.last_phase_prediction_delta,
            "presentation_phase_correction_delta": presented.last_phase_correction_delta,
            "presentation_phase_measurement_error": presented.last_phase_measurement_error,
            "presentation_phase_source_changed": presented.last_phase_source_changed,
            "evaluation": evaluation,
            "semantic_route": semantic_route.map(|trace| serde_json::json!({
                "requested_path": trace.requested_path,
                "path": trace.path,
                "inputs": &trace.inputs,
                "runtime_evaluated": trace.runtime_evaluated,
                "evaluation_equivalent": trace.evaluation == evaluation,
            })),
            "playback": {
                "use_authored_bind_pose": playback.use_authored_bind_pose,
                "whole_body_mirror": playback.whole_body_mirror,
                "ordinary_locomotion_active": playback.ordinary_locomotion_active,
                "clips": clips,
            },
        }));
    }
}

#[cfg(target_family = "wasm")]
pub(super) fn log_animation_diagnostics() {}
