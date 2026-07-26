use adventuresim_core::prelude::*;
use bevy::{ecs::system::SystemParam, prelude::*};
use bevy_enhanced_input::prelude::Actions;
use serde::{Deserialize, Serialize};

use crate::inventory::{InventoryView, InventoryViewer};

/// BEI Component alias to mark players that are controlled by the present client.
pub type ControlledPlayer = Actions<Player>;

/// Component for a player entity, for both client-controlled
/// active player and other players.
#[derive(Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, PartialEq, Eq)]
#[require(PlayerId, Limbs, Skills, Attributes, Stats)]
#[component(immutable)]
pub struct Player {
    pub name: String,
}

/// Player's client ID usable to distinguish the active player
/// from other connected players.
#[derive(
    Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, Copy, PartialEq, Eq,
)]
#[component(immutable)]
pub struct PlayerId(pub u64);

impl PlayerId {
    /// Get associated color of this player.
    pub fn color(&self) -> Color {
        // SplitMix64-style mixing for good bit diffusion
        let mut x = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^= x >> 31;

        let hue = (x % 360) as f32;
        let saturation = 0.28 + ((x >> 8) & 0xFF) as f32 / 255.0 * 0.18;
        let value = 0.90 + ((x >> 16) & 0xFF) as f32 / 255.0 * 0.08;

        Color::hsv(hue, saturation, value)
    }
}

/// Creature families used to select the attacker's anatomical lore.
///
/// Current tactical characters are Human by default. Multi-category enemies
/// can provide every applicable category without changing combat resolution.
#[derive(Component, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[component(immutable)]
pub struct BestiaryCategories(pub Vec<BestiaryCategory>);

impl Default for BestiaryCategories {
    fn default() -> Self {
        Self(vec![BestiaryCategory::Human])
    }
}

/// General player stats.
#[derive(Component, Serialize, Deserialize, Debug, Reflect, Clone, PartialEq)]
pub struct Stats {
    pub calories_used: f32,
    pub focus: f32,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            calories_used: 0.0,
            focus: 1.0,
        }
    }
}

impl PlayerEssentials for Stats {
    fn calories_used_today(&self) -> f32 {
        self.calories_used
    }

    fn focus_level(&self) -> f32 {
        self.focus
    }
}

/// Limb health status.
#[derive(Component, Serialize, Deserialize, Debug, Reflect, Clone, PartialEq)]
pub struct Limbs {
    pub left_arm: f32,
    pub right_arm: f32,
    pub left_leg: f32,
    pub right_leg: f32,
    pub chest: f32,
    pub stomach: f32,
    pub head: f32,
}

impl Default for Limbs {
    fn default() -> Self {
        Self {
            left_arm: 1.0,
            right_arm: 1.0,
            left_leg: 1.0,
            right_leg: 1.0,
            chest: 1.0,
            stomach: 1.0,
            head: 1.0,
        }
    }
}

impl PlayerBody for Limbs {
    fn body_part_health(&self, part: BodyPart) -> f32 {
        match part {
            BodyPart::LeftArm => self.left_arm,
            BodyPart::RightArm => self.right_arm,
            BodyPart::LeftLeg => self.left_leg,
            BodyPart::RightLeg => self.right_leg,
            BodyPart::Chest => self.chest,
            BodyPart::Stomach => self.stomach,
            BodyPart::Head => self.head,
        }
    }

    fn body_weight(&self) -> f32 {
        // TODO: this should be stored in DB ?
        10.0
    }

    fn primary_side(&self) -> BodySide {
        // TODO: this should be stored in DB ?
        BodySide::Right
    }
}

