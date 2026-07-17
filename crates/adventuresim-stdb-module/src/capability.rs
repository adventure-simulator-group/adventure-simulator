use adventuresim_core::prelude::*;
use spacetimedb::{ReducerContext, Table, reducer, table};

use crate::item::item as _;
use crate::{
    CharacterAttributes, CharacterEquip, CharacterLimbs, CharacterSkills, CharacterStats,
    InventoryItem, Item, ItemKind, character_attributes, character_equip, character_limbs,
    character_skills, character_stats, inventory_item,
};

#[derive(Clone, Debug, PartialEq)]
#[table(name = character_capability, public)]
pub struct CharacterCapability {
    #[primary_key]
    pub character_id: u64,
    pub melee: bool,
    pub ranged: bool,
    pub precise: bool,
    pub heavy: bool,
    pub quarter_armor: bool,
    pub half_armor: bool,
    pub three_quarter_armor: bool,
    pub full_armor: bool,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    pub athletics: f32,
    pub endurance: f32,
    pub medicine: f32,
    pub surgery: f32,
    pub charisma: f32,
    pub faith: f32,
    #[default(0.0)]
    pub weapon_precision: f32,
}

impl From<(u64, CharacterCapabilities)> for CharacterCapability {
    fn from((character_id, value): (u64, CharacterCapabilities)) -> Self {
        Self {
            character_id,
            melee: value.melee,
            ranged: value.ranged,
            precise: value.weapon_precision
                >= adventuresim_core::capability::WEAPON_PRECISION_RAPIER,
            heavy: value.heavy,
            quarter_armor: value.quarter_armor,
            half_armor: value.half_armor,
            three_quarter_armor: value.three_quarter_armor,
            full_armor: value.full_armor,
            blunt: false,
            slash: false,
            pierce: false,
            athletics: value.athletics,
            endurance: value.endurance,
            medicine: value.medicine,
            surgery: value.surgery,
            charisma: value.charisma,
            faith: value.faith,
            weapon_precision: value.weapon_precision,
        }
    }
}

