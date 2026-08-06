//! Semantic animation state shared by the tactical authority and presentation client.
//!
//! The server synchronizes intent and timing through [`SkeletonState`]. Authored
//! clips and evaluated bone transforms remain client-only presentation.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    str::FromStr,
};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Stable semantic names used by animation packs and glTF clip names.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Reflect,
)]
#[serde(rename_all = "snake_case")]
pub enum SemanticPose {
    IdleRelaxed,
    WalkContact,
    WalkPassing,
    RunContact,
    RunFlight,
    CrouchIdle,
    CrouchWalkContact,
    CrouchWalkPassing,
    DuckForward,
    DuckBackward,
    DuckLeft,
    DuckRight,
    JumpCenterLaunch,
    JumpCenterFlight,
    JumpCenterLanding,
    JumpForwardLaunch,
    JumpForwardFlight,
    JumpForwardLanding,
    JumpBackwardLaunch,
    JumpBackwardFlight,
    JumpBackwardLanding,
    JumpLeftLaunch,
    JumpLeftFlight,
    JumpLeftLanding,
    JumpRightLaunch,
    JumpRightFlight,
    JumpRightLanding,
    ProneIdle,
    SupineIdle,
    ProneCrawlContact,
    ProneCrawlPassing,
    ProneStrafeContact,
    ProneStrafePassing,
    SupineScamperContact,
    SupineScamperPassing,
    UprightProneTransition,
    DiveImpact,
    ProneSupineRollLeft,
    ProneSupineRollRight,
    GuardLeadLeft,
    GuardLeadRight,
    AttackThrustLeadLeftStayCommit,
    AttackThrustLeadLeftStayContact,
    AttackThrustLeadLeftStayFollowThrough,
    AttackThrustLeadLeftSwitchCommit,
    AttackThrustLeadLeftSwitchContact,
    AttackThrustLeadLeftSwitchFollowThrough,
    AttackThrustLeadRightStayCommit,
    AttackThrustLeadRightStayContact,
    AttackThrustLeadRightStayFollowThrough,
    AttackThrustLeadRightSwitchCommit,
    AttackThrustLeadRightSwitchContact,
    AttackThrustLeadRightSwitchFollowThrough,
    AttackSlashLeadLeftStayCommit,
    AttackSlashLeadLeftStayContact,
    AttackSlashLeadLeftStayFollowThrough,
    AttackSlashLeadLeftSwitchCommit,
    AttackSlashLeadLeftSwitchContact,
    AttackSlashLeadLeftSwitchFollowThrough,
    AttackSlashLeadRightStayCommit,
    AttackSlashLeadRightStayContact,
    AttackSlashLeadRightStayFollowThrough,
    AttackSlashLeadRightSwitchCommit,
    AttackSlashLeadRightSwitchContact,
    AttackSlashLeadRightSwitchFollowThrough,
    BlockCutLeftLeadLeft,
    BlockCutLeftLeadRight,
    BlockCutRightLeadLeft,
    BlockCutRightLeadRight,
    BlockThrustLeadLeft,
    BlockThrustLeadRight,
}

impl SemanticPose {
    pub const HUMANOID_REQUIRED: [Self; 71] = [
        Self::IdleRelaxed,
        Self::WalkContact,
        Self::WalkPassing,
        Self::RunContact,
        Self::RunFlight,
        Self::CrouchIdle,
        Self::CrouchWalkContact,
        Self::CrouchWalkPassing,
        Self::DuckForward,
        Self::DuckBackward,
        Self::DuckLeft,
        Self::DuckRight,
        Self::JumpCenterLaunch,
        Self::JumpCenterFlight,
        Self::JumpCenterLanding,
        Self::JumpForwardLaunch,
        Self::JumpForwardFlight,
        Self::JumpForwardLanding,
        Self::JumpBackwardLaunch,
        Self::JumpBackwardFlight,
        Self::JumpBackwardLanding,
        Self::JumpLeftLaunch,
        Self::JumpLeftFlight,
        Self::JumpLeftLanding,
        Self::JumpRightLaunch,
        Self::JumpRightFlight,
        Self::JumpRightLanding,
        Self::ProneIdle,
        Self::SupineIdle,
        Self::ProneCrawlContact,
        Self::ProneCrawlPassing,
        Self::ProneStrafeContact,
        Self::ProneStrafePassing,
        Self::SupineScamperContact,
        Self::SupineScamperPassing,
        Self::UprightProneTransition,
        Self::DiveImpact,
        Self::ProneSupineRollLeft,
        Self::ProneSupineRollRight,
        Self::GuardLeadLeft,
        Self::GuardLeadRight,
        Self::AttackThrustLeadLeftStayCommit,
        Self::AttackThrustLeadLeftStayContact,
        Self::AttackThrustLeadLeftStayFollowThrough,
        Self::AttackThrustLeadLeftSwitchCommit,
        Self::AttackThrustLeadLeftSwitchContact,
        Self::AttackThrustLeadLeftSwitchFollowThrough,
        Self::AttackThrustLeadRightStayCommit,
        Self::AttackThrustLeadRightStayContact,
        Self::AttackThrustLeadRightStayFollowThrough,
        Self::AttackThrustLeadRightSwitchCommit,
        Self::AttackThrustLeadRightSwitchContact,
        Self::AttackThrustLeadRightSwitchFollowThrough,
        Self::AttackSlashLeadLeftStayCommit,
        Self::AttackSlashLeadLeftStayContact,
        Self::AttackSlashLeadLeftStayFollowThrough,
        Self::AttackSlashLeadLeftSwitchCommit,
        Self::AttackSlashLeadLeftSwitchContact,
        Self::AttackSlashLeadLeftSwitchFollowThrough,
        Self::AttackSlashLeadRightStayCommit,
        Self::AttackSlashLeadRightStayContact,
        Self::AttackSlashLeadRightStayFollowThrough,
        Self::AttackSlashLeadRightSwitchCommit,
        Self::AttackSlashLeadRightSwitchContact,
        Self::AttackSlashLeadRightSwitchFollowThrough,
        Self::BlockCutLeftLeadLeft,
        Self::BlockCutLeftLeadRight,
        Self::BlockCutRightLeadLeft,
        Self::BlockCutRightLeadRight,
        Self::BlockThrustLeadLeft,
        Self::BlockThrustLeadRight,
    ];

