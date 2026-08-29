#[test]
fn seeded_family_contract_has_unique_roles_and_canonical_edges() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let seed = source
        .split("pub fn ensure_seeded_family_households")
        .nth(1)
        .unwrap()
        .split("fn father_of")
        .next()
        .unwrap();
    assert!(seed.contains("residents.chunks(4)"));
    assert!(seed.contains("HouseholdRole::Head"));
    assert!(seed.contains("HouseholdRole::Spouse"));
    assert!(seed.contains("KinshipKind::Parent"));
    assert!(seed.contains("KinshipKind::Sibling"));
    assert!(seed.contains("ensure_character_family_role"));
    assert!(seed.contains("seeded:{settlement_id}:{cohort}"));
    assert!(seed.contains(".character_personality()"));
    assert!(seed.contains(".character_id()"));
    assert!(seed.contains(".update(personality)"));
}

#[test]
fn marriage_preserves_birth_family_and_birth_copies_it_to_the_child() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let wedding = source
        .split("pub fn settle_due_weddings")
        .nth(1)
        .unwrap()
        .split("pub fn settle_due_weddings_global")
        .next()
        .unwrap();
    assert!(!wedding.contains("ensure_character_family_role"));
    assert!(!wedding.contains("delete_character_social_roles"));
    let birth = source
        .split("pub fn settle_due_births")
        .nth(1)
        .unwrap()
        .split("pub fn settle_due_births_global")
        .next()
        .unwrap();
    assert!(birth.contains("insert_character_with_origin"));
    assert!(birth.contains("copy_birth_family_roles"));
}
