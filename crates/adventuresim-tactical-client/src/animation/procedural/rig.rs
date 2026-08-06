use super::super::AnimationRigScene;
use super::*;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HumanoidBone {
    pub(crate) owner: Entity,
    pub(crate) role: BoneRole,
}

/// Cached rig topology. It changes only while an asynchronously loaded scene
/// is binding; procedural passes read it without rebuilding owner/role maps.
#[derive(Component, Debug, Clone)]
pub(crate) struct HumanoidRig {
    bones: [Option<Entity>; BoneRole::COUNT],
    rig_scene: Option<Entity>,
    sole_axes: [Option<Vec3>; 2],
}

impl Default for HumanoidRig {
    fn default() -> Self {
        Self {
            bones: [None; BoneRole::COUNT],
            rig_scene: None,
            sole_axes: [None; 2],
        }
    }
}

impl HumanoidRig {
    pub(crate) fn get(&self, role: &BoneRole) -> Option<&Entity> {
        self.bones[role.index()].as_ref()
    }

    pub(super) fn rig_scene(&self) -> Option<Entity> {
        self.rig_scene
    }

    pub(super) fn sole_axis(&self, left: bool) -> Option<Vec3> {
        self.sole_axes[usize::from(!left)]
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum BoneRole {
    Root,
    Pelvis,
    StomachOne,
    StomachTwo,
    Chest,
    NeckOne,
    NeckTwo,
    Head,
    ClavicleLeft,
    ClavicleRight,
    ThighLeft,
    ThighTwistLeft,
    ShinLeft,
    ShinTwistLeft,
    FootLeft,
    ToeLeft,
    ThighRight,
    ThighTwistRight,
    ShinRight,
    ShinTwistRight,
    FootRight,
    ToeRight,
    UpperArmLeft,
    UpperArmTwistLeft,
    ForearmLeft,
    ForearmTwistLeft,
    HandLeft,
    WeaponLeft,
    UpperArmRight,
    UpperArmTwistRight,
    ForearmRight,
    ForearmTwistRight,
    HandRight,
    WeaponRight,
}

impl BoneRole {
    pub(super) const COUNT: usize = 34;
    pub(super) const ALL: [Self; Self::COUNT] = [
        Self::Root,
        Self::Pelvis,
        Self::StomachOne,
        Self::StomachTwo,
        Self::Chest,
        Self::NeckOne,
        Self::NeckTwo,
        Self::Head,
        Self::ClavicleLeft,
        Self::ClavicleRight,
        Self::ThighLeft,
        Self::ThighTwistLeft,
        Self::ShinLeft,
        Self::ShinTwistLeft,
        Self::FootLeft,
        Self::ToeLeft,
        Self::ThighRight,
        Self::ThighTwistRight,
        Self::ShinRight,
        Self::ShinTwistRight,
        Self::FootRight,
        Self::ToeRight,
        Self::UpperArmLeft,
        Self::UpperArmTwistLeft,
        Self::ForearmLeft,
        Self::ForearmTwistLeft,
        Self::HandLeft,
        Self::WeaponLeft,
        Self::UpperArmRight,
        Self::UpperArmTwistRight,
        Self::ForearmRight,
        Self::ForearmTwistRight,
        Self::HandRight,
        Self::WeaponRight,
    ];

    pub(crate) fn index(self) -> usize {
        self as usize
    }

    pub(super) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "root" => Self::Root,
            "pelvis" => Self::Pelvis,
            "stomach_01" => Self::StomachOne,
            "stomach_02" => Self::StomachTwo,
            "chest" => Self::Chest,
            "neck_01" => Self::NeckOne,
            "neck_02" => Self::NeckTwo,
            "head" => Self::Head,
            "clavicle.L" => Self::ClavicleLeft,
            "clavicle.R" => Self::ClavicleRight,
            "thigh.L" => Self::ThighLeft,
            "thigh_twist.L" => Self::ThighTwistLeft,
            "shin.L" => Self::ShinLeft,
            "shin_twist.L" => Self::ShinTwistLeft,
            "foot.L" => Self::FootLeft,
            "toe.L" => Self::ToeLeft,
            "thigh.R" => Self::ThighRight,
            "thigh_twist.R" => Self::ThighTwistRight,
            "shin.R" => Self::ShinRight,
            "shin_twist.R" => Self::ShinTwistRight,
            "foot.R" => Self::FootRight,
            "toe.R" => Self::ToeRight,
            "upper_arm.L" => Self::UpperArmLeft,
            "upper_arm_twist.L" => Self::UpperArmTwistLeft,
            "forearm.L" => Self::ForearmLeft,
            "forearm_twist.L" => Self::ForearmTwistLeft,
            "hand.L" => Self::HandLeft,
            "weapon.L" => Self::WeaponLeft,
            "upper_arm.R" => Self::UpperArmRight,
            "upper_arm_twist.R" => Self::UpperArmTwistRight,
            "forearm.R" => Self::ForearmRight,
            "forearm_twist.R" => Self::ForearmTwistRight,
            "hand.R" => Self::HandRight,
            "weapon.R" => Self::WeaponRight,
            _ => return None,
        })
    }
}

fn retain_stable_role(
    bones: &mut [Option<Entity>; BoneRole::COUNT],
    role: BoneRole,
    entity: Entity,
) -> Option<Entity> {
    let slot = &mut bones[role.index()];
    let duplicate = *slot;
    *slot = Some(duplicate.map_or(entity, |existing| existing.min(entity)));
    duplicate
}

