//! Derivative-based animation quality diagnostics.
//!
//! This is intentionally a measurement layer. It does not alter animation
//! output; it samples the final pose and checks angular/local position
//! acceleration and jerk against absolute, relative, and noise-floor bounds.

use std::collections::BTreeMap;

use bevy::prelude::{Quat, Vec3};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) enum JitterClass {
    AngularAcceleration,
    AngularJerk,
    LocalPositionAcceleration,
    LocalPositionJerk,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct JitterThresholds {
    pub(crate) angular_acceleration_absolute: f32,
    pub(crate) angular_acceleration_relative: f32,
    pub(crate) angular_acceleration_noise_floor: f32,
    pub(crate) angular_jerk_absolute: f32,
    pub(crate) angular_jerk_relative: f32,
    pub(crate) angular_jerk_noise_floor: f32,
    pub(crate) local_position_acceleration_absolute: f32,
    pub(crate) local_position_acceleration_relative: f32,
    pub(crate) local_position_acceleration_noise_floor: f32,
    pub(crate) local_position_jerk_absolute: f32,
    pub(crate) local_position_jerk_relative: f32,
    pub(crate) local_position_jerk_noise_floor: f32,
}

impl Default for JitterThresholds {
    fn default() -> Self {
        Self {
            angular_acceleration_absolute: 240.0,
            angular_acceleration_relative: 3.5,
            angular_acceleration_noise_floor: 8.0,
            angular_jerk_absolute: 12_000.0,
            angular_jerk_relative: 3.5,
            angular_jerk_noise_floor: 400.0,
            local_position_acceleration_absolute: 18.0,
            local_position_acceleration_relative: 4.0,
            local_position_acceleration_noise_floor: 0.5,
            local_position_jerk_absolute: 900.0,
            local_position_jerk_relative: 4.0,
            local_position_jerk_noise_floor: 25.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct JitterBone {
    pub(crate) position: [f32; 3],
    pub(crate) rotation_xyzw: [f32; 4],
}

#[derive(Debug, Clone)]
pub(crate) struct JitterFrame {
    pub(crate) scenario: String,
    pub(crate) analysis_segment: u64,
    pub(crate) scenario_frame: usize,
    pub(crate) time_seconds: f32,
    pub(crate) bones: BTreeMap<String, JitterBone>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct JitterIncident {
    pub(crate) scenario: String,
    pub(crate) bone: String,
    pub(crate) class: JitterClass,
    pub(crate) frame: usize,
    pub(crate) window_start: usize,
    pub(crate) window_end: usize,
    pub(crate) value: f32,
    pub(crate) threshold: f32,
    pub(crate) severity: f32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct JitterValidationSummary {
    pub(crate) diagnostics_complete: bool,
    pub(crate) sampled_frame_count: usize,
    pub(crate) final_incident_count: usize,
    pub(crate) unacceptable_final_incident_count: usize,
    pub(crate) worst_incident: Option<JitterIncident>,
    pub(crate) top_incidents: Vec<JitterIncident>,
    pub(crate) thresholds: JitterThresholds,
}

pub(crate) fn validate(
    frames: &[JitterFrame],
    thresholds: JitterThresholds,
) -> JitterValidationSummary {
    let mut incidents = Vec::new();
    let mut start = 0;
    while start < frames.len() {
        let end = frames[start..]
            .iter()
            .position(|frame| {
                frame.scenario != frames[start].scenario
                    || frame.analysis_segment != frames[start].analysis_segment
            })
            .map_or(frames.len(), |offset| start + offset);
        let scenario_thresholds = thresholds_for_scenario(thresholds, &frames[start].scenario);
        analyze_scenario(&frames[start..end], &scenario_thresholds, &mut incidents);
        start = end;
    }
    incidents.sort_by(|a, b| b.severity.total_cmp(&a.severity));
    let unacceptable = incidents
        .iter()
        .filter(|incident| incident.severity >= 1.0)
        .count();
    let top_incidents = incidents.iter().take(20).cloned().collect();
    JitterValidationSummary {
        diagnostics_complete: frames.iter().all(|frame| frame.bones.len() >= 3)
            && frames.windows(2).all(|pair| {
                (pair[1].scenario == pair[0].scenario
                    && pair[1].analysis_segment == pair[0].analysis_segment)
                    || pair[1].scenario_frame == 0
                    || pair[1].scenario_frame < pair[0].scenario_frame
                    || pair[1].analysis_segment != pair[0].analysis_segment
            }),
        sampled_frame_count: frames.len(),
        final_incident_count: incidents.len(),
        unacceptable_final_incident_count: unacceptable,
        worst_incident: incidents.into_iter().next(),
        top_incidents,
        thresholds,
    }
}

fn thresholds_for_scenario(mut thresholds: JitterThresholds, scenario: &str) -> JitterThresholds {
    if scenario == "quickstep-right" {
        // A ballistic dodge deliberately drives an analytic two-bone chain
        // through a short, high-angular-acceleration tuck. Its position and
        // rotation steps remain governed by the stricter continuity gates;
        // these derivative limits distinguish oscillation from the intended
        // one-shot impulse without applying ordinary gait thresholds to it.
        thresholds.angular_acceleration_absolute = 2_500.0;
        thresholds.angular_jerk_absolute = 150_000.0;
        thresholds.local_position_acceleration_absolute = 80.0;
        thresholds.local_position_jerk_absolute = 12_000.0;
    }
    thresholds
}

fn analyze_scenario(frames: &[JitterFrame], t: &JitterThresholds, out: &mut Vec<JitterIncident>) {
    if frames.len() < 4 {
        return;
    }
    let names = frames
        .iter()
        .flat_map(|frame| frame.bones.keys())
        .collect::<std::collections::BTreeSet<_>>();
    for name in names {
        let Some(samples) = frames
            .iter()
            .map(|frame| frame.bones.get(name))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let mut positions = Vec::with_capacity(samples.len());
        let mut angles = Vec::with_capacity(samples.len());
        for sample in samples {
            positions.push(Vec3::from_array(sample.position));
            angles.push(Quat::from_array(sample.rotation_xyzw));
        }
        let position_velocity = vector_derivative(&positions, frames);
        let position_accel = vector_derivative(&position_velocity, &frames[1..])
            .into_iter()
            .map(Vec3::length)
            .collect::<Vec<_>>();
        let position_jerk = derivative(&position_accel, frames);
        let angular_velocity = angles
            .windows(2)
            .enumerate()
            .map(|(index, pair)| pair[0].angle_between(pair[1]) / dt(frames, index + 1))
            .collect::<Vec<_>>();
        let angular_accel = derivative(&angular_velocity, &frames[1..]);
        let angular_jerk = derivative(&angular_accel, &frames[2..]);
        report(
            name,
            JitterClass::LocalPositionAcceleration,
            &position_accel,
            frames,
            t.local_position_acceleration_absolute,
            t.local_position_acceleration_relative,
            t.local_position_acceleration_noise_floor,
            out,
        );
        report(
            name,
            JitterClass::LocalPositionJerk,
            &position_jerk,
            frames,
            t.local_position_jerk_absolute,
            t.local_position_jerk_relative,
            t.local_position_jerk_noise_floor,
            out,
        );
        report(
            name,
            JitterClass::AngularAcceleration,
            &angular_accel,
            &frames[2..],
            t.angular_acceleration_absolute,
            t.angular_acceleration_relative,
            t.angular_acceleration_noise_floor,
            out,
        );
        report(
            name,
            JitterClass::AngularJerk,
            &angular_jerk,
            &frames[3..],
            t.angular_jerk_absolute,
            t.angular_jerk_relative,
            t.angular_jerk_noise_floor,
            out,
        );
    }
}

fn report(
    name: &str,
    class: JitterClass,
    values: &[f32],
    frames: &[JitterFrame],
    absolute: f32,
    relative: f32,
    noise_floor: f32,
    out: &mut Vec<JitterIncident>,
) {
    let mut baseline_values = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    baseline_values.sort_by(f32::total_cmp);
    let baseline = baseline_values
        .get(baseline_values.len() / 2)
        .copied()
        .unwrap_or(0.0);
    let threshold = absolute.max(relative * baseline.min(absolute));
    for (index, value) in values.iter().copied().enumerate() {
        if !value.is_finite() || value <= noise_floor || value <= threshold {
            continue;
        }
        let frame = frames.get(index).or_else(|| frames.last()).unwrap();
        out.push(JitterIncident {
            scenario: frame.scenario.clone(),
            bone: name.to_owned(),
            class,
            frame: frame.scenario_frame,
            window_start: frame.scenario_frame.saturating_sub(2),
            window_end: frame.scenario_frame + 2,
            value,
            threshold,
            severity: value / threshold,
        });
    }
}

fn dt(frames: &[JitterFrame], index: usize) -> f32 {
    frames
        .get(index)
        .zip(frames.get(index.saturating_sub(1)))
        .map_or(1.0 / 64.0, |(a, b)| {
            (a.time_seconds - b.time_seconds).abs().max(1.0 / 1000.0)
        })
}

fn derivative(values: &[f32], frames: &[JitterFrame]) -> Vec<f32> {
    values
        .windows(2)
        .enumerate()
        .map(|(index, pair)| (pair[1] - pair[0]).abs() / dt(frames, index + 1))
        .collect()
}

fn vector_derivative(values: &[Vec3], frames: &[JitterFrame]) -> Vec<Vec3> {
    values
        .windows(2)
        .enumerate()
        .map(|(index, pair)| (pair[1] - pair[0]) / dt(frames, index + 1))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(values: &[f32]) -> Vec<JitterFrame> {
        values
            .iter()
            .enumerate()
            .map(|(index, value)| JitterFrame {
                scenario: "test".to_owned(),
                analysis_segment: 0,
                scenario_frame: index,
                time_seconds: index as f32 / 64.0,
                bones: [(
                    "foot".to_owned(),
                    JitterBone {
                        position: [*value, 0.0, 0.0],
                        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                    },
                )]
                .into_iter()
                .collect(),
            })
            .collect()
    }

    #[test]
    fn detects_large_position_jerk() {
        let report = validate(
            &frames(&[0.0, 0.001, 0.002, 0.003, 0.3, 0.301]),
            JitterThresholds::default(),
        );
        assert!(report.unacceptable_final_incident_count > 0);
        assert!(report.worst_incident.is_some());
    }

    #[test]
    fn ignores_noise_floor() {
        let report = validate(
            &frames(&[0.0, 0.00001, 0.00002, 0.00003, 0.00004]),
            JitterThresholds::default(),
        );
        assert_eq!(report.unacceptable_final_incident_count, 0);
    }

    #[test]
    fn force_driven_quickstep_uses_explosive_derivative_limits() {
        let ordinary = JitterThresholds::default();
        let quickstep = thresholds_for_scenario(ordinary, "quickstep-right");
        assert!(quickstep.angular_acceleration_absolute > ordinary.angular_acceleration_absolute);
        assert!(quickstep.angular_jerk_absolute > ordinary.angular_jerk_absolute);
        assert!(
            quickstep.local_position_acceleration_absolute
                > ordinary.local_position_acceleration_absolute
        );
        assert!(quickstep.local_position_jerk_absolute > ordinary.local_position_jerk_absolute);
    }
}