/// Physical and mental skills of a [`Player`].
#[derive(Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, PartialEq)]
#[component(immutable)]
pub struct Skills {
    pub polearm_hours: f32,
    pub axe_hours: f32,
    pub bludgeon_hours: f32,
    pub sword_hours: f32,
    pub knife_hours: f32,
    pub dodge_hours: f32,
    pub block_hours: f32,
    pub bow_hours: f32,
    pub crossbow_hours: f32,
    pub firearm_hours: f32,
    pub throw_hours: f32,
    pub will_hours: f32,
    pub insight_hours: f32,
    pub self_awareness_hours: f32,
    pub humor_hours: f32,
    pub command_hours: f32,
    pub deception_hours: f32,
    pub seduction_hours: f32,
    pub physiology_hours: f32,
    pub religion_hours: f32,
    pub bestiary_beast_hours: f32,
    pub bestiary_undead_hours: f32,
    pub bestiary_human_hours: f32,
    pub bestiary_werekin_hours: f32,
    pub bestiary_elf_hours: f32,
    pub bestiary_dwarf_hours: f32,
    pub bestiary_fey_hours: f32,
    pub bestiary_spirit_hours: f32,
    pub bestiary_greenskin_hours: f32,
    pub bestiary_insectoid_hours: f32,
    pub bestiary_draconid_hours: f32,
    pub bestiary_construct_hours: f32,
    pub bestiary_wildmen_hours: f32,
    pub anatomy_hours: f32,
    pub stealth_hours: f32,
    pub balance_hours: f32,
    pub tailoring_hours: f32,
    pub smithing_hours: f32,
}

impl Skills {
    fn bestiary_hours(&self) -> BestiaryHours {
        BestiaryHours {
            beast: self.bestiary_beast_hours,
            undead: self.bestiary_undead_hours,
            human: self.bestiary_human_hours,
            werekin: self.bestiary_werekin_hours,
            elf: self.bestiary_elf_hours,
            dwarf: self.bestiary_dwarf_hours,
            fey: self.bestiary_fey_hours,
            spirit: self.bestiary_spirit_hours,
            greenskin: self.bestiary_greenskin_hours,
            insectoid: self.bestiary_insectoid_hours,
            draconid: self.bestiary_draconid_hours,
            construct: self.bestiary_construct_hours,
            wildmen: self.bestiary_wildmen_hours,
        }
    }
}

impl PlayerSkills for Skills {
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
            Skill::Physiology => self.physiology_hours,
            Skill::Cooking => 0.0,
            Skill::Religion => self.religion_hours,
            Skill::Bestiary => self.bestiary_hours().aggregate_effective(),
            Skill::Anatomy => self.anatomy_hours,
            Skill::Stealth => self.stealth_hours,
            Skill::Balance => self.balance_hours,
            Skill::TerrainPlains
            | Skill::TerrainForest
            | Skill::TerrainHills
            | Skill::TerrainUrban => 0.0,
            Skill::Tailoring => self.tailoring_hours,
            Skill::Smithing => self.smithing_hours,
        }
    }

    fn bestiary_hours_for(&self, category: BestiaryCategory) -> f32 {
        self.bestiary_hours().effective(category)
    }
}

/// Genetic attributes of a [`Player`].
#[derive(Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, PartialEq)]
#[component(immutable)]
pub struct Attributes {
    pub endurance: f32,
    pub immunity: f32,
    pub gut: f32,
    pub precision: f32,
    pub intelligence: f32,
    pub instinct: f32,
    pub eyesight: f32,
    pub hearing: f32,
    pub left_arm_strength: f32,
    pub right_arm_strength: f32,
    pub left_leg_strength: f32,
    pub right_leg_strength: f32,
    pub left_arm_agility: f32,
    pub right_arm_agility: f32,
    pub left_leg_agility: f32,
    pub right_leg_agility: f32,
}

impl PlayerAttributes for Attributes {
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
            _ => unimplemented!(),
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

pub type TacticalPlayerView<'v, 'w, 's> =
    PlayerInfo<&'v Attributes, &'v Limbs, &'v Stats, InventoryView<'v, 'w, 's>, &'v Skills>;

#[derive(SystemParam)]
pub struct TacticalPlayerViewer<'w, 's> {
    pub inventory: InventoryViewer<'w, 's>,
    pub q_player: Query<
        'w,
        's,
        (
            &'static Limbs,
            &'static Skills,
            &'static Stats,
            &'static Attributes,
        ),
    >,
}

impl TacticalPlayerViewer<'_, '_> {
    pub fn get(&self, entity: Entity) -> Result<TacticalPlayerView<'_, '_, '_>> {
        let (limbs, skills, stats, attributes) = self.q_player.get(entity)?;
        let inventory = self.inventory.get(entity);
        Ok(PlayerInfo::empty()
            .with_attributes(attributes)
            .with_body(limbs)
            .with_essentials(stats)
            .with_equipment(inventory)
            .with_skills(skills))
    }
}
