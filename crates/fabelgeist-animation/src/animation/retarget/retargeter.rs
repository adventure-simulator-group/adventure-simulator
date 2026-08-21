//! The generic retargeting algorithm.
//!
//! Nothing here knows what a Mixamo rig is, or an FBX, or MHR. It works from a
//! resolved profile — joint indices and policy — and from the two skeletons'
//! rest poses. That is deliberately the whole input: everything a particular
//! source rig needs to say about itself has to be sayable in a profile, or the
//! system is not actually generic.
//!
//! # Rest-relative transfer
//!
//! Copying local rotations between rigs only works when the rigs agree on bone
//! orientation, which they never do. Instead each frame is read as motion
//! *away from the source's rest pose*, in model space:
//!
//! ```text
//! delta[i]  = model_source[i] * rest_source[i]^-1
//! model[j]  = delta[i] * rest_target[j]
//! local[j]  = model[parent(j)]^-1 * model[j]
//! ```
//!
//! The rest-pose difference between the rigs is therefore never computed as an
//! explicit correction — it falls out of conjugating by each rig's own rest.
//! A joint with no mapping keeps its rest local transform, so missing
//! intermediate bones and extra target bones both behave: the chain still
//! accumulates through them, and they stay in a valid pose.

use crate::animation::{
    Animation, Curve, JointTrack, JointTransform, LocalPose, RootMotion, model_pose, rest_pose,
};
use crate::skeleton::Skeleton;
use anyhow::Result;
use fabelgeist_math::vector::{Vec3, Vec4};

use super::profile::{
    Axis, ReferencePose, RetargetProfile, RetargetSettings, RigProfile, ScaleMeasure, ScalePolicy,
    TranslationPolicy,
};
use super::resolve::{ResolvedProfile, ResolvedRig};
use super::semantic::{HumanoidChain, HumanoidJoint};

/// Where one target joint takes its motion from.
#[derive(Clone, Debug, PartialEq)]
enum Motion {
    /// One source joint, the usual case.
    Joint(usize),
    /// A position along a source chain, for chains of unequal length. The
    /// motion at that position is interpolated between the chain's joints.
    Chain { chain: usize, position: f32 },
}

#[derive(Clone, Debug, PartialEq)]
struct Mapping {
    motion: Motion,
    /// The role this joint plays, when it has one.
    role: Option<HumanoidJoint>,
    /// Effective rest rotation of the source reference, model space.
    source_rest: Vec4,
    /// Effective rest rotation of the target, model space.
    target_rest: Vec4,
    translation: TranslationPolicy,
}

/// A prepared transfer between two specific skeletons.
///
/// Construction does all the resolution and measurement once; retargeting a
/// clip is then a straight pass over its keyframes.
pub struct Retargeter<'a> {
    source_skeleton: &'a Skeleton,
    target_skeleton: &'a Skeleton,
    resolved: ResolvedProfile,
    source_rest_model: LocalPose,
    target_rest: LocalPose,
    target_rest_model: LocalPose,
    /// Model-space poses of each rig in its reference posture, which is what
    /// motion is measured against. Equal to the rest poses unless a profile
    /// says its bind pose is not a usable reference.
    source_reference_model: LocalPose,
    target_reference_model: LocalPose,
    /// Source chains, in the order `Mapping::Chain` indexes them.
    source_chains: Vec<Vec<usize>>,
    /// One entry per target joint.
    mappings: Vec<Option<Mapping>>,
    /// Rotation from source-rig space to target-rig space.
    basis: Vec4,
    /// Target units per source unit.
    scale: f32,
    /// The source joint carrying locomotion.
    root: Option<usize>,
}

/// Retargets a clip from one skeleton to another.
///
/// This is the whole public entry point: two skeletons, a clip, and a profile
/// describing how the rigs correspond.
pub fn retarget(
    source_skeleton: &Skeleton,
    source_clip: &Animation,
    target_skeleton: &Skeleton,
    profile: &RetargetProfile,
) -> Result<Animation> {
    Retargeter::new(source_skeleton, target_skeleton, profile)
        .map(|retargeter| retargeter.clip(source_clip))
}

