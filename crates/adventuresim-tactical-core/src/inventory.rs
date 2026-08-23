use std::num::NonZeroU32;

use adventuresim_core::{
    body::{BodyPart, BodySide},
    equipment::MeleeAttackStyle,
    item_catalog::{EquipmentChannel, EquipmentLocation},
    prelude::PlayerEquipment,
};
use avian3d::prelude::LayerMask;
use bevy::{
    ecs::{
        entity::MapEntities, lifecycle::HookContext, query::QueryData, system::SystemParam,
        world::DeferredWorld,
    },
    prelude::*,
};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumCount, VariantArray};

use crate::animation::AttackHand;

pub const TACTICAL_TERRAIN_LAYER: LayerMask = LayerMask(1 << 5);
pub const TACTICAL_ITEM_LAYER: LayerMask = LayerMask(1 << 4);

#[derive(Component, Serialize, Deserialize, Debug, Reflect, PartialEq, Eq, Deref, DerefMut)]
#[reflect(Component)]
pub struct ItemQuantity(pub NonZeroU32);

impl Default for ItemQuantity {
    fn default() -> Self {
        Self(NonZeroU32::new(1).unwrap())
    }
}

#[derive(Component, Serialize, Deserialize, Debug, Reflect, PartialEq, Eq, Clone, MapEntities)]
#[reflect(Component)]
#[require(ItemProperties, ItemQuantity)]
#[relationship(relationship_target = InventoryItems)]
pub struct ItemOf(#[entities] pub Entity);

/// Reflectable so world dumps capture it alongside `ItemOf`, mirroring how
/// bevy's own `ChildOf`/`Children` pair works with dynamic scenes: a scene
/// carries *both* sides of a relationship and restores them verbatim
/// (entity-mapped), which is exactly why `DynamicScene::write_to_world`
/// silences relationship hooks. Capturing only the `ItemOf` side leaves
/// loaded owners with no inventory at all - and every after-the-fact repair
/// of that gap has proven fragile (a non-idempotent rebuild once wiped a
/// bot's whole inventory on client join).
#[derive(Component, Serialize, Deserialize, Debug, Reflect, PartialEq, Eq, Default)]
#[reflect(Component)]
#[relationship_target(relationship = ItemOf)]
pub struct InventoryItems {
    #[relationship]
    items: Vec<Entity>,
    #[entities]
    holding_weapon: Option<Entity>,
    #[entities]
    holding_shield: Option<Entity>,
}

impl InventoryItems {
    pub fn holding_weapon(&self) -> Option<Entity> {
        self.holding_weapon
    }

    pub fn holding_shield(&self) -> Option<Entity> {
        self.holding_shield
    }
}

#[derive(Component, Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[reflect(Component)]
pub struct ArmorItem {
    pub range_of_motion: f32,
    pub coverage: f32,
    pub slot: ArmorSlot,
    pub resistance: f32,
    pub padding: f32,
    pub flexibility: f32,
    pub covered_parts: [bool; 7],
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
#[reflect(Component)]
pub struct WeaponItem {
    pub skill_weights: [f32; 9],
    pub accuracy: f32,
    pub swing_precision: f32,
    pub stab_precision: f32,
    pub prefers_stab: bool,
    pub penetration: f32,
    pub reach: f32,
    pub balance: f32,
    pub precise: bool,
    pub melee: bool,
    pub ranged: bool,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    /// Real-time seconds between committing to a swing and the hit actually
    /// landing. The single source of truth for this weapon's windup pacing -
    /// see `PlayerEquipment::weapon_windup_secs`.
    pub windup_secs: f32,
    /// Offhand-specific commitment-to-contact time. Offhand attacks use this
    /// even when the same item is faster in the main hand.
    pub offhand_windup_secs: f32,
}

#[derive(Component, Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[reflect(Component)]
pub struct ShieldItem {
    pub block: f32,
}

#[derive(Component, Reflect, Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[reflect(Component)]
pub struct ItemProperties {
    pub id: String,
    pub weight: f32,
}

#[derive(Component, Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq, MapEntities)]
pub struct EquipmentTopology {
    pub placement_id: Option<String>,
    #[entities]
    pub occupancies: Vec<EquipmentTopologyOccupancy>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, MapEntities)]
pub struct EquipmentTopologyOccupancy {
    pub occupancy_id: String,
    #[entities]
    pub anchor: TacticalEquipmentAnchor,
    pub channel: EquipmentChannel,
    pub order: u16,
    pub requirement_index: u16,
    pub capacity_index: u16,
}

/// Recomputes the legacy combat hand caches after an ownership root is
/// rebound. Relationship hooks maintain the item list, but changing `ItemOf`
/// alone does not rerun the `EquipSlot` hooks that populate these caches.
pub fn rebuild_inventory_holding_cache(world: &mut World, root: Entity) {
    let mut weapon = None;
    let mut shield = None;
    let mut query = world.query::<(
        Entity,
        &ItemOf,
        &EquipSlot,
        Has<WeaponItem>,
        Has<ShieldItem>,
    )>();
    for (entity, owner, slot, is_weapon, is_shield) in query.iter(world) {
        if owner.0 != root || !matches!(slot, EquipSlot::HoldingLeft | EquipSlot::HoldingRight) {
            continue;
        }
        if is_weapon {
            weapon = Some(entity);
        }
        if is_shield {
            shield = Some(entity);
        }
    }
    if let Some(mut inventory) = world.get_mut::<InventoryItems>(root) {
        inventory.holding_weapon = weapon;
        inventory.holding_shield = shield;
    }
}

/// Tactical topology never exposes durable inventory row IDs to a client.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, MapEntities)]
pub enum TacticalEquipmentAnchor {
    CharacterLocation(EquipmentLocation),
    ItemAttachment {
        #[entities]
        parent: Entity,
        attachment_point_id: String,
    },
}

#[derive(Component, Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct EquipmentPhysical {
    pub dimensions_m: Vec3,
    pub grip_to_tip_m: f32,
    pub anchor_offset_m: Vec3,
}

