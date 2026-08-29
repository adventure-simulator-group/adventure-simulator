#[test]
fn container_cooking_is_distinct_from_loose_roasting() {
    let source = crate::production_source(crate::food::FOOD_SOURCE);
    assert!(!source.contains("pub fn set_fireplace_instrument"));
    let cooking = source
        .split("fn add_fireplace_ingredients_at")
        .nth(1)
        .unwrap();
    assert!(cooking.contains("let is_vessel = vessel_station.is_some()"));
    assert!(cooking.contains("!is_vessel && inventory_item_ids.len() > 32"));
    assert!(cooking.contains("CookingMethod::Roast"));
}

#[test]
fn vessel_selection_uses_direct_authoritative_food_lots() {
    let source = crate::production_source(crate::food::FOOD_SOURCE);
    let reducer = source
        .split("pub fn start_fireplace_container_cooking")
        .nth(1)
        .unwrap()
        .split("fn add_fireplace_ingredients_at")
        .next()
        .unwrap();
    assert!(reducer.contains(".parent_object_id()"));
    assert!(reducer.contains(".filter(container_object_id)"));
    assert!(!reducer.contains("subtree_object_ids"));
    assert!(reducer.contains("InventoryLocation::Personal"));
    assert!(reducer.contains("InventoryLocation::Party"));
    assert!(reducer.contains("let (Some(lot), Some(amount))"));
    assert!(reducer.contains("A cooked meal cannot be cooked again"));
}

#[test]
fn stew_water_and_fireplace_escrow_contract_are_explicit() {
    let source = crate::production_source(crate::food::FOOD_SOURCE);
    let cook = source
        .split("pub fn add_fireplace_ingredients")
        .nth(1)
        .and_then(|tail| tail.split("pub fn retrieve_fireplace_dish").next())
        .expect("fireplace ingredient reducer source");
    assert!(cook.contains("mass += water_ml / 1_000.0"));
    assert!(cook.contains("contained_water_ml"));
    assert!(cook.contains("food::microbial_concentration(loads.iter().sum(), mass)"));
    assert!(cook.contains("if method == CookingMethod::Stew"));
    assert!(cook.contains("ctx.db.fireplace_dish().insert"));
    assert!(!cook.contains("advance_character_wait_time"));
    assert!(!cook.contains("consume_food_amount(ctx, character_id"));
    assert!(cook.contains("pan_fry_has_enough_fat"));
    assert!(cook.contains("chef_quality_tier"));
}

#[test]
fn fireplace_authority_is_private_location_bound_and_race_safe() {
    let source = crate::production_source(crate::food::FOOD_SOURCE);
    assert!(source.contains("#[table(accessor = fireplace_station)]"));
    assert!(source.contains("#[table(accessor = fireplace_dish)]"));
    assert!(source.contains("#[view(accessor = backend_fireplace_stations, public)]"));
    assert!(source.contains("#[view(accessor = backend_fireplace_dishes, public)]"));
    let dish_projection = source
        .split("pub struct BackendFireplaceDish")
        .nth(1)
        .and_then(|tail| {
            tail.split("#[view(accessor = backend_fireplace_dishes")
                .next()
        })
        .expect("gateway dish projection");
    assert!(!dish_projection.contains("raw_contamination"));
    assert!(source.contains("parse::<StrategicFixtureId>()"));
    assert!(source.contains("current_journey_camp_place"));
    assert!(source.contains("fireplace_fixture_id"));
    assert!(!source.contains("pub context_key: String"));
    let camp_custody = source
        .split("pub(crate) fn require_clear_current_camp_fireplace")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(crate) fn require_members_clear_current_camp_fireplace")
                .next()
        })
        .expect("camp fireplace custody guard");
    assert!(camp_custody.contains("validate_persisted_station_fixture"));
    assert!(camp_custody.contains("validate_persisted_dish_fixture"));
    assert!(!camp_custody.contains("ends_with"));
    assert!(source.contains("This fireplace already holds a dish"));
    assert!(source.contains("Food lot selections must be unique and positive"));
    assert!(source.contains("vessel_station_key"));
    assert!(source.contains("Retrieve the cooked dish before removing its container"));
    let container_retrieval = source
        .split("pub fn retrieve_fireplace_container")
        .nth(1)
        .and_then(|tail| tail.split("fn preparation_skill_check").next())
        .expect("container retrieval reducer");
    assert!(container_retrieval.contains("OperationalCustody::Party(party_id)"));
    assert!(container_retrieval.contains(".party_authority()"));
    assert!(container_retrieval.contains(".is_none()"));
    assert!(source.contains("instrument_return_custody"));
    assert!(!source.contains("instrument_party_id"));
}

