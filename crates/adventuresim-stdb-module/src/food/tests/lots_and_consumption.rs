#[test]
fn every_food_lot_constructor_establishes_stable_identity_and_revision() {
    let source = crate::production_source(crate::food::FOOD_SOURCE);
    assert_eq!(
        source.matches("ctx.db.food_lot().insert(FoodLot {").count(),
        3
    );
    assert_eq!(source.matches("ctx.db.food_lot().insert(child)").count(), 3);
    assert_eq!(source.matches("material_revision: 1").count(), 3);
    assert!(source.matches("ensure_food_material_object").count() >= 8);
    assert!(!source.contains("material_revision: 0"));
}

#[test]
fn every_partial_food_split_scales_full_contamination_provenance() {
    let source = crate::production_source(crate::food::FOOD_SOURCE);
    assert_eq!(
        source
            .matches("split_food_contamination_provenance(ctx, source.id, child.id, ratio)")
            .count(),
        3
    );
    assert!(source.contains(".insert(FoodContaminationProvenance {"));
    assert!(source.contains("consume_food_contamination_provenance"));
}

#[test]
fn eating_and_travel_reconcile_stable_container_objects() {
    let source = crate::production_source(crate::food::FOOD_SOURCE);
    assert!(source.matches("reconcile_consumed_row(").count() >= 4);
    assert!(source.contains("CarriedInventoryScope::Personal"));
    assert!(source.contains("CarriedInventoryScope::Party"));
    assert!(source.contains("row_is_fireplace_rooted"));
}

#[test]
fn partial_lot_retains_quality_and_scales_every_flavor() {
    let mut lot = FoodLot {
        id: 1,
        inventory_item_id: Some(2),
        party_inventory_item_id: None,
        material_revision: 1,
        display_name: "Roasted test".into(),
        preparation: FoodPreparation::Roasted,
        ingredient_item_ids: vec!["salt".into()],
        ingredient_quantities: vec![1.0],
        salty_kg: 1.0,
        spicy_kg: 0.8,
        sweet_kg: 0.6,
        sour_kg: 0.4,
        savory_kg: 0.2,
        quality: 4,
        mass_kg: 1.0,
        nutrition_kcal: 100.0,
        total_value: 10.0,
        created_at_minute: 0,
    };
    retain_lot_fraction(&mut lot, 0.25).unwrap();
    assert_eq!(lot.quality, 4);
    assert_eq!(lot.salty_kg, 0.25);
    assert_eq!(lot.spicy_kg, 0.2);
    assert_eq!(lot.sweet_kg, 0.15);
    assert_eq!(lot.sour_kg, 0.1);
    assert_eq!(lot.savory_kg, 0.05);
}

#[test]
fn catalog_quality_is_copied_when_lots_are_acquired() {
    let source = crate::production_source(crate::food::FOOD_SOURCE);
    let constructor = source
        .split("pub fn create_personal_food_lot")
        .nth(1)
        .and_then(|tail| tail.split("pub fn create_party_food_lot").next())
        .expect("personal lot constructor");
    assert!(constructor.contains("quality: definition.default_quality.clamp(1, 5)"));
}

#[test]
fn hidden_food_contamination_uses_explicit_food_water_prevention() {
    let source = crate::production_source(crate::food::FOOD_SOURCE);
    let exposure = source
        .split("fn expose_to_dysentery")
        .nth(1)
        .and_then(|tail| tail.split("fn consume_food_amount").next())
        .expect("foodborne exposure source");
    assert!(exposure.contains("protected_point_exposure"));
    assert!(exposure.contains("TransmissionVector::FoodWater"));
    assert!(exposure.contains("protected_dose"));
}
