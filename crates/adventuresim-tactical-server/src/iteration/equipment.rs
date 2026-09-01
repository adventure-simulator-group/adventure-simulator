use adventuresim_core::autoresolve::{MeleeIterationBuild, body_part_index};
use adventuresim_tactical_core::prelude::*;
use bevy::prelude::*;

pub(super) fn spawn_equipment(world: &mut World, owner: Entity, build: &MeleeIterationBuild) {
    spawn_weapon(world, owner, build);
    spawn_shield(world, owner, build);
    spawn_armor(world, owner, build);
}

fn spawn_weapon(world: &mut World, owner: Entity, build: &MeleeIterationBuild) {
    if let Some(weapon) = build.combatant.equipment.melee_weapon {
        let definition = adventuresim_core::item_catalog::definition(build.weapon_id)
            .expect("iteration roster validates its weapon");
        let authored = definition
            .equipment
            .as_ref()
            .expect("weapon equipment metadata");
        let skills = weapon.skills;
        world.spawn((
            ItemOf(owner),
            ItemProperties {
                id: build.weapon_id.into(),
                weight: definition.weight_kg,
            },
            TacticalEquipmentPhysical {
                dimensions_m: Vec3::from_array(authored.physical.dimensions_m),
                grip_to_tip_m: authored.physical.grip_to_tip_m,
                striking_head_length_m: authored.physical.dimensions_m[0]
                    .max(authored.physical.dimensions_m[2]),
                anchor_offset_m: Vec3::from_array(authored.physical.anchor_offset_m),
            },
            TacticalInventoryItemId(1),
            EquipSlot::HoldingRight,
            WeaponItem {
                striking_material: authored
                    .striking_material
                    .expect("weapon striking material"),
                skill_weights: [
                    skills.polearm,
                    skills.axe,
                    skills.bludgeon,
                    skills.sword,
                    skills.knife,
                    0.0,
                    0.0,
                    0.0,
                    skills.throw,
                ],
                accuracy: weapon.accuracy,
                swing_precision: weapon.swing_precision,
                stab_precision: weapon.stab_precision,
                prefers_stab: matches!(
                    weapon.preferred_melee_style,
                    adventuresim_core::combat_style::MeleeAttackStyle::Stab
                ),
                penetration: weapon.penetration,
                reach: weapon.melee_reach,
                grip_to_tip_m: authored.physical.grip_to_tip_m,
                moment_of_inertia_kg_m2: match definition.kind {
                    adventuresim_core::item_catalog::ItemKind::Weapon {
                        moment_of_inertia_kg_m2,
                        ..
                    } => moment_of_inertia_kg_m2,
                    _ => unreachable!(),
                },
                precise: weapon.precise,
                melee: true,
                ranged: false,
                blunt: weapon.blunt,
                slash: weapon.slash,
                pierce: weapon.pierce,
            },
        ));
    }
}

fn spawn_shield(world: &mut World, owner: Entity, build: &MeleeIterationBuild) {
    if let Some(shield_id) = build.shield_id {
        let definition = adventuresim_core::item_catalog::definition(shield_id)
            .expect("iteration roster validates its shield");
        world.spawn((
            ItemOf(owner),
            ItemProperties {
                id: shield_id.into(),
                weight: definition.weight_kg,
            },
            TacticalInventoryItemId(2),
            EquipSlot::HoldingLeft,
            ShieldItem {
                block: build.combatant.equipment.shield_block_bonus,
            },
        ));
    }
}

fn spawn_armor(world: &mut World, owner: Entity, build: &MeleeIterationBuild) {
    for (item_index, armor_id) in build.armor_ids.iter().enumerate() {
        let definition = adventuresim_core::item_catalog::definition(armor_id)
            .expect("iteration roster validates its armor");
        let authored = definition
            .equipment
            .as_ref()
            .expect("iteration armor has equipment metadata");
        let occurrence = build.armor_ids[..item_index]
            .iter()
            .filter(|prior| *prior == armor_id)
            .count();
        let placement = authored
            .placements
            .get(occurrence % authored.placements.len().max(1))
            .expect("iteration armor has an authored placement");
        let part = placement
            .protection
            .first()
            .copied()
            .map(adventuresim_core::equipment::equipment_body_part)
            .expect("iteration armor placement protects a body part");
        let armor = build.combatant.equipment.armor[body_part_index(part)];
        let material = authored.material.expect("armor material metadata");
        let mut covered_parts = [false; 7];
        let mut coverage_spans = [None; 7];
        let mut coverage_geometry = [None; 7];
        for authored_part in &placement.protection {
            let body_part = adventuresim_core::equipment::equipment_body_part(*authored_part);
            let part_index = body_part_index(body_part);
            covered_parts[part_index] = true;
            let geometry = adventuresim_core::combat::authored_armor_coverage(
                placement,
                body_part,
                armor.coverage,
            );
            coverage_spans[part_index] = Some(geometry.span);
            coverage_geometry[part_index] = Some(geometry);
        }
        world.spawn((
            ItemOf(owner),
            ItemProperties {
                id: (*armor_id).into(),
                weight: definition.weight_kg,
            },
            TacticalInventoryItemId(10 + item_index as u64),
            EquipSlot::from_armor_body_part(part),
            ArmorItem {
                material,
                range_of_motion: armor.range_of_motion,
                coverage: armor.coverage,
                slot: armor_slot(part),
                resistance: armor.resistance,
                padding: armor.padding,
                flexibility: armor.flexibility,
                covered_parts,
                coverage_spans,
                coverage_geometry,
                layer_order: placement
                    .occupancy
                    .iter()
                    .map(|occupancy| occupancy.channel.order())
                    .max()
                    .unwrap_or_default(),
            },
        ));
    }
}

fn armor_slot(part: BodyPart) -> ArmorSlot {
    match part {
        BodyPart::LeftArm => ArmorSlot::Arms(Some(ArmorSide::Left)),
        BodyPart::RightArm => ArmorSlot::Arms(Some(ArmorSide::Right)),
        BodyPart::LeftLeg => ArmorSlot::Legs(Some(ArmorSide::Left)),
        BodyPart::RightLeg => ArmorSlot::Legs(Some(ArmorSide::Right)),
        BodyPart::Chest => ArmorSlot::Chest,
        BodyPart::Stomach => ArmorSlot::Stomach,
        BodyPart::Head => ArmorSlot::Head,
    }
}
