use adventuresim_core::autoresolve::CombatProjectileKind;
use adventuresim_core::item_catalog::{EquipmentBodyPart, EquipmentChannel, EquipmentLocation};
use adventuresim_core::physical_object::{CarriedInventoryScope, OperationalCustody};
use adventuresim_core::prelude::*;
use spacetimedb::{ReducerContext, Table, reducer, table};

use crate::character::{character_equipped_item as _, equipment_occupancy as _};
use crate::condition::character_condition as _;
use crate::food::food_lot as _;
use crate::item::item as _;
use crate::repair::item_condition as _;
use crate::{
    CharacterAttributes, CharacterLimbs, CharacterSkills, CharacterStats, InventoryItem, Item,
    ItemKind, character_attributes, character_limbs, character_skills, character_stats,
    character_strategic_condition, inventory_item,
};

#[derive(Clone, Debug, PartialEq)]
#[table(accessor = character_capability)]
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
    pub physiology: f32,
    pub knife: f32,
    pub tailoring: f32,
    pub surgery: f32,
    pub command: f32,
    pub religion: f32,
    #[default(0.0)]
    pub weapon_precision: f32,
    #[default(0u64)]
    pub autoresolve_combat_power: u64,
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
            physiology: value.physiology,
            knife: value.knife,
            tailoring: value.tailoring,
            surgery: value.surgery,
            command: value.command,
            religion: value.religion,
            weapon_precision: value.weapon_precision,
            autoresolve_combat_power: 0,
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
    let equipment = StrategicEquipment::load(ctx, character_id);
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
    let mut row = CharacterCapability::from((character_id, capabilities));
    let condition = ctx
        .db
        .character_strategic_condition()
        .character_id()
        .find(character_id);
    let combatant = load_combatant(
        ctx,
        character_id,
        condition.as_ref().map_or(0.0, |row| row.incapacitation),
        condition.as_ref().map_or(0.0, |row| row.pain),
        condition.as_ref().map_or(0.0, |row| row.blood_loss),
    )?;
    row.autoresolve_combat_power =
        adventuresim_core::autoresolve::autoresolve_combat_power(&combatant);
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
            let old_band = existing.physiology.round().clamp(0.0, 5.0) as u8;
            let new_band = row.physiology.round().clamp(0.0, 5.0) as u8;
            if old_band != new_band {
                crate::social::close_physiology_presence(ctx, character_id);
            }
            ctx.db.character_capability().character_id().update(row);
            if old_band != new_band {
                crate::social::reset_familiarity_after_join(ctx, character_id);
            }
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
            Skill::Charm => self.charm_hours,
            Skill::Command => self.command_hours,
            Skill::Deception => self.deception_hours,
            Skill::Physiology => self.physiology_hours,
            Skill::Cooking => self.cooking_hours,
            Skill::Herbalism => self.herbalism_hours,
            // Generic recruitment/tactical summaries use the character's best-covered
            // tradition. Authoritative religious morale always selects a tradition.
            Skill::Religion => self.religion_hours.maximum_effective(),
            Skill::Bestiary => self.bestiary_hours.aggregate_effective(),
            Skill::Surgery => self.surgery_hours,
            Skill::Stealth => self.stealth_hours,
            Skill::Balance => self.balance_hours,
            Skill::TerrainPlains => self.terrain_plains_hours,
            Skill::TerrainForest => self.terrain_forest_hours,
            Skill::TerrainHills => self.terrain_hills_hours,
            Skill::TerrainWetlands => self.terrain_wetlands_hours,
            Skill::TerrainUrban => self.terrain_urban_hours,
            Skill::TerrainSnow => self.terrain_snow_hours,
            Skill::Tailoring => self.tailoring_hours,
            Skill::Smithing => self.smithing_hours,
        }
    }

    fn bestiary_hours_for(&self, category: adventuresim_world_schema::BestiaryCategory) -> f32 {
        self.bestiary_hours.effective(category)
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
    armor: [adventuresim_core::equipment::LayeredArmor; 7],
    survival_clothing: adventuresim_core::survival::ClothingExposure,
    inventory_weight: f32,
}

