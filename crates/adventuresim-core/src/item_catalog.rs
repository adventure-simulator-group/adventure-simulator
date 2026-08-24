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
            for item in &mut items {
                let Some(equipment) = &item.equipment else {
                    continue;
                };
                let placement_coverages = equipment
                    .placements
                    .iter()
                    .filter_map(|placement| {
                        let regions = placement
                            .surface
                            .iter()
                            .map(|span| span.regions.len())
                            .sum::<usize>();
                        (regions > 0).then(|| {
                            placement
                                .surface
                                .iter()
                                .map(|span| span.coverage * span.regions.len() as f32)
                                .sum::<f32>()
                                / regions as f32
                        })
                    })
                    .collect::<Vec<_>>();
                if placement_coverages.is_empty() {
                    continue;
                }
                let coverage =
                    placement_coverages.iter().sum::<f32>() / placement_coverages.len() as f32;
                match &mut item.kind {
                    ItemKind::Armor {
                        coverage: authored, ..
                    } => *authored = coverage,
                    ItemKind::Clothing => {
                        if let Some(protection) = item
                            .equipment
                            .as_mut()
                            .and_then(|equipment| equipment.protection.as_mut())
                        {
                            protection.coverage = coverage;
                        }
                    }
                    _ => {}
                }
            }
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

pub fn weapon_carry(id: &str) -> Option<WeaponCarry> {
    match &definition(id)?.kind {
        ItemKind::Weapon { carry, .. } => Some(*carry),
        _ => None,
    }
}

