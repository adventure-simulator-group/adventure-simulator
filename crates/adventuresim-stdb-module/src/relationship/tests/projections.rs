#[test]
fn relationship_projection_is_effective_dated_by_personal_frontier() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let projection = source
        .split("pub fn backend_character_relationship_statuses")
        .nth(1)
        .unwrap()
        .split("pub struct SocializingReceipt")
        .next()
        .unwrap();
    assert!(projection.contains("observer_minute"));
    assert!(projection.contains("marriage.married_minute <= observer_minute"));
    assert!(projection.contains("row.conceived_minute <= observer_minute"));
    assert!(projection.contains("receipt.attempted_minute <= observer_minute"));
    assert!(projection.contains("row.due_minute <= observer_minute"));
}

#[test]
fn discovery_projection_is_gateway_and_observer_scoped() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let projection = source
        .split("pub fn backend_courtship_discoveries")
        .nth(1)
        .unwrap()
        .split("pub struct SocializingReceipt")
        .next()
        .unwrap();
    assert!(projection.contains("is_strategic_gateway"));
    assert!(projection.contains("observer_character_id"));
    assert!(projection.contains("receipt.attempted_minute <= observer_minute"));
}

#[test]
fn born_child_projection_is_not_an_active_pregnancy_projection() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let projection = source
        .split("pub fn backend_character_relationship_statuses")
        .nth(1)
        .unwrap()
        .split("pub fn backend_courtship_discoveries")
        .next()
        .unwrap();
    assert!(projection.contains("row.conceived_minute <= observer_minute"));
    assert!(projection.contains("resolved > observer_minute"));
    assert!(projection.contains("row.status == PregnancyStatus::Born"));
    assert!(projection.contains("pregnancy_child_id: born_child_id"));
}
