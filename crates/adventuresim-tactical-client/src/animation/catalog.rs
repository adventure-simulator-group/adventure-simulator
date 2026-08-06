use std::{collections::BTreeMap, str::FromStr};

#[cfg(any(test, feature = "animation-graph-editor"))]
use std::path::{Path, PathBuf};

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

#[cfg(any(test, feature = "animation-graph-editor"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditorRouteResolution {
    pub route: &'static str,
    pub requested: SemanticPose,
    pub resolved_pack: String,
    pub resolved_pose: SemanticPose,
    pub mirrored: bool,
    pub motion_path: PathBuf,
    pub frame: u16,
}

#[cfg(any(test, feature = "animation-graph-editor"))]
#[derive(Debug, Clone)]
pub(crate) struct EditorValidationReport {
    pub pack_count: usize,
    pub motion_count: usize,
    pub missing_motion_count: usize,
    pub warnings: Vec<String>,
    pub route_resolutions: Vec<EditorRouteResolution>,
}

/// Validate the code-owned semantic catalog against an editor asset root and
/// resolve the same deterministic ordinary and raised/attack samples used by
/// the capture viewer. Errors are returned together so the native tool can
/// print a complete actionable preflight instead of failing at the first file.
#[cfg(any(test, feature = "animation-graph-editor"))]
pub(crate) fn validate_editor_asset_root(
    asset_root: &Path,
) -> Result<EditorValidationReport, Vec<String>> {
    let catalog = AnimationPackCatalog::default();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut motion_count = 0;
    let mut missing_motion_count = 0;

    for (pack_id, pack) in &catalog.packs {
        motion_count += pack.motions.len();
        for (motion_id, motion) in &pack.motions {
            let full_path = asset_root.join(&motion.path);
            if !full_path.is_file() {
                missing_motion_count += 1;
                warnings.push(format!(
                    "pack {pack_id} motion {motion_id} is missing {}",
                    full_path.display()
                ));
            }
        }
        for (pose, anchor) in &pack.poses {
            match pack.motions.get(&anchor.motion) {
                Some(motion) if anchor.frame <= motion.last_frame => {}
                Some(motion) => errors.push(format!(
                    "pack {pack_id} pose {} frame {} exceeds {} frame {}",
                    pose.as_str(),
                    anchor.frame,
                    anchor.motion,
                    motion.last_frame
                )),
                None => errors.push(format!(
                    "pack {pack_id} pose {} references unknown motion {}",
                    pose.as_str(),
                    anchor.motion
                )),
            }
        }
    }

    let ordinary = SkeletonState::default()
        .with_local_velocity(Vec3::NEG_Z * 2.0)
        .with_world_velocity(Vec3::NEG_Z * 2.0);
    let mut raised_attack = SkeletonState::default().with_lead_foot(LeadFoot::Right);
    raised_attack.begin_attack(AttackSpec::default(), 10, 20);
    raised_attack.advance_action(20);

    let mut route_resolutions = Vec::new();
    for (route, state) in [
        ("ordinary_locomotion", ordinary),
        ("raised_guard_attack", raised_attack),
    ] {
        let evaluation = AnimationEvaluation::from_skeleton(&state);
        let samples = if evaluation.action.is_empty() {
            &evaluation.base
        } else {
            &evaluation.action
        };
        if samples.is_empty() {
            errors.push(format!("route {route} produced no semantic samples"));
            continue;
        }
        for sample in samples {
            match resolve_editor_pose(&catalog, HUMANOID_UNARMED_PACK, sample.pose, asset_root) {
                Some(resolution) => {
                    if !resolution.motion_path.is_file() {
                        errors.push(format!(
                            "required route {route} resolves {} to missing {}",
                            sample.pose.as_str(),
                            resolution.motion_path.display()
                        ));
                    }
                    route_resolutions.push(EditorRouteResolution {
                        route,
                        requested: sample.pose,
                        ..resolution
                    });
                }
                None => errors.push(format!(
                    "route {route} cannot resolve semantic pose {} from pack {HUMANOID_UNARMED_PACK}",
                    sample.pose.as_str()
                )),
            }
            if let PoseSampling::Span { end, progress } = sample.sampling
                && progress > f32::EPSILON
            {
                match resolve_editor_pose(&catalog, HUMANOID_UNARMED_PACK, end, asset_root) {
                    Some(resolution) => {
                        if !resolution.motion_path.is_file() {
                            errors.push(format!(
                                "required route {route} resolves span endpoint {} to missing {}",
                                end.as_str(),
                                resolution.motion_path.display()
                            ));
                        }
                        route_resolutions.push(EditorRouteResolution {
                            route,
                            requested: end,
                            ..resolution
                        });
                    }
                    None => errors.push(format!(
                        "route {route} cannot resolve span endpoint {} from pack {HUMANOID_UNARMED_PACK}",
                        end.as_str()
                    )),
                }
            }
        }
    }

    // The raised/right attack deliberately proves same-pack semantic mirroring
    // because the root pack authors the left contact and resolves its right
    // counterpart without introducing an independent authority path.
    if !route_resolutions
        .iter()
        .any(|resolution| resolution.route == "raised_guard_attack" && resolution.mirrored)
    {
        errors.push("raised_guard_attack did not exercise mirrored fallback resolution".into());
    }

    if errors.is_empty() {
        Ok(EditorValidationReport {
            pack_count: catalog.packs.len(),
            motion_count,
            missing_motion_count,
            warnings,
            route_resolutions,
        })
    } else {
        Err(errors)
    }
}

#[cfg(any(test, feature = "animation-graph-editor"))]
fn resolve_editor_pose(
    catalog: &AnimationPackCatalog,
    requested_pack: &str,
    requested_pose: SemanticPose,
    asset_root: &Path,
) -> Option<EditorRouteResolution> {
    let mut pack_id = requested_pack;
    loop {
        let pack = catalog.packs.get(pack_id)?;
        let resolved = pack
            .poses
            .get(&requested_pose)
            .map(|anchor| (requested_pose, anchor, false))
            .or_else(|| {
                let counterpart = requested_pose.mirrored_counterpart()?;
                pack.poses
                    .get(&counterpart)
                    .map(|anchor| (counterpart, anchor, true))
            });
        if let Some((resolved_pose, anchor, mirrored)) = resolved {
            let motion = pack.motions.get(&anchor.motion)?;
            return Some(EditorRouteResolution {
                route: "",
                requested: requested_pose,
                resolved_pack: pack_id.to_owned(),
                resolved_pose,
                mirrored,
                motion_path: asset_root.join(&motion.path),
                frame: anchor.frame,
            });
        }
        pack_id = pack.fallback.as_deref()?;
    }
}
