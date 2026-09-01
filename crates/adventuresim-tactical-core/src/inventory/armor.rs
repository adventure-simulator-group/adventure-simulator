use adventuresim_core::item_catalog_schema::EquipmentMaterial;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[reflect(opaque)]
#[reflect(Component)]
pub struct ArmorItem {
    pub material: EquipmentMaterial,
    pub range_of_motion: f32,
    pub coverage: f32,
    pub slot: ArmorSlot,
    pub resistance: f32,
    pub padding: f32,
    pub flexibility: f32,
    pub covered_parts: [bool; 7],
    #[reflect(ignore)]
    pub coverage_spans: [Option<adventuresim_core::combat::ArmorCoverageSpan>; 7],
    #[reflect(ignore)]
    pub coverage_geometry: [Option<adventuresim_core::combat::AuthoredArmorCoverage>; 7],
    /// Higher authored equipment channels are physically farther from tissue.
    pub layer_order: u8,
}

/// Stable strategic inventory identity retained on the transient tactical
/// projection so contact consequences can name the exact engaged item.
#[derive(Component, Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component)]
pub struct TacticalInventoryItemId(pub u64);

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ArmorLayerContact {
    pub item_id: String,
    pub inventory_item_id: Option<u64>,
    pub material: EquipmentMaterial,
    pub geometry: adventuresim_core::combat::AuthoredArmorCoverage,
    pub intersected: bool,
    pub selected: bool,
    pub surface: adventuresim_core::equipment::ArmorSurface,
}

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum ArmorSlot {
    Arms(Option<ArmorSide>),
    Legs(Option<ArmorSide>),
    Head,
    Chest,
    Stomach,
}

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum ArmorSide {
    Left,
    Right,
}
