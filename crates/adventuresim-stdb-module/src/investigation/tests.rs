#[cfg(test)]
mod tests {
    use super::{
        PersistedBestiaryLoreResult, PhysicalEvidenceInspectionActionReceipt,
        PhysicalEvidenceInspectionAttempt, augment_physical_evidence_inspection,
        derive_bestiary_deductions, inspection_action_receipt_matches, parse_bestiary_lore_results,
        successful_bestiary_lore_results,
    };
    use adventuresim_core::quest_generation::BestiaryEvidenceImplication;
    use adventuresim_world_schema::BestiaryCategory;

    include!("tests/bestiary_and_corrections.rs");
    include!("tests/projection_and_referrals.rs");
    include!("tests/sites_and_actions.rs");
    include!("tests/action_authority.rs");
}