impl<'a> Retargeter<'a> {
    /// Prepares a transfer, taking each rig's rest pose from its skeleton.
    pub fn new(
        source_skeleton: &'a Skeleton,
        target_skeleton: &'a Skeleton,
        profile: &RetargetProfile,
    ) -> Result<Self> {
        Self::with_rest_poses(source_skeleton, None, target_skeleton, None, profile)
    }

    /// As [`Retargeter::new`], with rest poses supplied directly.
    ///
    /// A skeleton stores rest rotations as Euler angles, which is exact enough
    /// for rigs authored that way but not for one whose rest pose is natively
    /// a quaternion. Such a rig can hand its own rest pose over rather than
    /// letting it round-trip.
    pub fn with_rest_poses(
        source_skeleton: &'a Skeleton,
        source_rest: Option<&[JointTransform]>,
        target_skeleton: &'a Skeleton,
        target_rest: Option<&[JointTransform]>,
        profile: &RetargetProfile,
    ) -> Result<Self> {
        let resolved = profile.resolve(source_skeleton, target_skeleton)?;

        let source_rest = match source_rest {
            Some(rest) if rest.len() == source_skeleton.joints.len() => rest.to_vec(),
            _ => rest_pose(source_skeleton),
        };
        let target_rest = match target_rest {
            Some(rest) if rest.len() == target_skeleton.joints.len() => rest.to_vec(),
            _ => rest_pose(target_skeleton),
        };
        // A skeleton's armature transform is what relates its joint space to
        // the scene, and rigs disagree: a Blender-exported glTF keeps its
        // joints in the source package's Z-up centimetres and puts the
        // conversion on the armature. Transferring a rotation between two such
        // frames without accounting for it turns the whole body 90 degrees, so
        // the armature rotation is the default basis and a profile only has to
        // say anything when the asset itself is wrong.
        let source_basis = profile
            .source
            .basis
            .unwrap_or_else(|| armature_rotation(source_skeleton));
        let target_basis = profile
            .target
            .basis
            .unwrap_or_else(|| armature_rotation(target_skeleton));
        let basis = target_basis.conjugate().mul_quat(source_basis).normalize();

        // With the source brought into the target's frame up front, everything
        // downstream — deltas, translations, the up axis root motion splits on
        // — is already speaking one language.
        let source_rest_model = rebase(model_pose(source_skeleton, &source_rest), basis);
        let target_rest_model = model_pose(target_skeleton, &target_rest);

        // Motion is transferred relative to a reference posture. The bind pose
        // only serves when both rigs bind alike, which Mixamo and MHR do not.
        let source_reference_model = rebase(
            model_pose(
                source_skeleton,
                &reference_pose(
                    source_skeleton,
                    &resolved.source,
                    &profile.source,
                    &source_rest,
                    &profile.source.reference,
                    source_basis,
                ),
            ),
            basis,
        );
        let target_reference_model = model_pose(
            target_skeleton,
            &reference_pose(
                target_skeleton,
                &resolved.target,
                &profile.target,
                &target_rest,
                &profile.target.reference,
                target_basis,
            ),
        );

        let scale = size_ratio(
            &resolved,
            &source_rest_model,
            &target_rest_model,
            profile.settings.scale,
            profile.settings.up,
        );

        let mut retargeter = Self {
            source_skeleton,
            target_skeleton,
            root: resolved
                .source
                .root
                .or_else(|| resolved.source.joint(HumanoidJoint::Pelvis)),
            resolved,
            source_rest_model,
            target_rest,
            target_rest_model,
            source_reference_model,
            target_reference_model,
            source_chains: Vec::new(),
            mappings: vec![None; target_skeleton.joints.len()],
            basis,
            scale,
        };
        retargeter.build_mappings(profile);
        Ok(retargeter)
    }

    pub fn resolved(&self) -> &ResolvedProfile {
        &self.resolved
    }

    pub fn settings(&self) -> &RetargetSettings {
        &self.resolved.settings
    }