    pub fn as_str(self) -> &'static str {
        use SemanticPose::*;
        match self {
            IdleRelaxed => "idle_relaxed",
            WalkContact => "walk_contact",
            WalkPassing => "walk_passing",
            RunContact => "run_contact",
            RunFlight => "run_flight",
            CrouchIdle => "crouch_idle",
            CrouchWalkContact => "crouch_walk_contact",
            CrouchWalkPassing => "crouch_walk_passing",
            DuckForward => "duck_forward",
            DuckBackward => "duck_backward",
            DuckLeft => "duck_left",
            DuckRight => "duck_right",
            JumpCenterLaunch => "jump_center_launch",
            JumpCenterFlight => "jump_center_flight",
            JumpCenterLanding => "jump_center_landing",
            JumpForwardLaunch => "jump_forward_launch",
            JumpForwardFlight => "jump_forward_flight",
            JumpForwardLanding => "jump_forward_landing",
            JumpBackwardLaunch => "jump_backward_launch",
            JumpBackwardFlight => "jump_backward_flight",
            JumpBackwardLanding => "jump_backward_landing",
            JumpLeftLaunch => "jump_left_launch",
            JumpLeftFlight => "jump_left_flight",
            JumpLeftLanding => "jump_left_landing",
            JumpRightLaunch => "jump_right_launch",
            JumpRightFlight => "jump_right_flight",
            JumpRightLanding => "jump_right_landing",
            ProneIdle => "prone_idle",
            SupineIdle => "supine_idle",
            ProneCrawlContact => "prone_crawl_contact",
            ProneCrawlPassing => "prone_crawl_passing",
            ProneStrafeContact => "prone_strafe_contact",
            ProneStrafePassing => "prone_strafe_passing",
            SupineScamperContact => "supine_scamper_contact",
            SupineScamperPassing => "supine_scamper_passing",
            UprightProneTransition => "upright_prone_transition",
            DiveImpact => "dive_impact",
            ProneSupineRollLeft => "prone_supine_roll_left",
            ProneSupineRollRight => "prone_supine_roll_right",
            GuardLeadLeft => "guard_lead_left",
            GuardLeadRight => "guard_lead_right",
            AttackThrustLeadLeftStayCommit => "attack_thrust_lead_left_stay_commit",
            AttackThrustLeadLeftStayContact => "attack_thrust_lead_left_stay_contact",
            AttackThrustLeadLeftStayFollowThrough => "attack_thrust_lead_left_stay_follow_through",
            AttackThrustLeadLeftSwitchCommit => "attack_thrust_lead_left_switch_commit",
            AttackThrustLeadLeftSwitchContact => "attack_thrust_lead_left_switch_contact",
            AttackThrustLeadLeftSwitchFollowThrough => {
                "attack_thrust_lead_left_switch_follow_through"
            }
            AttackThrustLeadRightStayCommit => "attack_thrust_lead_right_stay_commit",
            AttackThrustLeadRightStayContact => "attack_thrust_lead_right_stay_contact",
            AttackThrustLeadRightStayFollowThrough => {
                "attack_thrust_lead_right_stay_follow_through"
            }
            AttackThrustLeadRightSwitchCommit => "attack_thrust_lead_right_switch_commit",
            AttackThrustLeadRightSwitchContact => "attack_thrust_lead_right_switch_contact",
            AttackThrustLeadRightSwitchFollowThrough => {
                "attack_thrust_lead_right_switch_follow_through"
            }
            AttackSlashLeadLeftStayCommit => "attack_slash_lead_left_stay_commit",
            AttackSlashLeadLeftStayContact => "attack_slash_lead_left_stay_contact",
            AttackSlashLeadLeftStayFollowThrough => "attack_slash_lead_left_stay_follow_through",
            AttackSlashLeadLeftSwitchCommit => "attack_slash_lead_left_switch_commit",
            AttackSlashLeadLeftSwitchContact => "attack_slash_lead_left_switch_contact",
            AttackSlashLeadLeftSwitchFollowThrough => {
                "attack_slash_lead_left_switch_follow_through"
            }
            AttackSlashLeadRightStayCommit => "attack_slash_lead_right_stay_commit",
            AttackSlashLeadRightStayContact => "attack_slash_lead_right_stay_contact",
            AttackSlashLeadRightStayFollowThrough => "attack_slash_lead_right_stay_follow_through",
            AttackSlashLeadRightSwitchCommit => "attack_slash_lead_right_switch_commit",
            AttackSlashLeadRightSwitchContact => "attack_slash_lead_right_switch_contact",
            AttackSlashLeadRightSwitchFollowThrough => {
                "attack_slash_lead_right_switch_follow_through"
            }
            BlockCutLeftLeadLeft => "block_cut_left_lead_left",
            BlockCutLeftLeadRight => "block_cut_left_lead_right",
            BlockCutRightLeadLeft => "block_cut_right_lead_left",
            BlockCutRightLeadRight => "block_cut_right_lead_right",
            BlockThrustLeadLeft => "block_thrust_lead_left",
            BlockThrustLeadRight => "block_thrust_lead_right",
        }
    }

    /// The next closest semantic pose. A miss restarts lookup at the selected
    /// animation pack, so specialized packs can supply a useful substitute.
    pub fn fallback(self) -> Option<Self> {
        use SemanticPose::*;
        Some(match self {
            IdleRelaxed => return None,
            WalkContact => IdleRelaxed,
            WalkPassing => WalkContact,
            RunContact => WalkContact,
            RunFlight => WalkPassing,
            CrouchIdle => IdleRelaxed,
            CrouchWalkContact => WalkContact,
            CrouchWalkPassing => WalkPassing,
            DuckForward | DuckBackward | DuckLeft | DuckRight => CrouchIdle,
            JumpCenterLaunch | JumpCenterLanding => CrouchIdle,
            JumpCenterFlight => RunFlight,
            JumpForwardLaunch => JumpCenterLaunch,
            JumpForwardFlight => JumpCenterFlight,
            JumpForwardLanding => JumpCenterLanding,
            JumpBackwardLaunch => JumpCenterLaunch,
            JumpBackwardFlight => JumpCenterFlight,
            JumpBackwardLanding => JumpCenterLanding,
            JumpLeftLaunch => JumpCenterLaunch,
            JumpLeftFlight => JumpCenterFlight,
            JumpLeftLanding => JumpCenterLanding,
            JumpRightLaunch => JumpCenterLaunch,
            JumpRightFlight => JumpCenterFlight,
            JumpRightLanding => JumpCenterLanding,
            ProneIdle | SupineIdle => CrouchIdle,
            ProneCrawlContact | ProneCrawlPassing => ProneIdle,
            ProneStrafeContact => ProneCrawlContact,
            ProneStrafePassing => ProneCrawlPassing,
            SupineScamperContact | SupineScamperPassing => SupineIdle,
            UprightProneTransition => CrouchIdle,
            DiveImpact => JumpForwardLanding,
            ProneSupineRollLeft | ProneSupineRollRight => ProneIdle,
            GuardLeadLeft => IdleRelaxed,
            GuardLeadRight => GuardLeadLeft,
            AttackThrustLeadLeftStayCommit => AttackSlashLeadLeftStayCommit,
            AttackThrustLeadLeftStayContact => AttackSlashLeadLeftStayContact,
            AttackThrustLeadLeftStayFollowThrough => AttackSlashLeadLeftStayFollowThrough,
            AttackThrustLeadLeftSwitchCommit => AttackSlashLeadLeftSwitchCommit,
            AttackThrustLeadLeftSwitchContact => AttackSlashLeadLeftSwitchContact,
            AttackThrustLeadLeftSwitchFollowThrough => AttackSlashLeadLeftSwitchFollowThrough,
            AttackThrustLeadRightStayCommit => AttackSlashLeadRightStayCommit,
            AttackThrustLeadRightStayContact => AttackSlashLeadRightStayContact,
            AttackThrustLeadRightStayFollowThrough => AttackSlashLeadRightStayFollowThrough,
            AttackThrustLeadRightSwitchCommit => AttackSlashLeadRightSwitchCommit,
            AttackThrustLeadRightSwitchContact => AttackSlashLeadRightSwitchContact,
            AttackThrustLeadRightSwitchFollowThrough => AttackSlashLeadRightSwitchFollowThrough,
            AttackSlashLeadLeftStayCommit
            | AttackSlashLeadLeftStayContact
            | AttackSlashLeadLeftStayFollowThrough => GuardLeadLeft,
            AttackSlashLeadLeftSwitchCommit => AttackSlashLeadLeftStayCommit,
            AttackSlashLeadLeftSwitchContact => AttackSlashLeadLeftStayContact,
            AttackSlashLeadLeftSwitchFollowThrough => AttackSlashLeadLeftStayFollowThrough,
            AttackSlashLeadRightStayCommit => AttackSlashLeadLeftStayCommit,
            AttackSlashLeadRightStayContact => AttackSlashLeadLeftStayContact,
            AttackSlashLeadRightStayFollowThrough => AttackSlashLeadLeftStayFollowThrough,
            AttackSlashLeadRightSwitchCommit => AttackSlashLeadRightStayCommit,
            AttackSlashLeadRightSwitchContact => AttackSlashLeadRightStayContact,
            AttackSlashLeadRightSwitchFollowThrough => AttackSlashLeadRightStayFollowThrough,
            BlockCutLeftLeadLeft => BlockThrustLeadLeft,
            BlockCutLeftLeadRight => BlockThrustLeadRight,
            BlockCutRightLeadLeft => BlockCutLeftLeadLeft,
            BlockCutRightLeadRight => BlockCutLeftLeadRight,
            BlockThrustLeadLeft => GuardLeadLeft,
            BlockThrustLeadRight => GuardLeadRight,
        })
    }
}

