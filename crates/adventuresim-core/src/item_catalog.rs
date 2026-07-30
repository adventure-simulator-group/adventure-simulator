//! Build-time embedded, deterministic item definitions.
//!
//! Authors edit the strict JSON-compatible YAML documents in `content/items`.
//! `build.rs` validates and combines them; production code only reads this
//! embedded representation and never opens loose content files.

pub use crate::item_catalog_schema::*;
use serde::Deserialize;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/item_catalog.rs"));

static CATALOG: OnceLock<Vec<ItemDefinition>> = OnceLock::new();
static SOURCE_MAP: OnceLock<Vec<ItemSourceRef>> = OnceLock::new();

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ItemSourceRef {
    pub id: String,
    pub file: String,
    pub line: usize,
    pub column: usize,
}

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

pub fn source_for_item(id: &str) -> Option<&'static ItemSourceRef> {
    let sources = SOURCE_MAP.get_or_init(|| {
        let mut sources: Vec<ItemSourceRef> = serde_json::from_str(ITEM_CATALOG_SOURCE_MAP_JSON)
            .expect("validated embedded item source map");
        sources.sort_by(|a, b| a.id.cmp(&b.id));
        sources
    });
    sources
        .binary_search_by_key(&id, |source| source.id.as_str())
        .ok()
        .map(|index| &sources[index])
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
        assert_eq!(catalog().len(), 134);
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
        let sword_source = source_for_item("arming_sword").unwrap();
        assert_eq!(sword_source.file, "content/items/catalog.yaml");
        assert!(sword_source.line > 1);
        assert!(sword_source.column > 0);
        assert_eq!(source_for_item("missing"), None);
        assert!(
            catalog()
                .iter()
                .all(|item| source_for_item(&item.id).is_some()),
            "every item definition must retain an authored source location"
        );

        let stable_ids = catalog()
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_eq!(
            format!("{:x}", Sha256::digest(stable_ids.as_bytes())),
            "e7f25366ee7c6b5f3e5c1da889b6b600c0dc01883ce858cd0decf8f445d68056",
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
        assert_eq!(counts, [13, 6, 20, 14, 1, 1, 5, 24, 29, 21]);
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