    /// Target units per source unit, as measured or configured.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The mapping as text, for inspecting a rig that misbehaves.
    pub fn report(&self) -> String {
        let mut report = self
            .resolved
            .report(self.source_skeleton, self.target_skeleton);
        report.push_str(&format!(
            "\nscale: {:.4} target units per source unit\n",
            self.scale
        ));
        let effectors = self.resolved.target.end_effectors();
        if !effectors.is_empty() {
            let names: Vec<String> = effectors
                .iter()
                .map(|(role, index)| format!("{role}={}", self.target_skeleton.joints[*index].name))
                .collect();
            report.push_str(&format!("end effectors: {}\n", names.join(", ")));
        }
        report
    }

    /// Decides, per target joint, where its motion comes from.
    ///
    /// Roles map one to one. Chains take over only where a rig declared its
    /// own chain joints *and* the two chains differ in length — that is the
    /// case the vocabulary cannot express, and the only case where guessing a
    /// distribution beats a direct correspondence.
    fn build_mappings(&mut self, profile: &RetargetProfile) {
        for role in self.resolved.shared_roles() {
            let (Some(source), Some(target)) = (
                self.resolved.source.joint(role),
                self.resolved.target.joint(role),
            ) else {
                continue;
            };
            let source_correction = profile
                .source
                .binding(role)
                .and_then(|binding| binding.correction);
            let target_correction = profile
                .target
                .binding(role)
                .and_then(|binding| binding.correction);
            let translation = self.translation_policy_for(profile, Some(role), target);
            self.mappings[target] = Some(Mapping {
                motion: Motion::Joint(source),
                role: Some(role),
                source_rest: rest_rotation(&self.source_reference_model, source, source_correction),
                target_rest: rest_rotation(&self.target_reference_model, target, target_correction),
                translation,
            });
        }

        for chain in HumanoidChain::ALL {
            let (Some(source_chain), Some(target_chain)) = (
                self.resolved.source.chains.get(chain),
                self.resolved.target.chains.get(chain),
            ) else {
                continue;
            };
            let declared = profile.source.chains.contains_key(chain)
                || profile.target.chains.contains_key(chain);
            if !declared || source_chain.len() == target_chain.len() || source_chain.is_empty() {
                continue;
            }

            let source_chain = source_chain.clone();
            let target_chain = target_chain.clone();
            let chain_index = self.source_chains.len();
            self.source_chains.push(source_chain.clone());

            let last = target_chain.len().saturating_sub(1);
            for (link, target) in target_chain.iter().enumerate() {
                let position = if last == 0 {
                    1.0
                } else {
                    link as f32 / last as f32
                };
                let role = self
                    .resolved
                    .target
                    .joints
                    .iter()
                    .find_map(|(role, index)| (index == target).then_some(*role));
                let correction = role
                    .and_then(|role| profile.target.binding(role))
                    .and_then(|binding| binding.correction);
                self.mappings[*target] = Some(Mapping {
                    motion: Motion::Chain {
                        chain: chain_index,
                        position,
                    },
                    role,
                    source_rest: chain_rotation(
                        &self.source_reference_model,
                        &source_chain,
                        position,
                    ),
                    target_rest: rest_rotation(&self.target_reference_model, *target, correction),
                    translation: self.translation_policy_for(profile, role, *target),
                });
            }
        }
    }

    /// Settings override bindings, which override the profile-wide default.
    fn translation_policy_for(
        &self,
        profile: &RetargetProfile,
        role: Option<HumanoidJoint>,
        target: usize,
    ) -> TranslationPolicy {
        let policy = role
            .and_then(|role| self.resolved.settings.joint_translation.get(&role).copied())
            .or_else(|| {
                role.and_then(|role| {
                    profile
                        .target
                        .binding(role)
                        .and_then(|binding| binding.translation)
                })
            })
            .or_else(|| {
                role.and_then(|role| {
                    profile
                        .source
                        .binding(role)
                        .and_then(|binding| binding.translation)
                })
            })
            .unwrap_or(self.resolved.settings.translation);

        // The class policies resolve to a concrete answer per joint.
        match policy {
            TranslationPolicy::RootOnly => {
                let root = self
                    .resolved
                    .target
                    .root
                    .or_else(|| self.resolved.target.joint(HumanoidJoint::Pelvis));
                if root == Some(target) {
                    TranslationPolicy::Scaled
                } else {
                    TranslationPolicy::Ignore
                }
            }
            TranslationPolicy::PelvisOnly => {
                if role == Some(HumanoidJoint::Pelvis) {
                    TranslationPolicy::Scaled
                } else {
                    TranslationPolicy::Ignore
                }
            }
            policy => policy,
        }
    }

