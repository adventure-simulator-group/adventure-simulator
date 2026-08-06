use super::*;

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
    MissingRoot(String),
    RootHasFallback(String),
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
        let root_pack = self
            .packs
            .get(root)
            .ok_or_else(|| PackValidationError::MissingRoot(root.to_owned()))?;
        if root_pack.fallback.is_some() {
            return Err(PackValidationError::RootHasFallback(root.to_owned()));
        }
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
