//! Turning names into joint indices.
//!
//! This is the only place in the retargeting system that looks at strings.
//! Once a profile has been resolved against a skeleton, everything downstream
//! addresses joints by index, which is what makes the retargeter itself
//! indifferent to where an animation came from.

use crate::skeleton::Skeleton;
use anyhow::{Result, bail};
use indexmap::IndexMap;

use super::profile::{RetargetProfile, RetargetSettings, RigProfile, RootSource};
use super::semantic::{HumanoidChain, HumanoidJoint};

/// Loose name matching: `mixamorig:LeftUpLeg`, `mixamorig1:LeftUpLeg`,
/// `Left_Up_Leg` and `leftupleg` all reduce to the same key.
///
/// Namespaces are stripped because exporters add and rename them freely;
/// separators and case are dropped because rig authors are inconsistent about
/// both. Nothing else is normalized — this must not make distinct joints
/// collide.
fn normalize(name: &str) -> String {
    let name = name.rsplit([':', '|']).next().unwrap_or(name);
    name.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

/// A skeleton's joint names indexed for repeated lookups.
struct JointIndex {
    exact: IndexMap<String, usize>,
    normalized: IndexMap<String, usize>,
}

impl JointIndex {
    fn new(skeleton: &Skeleton) -> Self {
        let mut exact = IndexMap::new();
        let mut normalized = IndexMap::new();
        for (index, joint) in skeleton.joints.iter().enumerate() {
            exact.entry(joint.name.clone()).or_insert(index);
            normalized.entry(normalize(&joint.name)).or_insert(index);
        }
        Self { exact, normalized }
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.exact
            .get(name)
            .or_else(|| self.normalized.get(&normalize(name)))
            .copied()
    }
}

/// A profile bound to a concrete skeleton.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedRig {
    pub profile: String,
    /// Skeleton joint index per humanoid role that resolved.
    pub joints: IndexMap<HumanoidJoint, usize>,
    /// Skeleton joint indices per chain, root-most first.
    pub chains: IndexMap<HumanoidChain, Vec<usize>>,
    /// The joint carrying locomotion, if the rig has one.
    pub root: Option<usize>,
    /// Optional roles the profile named but the skeleton does not have.
    pub missing: Vec<HumanoidJoint>,
    /// Skeleton joints no role or chain claims. Harmless; listed for tooling.
    pub unmapped: Vec<usize>,
}

impl ResolvedRig {
    pub fn joint(&self, role: HumanoidJoint) -> Option<usize> {
        self.joints.get(&role).copied()
    }

    pub fn has(&self, role: HumanoidJoint) -> bool {
        self.joints.contains_key(&role)
    }

    /// The roles this rig resolved that a later IK or contact pass would pin.
    pub fn end_effectors(&self) -> Vec<(HumanoidJoint, usize)> {
        self.joints
            .iter()
            .filter(|(role, _)| role.is_end_effector())
            .map(|(role, index)| (*role, *index))
            .collect()
    }
}

impl RigProfile {
    /// Binds this profile's names to a skeleton's joints.
    ///
    /// Fails only on joints the profile marked required; everything else is
    /// reported and carried on without.
    pub fn resolve(&self, skeleton: &Skeleton) -> Result<ResolvedRig> {
        let index = JointIndex::new(skeleton);

        let mut joints = IndexMap::new();
        let mut missing = Vec::new();
        let mut missing_required = Vec::new();
        for (role, binding) in &self.joints {
            match binding.names.iter().find_map(|name| index.find(name)) {
                Some(joint) => {
                    joints.insert(*role, joint);
                }
                None if binding.required => missing_required.push(*role),
                None => missing.push(*role),
            }
        }

        if !missing_required.is_empty() {
            let names: Vec<String> = missing_required
                .iter()
                .map(|role| {
                    let candidates = self.joints[role].names.join(" | ");
                    format!("{role} (expected {candidates})")
                })
                .collect();
            bail!(
                "rig profile {:?} requires joints the skeleton does not have: {}",
                self.name,
                names.join(", ")
            );
        }

        let mut chains: IndexMap<HumanoidChain, Vec<usize>> = IndexMap::new();
        for chain in HumanoidChain::ALL {
            let resolved: Vec<usize> = match self.chains.get(chain) {
                // An explicit chain lists the rig's own joints, extras included.
                Some(binding) => binding
                    .joints
                    .iter()
                    .filter_map(|name| index.find(name))
                    .collect(),
                // Otherwise the chain is whatever roles of it did resolve.
                None => chain
                    .joints()
                    .iter()
                    .filter_map(|role| joints.get(role).copied())
                    .collect(),
            };
            if !resolved.is_empty() {
                chains.insert(*chain, resolved);
            }
        }

        let root = match &self.root {
            RootSource::Pelvis => joints.get(&HumanoidJoint::Pelvis).copied(),
            RootSource::Joint(name) => index.find(name),
            RootSource::None => None,
        };

        let claimed: std::collections::HashSet<usize> = joints
            .values()
            .copied()
            .chain(chains.values().flatten().copied())
            .chain(root)
            .collect();
        let unmapped = (0..skeleton.joints.len())
            .filter(|index| !claimed.contains(index))
            .collect();

        Ok(ResolvedRig {
            profile: self.name.clone(),
            joints,
            chains,
            root,
            missing,
            unmapped,
        })
    }