    /// Retargets one pose. `source_locals` is the source skeleton's local
    /// transforms in joint order, as [`Animation::sample`] produces them.
    pub fn pose(&self, source_locals: &[JointTransform]) -> LocalPose {
        self.pose_with_root(source_locals).0
    }

    /// As [`Retargeter::pose`], also returning the locomotion that was taken
    /// out of the pose, already in target units.
    pub fn pose_with_root(&self, source_locals: &[JointTransform]) -> (LocalPose, JointTransform) {
        let mut source_model = rebase(model_pose(self.source_skeleton, source_locals), self.basis);
        let locomotion = self.extract_locomotion(&mut source_model);

        let mut target_model: LocalPose =
            vec![JointTransform::identity(); self.target_skeleton.joints.len()];
        let mut target_locals = self.target_rest.clone();

        for target in 0..self.target_skeleton.joints.len() {
            let parent = self.target_skeleton.joints[target]
                .parent_index
                .filter(|parent| *parent < target);
            let parent_model = parent
                .map(|parent| target_model[parent])
                .unwrap_or_else(JointTransform::identity);

            let Some(mapping) = &self.mappings[target] else {
                // Unmapped joints ride along on their rest transform, which is
                // what keeps extra target bones valid.
                target_model[target] = parent_model.compose(self.target_rest[target]);
                continue;
            };

            let delta = self.delta(&source_model, mapping);
            let rotation = delta.mul_quat(mapping.target_rest).normalize();

            let inverse_parent = parent_model.inverse();
            let (local_translation, model_translation) =
                match self.model_translation(&source_model, mapping, target) {
                    // Translated joints are placed in model space, then read
                    // back into the parent's frame.
                    Some(model) => (inverse_parent.transform_point(model), model),
                    // Untranslated joints keep the target's own proportions
                    // and simply follow whatever the parent is doing.
                    None => {
                        let local = self.target_rest[target].translation;
                        (local, parent_model.transform_point(local))
                    }
                };

            target_locals[target] = JointTransform {
                translation: local_translation,
                rotation: inverse_parent.rotation.mul_quat(rotation).normalize(),
                scale: self.target_rest[target].scale,
            };
            target_model[target] = JointTransform {
                translation: model_translation,
                rotation,
                scale: parent_model.scale * self.target_rest[target].scale,
            };
        }

        (target_locals, locomotion)
    }

    /// The source's model-space motion away from its rest pose, expressed in
    /// target-rig space.
    fn delta(&self, source_model: &[JointTransform], mapping: &Mapping) -> Vec4 {
        let rotation = match &mapping.motion {
            Motion::Joint(source) => source_model[*source].rotation,
            Motion::Chain { chain, position } => {
                chain_rotation(source_model, &self.source_chains[*chain], *position)
            }
        };
        // Both rigs are already in the same frame, so this is a plain
        // difference -- then rotated onto the target's own rest bone, which is
        // what lets a T-posed capture drive an A-posed rig.
        // Both rigs are already in the same frame, so this is a plain
        // difference rather than a change of basis.
        rotation
            .mul_quat(mapping.source_rest.conjugate())
            .normalize()
    }

