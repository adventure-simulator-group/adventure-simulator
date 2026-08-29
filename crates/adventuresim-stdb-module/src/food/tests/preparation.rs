#[test]
fn cutting_weapon_binding_has_a_fixed_versioned_vector() {
    assert_eq!(
        cutting_weapon_binding(
            CarriedInventoryScope::Party,
            17,
            "item:knife",
            0.75,
            1.25,
            DamageBins([0.0, 1.0, 2.5, -0.0, f32::INFINITY]),
        ),
        "cutting-weapon:v1:fb6b2e4b08c494dbdac835682f969d237b20df43b98965171a56e18aa83d2004"
    );
}

#[test]
fn preparation_adapter_revalidates_and_persists_terminal_attempts() {
    let source = crate::production_source(crate::food::FOOD_SOURCE);
    let reducer = source
        .split("pub fn prepare_ingredient_lot")
        .nth(1)
        .and_then(|tail| tail.split("fn cooking_method_preparation").next())
        .expect("preparation reducer");
    assert!(reducer.contains("preparation_request_id"));
    assert!(reducer.matches("load_preparation_authority").count() >= 2);
    assert!(reducer.contains("validate_commit"));
    assert!(reducer.contains("validate_material_commit"));
    assert!(reducer.contains("ingredient_preparation_receipt"));
    assert!(reducer.contains("interrupted: true"));
    assert!(reducer.contains("interrupted: false"));
    assert!(reducer.contains("receipt.inventory_scope == inventory_scope"));
    assert!(reducer.contains("receipt.inventory_item_id == inventory_item_id"));
    assert!(reducer.contains("receipt.attempt_generation == attempt_generation"));
    assert!(reducer.contains("effect_commit.is_none()"));
    assert!(reducer.contains("let post = load_preparation_authority"));
    assert!(
        reducer.contains("post.material_source_digest != authority.material_source_digest")
    );
    assert!(reducer.contains("checked_add(1)"));
    assert!(
        source.contains("#[view(accessor = backend_ingredient_preparation_plans, public)]")
    );
    assert!(source.contains("preparation_authority_digest_parts("));
    assert!(source.contains("view_carried_custody_is_fully_resolved"));
    assert!(source.contains("view_direct_custody"));
    assert!(source.contains("party_at_bound_road_challenge_view"));
    assert!(source.contains("Some((CarriedInventoryScope::Personal, row_id))"));
    assert!(source.contains("Some((CarriedInventoryScope::Party, row_id))"));
    assert!(!source.contains("view_actor_has_stable_preparation_interval"));
}

#[test]
fn request_identity_binds_generation_and_submitted_locator() {
    let first = preparation_request_id(
        1,
        "personal",
        2,
        3,
        4,
        5,
        IngredientPreparationAction::Cut,
        0,
        "settlement:test",
        "character:1",
    );
    let next = preparation_request_id(
        1,
        "personal",
        2,
        3,
        4,
        5,
        IngredientPreparationAction::Cut,
        1,
        "settlement:test",
        "character:1",
    );
    let forged_row = preparation_request_id(
        1,
        "personal",
        99,
        3,
        4,
        5,
        IngredientPreparationAction::Cut,
        0,
        "settlement:test",
        "character:1",
    );
    let nested = preparation_request_id(
        1,
        "personal",
        2,
        3,
        4,
        5,
        IngredientPreparationAction::Cut,
        0,
        "settlement:test",
        "container:9",
    );
    assert_ne!(first, next);
    assert_ne!(first, forged_row);
    assert_ne!(first, nested);
}

#[test]
fn grown_contamination_and_terminal_boundaries_are_planning_inputs() {
    let source = crate::production_source(crate::food::FOOD_SOURCE);
    assert!(source.contains("current_minute.saturating_sub(row.anchor_minute)"));
    assert!(source.contains("preparation_terminal_minute("));
    assert!(source.contains("preview_disease_terminal_boundary"));
    assert!(source.contains("preview_injury_boundary"));
    assert!(source.contains("terminal_minute,"));
    assert!(source.contains("Ingredient preparation wait diverged"));
    let planner = source
        .split("fn preparation_terminal_minute")
        .nth(1)
        .and_then(|tail| tail.split("fn next_preparation_attempt_generation").next())
        .expect("terminal preview");
    assert!(!planner.contains("clip_elapsed_for_disease"));
    assert!(planner.contains("InjuryRecoveryMinutes::new(duration)"));
}

#[test]
fn material_revision_overflow_fails_closed() {
    let mut lot = FoodLot {
        id: 1,
        inventory_item_id: Some(2),
        party_inventory_item_id: None,
        material_revision: u64::MAX,
        display_name: "test".into(),
        preparation: FoodPreparation::Raw,
        ingredient_item_ids: Vec::new(),
        ingredient_quantities: Vec::new(),
        salty_kg: 0.0,
        spicy_kg: 0.0,
        sweet_kg: 0.0,
        sour_kg: 0.0,
        savory_kg: 0.0,
        quality: 1,
        mass_kg: 1.0,
        nutrition_kcal: 1.0,
        total_value: 1.0,
        created_at_minute: 0,
    };
    assert!(retain_lot_fraction(&mut lot, 0.5).is_err());
    assert_eq!(lot.material_revision, u64::MAX);
}

#[test]
fn authoritative_preview_rejects_cooked_output_as_an_ingredient() {
    let source = crate::production_source(crate::food::FOOD_SOURCE);
    let preview = source
        .split("pub fn preview_cooking")
        .nth(1)
        .and_then(|tail| tail.split("fn expose_to_dysentery").next())
        .expect("preview cooking implementation");
    assert!(preview.contains("food::is_cookable_ingredient(&inventory.item_id)"));
    assert!(preview.contains("A cooked meal cannot be cooked again"));
}

#[test]
fn physical_preparation_keeps_safe_prefix_and_exact_instance_tool_rules() {
    let source = crate::production_source(crate::food::FOOD_SOURCE);
    let reducer = source
        .split("pub fn prepare_ingredient_lot")
        .nth(1)
        .unwrap();
    let wait = reducer.find("advance_character_wait_time").unwrap();
    assert!(wait < reducer.find("lot.preparation = post.next").unwrap());
    assert!(wait < reducer.find("apply_direct_training").unwrap());
    assert!(source.contains(
        "effective_weapon_stat(item.accuracy, damage, item.edge_sensitivity) >= 0.5"
    ));
    assert!(source.contains("row_is_fireplace_rooted"));
    assert!(source.contains("Skill::Knife"));
    assert!(source.contains("Skill::Bludgeon"));
}
