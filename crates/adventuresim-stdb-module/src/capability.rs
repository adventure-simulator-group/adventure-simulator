use adventuresim_core::autoresolve::CombatProjectileKind;
use adventuresim_core::prelude::*;
use spacetimedb::{ReducerContext, Table, reducer, table};

use crate::condition::{character_condition as _, character_needs as _};
use crate::food::food_lot as _;
use crate::item::item as _;
use crate::repair::item_condition as _;
use crate::{
    CharacterAttributes, CharacterEquip, CharacterLimbs, CharacterSkills, CharacterStats,
    InventoryItem, Item, ItemKind, character_attributes, character_equip, character_limbs,
    character_skills, character_stats, inventory_item,
};

#[derive(Clone, Debug, PartialEq)]
#[table(accessor = character_capability, public)]
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
    pub anatomy: f32,
    pub knife: f32,
    pub tailoring: f32,
    pub surgery: f32,
    pub command: f32,
    pub religion: f32,
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
            anatomy: value.anatomy,
            knife: value.knife,
            tailoring: value.tailoring,
            surgery: value.surgery,
            command: value.command,
            religion: value.religion,
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
    let attributes = crate::disease::effective_attributes(ctx, character_id, attributes)?;
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

    fn raw_precision(&self) -> f32 {
        self.precision
    }

    fn has_dedicated_precision(&self) -> bool {
        true
    }
}

impl PlayerSkills for CharacterSkills {
    fn skill_hours_trained(&self, skill: Skill) -> f32 {
        match skill {
            Skill::Polearm => self.polearm_hours,
            Skill::Axe => self.axe_hours,
            Skill::Bludgeon => self.bludgeon_hours,
            Skill::Sword => self.sword_hours,
            Skill::Knife => self.knife_hours,
            Skill::Block => self.block_hours,
            Skill::Dodge => self.dodge_hours,
            Skill::Bow => self.bow_hours,
            Skill::Crossbow => self.crossbow_hours,
            Skill::Firearm => self.firearm_hours,
            Skill::Throw => self.throw_hours,
            Skill::Will => self.will_hours,
            Skill::Insight => self.insight_hours,
            Skill::SelfAwareness => self.self_awareness_hours,
            Skill::Humor => self.humor_hours,
            Skill::Command => self.command_hours,
            Skill::Deception => self.deception_hours,
            Skill::Seduction => self.seduction_hours,
            Skill::Medicine => self.medicine_hours,
            Skill::Cooking => self.cooking_hours,
            // Generic recruitment/tactical summaries use the character's best-covered
            // tradition. Authoritative religious morale always selects a tradition.
            Skill::Religion => self.religion_hours.maximum_effective(),
            Skill::Bestiary => self.bestiary_hours.maximum_effective(),
            Skill::Stealth => self.stealth_hours,
            Skill::Balance => self.balance_hours,
            Skill::TerrainPlains => self.terrain_plains_hours,
            Skill::TerrainForest => self.terrain_forest_hours,
            Skill::TerrainHills => self.terrain_hills_hours,
            Skill::TerrainUrban => self.terrain_urban_hours,
            Skill::Anatomy => self.anatomy_hours,
            Skill::Tailoring => self.tailoring_hours,
            Skill::Smithing => self.smithing_hours,
        }
    }
}

pub(crate) struct StrategicEquipment {
    hands: [Option<Item>; 2],
    weapon: Option<Item>,
    weapon_side: Option<BodySide>,
    melee_weapon: Option<Item>,
    melee_weapon_inventory_id: Option<u64>,
    melee_weapon_side: Option<BodySide>,
    ranged_weapon: Option<Item>,
    ranged_weapon_inventory_id: Option<u64>,
    ranged_weapon_side: Option<BodySide>,
    ammunition: u32,
    shield: Option<Item>,
    shield_inventory_id: Option<u64>,
    armor: [Option<Item>; 7],
    inventory_weight: f32,
}