/// Immutable, authoritative procedural appearance for a smith-made weapon.
///
/// The recipe uses the versioned `adventuresim-weapon-model` postcard wire
/// format. Keeping the transport opaque here avoids coupling tactical combat
/// state to render-only mesh types; the client validates and expands it into
/// geometry, while the server continues to use [`EquipmentPhysical`] as its
/// conservative interaction/collision proxy.
#[derive(Component, Reflect, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WeaponAppearance {
    pub generator_version: u16,
    pub design_hash: [u8; 32],
    pub recipe: Vec<u8>,
}

/// Immutable recipe used to derive a fitted sheath, scabbard, or haft loop.
/// This is attached to the holder entity, while [`WeaponAppearance`] remains
/// attached to the contained weapon entity.
#[derive(Component, Reflect, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WeaponHolderAppearance {
    pub generator_version: u16,
    pub design_hash: [u8; 32],
    pub recipe: Vec<u8>,
}

impl EquipmentPhysical {
    pub fn is_valid(self) -> bool {
        self.dimensions_m.is_finite()
            && self.dimensions_m.cmpgt(Vec3::ZERO).all()
            && self.grip_to_tip_m.is_finite()
            && self.grip_to_tip_m >= 0.0
            && self.anchor_offset_m.is_finite()
    }
}

#[derive(Component, Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TacticalSceneItem;

/// Replicated optimistic-concurrency token for tactical equipment actions.
/// The authority increments it after every accepted mutation.
#[derive(
    Component, Reflect, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq,
)]
pub struct EquipmentActionState {
    pub revision: u32,
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
#[reflect(Component)]
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
            attack_hand: AttackHand::Main,
        }
    }

    pub fn get_for_attack(
        &self,
        entity: Entity,
        attack_hand: AttackHand,
    ) -> InventoryView<'_, '_, '_> {
        InventoryView {
            entity,
            q_inventory: &self.q_inventory,
            q_item: &self.q_item,
            attack_hand,
        }
    }
}

