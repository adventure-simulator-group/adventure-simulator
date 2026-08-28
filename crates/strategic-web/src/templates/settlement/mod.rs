//! Settlement templates.
//!
//! Settlement pages deliberately keep the same ownership model: services and
//! settlement-owned information on the left, service context in the center,
//! and the active player's party on the right.
//!
//! The facade keeps the established `templates::settlement` API while the
//! implementation is grouped by presentation concern: location context,
//! shared page chrome, character details, skills, health, social interaction,
//! travel, trade, and rest.

mod character_details;
mod character_health;
mod character_skills;
mod chrome;
mod context;
mod rest;
mod social;
mod trade;
mod travel;

pub(crate) use character_details::{
    CharacterSheetView, character_sheet_markup, character_stats_panel, character_visual_preview,
};
pub use character_details::{party_personal_page, party_stats_page};
pub use character_health::{corpse_medical_dialog, surgery_dialog};
pub use character_skills::ActivityPreviewRates;
pub(crate) use character_skills::{CharacterSheetActions, CharacterSkillHours};
pub(crate) use chrome::{
    CharacterPortraitView, character_portrait_overlay, party_portrait_overlay,
    settlement_description,
};
pub use chrome::{
    ChildPresentation, RelationshipPresentation, WeddingPresentation, settlement_overview_page,
    settlement_residence_page, settlement_resident_location_page,
};
pub use context::{LocationKind, LocationView};
pub use rest::{RestSummary, SoapRestPreview, rest_result_page};
pub(crate) use rest::{party_rest_menu, rest_default_minutes, rest_service_menu};
pub(crate) use social::settlement_chat_area_with_info;
pub use social::{SocialFeedback, SocialPresentation, party_social_dialog};
pub use trade::{
    MerchantShop, fireplace_page, live_merchant_shop_page, merchants_page, party_discard_page,
    party_inventory_page, party_pool_page, religion_page,
};
pub(in crate::templates) use trade::{
    inventory_footer_controls, item_name_with_food_lot, item_name_with_quality, transfer_glyph,
};
pub(crate) use travel::{
    CampTravelDestination, map_destination_detail, map_destination_list_with_rest,
    travel_preferences_form,
};
pub use travel::{camp_page, settlement_map_page};

#[cfg(test)]
pub(super) mod test_support {
    use crate::routes::travel::TravelDestination;
    use crate::spacetimedb::*;

    pub(super) fn settlement() -> Settlement {
        Settlement {
            id: "viabundus-1".into(),
            name: "Lübeck".into(),
            coord_x: 10.0,
            coord_y: 53.0,
            population_level: 4,
            population_estimate: 12_000,
            category: crate::spacetimedb::SettlementCategory::City,
            languages: adventuresim_world_schema::SettlementLanguageProfile {
                east_central_bp: 2_000,
                west_central_bp: 2_000,
                low_bp: 6_000,
                yiddish_incidence_bp: 75,
            },
            industries: adventuresim_world_schema::InferredIndustryProfile::new(vec![
                adventuresim_world_schema::IndustryEvidence::Fallback(
                    adventuresim_world_schema::FallbackIndustry::CroplandGrain,
                ),
            ])
            .unwrap(),
            economy: adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder(),
            religious_status: adventuresim_world_schema::SettlementReligiousStatus::Established {
                religion: adventuresim_world_schema::OfficialReligion::RomanCatholic,
            },
            scene_key: "hills".into(),
            religion_id: "western_church".into(),
            currency_id: "lubeck_mark".into(),
            source_node_id: Some(1),
        }
    }

    pub(super) fn quest_destination() -> TravelDestination {
        TravelDestination {
            id: "quest-location".to_string(),
            name: "Bandit camp".to_string(),
            description: "A camp beside the road.".to_string(),
            summary: Some("Reported exact location".to_string()),
            travel_action: "/quests/quest-location/travel".to_string(),
            track_action: Some("/case-sites/quest-location/track".to_string()),
            tracked: false,
            distance_m: 1_000,
            journey_minutes: 48,
            camp_stop_minutes: Vec::new(),
            camp_forecasts: Vec::new(),
            departure_minute: 0,
            itinerary_total_elapsed_minutes: 96,
            itinerary_segments: Vec::new(),
            round_trip_destination: true,
            case_site_knowledge: Some(
                crate::routes::travel::CaseSiteKnowledgePresentation::ReportedExactLocation,
            ),
            active_contract_destination: false,
            provision_forecast: None,
            terrain_route: None,
            return_terrain_route: None,
            uses_straight_line_estimate: true,
        }
    }
}