    /// Whether a skeleton looks like this rig, by its marker joints.
    ///
    /// Detection is a convenience for tooling. It is never required: an
    /// explicit profile always works, and a rig with no markers simply never
    /// auto-detects.
    pub fn matches(&self, skeleton: &Skeleton) -> bool {
        if self.markers.is_empty() {
            return false;
        }
        let index = JointIndex::new(skeleton);
        self.markers.iter().all(|name| index.find(name).is_some())
    }
}

/// Both rigs bound to their skeletons, plus the policy joining them.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedProfile {
    pub name: String,
    pub source: ResolvedRig,
    pub target: ResolvedRig,
    pub settings: RetargetSettings,
}

impl RetargetProfile {
    pub fn resolve(
        &self,
        source_skeleton: &Skeleton,
        target_skeleton: &Skeleton,
    ) -> Result<ResolvedProfile> {
        let source = self.source.resolve(source_skeleton)?;
        let target = self.target.resolve(target_skeleton)?;

        if self.settings.strict {
            let orphaned: Vec<String> = source
                .joints
                .keys()
                .filter(|role| !target.has(**role))
                .map(|role| role.to_string())
                .collect();
            if !orphaned.is_empty() {
                bail!(
                    "strict retargeting: the target rig has no joint for {}",
                    orphaned.join(", ")
                );
            }
        }

        Ok(ResolvedProfile {
            name: self.name.clone(),
            source,
            target,
            settings: self.settings.clone(),
        })
    }
}

impl ResolvedProfile {
    /// Roles both rigs resolved, which is what actually gets retargeted.
    pub fn shared_roles(&self) -> Vec<HumanoidJoint> {
        HumanoidJoint::ALL
            .iter()
            .copied()
            .filter(|role| self.source.has(*role) && self.target.has(*role))
            .collect()
    }

    /// A human-readable dump of the mapping, for checking a new rig.
    ///
    /// ```text
    /// Pelvis:
    ///   source = mixamorig:Hips
    ///   target = root
    /// ```
    pub fn report(&self, source_skeleton: &Skeleton, target_skeleton: &Skeleton) -> String {
        let mut report = String::new();
        let name = |skeleton: &Skeleton, index: Option<usize>| match index {
            Some(index) => skeleton.joints[index].name.clone(),
            None => "<unmapped>".to_string(),
        };

        report.push_str(&format!("profile: {}\n", self.name));
        report.push_str(&format!(
            "source rig: {} ({} joints)\n",
            self.source.profile,
            source_skeleton.joints.len()
        ));
        report.push_str(&format!(
            "target rig: {} ({} joints)\n\n",
            self.target.profile,
            target_skeleton.joints.len()
        ));

        for role in HumanoidJoint::ALL {
            let source = self.source.joint(*role);
            let target = self.target.joint(*role);
            if source.is_none() && target.is_none() {
                continue;
            }
            report.push_str(&format!("{role}:\n"));
            report.push_str(&format!("  source = {}\n", name(source_skeleton, source)));
            report.push_str(&format!("  target = {}\n", name(target_skeleton, target)));
        }

        for chain in HumanoidChain::ALL {
            let source = self.source.chains.get(chain);
            let target = self.target.chains.get(chain);
            let (Some(source), Some(target)) = (source, target) else {
                continue;
            };
            if source.len() == target.len() {
                continue;
            }
            let joints = |skeleton: &Skeleton, indices: &[usize]| {
                indices
                    .iter()
                    .map(|index| skeleton.joints[*index].name.as_str())
                    .collect::<Vec<_>>()
                    .join(" -> ")
            };
            report.push_str(&format!(
                "\nchain {chain} ({} source joints, {} target joints, motion distributed)\n",
                source.len(),
                target.len()
            ));
            report.push_str(&format!("  source = {}\n", joints(source_skeleton, source)));
            report.push_str(&format!("  target = {}\n", joints(target_skeleton, target)));
        }

        if !self.source.missing.is_empty() {
            let missing: Vec<String> = self
                .source
                .missing
                .iter()
                .map(ToString::to_string)
                .collect();
            report.push_str(&format!(
                "\nsource rig is missing: {}\n",
                missing.join(", ")
            ));
        }
        if !self.target.missing.is_empty() {
            let missing: Vec<String> = self
                .target
                .missing
                .iter()
                .map(ToString::to_string)
                .collect();
            report.push_str(&format!("target rig is missing: {}\n", missing.join(", ")));
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaces_and_separators_do_not_defeat_lookup() {
        assert_eq!(normalize("mixamorig:LeftUpLeg"), "leftupleg");
        assert_eq!(normalize("mixamorig1:Left_Up_Leg"), "leftupleg");
        assert_eq!(normalize("Armature|LeftUpLeg"), "leftupleg");
        assert_ne!(normalize("l_upleg"), normalize("r_upleg"));
    }
}
