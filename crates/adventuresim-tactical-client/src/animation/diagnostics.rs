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
#[allow(dead_code)] // The gameplay client records render telemetry; the viewer only reads animation logs.
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
#[allow(dead_code)] // Construction and recording are native gameplay diagnostics entry points.
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
pub(super) fn log_animation_diagnostics(
    time: Res<Time>,
    mut log: Option<ResMut<AnimationDiagnosticLog>>,
    input: Option<Res<DiagnosticInputStatus>>,
    render_schedule: Option<Res<RenderScheduleTelemetry>>,
    players: Query<
        (
            &Transform,
            &SkeletonState,
            &PresentedSkeleton,
            &AnimationPlayback,
            Option<&semantic_graph::SemanticGraphTrace>,
        ),
        (With<Player>, With<crate::player::ClientPlayer>),
    >,
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
    for (transform, authoritative, presented, playback, semantic_graph) in &players {
        let evaluation = AnimationEvaluation::from_skeleton(presented);
        let transition = playback.presentation_transition.as_ref().map(|transition| {
            serde_json::json!({
                "elapsed_seconds": transition.elapsed_seconds,
                "progress": (transition.elapsed_seconds / PRESENTATION_CROSSFADE_SECONDS)
                    .clamp(0.0, 1.0),
            })
        });
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
        log.write(serde_json::json!({
            "elapsed_seconds": time.elapsed_secs_f64(),
            "wall_clock_unix_micros": wall_clock_unix_micros,
            "render_delta_seconds": time.delta_secs(),
            "render_schedule_completion": render_schedule_completion,
            "input": input.as_deref(),
            "controller_transform": {
                "translation": transform.translation.to_array(),
                "rotation_xyzw": transform.rotation.to_array(),
            },
            "authoritative": authoritative,
            "presented": &presented.state,
            "presentation_phase_error_remaining": presented.phase_error_remaining,
            "presentation_phase_prediction_delta": presented.last_phase_prediction_delta,
            "presentation_phase_correction_delta": presented.last_phase_correction_delta,
            "presentation_phase_measurement_error": presented.last_phase_measurement_error,
            "presentation_phase_source_changed": presented.last_phase_source_changed,
            "evaluation": evaluation,
            "semantic_graph": semantic_graph.map(|trace| serde_json::json!({
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
                "transition": transition,
                "clips": clips,
            },
        }));
    }
}

#[cfg(target_family = "wasm")]
pub(super) fn log_animation_diagnostics() {}
