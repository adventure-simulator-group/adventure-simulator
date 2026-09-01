use crate::{
    body::BodyPart,
    item_catalog_schema::{EquipmentBodyPart, EquipmentChannel},
};

/// Combat projection of one wearable layer over one fine-grained location.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WearableProtection {
    pub inventory_item_id: u64,
    pub body_part: BodyPart,
    pub channel: EquipmentChannel,
    pub order: u16,
    pub coverage: f32,
    pub resistance: f32,
    pub padding: f32,
    pub flexibility: f32,
    pub range_of_motion: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayeredArmor {
    pub coverage: f32,
    pub resistance: f32,
    pub padding: f32,
    pub flexibility: f32,
    pub range_of_motion: f32,
}

/// The specific authored protection layer intersected by a contact sample.
/// `None` from `PlayerEquipment::armor_surface` is an anatomical coverage gap.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArmorSurface {
    pub inventory_item_id: Option<u64>,
    pub material: Option<crate::item_catalog_schema::EquipmentMaterial>,
    pub resistance: f32,
    pub padding: f32,
    pub flexibility: f32,
}

pub const fn equipment_body_part(part: EquipmentBodyPart) -> BodyPart {
    match part {
        EquipmentBodyPart::LeftArm => BodyPart::LeftArm,
        EquipmentBodyPart::RightArm => BodyPart::RightArm,
        EquipmentBodyPart::LeftLeg => BodyPart::LeftLeg,
        EquipmentBodyPart::RightLeg => BodyPart::RightLeg,
        EquipmentBodyPart::Chest => BodyPart::Chest,
        EquipmentBodyPart::Stomach => BodyPart::Stomach,
        EquipmentBodyPart::Head => BodyPart::Head,
    }
}

/// Folds all applicable layers without expanding the combat body-part ABI.
pub fn aggregate_layered_armor(
    part: BodyPart,
    pieces: impl IntoIterator<Item = WearableProtection>,
) -> LayeredArmor {
    let mut result = LayeredArmor {
        coverage: 0.0,
        resistance: 0.0,
        padding: 0.0,
        flexibility: 0.0,
        range_of_motion: 1.0,
    };
    let mut weighted_flexibility = 0.0;
    for piece in pieces.into_iter().filter(|piece| piece.body_part == part) {
        let coverage = piece.coverage.clamp(0.0, 1.0);
        result.coverage = 1.0 - (1.0 - result.coverage) * (1.0 - coverage);
        let resistance = piece.resistance.max(0.0);
        result.resistance += resistance;
        result.padding += piece.padding.max(0.0);
        weighted_flexibility += piece.flexibility.clamp(0.0, 1.0) * resistance;
        result.range_of_motion = result
            .range_of_motion
            .min(piece.range_of_motion.clamp(0.0, 1.0));
    }
    result.flexibility = if result.resistance > f32::EPSILON {
        weighted_flexibility / result.resistance
    } else {
        0.0
    };
    result
}

/// Selects exactly one layer to receive contact wear. Higher layer order is
/// outermost; inventory ID is the deterministic tie-breaker for corrupt data.
pub fn outermost_wearable(
    part: BodyPart,
    pieces: impl IntoIterator<Item = WearableProtection>,
) -> Option<WearableProtection> {
    pieces
        .into_iter()
        .filter(|piece| piece.body_part == part)
        .max_by_key(|piece| (piece.channel.order(), piece.order, piece.inventory_item_id))
}
