use std::num::NonZeroU32;

use adventuresim_core::{body::BodyPart, prelude::PlayerEquipment};
use bevy::{
    ecs::{entity::MapEntities, query::QueryData, system::SystemParam},
    prelude::*,
};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumCount, VariantArray};

#[derive(Component, Serialize, Deserialize, Debug, Reflect, PartialEq, Eq, Deref, DerefMut)]
pub struct ItemQuantity(pub NonZeroU32);

impl Default for ItemQuantity {
    fn default() -> Self {
        Self(NonZeroU32::new(1).unwrap())
    }
}

#[derive(Component, Serialize, Deserialize, Debug, Reflect, PartialEq, Eq, Clone, MapEntities)]
#[require(ItemProperties, ItemQuantity)]
#[relationship(relationship_target = InventoryItems)]
pub struct ItemOf(#[entities] pub Entity);

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
    Chest,
    Stomach,
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
    ArmorChest,
    ArmorStomach,
}

impl EquipSlot {
    pub fn slots() -> &'static [Self] {
        Self::VARIANTS
    }

    pub fn count() -> usize {
        Self::COUNT
    }

    pub fn from_armor_body_part(part: BodyPart) -> Self {
        bitflags::bitflags_match!(part, {
            BodyPart::LeftArm => Self::ArmorLeftArm,
            BodyPart::RightArm => Self::ArmorRightArm,
            BodyPart::LeftLeg => Self::ArmorLeftLeg,
            BodyPart::RightLeg => Self::ArmorRightLeg,
            BodyPart::Chest => Self::ArmorChest,
            BodyPart::Stomach => Self::ArmorStomach,
            BodyPart::Head => Self::ArmorHead,
            _ => unimplemented!()
        })
    }
}

#[derive(QueryData)]
pub struct ItemQuery {
    pub entity: Entity,
    pub quantity: &'static ItemQuantity,
    pub properties: &'static ItemProperties,
    pub item_of: Option<&'static ItemOf>,
    pub slot: Option<&'static EquipSlot>,
    pub armor: Option<&'static ArmorItem>,
    pub weapon: Option<&'static WeaponItem>,
    pub shield: Option<&'static ShieldItem>,
}

#[derive(SystemParam)]
pub struct InventoryViewer<'w, 's> {
    q_inventory: Query<'w, 's, &'static InventoryItems>,
    q_item: Query<'w, 's, ItemQuery>,
}

impl InventoryViewer<'_, '_> {
    pub fn get(&self, entity: Entity) -> InventoryView<'_, '_, '_> {
        InventoryView {
            entity,
            q_inventory: &self.q_inventory,
            q_item: &self.q_item,
        }
    }
}

pub struct InventoryView<'v, 'w, 's> {
    entity: Entity,
    q_inventory: &'v Query<'w, 's, &'static InventoryItems>,
    q_item: &'v Query<'w, 's, ItemQuery>,
}

impl InventoryView<'_, '_, '_> {
    fn iter(&self) -> impl Iterator<Item = ItemQueryItem<'_, '_>> + use<'_> {
        let items = self
            .q_inventory
            .get(self.entity)
            .into_iter()
            .flat_map(|inv| inv.iter());

        self.q_item.iter_many(items)
    }

    fn equipped_weapon(&self) -> Option<ItemQueryItem<'_, '_>> {
        self.iter().find(|item| {
            matches!(
                item,
                ItemQueryItem {
                    slot: Some(EquipSlot::HoldingLeft | EquipSlot::HoldingRight),
                    weapon: Some(..),
                    ..
                }
            )
        })
    }

    fn equipped_shield(&self) -> Option<ItemQueryItem<'_, '_>> {
        self.iter().find(|item| {
            matches!(
                item,
                ItemQueryItem {
                    slot: Some(EquipSlot::HoldingLeft | EquipSlot::HoldingRight),
                    shield: Some(..),
                    ..
                }
            )
        })
    }

    fn equipped_armor_for(&self, slot: EquipSlot) -> Option<ItemQueryItem<'_, '_>> {
        self.iter().find(|item| {
            matches!(
                item,
                &ItemQueryItem {
                    slot: Some(&other_slot),
                    armor: Some(..),
                    ..
                } if other_slot == slot
            )
        })
    }
}

impl PlayerEquipment for InventoryView<'_, '_, '_> {
    fn weapon_accuracy(&self) -> f32 {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .map(|weapon| weapon.accuracy)
            .unwrap_or_default()
    }

    fn armor_dodge(&self, part: BodyPart) -> f32 {
        self.equipped_armor_for(EquipSlot::from_armor_body_part(part))
            .and_then(|item| item.armor)
            .map(|armor| armor.dodge)
            .unwrap_or_default()
    }

    fn inventory_weight(&self) -> f32 {
        self.iter().map(|item| item.properties.weight).sum()
    }

    fn shield_block_bonus(&self) -> f32 {
        self.equipped_shield()
            .and_then(|item| item.shield)
            .map(|shield| shield.block)
            .unwrap_or_default()
    }
}