pub(crate) fn bind_humanoid_bones(
    mut commands: Commands,
    bones: Query<(Entity, &Name), (Added<Name>, Without<HumanoidBone>)>,
    parents: Query<&ChildOf>,
    roots: Query<&AnimationRigScene>,
) {
    for (entity, name) in &bones {
        let Some(role) = BoneRole::from_name(name.as_str()) else {
            continue;
        };
        let mut current = entity;
        for _ in 0..64 {
            if let Ok(root) = roots.get(current) {
                commands.entity(entity).insert(HumanoidBone {
                    owner: root.0,
                    role,
                });
                break;
            }
            let Ok(parent) = parents.get(current) else {
                break;
            };
            current = parent.parent();
        }
    }
}

pub(crate) fn cache_humanoid_rigs(
    mut commands: Commands,
    all_bones: Query<(Entity, &HumanoidBone)>,
    cached: Query<(Entity, &HumanoidRig)>,
    added: Query<(), Added<HumanoidBone>>,
    mut removed: RemovedComponents<HumanoidBone>,
    rig_scenes: Query<(Entity, &AnimationRigScene)>,
) {
    let topology_changed = !added.is_empty() || removed.read().next().is_some();
    if !topology_changed {
        return;
    }
    let mut rigs = BTreeMap::<Entity, [Option<Entity>; BoneRole::COUNT]>::new();
    for (entity, bone) in &all_bones {
        let bones = rigs.entry(bone.owner).or_insert([None; BoneRole::COUNT]);
        if retain_stable_role(bones, bone.role, entity).is_some() {
            let retained = bones[bone.role.index()].expect("duplicate role remains populated");
            warn!(
                owner = ?bone.owner,
                role = ?bone.role,
                ?retained,
                "Duplicate humanoid bone role; retaining the lowest stable entity id"
            );
        }
    }
    let rig_roots = rig_scenes
        .iter()
        .map(|(root, scene)| (scene.0, root))
        .collect::<BTreeMap<_, _>>();
    for (owner, _) in &cached {
        if !rigs.contains_key(&owner) {
            commands.entity(owner).remove::<HumanoidRig>();
        }
    }
    for (owner, bones) in rigs {
        let sole_axes = cached
            .get(owner)
            .map(|(_, rig)| rig.sole_axes)
            .unwrap_or([None; 2]);
        if let Ok(mut entity) = commands.get_entity(owner) {
            entity.insert(HumanoidRig {
                bones,
                rig_scene: rig_roots.get(&owner).copied(),
                sole_axes,
            });
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub(in crate::animation) struct SoleAxisCaptured;

/// Captures the foot's bind-space sole normal from the authored global bind
/// transform. The Cascadeur rig's local +Y points ankle-to-toe, so assuming a
/// cardinal local up axis would pitch the feet even on flat terrain.
pub(crate) fn capture_humanoid_rig_axes(
    mut commands: Commands,
    feet: Query<(Entity, &HumanoidBone), (Added<HumanoidBone>, Without<SoleAxisCaptured>)>,
    mut rigs: Query<&mut HumanoidRig>,
    helper: TransformHelper,
) {
    for (entity, bone) in &feet {
        if !matches!(bone.role, BoneRole::FootLeft | BoneRole::FootRight) {
            continue;
        }
        let Ok(global) = helper.compute_global_transform(entity) else {
            continue;
        };
        let axis = sole_up_axis_from_bind(global.rotation());
        if let Some(axis) = axis.try_normalize() {
            commands.entity(entity).insert(SoleAxisCaptured);
            if let Ok(mut rig) = rigs.get_mut(bone.owner) {
                let index = usize::from(bone.role == BoneRole::FootRight);
                rig.sole_axes[index] = Some(axis);
            }
        }
    }
}

pub(super) fn sole_up_axis_from_bind(bind_world_rotation: Quat) -> Vec3 {
    bind_world_rotation.inverse() * Vec3::Y
}

pub(crate) fn pole_to_world(owner_rotation: Quat, owner_local_pole: Vec3) -> Vec3 {
    owner_rotation * owner_local_pole
}

pub(crate) fn pole_to_owner(owner_rotation: Quat, world_pole: Vec3) -> Vec3 {
    owner_rotation.inverse() * world_pole
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_roles_retain_the_lowest_stable_entity() {
        let mut bones = [None; BoneRole::COUNT];
        let high = Entity::from_bits(20);
        let low = Entity::from_bits(10);
        assert_eq!(retain_stable_role(&mut bones, BoneRole::Head, high), None);
        assert_eq!(
            retain_stable_role(&mut bones, BoneRole::Head, low),
            Some(high)
        );
        assert_eq!(bones[BoneRole::Head.index()], Some(low));
    }

    #[test]
    fn partial_and_empty_rigs_have_explicit_missing_chains() {
        let mut rig = HumanoidRig::default();
        assert_eq!(rig.get(&BoneRole::Head), None);
        retain_stable_role(&mut rig.bones, BoneRole::Head, Entity::from_bits(7));
        assert_eq!(rig.get(&BoneRole::Head), Some(&Entity::from_bits(7)));
        assert_eq!(rig.get(&BoneRole::NeckOne), None);
    }
}
