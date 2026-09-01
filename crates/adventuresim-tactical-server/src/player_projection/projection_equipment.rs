use adventuresim_stdb_client::{ConnectedPlayerItem, EquipmentBodyPart};
use adventuresim_tactical_core::prelude::{
    ArmorItem, ArmorSide, ArmorSlot, WeaponAppearance, WeaponHolderAppearance,
};

use super::tactical_covered_parts;

pub(super) fn projected_parametric_weapon(
    item: &ConnectedPlayerItem,
) -> Option<adventuresim_core::equipment::ParametricWeaponCombatGeometry> {
    let Some(appearance) = item.weapon_appearance.as_ref() else {
        assert!(
            adventuresim_weapon_model::default_design(&item.item.id).is_none(),
            "strategic authority omitted a parametric weapon recipe"
        );
        return None;
    };
    parametric_combat_geometry(&item.item.id, item.item.reach, appearance)
}

pub(super) fn parametric_combat_geometry(
    item_id: &str,
    catalog_melee_reach_m: f32,
    appearance: &adventuresim_stdb_client::ConnectedWeaponAppearance,
) -> Option<adventuresim_core::equipment::ParametricWeaponCombatGeometry> {
    use adventuresim_weapon_model::{GENERATOR_VERSION, decode, derive_properties, design_hash};
    assert_eq!(appearance.generator_version, GENERATOR_VERSION);
    let design = decode(&appearance.recipe).expect("strategic authority sent a decodable recipe");
    assert_eq!(design.catalog_id, item_id);
    assert_eq!(design_hash(&design).0.as_slice(), appearance.design_hash);
    let derived = derive_properties(&design).expect("strategic authority sent a valid recipe");
    let default_grip = adventuresim_weapon_model::default_design(item_id)
        .and_then(|design| derive_properties(&design).ok())
        .expect("parametric weapon has a canonical default recipe")
        .grip_to_tip_m;
    adventuresim_core::equipment::ParametricWeaponCombatGeometry::new(
        derived.mass_kg,
        derived.length_m,
        derived.grip_to_tip_m,
        derived.striking_head_length_m,
        derived.moment_of_inertia_kg_m2,
        derived.balance,
        catalog_melee_reach_m,
        default_grip,
    )
}

pub(super) fn projected_weapon_appearance(item: &ConnectedPlayerItem) -> Option<WeaponAppearance> {
    item.weapon_appearance
        .as_ref()
        .map(|appearance| WeaponAppearance {
            generator_version: appearance.generator_version,
            design_hash: appearance
                .design_hash
                .as_slice()
                .try_into()
                .expect("validated weapon design hash"),
            recipe: appearance.recipe.clone(),
        })
}

pub(super) fn projected_holder_appearance(
    item: &ConnectedPlayerItem,
) -> Option<WeaponHolderAppearance> {
    item.weapon_holder_appearance
        .as_ref()
        .map(|appearance| WeaponHolderAppearance {
            generator_version: appearance.generator_version,
            design_hash: appearance
                .design_hash
                .as_slice()
                .try_into()
                .expect("validated holder design hash"),
            recipe: appearance.recipe.clone(),
        })
}

pub(super) fn projected_armor(item: &ConnectedPlayerItem) -> Option<ArmorItem> {
    let part = item.protected_body_parts.first()?;
    let definition = adventuresim_core::item_catalog::definition(&item.item.id)
        .expect("validated armor exists in authored catalog");
    let authored = definition
        .equipment
        .as_ref()
        .expect("validated armor has equipment metadata");
    let placement = authored
        .placements
        .iter()
        .find(|placement| Some(placement.id.as_str()) == item.selected_placement_id.as_deref())
        .expect("validated armor retains its authored placement");
    let slot = match part {
        EquipmentBodyPart::LeftArm => ArmorSlot::Arms(Some(ArmorSide::Left)),
        EquipmentBodyPart::RightArm => ArmorSlot::Arms(Some(ArmorSide::Right)),
        EquipmentBodyPart::LeftLeg => ArmorSlot::Legs(Some(ArmorSide::Left)),
        EquipmentBodyPart::RightLeg => ArmorSlot::Legs(Some(ArmorSide::Right)),
        EquipmentBodyPart::Head => ArmorSlot::Head,
        EquipmentBodyPart::Chest => ArmorSlot::Chest,
        EquipmentBodyPart::Stomach => ArmorSlot::Stomach,
    };
    let mut coverage_spans = [None; 7];
    let mut coverage_geometry = [None; 7];
    for protected in &placement.protection {
        let body_part = adventuresim_core::equipment::equipment_body_part(*protected);
        let geometry = adventuresim_core::combat::authored_armor_coverage(
            placement,
            body_part,
            item.item.coverage,
        );
        let index = adventuresim_core::autoresolve::body_part_index(body_part);
        coverage_spans[index] = Some(geometry.span);
        coverage_geometry[index] = Some(geometry);
    }
    Some(ArmorItem {
        material: authored.material.expect("validated armor has a material"),
        range_of_motion: item.item.range_of_motion,
        coverage: item.item.coverage,
        slot,
        resistance: item.item.resistance,
        padding: item.item.padding,
        flexibility: item.item.flexibility,
        covered_parts: tactical_covered_parts(&item.protected_body_parts),
        coverage_spans,
        coverage_geometry,
        layer_order: placement
            .occupancy
            .iter()
            .map(|occupancy| occupancy.channel.order())
            .max()
            .unwrap_or_default(),
    })
}