pub struct InventoryView<'v, 'w, 's> {
    entity: Entity,
    q_inventory: &'v Query<'w, 's, &'static InventoryItems>,
    q_item: &'v Query<'w, 's, ItemQuery>,
    attack_hand: AttackHand,
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

    /// Returns whether this actor owns a non-empty stack of the requested
    /// tactical item without scanning other actors' inventories.
    pub fn has_item_id(&self, item_id: &str) -> bool {
        self.iter()
            .any(|item| item.properties.id == item_id && item.quantity.0.get() > 0)
    }

    fn striking_item(&self) -> Option<ItemQueryItem<'_, '_>> {
        let slot = match self.attack_hand {
            AttackHand::Main => EquipSlot::HoldingRight,
            AttackHand::Offhand => EquipSlot::HoldingLeft,
        };
        self.iter().find(|item| item.slot == Some(&slot))
    }

    fn equipped_weapon(&self) -> Option<ItemQueryItem<'_, '_>> {
        self.striking_item().filter(|item| item.weapon.is_some())
    }

    pub fn has_equipped_weapon(&self) -> bool {
        self.equipped_weapon().is_some()
    }

    pub fn has_striking_item(&self) -> bool {
        self.striking_item().is_some()
    }

    fn equipped_shield(&self) -> Option<ItemQueryItem<'_, '_>> {
        self.q_inventory
            .get(self.entity)
            .ok()
            .and_then(|inventory| inventory.holding_shield)
            .and_then(|weapon| self.q_item.get(weapon).ok())
    }

    fn layered_armor_for(&self, part: BodyPart) -> adventuresim_core::equipment::LayeredArmor {
        fold_armor_layers(
            body_part_index(part),
            self.iter().filter_map(|item| item.armor),
        )
    }
}

fn fold_armor_layers<'a>(
    index: usize,
    armor: impl IntoIterator<Item = &'a ArmorItem>,
) -> adventuresim_core::equipment::LayeredArmor {
    let mut result = adventuresim_core::equipment::LayeredArmor {
        range_of_motion: 1.0,
        ..Default::default()
    };
    let mut weighted_flexibility = 0.0;
    for armor in armor.into_iter().filter(|armor| armor.covered_parts[index]) {
        result.coverage = 1.0 - (1.0 - result.coverage) * (1.0 - armor.coverage.clamp(0.0, 1.0));
        let resistance = armor.resistance.max(0.0);
        result.resistance += resistance;
        result.padding += armor.padding.max(0.0);
        weighted_flexibility += armor.flexibility.clamp(0.0, 1.0) * resistance;
        result.range_of_motion = result
            .range_of_motion
            .min(armor.range_of_motion.clamp(0.0, 1.0));
    }
    result.flexibility = if result.resistance > f32::EPSILON {
        weighted_flexibility / result.resistance
    } else {
        0.0
    };
    result
}

impl PlayerEquipment for InventoryView<'_, '_, '_> {
    fn weapon_skill_distribution(&self) -> adventuresim_core::equipment::WeaponSkillDistribution {
        let Some(w) = self
            .equipped_weapon()
            .and_then(|item| item.weapon)
            .map(|weapon| weapon.skill_weights)
        else {
            return adventuresim_core::equipment::WeaponSkillDistribution::UNARMED;
        };
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

    fn weapon_swing_precision(&self) -> f32 {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .map(|weapon| weapon.swing_precision)
            .unwrap_or(adventuresim_core::combat::UNARMED_SWING_PRECISION)
    }

    fn weapon_stab_precision(&self) -> f32 {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .map(|weapon| weapon.stab_precision)
            .unwrap_or(adventuresim_core::combat::UNARMED_STAB_PRECISION)
    }

    fn weapon_preferred_melee_style(&self) -> MeleeAttackStyle {
        if self
            .equipped_weapon()
            .and_then(|item| item.weapon)
            .is_some_and(|weapon| weapon.prefers_stab)
        {
            MeleeAttackStyle::Stab
        } else {
            MeleeAttackStyle::Swing
        }
    }

    fn weapon_is_melee(&self) -> bool {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .is_none_or(|weapon| weapon.melee)
    }

    fn weapon_is_ranged(&self) -> bool {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .is_some_and(|weapon| weapon.ranged)
    }

    fn weapon_is_unarmed(&self) -> bool {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .is_none()
    }

    fn weapon_does_blunt(&self) -> bool {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .is_none_or(|weapon| weapon.blunt)
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
        Some(match self.attack_hand {
            AttackHand::Main => BodySide::Right,
            AttackHand::Offhand => BodySide::Left,
        })
    }

    fn weapon_reach(&self) -> f32 {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .map(|weapon| weapon.reach)
            .unwrap_or(crate::combat::HANDS_REACH)
    }

    fn weapon_windup_secs(&self) -> f32 {
        self.equipped_weapon()
            .and_then(|item| item.weapon)
            .map(|weapon| match self.attack_hand {
                AttackHand::Main => weapon.windup_secs,
                AttackHand::Offhand => weapon.offhand_windup_secs,
            })
            .unwrap_or(match self.attack_hand {
                AttackHand::Main => crate::combat::HANDS_WINDUP_SECS,
                AttackHand::Offhand if self.striking_item().is_some() => 0.38,
                AttackHand::Offhand => 0.24,
            })
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
        self.layered_armor_for(part).range_of_motion
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
        self.striking_item()
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
        self.layered_armor_for(part).resistance
    }

    fn armor_padding(&self, part: BodyPart) -> f32 {
        self.layered_armor_for(part).padding
    }

    fn armor_flexibility(&self, part: BodyPart) -> f32 {
        self.layered_armor_for(part).flexibility
    }

    fn armor_coverage(&self, part: BodyPart) -> f32 {
        self.layered_armor_for(part).coverage
    }
}