    /// The target joint's desired model-space position, or `None` when its
    /// policy leaves translation to the target rig's own proportions.
    ///
    /// The source's displacement away from its rest pose is what transfers,
    /// not its absolute position: that way the target keeps its own limb
    /// lengths and only the motion crosses over.
    fn model_translation(
        &self,
        source_model: &[JointTransform],
        mapping: &Mapping,
        target: usize,
    ) -> Option<Vec3> {
        let scale = match mapping.translation {
            TranslationPolicy::Copy => 1.0,
            TranslationPolicy::Scaled => self.scale,
            // The class policies were resolved to one of the above, or to
            // `Ignore`, when the mapping was built.
            _ => return None,
        };
        let source = match &mapping.motion {
            Motion::Joint(source) => *source,
            Motion::Chain { chain, position } => {
                nearest_chain_joint(&self.source_chains[*chain], *position)
            }
        };
        let delta = source_model[source].translation - self.source_rest_model[source].translation;
        Some(self.target_rest_model[target].translation + delta * scale)
    }

    /// Splits locomotion out of the source's model pose, returning it in
    /// target units. The pose that remains is in place.
    fn extract_locomotion(&self, source_model: &mut [JointTransform]) -> JointTransform {
        let Some(channels) = self.resolved.settings.root_motion.channels() else {
            return JointTransform::identity();
        };
        let Some(root) = self.root else {
            return JointTransform::identity();
        };

        let up = self.resolved.settings.up.vector();
        let rest = self.source_rest_model[root];
        let current = source_model[root];

        let displacement = current.translation - rest.translation;
        let vertical = up * displacement.dot(up);
        let mut extracted = Vec3::new(0.0, 0.0, 0.0);
        if channels.horizontal {
            extracted = extracted + (displacement - vertical);
        }
        if channels.vertical {
            extracted = extracted + vertical;
        }
        let yaw = if channels.yaw {
            current
                .rotation
                .mul_quat(rest.rotation.conjugate())
                .twist_about(up)
        } else {
            Vec4::quat_identity()
        };

        // Undo the extracted motion about the rest root position, so the
        // remaining pose sits where it started and faces where it started.
        let pivot = rest.translation;
        let removal = JointTransform {
            translation: pivot + extracted - yaw.rotate_vec3(pivot),
            rotation: yaw,
            scale: Vec3::ones(),
        }
        .inverse();
        for transform in source_model.iter_mut() {
            *transform = removal.compose(*transform);
        }

        JointTransform {
            translation: extracted * self.scale,
            rotation: yaw,
            scale: Vec3::ones(),
        }
    }

    /// Retargets a whole clip, re-keying at the source's own key times so the
    /// output's timing matches the input's frame for frame.
    pub fn clip(&self, source: &Animation) -> Animation {
        let times = source.key_times();
        let binding = source.bind(self.source_skeleton);

        let joints = self.target_skeleton.joints.len();
        let mut rotations: Vec<Vec<Vec4>> = vec![Vec::with_capacity(times.len()); joints];
        let mut translations: Vec<Vec<Vec3>> = vec![Vec::with_capacity(times.len()); joints];
        let mut root_translations = Vec::with_capacity(times.len());
        let mut root_rotations = Vec::with_capacity(times.len());

        for time in &times {
            let source_locals = source.sample(&binding, *time);
            let (locals, locomotion) = self.pose_with_root(&source_locals);
            for (index, local) in locals.iter().enumerate() {
                rotations[index].push(local.rotation);
                translations[index].push(local.translation);
            }
            root_translations.push(locomotion.translation);
            root_rotations.push(locomotion.rotation);
        }

        // Locomotion is measured against the rig's rest pose, but a clip is
        // played from wherever the character already stands, so the track is
        // re-based onto its own first frame. Without this a source authored
        // away from its bind pose hands out a constant offset that reads as a
        // teleport, and an in-place clip never looks in-place.
        if let (Some(origin), Some(turn)) = (
            root_translations.first().copied(),
            root_rotations.first().copied(),
        ) {
            let unturn = turn.conjugate();
            for translation in &mut root_translations {
                *translation = unturn.rotate_vec3(*translation - origin);
            }
            for rotation in &mut root_rotations {
                *rotation = unturn.mul_quat(*rotation).normalize();
            }
        }

        let mut clip = Animation {
            name: source.name.clone(),
            duration: source.duration.max(times.last().copied().unwrap_or(0.0)),
            tracks: Vec::new(),
            root_motion: None,
        };

        for (index, mapping) in self.mappings.iter().enumerate() {
            let Some(mapping) = mapping else { continue };
            let mut track = JointTrack::new(self.target_skeleton.joints[index].name.clone());
            track.rotation = Some(Curve::new(times.clone(), rotations[index].clone()));
            if mapping.translation != TranslationPolicy::Ignore {
                track.translation = Some(Curve::new(times.clone(), translations[index].clone()));
            }
            clip.tracks.push(track);
        }

        if self.resolved.settings.root_motion.keeps_track()
            && moves(&root_translations, &root_rotations)
        {
            clip.root_motion = Some(RootMotion {
                translation: Some(Curve::new(times.clone(), root_translations)),
                rotation: Some(Curve::new(times, root_rotations)),
            });
        }

        clip
    }
}

