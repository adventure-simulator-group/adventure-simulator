//! Presentation-only joint continuity diagnostics and targeted spike remediation.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::Serialize;

use super::{BoneRole, HumanoidBone, ProceduralAnimationClock};

pub(crate) const JITTER_SAMPLE_HZ: f32 = 64.0;
const MAX_RETAINED_INCIDENTS_PER_OWNER_SEAM: usize = 8_192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PoseDiagnosticSeam {
    Authored,
    Final,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JitterClass {
    AngularVelocity,
    AngularAcceleration,
    AngularJerk,
    LocalPositionVelocity,
    LocalPositionAcceleration,
    LocalPositionJerk,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct SpikeThreshold {
    pub absolute: f32,
    pub relative_multiplier: f32,
    pub noise_floor: f32,
}

impl SpikeThreshold {
    fn evidence(self, value: f32, previous: f32) -> Option<SpikeEvidence> {
        let absolute_exceeded = value >= self.absolute;
        let relative_baseline = previous.max(self.noise_floor);
        let relative_exceeded = value >= relative_baseline * self.relative_multiplier;
        (absolute_exceeded && relative_exceeded).then_some(SpikeEvidence {
            absolute_exceeded,
            relative_exceeded,
            previous_value: previous,
            relative_baseline,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct JitterThresholds {
    /// Radians per second squared.
    pub angular_acceleration: SpikeThreshold,
    /// Radians per second cubed.
    pub angular_jerk: SpikeThreshold,
    /// Metres per second squared in parent-local space.
    pub local_position_acceleration: SpikeThreshold,
    /// Metres per second cubed in parent-local space.
    pub local_position_jerk: SpikeThreshold,
}

impl Default for JitterThresholds {
    fn default() -> Self {
        Self {
            angular_acceleration: SpikeThreshold {
                absolute: 240.0,
                relative_multiplier: 3.5,
                noise_floor: 8.0,
            },
            angular_jerk: SpikeThreshold {
                absolute: 12_000.0,
                relative_multiplier: 3.5,
                noise_floor: 400.0,
            },
            local_position_acceleration: SpikeThreshold {
                absolute: 18.0,
                relative_multiplier: 4.0,
                noise_floor: 0.5,
            },
            local_position_jerk: SpikeThreshold {
                absolute: 900.0,
                relative_multiplier: 4.0,
                noise_floor: 25.0,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct SpikeEvidence {
    pub absolute_exceeded: bool,
    pub relative_exceeded: bool,
    pub previous_value: f32,
    pub relative_baseline: f32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct JitterIncident {
    pub seam: PoseDiagnosticSeam,
    pub joint: String,
    pub class: JitterClass,
    pub frame: u64,
    pub frame_window: [u64; 2],
    pub value: f32,
    pub severity: f32,
    pub evidence: SpikeEvidence,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct JitterSeamReport {
    pub incident_count: usize,
    pub worst: Option<JitterIncident>,
    pub maximum_by_class: BTreeMap<JitterClass, f32>,
    pub incidents: Vec<JitterIncident>,
    pub incidents_truncated: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct JitterDiagnosticReport {
    pub sample_hz: f32,
    pub thresholds: Option<JitterThresholds>,
    pub authored: JitterSeamReport,
    pub final_pose: JitterSeamReport,
}

#[derive(Debug, Clone, Copy, Default)]
struct MotionHistory {
    tick: Option<u64>,
    rotation: Quat,
    local_position: Vec3,
    angular_velocity: Option<Vec3>,
    angular_acceleration: Option<Vec3>,
    angular_jerk: Option<Vec3>,
    position_velocity: Option<Vec3>,
    position_acceleration: Option<Vec3>,
    position_jerk: Option<Vec3>,
}

#[derive(Resource, Debug)]
pub(crate) struct JointJitterDiagnostics {
    enabled: bool,
    thresholds: JitterThresholds,
    histories: BTreeMap<(PoseDiagnosticSeam, Entity, BoneRole), MotionHistory>,
    reports: BTreeMap<(Entity, PoseDiagnosticSeam), JitterSeamReport>,
    continuity_generation: u64,
    live_accumulator: f32,
    live_elapsed_since_sample: f32,
    live_sample_tick: u64,
    live_sample_dt: f32,
    live_sample_due: bool,
}

impl Default for JointJitterDiagnostics {
    fn default() -> Self {
        Self {
            enabled: false,
            thresholds: JitterThresholds::default(),
            histories: BTreeMap::new(),
            reports: BTreeMap::new(),
            continuity_generation: 0,
            live_accumulator: 0.0,
            live_elapsed_since_sample: 0.0,
            live_sample_tick: 0,
            live_sample_dt: 1.0 / JITTER_SAMPLE_HZ,
            live_sample_due: false,
        }
    }
}

impl JointJitterDiagnostics {
    pub(crate) fn enabled() -> Self {
        Self {
            enabled: true,
            ..default()
        }
    }

    pub(crate) fn report_for_owner(&self, owner: Entity) -> JitterDiagnosticReport {
        JitterDiagnosticReport {
            sample_hz: JITTER_SAMPLE_HZ,
            thresholds: Some(self.thresholds),
            authored: self
                .reports
                .get(&(owner, PoseDiagnosticSeam::Authored))
                .cloned()
                .unwrap_or_default(),
            final_pose: self
                .reports
                .get(&(owner, PoseDiagnosticSeam::Final))
                .cloned()
                .unwrap_or_default(),
        }
    }

    pub(crate) fn reset_histories(&mut self) {
        self.histories.clear();
        self.continuity_generation = self.continuity_generation.wrapping_add(1);
    }

    fn advance_live_clock(&mut self, delta_seconds: f32, fixed_override: bool) {
        if fixed_override || !self.enabled {
            self.live_sample_due = false;
            if !self.enabled {
                self.histories.clear();
                self.reports.clear();
            }
            return;
        }
        let delta_seconds = delta_seconds.max(0.0);
        self.live_accumulator += delta_seconds;
        self.live_elapsed_since_sample += delta_seconds;
        if self.live_accumulator >= 1.0 / JITTER_SAMPLE_HZ {
            self.live_sample_tick = self.live_sample_tick.wrapping_add(1);
            self.live_sample_dt = self.live_elapsed_since_sample;
            self.live_elapsed_since_sample = 0.0;
            self.live_accumulator = self.live_accumulator.rem_euclid(1.0 / JITTER_SAMPLE_HZ);
            self.live_sample_due = true;
        } else {
            self.live_sample_due = false;
        }
    }

    fn sample(
        &mut self,
        seam: PoseDiagnosticSeam,
        owner: Entity,
        role: BoneRole,
        tick: u64,
        dt: f32,
        transform: &Transform,
    ) {
        if !self.enabled {
            return;
        }
        if !transform.rotation.is_finite()
            || transform.rotation.length_squared() <= f32::EPSILON
            || !transform.translation.is_finite()
        {
            return;
        }
        let key = (seam, owner, role);
        let previous = self.histories.get(&key).copied().unwrap_or_default();
        if previous.tick == Some(tick) {
            return;
        }
        let mut next = MotionHistory {
            tick: Some(tick),
            rotation: transform.rotation.normalize(),
            local_position: transform.translation,
            ..default()
        };
        if previous
            .tick
            .is_some_and(|previous_tick| tick == previous_tick.wrapping_add(1))
        {
            let dt = dt.max(f32::EPSILON);
            let angular_velocity = shortest_rotation_vector(previous.rotation, next.rotation) / dt;
            let position_velocity = (next.local_position - previous.local_position) / dt;
            next.angular_velocity = Some(angular_velocity);
            next.position_velocity = Some(position_velocity);

            self.measure(
                owner,
                seam,
                JitterClass::AngularVelocity,
                angular_velocity.length(),
            );
            self.measure(
                owner,
                seam,
                JitterClass::LocalPositionVelocity,
                position_velocity.length(),
            );
            if let Some(previous_velocity) = previous.angular_velocity {
                let acceleration = (angular_velocity - previous_velocity) / dt;
                next.angular_acceleration = Some(acceleration);
                self.consider(
                    owner,
                    seam,
                    role,
                    tick,
                    JitterClass::AngularAcceleration,
                    acceleration.length(),
                    previous.angular_acceleration.map_or(0.0, Vec3::length),
                    self.thresholds.angular_acceleration,
                );
                if let Some(previous_acceleration) = previous.angular_acceleration {
                    let jerk = (acceleration - previous_acceleration) / dt;
                    next.angular_jerk = Some(jerk);
                    self.consider(
                        owner,
                        seam,
                        role,
                        tick,
                        JitterClass::AngularJerk,
                        jerk.length(),
                        previous.angular_jerk.map_or(0.0, Vec3::length),
                        self.thresholds.angular_jerk,
                    );
                }
            }
            if let Some(previous_velocity) = previous.position_velocity {
                let acceleration = (position_velocity - previous_velocity) / dt;
                next.position_acceleration = Some(acceleration);
                self.consider(
                    owner,
                    seam,
                    role,
                    tick,
                    JitterClass::LocalPositionAcceleration,
                    acceleration.length(),
                    previous.position_acceleration.map_or(0.0, Vec3::length),
                    self.thresholds.local_position_acceleration,
                );
                if let Some(previous_acceleration) = previous.position_acceleration {
                    let jerk = (acceleration - previous_acceleration) / dt;
                    next.position_jerk = Some(jerk);
                    self.consider(
                        owner,
                        seam,
                        role,
                        tick,
                        JitterClass::LocalPositionJerk,
                        jerk.length(),
                        previous.position_jerk.map_or(0.0, Vec3::length),
                        self.thresholds.local_position_jerk,
                    );
                }
            }
        }
        self.histories.insert(key, next);
    }

    fn consider(
        &mut self,
        owner: Entity,
        seam: PoseDiagnosticSeam,
        role: BoneRole,
        tick: u64,
        class: JitterClass,
        value: f32,
        previous: f32,
        threshold: SpikeThreshold,
    ) {
        self.measure(owner, seam, class, value);
        let Some(evidence) = threshold.evidence(value, previous) else {
            return;
        };
        let incident = JitterIncident {
            seam,
            joint: role.label().to_owned(),
            class,
            frame: tick,
            frame_window: [
                tick.saturating_sub(match class {
                    JitterClass::AngularVelocity | JitterClass::LocalPositionVelocity => 1,
                    JitterClass::AngularAcceleration | JitterClass::LocalPositionAcceleration => 2,
                    JitterClass::AngularJerk | JitterClass::LocalPositionJerk => 3,
                }),
                tick.saturating_add(2),
            ],
            value,
            severity: value / threshold.absolute.max(f32::EPSILON),
            evidence,
        };
        let report = self.reports.entry((owner, seam)).or_default();
        report.incident_count += 1;
        if report
            .worst
            .as_ref()
            .is_none_or(|worst| incident.severity > worst.severity)
        {
            report.worst = Some(incident.clone());
        }
        if report.incidents.len() < MAX_RETAINED_INCIDENTS_PER_OWNER_SEAM {
            report.incidents.push(incident);
        } else {
            report.incidents_truncated += 1;
        }
    }

    fn measure(&mut self, owner: Entity, seam: PoseDiagnosticSeam, class: JitterClass, value: f32) {
        self.reports
            .entry((owner, seam))
            .or_default()
            .maximum_by_class
            .entry(class)
            .and_modify(|maximum| *maximum = maximum.max(value))
            .or_insert(value);
    }
}

pub(super) fn sample_authored_pose_jitter(
    clock: Res<ProceduralAnimationClock>,
    bones: Query<(&HumanoidBone, &Transform)>,
    mut diagnostics: ResMut<JointJitterDiagnostics>,
) {
    sample_pose(
        PoseDiagnosticSeam::Authored,
        &clock,
        &bones,
        &mut diagnostics,
    );
}

pub(super) fn advance_jitter_diagnostic_clock(
    time: Res<Time>,
    clock: Res<ProceduralAnimationClock>,
    mut diagnostics: ResMut<JointJitterDiagnostics>,
) {
    diagnostics.advance_live_clock(time.delta_secs(), clock.fixed_step().is_some());
}

pub(super) fn sample_final_pose_jitter(
    clock: Res<ProceduralAnimationClock>,
    bones: Query<(&HumanoidBone, &Transform)>,
    mut diagnostics: ResMut<JointJitterDiagnostics>,
) {
    sample_pose(PoseDiagnosticSeam::Final, &clock, &bones, &mut diagnostics);
}

fn sample_pose(
    seam: PoseDiagnosticSeam,
    clock: &ProceduralAnimationClock,
    bones: &Query<(&HumanoidBone, &Transform)>,
    diagnostics: &mut JointJitterDiagnostics,
) {
    if !diagnostics.enabled {
        return;
    }
    let sample = clock.fixed_step().or_else(|| {
        diagnostics
            .live_sample_due
            .then_some((diagnostics.live_sample_tick, diagnostics.live_sample_dt))
    });
    let Some((tick, dt)) = sample else { return };
    let mut present = BTreeMap::new();
    for (bone, transform) in bones {
        present.insert((bone.owner, bone.role), ());
        diagnostics.sample(seam, bone.owner, bone.role, tick, dt, transform);
    }
    diagnostics
        .histories
        .retain(|(entry_seam, owner, role), _| {
            *entry_seam != seam || present.contains_key(&(*owner, *role))
        });
    let owners = present
        .keys()
        .map(|(owner, _)| *owner)
        .collect::<std::collections::BTreeSet<_>>();
    diagnostics
        .reports
        .retain(|(owner, _), _| owners.contains(owner));
    if seam == PoseDiagnosticSeam::Final && clock.fixed_step().is_none() {
        diagnostics.live_sample_due = false;
    }
}

/// Quaternion logarithm expressed as a shortest-hemisphere rotation vector.
fn shortest_rotation_vector(from: Quat, to: Quat) -> Vec3 {
    let from = from.normalize();
    let mut to = to.normalize();
    if from.dot(to) < 0.0 {
        to = -to;
    }
    // Parent-local joint samples share the same parent frame. Express every
    // delta in that stable spatial frame so acceleration and jerk subtract
    // vectors with consistent axes as the joint itself turns.
    let mut delta = (to * from.conjugate()).normalize();
    if delta.w < 0.0 {
        delta = -delta;
    }
    let vector = Vec3::new(delta.x, delta.y, delta.z);
    let length = vector.length();
    if length <= 1.0e-7 {
        return vector * 2.0;
    }
    vector / length * (2.0 * length.atan2(delta.w.clamp(-1.0, 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quaternion_log_uses_the_shortest_hemisphere() {
        let rotation = Quat::from_rotation_y(0.25);
        let vector = shortest_rotation_vector(Quat::IDENTITY, -rotation);
        assert!((vector - Vec3::Y * 0.25).length() < 0.000_01);
    }

    #[test]
    fn quaternion_log_keeps_non_commuting_deltas_in_the_parent_frame() {
        let from = Quat::from_rotation_x(0.7);
        let parent_frame_delta = Quat::from_rotation_y(0.2);
        let to = parent_frame_delta * from;
        let vector = shortest_rotation_vector(from, to);
        assert!((vector - Vec3::Y * 0.2).length() < 0.000_01);
    }

    #[test]
    fn relative_ratio_alone_never_reports_a_spike() {
        let threshold = SpikeThreshold {
            absolute: 10.0,
            relative_multiplier: 2.0,
            noise_floor: 0.1,
        };
        assert!(threshold.evidence(0.5, 0.0).is_none());
        assert!(threshold.evidence(12.0, 8.0).is_none());
        assert!(threshold.evidence(12.0, 1.0).is_some());
    }

    #[test]
    fn report_names_the_worst_joint_and_frame_window() {
        let mut diagnostics = JointJitterDiagnostics::enabled();
        let thresholds = diagnostics.thresholds;
        let owner = Entity::from_bits(1);
        diagnostics.consider(
            owner,
            PoseDiagnosticSeam::Final,
            BoneRole::ShinLeft,
            10,
            JitterClass::AngularAcceleration,
            300.0,
            1.0,
            thresholds.angular_acceleration,
        );
        diagnostics.consider(
            owner,
            PoseDiagnosticSeam::Final,
            BoneRole::ShinRight,
            20,
            JitterClass::AngularJerk,
            20_000.0,
            1.0,
            thresholds.angular_jerk,
        );
        let report = diagnostics.report_for_owner(owner);
        let worst = report.final_pose.worst.as_ref().unwrap();
        assert_eq!(worst.joint, "right_knee");
        assert_eq!(worst.frame_window, [17, 22]);
    }

    #[test]
    fn derivatives_require_valid_contiguous_predecessors() {
        let mut diagnostics = JointJitterDiagnostics::enabled();
        let owner = Entity::from_bits(2);
        let pose = |angle| Transform::from_rotation(Quat::from_rotation_y(angle));
        diagnostics.sample(
            PoseDiagnosticSeam::Final,
            owner,
            BoneRole::Chest,
            0,
            1.0 / 64.0,
            &pose(0.0),
        );
        diagnostics.sample(
            PoseDiagnosticSeam::Final,
            owner,
            BoneRole::Chest,
            1,
            1.0 / 64.0,
            &pose(0.01),
        );
        let after_velocity =
            diagnostics.histories[&(PoseDiagnosticSeam::Final, owner, BoneRole::Chest)];
        assert!(after_velocity.angular_velocity.is_some());
        assert!(after_velocity.angular_acceleration.is_none());
        diagnostics.sample(
            PoseDiagnosticSeam::Final,
            owner,
            BoneRole::Chest,
            2,
            1.0 / 64.0,
            &pose(0.02),
        );
        let after_acceleration =
            diagnostics.histories[&(PoseDiagnosticSeam::Final, owner, BoneRole::Chest)];
        assert!(after_acceleration.angular_acceleration.is_some());
        assert!(after_acceleration.angular_jerk.is_none());
        diagnostics.sample(
            PoseDiagnosticSeam::Final,
            owner,
            BoneRole::Chest,
            3,
            1.0 / 64.0,
            &pose(0.03),
        );
        let after_jerk =
            diagnostics.histories[&(PoseDiagnosticSeam::Final, owner, BoneRole::Chest)];
        assert!(after_jerk.angular_jerk.is_some());
        diagnostics.sample(
            PoseDiagnosticSeam::Final,
            owner,
            BoneRole::Chest,
            5,
            1.0 / 64.0,
            &pose(0.05),
        );
        let after_gap = diagnostics.histories[&(PoseDiagnosticSeam::Final, owner, BoneRole::Chest)];
        assert!(after_gap.angular_velocity.is_none());
        assert_eq!(
            diagnostics
                .report_for_owner(owner)
                .final_pose
                .incident_count,
            0
        );
    }

    #[test]
    fn live_clock_retains_actual_sample_interval_and_disabled_state_releases_history() {
        let mut diagnostics = JointJitterDiagnostics::enabled();
        diagnostics.advance_live_clock(1.0 / 144.0, false);
        diagnostics.advance_live_clock(1.0 / 144.0, false);
        assert!(!diagnostics.live_sample_due);
        diagnostics.advance_live_clock(1.0 / 144.0, false);
        assert!(diagnostics.live_sample_due);
        assert_eq!(diagnostics.live_sample_tick, 1);
        assert!((diagnostics.live_sample_dt - 3.0 / 144.0).abs() < 0.000_001);

        let owner = Entity::from_bits(3);
        diagnostics.sample(
            PoseDiagnosticSeam::Final,
            owner,
            BoneRole::Chest,
            1,
            diagnostics.live_sample_dt,
            &Transform::IDENTITY,
        );
        assert!(!diagnostics.histories.is_empty());
        diagnostics.enabled = false;
        diagnostics.advance_live_clock(1.0 / 60.0, false);
        assert!(diagnostics.histories.is_empty());
        assert!(diagnostics.reports.is_empty());
    }

    #[test]
    fn reports_are_isolated_by_pose_owner() {
        let mut diagnostics = JointJitterDiagnostics::enabled();
        let first = Entity::from_bits(10);
        let second = Entity::from_bits(11);
        let threshold = diagnostics.thresholds.angular_acceleration;
        diagnostics.consider(
            first,
            PoseDiagnosticSeam::Final,
            BoneRole::Chest,
            8,
            JitterClass::AngularAcceleration,
            threshold.absolute * 2.0,
            threshold.noise_floor,
            threshold,
        );
        assert_eq!(
            diagnostics
                .report_for_owner(first)
                .final_pose
                .incident_count,
            1
        );
        assert_eq!(
            diagnostics
                .report_for_owner(second)
                .final_pose
                .incident_count,
            0
        );
    }
}