#[test]
fn camp_departure_and_retrieval_cleanup_enforce_fireplace_custody() {
    let food_source = crate::production_source(crate::food::FOOD_SOURCE);
    let travel_source = crate::production_source(include_str!("../../strategic/travel_reducers.rs"));
    assert!(travel_source.contains("require_clear_current_camp_fireplace"));
    assert!(food_source.contains(
        "Retrieve every dish and remove every cooking instrument before breaking camp"
    ));
    let retrieval = food_source
        .split("pub fn retrieve_fireplace_dish")
        .nth(1)
        .and_then(|tail| tail.split("pub fn preview_cooking").next())
        .expect("dish retrieval reducer");
    assert!(retrieval.contains("fireplace_station().key().delete"));
    assert!(!retrieval.contains("train_skill"));
    assert!(!retrieval.contains("morale"));
}

#[test]
fn dish_retrieval_is_bound_to_immutable_source_custody() {
    let party_source = crate::object_custody::encode_custody(
        &adventuresim_core::physical_object::OperationalCustody::party("party-before-transfer")
            .unwrap(),
    );
    let expected =
        OperationalCustody::party("party-before-transfer").map_err(|error| error.to_string());
    assert_eq!(dish_inventory_destination(&party_source, 7), expected);

    let personal_source = crate::object_custody::encode_custody(
        &adventuresim_core::physical_object::OperationalCustody::character(7).unwrap(),
    );
    assert!(dish_inventory_destination(&personal_source, 8).is_err());
}

#[test]
fn fireplace_container_retrieval_rejects_tactical_actors() {
    let source = crate::production_source(crate::food::FOOD_SOURCE);
    let retrieval = source
        .split("pub fn retrieve_fireplace_container")
        .nth(1)
        .and_then(|tail| tail.split("fn preparation_skill_check").next())
        .expect("container retrieval reducer");
    assert!(retrieval.contains("if actor.in_server"));
    assert!(retrieval.contains("Cooking is unavailable during a tactical encounter"));
}

#[test]
fn party_exit_and_death_have_explicit_fireplace_custody_policy() {
    let food_source = crate::production_source(crate::food::FOOD_SOURCE);
    let party_source = include_str!("../../strategic/inventory_trade.rs");
    let character_source = crate::production_source(include_str!("../../character.rs"));
    let removal = party_source
        .split("pub fn remove_party_member")
        .nth(1)
        .and_then(|tail| tail.split("pub fn disband_party").next())
        .expect("party member removal reducer");
    let disband = party_source
        .split("pub fn disband_party")
        .nth(1)
        .expect("party disband reducer");
    assert!(removal.contains("require_members_clear_current_camp_fireplace"));
    assert!(disband.contains("require_members_clear_current_camp_fireplace"));
    assert!(
        character_source
            .contains("crate::food::cleanup_fireplace_custody_for_death(ctx, character_id)")
    );

    let cleanup = food_source
        .split("pub(crate) fn cleanup_fireplace_custody_for_death")
        .nth(1)
        .and_then(|tail| tail.split("fn validate_fireplace_fixture").next())
        .expect("death fireplace cleanup");
    assert!(cleanup.contains("fireplace_dish()"));
    assert!(cleanup.contains(".character_id()"));
    assert!(cleanup.contains("add_to_party_inventory_checked"));
    assert!(cleanup.contains("ctx.db.inventory_item().insert"));
    assert!(cleanup.contains("fireplace_station().key().delete"));
    assert!(cleanup.contains("prevalidate_rehome_subtree"));
    assert!(cleanup.contains("rehome_subtree(ctx, object_id, &destination)?"));
    assert!(!cleanup.contains("let _ ="));
    assert!(cleanup.contains("Abandoned tools remain installed at their station"));
}

#[test]
fn vessel_selection_is_direct_and_preparation_shortens_safety_time() {
    let source = crate::production_source(crate::food::FOOD_SOURCE);
    assert!(source.contains(".parent_object_id()"));
    assert!(source.contains(".filter(container_object_id)"));
    assert!(source.contains("CUT_COOKING_TIME_FACTOR"));
    assert!(source.contains("GROUND_COOKING_TIME_FACTOR"));
    assert!(source.contains("method_doneness_outcome"));
}
