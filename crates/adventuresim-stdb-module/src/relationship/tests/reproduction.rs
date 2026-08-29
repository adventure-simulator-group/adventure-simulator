#[test]
fn birth_uses_reserved_identity_and_constructs_age_zero() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let birth = source
        .split("pub fn settle_due_births")
        .nth(1)
        .unwrap()
        .split("fn socializing_id")
        .next()
        .unwrap();
    assert!(birth.contains("pregnancy.reserved_child_id"));
    assert!(birth.contains("NpcLifeFacts {"));
    assert!(birth.contains("age_years: 0"));
    assert!(birth.contains("record_character_birth"));
    assert!(birth.contains("household_id_at(ctx, mother.id, pregnancy.due_minute)"));
    assert!(birth.contains("occupant_holding_id_at("));
    assert!(birth.contains("holding_active_at("));
    assert!(birth.contains("move_residence_occupant_effective"));
    assert!(!birth.contains("pregnancy.birth_residence_holding_id"));
    assert!(!birth.contains("child.age_years = 0"));
    assert!(birth.contains("active_pregnancy()"));
    assert!(birth.contains(".delete(pregnancy.mother_id)"));
}

#[test]
fn spouse_leisure_is_simultaneous_conserved_and_idempotent() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let settlement = source
        .split("fn settle_spouse_leisure_pair")
        .nth(1)
        .unwrap()
        .split("pub fn apply_spouse_leisure_conception")
        .next()
        .unwrap();
    assert!(settlement.contains("joint_leisure_minutes("));
    assert!(settlement.contains("conception_quantum_plan("));
    assert!(settlement.contains("spouse_leisure_overlap().id().find"));
    assert!(settlement.contains(".conception_trial_receipt()"));
    assert!(settlement.contains(".find(&receipt_id)"));
    assert!(settlement.contains("refresh_spouse_pair_morale"));
}

#[test]
fn spouse_morale_is_awarded_to_both_and_respects_combined_cap() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let morale = source
        .split("fn refresh_spouse_pair_morale")
        .nth(1)
        .unwrap()
        .split("fn settle_spouse_leisure_pair")
        .next()
        .unwrap();
    assert!(morale.contains("for character_id in [first_id, second_id]"));
    assert!(morale.contains("refresh_bounded_leisure_morale"));
    assert!(morale.contains("SPOUSE_LEISURE_MORALE_SPEC"));
}