impl StrategicEquipment {
    pub(crate) fn load(ctx: &ReducerContext, character_id: u64, equip: &CharacterEquip) -> Self {
        let definition = |inventory_id: Option<u64>| {
            let id = inventory_id?;
            let inventory = ctx.db.inventory_item().id().find(id)?;
            let mut item = ctx.db.item().id().find(&inventory.item_id)?;
            if let Some(condition) = ctx.db.item_condition().inventory_item_id().find(id) {
                let damage = condition.bins();
                item.accuracy = effective_weapon_stat(item.accuracy, damage, item.edge_sensitivity);
                item.penetration =
                    effective_weapon_stat(item.penetration, damage, item.edge_sensitivity * 0.6);
                item.block = effective_weapon_stat(item.block, damage, item.handling_sensitivity);
                item.range_of_motion =
                    effective_handling(item.range_of_motion, damage, item.handling_sensitivity);
                item.resistance = effective_weapon_stat(item.resistance, damage, 0.1);
            }
            Some(item)
        };
        let hands = [
            definition(equip.left_hand_item_id),
            definition(equip.right_hand_item_id),
        ];
        let weapon_index = hands.iter().position(|item| {
            item.as_ref()
                .is_some_and(|item| item.kind == ItemKind::Weapon)
        });
        let weapon = weapon_index.and_then(|index| hands[index].clone());
        let weapon_side = weapon_index.map(|index| {
            if index == 0 {
                BodySide::Left
            } else {
                BodySide::Right
            }
        });
        let shield = hands
            .iter()
            .flatten()
            .find(|item| item.kind == ItemKind::Shield)
            .cloned();
        let shield_inventory_id = hands
            .iter()
            .position(|item| {
                item.as_ref()
                    .is_some_and(|item| item.kind == ItemKind::Shield)
            })
            .and_then(|index| [equip.left_hand_item_id, equip.right_hand_item_id][index]);
        let melee_weapon = hands
            .iter()
            .flatten()
            .find(|item| item.kind == ItemKind::Weapon && item.melee)
            .cloned();
        let melee_weapon_side = hands
            .iter()
            .position(|item| {
                item.as_ref()
                    .is_some_and(|item| item.kind == ItemKind::Weapon && item.melee)
            })
            .map(hand_side);
        let melee_weapon_inventory_id = hands
            .iter()
            .position(|item| {
                item.as_ref()
                    .is_some_and(|item| item.kind == ItemKind::Weapon && item.melee)
            })
            .and_then(|index| [equip.left_hand_item_id, equip.right_hand_item_id][index]);
        let ranged_weapon = hands
            .iter()
            .flatten()
            .find(|item| item.kind == ItemKind::Weapon && item.ranged)
            .cloned();
        let ranged_weapon_side = hands
            .iter()
            .position(|item| {
                item.as_ref()
                    .is_some_and(|item| item.kind == ItemKind::Weapon && item.ranged)
            })
            .map(hand_side);
        let ranged_weapon_inventory_id = hands
            .iter()
            .position(|item| {
                item.as_ref()
                    .is_some_and(|item| item.kind == ItemKind::Weapon && item.ranged)
            })
            .and_then(|index| [equip.left_hand_item_id, equip.right_hand_item_id][index]);
        let ammunition = ctx
            .db
            .inventory_item()
            .character_id()
            .filter(character_id)
            .filter(|inventory| inventory.item_id == "arrow")
            .map(|inventory| inventory.quantity)
            .sum();
        let armor = [
            definition(equip.left_arm_armor_id),
            definition(equip.right_arm_armor_id),
            definition(equip.left_leg_armor_id),
            definition(equip.right_leg_armor_id),
            definition(equip.chest_armor_id),
            definition(equip.stomach_armor_id),
            definition(equip.head_armor_id),
        ];
        let dry_inventory_weight: f32 = ctx
            .db
            .inventory_item()
            .character_id()
            .filter(character_id)
            .filter_map(|inventory: InventoryItem| {
                if let Some(lot) = ctx
                    .db
                    .food_lot()
                    .iter()
                    .find(|lot| lot.inventory_item_id == Some(inventory.id))
                {
                    return Some(lot.mass_kg.max(0.0));
                }
                ctx.db
                    .item()
                    .id()
                    .find(&inventory.item_id)
                    .map(|item| item.weight * inventory.quantity as f32)
            })
            .sum();
        let carried_water_weight = ctx
            .db
            .character_needs()
            .character_id()
            .find(character_id)
            .map_or(0.0, |needs| carried_water_weight_kg(needs.carried_water_ml));
        Self {
            hands,
            weapon,
            weapon_side,
            melee_weapon,
            melee_weapon_inventory_id,
            melee_weapon_side,
            ranged_weapon,
            ranged_weapon_inventory_id,
            ranged_weapon_side,
            ammunition,
            shield,
            shield_inventory_id,
            armor,
            inventory_weight: dry_inventory_weight + carried_water_weight,
        }
    }

