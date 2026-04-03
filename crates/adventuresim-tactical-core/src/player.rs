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
        bitflags::bitflags_match!(part, {
            BodyPart::LeftArm => self.left_arm,
            BodyPart::RightArm => self.right_arm,
            BodyPart::LeftLeg => self.left_leg,
            BodyPart::RightLeg => self.right_leg,
            BodyPart::Chest => self.chest,
            BodyPart::Stomach => self.stomach,
            BodyPart::Head => self.head,
            _ => unimplemented!()
        })
    }

    fn body_weight(&self) -> f32 {
        10.0
    }
}

/// Physical and mental skills of a [`Player`].
#[derive(Component, Serialize, Deserialize, Debug, Reflect, Clone, PartialEq)]
#[component(immutable)]
pub struct Skills {
    pub melee_hours: f32,
    pub dodge_hours: f32,
    pub block_hours: f32,
}

impl Default for Skills {
    fn default() -> Self {
        Self {
            melee_hours: 1.0,
            dodge_hours: 1.0,
            block_hours: 1.0,
        }
    }
}

impl PlayerSkills for Skills {
    fn skill_hours_trained(&self, skill: Skill) -> f32 {
        match skill {
            Skill::Melee => self.melee_hours,
            Skill::Block => self.block_hours,
            Skill::Dodge => self.dodge_hours,
            Skill::Ranged
            | Skill::Will
            | Skill::Charisma
            | Skill::Medicine
            | Skill::Faith
            | Skill::Stealth
            | Skill::Balance
            | Skill::Surgeon => unimplemented!(),
        }
    }
}

/// Genetic attributes of a [`Player`].
#[derive(Component, Serialize, Deserialize, Debug, Reflect, Clone, PartialEq)]
#[component(immutable)]
pub struct Attributes {
    pub endurance: f32,
    pub immunity: f32,
    pub gut: f32,
    pub strength: f32,
    pub precision: f32,
    pub agility: f32,
    pub intelligence: f32,
    pub instinct: f32,
    pub eyesight: f32,
    pub hearing: f32,
}

impl Default for Attributes {
    fn default() -> Self {
        Self {
            endurance: 2.0,
            immunity: 2.0,
            gut: 2.0,
            strength: 2.0,
            precision: 2.0,
            agility: 2.0,
            intelligence: 2.0,
            instinct: 2.0,
            eyesight: 2.0,
            hearing: 2.0,
        }
    }
}

impl PlayerAttributes for Attributes {
    fn attr(&self, attr: Attribute) -> f32 {
        match attr {
            Attribute::Endurance => self.endurance,
            Attribute::Immunity => self.immunity,
            Attribute::Gut => self.gut,
            Attribute::Strength => self.strength,
            Attribute::Precision => self.precision,
            Attribute::Agility => self.agility,
            Attribute::Intelligence => self.intelligence,
            Attribute::Instinct => self.instinct,
            Attribute::Eyesight => self.eyesight,
            Attribute::Hearing => self.hearing,
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
    pub fn get(&self, entity: Entity) -> Option<TacticalPlayerView<'_, '_, '_>> {
        let (limbs, skills, stats, attributes) = self.q_player.get(entity).ok()?;
        let inventory = self.inventory.get(entity);
        Some(
            PlayerInfo::empty()
                .with_attributes(attributes)
                .with_body(limbs)
                .with_essentials(stats)
                .with_equipment(inventory)
                .with_skills(skills),
        )
    }
}
