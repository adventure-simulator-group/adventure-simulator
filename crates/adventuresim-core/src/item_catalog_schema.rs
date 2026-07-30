//! Exact typed authoring schema shared by the build compiler and runtime.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemCatalogDocument {
    pub schema_version: u32,
    pub items: Vec<ItemDefinition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemDefinition {
    pub id: String,
    pub display_name: String,
    pub weight_kg: f32,
    pub base_value: u32,
    #[serde(default)]
    pub tags: Vec<String>,
    pub presentation: Presentation,
    #[serde(flatten)]
    pub kind: ItemKind,
    #[serde(default)]
    pub capabilities: Capabilities,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Presentation {
    pub icon: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ItemKind {
    Simple,
    Currency,
    Ingredient,
    Medication,
    Clothing,
    Container {
        slot: Slot,
    },
    Shield {
        slot: Slot,
        block: f32,
    },
    Armor {
        slot: Slot,
        coverage: f32,
        resistance: f32,
        padding: f32,
        flexibility: f32,
        range_of_motion: f32,
    },
    Weapon {
        slot: Slot,
        accuracy: f32,
        reach_m: f32,
        penetration: f32,
        balance: f32,
        precise: bool,
        melee: bool,
        ranged: bool,
        damage_types: Vec<DamageType>,
        skills: WeaponSkills,
    },
    Food,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    None,
    LeftHolding,
    RightHolding,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Chest,
    Stomach,
    Head,
    AnyHolding,
    AnyArm,
    AnyLeg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamageType {
    Blunt,
    Slash,
    Pierce,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponSkills {
    #[serde(default)]
    pub polearm: f32,
    #[serde(default)]
    pub axe: f32,
    #[serde(default)]
    pub bludgeon: f32,
    #[serde(default)]
    pub sword: f32,
    #[serde(default)]
    pub knife: f32,
    #[serde(default)]
    pub bow: f32,
    #[serde(default)]
    pub crossbow: f32,
    #[serde(default)]
    pub firearm: f32,
    #[serde(default)]
    pub throw: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub durability: Option<Durability>,
    pub food: Option<Food>,
    pub alcohol: Option<Alcohol>,
    pub container: Option<Container>,
    pub book: Option<Book>,
}

/// Authored teaching metadata. Books remain ordinary `simple` items; this
/// capability is resolved from the embedded catalog and is never flattened
/// into a persisted inventory row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Book {
    pub medium: adventuresim_world_schema::WrittenLanguage,
    pub target: BookTarget,
    pub lower_rank: u8,
    pub upper_rank: u8,
    #[serde(default)]
    pub settlement_allowlist: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BookTarget {
    Written {
        language: adventuresim_world_schema::WrittenLanguage,
    },
    Religion {
        religion: adventuresim_world_schema::OfficialReligion,
    },
    Bestiary {
        category: adventuresim_world_schema::BestiaryCategory,
    },
    Terrain {
        terrain: String,
    },
    Skill {
        skill: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Durability {
    pub quality: u8,
    pub yield_j: f32,
    pub fracture_j: f32,
    pub wear: f32,
    pub failure_share: f32,
    pub edge_sensitivity: f32,
    pub handling_sensitivity: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Food {
    pub class: String,
    pub nutrition_kcal: f32,
    pub value_per_unit: f32,
    pub growth_per_hour: f32,
    pub cooking_minutes: u32,
    pub flavors_kg: Flavors,
    pub culinary_fat: bool,
    pub quality: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flavors {
    pub salty: f32,
    pub spicy: f32,
    pub sweet: f32,
    pub sour: f32,
    pub savory: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alcohol {
    pub serving_ml: u32,
    pub abv_basis_points: u16,
    pub net_hydration_ml: u32,
    pub disinfectant_effectiveness: u16,
    pub disinfectant_focused: bool,
    pub potable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Container {
    pub capacity_ml: u32,
}