/// Whether a locomotion track carries anything, so in-place clips do not end
/// up with a root motion track full of zeroes.
fn moves(translations: &[Vec3], rotations: &[Vec4]) -> bool {
    translations
        .iter()
        .any(|translation| translation.length() > 1.0e-4)
        || rotations
            .iter()
            .any(|rotation| rotation.w.abs() < 1.0 - 1.0e-6)
}

/// A skeleton's own space as a rotation, taken from its armature transform.
fn armature_rotation(skeleton: &Skeleton) -> Vec4 {
    JointTransform::from_transform(&skeleton.transform).rotation
}

/// Turns a model-space pose into another frame, rotating about the origin.
fn rebase(pose: LocalPose, basis: Vec4) -> LocalPose {
    if basis.w.abs() > 1.0 - 1.0e-9 {
        return pose;
    }
    pose.into_iter()
        .map(|transform| JointTransform {
            translation: basis.rotate_vec3(transform.translation),
            rotation: basis.mul_quat(transform.rotation).normalize(),
            scale: transform.scale,
        })
        .collect()
}

/// A rig's local pose in the posture its motion is measured against.
///
/// `TPose` straightens the rig from its own geometry: each bone that the
/// vocabulary gives a canonical direction is turned onto it, parents before
/// children so the correction accumulates down a limb rather than fighting it.
/// Bones with no canonical direction — feet, fingers — keep what the rig
/// authored. Then any hinge the profile declared is aimed, which is the half
/// of the posture bone directions cannot express.
/// `basis` is the rotation taking this rig's joint space to engine space. The
/// canonical directions are engine-space, the pose being straightened is not —
/// a Blender-exported glTF keeps its joints Z-up and puts the conversion on the
/// armature — so the directions are brought into the rig's own frame rather
/// than the whole pose into engine space.
fn reference_pose(
    skeleton: &Skeleton,
    rig: &ResolvedRig,
    profile: &RigProfile,
    bind: &LocalPose,
    reference: &ReferencePose,
    basis: Vec4,
) -> LocalPose {
    let mut local = bind.to_vec();
    let engine = basis.conjugate();
    match reference {
        ReferencePose::Bind => local,
        ReferencePose::Pose(rotations) => {
            for (role, rotation) in rotations {
                if let Some(joint) = rig.joint(*role) {
                    local[joint].rotation = local[joint].rotation.mul_quat(*rotation).normalize();
                }
            }
            local
        }
        ReferencePose::TPose => {
            for role in HumanoidJoint::ALL {
                let (Some(direction), Some(joint)) = (role.t_pose_direction(), rig.joint(*role))
                else {
                    continue;
                };
                let direction = engine.rotate_vec3(direction);
                let Some(tip) = role.child().and_then(|child| rig.joint(child)) else {
                    continue;
                };

                // Recomputed each step: turning the upper arm moves the forearm.
                let model = model_pose(skeleton, &local);
                let bone = model[tip].translation - model[joint].translation;
                if bone.length() < 1.0e-6 {
                    continue;
                }
                let turn = shortest_arc(bone.normalize(), direction);
                turn_joint(skeleton, &mut local, &model, joint, turn);
            }
            aim_hinges(skeleton, rig, profile, &mut local, engine);
            local
        }
    }
}

