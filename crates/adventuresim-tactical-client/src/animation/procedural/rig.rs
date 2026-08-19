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

    pub(crate) fn sole_axis(&self, left: bool) -> Option<Vec3> {
        self.sole_axes[usize::from(!left)]
    }

    #[cfg(test)]
    pub(super) fn with_test_bones(bones: &[(BoneRole, Entity)]) -> Self {
        let mut rig = Self::default();
        for &(role, entity) in bones {
            rig.bones[role.index()] = Some(entity);
        }
        rig
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

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Pelvis => "pelvis",
            Self::StomachOne => "stomach_01",
            Self::StomachTwo => "stomach_02",
            Self::Chest => "chest",
            Self::NeckOne => "neck_01",
            Self::NeckTwo => "neck_02",
            Self::Head => "head",
            Self::ClavicleLeft => "clavicle_left",
            Self::ClavicleRight => "clavicle_right",
            Self::ThighLeft => "left_hip",
            Self::ThighTwistLeft => "left_thigh_twist",
            Self::ShinLeft => "left_knee",
            Self::ShinTwistLeft => "left_shin_twist",
            Self::FootLeft => "left_foot",
            Self::ToeLeft => "left_toe",
            Self::ThighRight => "right_hip",
            Self::ThighTwistRight => "right_thigh_twist",
            Self::ShinRight => "right_knee",
            Self::ShinTwistRight => "right_shin_twist",
            Self::FootRight => "right_foot",
            Self::ToeRight => "right_toe",
            Self::UpperArmLeft => "left_shoulder",
            Self::UpperArmTwistLeft => "left_upper_arm_twist",
            Self::ForearmLeft => "left_elbow",
            Self::ForearmTwistLeft => "left_forearm_twist",
            Self::HandLeft => "left_hand",
            Self::WeaponLeft => "left_weapon",
            Self::UpperArmRight => "right_shoulder",
            Self::UpperArmTwistRight => "right_upper_arm_twist",
            Self::ForearmRight => "right_elbow",
            Self::ForearmTwistRight => "right_forearm_twist",
            Self::HandRight => "right_hand",
            Self::WeaponRight => "right_weapon",
        }
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
/// transform. The Cascadeur rig has no cardinal local sole-up axis, so the
/// authored flat bind pose defines it explicitly.
pub(crate) fn capture_humanoid_rig_axes(
    mut commands: Commands,
    feet: Query<(Entity, &HumanoidBone, &AuthoredBindTransform), Without<SoleAxisCaptured>>,
    bind_nodes: Query<(&AuthoredBindTransform, Option<&ChildOf>)>,
    mut rigs: Query<&mut HumanoidRig>,
) {
    for (entity, bone, _) in &feet {
        if !matches!(bone.role, BoneRole::FootLeft | BoneRole::FootRight) {
            continue;
        }
        let Some(bind_global) = authored_bind_global(entity, bone.owner, &bind_nodes) else {
            continue;
        };
        // Never infer the sole plane from the ankle-to-toe joint vector. That
        // vector slopes down through a normally planted foot; forcing it
        // horizontal raises the visible forefoot and produces the toe-up IK
        // pose. Sampling the live FK rotation is likewise invalid because it
        // permanently calibrates the sole from an arbitrary gait frame.
        let axis = sole_up_axis_from_bind(bind_global.rotation());
        if let Some(axis) = axis.try_normalize() {
            commands.entity(entity).insert(SoleAxisCaptured);
            if let Ok(mut rig) = rigs.get_mut(bone.owner) {
                let index = usize::from(bone.role == BoneRole::FootRight);
                rig.sole_axes[index] = Some(axis);
            }
        }
    }
}

fn authored_bind_global(
    entity: Entity,
    owner: Entity,
    bind_nodes: &Query<(&AuthoredBindTransform, Option<&ChildOf>)>,
) -> Option<GlobalTransform> {
    let mut current = entity;
    let mut locals = Vec::new();
    for _ in 0..64 {
        let Ok((bind, parent)) = bind_nodes.get(current) else {
            break;
        };
        if bind.owner != owner {
            break;
        }
        locals.push(bind.local);
        let Some(parent) = parent else {
            break;
        };
        current = parent.parent();
    }
    (!locals.is_empty()).then(|| {
        locals
            .into_iter()
            .rev()
            .fold(GlobalTransform::IDENTITY, |global, local| {
                global.mul_transform(local)
            })
    })
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

    #[test]
    fn sole_axis_is_calibrated_from_authored_bind_not_live_fk() {
        let mut world = World::new();
        let owner = world.spawn(HumanoidRig::default()).id();
        let bind_root_rotation = Quat::from_rotation_y(0.7);
        let rig_root = world
            .spawn(AuthoredBindTransform {
                owner,
                local: Transform::from_rotation(bind_root_rotation),
            })
            .id();
        let bind_foot_rotation = Quat::from_rotation_x(-0.6);
        let foot = world
            .spawn((
                HumanoidBone {
                    owner,
                    role: BoneRole::FootLeft,
                },
                AuthoredBindTransform {
                    owner,
                    local: Transform::from_rotation(bind_foot_rotation),
                },
                // This deliberately disagrees with the authored bind. A live
                // gait sample must have no effect on one-shot calibration.
                Transform::from_rotation(Quat::from_rotation_x(1.2)),
            ))
            .id();
        let toe = world
            .spawn((
                HumanoidBone {
                    owner,
                    role: BoneRole::ToeLeft,
                },
                AuthoredBindTransform {
                    owner,
                    local: Transform::from_translation(Vec3::Y * 0.14),
                },
                Transform::from_translation(Vec3::NEG_Z * 0.14),
            ))
            .id();
        world.entity_mut(rig_root).add_child(foot);
        world.entity_mut(foot).add_child(toe);
        {
            let mut rig = world.get_mut::<HumanoidRig>(owner).unwrap();
            rig.bones[BoneRole::FootLeft.index()] = Some(foot);
            rig.bones[BoneRole::ToeLeft.index()] = Some(toe);
        }

        world.run_system_cached(capture_humanoid_rig_axes).unwrap();

        let bind_world_rotation = bind_root_rotation * bind_foot_rotation;
        let raw_axis = sole_up_axis_from_bind(bind_world_rotation);
        let expected = raw_axis.normalize_or_zero();
        let actual = world
            .get::<HumanoidRig>(owner)
            .unwrap()
            .sole_axis(true)
            .unwrap();
        assert!(actual.dot(expected) > 0.9999);
        assert!(world.get::<SoleAxisCaptured>(foot).is_some());
    }
}
