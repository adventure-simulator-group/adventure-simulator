use adventuresim_stdb_client::{ConnectedPlayerItem, EquipmentBodyPart};
use adventuresim_tactical_core::prelude::{ArmorItem, ArmorSide, ArmorSlot};

use super::tactical_covered_parts;

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
