//! Settlement route assembly and behavior-domain handlers.

//! Route registration stays in this facade. Handler modules own their forms,
//! policy, database reads, rendering adapters, and behavior-local tests.

#[cfg(test)]
pub(crate) const SETTLEMENTS_SOURCE: &str = concat!(
    include_str!("router.rs"),
    include_str!("medical.rs"),
    include_str!("overview.rs"),
    include_str!("camp.rs"),
    include_str!("service_quests.rs"),
    include_str!("party/location_personal.rs"),
    include_str!("party/cooking.rs"),
    include_str!("party/herbalism.rs"),
    include_str!("party/training_activity.rs"),
    include_str!("party/inventory_medical.rs"),
    include_str!("party/social.rs"),
    include_str!("party/transfers.rs"),
    include_str!("religion_party.rs"),
    include_str!("commerce.rs"),
    include_str!("rest.rs"),
    include_str!("religion.rs"),
    include_str!("rendering.rs"),
    include_str!("encumbrance.rs"),
    include_str!("rest_preview.rs"),
    include_str!("social_tests.rs"),
    include_str!("rest_tests.rs"),
    include_str!("herbalist_tests.rs"),
    include_str!("encumbrance_tests.rs"),
);

include!("router.rs");

mod medical {
    use super::*;
    include!("medical.rs");
}
mod overview {
    use super::*;
    include!("overview.rs");
}
mod camp {
    use super::*;
    include!("camp.rs");
}
mod service_quests {
    use super::*;
    include!("service_quests.rs");
}
mod party {
    use super::*;
    include!("party/mod.rs");
    include!("social_tests.rs");
}
mod commerce {
    use super::*;
    include!("commerce.rs");
}
mod rest {
    use super::*;
    include!("rest.rs");
    include!("rest_tests.rs");
    include!("herbalist_tests.rs");
}
mod religion {
    use super::*;
    include!("religion.rs");
    include!("religion_party.rs");
}
mod rendering {
    use super::*;
    include!("rendering.rs");
}
mod encumbrance {
    use super::*;
    include!("encumbrance.rs");
    include!("encumbrance_tests.rs");
}
mod rest_preview {
    use super::AppState;
    use crate::spacetimedb::{
        Character, CharacterFilth, CharacterPersonality, InventoryItem, InventoryItemAmount,
        ItemDefinition, PartyInventoryItem, PartyItemAmount,
    };
    use crate::templates::settlement::SoapRestPreview;
    include!("rest_preview.rs");
}

#[cfg(test)]
use camp::camp_continue_block_reason;
use camp::{
    bandage_camp_counterparty, camp, change_camp_destination, contact_camp_counterparty,
    continue_camp_travel, resolve_camp_encounter, resolve_errantry_road_challenge, rest_at_camp,
    surrender_camp_counterparty, update_camp_travel_configuration, update_travel_configuration,
};
use commerce::{
    inn, merchant_provider_id, merchant_service_location, provisioning_storefront_path,
    rest_at_settlement_map,
};
use encumbrance::{
    EncumbranceRows, camp_entry_redirect, get_active_character, get_character_capability,
    inventory_encumbrance_summaries, personal_encumbrance,
};
use medical::{
    alchemy, change_residence, perform_surgery, retrieve_repair, retrieve_repairs,
    schedule_allocation_reducer_arg, settlement_resident_place, show_settlement,
    show_settlement_location, submit_all_repairs, submit_repair, surgery,
};
use overview::settlement_map;
use party::{
    LocationLookup, character_is_at_location, chat_with_party_member, cook_food,
    deposit_party_inventory, discard_inventory_items, finalize_merchant_offer,
    finalize_party_offer, liquidate_party_assets, merchants, party_member, party_personal,
    party_pool_inventory, party_social, party_stats, perform_immediate_activity,
    perform_social_action, prepare_herbal_remedy, remove_party_member, render_party_personal,
    render_party_stats, resolve_location, set_automatic_social_chat, set_equipment,
    set_inventory_target, stop_preparation, transfer_party_item, update_training_schedule,
    withdraw_party_inventory,
};
use religion::{religion_dialogue, renounce_religion, resolve_religious_demand, set_religion};
use rendering::{
    inventory_trade_context, merchant_shop, personal_inventory_targets, render_service_page,
};
use rest::{
    armor, bookstore, clothing, herbalist, purchase_from_herbalist, query_local_reputation,
    query_single, religion, rest, settlement_action_service_available, travel, weapons,
};
#[cfg(test)]
use rest_preview::{calculate_rest_supply_availability, calculate_soap_rest_preview};
use service_quests::{
    begin_service_apprenticeship, clear_presented_organization, service_quest_offers,
    update_organization_presentation,
};

pub(crate) use camp::travel_provision_forecast;
pub(crate) use encumbrance::{get_active_party_members, get_combat_training_profile};
pub(crate) use party::medical_presentation;
pub(crate) use rest::{RestForm, field_shelter_argument, travel_rest_minutes};
pub(crate) use rest_preview::soap_rest_preview;
pub(crate) use service_quests::living_party_members;
