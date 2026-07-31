#[cfg(test)]
mod healing_tests {
    use super::{
        CaseAuthority, CaseFinaleAuthority, CaseResolutionStatus, CaseSiteAuthority, CaseSiteId,
        FinaleKind, FinaleStatus, HostileGroupAuthority, HostileGroupDisposition,
        HostileResolutionKind, IncidentStatus, LocalChatMessage, MissionApproachCapability,
        MissionAttemptStatus, MissionAuthority, MissionOutcomeCandidate, QuestGenerationAuthority,
        RecruitmentOffer, RecruitmentOfferBindingFields, RecruitmentOfferId,
        RecruitmentOfferStatus, RecruitmentSourceId, STRATEGIC_SOURCE, activity_incident_source_id,
        autoresolve_drop, carrying_capacity_multiplier_for_condition,
        case_refs_have_exact_dialogue_provenance, generated_case_site_combat_eligible,
        generated_dialogue_action_matches, generated_dialogue_producer_recipient,
        generated_scene_key, generated_witness_visible_description, hostile_group_authority_row,
        hostile_resolution_for_objective, incident_group_matches, merchant_storefront,
        mission_candidate_from_capability, npc_conversation_authority_matches,
        player_participant_ids, project_local_chat_message, quest_encounter_archetype,
        quest_generation_context_commitment, recruitment_offer_binding_fields_are_live,
        refreshed_recruitment_offer_status, renewed_recruitment_offer_expiry,
        sample_mission_candidate, sanitized_encounter_body_weight, settlement_activity_stage_error,
        unique_default_merchant_provider, validate_quest_generation_authority,
        validated_generated_dialogue_manifest,
    };
    use adventuresim_core::encounter::EncounterArchetype;
    use std::collections::HashSet;

    include!("tests/combat_party.rs");
    include!("tests/authority_trade_dialogue.rs");
    include!("tests/generated_world.rs");
}