pub fn is_sheathable_weapon(id: &str) -> bool {
    weapon_carry(id) == Some(WeaponCarry::Sheathable)
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
        // The source catalog expands each availability epoch into a compiled
        // definition; the generated weapon loop adds four epoch rows.
        assert_eq!(catalog().len(), 167);
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
            "3225263028af61c4b6aa5645c8686a05dfd2efa30d6c7b6a156b3ce89cb5fe7e",
            "stable-ID golden changed intentionally: linen breeches were added; development data must be reseeded"
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
        // Holder chassis are simple catalog rows; their individual procedural
        // identities live in WeaponHolderInstance.
        assert_eq!(counts, [47, 6, 16, 14, 3, 1, 5, 25, 29, 21]);
    }

    #[test]
    fn field_tent_is_weighted_valuable_general_goods_shelter() {
        let tent = definition(crate::item_references::FIELD_TENT_ID).expect("field tent");
        assert!(tent.weight_kg > 0.0);
        assert!(tent.base_value > 0);
        assert!(tent.tags.iter().any(|tag| tag == "general_goods"));
        assert!(tent.tags.iter().any(|tag| tag == "field_shelter"));
        assert!(matches!(tent.kind, super::ItemKind::Simple));
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
    fn authored_weapon_carry_contract_matches_parent_placements() {
        for item in catalog() {
            let ItemKind::Weapon { carry, .. } = &item.kind else {
                continue;
            };
            let equipment = item
                .equipment
                .as_ref()
                .unwrap_or_else(|| panic!("{} lacks equipment", item.id));
            let has_sheath_placement = equipment.placements.iter().any(|placement| {
                placement.parents.len() == 1
                    && placement.parents[0].channel == EquipmentChannel::Containment
            });
            let all_hand_held_roots = equipment.placements.iter().all(|placement| {
                placement.parents.is_empty()
                    && placement.occupancy.len() == 1
                    && matches!(
                        placement.occupancy[0].location,
                        EquipmentLocation::LeftHand | EquipmentLocation::RightHand
                    )
                    && placement.occupancy[0].channel == EquipmentChannel::Held
                    && placement.occupancy[0].order == 0
            });
            let tagged = equipment
                .attachment_tags
                .iter()
                .any(|tag| tag == "sheathable_weapon");
            match *carry {
                WeaponCarry::Sheathable => {
                    assert!(has_sheath_placement && tagged, "{}", item.id)
                }
                WeaponCarry::HandOnly => assert!(all_hand_held_roots && !tagged, "{}", item.id),
            }
        }
        for item_id in [
            "halberd",
            "hunting_spear",
            "military_pike",
            "spear",
            "walking_staff",
        ] {
            assert_eq!(weapon_carry(item_id), Some(WeaponCarry::HandOnly));
        }
        for item_id in [
            "club",
            "flanged_mace",
            "hand_axe",
            "war_hammer",
            "zweihander",
        ] {
            assert_eq!(weapon_carry(item_id), Some(WeaponCarry::Sheathable));
        }
    }

    #[test]
    fn armor_and_clothing_definitions_have_explicit_equipment_projections() {
        for item in catalog()
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Armor { .. } | ItemKind::Clothing))
        {
            let equipment = item
                .equipment
                .as_ref()
                .unwrap_or_else(|| panic!("{} lacks equipment projection", item.id));
            assert!(!equipment.placements.is_empty(), "{}", item.id);
            assert!(
                equipment
                    .placements
                    .iter()
                    .all(|placement| !placement.occupancy.is_empty()),
                "{}",
                item.id
            );
        }
        let tunic = definition("linen_tunic")
            .unwrap()
            .equipment
            .as_ref()
            .unwrap();
        assert_eq!(tunic.placements[0].occupancy.len(), 4);
        assert_eq!(tunic.placements[0].protection.len(), 4);
    }

    #[test]
    fn compiled_catalog_represents_attachment_graph_examples_without_kind_inference() {
        use crate::item_catalog_schema::EquipmentChannel;

        let belt = definition("leather_belt")
            .unwrap()
            .equipment
            .as_ref()
            .unwrap();
        assert!(
            belt.attachment_points
                .iter()
                .any(|point| point.id == "left")
        );
        let sheath = definition("sword_sheath")
            .unwrap()
            .equipment
            .as_ref()
            .unwrap();
        assert!(sheath.attachment_tags.contains(&"sheath".to_owned()));
        let weapon_loop = definition("weapon_loop")
            .unwrap()
            .equipment
            .as_ref()
            .unwrap();
        assert!(weapon_loop.attachment_tags.contains(&"sheath".to_owned()));
        assert!(
            weapon_loop
                .attachment_points
                .iter()
                .any(|point| { point.accepts_tags.contains(&"sheathable_weapon".to_owned()) })
        );
        assert!(
            sheath
                .attachment_points
                .iter()
                .any(|point| point.accepts_tags.contains(&"sheathable_weapon".to_owned()))
        );
        let knife = definition("utility_knife")
            .unwrap()
            .equipment
            .as_ref()
            .unwrap();
        assert!(
            knife
                .attachment_tags
                .contains(&"sheathable_weapon".to_owned())
        );
        let attached_knife = knife
            .placements
            .iter()
            .find(|placement| placement.id == "attached")
            .expect("attached knife placement");
        assert_eq!(
            attached_knife.parents[0].channel,
            EquipmentChannel::Containment
        );
        let sheath_mount = sheath
            .placements
            .iter()
            .flat_map(|placement| placement.parents.iter())
            .find(|parent| parent.channel == EquipmentChannel::Mount)
            .expect("sheath mounts to belt");
        assert_eq!(sheath_mount.channel, EquipmentChannel::Mount);
        assert_eq!(
            sheath.placements[0].parents.len(),
            2,
            "the sheath fixture exercises multi-point attachment"
        );
        let sheath_blade = sheath
            .attachment_points
            .iter()
            .find(|point| point.id == "blade")
            .expect("sheath contains weapon");
        assert_eq!(sheath_blade.channel, EquipmentChannel::Containment);
        assert!(
            sheath_blade
                .accepts_tags
                .contains(&"sheathable_weapon".to_owned())
        );

        let bag = definition("leather_satchel")
            .unwrap()
            .equipment
            .as_ref()
            .unwrap();
        assert!(bag.placements.iter().any(|placement| {
            placement
                .parents
                .iter()
                .any(|parent| parent.channel == EquipmentChannel::Mount)
        }));
        let contents = bag
            .attachment_points
            .iter()
            .find(|point| point.id == "contents")
            .expect("bag contents");
        assert_eq!(contents.channel, EquipmentChannel::Containment);
        assert!(contents.capacity > 1);
        assert!(contents.accepts_tags.is_empty());
        assert!(bag.placements.iter().any(|placement| {
            placement.parents.is_empty()
                && placement.occupancy.iter().any(|requirement| {
                    matches!(
                        requirement.location,
                        crate::item_catalog_schema::EquipmentLocation::LeftShoulder
                            | crate::item_catalog_schema::EquipmentLocation::RightShoulder
                    )
                })
        }));

        let sword = definition("arming_sword")
            .unwrap()
            .equipment
            .as_ref()
            .unwrap();
        assert!(sword.placements.iter().any(|placement| {
            placement
                .parents
                .iter()
                .any(|parent| parent.channel == EquipmentChannel::Containment)
        }));

        let authored_point_order = belt
            .attachment_points
            .iter()
            .map(|point| point.order)
            .collect::<Vec<_>>();
        assert!(
            authored_point_order
                .windows(2)
                .all(|pair| pair[0] <= pair[1]),
            "repeated selection traverses attachment points in authored order"
        );
        for fixture in ["boot_sheath", "forearm_holster"] {
            let definition = definition(fixture).unwrap().equipment.as_ref().unwrap();
            assert!(
                definition
                    .placements
                    .iter()
                    .all(|placement| placement.protection.is_empty())
            );
        }
    }

    #[test]
    fn every_equipment_item_has_finite_positive_tactical_geometry() {
        let equipment: Vec<_> = catalog()
            .iter()
            .filter_map(|item| item.equipment.as_ref().map(|equipment| (item, equipment)))
            .collect();
        assert_eq!(equipment.len(), 69);
        for (item, equipment) in equipment {
            assert!(
                equipment
                    .physical
                    .dimensions_m
                    .iter()
                    .all(|dimension| dimension.is_finite() && *dimension > 0.0),
                "{}",
                item.id
            );
            assert!(equipment.physical.grip_to_tip_m.is_finite());
            assert!(equipment.physical.grip_to_tip_m >= 0.0);
            assert!(
                equipment
                    .physical
                    .anchor_offset_m
                    .iter()
                    .all(|offset| offset.is_finite()),
                "{}",
                item.id
            );
        }
    }

    #[test]
    fn every_held_catalog_item_has_explicit_authored_hand_placements() {
        for definition in catalog() {
            let slot = match &definition.kind {
                ItemKind::Weapon { slot, .. } | ItemKind::Shield { slot, .. } => *slot,
                _ => continue,
            };
            let equipment = definition
                .equipment
                .as_ref()
                .unwrap_or_else(|| panic!("{} lacks equipment topology", definition.id));
            let has_held = |location| {
                equipment.placements.iter().any(|placement| {
                    placement.parents.is_empty()
                        && placement.occupancy.iter().any(|requirement| {
                            requirement.location == location
                                && requirement.channel == EquipmentChannel::Held
                        })
                })
            };
            match slot {
                Slot::AnyHolding => {
                    assert!(
                        has_held(EquipmentLocation::LeftHand)
                            && has_held(EquipmentLocation::RightHand),
                        "{} needs explicit left and right hand placements",
                        definition.id
                    );
                }
                Slot::LeftHolding => {
                    assert!(has_held(EquipmentLocation::LeftHand), "{}", definition.id);
                }
                Slot::RightHolding => {
                    assert!(has_held(EquipmentLocation::RightHand), "{}", definition.id);
                }
                _ => {}
            }
        }
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
