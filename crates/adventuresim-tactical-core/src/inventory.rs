use std::num::NonZeroU32;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumCount, VariantArray};

#[derive(Component, Serialize, Deserialize, Debug, Reflect, PartialEq, Eq, Deref, DerefMut)]
pub struct ItemQuantity(pub NonZeroU32);

impl Default for ItemQuantity {
    fn default() -> Self {
        Self(NonZeroU32::new(1).unwrap())
    }
}

#[derive(Component, Serialize, Deserialize, Debug, Reflect, PartialEq, Eq, Clone)]
#[require(ItemProperties, ItemQuantity)]
#[relationship(relationship_target = InventoryItems)]
pub struct ItemOf(pub Entity);

#[derive(Component, Serialize, Deserialize, Debug, Reflect, PartialEq, Eq, Default)]
#[relationship_target(relationship = ItemOf)]
pub struct InventoryItems(Vec<Entity>);

#[derive(Component, Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ArmorItem {
    pub dodge: f32,
    pub coverage: f32,
    pub slot: ArmorSlot,
}

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum ArmorSlot {
    Arms(Option<ArmorSide>),
    Legs(Option<ArmorSide>),
    Head,
    Torso,
}

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum ArmorSide {
    Left,
    Right,
}

#[derive(Component, Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct WeaponItem {
    pub accuracy: f32,
}

#[derive(Component, Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ShieldItem {
    pub block: f32,
}

#[derive(Component, Reflect, Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct ItemProperties {
    pub id: String,
    pub weight: f32,
}

#[derive(
    Component,
    Reflect,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    EnumCount,
    VariantArray,
    Display,
)]
pub enum EquipSlot {
    HoldingLeft,
    HoldingRight,
    ArmorLeftArm,
    ArmorRightArm,
    ArmorLeftLeg,
    ArmorRightLeg,
    ArmorHead,
    ArmorTorso,
}

impl EquipSlot {
    pub fn slots() -> &'static [Self] {
        Self::VARIANTS
    }

    pub fn count() -> usize {
        Self::COUNT
    }
}