impl FromStr for SemanticPose {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::HUMANOID_REQUIRED
            .into_iter()
            .find(|pose| pose.as_str() == value)
            .ok_or(())
    }
}

/// One authored pack. `clips` contains semantics whose catalog motions are
/// currently available to the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationPack {
    pub id: String,
    pub skeleton_family: String,
    pub fallback: Option<String>,
    pub clips: BTreeSet<SemanticPose>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedPose<'a> {
    Clip {
        pack_id: &'a str,
        pose: SemanticPose,
    },
    /// Use the rig's authored bind transform. For the humanoid convention this
    /// is a T-pose and needs no animation clip.
    BindPoseT,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackValidationError {
    Duplicate(String),
    MissingFallback { pack: String, fallback: String },
    FallbackCycle(String),
    IncompatibleSkeleton { pack: String, fallback: String },
    MissingRequiredPose(SemanticPose),
}

#[derive(Debug, Default)]
pub struct AnimationPackLibrary {
    packs: BTreeMap<String, AnimationPack>,
}

impl AnimationPackLibrary {
    pub fn insert(&mut self, pack: AnimationPack) -> Result<(), PackValidationError> {
        if self.packs.contains_key(&pack.id) {
            return Err(PackValidationError::Duplicate(pack.id));
        }
        self.packs.insert(pack.id.clone(), pack);
        Ok(())
    }

    /// Validates references, cycles, and skeleton compatibility. Incomplete
    /// packs are accepted while art is in progress; call [`Self::validate_complete`]
    /// for release/content validation.
    pub fn validate_structure(&self) -> Result<(), PackValidationError> {
        for pack in self.packs.values() {
            let mut seen = HashSet::new();
            let mut current = pack;
            loop {
                if !seen.insert(current.id.as_str()) {
                    return Err(PackValidationError::FallbackCycle(pack.id.clone()));
                }
                let Some(fallback_id) = current.fallback.as_deref() else {
                    break;
                };
                let Some(fallback) = self.packs.get(fallback_id) else {
                    return Err(PackValidationError::MissingFallback {
                        pack: current.id.clone(),
                        fallback: fallback_id.to_owned(),
                    });
                };
                if fallback.skeleton_family != pack.skeleton_family {
                    return Err(PackValidationError::IncompatibleSkeleton {
                        pack: current.id.clone(),
                        fallback: fallback.id.clone(),
                    });
                }
                current = fallback;
            }
        }
        Ok(())
    }