impl StrategicEquipment {
    pub(crate) fn load(ctx: &ReducerContext, character_id: u64) -> Self {
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
        let normalized_hand = |location| {
            ctx.db
                .equipment_occupancy()
                .character_id()
                .filter(character_id)
                .find(|row| row.location == Some(location) && row.channel == EquipmentChannel::Held)
                .map(|row| row.inventory_item_id)
        };
        let hand_inventory_ids = [
            normalized_hand(EquipmentLocation::LeftHand),
            normalized_hand(EquipmentLocation::RightHand),
        ];
        let hands = [
            definition(hand_inventory_ids[0]),
            definition(hand_inventory_ids[1]),
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
            .and_then(|index| hand_inventory_ids[index]);
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
            .and_then(|index| hand_inventory_ids[index]);
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
            .and_then(|index| hand_inventory_ids[index]);
        let ammunition = ctx
            .db
            .inventory_item()
            .character_id()
            .filter(character_id)
            .filter(|inventory| inventory.item_id == "arrow")
            .filter(|inventory| {
                !crate::inventory_container::row_is_fireplace_rooted(
                    ctx,
                    CarriedInventoryScope::Personal,
                    inventory.id,
                )
            })
            .map(|inventory| inventory.quantity)
            .sum();
        let mut armor = [adventuresim_core::equipment::LayeredArmor::default(); 7];
        let mut survival_layers = Vec::new();
        let mut weatherproofing_total = 0_u32;
        let mut peripheral_protection_bps = [0_u16; 4];
        for part in BodyPart::FULL_BODY.iter() {
            let pieces: Vec<_> = ctx
                .db
                .character_equipped_item()
                .character_id()
                .filter(character_id)
                .filter_map(|equipped| {
                    let inventory = ctx
                        .db
                        .inventory_item()
                        .id()
                        .find(equipped.inventory_item_id)?;
                    let item = definition(Some(inventory.id))?;
                    let placement = item
                        .equipment_placements
                        .iter()
                        .find(|placement| placement.id == equipped.placement_id)?;
                    if !placement
                        .protection
                        .iter()
                        .any(|target| runtime_body_part(*target) == part)
                    {
                        return None;
                    }
                    let (channel, order) = ctx
                        .db
                        .equipment_occupancy()
                        .inventory_item_id()
                        .filter(inventory.id)
                        .max_by_key(|row| (row.channel.order(), row.order))
                        .map_or((EquipmentChannel::Containment, 0), |row| {
                            (row.channel, row.order)
                        });
                    Some(adventuresim_core::equipment::WearableProtection {
                        inventory_item_id: inventory.id,
                        body_part: part,
                        channel,
                        order,
                        coverage: item.coverage,
                        resistance: item.resistance,
                        padding: item.padding,
                        flexibility: item.flexibility,
                        range_of_motion: item.range_of_motion,
                    })
                })
                .collect();
            survival_layers.extend(pieces.iter().map(|piece| (piece.padding, piece.coverage)));
            if let Some(piece) =
                adventuresim_core::equipment::outermost_wearable(part, pieces.iter().copied())
            {
                let protection = adventuresim_core::survival::weatherproofing_from_outer_layer(
                    piece.resistance,
                    piece.coverage,
                );
                weatherproofing_total = weatherproofing_total.saturating_add(u32::from(protection));
                let peripheral_index = match part {
                    BodyPart::LeftArm => Some(0),
                    BodyPart::RightArm => Some(1),
                    BodyPart::LeftLeg => Some(2),
                    BodyPart::RightLeg => Some(3),
                    _ => None,
                };
                if let Some(index) = peripheral_index {
                    peripheral_protection_bps[index] = protection;
                }
            }
            armor[body_part_index(part)] =
                adventuresim_core::equipment::aggregate_layered_armor(part, pieces);
        }
        let dry_inventory_weight: f32 = ctx
            .db
            .inventory_item()
            .character_id()
            .filter(character_id)
            .filter_map(|inventory: InventoryItem| {
                if crate::inventory_container::row_is_fireplace_rooted(
                    ctx,
                    CarriedInventoryScope::Personal,
                    inventory.id,
                ) {
                    return None;
                }
                if let Some(lot) = ctx
                    .db
                    .food_lot()
                    .iter()
                    .find(|lot| lot.inventory_item_id == Some(inventory.id))
                {
                    return Some(lot.mass_kg.max(0.0));
                }
                ctx.db.item().id().find(&inventory.item_id).map(|item| {
                    let effective_quantity =
                        crate::inventory_amount::personal_fraction(ctx, inventory.id)
                            .map_or(inventory.quantity as f32, |fraction| fraction.as_unit_f32());
                    item.weight * effective_quantity
                })
            })
            .sum();
        let personal_custody = OperationalCustody::character(character_id)
            .expect("persisted character identities must be nonzero");
        let contained_water_weight =
            crate::inventory_container::contained_water_ml(ctx, &personal_custody)
                .map_or(f32::INFINITY, |water_ml| water_ml as f32 / 1_000.0);
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
            survival_clothing: adventuresim_core::survival::ClothingExposure {
                insulation_bps: adventuresim_core::survival::insulation_from_layers(
                    survival_layers,
                ),
                // Coverage scales continuously across the seven stable body
                // regions; resistance is capped at leather equivalence.
                weatherproofing_bps: (weatherproofing_total / 7)
                    .min(u32::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE))
                    as u16,
                peripheral_protection_bps,
            },
            inventory_weight: dry_inventory_weight + contained_water_weight,
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

    fn armor_for(&self, part: BodyPart) -> adventuresim_core::equipment::LayeredArmor {
        self.armor[body_part_index(part)]
    }

    pub(crate) fn survival_clothing(&self) -> adventuresim_core::survival::ClothingExposure {
        self.survival_clothing
    }

    pub(crate) fn combat_equipment(&self) -> CombatEquipment {
        let mut armor = [CombatArmor {
            flexibility: 1.0,
            range_of_motion: 1.0,
            ..CombatArmor::default()
        }; 7];
        for part in BodyPart::FULL_BODY.iter() {
            let item = self.armor_for(part);
            armor[body_part_index(part)] = CombatArmor {
                resistance: item.resistance,
                padding: item.padding,
                flexibility: item.flexibility,
                range_of_motion: item.range_of_motion,
                coverage: item.coverage,
            };
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

fn runtime_body_part(part: EquipmentBodyPart) -> BodyPart {
    use EquipmentBodyPart as E;
    match part {
        E::LeftArm => BodyPart::LeftArm,
        E::RightArm => BodyPart::RightArm,
        E::LeftLeg => BodyPart::LeftLeg,
        E::RightLeg => BodyPart::RightLeg,
        E::Chest => BodyPart::Chest,
        E::Stomach => BodyPart::Stomach,
        E::Head => BodyPart::Head,
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
        swing_precision: item.swing_precision,
        stab_precision: item.stab_precision,
        preferred_melee_style: item.preferred_melee_style,
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
    fn weapon_swing_precision(&self) -> f32 {
        self.weapon
            .as_ref()
            .map_or(0.0, |item| item.swing_precision)
    }
    fn weapon_stab_precision(&self) -> f32 {
        self.weapon.as_ref().map_or(0.0, |item| item.stab_precision)
    }
    fn weapon_preferred_melee_style(&self) -> MeleeAttackStyle {
        self.weapon
            .as_ref()
            .map_or(MeleeAttackStyle::Swing, |item| item.preferred_melee_style)
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
        self.armor_for(part).resistance
    }
    fn armor_padding(&self, part: BodyPart) -> f32 {
        self.armor_for(part).padding
    }
    fn armor_flexibility(&self, part: BodyPart) -> f32 {
        self.armor_for(part).flexibility
    }
    fn armor_range_of_motion(&self, part: BodyPart) -> f32 {
        self.armor_for(part).range_of_motion
    }
    fn armor_coverage(&self, part: BodyPart) -> f32 {
        self.armor_for(part).coverage
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
    let equipment = StrategicEquipment::load(ctx, character_id);
    let combat_equipment = equipment.combat_equipment();
    let initial_ammunition = combat_equipment.ammunition;

    let (starting_incapacitation, starting_blood_fraction) = derive_combat_starting_condition(
        strategic_incapacitation,
        strategic_pain,
        strategic_blood_loss,
        condition.current_blood_ml,
        condition.maximum_blood_ml,
    );

    Ok(Combatant {
        id: character_id,
        attributes: CombatAttributes {
            endurance: attributes.endurance,
            immunity: attributes.immunity,
            gut: attributes.gut,
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
            charm_hours: skills.charm_hours,
            command_hours: skills.command_hours,
            deception_hours: skills.deception_hours,
            physiology_hours: skills.physiology_hours,
            religion_hours: skills.religion_hours.total_direct(),
            stealth_hours: skills.stealth_hours,
            balance_hours: skills.balance_hours,
            bestiary_hours: skills.bestiary_hours,
            surgery_hours: skills.surgery_hours,
            tailoring_hours: skills.tailoring_hours,
            smithing_hours: skills.smithing_hours,
        },
        starting_incapacitation,
        starting_blood_fraction,
        initial_ammunition,
        ..Combatant::new(character_id)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn water_burden_comes_only_from_physical_containers() {
        let source = include_str!("capability.rs");
        assert!(source.contains("contained_water_ml"));
        assert!(!source.contains("carried_water_ml"));
    }
}