pub fn evaluate_character(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<CharacterCapabilities, String> {
    let attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .ok_or("Character attributes not found")?;
    let skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?;
    let equip = ctx
        .db
        .character_equip()
        .character_id()
        .find(character_id)
        .ok_or("Character equipment not found")?;
    let body = ctx
        .db
        .character_limbs()
        .character_id()
        .find(character_id)
        .ok_or("Character limbs not found")?;
    let essentials = ctx
        .db
        .character_stats()
        .character_id()
        .find(character_id)
        .ok_or("Character stats not found")?;
    let equipment = StrategicEquipment::load(ctx, character_id, &equip);
    Ok(evaluate_capabilities(
        &attributes,
        &body,
        &essentials,
        &equipment,
        &skills,
    ))
}

pub fn refresh_character_capability(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<CharacterCapabilities, String> {
    let capabilities = evaluate_character(ctx, character_id)?;
    let row = CharacterCapability::from((character_id, capabilities));
    if let Some(existing) = ctx
        .db
        .character_capability()
        .character_id()
        .find(character_id)
    {
        // Capability reads are currently refreshed lazily by the web layer. Avoid
        // emitting a table update when the derived value has not changed: that
        // update invalidates the SSE UI, which otherwise refreshes the same
        // capability again and creates a feedback loop.
        if existing != row {
            ctx.db.character_capability().character_id().update(row);
        }
    } else {
        ctx.db.character_capability().insert(row);
    }
    Ok(capabilities)
}

#[reducer]
pub fn refresh_capabilities(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    refresh_character_capability(ctx, character_id).map(|_| ())
}

impl PlayerBody for CharacterLimbs {
    fn body_part_health(&self, part: BodyPart) -> f32 {
        match part {
            BodyPart::LeftArm => self.left_arm_health,
            BodyPart::RightArm => self.right_arm_health,
            BodyPart::LeftLeg => self.left_leg_health,
            BodyPart::RightLeg => self.right_leg_health,
            BodyPart::Chest => self.chest_health,
            BodyPart::Stomach => self.stomach_health,
            BodyPart::Head => self.head_health,
        }
    }

    fn body_weight(&self) -> f32 {
        70.0
    }

    fn primary_side(&self) -> BodySide {
        BodySide::Right
    }
}

impl PlayerEssentials for CharacterStats {
    fn calories_used_today(&self) -> f32 {
        self.calories_used
    }

    fn focus_level(&self) -> f32 {
        self.focus
    }
}

impl PlayerAttributes for CharacterAttributes {
    fn raw_limb_attr(&self, attr: LimbAttribute, limb: BodyPart) -> f32 {
        match (attr, limb) {
            (LimbAttribute::Strength, BodyPart::LeftArm) => self.left_arm_strength,
            (LimbAttribute::Strength, BodyPart::RightArm) => self.right_arm_strength,
            (LimbAttribute::Strength, BodyPart::LeftLeg) => self.left_leg_strength,
            (LimbAttribute::Strength, BodyPart::RightLeg) => self.right_leg_strength,
            (LimbAttribute::Agility, BodyPart::LeftArm) => self.left_arm_agility,
            (LimbAttribute::Agility, BodyPart::RightArm) => self.right_arm_agility,
            (LimbAttribute::Agility, BodyPart::LeftLeg) => self.left_leg_agility,
            (LimbAttribute::Agility, BodyPart::RightLeg) => self.right_leg_agility,
            _ => 0.0,
        }
    }

    fn raw_single_body_part_attr(&self, attr: SimpleAttribute) -> f32 {
        match attr {
            SimpleAttribute::Endurance => self.endurance,
            SimpleAttribute::Immunity => self.immunity,
            SimpleAttribute::Gut => self.gut,
            SimpleAttribute::Intelligence => self.intelligence,
            SimpleAttribute::Instinct => self.instinct,
            SimpleAttribute::Eyesight => self.eyesight,
            SimpleAttribute::Hearing => self.hearing,
        }
    }
}

impl PlayerSkills for CharacterSkills {
    fn skill_hours_trained(&self, skill: Skill) -> f32 {
        match skill {
            Skill::Melee => self.melee_hours,
            Skill::Block => self.block_hours,
            Skill::Dodge => self.dodge_hours,
            Skill::Ranged => self.ranged_hours,
            Skill::Will => self.will_hours,
            Skill::Charisma => self.charisma_hours,
            Skill::Medicine => self.medicine_hours,
            Skill::Faith => self.faith_hours,
            Skill::Stealth => self.stealth_hours,
            Skill::Balance => self.balance_hours,
            Skill::Surgeon => self.surgeon_hours,
        }
    }
}

pub(crate) struct StrategicEquipment {
    weapon: Option<Item>,
    shield: Option<Item>,
    armor: [Option<Item>; 7],
    inventory_weight: f32,
}

impl StrategicEquipment {
    pub(crate) fn load(ctx: &ReducerContext, character_id: u64, equip: &CharacterEquip) -> Self {
        let definition = |inventory_id: Option<u64>| {
            inventory_id
                .and_then(|id| ctx.db.inventory_item().id().find(id))
                .and_then(|inventory| ctx.db.item().id().find(&inventory.item_id))
        };
        let hands = [
            definition(equip.left_hand_item_id),
            definition(equip.right_hand_item_id),
        ];
        let weapon = hands
            .iter()
            .flatten()
            .find(|item| item.kind == ItemKind::Weapon)
            .cloned();
        let shield = hands
            .iter()
            .flatten()
            .find(|item| item.kind == ItemKind::Shield)
            .cloned();
        let armor = [
            definition(equip.left_arm_armor_id),
            definition(equip.right_arm_armor_id),
            definition(equip.left_leg_armor_id),
            definition(equip.right_leg_armor_id),
            definition(equip.chest_armor_id),
            definition(equip.stomach_armor_id),
            definition(equip.head_armor_id),
        ];
        let inventory_weight = ctx
            .db
            .inventory_item()
            .character_id()
            .filter(character_id)
            .filter_map(|inventory: InventoryItem| {
                ctx.db
                    .item()
                    .id()
                    .find(&inventory.item_id)
                    .map(|item| item.weight * inventory.quantity as f32)
            })
            .sum();
        Self {
            weapon,
            shield,
            armor,
            inventory_weight,
        }
    }

    fn armor_for(&self, part: BodyPart) -> Option<&Item> {
        let index = match part {
            BodyPart::LeftArm => 0,
            BodyPart::RightArm => 1,
            BodyPart::LeftLeg => 2,
            BodyPart::RightLeg => 3,
            BodyPart::Chest => 4,
            BodyPart::Stomach => 5,
            BodyPart::Head => 6,
        };
        self.armor[index].as_ref()
    }
}

impl PlayerEquipment for StrategicEquipment {
    fn weapon_is_melee(&self) -> bool {
        self.weapon.as_ref().is_some_and(|item| item.melee)
    }
    fn weapon_is_ranged(&self) -> bool {
        self.weapon.as_ref().is_some_and(|item| item.ranged)
    }
    fn weapon_does_blunt(&self) -> bool {
        self.weapon.as_ref().is_some_and(|item| item.blunt)
    }
    fn weapon_does_slash(&self) -> bool {
        self.weapon.as_ref().is_some_and(|item| item.slash)
    }
    fn weapon_does_pierce(&self) -> bool {
        self.weapon.as_ref().is_some_and(|item| item.pierce)
    }
    fn weapon_accuracy(&self) -> f32 {
        self.weapon.as_ref().map_or(0.0, |item| item.accuracy)
    }
    fn weapon_weight(&self) -> f32 {
        self.weapon.as_ref().map_or(0.0, |item| item.weight)
    }
    fn weapon_penetration(&self) -> f32 {
        self.weapon.as_ref().map_or(0.0, |item| item.penetration)
    }
    fn weapon_reach(&self) -> f32 {
        self.weapon.as_ref().map_or(0.0, |item| item.reach)
    }
    fn weapon_holding_side(&self) -> Option<BodySide> {
        Some(BodySide::Right)
    }
    fn weapon_is_precise(&self) -> bool {
        self.weapon.as_ref().is_some_and(|item| item.precise)
    }
    fn weapon_balance(&self) -> f32 {
        self.weapon.as_ref().map_or(0.0, |item| item.balance)
    }
    fn shield_block_bonus(&self) -> f32 {
        self.shield.as_ref().map_or(0.0, |item| item.block)
    }
    fn armor_resistance(&self, part: BodyPart) -> f32 {
        self.armor_for(part).map_or(0.0, |item| item.resistance)
    }
    fn armor_padding(&self, part: BodyPart) -> f32 {
        self.armor_for(part).map_or(0.0, |item| item.padding)
    }
    fn armor_flexibility(&self, part: BodyPart) -> f32 {
        self.armor_for(part).map_or(1.0, |item| item.flexibility)
    }
    fn armor_range_of_motion(&self, part: BodyPart) -> f32 {
        self.armor_for(part)
            .map_or(1.0, |item| item.range_of_motion)
    }
    fn armor_coverage(&self, part: BodyPart) -> f32 {
        self.armor_for(part).map_or(0.0, |item| item.coverage)
    }
    fn inventory_weight(&self) -> f32 {
        self.inventory_weight
    }
}
