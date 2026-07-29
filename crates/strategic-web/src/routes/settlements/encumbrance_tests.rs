#[cfg(test)]
mod encumbrance_tests {
    use super::{
        ENCUMBRANCE_QUERY_CONCURRENCY, EncumbranceRows, encumbrance_query_ids, party_encumbrance,
        personal_encumbrance,
    };
    use crate::spacetimedb::{
        Character, CharacterAttributes, CharacterCondition, CharacterLimbs, CharacterNeeds,
        FoodLot, FoodPreparation, InventoryItem, ItemDefinition, PartyInventoryItem,
    };
    use serde_json::json;

    fn item(id: &str, weight: f32) -> ItemDefinition {
        serde_json::from_value(json!({
            "id": id,
            "weight": weight,
            "kind": "Weapon"
        }))
        .unwrap()
    }

    fn character(id: u64, alive: bool) -> Character {
        Character {
            id,
            name: format!("Character {id}"),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: None,
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        }
    }

    fn rows() -> EncumbranceRows {
        EncumbranceRows {
            attributes: vec![CharacterAttributes {
                character_id: 1,
                endurance: 0.0,
                immunity: 0.0,
                gut: 0.0,
                intelligence: 0.0,
                instinct: 0.0,
                eyesight: 0.0,
                hearing: 0.0,
                left_arm_strength: 0.0,
                right_arm_strength: 0.0,
                left_leg_strength: 4.0,
                right_leg_strength: 2.0,
                left_arm_agility: 0.0,
                right_arm_agility: 0.0,
                left_leg_agility: 0.0,
                right_leg_agility: 0.0,
            }],
            limbs: vec![CharacterLimbs {
                character_id: 1,
                left_arm_health: 1.0,
                right_arm_health: 1.0,
                left_leg_health: 0.5,
                right_leg_health: 1.0,
                head_health: 1.0,
                chest_health: 1.0,
                stomach_health: 1.0,
            }],
            conditions: vec![
                CharacterCondition {
                    character_id: 1,
                    body_weight_kg: 70.0,
                    current_blood_ml: 5_000.0,
                    maximum_blood_ml: 5_000.0,
                    religion_id: None,
                },
                CharacterCondition {
                    character_id: 2,
                    body_weight_kg: 90.0,
                    current_blood_ml: 5_000.0,
                    maximum_blood_ml: 5_000.0,
                    religion_id: None,
                },
            ],
            needs: vec![CharacterNeeds {
                character_id: 1,
                food_balance_kcal: 0.0,
                water_balance_ml: 0.0,
                carried_water_ml: 2_500.0,
            }],
        }
    }

    #[test]
    fn personal_summary_counts_body_water_quantity_and_injury_adjusted_capacity() {
        let inventory = vec![InventoryItem {
            id: 10,
            character_id: 1,
            item_id: "sword".into(),
            qty: 3,
        }];
        let summary = personal_encumbrance(1, &inventory, &[item("sword", 4.0)], &[], &rows());
        assert_eq!(summary.burden_kg, 84.5);
        assert_eq!(summary.capacity_kg, 300.0);
    }

    #[test]
    fn party_summary_excludes_dead_members_and_adds_shared_pool_once() {
        let inventories = vec![
            InventoryItem {
                id: 10,
                character_id: 1,
                item_id: "sword".into(),
                qty: 3,
            },
            InventoryItem {
                id: 11,
                character_id: 2,
                item_id: "sword".into(),
                qty: 20,
            },
        ];
        let pooled = vec![PartyInventoryItem {
            id: 20,
            party_id: "party".into(),
            item_id: "sword".into(),
            quantity: 2,
        }];
        let summary = party_encumbrance(
            &[character(1, true), character(2, false)],
            &inventories,
            &pooled,
            &[item("sword", 4.0)],
            &[],
            &rows(),
        );
        assert_eq!(summary.burden_kg, 92.5);
        assert_eq!(summary.capacity_kg, 300.0);
    }

    #[test]
    fn query_ids_are_living_only_deduplicated_and_keep_active_personal_rows() {
        let (inventory_ids, row_ids) = encumbrance_query_ids(
            &[character(1, true), character(1, true), character(2, false)],
            2,
        );
        assert_eq!(inventory_ids, vec![1]);
        assert_eq!(row_ids, vec![1, 2]);
        assert_eq!(ENCUMBRANCE_QUERY_CONCURRENCY, 4);
    }

    #[test]
    fn missing_rows_and_item_definitions_fail_closed_without_nan() {
        let summary = personal_encumbrance(
            99,
            &[InventoryItem {
                id: 30,
                character_id: 99,
                item_id: "unknown".into(),
                qty: 4,
            }],
            &[],
            &[],
            &EncumbranceRows::default(),
        );
        assert_eq!(summary.burden_kg, 0.0);
        assert_eq!(summary.capacity_kg, 0.0);
        assert_eq!(summary.penalty_fraction(), 1.0);
    }

    #[test]
    fn linked_food_lot_mass_replaces_static_item_weight() {
        let inventory = vec![InventoryItem {
            id: 40,
            character_id: 1,
            item_id: "cooked_meal".into(),
            qty: 1,
        }];
        let lots = vec![FoodLot {
            id: 5,
            inventory_item_id: Some(40),
            party_inventory_item_id: None,
            display_name: "Large stew".into(),
            preparation: FoodPreparation::Stewed,
            ingredient_item_ids: vec!["raw_venison".into()],
            ingredient_quantities: vec![25.0],
            salty_kg: 0.0,
            spicy_kg: 0.0,
            sweet_kg: 0.0,
            sour_kg: 0.0,
            savory_kg: 10.0,
            quality: 3,
            mass_kg: 25.0,
            nutrition_kcal: 10_000.0,
            total_value: 25.0,
            created_at_minute: 1,
        }];
        let summary =
            personal_encumbrance(1, &inventory, &[item("cooked_meal", 0.0)], &lots, &rows());
        assert_eq!(summary.burden_kg, 97.5);

        let mut partial = lots[0].clone();
        partial.mass_kg = 6.25;
        let summary = personal_encumbrance(
            1,
            &inventory,
            &[item("cooked_meal", 0.0)],
            &[partial],
            &rows(),
        );
        assert_eq!(summary.burden_kg, 78.75);
    }
}
