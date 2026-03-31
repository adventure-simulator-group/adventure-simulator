//! Stub types for documentation and testing purposes.
//! These provide concrete implementations of the player traits for examples.
//!
//! Note: These are only meant for doc tests and internal testing.
//! In production code, you would implement the traits for your own types.
#![allow(unused)]
#![doc(hidden)]

use crate::prelude::*;

#[derive(Default, Debug)]
pub struct StubAttributes;

impl PlayerAttributes for StubAttributes {
    fn attr(&self, _attr: Attribute) -> f32 {
        1.0
    }
}

#[derive(Default, Debug)]
pub struct StubBody;

impl PlayerBody for StubBody {
    fn body_part_health(&self, _part: BodyPart) -> f32 {
        1.0
    }

    fn body_weight(&self) -> f32 {
        70.0
    }
}

#[derive(Default, Debug)]
pub struct StubEssentials;

impl PlayerEssentials for StubEssentials {
    fn calories_used_today(&self) -> f32 {
        0.0
    }

    fn focus_level(&self) -> f32 {
        1.0
    }
}

#[derive(Default, Debug)]
pub struct StubEquipment;

impl PlayerEquipment for StubEquipment {
    fn weapon_accuracy(&self) -> f32 {
        1.0
    }

    fn armor_dodge(&self) -> f32 {
        1.0
    }

    fn inventory_weight(&self) -> f32 {
        10.0
    }

    fn shield_block_bonus(&self) -> f32 {
        1.0
    }
}

#[derive(Default, Debug)]
pub struct StubSkills;

impl PlayerSkills for StubSkills {
    fn skill_hours_trained(&self, _skill: Skill) -> f32 {
        1000.0
    }
}
