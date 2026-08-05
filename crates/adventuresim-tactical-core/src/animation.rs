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
    DuckLeadLeftBackward,
    DuckLeadLeftLeft,
    DuckLeadLeftRight,
    DuckLeadRightBackward,
    DuckLeadRightLeft,
    DuckLeadRightRight,
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
    GuardWalkLeadLeft,
    GuardWalkLeadRight,
    GuardStrafeLeadLeftLeft,
    GuardStrafeLeadLeftRight,
    GuardStrafeLeadRightLeft,
    GuardStrafeLeadRightRight,
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
    pub const HUMANOID_REQUIRED: [Self; 80] = [
        Self::IdleRelaxed,
        Self::WalkContact,
        Self::WalkPassing,
        Self::RunContact,
        Self::RunFlight,
        Self::CrouchIdle,
        Self::CrouchWalkContact,
        Self::CrouchWalkPassing,
        Self::DuckForward,
        Self::DuckLeadLeftBackward,
        Self::DuckLeadLeftLeft,
        Self::DuckLeadLeftRight,
        Self::DuckLeadRightBackward,
        Self::DuckLeadRightLeft,
        Self::DuckLeadRightRight,
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
        Self::GuardWalkLeadLeft,
        Self::GuardWalkLeadRight,
        Self::GuardStrafeLeadLeftLeft,
        Self::GuardStrafeLeadLeftRight,
        Self::GuardStrafeLeadRightLeft,
        Self::GuardStrafeLeadRightRight,
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
            DuckLeadLeftBackward => "duck_lead_left_backward",
            DuckLeadLeftLeft => "duck_lead_left_left",
            DuckLeadLeftRight => "duck_lead_left_right",
            DuckLeadRightBackward => "duck_lead_right_backward",
            DuckLeadRightLeft => "duck_lead_right_left",
            DuckLeadRightRight => "duck_lead_right_right",
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
            GuardWalkLeadLeft => "guard_walk_lead_left",
            GuardWalkLeadRight => "guard_walk_lead_right",
            GuardStrafeLeadLeftLeft => "guard_strafe_lead_left_left",
            GuardStrafeLeadLeftRight => "guard_strafe_lead_left_right",
            GuardStrafeLeadRightLeft => "guard_strafe_lead_right_left",
            GuardStrafeLeadRightRight => "guard_strafe_lead_right_right",
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

    /// Authored whole-body counterpart that may satisfy this pose by
    /// reflection when the exact pose is absent from the same pack. Exact
    /// authored clips always win, so handed packs opt out simply by exporting
    /// both sides.
    pub fn mirrored_counterpart(self) -> Option<Self> {
        use SemanticPose::*;
        Some(match self {
            DuckLeadLeftBackward => DuckLeadRightBackward,
            DuckLeadLeftLeft => DuckLeadRightRight,
            DuckLeadLeftRight => DuckLeadRightLeft,
            DuckLeadRightBackward => DuckLeadLeftBackward,
            DuckLeadRightLeft => DuckLeadLeftRight,
            DuckLeadRightRight => DuckLeadLeftLeft,
            GuardLeadLeft => GuardLeadRight,
            GuardLeadRight => GuardLeadLeft,
            GuardWalkLeadLeft => GuardWalkLeadRight,
            GuardWalkLeadRight => GuardWalkLeadLeft,
            GuardStrafeLeadLeftLeft => GuardStrafeLeadRightRight,
            GuardStrafeLeadLeftRight => GuardStrafeLeadRightLeft,
            GuardStrafeLeadRightLeft => GuardStrafeLeadLeftRight,
            GuardStrafeLeadRightRight => GuardStrafeLeadLeftLeft,
            AttackThrustLeadLeftStayCommit => AttackThrustLeadRightStayCommit,
            AttackThrustLeadLeftStayContact => AttackThrustLeadRightStayContact,
            AttackThrustLeadLeftStayFollowThrough => AttackThrustLeadRightStayFollowThrough,
            AttackThrustLeadLeftSwitchCommit => AttackThrustLeadRightSwitchCommit,
            AttackThrustLeadLeftSwitchContact => AttackThrustLeadRightSwitchContact,
            AttackThrustLeadLeftSwitchFollowThrough => AttackThrustLeadRightSwitchFollowThrough,
            AttackThrustLeadRightStayCommit => AttackThrustLeadLeftStayCommit,
            AttackThrustLeadRightStayContact => AttackThrustLeadLeftStayContact,
            AttackThrustLeadRightStayFollowThrough => AttackThrustLeadLeftStayFollowThrough,
            AttackThrustLeadRightSwitchCommit => AttackThrustLeadLeftSwitchCommit,
            AttackThrustLeadRightSwitchContact => AttackThrustLeadLeftSwitchContact,
            AttackThrustLeadRightSwitchFollowThrough => AttackThrustLeadLeftSwitchFollowThrough,
            AttackSlashLeadLeftStayCommit => AttackSlashLeadRightStayCommit,
            AttackSlashLeadLeftStayContact => AttackSlashLeadRightStayContact,
            AttackSlashLeadLeftStayFollowThrough => AttackSlashLeadRightStayFollowThrough,
            AttackSlashLeadLeftSwitchCommit => AttackSlashLeadRightSwitchCommit,
            AttackSlashLeadLeftSwitchContact => AttackSlashLeadRightSwitchContact,
            AttackSlashLeadLeftSwitchFollowThrough => AttackSlashLeadRightSwitchFollowThrough,
            AttackSlashLeadRightStayCommit => AttackSlashLeadLeftStayCommit,
            AttackSlashLeadRightStayContact => AttackSlashLeadLeftStayContact,
            AttackSlashLeadRightStayFollowThrough => AttackSlashLeadLeftStayFollowThrough,
            AttackSlashLeadRightSwitchCommit => AttackSlashLeadLeftSwitchCommit,
            AttackSlashLeadRightSwitchContact => AttackSlashLeadLeftSwitchContact,
            AttackSlashLeadRightSwitchFollowThrough => AttackSlashLeadLeftSwitchFollowThrough,
            _ => return None,
        })
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
            DuckForward => CrouchIdle,
            DuckLeadLeftBackward | DuckLeadLeftLeft | DuckLeadLeftRight => GuardLeadLeft,
            DuckLeadRightBackward | DuckLeadRightLeft | DuckLeadRightRight => GuardLeadRight,
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
            GuardLeadRight => IdleRelaxed,
            GuardWalkLeadLeft => GuardLeadLeft,
            GuardWalkLeadRight => GuardLeadRight,
            GuardStrafeLeadLeftLeft | GuardStrafeLeadLeftRight => GuardWalkLeadLeft,
            GuardStrafeLeadRightLeft | GuardStrafeLeadRightRight => GuardWalkLeadRight,
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
            AttackSlashLeadRightStayCommit
            | AttackSlashLeadRightStayContact
            | AttackSlashLeadRightStayFollowThrough => GuardLeadRight,
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
        /// Semantic pose satisfied before any ordinary semantic fallback.
        semantic: SemanticPose,
        /// Authored clip sampled for that semantic pose.
        pose: SemanticPose,
        mirrored: bool,
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
            if !matches!(self.resolve(root, pose), ResolvedPose::Clip { semantic, .. } if semantic == pose)
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
                        semantic: pose,
                        pose,
                        mirrored: false,
                    };
                }
                if let Some(source) = pose.mirrored_counterpart()
                    && pack.clips.contains(&source)
                {
                    return ResolvedPose::Clip {
                        pack_id: &pack.id,
                        semantic: pose,
                        pose: source,
                        mirrored: true,
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
pub enum WeaponGuardState {
    #[default]
    Lowered,
    Raised,
}

/// Compact authoritative input for client-side raised-guard foot placement.
/// Speed follows the controller continuously so acceleration changes cadence
/// during the current step. Ordinary turns wait for the next foot handoff;
/// material opposite-direction reversals perform an immediate safe semantic
/// handoff so the support side agrees with the already-reversed gameplay root.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
pub struct RaisedLocomotionIntent {
    pub active: bool,
    pub local_direction: Vec2,
    pub speed: f32,
    /// Swing-side state is independent from the fixed guard lead.
    pub swing_foot: LeadFoot,
    /// Monotonic semantic handoff identity. Clients use this to detect
    /// coalesced updates without treating gait-phase parity as step identity.
    pub step_sequence: u32,
}

impl RaisedLocomotionIntent {
    pub fn local_velocity(self) -> Vec3 {
        Vec3::new(self.local_direction.x, 0.0, self.local_direction.y) * self.speed
    }
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

/// Typed locomotion families shared by authoritative cadence projection and
/// client-only presentation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum LocomotionGait {
    #[default]
    Walk,
    Run,
    Crouch,
    RaisedGuard,
}

/// Compact gait dynamics metadata. Phase 0..1 is one complete left/right
/// cycle; contact phases are 0 and 0.5.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub struct LocomotionProfile {
    pub gait: LocomotionGait,
    pub reference_speed: f32,
    pub step_distance: f32,
    /// Radius around each contact phase that can carry support.
    pub support_phase_radius: f32,
    /// Visual grounded bounce, in metres.
    pub bounce_metres: f32,
    /// Visual unsupported apex, in metres. Zero means a grounded curve.
    pub flight_apex_metres: f32,
    pub landing: LandingProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub struct LandingProfile {
    pub compression_per_metre_per_second: f32,
    pub minimum_compression_metres: f32,
    pub maximum_compression_metres: f32,
    pub recovery_seconds: f32,
}

pub const HUMANOID_LANDING_PROFILE: LandingProfile = LandingProfile {
    compression_per_metre_per_second: 0.012,
    minimum_compression_metres: 0.04,
    maximum_compression_metres: 0.08,
    recovery_seconds: 0.16,
};

pub const LOCOMOTION_SAMPLE_HZ: f32 = 64.0;

pub const WALK_LOCOMOTION_PROFILE: LocomotionProfile = LocomotionProfile {
    gait: LocomotionGait::Walk,
    reference_speed: 2.0,
    step_distance: 1.22,
    support_phase_radius: 0.28,
    bounce_metres: 0.04,
    flight_apex_metres: 0.0,
    landing: HUMANOID_LANDING_PROFILE,
};
pub const RUN_LOCOMOTION_PROFILE: LocomotionProfile = LocomotionProfile {
    gait: LocomotionGait::Run,
    reference_speed: 5.5,
    step_distance: 1.78,
    support_phase_radius: 0.175,
    bounce_metres: 0.0,
    flight_apex_metres: 0.09,
    landing: HUMANOID_LANDING_PROFILE,
};
pub const CROUCH_LOCOMOTION_PROFILE: LocomotionProfile = LocomotionProfile {
    gait: LocomotionGait::Crouch,
    reference_speed: 1.5,
    step_distance: 1.14,
    support_phase_radius: 0.30,
    bounce_metres: 0.025,
    flight_apex_metres: 0.0,
    landing: HUMANOID_LANDING_PROFILE,
};
pub const RAISED_GUARD_LOCOMOTION_PROFILE: LocomotionProfile = LocomotionProfile {
    gait: LocomotionGait::RaisedGuard,
    reference_speed: 2.0,
    step_distance: 0.38,
    support_phase_radius: 0.25,
    bounce_metres: 0.03,
    flight_apex_metres: 0.0,
    landing: HUMANOID_LANDING_PROFILE,
};

pub fn locomotion_profile(state: &SkeletonState) -> LocomotionProfile {
    let speed = state.animation_speed();
    if state.posture == Posture::Crouched {
        return CROUCH_LOCOMOTION_PROFILE;
    }
    if state.weapon_guard == WeaponGuardState::Raised {
        return LocomotionProfile {
            step_distance: guard_step_length(speed),
            ..RAISED_GUARD_LOCOMOTION_PROFILE
        };
    }
    let run = ((speed - WALK_LOCOMOTION_PROFILE.reference_speed)
        / (RUN_LOCOMOTION_PROFILE.reference_speed - WALK_LOCOMOTION_PROFILE.reference_speed))
        .clamp(0.0, 1.0);
    LocomotionProfile {
        gait: if run >= 0.5 {
            LocomotionGait::Run
        } else {
            LocomotionGait::Walk
        },
        reference_speed: WALK_LOCOMOTION_PROFILE
            .reference_speed
            .lerp(RUN_LOCOMOTION_PROFILE.reference_speed, run),
        step_distance: ordinary_step_distance(speed),
        support_phase_radius: WALK_LOCOMOTION_PROFILE
            .support_phase_radius
            .lerp(RUN_LOCOMOTION_PROFILE.support_phase_radius, run),
        bounce_metres: WALK_LOCOMOTION_PROFILE.bounce_metres * (1.0 - run),
        flight_apex_metres: RUN_LOCOMOTION_PROFILE.flight_apex_metres * run,
        landing: HUMANOID_LANDING_PROFILE,
    }
}

/// Shared distance model through authored walk/run reference points. This
/// replaces duplicated cadence arithmetic without changing current timing.
pub fn ordinary_step_distance(speed: f32) -> f32 {
    let speed = speed.max(0.0);
    if speed <= WALK_LOCOMOTION_PROFILE.reference_speed {
        0.9_f32.lerp(
            WALK_LOCOMOTION_PROFILE.step_distance,
            speed / WALK_LOCOMOTION_PROFILE.reference_speed,
        )
    } else {
        let blend = ((speed - WALK_LOCOMOTION_PROFILE.reference_speed)
            / (RUN_LOCOMOTION_PROFILE.reference_speed - WALK_LOCOMOTION_PROFILE.reference_speed))
            .clamp(0.0, 1.0);
        WALK_LOCOMOTION_PROFILE
            .step_distance
            .lerp(RUN_LOCOMOTION_PROFILE.step_distance, blend)
    }
}

pub fn gait_cycle_phase_delta(profile: LocomotionProfile, speed: f32, delta_seconds: f32) -> f32 {
    speed.max(0.0) * delta_seconds.max(0.0) / (profile.step_distance.max(0.01) * 2.0)
}

pub fn gait_support_weights(profile: LocomotionProfile, phase: f32) -> (f32, f32) {
    if profile.gait == LocomotionGait::RaisedGuard {
        return (1.0, 1.0);
    }
    let support = |contact: f32| {
        let distance = {
            let delta = (phase - contact).abs();
            delta.min(1.0 - delta)
        };
        (1.0 - smoothstep(
            profile.support_phase_radius * 0.45,
            profile.support_phase_radius,
            distance,
        ))
        .clamp(0.0, 1.0)
    };
    (support(0.0), support(0.5))
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0).max(f32::EPSILON)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
    pub world_velocity: Vec3,
    pub grounded: bool,
    pub gait_phase: f32,
    pub locomotion_sample_tick: u64,
    pub world_acceleration: Vec3,
    pub contact_sequence: u64,
    pub contact_foot: LeadFoot,
    pub landing_sequence: u64,
    pub landing_impact_speed: f32,
    pub lead_foot: LeadFoot,
    pub weapon_guard: WeaponGuardState,
    pub raised_locomotion: RaisedLocomotionIntent,
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
            world_velocity: Vec3::ZERO,
            grounded: true,
            gait_phase: 0.0,
            locomotion_sample_tick: 0,
            world_acceleration: Vec3::ZERO,
            contact_sequence: 0,
            contact_foot: LeadFoot::Left,
            landing_sequence: 0,
            landing_impact_speed: 0.0,
            lead_foot: LeadFoot::Left,
            weapon_guard: WeaponGuardState::Lowered,
            raised_locomotion: RaisedLocomotionIntent::default(),
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

/// Applies authoritative guard state and aligns a newly raised stance with
/// the static-guard endpoint shared by every directional shuttle.
pub fn set_weapon_guard(skeleton: &mut SkeletonState, weapon_guard: WeaponGuardState) {
    if skeleton.weapon_guard == WeaponGuardState::Lowered
        && weapon_guard == WeaponGuardState::Raised
    {
        skeleton.gait_phase = 0.0;
        skeleton.raised_locomotion = RaisedLocomotionIntent::default();
    }
    if weapon_guard == WeaponGuardState::Lowered {
        skeleton.raised_locomotion = RaisedLocomotionIntent::default();
    }
    skeleton.weapon_guard = weapon_guard;
}

impl SkeletonState {
    /// Presentation motion finishes an in-flight raised-guard step after
    /// gameplay velocity stops. Speed otherwise follows authoritative motion.
    pub fn animation_local_velocity(&self) -> Vec3 {
        if self.weapon_guard == WeaponGuardState::Raised && self.raised_locomotion.active {
            self.raised_locomotion.local_velocity()
        } else {
            self.local_velocity
        }
    }

    pub fn animation_speed(&self) -> f32 {
        self.animation_local_velocity().xz().length()
    }

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

/// One authoritative fixed-tick locomotion observation. The tactical server
/// supplies this from its character controller; deterministic presentation
/// fixtures replay the same boundary without inventing gait phase directly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkeletonLocomotionInput {
    pub orientation: Quat,
    pub linear_velocity: Vec3,
    pub grounded: bool,
    pub crouching: bool,
    pub delta_seconds: f32,
    pub tick: u64,
}

/// Maximum server-authoritative body turn speed during ordinary locomotion.
pub const BODY_TURN_SPEED_RADIANS: f32 = std::f32::consts::PI / 0.25;

/// Returns the controller's yaw without allowing camera pitch or roll to tilt
/// planar locomotion into or out of the ground plane.
pub fn controller_yaw(orientation: Quat) -> Quat {
    let forward = orientation * Vec3::NEG_Z;
    let Some(flat_forward) = forward.xz().try_normalize() else {
        return Quat::IDENTITY;
    };
    Quat::from_rotation_y((-flat_forward.x).atan2(-flat_forward.y))
}

/// Advances the authored body's +Z forward axis toward its single desired
/// world direction. Attack and block are the current guard boundary and keep
/// look-facing; all other moving states face authoritative planar velocity.
/// At exactly 180 degrees the positive turn direction is chosen consistently,
/// avoiding a normalize-through-zero snap.
pub fn advance_body_facing(
    current: Quat,
    controller_orientation: Quat,
    linear_velocity: Vec3,
    action: SkeletonAction,
    weapon_guard: WeaponGuardState,
    delta_seconds: f32,
) -> Quat {
    let current_yaw = body_yaw(current);
    let desired_forward = if weapon_guard == WeaponGuardState::Raised
        || matches!(action, SkeletonAction::Attack | SkeletonAction::Block)
    {
        controller_yaw(controller_orientation) * Vec3::NEG_Z
    } else {
        if linear_velocity.xz().length() <= 0.05 {
            return Quat::from_rotation_y(current_yaw);
        }
        let Some(direction) = linear_velocity.xz().try_normalize() else {
            return Quat::from_rotation_y(current_yaw);
        };
        Vec3::new(direction.x, 0.0, direction.y)
    };
    let desired_yaw = desired_forward.x.atan2(desired_forward.z);
    let mut delta = (desired_yaw - current_yaw + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    if (delta + std::f32::consts::PI).abs() <= 1.0e-5 {
        delta = std::f32::consts::PI;
    }
    let maximum = (BODY_TURN_SPEED_RADIANS * delta_seconds.max(0.0)).min(std::f32::consts::PI);
    Quat::from_rotation_y(current_yaw + delta.clamp(-maximum, maximum))
}

fn body_yaw(rotation: Quat) -> f32 {
    let forward = rotation * Vec3::Z;
    forward.x.atan2(forward.z)
}

/// Projects controller motion into the compact replicated animation state.
/// Bone evaluation remains client-only; this is the shared server seam that
/// keeps deterministic captures on the same stride and posture rules.
pub fn project_skeleton_locomotion(skeleton: &mut SkeletonState, input: SkeletonLocomotionInput) {
    let previous_world_velocity = skeleton.world_velocity;
    let was_grounded = skeleton.grounded;
    let previous_guard_sequence = skeleton.raised_locomotion.step_sequence;
    let previous_guard_swing = skeleton
        .raised_locomotion
        .active
        .then_some(skeleton.raised_locomotion.swing_foot);
    let local_velocity = controller_yaw(input.orientation).inverse() * input.linear_velocity;
    let physical_speed = local_velocity.xz().length();
    let contiguous_sample = input.tick == skeleton.locomotion_sample_tick.wrapping_add(1);
    skeleton.world_acceleration = if contiguous_sample {
        ((input.linear_velocity - previous_world_velocity) * LOCOMOTION_SAMPLE_HZ)
            .clamp_length_max(80.0)
    } else {
        Vec3::ZERO
    };
    skeleton.local_velocity = local_velocity;
    skeleton.world_velocity = input.linear_velocity;
    skeleton.grounded = input.grounded;
    skeleton.locomotion_sample_tick = input.tick;
    if !was_grounded && input.grounded {
        skeleton.landing_sequence = skeleton.landing_sequence.wrapping_add(1);
        skeleton.landing_impact_speed = (-previous_world_velocity.y).max(0.0);
    }
    skeleton.posture = if input.grounded {
        if input.crouching {
            Posture::Crouched
        } else {
            Posture::Upright
        }
    } else {
        Posture::Airborne
    };

    let ground_speed = physical_speed;
    if skeleton.weapon_guard == WeaponGuardState::Raised && skeleton.posture == Posture::Upright {
        advance_raised_locomotion_intent(skeleton, local_velocity, input.delta_seconds);
        let handoffs = skeleton
            .raised_locomotion
            .step_sequence
            .wrapping_sub(previous_guard_sequence);
        advance_contact_identity(skeleton, handoffs, previous_guard_swing);
    } else {
        skeleton.raised_locomotion = RaisedLocomotionIntent::default();
        if input.grounded && ground_speed > 0.05 {
            let profile = locomotion_profile(skeleton);
            let phase = skeleton.gait_phase.rem_euclid(1.0);
            let next_phase =
                phase + gait_cycle_phase_delta(profile, ground_speed, input.delta_seconds);
            let handoffs = ((next_phase * 2.0).floor() - (phase * 2.0).floor()).max(0.0) as u32;
            skeleton.gait_phase = next_phase.rem_euclid(1.0);
            advance_contact_identity(skeleton, handoffs, None);
            if skeleton.weapon_guard == WeaponGuardState::Lowered {
                skeleton.lead_foot = if skeleton.gait_phase < 0.5 {
                    LeadFoot::Left
                } else {
                    LeadFoot::Right
                };
            }
        }
    }
    skeleton.advance_action(input.tick);
}

fn advance_contact_identity(
    skeleton: &mut SkeletonState,
    handoffs: u32,
    first_contact: Option<LeadFoot>,
) {
    for handoff in 0..handoffs {
        skeleton.contact_sequence = skeleton.contact_sequence.wrapping_add(1);
        skeleton.contact_foot = if handoff == 0 {
            first_contact.unwrap_or_else(|| opposite_foot(skeleton.contact_foot))
        } else {
            opposite_foot(skeleton.contact_foot)
        };
    }
}

fn opposite_foot(foot: LeadFoot) -> LeadFoot {
    match foot {
        LeadFoot::Left => LeadFoot::Right,
        LeadFoot::Right => LeadFoot::Left,
    }
}

fn advance_raised_locomotion_intent(
    skeleton: &mut SkeletonState,
    observed_local_velocity: Vec3,
    delta_seconds: f32,
) {
    let observed_speed = observed_local_velocity.xz().length();
    let observed = (observed_speed > 0.05).then(|| RaisedLocomotionIntent {
        active: true,
        local_direction: Vec2::new(observed_local_velocity.x, observed_local_velocity.z)
            .normalize_or_zero(),
        speed: observed_speed,
        swing_foot: skeleton.lead_foot,
        step_sequence: skeleton.raised_locomotion.step_sequence,
    });
    if !skeleton.raised_locomotion.active {
        let Some(observed) = observed else {
            skeleton.gait_phase = 0.0;
            return;
        };
        skeleton.raised_locomotion = RaisedLocomotionIntent {
            swing_foot: initial_guard_swing_foot(observed.local_direction, skeleton.lead_foot),
            ..observed
        };
        skeleton.gait_phase = 0.0;
    }

    if let Some(observed) = observed {
        // Do not latch the tiny velocity from the first acceleration tick for
        // a complete pulse. Cadence and reach adapt immediately; only a hard
        // direction change waits until the current swing foot lands.
        skeleton.raised_locomotion.speed = observed.speed;
        if skeleton
            .raised_locomotion
            .local_direction
            .dot(observed.local_direction)
            < -0.5
        {
            // Gameplay root velocity reverses immediately. Hand support off
            // immediately too, rather than dragging the old world plant
            // across its anatomical corridor until the scheduled seam.
            skeleton.raised_locomotion.local_direction = observed.local_direction;
            skeleton.raised_locomotion.swing_foot = match skeleton.raised_locomotion.swing_foot {
                LeadFoot::Left => LeadFoot::Right,
                LeadFoot::Right => LeadFoot::Left,
            };
            skeleton.raised_locomotion.step_sequence =
                skeleton.raised_locomotion.step_sequence.wrapping_add(1);
            skeleton.gait_phase = if skeleton.gait_phase < 0.5 { 0.5 } else { 0.0 };
            return;
        }
    }
    let speed = skeleton.raised_locomotion.speed;
    let phase = skeleton.gait_phase.rem_euclid(1.0);
    let profile = LocomotionProfile {
        step_distance: guard_step_length(speed),
        ..RAISED_GUARD_LOCOMOTION_PROFILE
    };
    let next_phase = phase + gait_cycle_phase_delta(profile, speed, delta_seconds.max(0.0));
    let handoffs = ((next_phase * 2.0).floor() - (phase * 2.0).floor()).max(0.0) as u32;
    let crossed_handoff = handoffs > 0;

    if observed.is_none() && crossed_handoff {
        let step_sequence = skeleton.raised_locomotion.step_sequence.wrapping_add(1);
        skeleton.raised_locomotion = RaisedLocomotionIntent {
            step_sequence,
            ..default()
        };
        skeleton.gait_phase = if phase < 0.5 { 0.5 } else { 0.0 };
        return;
    }

    skeleton.gait_phase = next_phase.rem_euclid(1.0);
    if crossed_handoff && let Some(observed) = observed {
        if handoffs % 2 == 1 {
            skeleton.raised_locomotion.swing_foot = match skeleton.raised_locomotion.swing_foot {
                LeadFoot::Left => LeadFoot::Right,
                LeadFoot::Right => LeadFoot::Left,
            };
        }
        skeleton.raised_locomotion.step_sequence = skeleton
            .raised_locomotion
            .step_sequence
            .wrapping_add(handoffs);
        skeleton.raised_locomotion.local_direction = observed.local_direction;
    }
}

fn initial_guard_swing_foot(direction: Vec2, lead: LeadFoot) -> LeadFoot {
    if direction.x.abs() >= direction.y.abs() {
        if direction.x.is_sign_negative() {
            LeadFoot::Left
        } else {
            LeadFoot::Right
        }
    } else if direction.y.is_sign_positive() {
        // Retreat begins with the forward foot; advance begins with the rear.
        lead
    } else {
        match lead {
            LeadFoot::Left => LeadFoot::Right,
            LeadFoot::Right => LeadFoot::Left,
        }
    }
}

/// Ground distance covered by one procedural combat-stance step. Raised
/// movement uses compact shuffles rather than ordinary walking strides.
pub fn guard_step_length(speed: f32) -> f32 {
    (0.26 + speed.max(0.0) * 0.06).clamp(0.28, 0.42)
}

/// One weighted authored pose contributing to the FK result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PoseSampling {
    /// Sample the pose's authoritative catalog frame.
    Anchor,
    /// Sample the complete cyclic motion containing this pose. The client
    /// maps normalized gait phase across the motion's catalog frame range.
    Cycle { progress: f32 },
    /// Blend two semantic anchor poses. The client samples both catalog frames
    /// exactly and never evaluates exported in-between keys.
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
        let speed = state.animation_speed();
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
            Posture::Upright if state.weapon_guard == WeaponGuardState::Raised => {
                raised_guard_locomotion_samples(
                    state.animation_local_velocity(),
                    gait_phase,
                    state.lead_foot,
                )
            }
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

fn raised_guard_locomotion_samples(
    _local_velocity: Vec3,
    _phase: f32,
    lead: LeadFoot,
) -> Vec<PoseSample> {
    vec![PoseSample {
        pose: guard_pose(lead),
        sampling: PoseSampling::Anchor,
        weight: 1.0,
        mirror_lower_body: 0.0,
    }]
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
    // Contact and passing are authoritative sparse poses. The client gives
    // each catalog frame its own Bevy graph node, samples both endpoints
    // exactly, and uses this progress as their blend weight. Exported
    // in-between keys therefore cannot change procedural gait timing.
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
                duck_side_pose(state.lead_foot, state.action_direction.x < 0.0)
            } else if state.action_direction.y < 0.0 {
                match state.lead_foot {
                    LeadFoot::Left => SemanticPose::DuckLeadLeftBackward,
                    LeadFoot::Right => SemanticPose::DuckLeadRightBackward,
                }
            } else {
                SemanticPose::DuckForward
            };
            vec![out_and_back(
                guard_pose(state.lead_foot),
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

fn duck_side_pose(lead: LeadFoot, duck_left: bool) -> SemanticPose {
    match (lead, duck_left) {
        (LeadFoot::Left, true) => SemanticPose::DuckLeadLeftLeft,
        (LeadFoot::Left, false) => SemanticPose::DuckLeadLeftRight,
        (LeadFoot::Right, true) => SemanticPose::DuckLeadRightLeft,
        (LeadFoot::Right, false) => SemanticPose::DuckLeadRightRight,
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

    fn raised_intent(local_velocity: Vec3) -> RaisedLocomotionIntent {
        let speed = local_velocity.xz().length();
        RaisedLocomotionIntent {
            active: speed > 0.05,
            local_direction: Vec2::new(local_velocity.x, local_velocity.z).normalize_or_zero(),
            speed,
            swing_foot: LeadFoot::Left,
            step_sequence: 0,
        }
    }

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
                semantic: SemanticPose::WalkContact,
                pose: SemanticPose::WalkContact,
                mirrored: false,
            }
        );
        assert_eq!(
            library.resolve("rapier", SemanticPose::RunFlight),
            ResolvedPose::Clip {
                pack_id: "rapier",
                semantic: SemanticPose::RunFlight,
                pose: SemanticPose::RunFlight,
                mirrored: false,
            }
        );
    }

    #[test]
    fn missing_side_resolves_to_mirrored_authored_counterpart() {
        let mut library = AnimationPackLibrary::default();
        library
            .insert(pack("unarmed", None, [SemanticPose::GuardLeadLeft]))
            .unwrap();

        assert_eq!(
            library.resolve("unarmed", SemanticPose::GuardLeadRight),
            ResolvedPose::Clip {
                pack_id: "unarmed",
                semantic: SemanticPose::GuardLeadRight,
                pose: SemanticPose::GuardLeadLeft,
                mirrored: true,
            }
        );
    }

    #[test]
    fn mirrored_semantic_counterparts_are_involutions() {
        for pose in SemanticPose::HUMANOID_REQUIRED {
            let Some(counterpart) = pose.mirrored_counterpart() else {
                continue;
            };
            assert_ne!(pose, counterpart);
            assert_eq!(counterpart.mirrored_counterpart(), Some(pose));
        }
    }

    #[test]
    fn specialized_pack_mirrors_its_own_counterpart_before_parent_fallback() {
        let mut library = AnimationPackLibrary::default();
        library
            .insert(pack("unarmed", None, [SemanticPose::GuardLeadRight]))
            .unwrap();
        library
            .insert(pack(
                "sword",
                Some("unarmed"),
                [SemanticPose::GuardLeadLeft],
            ))
            .unwrap();

        assert_eq!(
            library.resolve("sword", SemanticPose::GuardLeadRight),
            ResolvedPose::Clip {
                pack_id: "sword",
                semantic: SemanticPose::GuardLeadRight,
                pose: SemanticPose::GuardLeadLeft,
                mirrored: true,
            }
        );
    }

    #[test]
    fn authored_opposite_side_wins_over_mirroring() {
        let mut library = AnimationPackLibrary::default();
        library
            .insert(pack(
                "sword",
                None,
                [SemanticPose::GuardLeadLeft, SemanticPose::GuardLeadRight],
            ))
            .unwrap();

        assert_eq!(
            library.resolve("sword", SemanticPose::GuardLeadRight),
            ResolvedPose::Clip {
                pack_id: "sword",
                semantic: SemanticPose::GuardLeadRight,
                pose: SemanticPose::GuardLeadRight,
                mirrored: false,
            }
        );
    }

    #[test]
    fn guard_locomotion_prefers_exact_then_same_pack_mirror_then_parent() {
        let mut library = AnimationPackLibrary::default();
        library
            .insert(pack(
                "unarmed",
                None,
                [
                    SemanticPose::GuardWalkLeadRight,
                    SemanticPose::GuardStrafeLeadRightRight,
                ],
            ))
            .unwrap();
        library
            .insert(pack(
                "sword",
                Some("unarmed"),
                [
                    SemanticPose::GuardWalkLeadLeft,
                    SemanticPose::GuardStrafeLeadLeftLeft,
                ],
            ))
            .unwrap();

        assert_eq!(
            library.resolve("sword", SemanticPose::GuardWalkLeadRight),
            ResolvedPose::Clip {
                pack_id: "sword",
                semantic: SemanticPose::GuardWalkLeadRight,
                pose: SemanticPose::GuardWalkLeadLeft,
                mirrored: true,
            }
        );
        assert_eq!(
            library.resolve("sword", SemanticPose::GuardStrafeLeadLeftLeft),
            ResolvedPose::Clip {
                pack_id: "sword",
                semantic: SemanticPose::GuardStrafeLeadLeftLeft,
                pose: SemanticPose::GuardStrafeLeadLeftLeft,
                mirrored: false,
            }
        );
    }

    #[test]
    fn missing_guard_strafe_falls_back_to_same_lead_walk_then_guard() {
        let mut library = AnimationPackLibrary::default();
        library
            .insert(pack("unarmed", None, [SemanticPose::GuardWalkLeadLeft]))
            .unwrap();
        assert_eq!(
            library.resolve("unarmed", SemanticPose::GuardStrafeLeadLeftRight),
            ResolvedPose::Clip {
                pack_id: "unarmed",
                semantic: SemanticPose::GuardWalkLeadLeft,
                pose: SemanticPose::GuardWalkLeadLeft,
                mirrored: false,
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
                semantic: SemanticPose::AttackSlashLeadRightSwitchContact,
                pose: SemanticPose::AttackSlashLeadRightSwitchContact,
                mirrored: false,
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
    fn locomotion_projection_uses_controller_frame_and_fixed_stride() {
        let mut state = SkeletonState::default();
        let orientation = Quat::from_rotation_y(std::f32::consts::PI);
        let local_velocity = Vec3::NEG_Z * 2.0;
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation,
                linear_velocity: orientation * local_velocity,
                grounded: true,
                crouching: false,
                delta_seconds: 1.0 / 64.0,
                tick: 1,
            },
        );

        assert!(state.grounded);
        assert_eq!(state.posture, Posture::Upright);
        assert!((state.local_velocity - local_velocity).length() < 0.0001);
        assert!(
            (state.gait_phase - gait_cycle_phase_delta(WALK_LOCOMOTION_PROFILE, 2.0, 1.0 / 64.0))
                .abs()
                < 0.0001
        );
        assert_eq!(state.lead_foot, LeadFoot::Left);
    }

    #[test]
    fn shared_profiles_own_cadence_support_and_flight() {
        assert!(
            (ordinary_step_distance(2.0) - WALK_LOCOMOTION_PROFILE.step_distance).abs() < 0.0001
        );
        assert!(
            (ordinary_step_distance(5.5) - RUN_LOCOMOTION_PROFILE.step_distance).abs() < 0.0001
        );
        let (walk_left, walk_right) = gait_support_weights(WALK_LOCOMOTION_PROFILE, 0.25);
        assert!(walk_left + walk_right > 0.0);
        assert_eq!(
            gait_support_weights(RUN_LOCOMOTION_PROFILE, 0.25),
            (0.0, 0.0)
        );
        assert_eq!(RUN_LOCOMOTION_PROFILE.flight_apex_metres, 0.09);
    }

    #[test]
    fn locomotion_style_uses_current_physical_speed() {
        let mut state = SkeletonState::default();
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation: Quat::IDENTITY,
                linear_velocity: Vec3::NEG_Z * 2.0,
                grounded: true,
                crouching: false,
                delta_seconds: 1.0 / LOCOMOTION_SAMPLE_HZ,
                tick: 1,
            },
        );

        assert_eq!(
            state.animation_speed(),
            WALK_LOCOMOTION_PROFILE.reference_speed
        );
        let evaluation = AnimationEvaluation::from_skeleton(&state);
        assert!(evaluation.base.iter().all(|sample| matches!(
            sample.pose,
            SemanticPose::WalkContact | SemanticPose::WalkPassing
        )));
        assert_eq!(
            evaluation
                .base
                .iter()
                .map(|sample| sample.weight)
                .sum::<f32>(),
            1.0
        );
    }

    #[test]
    fn projector_sequences_contacts_acceleration_and_one_landing_edge() {
        let mut state = SkeletonState {
            gait_phase: 0.49,
            locomotion_sample_tick: 1,
            local_velocity: Vec3::new(0.0, -4.0, -1.0),
            world_velocity: Vec3::new(0.0, -4.0, -1.0),
            grounded: false,
            ..default()
        };
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation: Quat::IDENTITY,
                linear_velocity: Vec3::NEG_Z * 2.0,
                grounded: true,
                crouching: false,
                delta_seconds: 0.1,
                tick: 2,
            },
        );
        assert_eq!(state.contact_sequence, 1);
        assert_eq!(state.contact_foot, LeadFoot::Right);
        assert_eq!(state.landing_sequence, 1);
        assert_eq!(state.landing_impact_speed, 4.0);
        assert!(state.world_acceleration.length() > 0.0);
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation: Quat::IDENTITY,
                linear_velocity: Vec3::NEG_Z * 2.0,
                grounded: true,
                crouching: false,
                delta_seconds: 0.1,
                tick: 3,
            },
        );
        assert_eq!(state.landing_sequence, 1);
    }

    #[test]
    fn turning_acceleration_is_differenced_in_one_world_frame() {
        let previous_velocity = Vec3::NEG_Z * 5.5;
        let orientation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let current_velocity = controller_yaw(orientation) * Vec3::NEG_Z * 5.5;
        let mut state = SkeletonState {
            locomotion_sample_tick: 4,
            local_velocity: Vec3::NEG_Z * 5.5,
            world_velocity: previous_velocity,
            ..default()
        };
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation,
                linear_velocity: current_velocity,
                grounded: true,
                crouching: false,
                delta_seconds: 1.0 / LOCOMOTION_SAMPLE_HZ,
                tick: 5,
            },
        );
        let expected =
            ((current_velocity - previous_velocity) * LOCOMOTION_SAMPLE_HZ).clamp_length_max(80.0);
        assert!(state.world_acceleration.abs_diff_eq(expected, 0.0001));
    }

    #[test]
    fn planar_projection_ignores_camera_pitch() {
        let yaw = 0.7;
        let orientation = Quat::from_euler(EulerRot::YXZ, yaw, 1.25, 0.0);
        let world_velocity = Quat::from_rotation_y(yaw) * Vec3::NEG_Z * 3.0;
        let mut state = SkeletonState::default();
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation,
                linear_velocity: world_velocity,
                grounded: true,
                crouching: false,
                delta_seconds: 1.0 / 64.0,
                tick: 1,
            },
        );
        assert!(state.local_velocity.abs_diff_eq(Vec3::NEG_Z * 3.0, 0.0001));
    }

    #[test]
    fn raised_guard_freezes_lead_and_all_directions_share_one_pulse_phase() {
        let input = |linear_velocity| SkeletonLocomotionInput {
            orientation: Quat::IDENTITY,
            linear_velocity,
            grounded: true,
            crouching: false,
            delta_seconds: 0.1,
            tick: 1,
        };
        let mut forward = SkeletonState {
            weapon_guard: WeaponGuardState::Raised,
            lead_foot: LeadFoot::Left,
            gait_phase: 0.25,
            ..default()
        };
        let mut retreat = forward.clone();
        project_skeleton_locomotion(&mut forward, input(Vec3::NEG_Z * 2.0));
        project_skeleton_locomotion(&mut retreat, input(Vec3::Z * 2.0));

        assert_eq!(forward.lead_foot, LeadFoot::Left);
        assert_eq!(retreat.lead_foot, LeadFoot::Left);
        assert!((forward.gait_phase - retreat.gait_phase).abs() < 0.0001);

        let mut lowered = SkeletonState {
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
            gait_phase: 0.49,
            ..default()
        };
        project_skeleton_locomotion(&mut lowered, input(Vec3::NEG_Z * 2.0));
        assert_eq!(lowered.lead_foot, LeadFoot::Right);
    }

    #[test]
    fn body_facing_is_bounded_and_uses_stable_half_turn() {
        let first = advance_body_facing(
            Quat::IDENTITY,
            Quat::IDENTITY,
            Vec3::NEG_Z,
            SkeletonAction::None,
            WeaponGuardState::Lowered,
            1.0 / 64.0,
        );
        let angle = Quat::IDENTITY.angle_between(first);
        assert!((angle - BODY_TURN_SPEED_RADIANS / 64.0).abs() < 0.0001);
        assert!(
            (first * Vec3::Z).x > 0.0,
            "exact reversal chooses positive yaw"
        );

        let completed = advance_body_facing(
            Quat::IDENTITY,
            Quat::IDENTITY,
            Vec3::NEG_Z,
            SkeletonAction::None,
            WeaponGuardState::Lowered,
            0.25,
        );
        assert!((Quat::IDENTITY.angle_between(completed) - std::f32::consts::PI).abs() < 0.0001);
    }

    #[test]
    fn guard_faces_look_while_locomotion_faces_world_velocity() {
        let look = Quat::from_rotation_y(0.8);
        let guard = advance_body_facing(
            Quat::IDENTITY,
            look,
            Vec3::X,
            SkeletonAction::Block,
            WeaponGuardState::Lowered,
            1.0,
        );
        assert!((guard * Vec3::Z).abs_diff_eq(look * Vec3::NEG_Z, 0.0001));
        let travel = advance_body_facing(
            Quat::IDENTITY,
            look,
            Vec3::X,
            SkeletonAction::None,
            WeaponGuardState::Lowered,
            1.0,
        );
        assert!((travel * Vec3::Z).abs_diff_eq(Vec3::X, 0.0001));
        let raised = advance_body_facing(
            Quat::IDENTITY,
            look,
            Vec3::X,
            SkeletonAction::None,
            WeaponGuardState::Raised,
            1.0,
        );
        assert!((raised * Vec3::Z).abs_diff_eq(look * Vec3::NEG_Z, 0.0001));
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
    fn gait_phase_spans_two_steps_at_run_speed() {
        let speed = 5.5;
        let cycle_seconds = RUN_LOCOMOTION_PROFILE.step_distance * 2.0 / speed;
        assert!(
            (gait_cycle_phase_delta(RUN_LOCOMOTION_PROFILE, speed, cycle_seconds) - 1.0).abs()
                < 0.0001
        );
    }

    #[test]
    fn raised_guard_locomotion_uses_static_lead_guard_for_procedural_legs() {
        let evaluate = |velocity| {
            AnimationEvaluation::from_skeleton(&SkeletonState {
                weapon_guard: WeaponGuardState::Raised,
                lead_foot: LeadFoot::Left,
                local_velocity: velocity,
                raised_locomotion: raised_intent(velocity),
                gait_phase: 0.25,
                ..default()
            })
        };
        let idle = evaluate(Vec3::ZERO);
        assert_eq!(idle.base[0].pose, SemanticPose::GuardLeadLeft);
        assert_eq!(idle.base[0].sampling, PoseSampling::Anchor);

        for velocity in [Vec3::NEG_Z, Vec3::Z, Vec3::NEG_X, Vec3::X] {
            let evaluation = evaluate(velocity);
            assert_eq!(evaluation.base.len(), 1);
            assert_eq!(evaluation.base[0].pose, SemanticPose::GuardLeadLeft);
            assert_eq!(evaluation.base[0].sampling, PoseSampling::Anchor);
        }
    }

    #[test]
    fn raised_guard_diagonal_keeps_static_guard_and_fixed_lead() {
        let evaluation = AnimationEvaluation::from_skeleton(&SkeletonState {
            weapon_guard: WeaponGuardState::Raised,
            lead_foot: LeadFoot::Right,
            local_velocity: Vec3::new(-3.0, 0.0, -1.0),
            raised_locomotion: raised_intent(Vec3::new(-3.0, 0.0, -1.0)),
            gait_phase: 0.75,
            ..default()
        });
        assert_eq!(evaluation.base.len(), 1);
        assert_eq!(evaluation.base[0].pose, SemanticPose::GuardLeadRight);
        assert_eq!(evaluation.base[0].sampling, PoseSampling::Anchor);
        assert_eq!(evaluation.base[0].weight, 1.0);
    }

    #[test]
    fn raised_guard_fk_stays_at_guard_through_both_procedural_steps() {
        for phase in [0.0, 0.5, 0.999] {
            let evaluation = AnimationEvaluation::from_skeleton(&SkeletonState {
                weapon_guard: WeaponGuardState::Raised,
                lead_foot: LeadFoot::Right,
                local_velocity: Vec3::NEG_Z,
                raised_locomotion: raised_intent(Vec3::NEG_Z),
                gait_phase: phase,
                ..default()
            });
            assert_eq!(evaluation.base[0].pose, SemanticPose::GuardLeadRight);
            assert_eq!(evaluation.base[0].sampling, PoseSampling::Anchor);
        }
    }

    #[test]
    fn entering_raised_guard_resets_to_static_guard_endpoint_once() {
        let mut state = SkeletonState {
            gait_phase: 0.63,
            lead_foot: LeadFoot::Right,
            ..default()
        };
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        assert_eq!(state.gait_phase, 0.0);
        assert_eq!(state.lead_foot, LeadFoot::Right);

        state.gait_phase = 0.25;
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        assert_eq!(state.gait_phase, 0.25);
        set_weapon_guard(&mut state, WeaponGuardState::Lowered);
        assert_eq!(state.gait_phase, 0.25);
    }

    #[test]
    fn raised_guard_release_finishes_only_the_in_flight_step() {
        let mut state = SkeletonState::default();
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        let input = |velocity, delta_seconds| SkeletonLocomotionInput {
            orientation: Quat::IDENTITY,
            linear_velocity: velocity,
            grounded: true,
            crouching: false,
            delta_seconds,
            tick: 1,
        };
        project_skeleton_locomotion(&mut state, input(Vec3::NEG_Z * 2.0, 0.095));
        assert!(state.raised_locomotion.active);
        assert!((state.gait_phase - 0.25).abs() < 0.001);

        project_skeleton_locomotion(&mut state, input(Vec3::ZERO, 0.08));
        assert!(state.raised_locomotion.active);
        assert_eq!(state.raised_locomotion.local_direction, Vec2::NEG_Y);
        project_skeleton_locomotion(&mut state, input(Vec3::ZERO, 0.02));
        assert!(!state.raised_locomotion.active);
        assert_eq!(state.gait_phase, 0.5);
    }

    #[test]
    fn raised_guard_direction_change_waits_only_for_foot_handoff() {
        let mut state = SkeletonState::default();
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        let input = |velocity, delta_seconds| SkeletonLocomotionInput {
            orientation: Quat::IDENTITY,
            linear_velocity: velocity,
            grounded: true,
            crouching: false,
            delta_seconds,
            tick: 1,
        };
        project_skeleton_locomotion(&mut state, input(Vec3::NEG_X * 2.0, 0.05));
        project_skeleton_locomotion(&mut state, input(Vec3::NEG_Z * 2.0, 0.05));
        assert_eq!(state.raised_locomotion.local_direction, Vec2::NEG_X);

        project_skeleton_locomotion(&mut state, input(Vec3::NEG_Z * 2.0, 0.15));
        assert_eq!(state.raised_locomotion.local_direction, Vec2::NEG_Y);
        assert!(state.gait_phase > 0.5);
        assert_eq!(state.lead_foot, LeadFoot::Left);
    }

    #[test]
    fn raised_guard_reversal_hands_support_off_immediately() {
        let mut state = SkeletonState::default();
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        let input = |velocity| SkeletonLocomotionInput {
            orientation: Quat::IDENTITY,
            linear_velocity: velocity,
            grounded: true,
            crouching: false,
            delta_seconds: 0.05,
            tick: 1,
        };
        project_skeleton_locomotion(&mut state, input(Vec3::NEG_X * 2.0));
        let sequence = state.raised_locomotion.step_sequence;
        let swing = state.raised_locomotion.swing_foot;
        project_skeleton_locomotion(&mut state, input(Vec3::X * 2.0));
        assert_eq!(state.raised_locomotion.local_direction, Vec2::X);
        assert_eq!(state.raised_locomotion.step_sequence, sequence + 1);
        assert_ne!(state.raised_locomotion.swing_foot, swing);
        assert!(state.gait_phase == 0.0 || state.gait_phase == 0.5);
    }

    #[test]
    fn raised_guard_cadence_adapts_during_first_acceleration_step() {
        let mut state = SkeletonState::default();
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        let input = |velocity| SkeletonLocomotionInput {
            orientation: Quat::IDENTITY,
            linear_velocity: velocity,
            grounded: true,
            crouching: false,
            delta_seconds: 0.05,
            tick: 1,
        };
        project_skeleton_locomotion(&mut state, input(Vec3::NEG_Z * 0.1));
        let slow_delta = state.gait_phase;
        project_skeleton_locomotion(&mut state, input(Vec3::NEG_Z * 2.0));
        let fast_delta = state.gait_phase - slow_delta;
        assert_eq!(state.raised_locomotion.speed, 2.0);
        assert!(fast_delta > slow_delta * 5.0);
    }

    #[test]
    fn raised_guard_sequence_counts_coalesced_handoffs_beyond_phase_parity() {
        let mut state = SkeletonState::default();
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation: Quat::IDENTITY,
                linear_velocity: Vec3::NEG_Z * 2.0,
                grounded: true,
                crouching: false,
                // At 2 m/s a full two-step cycle is 0.38 seconds.
                delta_seconds: guard_step_length(2.0) * 2.0 / 2.0,
                tick: 1,
            },
        );
        assert_eq!(state.raised_locomotion.step_sequence, 2);
        assert_eq!(state.raised_locomotion.swing_foot, LeadFoot::Right);
        assert!(state.gait_phase < 0.0001);
    }

    #[test]
    fn diagonal_guard_steps_begin_with_the_outward_lateral_foot() {
        for lead in [LeadFoot::Left, LeadFoot::Right] {
            assert_eq!(
                initial_guard_swing_foot(Vec2::new(-1.0, -1.0).normalize(), lead),
                LeadFoot::Left
            );
            assert_eq!(
                initial_guard_swing_foot(Vec2::new(1.0, 1.0).normalize(), lead),
                LeadFoot::Right
            );
        }
    }

    #[test]
    fn raised_guard_preserves_existing_crouch_and_airborne_postures() {
        let crouched = AnimationEvaluation::from_skeleton(&SkeletonState {
            weapon_guard: WeaponGuardState::Raised,
            posture: Posture::Crouched,
            local_velocity: Vec3::NEG_Z,
            ..default()
        });
        assert!(
            crouched
                .base
                .iter()
                .any(|sample| { sample.pose == SemanticPose::CrouchWalkContact })
        );
        let airborne = AnimationEvaluation::from_skeleton(&SkeletonState {
            weapon_guard: WeaponGuardState::Raised,
            posture: Posture::Airborne,
            local_velocity: Vec3::Y,
            ..default()
        });
        assert_eq!(airborne.base[0].pose, SemanticPose::JumpCenterLaunch);
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
        assert_eq!(dodge.action[0].pose, SemanticPose::DuckLeadLeftRight);
        assert_eq!(
            dodge.action[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::GuardLeadLeft,
                progress: 0.5,
            }
        );

        let right_lead_dodge = AnimationEvaluation::from_skeleton(&SkeletonState {
            lead_foot: LeadFoot::Right,
            action: SkeletonAction::Dodge,
            action_phase: 0.25,
            action_direction: Vec2::NEG_X,
            ..default()
        });
        assert_eq!(
            right_lead_dodge.action[0].pose,
            SemanticPose::GuardLeadRight
        );
        assert_eq!(
            right_lead_dodge.action[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::DuckLeadRightLeft,
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