/// Rolls each declared hinge onto the direction a T-pose says it faces.
///
/// Straightening a limb pins where its bones point and nothing else: an arm
/// out along X can have its elbow set to bend forwards, upwards, or anywhere
/// between, and all of them are equally straight. That leftover roll is what
/// aims the hinge, so two rigs straightened the same way can still disagree
/// about which way an elbow bends — and a rig whose elbow is a single channel
/// cannot absorb the difference, it just loses the motion.
///
/// The roll is applied to the bone *above* the hinge. In a straightened limb
/// the two bones are collinear, so turning the parent about its own direction
/// aims the hinge while leaving both bones exactly where the direction pass
/// put them.
fn aim_hinges(
    skeleton: &Skeleton,
    rig: &ResolvedRig,
    profile: &RigProfile,
    local: &mut LocalPose,
    engine: Vec4,
) {
    for role in HumanoidJoint::ALL {
        let (Some(hinge), Some(wanted), Some(joint)) = (
            profile.binding(*role).and_then(|binding| binding.hinge),
            role.t_pose_hinge().map(|wanted| engine.rotate_vec3(wanted)),
            rig.joint(*role),
        ) else {
            continue;
        };
        // The roll lives on the parent bone, and it turns about that bone's
        // own direction, which the pass above has just established.
        let Some(parent_role) = role.parent() else {
            continue;
        };
        let (Some(parent), Some(axis)) = (
            rig.joint(parent_role),
            parent_role
                .t_pose_direction()
                .map(|axis| engine.rotate_vec3(axis)),
        ) else {
            continue;
        };

        let model = model_pose(skeleton, local);
        let facing = model[joint].rotation.rotate_vec3(hinge);
        let Some(turn) = roll_onto(facing, wanted, axis) else {
            continue;
        };
        turn_joint(skeleton, local, &model, parent, turn);
    }
}

/// The rotation about `axis` that brings `from` as close to `to` as a roll
/// can, or `None` when either is too near the axis to have a direction in the
/// plane.
fn roll_onto(from: Vec3, to: Vec3, axis: Vec3) -> Option<Vec4> {
    let flatten = |vector: Vec3| {
        let flat = vector - axis * vector.dot(axis);
        (flat.length() > 1.0e-4).then(|| flat.normalize())
    };
    let (from, to) = (flatten(from)?, flatten(to)?);
    let angle = from.cross(to).dot(axis).atan2(from.dot(to));
    (angle.abs() > 1.0e-6).then(|| Vec4::from_axis_angle(axis, angle))
}

/// Applies a model-space rotation to one joint, written back as the local
/// rotation that produces it.
fn turn_joint(
    skeleton: &Skeleton,
    local: &mut LocalPose,
    model: &[JointTransform],
    joint: usize,
    turn: Vec4,
) {
    let parent = skeleton.joints[joint]
        .parent_index
        .filter(|parent| *parent < joint)
        .map(|parent| model[parent].rotation)
        .unwrap_or_else(Vec4::quat_identity);
    local[joint].rotation = parent
        .conjugate()
        .mul_quat(turn)
        .mul_quat(model[joint].rotation)
        .normalize();
}

/// The smallest rotation taking one unit vector onto another.
fn shortest_arc(from: Vec3, to: Vec3) -> Vec4 {
    let dot = from.dot(to).clamp(-1.0, 1.0);
    if dot > 1.0 - 1.0e-9 {
        return Vec4::quat_identity();
    }
    if dot < -1.0 + 1.0e-6 {
        // Opposed: any perpendicular axis turns one onto the other.
        let aside = if from.x.abs() < 0.9 {
            Vec3::new(1.0, 0.0, 0.0)
        } else {
            Vec3::new(0.0, 1.0, 0.0)
        };
        return Vec4::from_axis_angle(from.cross(aside), std::f32::consts::PI);
    }
    let axis = from.cross(to);
    Vec4::new(axis.x, axis.y, axis.z, 1.0 + dot).normalize()
}

fn rest_rotation(model: &[JointTransform], joint: usize, correction: Option<Vec4>) -> Vec4 {
    let rest = model[joint].rotation;
    match correction {
        Some(correction) => rest.mul_quat(correction).normalize(),
        None => rest,
    }
}