fn body_part_index(part: BodyPart) -> usize {
    match part {
        BodyPart::LeftArm => 0,
        BodyPart::RightArm => 1,
        BodyPart::LeftLeg => 2,
        BodyPart::RightLeg => 3,
        BodyPart::Chest => 4,
        BodyPart::Stomach => 5,
        BodyPart::Head => 6,
    }
}

/// Requires `ItemOf` to already be on the entity: single-component inserts
/// in every live equip path place `ItemOf` first, and batched inserts
/// (spawn bundles, replication) run hooks only after the whole batch has
/// landed. The one path where neither holds - `bevy_scene` applying
/// components one at a time in an order that puts `EquipSlot` before
/// `ItemOf` - doesn't need this hook at all, because scenes capture and
/// restore `InventoryItems` (holding fields included) as data.
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::SystemState;

    #[test]
    fn unarmed_combat_has_authored_windup_timing() {
        let mut world = World::new();
        let owner = world.spawn(InventoryItems::default()).id();
        let mut viewer = SystemState::<InventoryViewer>::new(&mut world);
        let inventory = viewer.get(&world).unwrap();

        assert_eq!(
            inventory.get(owner).weapon_windup_secs(),
            crate::combat::HANDS_WINDUP_SECS
        );
        assert_eq!(
            inventory.get(owner).weapon_preferred_melee_style(),
            MeleeAttackStyle::Swing
        );
        assert_eq!(
            inventory.get(owner).weapon_skill_distribution(),
            adventuresim_core::equipment::WeaponSkillDistribution::UNARMED
        );
        assert_eq!(
            inventory.get(owner).weapon_swing_precision(),
            adventuresim_core::combat::UNARMED_SWING_PRECISION
        );
        assert_eq!(
            inventory.get(owner).weapon_stab_precision(),
            adventuresim_core::combat::UNARMED_STAB_PRECISION
        );
    }

    #[test]
    fn rebuilding_holding_cache_preserves_weapon_and_shield_after_owner_rebind() {
        let mut world = World::new();
        let owner = world.spawn(InventoryItems::default()).id();
        let weapon = world
            .spawn((
                ItemOf(owner),
                EquipSlot::HoldingRight,
                WeaponItem {
                    skill_weights: [0.0; 9],
                    accuracy: 0.0,
                    swing_precision: 0.0,
                    stab_precision: 0.0,
                    prefers_stab: false,
                    penetration: 0.0,
                    reach: 1.0,
                    balance: 0.0,
                    precise: false,
                    melee: true,
                    ranged: false,
                    blunt: false,
                    slash: true,
                    pierce: false,
                    windup_secs: 0.0,
                    offhand_windup_secs: 0.34,
                },
            ))
            .id();
        let shield = world
            .spawn((
                ItemOf(owner),
                EquipSlot::HoldingLeft,
                ShieldItem { block: 1.0 },
            ))
            .id();
        {
            let mut inventory = world.get_mut::<InventoryItems>(owner).unwrap();
            inventory.holding_weapon = None;
            inventory.holding_shield = None;
        }
        rebuild_inventory_holding_cache(&mut world, owner);
        let inventory = world.get::<InventoryItems>(owner).unwrap();
        assert_eq!(inventory.holding_weapon, Some(weapon));
        assert_eq!(inventory.holding_shield, Some(shield));
    }

    #[test]
    fn tactical_handoff_folds_multiple_layers_once_per_inventory_item() {
        let inner = ArmorItem {
            range_of_motion: 0.9,
            coverage: 0.5,
            slot: ArmorSlot::Chest,
            resistance: 20.0,
            padding: 30.0,
            flexibility: 0.8,
            covered_parts: [false, false, false, false, true, false, false],
        };
        let outer = ArmorItem {
            range_of_motion: 0.7,
            coverage: 0.8,
            slot: ArmorSlot::Chest,
            resistance: 100.0,
            padding: 10.0,
            flexibility: 0.2,
            covered_parts: [false, false, false, false, true, false, false],
        };
        let armor = fold_armor_layers(4, [&inner, &outer]);
        assert_eq!(armor.resistance, 120.0);
        assert_eq!(armor.padding, 40.0);
        assert!((armor.coverage - 0.9).abs() < 0.0001);
        assert_eq!(armor.range_of_motion, 0.7);
        assert!((armor.flexibility - 0.3).abs() < 0.0001);
    }

    #[test]
    fn attached_protection_and_multi_location_projection_do_not_require_legacy_slots() {
        let attached = ArmorItem {
            range_of_motion: 1.0,
            coverage: 0.4,
            slot: ArmorSlot::Arms(None),
            resistance: 12.0,
            padding: 3.0,
            flexibility: 0.75,
            covered_parts: [true, true, false, false, false, false, false],
        };
        assert_eq!(fold_armor_layers(0, [&attached]).resistance, 12.0);
        assert_eq!(fold_armor_layers(1, [&attached]).resistance, 12.0);
        assert_eq!(fold_armor_layers(4, [&attached]).resistance, 0.0);
    }

    #[test]
    fn topology_retains_stable_placement_and_attachment_edge_identity() {
        let topology = EquipmentTopology {
            placement_id: Some("double_strap".into()),
            occupancies: vec![
                EquipmentTopologyOccupancy {
                    occupancy_id: "character:1:Chest:60:0".into(),
                    anchor: TacticalEquipmentAnchor::CharacterLocation(EquipmentLocation::Chest),
                    channel: EquipmentChannel::Accessory,
                    order: 0,
                    requirement_index: 0,
                    capacity_index: 0,
                },
                EquipmentTopologyOccupancy {
                    occupancy_id: "item:41:left:0".into(),
                    anchor: TacticalEquipmentAnchor::ItemAttachment {
                        parent: Entity::from_bits(41),
                        attachment_point_id: "left".into(),
                    },
                    channel: EquipmentChannel::Mount,
                    order: 0,
                    requirement_index: 0,
                    capacity_index: 0,
                },
                EquipmentTopologyOccupancy {
                    occupancy_id: "item:42:right:0".into(),
                    anchor: TacticalEquipmentAnchor::ItemAttachment {
                        parent: Entity::from_bits(42),
                        attachment_point_id: "right".into(),
                    },
                    channel: EquipmentChannel::Mount,
                    order: 1,
                    requirement_index: 1,
                    capacity_index: 0,
                },
            ],
        };
        assert_eq!(topology.placement_id.as_deref(), Some("double_strap"));
        assert_eq!(topology.occupancies.len(), 3);
        assert!(matches!(
            topology.occupancies[1].anchor,
            TacticalEquipmentAnchor::ItemAttachment { .. }
        ));
        assert_eq!(topology.occupancies[2].requirement_index, 1);
    }

    #[test]
    fn topology_maps_nested_attachment_parent_entities() {
        let parent = Entity::from_bits(41);
        let mapped_parent = Entity::from_bits(141);
        let mut topology = EquipmentTopology {
            placement_id: Some("attached".into()),
            occupancies: vec![EquipmentTopologyOccupancy {
                occupancy_id: "item:41:left:0".into(),
                anchor: TacticalEquipmentAnchor::ItemAttachment {
                    parent,
                    attachment_point_id: "left".into(),
                },
                channel: EquipmentChannel::Mount,
                order: 0,
                requirement_index: 0,
                capacity_index: 0,
            }],
        };

        topology.map_entities(&mut (parent, mapped_parent));

        assert!(matches!(
            topology.occupancies[0].anchor,
            TacticalEquipmentAnchor::ItemAttachment { parent: found, .. }
                if found == mapped_parent
        ));
    }
}