    pub(crate) fn combat_training_profile(
        &self,
    ) -> adventuresim_core::strategic_schedule::CombatTrainingProfile {
        use adventuresim_core::strategic_schedule::EquippedCombatItem;
        adventuresim_core::strategic_schedule::CombatTrainingProfile::from_equipped_hands(
            self.hands.iter().flatten().map(|item| EquippedCombatItem {
                weapons: item.weapon_skills.core(),
                shield: item.kind == ItemKind::Shield,
                balance: item.balance,
            }),
        )
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

    pub(crate) fn combat_equipment(&self) -> CombatEquipment {
        let mut armor = [CombatArmor {
            flexibility: 1.0,
            range_of_motion: 1.0,
            ..CombatArmor::default()
        }; 7];
        for part in BodyPart::FULL_BODY.iter() {
            if let Some(item) = self.armor_for(part) {
                armor[body_part_index(part)] = CombatArmor {
                    resistance: item.resistance,
                    padding: item.padding,
                    flexibility: item.flexibility,
                    range_of_motion: item.range_of_motion,
                    coverage: item.coverage,
                };
            }
        }
        CombatEquipment {
            weapon: self.weapon.as_ref().map(combat_weapon),
            melee_weapon: self.melee_weapon.as_ref().map(combat_weapon),
            ranged_weapon: self.ranged_weapon.as_ref().map(combat_weapon),
            melee_weapon_id: self.melee_weapon_inventory_id,
            ranged_weapon_id: self.ranged_weapon_inventory_id,
            ranged_projectile_kind: self.ranged_weapon.as_ref().map(|weapon| {
                if weapon.id.contains("arquebus") {
                    CombatProjectileKind::Ball
                } else {
                    CombatProjectileKind::Arrowhead
                }
            }),
            defense_item_id: self.shield_inventory_id.or(self.melee_weapon_inventory_id),
            ammunition: self.ammunition,
            holding_side: self.weapon_side.unwrap_or(BodySide::Right),
            melee_holding_side: self.melee_weapon_side.unwrap_or(BodySide::Right),
            ranged_holding_side: self.ranged_weapon_side.unwrap_or(BodySide::Right),
            shield_block_bonus: self.shield.as_ref().map_or(0.0, |item| item.block),
            armor,
            inventory_weight: self.inventory_weight,
        }
    }
}

fn hand_side(index: usize) -> BodySide {
    if index == 0 {
        BodySide::Left
    } else {
        BodySide::Right
    }
}

fn combat_weapon(item: &Item) -> CombatWeapon {
    CombatWeapon {
        skills: item.weapon_skills.core(),
        melee: item.melee,
        ranged: item.ranged,
        blunt: item.blunt,
        slash: item.slash,
        pierce: item.pierce,
        accuracy: item.accuracy,
        weight: item.weight,
        penetration: item.penetration,
        melee_reach: if item.melee { item.reach } else { 0.0 },
        ranged_range: if item.ranged { item.reach } else { 0.0 },
        attack_interval_seconds: weapon_attack_interval(item),
        precise: item.precise,
        balance: item.balance,
        ranged_force_joules: 40.0 * item.weight.max(0.5),
    }
}

fn weapon_attack_interval(item: &Item) -> f32 {
    let draw_or_recovery = if item.ranged { 0.45 } else { 0.0 };
    (0.4 + item.weight.max(0.1) * 0.15 + item.balance.max(0.0) * 0.2 + draw_or_recovery)
        .clamp(0.35, 3.0)
}

fn carried_water_weight_kg(carried_water_ml: f32) -> f32 {
    carried_water_ml.max(0.0) / 1_000.0
}

impl PlayerEquipment for StrategicEquipment {
    fn weapon_skill_distribution(&self) -> adventuresim_core::equipment::WeaponSkillDistribution {
        self.weapon
            .as_ref()
            .map_or_else(Default::default, |item| item.weapon_skills.core())
    }
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
        self.weapon_side
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

pub(crate) fn load_combatant(
    ctx: &ReducerContext,
    character_id: u64,
    strategic_incapacitation: f32,
    strategic_pain: f32,
    strategic_blood_loss: f32,
) -> Result<Combatant, String> {
    let attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .ok_or("Character attributes not found")?;
    let attributes = crate::disease::effective_attributes(ctx, character_id, attributes)?;
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
    let limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(character_id)
        .ok_or("Character limbs not found")?;
    let stats = ctx
        .db
        .character_stats()
        .character_id()
        .find(character_id)
        .ok_or("Character stats not found")?;
    let condition = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .ok_or("Character condition not found")?;
    let equipment = StrategicEquipment::load(ctx, character_id, &equip);
    let combat_equipment = equipment.combat_equipment();
    let initial_ammunition = combat_equipment.ammunition;

    Ok(Combatant {
        id: character_id,
        attributes: CombatAttributes {
            endurance: attributes.endurance,
            immunity: attributes.immunity,
            gut: attributes.gut,
            precision: attributes.precision,
            intelligence: attributes.intelligence,
            instinct: attributes.instinct,
            eyesight: attributes.eyesight,
            hearing: attributes.hearing,
            left_arm_strength: attributes.left_arm_strength,
            right_arm_strength: attributes.right_arm_strength,
            left_leg_strength: attributes.left_leg_strength,
            right_leg_strength: attributes.right_leg_strength,
            left_arm_agility: attributes.left_arm_agility,
            right_arm_agility: attributes.right_arm_agility,
            left_leg_agility: attributes.left_leg_agility,
            right_leg_agility: attributes.right_leg_agility,
        },
        body: CombatBody {
            health: [
                limbs.left_arm_health,
                limbs.right_arm_health,
                limbs.left_leg_health,
                limbs.right_leg_health,
                limbs.chest_health,
                limbs.stomach_health,
                limbs.head_health,
            ],
            weight_kg: condition.body_weight_kg,
            primary_side: BodySide::Right,
        },
        essentials: CombatEssentials {
            calories_used_today: stats.calories_used,
            focus_level: stats.focus,
        },
        equipment: combat_equipment,
        skills: CombatSkills {
            polearm_hours: skills.polearm_hours,
            axe_hours: skills.axe_hours,
            bludgeon_hours: skills.bludgeon_hours,
            sword_hours: skills.sword_hours,
            knife_hours: skills.knife_hours,
            dodge_hours: skills.dodge_hours,
            block_hours: skills.block_hours,
            bow_hours: skills.bow_hours,
            crossbow_hours: skills.crossbow_hours,
            firearm_hours: skills.firearm_hours,
            throw_hours: skills.throw_hours,
            will_hours: skills.will_hours,
            insight_hours: skills.insight_hours,
            self_awareness_hours: skills.self_awareness_hours,
            humor_hours: skills.humor_hours,
            command_hours: skills.command_hours,
            deception_hours: skills.deception_hours,
            seduction_hours: skills.seduction_hours,
            medicine_hours: skills.medicine_hours,
            religion_hours: skills.religion_hours.total_direct(),
            stealth_hours: skills.stealth_hours,
            balance_hours: skills.balance_hours,
            anatomy_hours: skills.anatomy_hours,
            tailoring_hours: skills.tailoring_hours,
            smithing_hours: skills.smithing_hours,
        },
        starting_incapacitation: (strategic_incapacitation - strategic_pain - strategic_blood_loss)
            .max(0.0),
        starting_blood_fraction: if condition.maximum_blood_ml > 0.0 {
            (condition.current_blood_ml / condition.maximum_blood_ml).clamp(0.0, 1.0)
        } else {
            1.0
        },
        initial_ammunition,
        ..Combatant::new(character_id)
    })
}

#[cfg(test)]
mod tests {
    use super::carried_water_weight_kg;

    #[test]
    fn carried_water_contributes_one_kilogram_per_litre() {
        assert_eq!(carried_water_weight_kg(4_000.0), 4.0);
        assert_eq!(carried_water_weight_kg(-1.0), 0.0);
    }
}