/// The rotation partway along a chain, interpolated between its joints.
fn chain_rotation(model: &[JointTransform], chain: &[usize], position: f32) -> Vec4 {
    match chain.len() {
        0 => Vec4::quat_identity(),
        1 => model[chain[0]].rotation,
        length => {
            let scaled = position.clamp(0.0, 1.0) * (length - 1) as f32;
            let lower = (scaled.floor() as usize).min(length - 1);
            let upper = (lower + 1).min(length - 1);
            model[chain[lower]]
                .rotation
                .slerp(model[chain[upper]].rotation, scaled - lower as f32)
        }
    }
}

fn nearest_chain_joint(chain: &[usize], position: f32) -> usize {
    if chain.is_empty() {
        return 0;
    }
    let scaled = position.clamp(0.0, 1.0) * (chain.len() - 1) as f32;
    chain[(scaled.round() as usize).min(chain.len() - 1)]
}

/// How much bigger the target rig is than the source rig.
///
/// Measured from the rest poses, never from the source file's scene scale:
/// exporters disagree about units, and a scene scale says nothing about how
/// long the character's legs are. Falls back through progressively cruder
/// measurements so a partial rig still gets a sane number.
fn size_ratio(
    resolved: &ResolvedProfile,
    source_model: &[JointTransform],
    target_model: &[JointTransform],
    policy: ScalePolicy,
    up: Axis,
) -> f32 {
    let measure = match policy {
        ScalePolicy::None => return 1.0,
        ScalePolicy::Fixed(ratio) => return ratio,
        ScalePolicy::Auto(measure) => measure,
    };

    let order = match measure {
        ScaleMeasure::PelvisToHead => [
            ScaleMeasure::PelvisToHead,
            ScaleMeasure::LegLength,
            ScaleMeasure::SkeletonHeight,
        ],
        ScaleMeasure::LegLength => [
            ScaleMeasure::LegLength,
            ScaleMeasure::PelvisToHead,
            ScaleMeasure::SkeletonHeight,
        ],
        ScaleMeasure::SkeletonHeight => [
            ScaleMeasure::SkeletonHeight,
            ScaleMeasure::PelvisToHead,
            ScaleMeasure::LegLength,
        ],
    };

    for measure in order {
        let source = measure_rig(&resolved.source, source_model, measure, up);
        let target = measure_rig(&resolved.target, target_model, measure, up);
        if let (Some(source), Some(target)) = (source, target)
            && source > 1.0e-4
            && target > 1.0e-4
        {
            return target / source;
        }
    }
    1.0
}

fn measure_rig(
    rig: &ResolvedRig,
    model: &[JointTransform],
    measure: ScaleMeasure,
    up: Axis,
) -> Option<f32> {
    let position = |role: HumanoidJoint| rig.joint(role).map(|index| model[index].translation);
    match measure {
        ScaleMeasure::PelvisToHead => {
            let pelvis = position(HumanoidJoint::Pelvis)?;
            let head = position(HumanoidJoint::Head)
                .or_else(|| position(HumanoidJoint::Neck))
                .or_else(|| position(HumanoidJoint::Chest))?;
            Some((head - pelvis).length())
        }
        ScaleMeasure::LegLength => {
            let pelvis = position(HumanoidJoint::Pelvis)?;
            let knee = position(HumanoidJoint::LowerLegLeft)
                .or_else(|| position(HumanoidJoint::LowerLegRight))?;
            let foot =
                position(HumanoidJoint::FootLeft).or_else(|| position(HumanoidJoint::FootRight))?;
            let hip = position(HumanoidJoint::UpperLegLeft)
                .or_else(|| position(HumanoidJoint::UpperLegRight))
                .unwrap_or(pelvis);
            Some((knee - hip).length() + (foot - knee).length())
        }
        ScaleMeasure::SkeletonHeight => {
            let axis = up.vector();
            let mut lowest = f32::MAX;
            let mut highest = f32::MIN;
            for transform in model {
                let height = transform.translation.dot(axis);
                lowest = lowest.min(height);
                highest = highest.max(height);
            }
            (highest > lowest).then_some(highest - lowest)
        }
    }
}
