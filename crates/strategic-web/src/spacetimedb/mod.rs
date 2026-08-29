//! SpacetimeDB HTTP client module

mod client;
mod queries;
mod types;

pub(crate) use client::{Result, SpacetimeClient, SpacetimeError};
pub(crate) use queries::{
    SqlQuery, automatic_social_chat_by_id, autoresolve_report_by_battle_id,
    battle_result_by_battle_id, case_site_pin_by_case_site_id, character_affinity_by_id,
    character_attributes_by_character_id, character_by_id, character_capability_by_character_id,
    character_case_site_location_by_character_id, character_condition_by_character_id,
    character_death_by_character_id, character_familiarity_by_id, character_limbs_by_character_id,
    character_needs_by_character_id, character_personality_by_character_id,
    character_relationship_status_by_character_id, character_residence_status_by_character_id,
    character_skills_by_character_id, character_stats_by_character_id,
    character_strategic_condition_by_character_id, character_time_by_character_id,
    character_training_schedule_by_character_id, contract_by_id, fireplace_dish_by_station_key,
    fireplace_station_by_key, forage_attempt_state_by_character_id, inventory_item_by_id,
    inventory_object_by_id, item_by_id, organization_presentation_by_character_id,
    party_action_request_by_id, party_by_id, party_journey_by_party_id,
    party_journey_route_by_party_id, party_recruitment_role_by_id, settlement_by_id,
    settlement_resident_by_character_id, settlement_resident_presence_by_character_id,
    settlement_smith_by_settlement_id, social_address_by_id, sql_string_literal,
    strategic_encounter_by_party_id, tactical_server_by_mission_id,
    tactical_server_request_by_mission_id, weapon_holder_instance_by_physical_object_id,
    weapon_instance_by_physical_object_id, world_clock_singleton,
};
pub use types::*;

/// SpacetimeDB's raw HTTP reducer API represents algebraic `Option<T>` values
/// as sum variants rather than Serde's scalar-or-null representation.
pub(crate) fn sats_option<T: serde::Serialize>(value: Option<T>) -> serde_json::Value {
    match value {
        Some(value) => serde_json::json!({ "some": value }),
        None => serde_json::json!({ "none": [] }),
    }
}

/// Supplies the exact schema name for a unit variant at the raw SATS boundary.
pub(crate) trait SatsUnitVariant {
    fn sats_name(self) -> &'static str;
}

impl SatsUnitVariant for adventuresim_core::physiology::BodyRegion {
    fn sats_name(self) -> &'static str {
        use adventuresim_core::physiology::BodyRegion;

        match self {
            BodyRegion::LeftArm => "leftArm",
            BodyRegion::RightArm => "rightArm",
            BodyRegion::LeftLeg => "leftLeg",
            BodyRegion::RightLeg => "rightLeg",
            BodyRegion::Chest => "chest",
            BodyRegion::Abdomen => "abdomen",
            BodyRegion::Head => "head",
        }
    }
}

impl SatsUnitVariant for adventuresim_core::physiology::InterventionRoute {
    fn sats_name(self) -> &'static str {
        use adventuresim_core::physiology::InterventionRoute;

        match self {
            InterventionRoute::Oral => "oral",
            InterventionRoute::Topical => "topical",
            InterventionRoute::Inhaled => "inhaled",
            InterventionRoute::Injected => "injected",
        }
    }
}

impl SatsUnitVariant for adventuresim_core::surgery::SurgeryProcedure {
    fn sats_name(self) -> &'static str {
        use adventuresim_core::surgery::SurgeryProcedure;

        match self {
            SurgeryProcedure::Bandage => "bandage",
            SurgeryProcedure::Stitch => "stitch",
            SurgeryProcedure::Splint => "splint",
            SurgeryProcedure::RemoveSplint => "removeSplint",
            SurgeryProcedure::Extract => "extract",
            SurgeryProcedure::OpenBody => "openBody",
        }
    }
}

/// SpacetimeDB's raw HTTP reducer API represents unit enum variants as a
/// single-key sum object. Domain types own the exact schema-name mapping above.
pub(crate) fn sats_unit_variant(variant: impl SatsUnitVariant) -> serde_json::Value {
    serde_json::json!({ (variant.sats_name()): {} })
}

#[cfg(test)]
mod tests {
    use super::{sats_option, sats_unit_variant};

    #[test]
    fn reducer_options_use_spacetimedb_sum_encoding() {
        assert_eq!(
            sats_option(Some("digest")),
            serde_json::json!({ "some": "digest" })
        );
        assert_eq!(sats_option(Some(73_u64)), serde_json::json!({ "some": 73 }));
        assert_eq!(sats_option::<u64>(None), serde_json::json!({ "none": [] }));
    }

    #[test]
    fn reducer_unit_variants_use_spacetimedb_sum_encoding() {
        assert_eq!(
            sats_unit_variant(adventuresim_core::physiology::BodyRegion::LeftArm),
            serde_json::json!({ "leftArm": {} })
        );
        assert_eq!(
            sats_unit_variant(adventuresim_core::physiology::InterventionRoute::Oral),
            serde_json::json!({ "oral": {} })
        );
        assert_eq!(
            sats_unit_variant(adventuresim_core::surgery::SurgeryProcedure::RemoveSplint),
            serde_json::json!({ "removeSplint": {} })
        );
        assert_eq!(
            sats_unit_variant(adventuresim_core::surgery::SurgeryProcedure::OpenBody),
            serde_json::json!({ "openBody": {} })
        );
    }
}
