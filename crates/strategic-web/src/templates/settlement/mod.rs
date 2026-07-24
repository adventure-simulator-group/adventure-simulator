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

use adventuresim_core::{
    activity::{PRAYER_MORALE_LIMIT, PRAYER_MORALE_SCALE_MINUTES, settlement_population_scale},
    bestiary::ThreatId,
    equipment::EncumbranceSummary,
    prelude::Skill,
    strategic_schedule::{
        BASELINE_FATIGUE_PER_DAY, CombatTrainingProfile, DailySchedule,
        FATIGUE_RESERVOIR_PER_PREVIEW_POINT, LABOR_FATIGUE_PER_HOUR,
        LEISURE_FATIGUE_RECOVERY_PER_HOUR, LEISURE_MORALE_LIMIT, LEISURE_MORALE_SCALE_FATIGUE,
        LeisureOutcome, settlement_leisure_outcome,
    },
    strategic_time::{ItinerarySegment, ItinerarySegmentKind, MINUTES_PER_DAY},
};
use adventuresim_world_schema::OfficialReligion;
use maud::{Markup, html};
use std::{collections::BTreeSet, fmt, str::FromStr};

use super::inventory_browser::{InventoryBrowser, InventoryColumnSet};
use super::{
    camp_location_layout_with_session, decorative_game_icon, empty_state, game_icon,
    item_display_name, item_type_header, item_type_icon, population_description,
    quest_location_layout_with_session, religion_icon, settlement_layout_with_session,
    sidebar_section, stat_icon_path,
};
use crate::medical::MedicalPresentation;
use crate::routes::travel::{TravelDestination, TravelProvisionForecast};
use crate::spacetimedb::{
    Character, CharacterApprenticeship, CharacterAttributes, CharacterCapability,
    CharacterCondition, CharacterEquip, CharacterLimbs, CharacterSkills, CharacterStats,
    CharacterStrategicCondition, CharacterTrainingSchedule, ContractPresentation, FoodLot,
    InventoryItem, InventoryQuantityTarget, ItemDefinition, ItemSlot, JourneyTerrainKind,
    LimbInjury, LimbRegion, Party, PartyInventoryItem, PartyJourney, PartyJourneyItinerary,
    PartyJourneyRoute, ProjectileKind, RetainedProjectile, ScheduleAllocation, Settlement,
    SettlementAlias, SettlementCategory, SettlementDescription, SettlementDescriptionKind,
    StrategicEncounter,
};

mod character_details;
mod character_health;
mod character_skills;
mod chrome;
mod context;
mod rest;
mod social;
mod trade;
mod travel;

pub(crate) use character_details::{character_stats_panel, character_visual_preview};
pub use character_details::{party_personal_page, party_stats_page};
pub use character_health::surgery_dialog;
pub use character_skills::ActivityPreviewRates;
pub(crate) use chrome::{party_portrait_overlay, settlement_description, visual_stage};
pub use chrome::{settlement_npc_location_page, settlement_overview_page};
pub use context::{LocationKind, LocationView};
pub(crate) use rest::{party_rest_menu, rest_default_minutes};
pub use rest::{RestSummary, SoapRestPreview, rest_result_page};
pub(crate) use social::settlement_chat_area_with_info;
pub use social::{SocialPresentation, party_social_dialog};
pub use trade::{
    MerchantShop, alchemy_page, live_merchant_shop_page, merchants_page, party_discard_page,
    party_inventory_page, party_pool_page, religion_page,
};
pub(crate) use travel::{
    CampTravelDestination, map_destination_detail, map_destination_list_with_rest,
    travel_preferences_form,
};
pub use travel::{camp_page, settlement_map_page};

#[cfg(test)]
pub(super) mod test_support {
    use super::*;
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
            summary: Some("Active quest".to_string()),
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
            quest_in_progress: true,
            provision_forecast: None,
            terrain_route: None,
            return_terrain_route: None,
            route_fallback: true,
        }
    }
}
