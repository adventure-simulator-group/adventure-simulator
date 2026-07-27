//! Build-time embedded, deterministic item definitions.
//!
//! Authors edit the strict JSON-compatible YAML documents in `content/items`.
//! `build.rs` validates and combines them; production code only reads this
//! embedded representation and never opens loose content files.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/item_catalog.rs"));

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

static CATALOG: OnceLock<Vec<ItemDefinition>> = OnceLock::new();

pub fn catalog() -> &'static [ItemDefinition] {
    CATALOG
        .get_or_init(|| {
            let mut documents: Vec<ItemCatalogDocument> =
                serde_json::from_str(ITEM_CATALOG_JSON).expect("validated embedded item catalog");
            let mut items: Vec<_> = documents
                .drain(..)
                .flat_map(|document| document.items)
                .collect();
            items.sort_by(|a, b| a.id.cmp(&b.id));
            items
        })
        .as_slice()
}

pub fn definition(id: &str) -> Option<&'static ItemDefinition> {
    catalog()
        .binary_search_by_key(&id, |definition| definition.id.as_str())
        .ok()
        .map(|index| &catalog()[index])
}

pub const fn revision() -> &'static str {
    ITEM_CATALOG_DIGEST
}

pub fn weapon_skills(id: &str) -> Option<WeaponSkills> {
    match &definition(id)?.kind {
        ItemKind::Weapon { skills, .. } => Some(*skills),
        _ => None,
    }
}

pub fn validate_references<'a>(
    references: impl IntoIterator<Item = &'a str>,
) -> Result<(), Vec<String>> {
    let missing: Vec<_> = references
        .into_iter()
        .filter(|id| definition(id).is_none())
        .map(str::to_owned)
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn embedded_catalog_is_sorted_unique_complete_and_revisioned() {
        assert_eq!(catalog().len(), 110);
        assert!(revision().len() == 64 && revision().bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(
            catalog()
                .windows(2)
                .all(|pair| pair[0].id.as_str() < pair[1].id.as_str())
        );
        assert_eq!(
            definition("arming_sword").unwrap().display_name,
            "Arming sword"
        );
        assert!(definition("missing").is_none());

        let stable_ids = catalog()
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_eq!(
            format!("{:x}", Sha256::digest(stable_ids.as_bytes())),
            "7b430f029fa43d76c4aa22c51524bf15f06b2e9457f28eff7d5d22b62fa32c52",
            "stable-ID golden changed; review persistence and reseed impact"
        );

        let counts = catalog().iter().fold([0_u16; 10], |mut counts, item| {
            let index = match &item.kind {
                ItemKind::Simple => 0,
                ItemKind::Currency => 1,
                ItemKind::Ingredient => 2,
                ItemKind::Medication => 3,
                ItemKind::Clothing => 4,
                ItemKind::Container { .. } => 5,
                ItemKind::Shield { .. } => 6,
                ItemKind::Armor { .. } => 7,
                ItemKind::Weapon { .. } => 8,
                ItemKind::Food => 9,
            };
            counts[index] += 1;
            counts
        });
        assert_eq!(counts, [12, 6, 11, 3, 1, 1, 5, 24, 26, 21]);
    }

    #[test]
    fn compositional_food_and_alcohol_are_not_flat_kinds() {
        let garlic = definition("garlic").unwrap();
        assert!(matches!(&garlic.kind, ItemKind::Ingredient));
        assert!(garlic.capabilities.food.is_some());
        let beer = definition("small_beer").unwrap();
        assert!(matches!(&beer.kind, ItemKind::Simple));
        assert!(beer.capabilities.alcohol.is_some());
    }

    #[test]
    fn authored_weapon_skills_are_explicit_and_normalized() {
        for item in catalog() {
            if let ItemKind::Weapon { skills, .. } = &item.kind {
                let total = skills.polearm
                    + skills.axe
                    + skills.bludgeon
                    + skills.sword
                    + skills.knife
                    + skills.bow
                    + skills.crossbow
                    + skills.firearm
                    + skills.throw;
                assert!((total - 1.0).abs() < 0.000_1, "{}", item.id);
            }
        }
        assert!(weapon_skills("not_an_item").is_none());
    }

    #[test]
    fn stable_reference_validation_rejects_missing_ids() {
        assert!(validate_references(["torch", "waterskin"]).is_ok());
        assert_eq!(
            validate_references(["torch", "removed_item"]),
            Err(vec!["removed_item".to_owned()])
        );
    }
}
