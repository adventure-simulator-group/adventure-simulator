use std::{collections::BTreeMap, str::FromStr};

use adventuresim_tactical_core::prelude::*;
use bevy::prelude::*;

use super::HUMANOID_UNARMED_PACK;

/// Explicit code-owned file/frame catalog. Semantic ownership never depends on
/// glTF animation names or scene contents.
#[derive(Resource, Debug)]
pub struct AnimationPackCatalog {
    pub(super) packs: BTreeMap<String, PackCatalog>,
}

impl Default for AnimationPackCatalog {
    fn default() -> Self {
        Self::biped_root().expect("built-in biped animation catalog must be valid")
    }
}

#[derive(Debug, Clone)]
pub(super) struct PackCatalog {
    pub(super) skeleton_family: String,
    pub(super) fallback: Option<String>,
    pub(super) motions: BTreeMap<String, MotionSource>,
    pub(super) poses: BTreeMap<SemanticPose, PoseAnchor>,
    pub(super) references: BTreeMap<String, Vec<ReferenceAnchor>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MotionSource {
    pub(super) path: String,
    pub(super) last_frame: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PoseAnchor {
    pub(super) motion: String,
    pub(super) frame: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReferenceAnchor {
    pub(super) pose: SemanticPose,
    pub(super) frame: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CatalogError {
    DuplicatePose(SemanticPose),
    UnknownMotion(String),
    InvalidPack(PackValidationError),
}

pub(super) struct PackBuilder {
    id: String,
    path_prefix: String,
    pack: PackCatalog,
}

impl PackBuilder {
    pub(super) fn new(
        id: &str,
        skeleton_family: &str,
        fallback: Option<&str>,
        path_prefix: &str,
    ) -> Self {
        Self {
            id: id.to_owned(),
            path_prefix: path_prefix.trim_end_matches('/').to_owned(),
            pack: PackCatalog {
                skeleton_family: skeleton_family.to_owned(),
                fallback: fallback.map(str::to_owned),
                motions: BTreeMap::new(),
                poses: BTreeMap::new(),
                references: BTreeMap::new(),
            },
        }
    }

    pub(super) fn motion(&mut self, id: &str, last_frame: u16) {
        self.pack.motions.insert(
            id.to_owned(),
            MotionSource {
                path: format!("{}/{id}.glb", self.path_prefix),
                last_frame,
            },
        );
    }

    pub(super) fn pose(
        &mut self,
        motion: &str,
        frame: u16,
        pose: SemanticPose,
    ) -> Result<(), CatalogError> {
        if !self.pack.motions.contains_key(motion) {
            return Err(CatalogError::UnknownMotion(motion.to_owned()));
        }
        if self
            .pack
            .poses
            .insert(
                pose,
                PoseAnchor {
                    motion: motion.to_owned(),
                    frame,
                },
            )
            .is_some()
        {
            return Err(CatalogError::DuplicatePose(pose));
        }
        Ok(())
    }

    pub(super) fn reference(
        &mut self,
        motion: &str,
        frame: u16,
        pose: &str,
    ) -> Result<(), CatalogError> {
        if !self.pack.motions.contains_key(motion) {
            return Err(CatalogError::UnknownMotion(motion.to_owned()));
        }
        let pose = SemanticPose::from_str(pose)
            .map_err(|()| CatalogError::UnknownMotion(pose.to_owned()))?;
        self.pack
            .references
            .entry(motion.to_owned())
            .or_default()
            .push(ReferenceAnchor { pose, frame });
        Ok(())
    }

    pub(super) fn finish(self) -> (String, PackCatalog) {
        (self.id, self.pack)
    }
}

impl AnimationPackCatalog {
    pub(super) fn biped_root() -> Result<Self, CatalogError> {
        let mut builder = PackBuilder::new(
            HUMANOID_UNARMED_PACK,
            "humanoid",
            None,
            "animations/biped/unarmed",
        );
        for pose in [
            "idle_relaxed",
            "crouch_idle",
            "guard_lead_left",
            "guard_lead_right",
            "prone_idle",
            "supine_idle",
        ] {
            builder.motion(pose, 0);
            builder.pose(
                pose,
                0,
                SemanticPose::from_str(pose).expect("typed catalog pose"),
            )?;
        }
        for motion in [
            "guard_walk_lead_left",
            "guard_walk_lead_right",
            "guard_strafe_lead_left_left",
            "guard_strafe_lead_left_right",
            "guard_strafe_lead_right_left",
            "guard_strafe_lead_right_right",
        ] {
            builder.motion(motion, 0);
            builder.pose(
                motion,
                0,
                SemanticPose::from_str(motion).expect("typed catalog pose"),
            )?;
        }
        for (motion, last_frame, anchors) in [
            ("walk", 32, [(0, "walk_contact"), (8, "walk_passing")]),
            ("run", 20, [(0, "run_contact"), (5, "run_flight")]),
            (
                "prone_crawl",
                32,
                [(0, "prone_crawl_contact"), (8, "prone_crawl_passing")],
            ),
            (
                "supine_scamper",
                32,
                [(0, "supine_scamper_contact"), (8, "supine_scamper_passing")],
            ),
        ] {
            builder.motion(motion, last_frame);
            for (frame, pose) in anchors {
                builder.pose(
                    motion,
                    frame,
                    SemanticPose::from_str(pose).expect("typed catalog pose"),
                )?;
            }
        }
        // Pre-reflected copies of the sparse gait anchors are selected only
        // as mirrored blend endpoints; they own no independent semantics.
        builder.motion("walk_mirrored", 32);
        builder.motion("run_mirrored", 20);
        for (motion, pose) in [
            ("duck_lead_left_backward", "duck_lead_left_backward"),
            ("duck_lead_left_left", "duck_lead_left_left"),
            ("duck_lead_left_right", "duck_lead_left_right"),
        ] {
            builder.motion(motion, 0);
            builder.pose(
                motion,
                0,
                SemanticPose::from_str(pose).expect("typed catalog pose"),
            )?;
        }
        for (motion, pose) in [
            ("airborne_center", SemanticPose::AirborneCenter),
            ("airborne_travel", SemanticPose::AirborneTravel),
        ] {
            builder.motion(motion, 0);
            builder.pose(motion, 0, pose)?;
        }
        for family in ["thrust", "slash"] {
            let contact_motion = format!("attack_{family}_lead_left_contact");
            builder.motion(&contact_motion, 0);
            let pose = SemanticPose::from_str(&contact_motion).expect("typed attack pose");
            builder.pose(&contact_motion, 0, pose)?;
        }
        for motion in [
            "block_cut_left_lead_left",
            "block_cut_left_lead_right",
            "block_cut_right_lead_left",
            "block_cut_right_lead_right",
            "block_thrust_lead_left",
            "block_thrust_lead_right",
        ] {
            builder.motion(motion, 14);
            builder.pose(
                motion,
                6,
                SemanticPose::from_str(motion).expect("typed block pose"),
            )?;
            let lead = if motion.ends_with("lead_left") {
                "guard_lead_left"
            } else {
                "guard_lead_right"
            };
            builder.reference(motion, 0, lead)?;
            builder.reference(motion, 14, lead)?;
        }
        for (motion, last_frame, frame, pose) in [
            (
                "upright_prone_transition",
                24,
                12,
                "upright_prone_transition",
            ),
            ("dive", 18, 10, "dive_impact"),
        ] {
            builder.motion(motion, last_frame);
            builder.pose(
                motion,
                frame,
                SemanticPose::from_str(pose).expect("typed catalog pose"),
            )?;
            match motion {
                "upright_prone_transition" => {
                    builder.reference(motion, 0, "crouch_idle")?;
                    builder.reference(motion, 24, "prone_idle")?;
                }
                "dive" => {
                    builder.reference(motion, 0, "airborne_travel")?;
                    builder.reference(motion, 18, "prone_idle")?;
                }
                _ => {}
            }
        }
        let (id, pack) = builder.finish();
        let mut library = AnimationPackLibrary::default();
        library
            .insert(AnimationPack {
                id: id.clone(),
                skeleton_family: pack.skeleton_family.clone(),
                fallback: pack.fallback.clone(),
                clips: pack.poses.keys().copied().collect(),
            })
            .map_err(CatalogError::InvalidPack)?;
        library
            .validate_complete(&id)
            .map_err(CatalogError::InvalidPack)?;
        Ok(Self {
            packs: BTreeMap::from([(id, pack)]),
        })
    }
}