    pub fn validate_complete(&self, root: &str) -> Result<(), PackValidationError> {
        self.validate_structure()?;
        for pose in SemanticPose::HUMANOID_REQUIRED {
            if !matches!(self.resolve(root, pose), ResolvedPose::Clip { pose: p, .. } if p == pose)
            {
                return Err(PackValidationError::MissingRequiredPose(pose));
            }
        }
        Ok(())
    }

    /// Resolves pack fallback first, then the deterministic semantic fallback
    /// chain. Missing packs and fully empty chains safely produce the T-pose.
    pub fn resolve(&self, root: &str, requested: SemanticPose) -> ResolvedPose<'_> {
        let mut semantic = Some(requested);
        let mut semantic_seen = HashSet::new();
        while let Some(pose) = semantic {
            if !semantic_seen.insert(pose) {
                break;
            }
            let mut pack_id = Some(root);
            let mut pack_seen = HashSet::new();
            while let Some(id) = pack_id {
                if !pack_seen.insert(id) {
                    break;
                }
                let Some(pack) = self.packs.get(id) else {
                    break;
                };
                if pack.clips.contains(&pose) {
                    return ResolvedPose::Clip {
                        pack_id: &pack.id,
                        pose,
                    };
                }
                pack_id = pack.fallback.as_deref();
            }
            semantic = pose.fallback();
        }
        ResolvedPose::BindPoseT
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum Posture {
    #[default]
    Upright,
    Crouched,
    Airborne,
    Prone,
    Supine,
    Ragdolled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum LeadFoot {
    #[default]
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum SkeletonAction {
    #[default]
    None,
    JumpCharge,
    Jump,
    Dodge,
    Attack,
    Block,
    HitReaction,
    ProneTransition,
    GetUp,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum StrikeFamily {
    #[default]
    Thrust,
    Slash,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum Footwork {
    #[default]
    Stay,
    Switch,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum AttackLine {
    #[default]
    Thrust,
    CutFromLeft,
    CutFromRight,
}

/// Minimum server-authored presentation state. Bone transforms and IK targets
/// intentionally do not cross the network boundary.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SkeletonState {
    pub posture: Posture,
    pub local_velocity: Vec3,
    pub grounded: bool,
    pub gait_phase: f32,
    pub lead_foot: LeadFoot,
    pub action: SkeletonAction,
    pub action_phase: f32,
    pub action_direction: Vec2,
    pub attack_target_height: f32,
    pub strike_family: StrikeFamily,
    pub footwork: Footwork,
    pub incoming_attack_line: AttackLine,
    pub animation_pack: String,
    pub action_started_tick: u64,
    pub action_contact_tick: u64,
}

impl Default for SkeletonState {
    fn default() -> Self {
        Self {
            posture: Posture::Upright,
            local_velocity: Vec3::ZERO,
            grounded: true,
            gait_phase: 0.0,
            lead_foot: LeadFoot::Left,
            action: SkeletonAction::None,
            action_phase: 0.0,
            action_direction: Vec2::ZERO,
            attack_target_height: 0.5,
            strike_family: StrikeFamily::Thrust,
            footwork: Footwork::Stay,
            incoming_attack_line: AttackLine::Thrust,
            animation_pack: "humanoid_unarmed".to_owned(),
            action_started_tick: 0,
            action_contact_tick: 0,
        }
    }
}

impl SkeletonState {
    pub fn begin_action(&mut self, action: SkeletonAction, start_tick: u64, contact_tick: u64) {
        self.action = action;
        self.action_phase = 0.0;
        self.action_started_tick = start_tick;
        self.action_contact_tick = contact_tick.max(start_tick + 1);
    }

    /// Advances an action whose contact is the midpoint of its visual
    /// timeline. Recovery gets the same bounded duration as preparation.
    pub fn advance_action(&mut self, current_tick: u64) {
        if self.action == SkeletonAction::None {
            return;
        }
        let preparation = self
            .action_contact_tick
            .saturating_sub(self.action_started_tick)
            .max(1);
        let end_tick = self.action_contact_tick.saturating_add(preparation);
        if current_tick >= end_tick {
            self.action = SkeletonAction::None;
            self.action_phase = 0.0;
            return;
        }
        self.action_phase = if current_tick <= self.action_contact_tick {
            0.5 * current_tick.saturating_sub(self.action_started_tick) as f32 / preparation as f32
        } else {
            0.5 + 0.5 * current_tick.saturating_sub(self.action_contact_tick) as f32
                / preparation as f32
        };
    }
}

/// One weighted authored pose contributing to the FK result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PoseSampling {
    /// Sample the pose's authoritative catalog frame.
    Anchor,
    /// Sample the complete cyclic motion containing this pose. The client
    /// maps normalized gait phase across the motion's catalog frame range.
    Cycle { progress: f32 },
    /// Sample between two semantic anchors. The client uses one exact clip
    /// time when both anchors belong to the same motion and blends otherwise.
    Span { end: SemanticPose, progress: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoseSample {
    pub pose: SemanticPose,
    pub sampling: PoseSampling,
    pub weight: f32,
    /// Continuous weight for exchanging and reflecting the authored left/right
    /// leg transforms when a resolved source lacks an authored opposite half.
    pub mirror_lower_body: f32,
}

/// Client-side blend coordinates derived from authoritative state.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationEvaluation {
    pub base: Vec<PoseSample>,
    pub action: Vec<PoseSample>,
    pub movement_speed: f32,
    pub gait_phase: f32,
    pub crouch_amount: f32,
    pub airborne_phase: f32,
    pub action_phase: f32,
    pub attack_target_height: f32,
}

impl AnimationEvaluation {
    /// Evaluates semantic FK inputs. Clip resolution, bone masks, IK, and the
    /// final procedural pass happen after this deterministic selection stage.
    pub fn from_skeleton(state: &SkeletonState) -> Self {
        let speed = state.local_velocity.xz().length();
        let gait_phase = state.gait_phase.rem_euclid(1.0);
        let crouch_amount = matches!(state.posture, Posture::Crouched) as u8 as f32;
        let base = match state.posture {
            Posture::Prone => gait_or_idle(
                speed,
                gait_phase,
                SemanticPose::ProneIdle,
                SemanticPose::ProneCrawlContact,
                SemanticPose::ProneCrawlPassing,
            ),
            Posture::Supine => gait_or_idle(
                speed,
                gait_phase,
                SemanticPose::SupineIdle,
                SemanticPose::SupineScamperContact,
                SemanticPose::SupineScamperPassing,
            ),
            Posture::Airborne => vec![airborne_sample(
                state.action_direction,
                state.local_velocity.y,
            )],
            Posture::Ragdolled => Vec::new(),
            Posture::Upright | Posture::Crouched => {
                locomotion_samples(speed, gait_phase, crouch_amount)
            }
        };
        let action = action_samples(state);
        Self {
            base,
            action,
            movement_speed: speed,
            gait_phase,
            crouch_amount,
            airborne_phase: (0.5 - state.local_velocity.y * 0.2).clamp(0.0, 1.0),
            action_phase: state.action_phase.clamp(0.0, 1.0),
            attack_target_height: state.attack_target_height.clamp(0.0, 1.0),
        }
    }
}

fn gait_or_idle(
    speed: f32,
    phase: f32,
    idle: SemanticPose,
    contact: SemanticPose,
    passing: SemanticPose,
) -> Vec<PoseSample> {
    if speed < 0.05 {
        vec![PoseSample {
            pose: idle,
            sampling: PoseSampling::Anchor,
            weight: 1.0,
            mirror_lower_body: 0.0,
        }]
    } else {
        gait_pair(phase, contact, passing)
    }
}

fn locomotion_samples(speed: f32, phase: f32, crouch: f32) -> Vec<PoseSample> {
    const WALK_REFERENCE_SPEED: f32 = 2.0;
    const RUN_REFERENCE_SPEED: f32 = 5.5;
    const LOCOMOTION_BLEND_SPEED: f32 = 0.75;
    let locomotion = smoothstep01(speed / LOCOMOTION_BLEND_SPEED);
    let run = ((speed - WALK_REFERENCE_SPEED) / (RUN_REFERENCE_SPEED - WALK_REFERENCE_SPEED))
        .clamp(0.0, 1.0);
    let mut samples = Vec::with_capacity(8);
    let mut idle = weighted_pair(SemanticPose::IdleRelaxed, SemanticPose::CrouchIdle, crouch);
    // The idle lower body is authored symmetrically. Give every contribution
    // the same reflection coordinate before Bevy blends it, so the client
    // never has to average incompatible mirrored and unmirrored semantics.
    for sample in &mut idle {
        sample.mirror_lower_body = gait_mirror(phase);
    }
    append_scaled(&mut samples, idle, 1.0 - locomotion);
    append_scaled(
        &mut samples,
        gait_pair(phase, SemanticPose::WalkContact, SemanticPose::WalkPassing),
        locomotion * (1.0 - run) * (1.0 - crouch),
    );
    append_scaled(
        &mut samples,
        gait_pair(phase, SemanticPose::RunContact, SemanticPose::RunFlight),
        locomotion * run * (1.0 - crouch),
    );
    append_scaled(
        &mut samples,
        gait_pair(
            phase,
            SemanticPose::CrouchWalkContact,
            SemanticPose::CrouchWalkPassing,
        ),
        locomotion * crouch,
    );
    samples.retain(|sample| sample.weight > f32::EPSILON);
    samples
}

fn append_scaled(into: &mut Vec<PoseSample>, samples: Vec<PoseSample>, scale: f32) {
    into.extend(samples.into_iter().map(|mut sample| {
        sample.weight *= scale;
        sample
    }));
}

fn weighted_pair(a: SemanticPose, b: SemanticPose, b_weight: f32) -> Vec<PoseSample> {
    let b_weight = b_weight.clamp(0.0, 1.0);
    let mut samples = Vec::with_capacity(2);
    if b_weight < 1.0 {
        samples.push(PoseSample {
            pose: a,
            sampling: PoseSampling::Anchor,
            weight: 1.0 - b_weight,
            mirror_lower_body: 0.0,
        });
    }
    if b_weight > 0.0 {
        samples.push(PoseSample {
            pose: b,
            sampling: PoseSampling::Anchor,
            weight: b_weight,
            mirror_lower_body: 0.0,
        });
    }
    samples
}

fn gait_mirror(phase: f32) -> f32 {
    let phase = phase.rem_euclid(1.0);
    if phase < 0.15 {
        0.0
    } else if phase < 0.35 {
        smoothstep01((phase - 0.15) / 0.20)
    } else if phase < 0.65 {
        1.0
    } else if phase < 0.85 {
        1.0 - smoothstep01((phase - 0.65) / 0.20)
    } else {
        0.0
    }
}

fn gait_pair(phase: f32, contact: SemanticPose, passing: SemanticPose) -> Vec<PoseSample> {
    let phase = phase.rem_euclid(1.0);
    let quarter = phase * 4.0;
    let index = quarter.floor() as u8;
    let progress = smoothstep01(quarter.fract());
    let (start, end) = match index {
        0 => (contact, passing),
        1 => (passing, contact),
        2 => (contact, passing),
        _ => (passing, contact),
    };
    // Swap anatomical sides only near each passing pose. A hard swap pops,
    // while blending throughout contact folds the planted stride through
    // itself. The 20%-cycle windows keep interpolation around the pose where
    // the legs are closest and preserve the authored contact silhouettes.
    let mirror = gait_mirror(phase);
    // Contact and passing are authoritative timestamps in the same sparse
    // motion file. Sample the interval directly instead of asking Bevy to
    // blend two times on one animation-graph node: a node has only one seek
    // position, so the latter sample would overwrite the former and create a
    // hard quarter-cycle pose swap.
    vec![PoseSample {
        pose: start,
        sampling: PoseSampling::Span { end, progress },
        weight: 1.0,
        mirror_lower_body: mirror,
    }]
}

fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn directional_jump_poses(direction: Vec2) -> [SemanticPose; 3] {
    use SemanticPose::*;
    let direction = if direction.length_squared() < 0.04 {
        0
    } else if direction.x.abs() > direction.y.abs() {
        if direction.x < 0.0 { 3 } else { 4 }
    } else if direction.y >= 0.0 {
        1
    } else {
        2
    };
    match direction {
        0 => [JumpCenterLaunch, JumpCenterFlight, JumpCenterLanding],
        1 => [JumpForwardLaunch, JumpForwardFlight, JumpForwardLanding],
        2 => [JumpBackwardLaunch, JumpBackwardFlight, JumpBackwardLanding],
        3 => [JumpLeftLaunch, JumpLeftFlight, JumpLeftLanding],
        _ => [JumpRightLaunch, JumpRightFlight, JumpRightLanding],
    }
}

fn airborne_sample(direction: Vec2, vertical_velocity: f32) -> PoseSample {
    let [launch, flight, landing] = directional_jump_poses(direction);
    let phase = (0.5 - vertical_velocity * 0.2).clamp(0.0, 1.0);
    let (pose, end, progress) = if phase < 0.5 {
        (launch, flight, phase * 2.0)
    } else {
        (flight, landing, (phase - 0.5) * 2.0)
    };
    PoseSample {
        pose,
        sampling: PoseSampling::Span { end, progress },
        weight: 1.0,
        mirror_lower_body: 0.0,
    }
}

fn out_and_back(start: SemanticPose, middle: SemanticPose, phase: f32) -> PoseSample {
    let phase = phase.clamp(0.0, 1.0);
    let (pose, end, progress) = if phase < 0.5 {
        (start, middle, phase * 2.0)
    } else {
        (middle, start, (phase - 0.5) * 2.0)
    };
    PoseSample {
        pose,
        sampling: PoseSampling::Span { end, progress },
        weight: 1.0,
        mirror_lower_body: 0.0,
    }
}

fn through_transition(
    start: SemanticPose,
    middle: SemanticPose,
    end: SemanticPose,
    phase: f32,
) -> PoseSample {
    let phase = phase.clamp(0.0, 1.0);
    let (pose, end, progress) = if phase < 0.5 {
        (start, middle, phase * 2.0)
    } else {
        (middle, end, (phase - 0.5) * 2.0)
    };
    PoseSample {
        pose,
        sampling: PoseSampling::Span { end, progress },
        weight: 1.0,
        mirror_lower_body: 0.0,
    }
}

fn action_samples(state: &SkeletonState) -> Vec<PoseSample> {
    match state.action {
        SkeletonAction::None | SkeletonAction::JumpCharge | SkeletonAction::Jump => Vec::new(),
        SkeletonAction::Dodge => {
            let pose = if state.action_direction.x.abs() > state.action_direction.y.abs() {
                if state.action_direction.x < 0.0 {
                    SemanticPose::DuckLeft
                } else {
                    SemanticPose::DuckRight
                }
            } else if state.action_direction.y < 0.0 {
                SemanticPose::DuckBackward
            } else {
                SemanticPose::DuckForward
            };
            vec![out_and_back(
                SemanticPose::CrouchIdle,
                pose,
                state.action_phase,
            )]
        }
        SkeletonAction::Attack => attack_samples(state),
        SkeletonAction::Block => vec![out_and_back(
            guard_pose(state.lead_foot),
            block_pose(state.incoming_attack_line, state.lead_foot),
            state.action_phase,
        )],
        SkeletonAction::HitReaction => Vec::new(),
        SkeletonAction::ProneTransition => vec![through_transition(
            SemanticPose::CrouchIdle,
            SemanticPose::UprightProneTransition,
            SemanticPose::ProneIdle,
            state.action_phase,
        )],
        SkeletonAction::GetUp => vec![through_transition(
            SemanticPose::ProneIdle,
            SemanticPose::UprightProneTransition,
            SemanticPose::CrouchIdle,
            state.action_phase,
        )],
    }
}

fn attack_samples(state: &SkeletonState) -> Vec<PoseSample> {
    let phase = state.action_phase.clamp(0.0, 1.0);
    let start_guard = guard_pose(state.lead_foot);
    let end_guard = guard_pose(match state.footwork {
        Footwork::Stay => state.lead_foot,
        Footwork::Switch => opposite(state.lead_foot),
    });
    let poses = [
        start_guard,
        attack_pose(state, 0),
        attack_pose(state, 1),
        attack_pose(state, 2),
        end_guard,
    ];
    let scaled = phase * 4.0;
    let segment = (scaled.floor() as usize).min(3);
    let blend = if phase >= 1.0 { 1.0 } else { scaled.fract() };
    vec![PoseSample {
        pose: poses[segment],
        sampling: PoseSampling::Span {
            end: poses[segment + 1],
            progress: blend,
        },
        weight: 1.0,
        mirror_lower_body: 0.0,
    }]
}

fn opposite(foot: LeadFoot) -> LeadFoot {
    match foot {
        LeadFoot::Left => LeadFoot::Right,
        LeadFoot::Right => LeadFoot::Left,
    }
}

fn guard_pose(lead: LeadFoot) -> SemanticPose {
    match lead {
        LeadFoot::Left => SemanticPose::GuardLeadLeft,
        LeadFoot::Right => SemanticPose::GuardLeadRight,
    }
}

fn block_pose(line: AttackLine, lead: LeadFoot) -> SemanticPose {
    use {AttackLine::*, LeadFoot::*, SemanticPose::*};
    match (line, lead) {
        (CutFromLeft, Left) => BlockCutLeftLeadLeft,
        (CutFromLeft, Right) => BlockCutLeftLeadRight,
        (CutFromRight, Left) => BlockCutRightLeadLeft,
        (CutFromRight, Right) => BlockCutRightLeadRight,
        (Thrust, Left) => BlockThrustLeadLeft,
        (Thrust, Right) => BlockThrustLeadRight,
    }
}

fn attack_pose(state: &SkeletonState, phase: u8) -> SemanticPose {
    use {Footwork::*, LeadFoot::*, SemanticPose::*, StrikeFamily::*};
    match (state.strike_family, state.lead_foot, state.footwork, phase) {
        (Thrust, Left, Stay, 0) => AttackThrustLeadLeftStayCommit,
        (Thrust, Left, Stay, 1) => AttackThrustLeadLeftStayContact,
        (Thrust, Left, Stay, _) => AttackThrustLeadLeftStayFollowThrough,
        (Thrust, Left, Switch, 0) => AttackThrustLeadLeftSwitchCommit,
        (Thrust, Left, Switch, 1) => AttackThrustLeadLeftSwitchContact,
        (Thrust, Left, Switch, _) => AttackThrustLeadLeftSwitchFollowThrough,
        (Thrust, Right, Stay, 0) => AttackThrustLeadRightStayCommit,
        (Thrust, Right, Stay, 1) => AttackThrustLeadRightStayContact,
        (Thrust, Right, Stay, _) => AttackThrustLeadRightStayFollowThrough,
        (Thrust, Right, Switch, 0) => AttackThrustLeadRightSwitchCommit,
        (Thrust, Right, Switch, 1) => AttackThrustLeadRightSwitchContact,
        (Thrust, Right, Switch, _) => AttackThrustLeadRightSwitchFollowThrough,
        (Slash, Left, Stay, 0) => AttackSlashLeadLeftStayCommit,
        (Slash, Left, Stay, 1) => AttackSlashLeadLeftStayContact,
        (Slash, Left, Stay, _) => AttackSlashLeadLeftStayFollowThrough,
        (Slash, Left, Switch, 0) => AttackSlashLeadLeftSwitchCommit,
        (Slash, Left, Switch, 1) => AttackSlashLeadLeftSwitchContact,
        (Slash, Left, Switch, _) => AttackSlashLeadLeftSwitchFollowThrough,
        (Slash, Right, Stay, 0) => AttackSlashLeadRightStayCommit,
        (Slash, Right, Stay, 1) => AttackSlashLeadRightStayContact,
        (Slash, Right, Stay, _) => AttackSlashLeadRightStayFollowThrough,
        (Slash, Right, Switch, 0) => AttackSlashLeadRightSwitchCommit,
        (Slash, Right, Switch, 1) => AttackSlashLeadRightSwitchContact,
        (Slash, Right, Switch, _) => AttackSlashLeadRightSwitchFollowThrough,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(
        id: &str,
        fallback: Option<&str>,
        clips: impl IntoIterator<Item = SemanticPose>,
    ) -> AnimationPack {
        AnimationPack {
            id: id.to_owned(),
            skeleton_family: "humanoid".to_owned(),
            fallback: fallback.map(str::to_owned),
            clips: clips.into_iter().collect(),
        }
    }

    #[test]
    fn pack_then_semantic_fallback_is_deterministic() {
        let mut library = AnimationPackLibrary::default();
        library
            .insert(pack("unarmed", None, [SemanticPose::WalkContact]))
            .unwrap();
        library
            .insert(pack("rapier", Some("unarmed"), [SemanticPose::RunFlight]))
            .unwrap();

        assert_eq!(
            library.resolve("rapier", SemanticPose::RunContact),
            ResolvedPose::Clip {
                pack_id: "unarmed",
                pose: SemanticPose::WalkContact,
            }
        );
        assert_eq!(
            library.resolve("rapier", SemanticPose::RunFlight),
            ResolvedPose::Clip {
                pack_id: "rapier",
                pose: SemanticPose::RunFlight,
            }
        );
    }

    #[test]
    fn thrust_falls_back_to_corresponding_slash_before_guard() {
        let mut library = AnimationPackLibrary::default();
        library
            .insert(pack(
                "unarmed",
                None,
                [SemanticPose::AttackSlashLeadRightSwitchContact],
            ))
            .unwrap();
        assert_eq!(
            library.resolve("unarmed", SemanticPose::AttackThrustLeadRightSwitchContact,),
            ResolvedPose::Clip {
                pack_id: "unarmed",
                pose: SemanticPose::AttackSlashLeadRightSwitchContact,
            }
        );
    }

    #[test]
    fn empty_or_unknown_pack_uses_bind_pose_t() {
        let mut library = AnimationPackLibrary::default();
        library.insert(pack("empty", None, [])).unwrap();
        assert_eq!(
            library.resolve("empty", SemanticPose::JumpLeftFlight),
            ResolvedPose::BindPoseT
        );
        assert_eq!(
            library.resolve("missing", SemanticPose::IdleRelaxed),
            ResolvedPose::BindPoseT
        );
    }

    #[test]
    fn invalid_fallback_graph_is_rejected() {
        let mut library = AnimationPackLibrary::default();
        library.insert(pack("a", Some("b"), [])).unwrap();
        library.insert(pack("b", Some("a"), [])).unwrap();
        assert_eq!(
            library.validate_structure(),
            Err(PackValidationError::FallbackCycle("a".to_owned()))
        );
    }

    #[test]
    fn locomotion_shares_phase_across_walk_and_run() {
        let state = SkeletonState {
            local_velocity: Vec3::new(3.75, 0.0, 0.0),
            gait_phase: 0.25,
            ..default()
        };
        let evaluation = AnimationEvaluation::from_skeleton(&state);
        assert_eq!(evaluation.base.len(), 2);
        assert!(evaluation.base.iter().any(|sample| {
            sample.pose == SemanticPose::WalkPassing
                && sample.sampling
                    == PoseSampling::Span {
                        end: SemanticPose::WalkContact,
                        progress: 0.0,
                    }
                && sample.weight == 0.5
        }));
        assert!(evaluation.base.iter().any(|sample| {
            sample.pose == SemanticPose::RunFlight
                && sample.sampling
                    == PoseSampling::Span {
                        end: SemanticPose::RunContact,
                        progress: 0.0,
                    }
                && sample.weight == 0.5
        }));
    }

    #[test]
    fn low_speed_idle_and_gait_contributions_share_mirror_semantics() {
        let evaluation = AnimationEvaluation::from_skeleton(&SkeletonState {
            local_velocity: Vec3::new(0.25, 0.0, 0.0),
            gait_phase: 0.25,
            ..default()
        });
        assert!(evaluation.base.len() >= 2);
        let mirror = evaluation.base[0].mirror_lower_body;
        assert!(
            evaluation
                .base
                .iter()
                .all(|sample| (sample.mirror_lower_body - mirror).abs() < 0.0001)
        );
    }

    #[test]
    fn gait_constructs_four_quarters_from_sparse_authoritative_anchors() {
        let samples = [0.0, 0.25, 0.5, 0.75]
            .map(|phase| gait_pair(phase, SemanticPose::WalkContact, SemanticPose::WalkPassing)[0]);
        assert_eq!(
            samples.map(|sample| sample.pose),
            [
                SemanticPose::WalkContact,
                SemanticPose::WalkPassing,
                SemanticPose::WalkContact,
                SemanticPose::WalkPassing,
            ]
        );
        assert_eq!(
            samples.map(|sample| sample.sampling),
            [
                PoseSampling::Span {
                    end: SemanticPose::WalkPassing,
                    progress: 0.0,
                },
                PoseSampling::Span {
                    end: SemanticPose::WalkContact,
                    progress: 0.0,
                },
                PoseSampling::Span {
                    end: SemanticPose::WalkPassing,
                    progress: 0.0,
                },
                PoseSampling::Span {
                    end: SemanticPose::WalkContact,
                    progress: 0.0,
                },
            ]
        );
        for (actual, expected) in samples
            .map(|sample| sample.mirror_lower_body)
            .into_iter()
            .zip([0.0, 0.5, 1.0, 0.5])
        {
            assert!((actual - expected).abs() < 0.0001);
        }
        let between = gait_pair(0.375, SemanticPose::WalkContact, SemanticPose::WalkPassing);
        assert_eq!(between.len(), 1);
        assert_eq!(
            between[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::WalkContact,
                progress: 0.5,
            }
        );
        assert_eq!(between[0].weight, 1.0);
        assert_eq!(between[0].mirror_lower_body, 1.0);
    }

    #[test]
    fn attack_blends_guard_commit_contact_follow_through_and_end_guard() {
        let state = SkeletonState {
            action: SkeletonAction::Attack,
            action_phase: 0.5,
            strike_family: StrikeFamily::Thrust,
            lead_foot: LeadFoot::Left,
            footwork: Footwork::Switch,
            ..default()
        };
        let evaluation = AnimationEvaluation::from_skeleton(&state);
        assert_eq!(
            evaluation.action,
            vec![PoseSample {
                pose: SemanticPose::AttackThrustLeadLeftSwitchContact,
                sampling: PoseSampling::Span {
                    end: SemanticPose::AttackThrustLeadLeftSwitchFollowThrough,
                    progress: 0.0,
                },
                weight: 1.0,
                mirror_lower_body: 0.0,
            }]
        );

        let end = AnimationEvaluation::from_skeleton(&SkeletonState {
            action_phase: 1.0,
            ..state
        });
        assert_eq!(
            end.action.last().unwrap().pose,
            SemanticPose::AttackThrustLeadLeftSwitchFollowThrough
        );
        assert_eq!(
            end.action.last().unwrap().sampling,
            PoseSampling::Span {
                end: SemanticPose::GuardLeadRight,
                progress: 1.0,
            }
        );
    }

    #[test]
    fn vertical_velocity_drives_continuous_airborne_spans() {
        for (velocity, pose, end, progress) in [
            (
                3.0,
                SemanticPose::JumpForwardLaunch,
                SemanticPose::JumpForwardFlight,
                0.0,
            ),
            (
                0.0,
                SemanticPose::JumpForwardFlight,
                SemanticPose::JumpForwardLanding,
                0.0,
            ),
            (
                -3.0,
                SemanticPose::JumpForwardFlight,
                SemanticPose::JumpForwardLanding,
                1.0,
            ),
        ] {
            let evaluation = AnimationEvaluation::from_skeleton(&SkeletonState {
                posture: Posture::Airborne,
                local_velocity: Vec3::new(0.0, velocity, 0.0),
                action_direction: Vec2::Y,
                ..default()
            });
            assert_eq!(evaluation.base[0].pose, pose);
            assert_eq!(
                evaluation.base[0].sampling,
                PoseSampling::Span { end, progress }
            );
        }
    }
    #[test]
    fn dodge_and_block_return_to_their_reference_stances() {
        let dodge = AnimationEvaluation::from_skeleton(&SkeletonState {
            action: SkeletonAction::Dodge,
            action_phase: 0.75,
            action_direction: Vec2::X,
            ..default()
        });
        assert_eq!(dodge.action[0].pose, SemanticPose::DuckRight);
        assert_eq!(
            dodge.action[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::CrouchIdle,
                progress: 0.5,
            }
        );

        let block = AnimationEvaluation::from_skeleton(&SkeletonState {
            action: SkeletonAction::Block,
            action_phase: 0.75,
            lead_foot: LeadFoot::Left,
            incoming_attack_line: AttackLine::Thrust,
            ..default()
        });
        assert_eq!(block.action[0].pose, SemanticPose::BlockThrustLeadLeft);
        assert_eq!(
            block.action[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::GuardLeadLeft,
                progress: 0.5,
            }
        );
    }

    #[test]
    fn prone_transition_and_get_up_use_opposite_coherent_timelines() {
        let down = AnimationEvaluation::from_skeleton(&SkeletonState {
            action: SkeletonAction::ProneTransition,
            action_phase: 0.75,
            ..default()
        });
        assert_eq!(down.action[0].pose, SemanticPose::UprightProneTransition);
        assert_eq!(
            down.action[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::ProneIdle,
                progress: 0.5,
            }
        );
        let up = AnimationEvaluation::from_skeleton(&SkeletonState {
            action: SkeletonAction::GetUp,
            action_phase: 0.75,
            ..default()
        });
        assert_eq!(up.action[0].pose, SemanticPose::UprightProneTransition);
        assert_eq!(
            up.action[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::CrouchIdle,
                progress: 0.5,
            }
        );
    }

    #[test]
    fn authoritative_action_clock_centers_contact_and_finishes_recovery() {
        let mut state = SkeletonState::default();
        state.begin_action(SkeletonAction::Attack, 10, 20);
        state.advance_action(15);
        assert_eq!(state.action_phase, 0.25);
        state.advance_action(20);
        assert_eq!(state.action_phase, 0.5);
        state.advance_action(25);
        assert_eq!(state.action_phase, 0.75);
        state.advance_action(30);
        assert_eq!(state.action, SkeletonAction::None);
    }
}
