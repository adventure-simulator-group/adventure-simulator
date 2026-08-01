use std::num::NonZeroU32;

use adventuresim_core::{
    body::{BodyPart, BodySide},
    prelude::PlayerEquipment,
};
use bevy::{
    ecs::{
        entity::MapEntities, lifecycle::HookContext, query::QueryData, system::SystemParam,
        world::DeferredWorld,
    },
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
pub struct InventoryItems {
    #[relationship]
    items: Vec<Entity>,
    holding_weapon: Option<Entity>,
    holding_shield: Option<Entity>,
}

#[derive(Component, Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ArmorItem {
    pub range_of_motion: f32,
    pub coverage: f32,
    pub slot: ArmorSlot,
    pub resistance: f32,
    pub padding: f32,
    pub flexibility: f32,
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
    pub skill_weights: [f32; 9],
    pub accuracy: f32,
    pub penetration: f32,
    pub reach: f32,
    pub balance: f32,
    pub precise: bool,
    pub melee: bool,
    pub ranged: bool,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
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
#[component(on_insert = on_equip_slot_replaced, on_remove = on_equip_slot_removed)]
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
        match part {
            BodyPart::LeftArm => Self::ArmorLeftArm,
            BodyPart::RightArm => Self::ArmorRightArm,
            BodyPart::LeftLeg => Self::ArmorLeftLeg,
            BodyPart::RightLeg => Self::ArmorRightLeg,
            BodyPart::Chest => Self::ArmorChest,
            BodyPart::Stomach => Self::ArmorStomach,
            BodyPart::Head => Self::ArmorHead,
        }
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
        self.q_inventory
            .get(self.entity)
            .ok()
            .and_then(|inventory| inventory.holding_weapon)
            .and_then(|weapon| self.q_item.get(weapon).ok())
    }

    fn equipped_shield(&self) -> Option<ItemQueryItem<'_, '_>> {
        self.q_inventory
            .get(self.entity)
            .ok()
            .and_then(|inventory| inventory.holding_shield)
            .and_then(|weapon| self.q_item.get(weapon).ok())
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
    fn weapon_skill_distribution(&self) -> adventuresim_core::equipment::WeaponSkillDistribution {
        let w = self
            .equipped_weapon()
            .and_then(|item| item.weapon)
            .map(|weapon| weapon.skill_weights)
            .unwrap_or([0.0; 9]);
        adventuresim_core::equipment::WeaponSkillDistribution {
            polearm: w[0],
            axe: w[1],
            bludgeon: w[2],
            sword: w[3],
            knife: w[4],
            bow: w[5],
            crossbow: w[6],
            firearm: w[7],
            throw: w[8],
        }
    }
    fn weapon_accuracy(&self) -> f32 {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .map(|weapon| weapon.accuracy)
            .unwrap_or_default()
    }

    fn weapon_is_melee(&self) -> bool {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .is_some_and(|weapon| weapon.melee)
    }

    fn weapon_is_ranged(&self) -> bool {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .is_some_and(|weapon| weapon.ranged)
    }

    fn weapon_does_blunt(&self) -> bool {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .is_some_and(|weapon| weapon.blunt)
    }

    fn weapon_does_slash(&self) -> bool {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .is_some_and(|weapon| weapon.slash)
    }

    fn weapon_does_pierce(&self) -> bool {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .is_some_and(|weapon| weapon.pierce)
    }

    fn weapon_holding_side(&self) -> Option<BodySide> {
        self.equipped_weapon()
            .and_then(|item| item.slot)
            .and_then(|slot| match slot {
                EquipSlot::HoldingLeft => Some(BodySide::Left),
                EquipSlot::HoldingRight => Some(BodySide::Right),
                _ => None,
            })
    }

    fn weapon_reach(&self) -> f32 {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .map(|weapon| weapon.reach)
            .unwrap_or_default()
    }

    fn weapon_is_precise(&self) -> bool {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .map(|weapon| weapon.precise)
            .unwrap_or_default()
    }

    fn weapon_balance(&self) -> f32 {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .map(|weapon| weapon.balance)
            .unwrap_or_default()
    }

    fn armor_range_of_motion(&self, part: BodyPart) -> f32 {
        self.equipped_armor_for(EquipSlot::from_armor_body_part(part))
            .and_then(|item| item.armor)
            .map(|armor| armor.range_of_motion)
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

    fn shield_holding_side(&self) -> Option<BodySide> {
        self.equipped_shield()
            .and_then(|item| item.slot)
            .and_then(|slot| match slot {
                EquipSlot::HoldingLeft => Some(BodySide::Left),
                EquipSlot::HoldingRight => Some(BodySide::Right),
                _ => None,
            })
    }

    fn weapon_weight(&self) -> f32 {
        self.equipped_weapon()
            .map(|item| item.properties.weight)
            .unwrap_or_default()
    }

    fn weapon_penetration(&self) -> f32 {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .map(|weapon| weapon.penetration)
            .unwrap_or(1.0)
    }

    fn armor_resistance(&self, part: BodyPart) -> f32 {
        self.equipped_armor_for(EquipSlot::from_armor_body_part(part))
            .and_then(|item| item.armor)
            .map(|armor| armor.resistance)
            .unwrap_or_default()
    }

    fn armor_padding(&self, part: BodyPart) -> f32 {
        self.equipped_armor_for(EquipSlot::from_armor_body_part(part))
            .and_then(|item| item.armor)
            .map(|armor| armor.padding)
            .unwrap_or_default()
    }

    fn armor_flexibility(&self, part: BodyPart) -> f32 {
        self.equipped_armor_for(EquipSlot::from_armor_body_part(part))
            .and_then(|item| item.armor)
            .map(|armor| armor.flexibility)
            .unwrap_or_default()
    }

    fn armor_coverage(&self, part: BodyPart) -> f32 {
        self.equipped_armor_for(EquipSlot::from_armor_body_part(part))
            .and_then(|item| item.armor)
            .map(|armor| armor.coverage)
            .unwrap_or_default()
    }
}

fn on_equip_slot_replaced(mut world: DeferredWorld, ctx: HookContext) {
    match world.get::<EquipSlot>(ctx.entity) {
        Some(EquipSlot::HoldingLeft | EquipSlot::HoldingRight)
            if world.get::<WeaponItem>(ctx.entity).is_some() =>
        {
            let Some(root) = world.get::<ItemOf>(ctx.entity).map(|i| i.0) else {
                return;
            };
            let Some(mut items) = world.get_mut::<InventoryItems>(root) else {
                return;
            };
            items.holding_weapon = Some(ctx.entity);
        }
        Some(EquipSlot::HoldingLeft | EquipSlot::HoldingRight)
            if world.get::<ShieldItem>(ctx.entity).is_some() =>
        {
            let Some(root) = world.get::<ItemOf>(ctx.entity).map(|i| i.0) else {
                return;
            };
            let Some(mut items) = world.get_mut::<InventoryItems>(root) else {
                return;
            };
            items.holding_shield = Some(ctx.entity);
        }
        _ => {}
    }
}

fn on_equip_slot_removed(mut world: DeferredWorld, ctx: HookContext) {
    let Some(root) = world.get::<ItemOf>(ctx.entity).map(|i| i.0) else {
        return;
    };
    let Some(mut items) = world.get_mut::<InventoryItems>(root) else {
        return;
    };

    if items.holding_weapon == Some(ctx.entity) {
        items.holding_weapon = None;
    }
    if items.holding_shield == Some(ctx.entity) {
        items.holding_shield = None;
    }
}
