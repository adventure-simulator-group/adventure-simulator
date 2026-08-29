//! Transient retained state and diagnostics. None of this tick-local animation
// authority is persisted outside the Bevy presentation world.

use super::*;
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::animation::procedural) struct LocomotionSettleState {
    pub(super) support_left: bool,
    pub(super) swing_start: Vec3,
    pub(super) capture_point: Vec3,
    pub(super) landing_target: Vec3,
    pub(super) progress: f32,
    pub(super) elapsed_seconds: f32,
    pub(super) raised_handoff: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::animation::procedural) enum SlopeAlignmentMode {
    Raised,
    Ordinary,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::animation::procedural) struct LegRotationChain {
    pub(super) upper: Quat,
    pub(super) lower: Quat,
    pub(super) foot: Quat,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(in crate::animation::procedural) struct LegIkMemory {
    pub(super) left_leg: Option<Vec3>,
    pub(super) right_leg: Option<Vec3>,
    pub(super) left_terrain_pole_world: Option<Vec3>,
    pub(super) right_terrain_pole_world: Option<Vec3>,
    pub(super) left_terrain_end_direction: Option<Vec3>,
    pub(super) right_terrain_end_direction: Option<Vec3>,
    pub(super) left_knee_foot_yaw_offset_degrees: f32,
    pub(super) right_knee_foot_yaw_offset_degrees: f32,
    pub(super) left_rotation_chain: Option<LegRotationChain>,
    pub(super) right_rotation_chain: Option<LegRotationChain>,
    pub(super) left_foot_orientation_world: Option<Quat>,
    pub(super) right_foot_orientation_world: Option<Quat>,
    pub(super) left_contact_orientation_blend_active: bool,
    pub(super) right_contact_orientation_blend_active: bool,
    pub(super) slope_alignment_mode: Option<SlopeAlignmentMode>,
    pub(super) left_foot_plant: Option<Vec3>,
    pub(super) right_foot_plant: Option<Vec3>,
    pub(super) left_foot_plant_acquired: bool,
    pub(super) right_foot_plant_acquired: bool,
    pub(super) left_foot_target: Option<Vec3>,
    pub(super) right_foot_target: Option<Vec3>,
    pub(super) left_foot_world_target: Option<Vec3>,
    pub(super) right_foot_world_target: Option<Vec3>,
    // The quickstep solver writes its final visible landing stance here. The
    // ordinary raised-guard follower consumes it on the first post-action
    // frame instead of reacquiring the authored feet from scratch.
    pub(super) quickstep_handoff: QuickstepContactHandoff,
    // The last propagated ankle positions are the last pose the player
    // actually saw. At the start of a stop, FK has already restored the new
    // idle sample before IK runs, so sampling globals in the IK pass would
    // mistake that authored pose for the preceding rendered run pose.
    pub(super) left_last_rendered_world: Option<Vec3>,
    pub(super) right_last_rendered_world: Option<Vec3>,
    pub(super) left_last_rendered_toe_world: Option<Vec3>,
    pub(super) right_last_rendered_toe_world: Option<Vec3>,
    pub(super) left_last_rendered_owner: Option<Vec3>,
    pub(super) right_last_rendered_owner: Option<Vec3>,
    pub(super) left_last_rendered_foot_rotation_world: Option<Quat>,
    pub(super) right_last_rendered_foot_rotation_world: Option<Quat>,
    pub(super) left_authored_world_target: Option<Vec3>,
    pub(super) right_authored_world_target: Option<Vec3>,
    pub(super) left_planned_contact: Option<Vec3>,
    pub(super) right_planned_contact: Option<Vec3>,
    pub(super) left_planned_contact_start: Option<Vec3>,
    pub(super) right_planned_contact_start: Option<Vec3>,
    pub(super) left_planned_contact_phase_start: Option<f32>,
    pub(super) right_planned_contact_phase_start: Option<f32>,
    pub(super) left_support_weight: Option<f32>,
    pub(super) right_support_weight: Option<f32>,
    // Solver ownership is separate from truthful post-propagation contact
    // diagnostics. A rendered miss may report zero without erasing the fact
    // that the next solve must release from the preceding planted chain.
    pub(super) left_transition_support_weight: Option<f32>,
    pub(super) right_transition_support_weight: Option<f32>,
    pub(super) left_support_exhausted_until_flight: bool,
    pub(super) right_support_exhausted_until_flight: bool,
    pub(super) left_release_active: bool,
    pub(super) right_release_active: bool,
    pub(super) left_release_target: Option<Vec3>,
    pub(super) right_release_target: Option<Vec3>,
    pub(super) pelvis_shift: f32,
    // Terminal stop correction is an absolute offset from the local rig-root
    // pose captured when dual-contact convergence begins. Sparse idle clips
    // do not necessarily rewrite that root every tick, so adding the retained
    // ordinary pelvis scalar repeatedly stalls or double-applies correction.
    pub(super) terminal_contacts_prepared: bool,
    pub(super) terminal_root_base_translation: Option<Vec3>,
    pub(super) terminal_reach_shift: f32,
    pub(super) terminal_reach_target_shift: Option<f32>,
    pub(super) raised_pelvis_shift: f32,
    pub(super) terrain_blend: f32,
    pub(super) rig_origin: Option<Vec3>,
    pub(super) rig_rotation: Option<Quat>,
    pub(super) measured_owner_planar_speed: f32,
    pub(super) evaluation_tick: Option<u64>,
    pub(super) recent_movement_velocity: Vec3,
    pub(super) settle: Option<LocomotionSettleState>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(in crate::animation::procedural) enum QuickstepContactHandoff {
    #[default]
    None,
    Converging {
        left: Vec3,
        right: Vec3,
    },
    Held {
        left: Vec3,
        right: Vec3,
    },
}

impl QuickstepContactHandoff {
    pub(super) fn is_pending(self) -> bool {
        matches!(self, Self::Converging { .. })
    }

    pub(super) fn is_held(self) -> bool {
        matches!(self, Self::Held { .. })
    }

    pub(super) fn targets(self) -> Option<(Vec3, Vec3)> {
        match self {
            Self::None => None,
            Self::Converging { left, right } | Self::Held { left, right } => Some((left, right)),
        }
    }

    pub(super) fn update_targets(&mut self, left: Vec3, right: Vec3) {
        *self = match self {
            Self::Converging { .. } => Self::Converging { left, right },
            Self::Held { .. } => Self::Held { left, right },
            Self::None => Self::None,
        };
    }

    pub(super) fn hold(&mut self) {
        if let Self::Converging { left, right } = *self {
            *self = Self::Held { left, right };
        }
    }
}

pub(in crate::animation::procedural) fn seed_quickstep_contact_handoff(
    memory: &mut LegIkMemory,
    rig_origin: Vec3,
    rig_rotation: Quat,
    left_world: Vec3,
    right_world: Vec3,
) -> bool {
    if !rig_origin.is_finite()
        || !rig_rotation.is_finite()
        || !left_world.is_finite()
        || !right_world.is_finite()
    {
        return false;
    }
    let inverse_rotation = rig_rotation.inverse();
    let left = inverse_rotation * (left_world - rig_origin);
    let right = inverse_rotation * (right_world - rig_origin);
    if !left.is_finite() || !right.is_finite() {
        return false;
    }
    memory.quickstep_handoff = QuickstepContactHandoff::Converging { left, right };
    memory.rig_origin = Some(rig_origin);
    memory.rig_rotation = Some(rig_rotation);
    true
}

#[derive(Debug, Clone, Copy, Default)]
pub(in crate::animation::procedural) struct ArmIkMemory {
    pub(super) left_arm: Option<Vec3>,
    pub(super) right_arm: Option<Vec3>,
}

pub(in crate::animation::procedural) fn repeated_fixed_tick_skips_ik(
    fixed_tick: bool,
    evaluation_advances: bool,
) -> bool {
    fixed_tick && !evaluation_advances
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct LegIkState(pub(super) LegIkMemory);

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct LegIkDiagnostics {
    pub left_authored_target: Option<Vec3>,
    pub right_authored_target: Option<Vec3>,
    pub left_planned_contact: Option<Vec3>,
    pub right_planned_contact: Option<Vec3>,
    pub settle_capture_point: Option<Vec3>,
    pub left_solve_target: Option<Vec3>,
    pub right_solve_target: Option<Vec3>,
    pub left_support_weight: f32,
    pub right_support_weight: f32,
    pub left_release_active: bool,
    pub right_release_active: bool,
    pub left_release_target: Option<Vec3>,
    pub right_release_target: Option<Vec3>,
    pub settle_progress: Option<f32>,
    pub left_knee_foot_yaw_offset_degrees: f32,
    pub right_knee_foot_yaw_offset_degrees: f32,
}

impl LegIkState {
    pub(crate) fn diagnostics(&self) -> LegIkDiagnostics {
        let settle = self.0.settle;

        LegIkDiagnostics {
            left_authored_target: self.0.left_authored_world_target,
            right_authored_target: self.0.right_authored_world_target,
            left_planned_contact: settle
                .filter(|state| !state.support_left)
                .map(|state| state.landing_target)
                .or(self.0.left_planned_contact),
            right_planned_contact: settle
                .filter(|state| state.support_left)
                .map(|state| state.landing_target)
                .or(self.0.right_planned_contact),
            settle_capture_point: settle.map(|state| state.capture_point),
            left_solve_target: self.0.left_foot_world_target,
            right_solve_target: self.0.right_foot_world_target,
            left_support_weight: self.0.left_support_weight.unwrap_or(0.0),
            right_support_weight: self.0.right_support_weight.unwrap_or(0.0),
            left_release_active: self.0.left_release_active,
            right_release_active: self.0.right_release_active,
            left_release_target: self
                .0
                .left_release_target
                .and_then(|target| Some(self.0.rig_origin? + self.0.rig_rotation? * target)),
            right_release_target: self
                .0
                .right_release_target
                .and_then(|target| Some(self.0.rig_origin? + self.0.rig_rotation? * target)),
            settle_progress: settle.map(|state| state.progress),
            left_knee_foot_yaw_offset_degrees: self.0.left_knee_foot_yaw_offset_degrees,
            right_knee_foot_yaw_offset_degrees: self.0.right_knee_foot_yaw_offset_degrees,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct ArmIkState(pub(super) ArmIkMemory);
