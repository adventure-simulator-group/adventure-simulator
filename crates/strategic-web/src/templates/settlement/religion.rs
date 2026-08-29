//! Religion service and personal conviction-demand presentation.

use maud::{Markup, html};

use super::rest::{SoapRestPreview, rest_default_minutes};
use super::service::service_page;
use crate::spacetimedb::{
    CharacterCondition, CharacterLimbs, CharacterStats, CharacterView, FoodLot, InventoryItem,
    SettlementView,
};
use crate::templates::sidebar_section;

/// Church interface.
#[expect(
    clippy::too_many_arguments,
    reason = "the religion page boundary composes independent settlement, party, and service projections"
)]
pub fn religion_page(
    settlement: &SettlementView,
    active_character: Option<&CharacterView>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::CatalogItemView],
    food_lots: &[FoodLot],
    party_members: &[CharacterView],
    limbs: Option<&CharacterLimbs>,
    stats: Option<&CharacterStats>,
    condition: Option<&CharacterCondition>,
    field_repair_minutes: u64,
    smith_wait_minutes: u64,
    soap_preview: SoapRestPreview,
    logged_in_as: Option<&str>,
) -> Markup {
    service_page(
        settlement,
        "religion",
        "Church",
        "Priest",
        "Faith, donations, and divine services require the religion and reputation systems.",
        active_character,
        inventory,
        items,
        food_lots,
        party_members,
        logged_in_as,
        rest_default_minutes(
            limbs,
            stats,
            condition,
            field_repair_minutes,
            smith_wait_minutes,
        ),
        None,
        soap_preview,
    )
}
pub(super) fn religious_demand_rail(
    demand: &crate::spacetimedb::ReligiousDemand,
    location_path: &str,
    character_id: u64,
) -> Markup {
    let action = format!(
        "{location_path}/party/{character_id}/religious-demand/{}",
        demand.id
    );
    html! {
        (sidebar_section("Conviction demands", html! {
            article class="religious-demand" {
                h3 { (&demand.title) }
                p { (&demand.description) }
                p class="text-muted small-copy" {
                    "Observe and bear the practical cost, or decline. Party Command automatically reduces the morale cost of neglect and can remove it entirely."
                }
                form method="post" action=(action) class="religious-demand-actions" {
                    button type="submit" name="choice" value="observe" class="btn btn-primary" { "Observe" }
                    button type="submit" name="choice" value="refuse" class="btn btn-danger" { "Do not observe" }
                }
            }
        }))
    }
}
