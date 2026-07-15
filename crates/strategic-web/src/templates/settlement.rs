//! Settlement templates.
//!
//! Settlement pages deliberately keep the same ownership model: services and
//! settlement-owned information on the left, service context in the center,
//! and the active player's party on the right.

use maud::{Markup, html};
use std::{fmt, str::FromStr};

use super::{
    empty_state, population_description, quest_location_layout_with_session,
    settlement_layout_with_session, sidebar_section,
};
use crate::routes::travel::TravelDestination;
use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterCapability, CharacterEquip, CharacterLimbs,
    CharacterSkills, CharacterStrategicCondition, CharacterTrainingSchedule, InventoryItem,
    InventoryQuantityTarget, Party, PartyInventoryItem, Settlement,
};

#[derive(Clone, Debug)]
pub struct LocationView {
    pub kind: LocationKind,
    pub id: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationKind {
    Settlement,
    Quest,
}

impl LocationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Settlement => "settlement",
            Self::Quest => "quest",
        }
    }
}

impl fmt::Display for LocationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LocationKind {
    type Err = ();
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "settlement" => Ok(Self::Settlement),
            "quest" => Ok(Self::Quest),
            _ => Err(()),
        }
    }
}

impl LocationView {
    pub fn base_path(&self) -> String {
        format!("/locations/{}/{}", self.kind, self.id)
    }

    fn render_layout(
        &self,
        title: &str,
        content: Markup,
        logged_in_as: Option<&str>,
        theme: &str,
    ) -> Markup {
        if self.kind == LocationKind::Settlement {
            settlement_layout_with_session(
                title,
                &self.name,
                &self.id,
                "",
                content,
                logged_in_as,
                theme,
            )
        } else {
            quest_location_layout_with_session(
                title,
                &self.name,
                &self.id,
                "",
                content,
                logged_in_as,
                theme,
            )
        }
    }
}

/// The currently available merchant storefronts. They share trade mechanics,
/// but each storefront limits the stock shown on its left-hand side.
#[derive(Clone, Copy)]
pub enum MerchantShop {
    General,
    Weapons,
    Armor,
    Clothing,
}

pub struct RestSummary {
    pub days: u64,
    pub gold_spent: u32,
    pub healed: Vec<(String, f32)>,
    pub trained: Vec<(String, f32)>,
}

impl MerchantShop {
    pub fn service_id(self) -> &'static str {
        match self {
            Self::General => "merchants",
            Self::Weapons => "weapons",
            Self::Armor => "armor",
            Self::Clothing => "clothing",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::General => "Market Square",
            Self::Weapons => "Weaponsmith",
            Self::Armor => "Armourer",
            Self::Clothing => "Tailor",
        }
    }

    fn stocks(self, kind: crate::spacetimedb::ItemKind) -> bool {
        match self {
            Self::General => kind != crate::spacetimedb::ItemKind::Currency,
            Self::Weapons => kind == crate::spacetimedb::ItemKind::Weapon,
            Self::Armor => matches!(
                kind,
                crate::spacetimedb::ItemKind::Armor | crate::spacetimedb::ItemKind::Shield
            ),
            Self::Clothing => kind == crate::spacetimedb::ItemKind::Clothing,
        }
    }
}

/// Settlement information and the next destinations on the imported road and
/// ferry network.
pub fn settlement_overview_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Settlement", html! {
                div class="settlement-summary" {
                    dl class="location-stat-list" {
                        div { dt { "Population" } dd { (format_population(settlement)) } }
                        div { dt { "Size" } dd { (population_description(settlement.population_level)) } }
                        div { dt { "Coordinates" } dd { (format!("{}, {}", settlement.coord_x as i32, settlement.coord_y as i32)) } }
                    }
                }
            }))
        }
        main class="center-content settlement-main settlement-overview" {
            (party_portrait_overlay(party_members, active_character, &format!("/locations/settlement/{}", settlement.id), None))
            (visual_stage("map", &settlement.name, "TODO: settlement image"))
            (settlement_chat_area(&settlement.name, active_character))
        }
        aside class="right-sidebar" {
            (sidebar_section("Description", html! {
                p { (settlement_description(settlement.population_level)) }
            }))
        }
    };
    settlement_layout_with_session(
        &settlement.name,
        &settlement.name,
        &settlement.id,
        "",
        content,
        logged_in_as,
        theme,
    )
}

pub fn settlement_map_page(
    settlement: &Settlement,
    destinations: &[TravelDestination],
    selected_id: Option<&str>,
    active_character: Option<&Character>,
    party_members: &[Character],
    can_travel: bool,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let selected = selected_id.and_then(|id| destinations.iter().find(|entry| entry.id == id));
    let base_path = format!("/locations/settlement/{}/map", settlement.id);
    let content = html! {
        (map_destination_list(destinations, selected_id, &base_path))
        main class="center-content settlement-main settlement-overview" {
            (party_portrait_overlay(party_members, active_character, &format!("/locations/settlement/{}", settlement.id), None))
            (visual_stage("map", &settlement.name, "TODO: settlement map"))
            (settlement_chat_area(&settlement.name, active_character))
        }
        (map_destination_detail(selected, can_travel))
    };
    settlement_layout_with_session(
        &format!("{} map", settlement.name),
        &settlement.name,
        &settlement.id,
        "map",
        content,
        logged_in_as,
        theme,
    )
}

pub(crate) fn map_destination_list(
    destinations: &[TravelDestination],
    selected_id: Option<&str>,
    base_path: &str,
) -> Markup {
    html! {
        aside class="left-sidebar" {
            (sidebar_section("Destinations", html! {
                @if destinations.is_empty() {
                    (empty_state("No destinations are available from this location.", None, None))
                } @else {
                    nav class="location-destination-list" aria-label="Travel destinations" {
                        @for destination in destinations {
                            a href=(format!("{}?destination={}", base_path, destination.id))
                                class=(if selected_id == Some(destination.id.as_str()) { "list-item active" } else { "list-item" }) {
                                strong { (&destination.name) }
                                @if destination.quest_in_progress {
                                    span class="destination-quest-badge" title="Active quest destination"
                                        aria-label="Active quest destination" { "!" }
                                } @else if destination.turn_in_ready {
                                    span class="destination-turn-in-badge" title="Quest ready to turn in here"
                                        aria-label="Quest ready to turn in here" { "!" }
                                }
                                span class="text-muted small-copy" { (format_distance(destination.distance_m)) }
                            }
                        }
                    }
                }
            }))
        }
    }
}

pub(crate) fn map_destination_detail(
    selected: Option<&TravelDestination>,
    can_travel: bool,
) -> Markup {
    html! {
        aside class="right-sidebar" {
            @if let Some(destination) = selected {
                (sidebar_section(&destination.name, html! {
                    @if can_travel {
                        form method="post" action=(&destination.travel_action) {
                            button type="submit" class="btn btn-primary btn-block" { "Travel" }
                        }
                    }
                    p { (&destination.description) }
                    p class="text-muted small-copy" {
                        @if let Some(summary) = &destination.summary { (summary) " · " }
                        (format_distance(destination.distance_m))
                        " · " (format_journey_time(destination.journey_minutes))
                    }
                }))
            } @else {
                (sidebar_section("Destination", html! {
                    p class="text-muted small-copy" { "Select a destination to inspect it and plan travel." }
                }))
            }
        }
    }
}

pub(crate) fn settlement_description(population_level: i32) -> &'static str {
    match population_level {
        i32::MIN..=1 => "A quiet cluster of farmsteads and cottages.",
        2 => "A quaint hamlet gathered around a well-worn road.",
        3 => "A modest village serving the surrounding countryside.",
        4 => "A busy market town with a steady flow of travelers.",
        5 => "A prosperous town enclosed by crowded streets.",
        _ => "A large and bustling city whose streets rarely fall silent.",
    }
}

fn format_distance(distance_m: u64) -> String {
    format!("{:.1} km", distance_m as f64 / 1_000.0)
}

fn format_population(settlement: &Settlement) -> String {
    match settlement.population_estimate {
        0 => population_description(settlement.population_level).to_string(),
        population => format!("approximately {}", format_number(population)),
    }
}

fn format_number(value: u32) -> String {
    let digits = value.to_string();
    let first_group = match digits.len() % 3 {
        0 => 3,
        remainder => remainder,
    };
    let mut formatted = digits[..first_group].to_string();
    for group in digits[first_group..].as_bytes().chunks(3) {
        formatted.push(',');
        formatted.push_str(std::str::from_utf8(group).expect("population digits are valid UTF-8"));
    }
    formatted
}

fn format_journey_time(minutes: u64) -> String {
    let hours = minutes / 60;
    let minutes = minutes % 60;
    if hours == 0 {
        format!("{minutes} min")
    } else if minutes == 0 {
        format!("{hours} h")
    } else {
        format!("{hours} h {minutes} min")
    }
}

/// Market interface. Inventory and prices are intentionally UI-only placeholders
/// until settlement-owned inventory and trade reducers exist.
pub fn merchants_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement,
        "merchants",
        "Market Square",
        "Market Steward",
        "Merchant stock and prices will appear here once the trade backend is available.",
        active_character,
        inventory,
        party_members,
        logged_in_as,
        theme,
        None,
        None,
    )
}

/// Inn interface placeholder.
pub fn inn_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    limbs: Option<&CharacterLimbs>,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement,
        "inn",
        "The Inn",
        "Innkeeper",
        "Rest duration, recovery, training, and strategic time advancement are not connected yet.",
        active_character,
        inventory,
        party_members,
        logged_in_as,
        theme,
        limbs.map(days_to_full_health),
        None,
    )
}

/// Church placeholder.
pub fn religion_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    limbs: Option<&CharacterLimbs>,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement,
        "religion",
        "Church",
        "Priest",
        "Faith, donations, and divine services require the religion and reputation systems.",
        active_character,
        inventory,
        party_members,
        logged_in_as,
        theme,
        limbs.map(days_to_full_health),
        None,
    )
}

/// Party inventory comparison.
pub fn party_inventory_page(
    location: &LocationView,
    selected: &Character,
    selected_inventory: &[InventoryItem],
    active_character: &Character,
    active_inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    selected_equip: Option<&CharacterEquip>,
    active_equip: Option<&CharacterEquip>,
    selected_targets: &[InventoryQuantityTarget],
    active_targets: &[InventoryQuantityTarget],
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (party_trade_inventory_rail(selected, selected_inventory, items, active_character.id, "right", selected_equip, active_targets))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(selected.id)))
            (visual_stage("npc", &selected.name, &format!("TODO: {} portrait", selected.name.to_lowercase())))
            (player_chat_area(selected, active_character))
            form id="party-offer" class="party-offer" action=(format!("{}/party/{}/inventory/offer", location.base_path(), selected.id)) method="post" hidden {
                button type="button" class="party-offer-cancel" data-cancel-trade="party" { "Cancel" }
                button type="submit" disabled { "Offer" }
            }
        }
        aside class="right-sidebar" {
            (party_trade_inventory_rail(active_character, active_inventory, items, selected.id, "left", active_equip, selected_targets))
        }
    };
    location.render_layout("Party", content, Some(&active_character.name), theme)
}

/// The active character's inventory with a staged discard list.
pub fn party_discard_page(
    location: &LocationView,
    active_character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    equip: Option<&CharacterEquip>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Discard", html! {
                p class="text-muted small-copy" data-discard-empty { "Stage carried items here before discarding them." }
                table class="trade-inventory-table" data-discard-table hidden {
                    (trade_inventory_table_header(false))
                    tbody data-discard-list {}
                }
            }))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(active_character.id)))
            (visual_stage("npc", &active_character.name, &format!("TODO: {} portrait", active_character.name.to_lowercase())))
            (settlement_chat_area(&active_character.name, Some(active_character)))
            form id="inventory-discard" class="party-offer"
                action=(format!("{}/party/{}/inventory/discard", location.base_path(), active_character.id))
                method="post" hidden {
                button type="button" class="party-offer-cancel" data-cancel-trade="discard" { "Cancel" }
                button type="submit" disabled { "Discard" }
            }
        }
        aside class="right-sidebar" {
            (discard_inventory_rail(active_character, inventory, items, equip))
        }
    };
    location.render_layout("Inventory", content, Some(&active_character.name), theme)
}

/// Active character's combined strategic view.
pub fn party_personal_page(
    location: &LocationView,
    active_character: &Character,
    party_members: &[Character],
    capability: Option<&CharacterCapability>,
    attributes: Option<&CharacterAttributes>,
    skills: Option<&CharacterSkills>,
    limbs: Option<&CharacterLimbs>,
    condition: Option<&CharacterStrategicCondition>,
    schedule: Option<&CharacterTrainingSchedule>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (character_summary_rail(capability))
            (party_attributes_rail("Your attributes", attributes, limbs))
            @let schedule_action = format!("{}/party/{}/schedule", location.base_path(), active_character.id);
            (party_skills_rail("Your skills", skills, limbs, schedule, Some(&schedule_action)))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(active_character.id)))
            (visual_stage("npc", &active_character.name, &format!("TODO: {} portrait", active_character.name.to_lowercase())))
            (settlement_chat_area(&active_character.name, Some(active_character)))
        }
        aside class="right-sidebar" {
            (strategic_condition_rail(condition))
            (character_bio_rail(active_character))
        }
    };
    location.render_layout("Party", content, Some(&active_character.name), theme)
}

/// Selected party member stats and biography.
pub fn party_stats_page(
    location: &LocationView,
    selected: &Character,
    active_character: &Character,
    party_members: &[Character],
    capability: Option<&CharacterCapability>,
    selected_attributes: Option<&CharacterAttributes>,
    selected_skills: Option<&CharacterSkills>,
    selected_limbs: Option<&CharacterLimbs>,
    condition: Option<&CharacterStrategicCondition>,
    active_party: Option<&Party>,
    selected_party: Option<&Party>,
    theme: &str,
) -> Markup {
    let selected_attributes_title = format!("{}'s attributes", selected.name);
    let selected_skills_title = format!("{}'s skills", selected.name);
    let content = html! {
        aside class="left-sidebar" {
            (character_summary_rail(capability))
            (party_attributes_rail(&selected_attributes_title, selected_attributes, selected_limbs))
            (party_skills_rail(&selected_skills_title, selected_skills, selected_limbs, None, None))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(selected.id)))
            (visual_stage("npc", &selected.name, &format!("TODO: {} portrait", selected.name.to_lowercase())))
            (player_chat_area(selected, active_character))
        }
        aside class="right-sidebar" {
            (strategic_condition_rail(condition))
            (character_bio_rail(selected))
            @if selected.id != active_character.id {
                @if active_character.party_id == selected.party_id {
                    @if active_party.is_some_and(|party| party.leader_id == selected.id) {
                        (sidebar_section("Party", html! {
                            form method="post" action=(format!("{}/party/{}/remove", location.base_path(), active_character.id)) {
                                button type="submit" class="btn btn-danger btn-block" { "Leave party" }
                            }
                        }))
                    } @else {
                        (sidebar_section("Party", html! {
                            form method="post" action=(format!("{}/party/{}/remove", location.base_path(), selected.id)) {
                                button type="submit" class="btn btn-danger btn-block" {
                                    @if active_party.is_some_and(|party| party.leader_id == active_character.id) { "Kick from party" }
                                    @else { "Request kick" }
                                }
                            }
                        }))
                    }
                } @else if let Some(party) = selected_party {
                    (sidebar_section("Party", html! {
                        p { (&party.name) }
                        form method="post" action=(format!("/parties/{}/join-general", party.id)) {
                            button type="submit" class="btn btn-primary btn-block" { "Request to join party" }
                        }
                    }))
                }
            }
        }
    };
    location.render_layout("Party stats", content, Some(&active_character.name), theme)
}

fn service_page(
    settlement: &Settlement,
    service_id: &str,
    title: &str,
    npc_name: &str,
    todo: &str,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
    healing_days: Option<u16>,
    rest_summary: Option<&RestSummary>,
) -> Markup {
    let trade_offers: Option<(&str, &[&str])> = match service_id {
        "merchants" => Some((
            "Merchant stock",
            &["Weapon offer", "Armour offer", "Provision offer"],
        )),
        "weapons" => Some((
            "Weapons",
            &["Weapon offer", "Shield offer", "Ammunition offer"],
        )),
        "armor" => Some((
            "Armour",
            &["Head protection", "Torso protection", "Limb protection"],
        )),
        "clothing" => Some((
            "Clothing",
            &["Travel attire", "Cold-weather clothing", "Fine clothing"],
        )),
        "inn" => Some((
            "Inn supplies",
            &["Rations", "Water", "Supplies", "Bed for the night"],
        )),
        _ => None,
    };
    let content = html! {
        aside class=(if service_id == "inn" || service_id == "religion" { "left-sidebar service-left-sidebar" } else { "left-sidebar" }) {
            @if service_id == "inn" {
                div class="service-left-stack" {
                    div class="service-inventory-area" { (merchant_offers_rail("Inn supplies", &["Rations", "Water", "Supplies", "Bed for the night"])) }
                    (rest_service_menu("Inn", &settlement.id, "inn", healing_days, rest_summary))
                }
            } @else if service_id == "religion" {
                div class="service-left-stack" {
                    div class="service-inventory-area" {
                        (sidebar_section("Church services", html! {
                            @if active_character.is_some() {
                                form method="post" action=(format!("/settlements/{}/religion", settlement.id)) {
                                    label for="religion-id" { "Conviction" }
                                    select id="religion-id" name="religion_id" {
                                        option value="" { "No religious conviction" }
                                        option value="western_church" { "Western Church" }
                                        option value="reformed" { "Reformed Church" }
                                        option value="old_faith" { "Old Faith" }
                                    }
                                    button type="submit" class="btn btn-primary btn-block mt-1" { "Set conviction" }
                                }
                            }
                            p class="text-muted small-copy" { "Shared conviction strengthens allied Charisma. Conflicting conviction turns that influence into a morale penalty." }
                        }))
                    }
                    (rest_service_menu("Temple", &settlement.id, "temple", healing_days, rest_summary))
                }
            } @else if let Some((stock_title, offers)) = trade_offers {
                (merchant_offers_rail(stock_title, offers))
            } @else {
                (sidebar_section("Settlement offerings", html! {
                    div class="service-placeholder-list" {
                        span { "Inventory / offers" }
                        span class="badge badge-warning" { "TODO" }
                    }
                    p class="text-muted small-copy" { (todo) }
                }))
            }
        }
        main class="center-content settlement-main" {
            (party_portrait_overlay(party_members, active_character, &format!("/locations/settlement/{}", settlement.id), None))
            (visual_stage("npc", npc_name, &format!("TODO: {} portrait", npc_name.to_lowercase())))
            (settlement_service_chat_area(
                title,
                active_character,
                &settlement.id,
                service_id,
            ))
        }
        aside class="right-sidebar" {
            @if trade_offers.is_some() {
                (inventory_rail(
                    active_character,
                    inventory,
                    Some(("Sell", "TODO: selling requires merchant pricing and trade reducers")),
                    matches!(service_id, "weapons" | "armor" | "clothing"),
                ))
            } @else if service_id == "smith" {
                (inventory_rail(
                    active_character,
                    inventory,
                    Some(("Repair", "TODO: repairs require durability, pricing, and smithing reducers")),
                    true,
                ))
            } @else if service_id == "religion" {
                (inventory_rail(active_character, inventory, None, false))
            } @else {
                (sidebar_section("Service", html! {
                    p class="text-muted small-copy" { (todo) }
                }))
            }
        }
    };
    settlement_layout_with_session(
        title,
        &settlement.name,
        &settlement.id,
        service_id,
        content,
        logged_in_as,
        theme,
    )
}

fn party_trade_inventory_rail(
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    recipient_id: u64,
    direction: &str,
    equip: Option<&CharacterEquip>,
    recipient_targets: &[InventoryQuantityTarget],
) -> Markup {
    let title = format!("{}'s inventory", character.name);
    html! {
        (sidebar_section(&title, html! {
            @if inventory.is_empty() {
                p class="text-muted small-copy" { "No items carried." }
            } @else {
                (trade_inventory_table(true, html! {
                    @for item in inventory {
                        @let is_equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let target = target_quantity(recipient_targets, &item.item_id);
                            tr class=(if direction == "left" { "trade-inventory-row trade-row-player" } else { "trade-inventory-row trade-row-merchant" }) data-item-key=(&item.item_id) {
                                td class="inventory-item-name" {
                                    (&item.item_id)
                                    @if !is_equipped { span class="inventory-row-actions" { @for (mode, arrows) in [("one",1),("target",2),("all",3)] { button type="button" class=(format!("trade-transfer trade-transfer-{direction} party-draft-transfer")) data-from=(character.id) data-to=(recipient_id) data-item=(item.id) data-key=(&item.item_id) data-count=(item.qty) data-target=(target) data-transfer-mode=(mode) aria-label=(format!("Transfer {}", item.item_id)) title="Stage items for trade" { (transfer_glyph(arrows)) } } } }
                                }
                                td class="inventory-count" { (item.qty) }
                                td class="inventory-equipped" { input type="checkbox" checked[is_equipped] disabled; }
                                td class="inventory-weight" { (item_weight(definition)) }
                                td class="inventory-gold" { (item_value(definition)) }
                            }
                    }
                }))
                (inventory_footer_controls(if direction == "left" { "party-left" } else { "party-right" }, "Transfer to targets", "Transfer everything"))
            }
        }))
    }
}

fn discard_inventory_rail(
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    equip: Option<&CharacterEquip>,
) -> Markup {
    let title = format!("{}'s inventory", character.name);
    html! {
        (sidebar_section(&title, html! {
            @if inventory.is_empty() {
                p class="text-muted small-copy" { "No items carried." }
            } @else {
                (trade_inventory_table(true, html! {
                    @for item in inventory {
                        @let is_equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        tr class="trade-inventory-row trade-row-player" data-discard-source=(item.id) data-item-key=(&item.item_id) {
                            td class="inventory-item-name" {
                                (&item.item_id)
                                @if !is_equipped {
                                    button type="button" class="trade-transfer trade-transfer-left"
                                        data-discard-item=(item.id) data-count=(item.qty)
                                        aria-label=(format!("Discard {}", item.item_id))
                                        title="Stage one item for discarding" {}
                                }
                            }
                            td class="inventory-count" { (item.qty) }
                            td class="inventory-equipped" { input type="checkbox" checked[is_equipped] disabled; }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
            }
        }))
    }
}

pub fn live_merchant_shop_page(
    settlement: &Settlement,
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    equip: Option<&CharacterEquip>,
    personal_targets: &[InventoryQuantityTarget],
    party_targets: &[InventoryQuantityTarget],
    pooled: &[PartyInventoryItem],
    theme: &str,
    shop: MerchantShop,
) -> Markup {
    let title = shop.title();
    let service_id = shop.service_id();
    let content = html! {
        aside class="left-sidebar" { (sidebar_section("Merchant stock", html! {
            (trade_inventory_table(false, html! {
                @for item in items.iter().filter(|item| shop.stocks(item.kind)) {
                    @let buy_price = (item.base_value.unwrap_or(1) as f32 * 1.375).ceil() as u32;
                    @let sell_price = (item.base_value.unwrap_or(1) as f32 / 1.25).floor().max(1.0) as u32;
                    @let target = target_quantity(personal_targets, &item.id);
                    tr class="trade-inventory-row trade-row-merchant" data-merchant-item=(&item.id) data-merchant-sell-price=(sell_price) { td class="inventory-item-name" { (&item.id) (merchant_buy_controls(&item.id, buy_price, target, 999)) } td class="inventory-count" { "999" } td class="inventory-weight" { (weight_display(item.weight)) } td class="inventory-gold" { (buy_price) } }
                }
            }))
            (inventory_footer_controls("buy", "Buy to targets", "Buy everything"))
        })) }
        main class="center-content settlement-main" { (party_portrait_overlay(party_members, Some(character), &format!("/locations/settlement/{}", settlement.id), None)) (visual_stage("npc", title, &format!("TODO: {} portrait", title.to_lowercase()))) (settlement_service_chat_area(title, Some(character), &settlement.id, service_id)) form # "merchant-offer" class="party-offer" action=(format!("/settlements/{}/merchants/offer", settlement.id)) method="post" hidden { input type="hidden" name="return_to" value=(service_id); input type="hidden" name="inventory_scope" value="player"; button type="button" class="party-offer-cancel" data-cancel-trade="merchant" { "Cancel" } button type="submit" disabled { "Offer" } } }
        aside class="right-sidebar inventory-owner-panel" data-inventory-tabs {
            nav class="inventory-owner-tabs" aria-label="Trading inventory" {
                button type="button" class="inventory-owner-tab active" data-inventory-tab="player" { "Player" }
                button type="button" class="inventory-owner-tab" data-inventory-tab="party" { "Party" }
            }
            div data-inventory-pane="player" {
            div class="sidebar-section" {
                (trade_inventory_table(true, html! {
                    @for item in inventory {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let is_currency = definition.is_some_and(|definition| definition.kind == crate::spacetimedb::ItemKind::Currency);
                        @let is_equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                        @let sell_price = definition.map_or(0, |definition| (definition.base_value.unwrap_or(1) as f32 / 1.25).floor().max(1.0) as u32);
                        @let target = target_quantity(personal_targets, &item.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&item.item_id) data-merchant-equipped=(is_equipped) data-inventory-quantity=(item.qty) data-target=(target) {
                        td class="inventory-item-name" { (&item.item_id) @if !is_currency && !is_equipped { (merchant_sell_controls(item.id, &item.item_id, sell_price, item.qty, target)) } }
                        td class="inventory-count" { (quantity_target_control(item.qty, target, &item.item_id, false)) } td class="inventory-equipped" { input type="checkbox" checked[is_equipped] disabled; } td class="inventory-weight" { (item_weight(definition)) } td class="inventory-gold" { (sell_price) }
                    }}
                    @for target in personal_targets.iter().filter(|target| target.quantity > 0 && !inventory.iter().any(|item| item.item_id == target.item_id)) {
                        @let definition = items.iter().find(|definition| definition.id == target.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&target.item_id) data-inventory-quantity="0" data-target=(target.quantity) {
                            td class="inventory-item-name" { (&target.item_id) }
                            td class="inventory-count" { (quantity_target_control(0, target.quantity, &target.item_id, false)) }
                            td class="inventory-equipped" { input type="checkbox" disabled; }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
                (inventory_footer_controls("sell", "Sell surplus", "Sell everything"))
            }
            }
            div data-inventory-pane="party" hidden {
            div class="sidebar-section" {
                (trade_inventory_table(false, html! {
                    @for item in pooled {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let is_currency = definition.is_some_and(|definition| definition.kind == crate::spacetimedb::ItemKind::Currency);
                        @let sell_price = definition.map_or(0, |definition| (definition.base_value.unwrap_or(1) as f32 / 1.25).floor().max(1.0) as u32);
                        @let target = target_quantity(party_targets, &item.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&item.item_id) data-party-inventory-id=(item.id) data-inventory-quantity=(item.quantity) data-target=(target) {
                            td class="inventory-item-name" { (&item.item_id) @if !is_currency { (merchant_sell_controls(item.id, &item.item_id, sell_price, item.quantity, target)) } }
                            td class="inventory-count" { (quantity_target_control(item.quantity, target, &item.item_id, true)) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (sell_price) }
                        }
                    }
                    @for target in party_targets.iter().filter(|target| target.quantity > 0 && !pooled.iter().any(|item| item.item_id == target.item_id)) {
                        @let definition = items.iter().find(|definition| definition.id == target.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&target.item_id) data-inventory-quantity="0" data-target=(target.quantity) {
                            td class="inventory-item-name" { (&target.item_id) }
                            td class="inventory-count" { (quantity_target_control(0, target.quantity, &target.item_id, true)) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
                (inventory_footer_controls("sell", "Sell surplus", "Sell everything"))
            }
            }
        }
    };
    settlement_layout_with_session(
        title,
        &settlement.name,
        &settlement.id,
        service_id,
        content,
        Some(&character.name),
        theme,
    )
}

/// Two-sided transfer view for the equally owned party chest.
pub fn party_pool_page(
    location: &LocationView,
    character: &Character,
    inventory: &[InventoryItem],
    pooled: &[PartyInventoryItem],
    stake: u64,
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    equip: Option<&CharacterEquip>,
    personal_targets: &[InventoryQuantityTarget],
    party_targets: &[InventoryQuantityTarget],
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Party inventory", html! {
                div class="party-stake-summary" {
                    span { "Your available stake" }
                    strong { (stake) " gold" }
                }
                p class="small-copy text-muted" { "Withdrawals use your stake. Personal gold automatically covers an indivisible item's shortfall." }
                (trade_inventory_table(false, html! {
                    @for item in pooled {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let value = definition.and_then(|definition| definition.base_value).unwrap_or(0) as u64;
                        @let target = target_quantity(personal_targets, &item.item_id);
                        @let current = inventory.iter().find(|personal| personal.item_id == item.item_id).map_or(0, |personal| personal.qty);
                        tr class="trade-inventory-row" {
                            td class="inventory-item-name" {
                                (&item.item_id)
                                span class="inventory-row-actions" { @for (mode, arrows) in [("one",1),("target",2),("all",3)] { button type="button" class="trade-transfer trade-transfer-right" data-pool-stage=(item.id) data-pool-direction="withdraw" data-transfer-mode=(mode) data-count=(item.quantity) data-current=(current) data-target=(target) title=(if value > stake { format!("Withdraw; {} personal gold required", value - stake) } else { "Withdraw using your stake".to_string() }) aria-label=(format!("Withdraw {}", item.item_id)) { (transfer_glyph(arrows)) } } }
                            }
                            td class="inventory-count" { (quantity_target_control(item.quantity, target_quantity(party_targets, &item.item_id), &item.item_id, true)) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
                (inventory_footer_controls("withdraw", "Withdraw to personal targets", "Withdraw everything"))
            }))
        }
        main class="center-content settlement-main" {
            (party_portrait_overlay(party_members, Some(character), &location.base_path(), None))
            (visual_stage("npc", "Party chest", "Shared party inventory chest"))
            (settlement_chat_area("Party inventory", Some(character)))
        }
        aside class="right-sidebar" {
            (sidebar_section(&format!("{}'s inventory", character.name), html! {
                p class="small-copy text-muted" { "Add items at their objective gold value." }
                (trade_inventory_table(true, html! {
                    @for item in inventory {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                        @let target = target_quantity(party_targets, &item.item_id);
                        @let current = pooled.iter().find(|pooled| pooled.item_id == item.item_id).map_or(0, |pooled| pooled.quantity);
                        tr class="trade-inventory-row" {
                            td class="inventory-item-name" {
                                (&item.item_id)
                                @if !equipped {
                                    span class="inventory-row-actions" { @for (mode, arrows) in [("one",1),("target",2),("all",3)] { button type="button" class="trade-transfer trade-transfer-left" data-pool-stage=(item.id) data-pool-direction="deposit" data-transfer-mode=(mode) data-count=(item.qty) data-current=(current) data-target=(target) aria-label=(format!("Add {} to party inventory", item.item_id)) { (transfer_glyph(arrows)) } } }
                                }
                            }
                            td class="inventory-count" { (quantity_target_control(item.qty, target_quantity(personal_targets, &item.item_id), &item.item_id, false)) }
                            td class="inventory-equipped" { input type="checkbox" checked[equipped] disabled; }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
                (inventory_footer_controls("deposit", "Deposit to party targets", "Deposit everything"))
            }))
        }
        form method="post" action=(format!("{}/party-inventory/deposit", location.base_path())) id="pool-transfer-offer" class="party-offer" hidden { button type="button" data-cancel-pool class="party-offer-cancel" { "Cancel" } button type="submit" disabled { "Offer" } }
    };
    location.render_layout("Party inventory", content, Some(&character.name), theme)
}

fn item_weight(item: Option<&crate::spacetimedb::ItemDefinition>) -> String {
    item.map_or_else(|| "—".to_owned(), |item| weight_display(item.weight))
}

fn item_value(item: Option<&crate::spacetimedb::ItemDefinition>) -> String {
    item.and_then(|item| item.base_value)
        .map_or_else(|| "—".to_owned(), |value| value.to_string())
}

fn weight_display(weight: f32) -> String {
    let display = format!("{weight:.2}");
    display
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn trade_inventory_table(show_equipped: bool, rows: Markup) -> Markup {
    html! {
        table class="trade-inventory-table" {
            @if show_equipped {
                colgroup {
                    col class="inventory-column-item";
                    col class="inventory-column-count";
                    col class="inventory-column-equipped";
                    col class="inventory-column-weight";
                    col class="inventory-column-gold";
                }
            }
            (trade_inventory_table_header(show_equipped))
            tbody { (rows) }
        }
    }
}

fn target_quantity(targets: &[InventoryQuantityTarget], item_id: &str) -> u32 {
    targets
        .iter()
        .find(|target| target.item_id == item_id)
        .map_or(0, |target| target.quantity)
}

fn quantity_target_control(quantity: u32, target: u32, item_id: &str, party_scope: bool) -> Markup {
    html! {
        span class="inventory-target-control" data-target-control data-item-id=(item_id) data-party-scope=(party_scope) title=(format!("Carrying {quantity}; target {target}")) {
            span class="inventory-target-prefix" { (quantity) "/" }
            span class="inventory-target-denominator" {
                button type="button" class="inventory-target-step inventory-target-up" data-target-step="1" aria-label=(format!("Increase {} target", item_id)) { "⌃" }
                span class="inventory-target-value" data-target-value { (target) }
                button type="button" class="inventory-target-step inventory-target-down" data-target-step="-1" aria-label=(format!("Decrease {} target", item_id)) hidden[target == 0] { "⌄" }
            }
        }
    }
}

pub(crate) fn transfer_glyph(count: usize) -> Markup {
    html! { span class=(format!("inventory-transfer-glyph arrows-{count}")) aria-hidden="true" { @for _ in 0..count { i {} } } }
}

fn merchant_buy_controls(item_id: &str, price: u32, target: u32, available: u32) -> Markup {
    html! { span class="inventory-row-actions" {
        button type="button" class="trade-transfer trade-transfer-right" data-merchant-buy=(item_id) data-merchant-buy-price=(price) data-transfer-mode="one" aria-label=(format!("Buy one {item_id}")) { (transfer_glyph(1)) }
        button type="button" class="trade-transfer trade-transfer-right" data-merchant-buy=(item_id) data-merchant-buy-price=(price) data-transfer-mode="target" data-target=(target) data-count=(available) aria-label=(format!("Buy {item_id} to target")) { (transfer_glyph(2)) }
        button type="button" class="trade-transfer trade-transfer-right" data-merchant-buy=(item_id) data-merchant-buy-price=(price) data-transfer-mode="all" data-count=(available) aria-label=(format!("Buy all {item_id}")) { (transfer_glyph(3)) }
    } }
}

fn merchant_sell_controls(
    id: u64,
    item_id: &str,
    price: u32,
    quantity: u32,
    target: u32,
) -> Markup {
    html! { span class="inventory-row-actions" {
        @for (mode, arrows, label) in [("one", 1, "Sell one"), ("target", 2, "Sell surplus"), ("all", 3, "Sell all")] {
            button type="button" class="trade-transfer trade-transfer-left" data-merchant-sell=(id) data-item-name=(item_id) data-merchant-sell-price=(price) data-transfer-mode=(mode) data-count=(quantity) data-target=(target) aria-label=(format!("{label} {item_id}")) { (transfer_glyph(arrows)) }
        }
    } }
}

pub(crate) fn inventory_footer_controls(
    action: &str,
    target_label: &str,
    all_label: &str,
) -> Markup {
    html! { div class="inventory-footer-actions" {
        button type="button" class="trade-transfer inventory-footer-transfer" data-inventory-bulk=(action) data-transfer-mode="target" aria-label=(target_label) title=(target_label) { (transfer_glyph(2)) }
        button type="button" class="trade-transfer inventory-footer-transfer" data-inventory-bulk=(action) data-transfer-mode="all" aria-label=(all_label) title=(all_label) { (transfer_glyph(3)) }
    } }
}

fn trade_inventory_table_header(show_equipped: bool) -> Markup {
    html! { thead { tr {
        th scope="col" class="inventory-column-item" { "Item" }
        th scope="col" class="inventory-column-count" { "#" }
        @if show_equipped { th scope="col" class="inventory-column-equipped" title="Equipped" { "✓" } }
        th scope="col" class="inventory-column-weight" title="Weight" { span class="inventory-header-weight" aria-label="Weight" {} }
        th scope="col" class="inventory-column-gold" title="Gold" { span class="inventory-header-coin" aria-label="Gold" {} }
    } } }
}

fn party_skills_rail(
    title: &str,
    skills: Option<&CharacterSkills>,
    limbs: Option<&CharacterLimbs>,
    schedule: Option<&CharacterTrainingSchedule>,
    schedule_action: Option<&str>,
) -> Markup {
    let head_health = limbs.map_or(1.0, |limbs| limbs.head_health);
    let upper_health = limbs.map_or(1.0, |limbs| {
        (limbs.left_arm_health + limbs.right_arm_health) / 2.0
    });
    let lower_health = limbs.map_or(1.0, |limbs| {
        (limbs.left_leg_health + limbs.right_leg_health) / 2.0
    });
    html! {
        (sidebar_section(title, html! {
            @if let Some(skills) = skills {
                @if let (Some(schedule), Some(action)) = (schedule, schedule_action) {
                    form class="skill-schedule" data-skill-schedule action=(action) method="post" {
                        (skills_table(skills, head_health, upper_health, lower_health, Some(schedule)))
                    }
                    script src="/static/training-schedule.js?v=hourglass-handle-1" {}
                } @else {
                    (skills_table(skills, head_health, upper_health, lower_health, None))
                }
            } @else {
                p class="text-muted small-copy" { "Skill records have not been created yet." }
            }
        }))
    }
}

fn skills_table(
    skills: &CharacterSkills,
    head_health: f32,
    upper_health: f32,
    lower_health: f32,
    schedule: Option<&CharacterTrainingSchedule>,
) -> Markup {
    html! {
            table class="party-skills-table" {
                colgroup {
                    col class="party-skill-icon-column";
                    col class="party-skill-name-column";
                    col class="party-skill-meter-column";
                    @if schedule.is_some() {
                        col class="party-skill-time-column";
                    }
                }
                tbody {
                    (party_skill_row("Will", "will", skills.will_hours, 5_000.0, head_health, schedule.map(|s| s.will_minutes)))
                    (party_skill_row("Charisma", "charisma", skills.charisma_hours, 20_000.0, head_health, schedule.map(|s| s.charisma_minutes)))
                    (party_skill_row("Medicine", "medicine", skills.medicine_hours, 10_000.0, head_health, schedule.map(|s| s.medicine_minutes)))
                    (party_skill_row("Faith", "faith", skills.faith_hours, 5_000.0, head_health, schedule.map(|s| s.faith_minutes)))
                    (party_skill_row("Melee", "melee", skills.melee_hours, 8_000.0, upper_health, schedule.map(|s| s.melee_minutes)))
                    (party_skill_row("Ranged", "ranged", skills.ranged_hours, 15_000.0, upper_health, schedule.map(|s| s.ranged_minutes)))
                    (party_skill_row("Dodge", "dodge", skills.dodge_hours, 20_000.0, lower_health, schedule.map(|s| s.dodge_minutes)))
                    (party_skill_row("Block", "block", skills.block_hours, 12_000.0, upper_health, schedule.map(|s| s.block_minutes)))
                    (party_skill_row("Stealth", "stealth", skills.stealth_hours, 8_000.0, upper_health, schedule.map(|s| s.stealth_minutes)))
                    (party_skill_row("Balance", "balance", skills.balance_hours, 30_000.0, lower_health, schedule.map(|s| s.balance_minutes)))
                    (party_skill_row("Surgeon", "surgeon", skills.surgeon_hours, 10_000.0, upper_health, schedule.map(|s| s.surgeon_minutes)))
                    @if let Some(schedule) = schedule {
                        tr class="schedule-divider" { td colspan="4" {} }
                        (schedule_special_row("Labor", "clothing", "labor_minutes", schedule.labor_minutes, true))
                        (schedule_special_row("Leisure", "inn", "leisure_minutes", 0, false))
                    }
            }
        }
    }
}

fn party_skill_row(
    name: &str,
    icon: &str,
    hours: f32,
    half_hours: f32,
    health: f32,
    schedule_minutes: Option<u16>,
) -> Markup {
    let rank = 5.0 * hours / (hours + half_hours);
    let effective_rank = rank * health.clamp(0.0, 1.0);
    let current_width = (effective_rank.clamp(0.0, 5.0) / 5.0) * 100.0;
    let damage_width = ((rank - effective_rank).max(0.0) / 5.0) * 100.0;
    let invested_hours = hours.max(0.0).floor() as u64;
    html! {
        tr class="party-skill-row" {
            td class="party-skill-icon-cell" { (stat_icon(name, "skills", icon)) }
            td class="party-skill-name" { (name) }
            td class="party-skill-meter" {
                div class="skill-rank-bar" title=(format!("{invested_hours} hours invested")) {
                    span class="rank-current" style=(format!("width:{current_width:.1}%")) {}
                    span class="rank-damage" style=(format!("left:{current_width:.1}%;width:{damage_width:.1}%")) {}
                    span class="skill-rank-value" style=(format!("left:{current_width:.1}%")) { (format!("{effective_rank:.1}")) }
                    @if let Some(minutes) = schedule_minutes {
                        (schedule_handle(name, &format!("{}_minutes", icon), minutes))
                    }
                }
            }
            @if let Some(minutes) = schedule_minutes {
                td class="party-skill-allocation" data-schedule-value=(format!("{}_minutes", icon)) {
                    (schedule_step_button("Decrease daily allocation", -15))
                    span data-schedule-display { (format_schedule_hours(minutes)) }
                    (schedule_step_button("Increase daily allocation", 15))
                }
            }
        }
    }
}

fn schedule_special_row(
    label: &str,
    icon: &str,
    name: &str,
    minutes: u16,
    editable: bool,
) -> Markup {
    html! {
        tr class="party-skill-row schedule-special-row" {
            td class="party-skill-icon-cell" { (schedule_icon(label, icon)) }
            td class="party-skill-name" { strong { (label) } }
            td class="party-skill-meter" {
                div class="skill-rank-bar schedule-special-track" {
                @if editable {
                    (schedule_handle(label, name, minutes))
                } @else {
                    span class="schedule-leisure-fill" data-leisure-fill {}
                }
                }
            }
            td class="party-skill-allocation" data-schedule-value=(name) {
                @if editable {
                    (schedule_step_button("Decrease daily allocation", -15))
                    span data-schedule-display { (format_schedule_hours(minutes)) }
                    (schedule_step_button("Increase daily allocation", 15))
                } @else { span data-schedule-display { "0h" } }
            }
        }
    }
}

fn schedule_step_button(label: &str, delta: i16) -> Markup {
    html! {
        button type="button" class=(if delta < 0 { "schedule-step schedule-step-decrease" } else { "schedule-step schedule-step-increase" })
            data-schedule-step=(delta) aria-label=(label) {}
    }
}

fn schedule_icon(label: &str, icon: &str) -> Markup {
    html! {
        span
            class="stat-icon schedule-special-icon"
            style=(format!("--stat-icon: url('/static/icons/strategic/{icon}.png')"))
            role="img"
            aria-label=(label)
            title=(label)
        {}
    }
}

fn schedule_handle(label: &str, name: &str, minutes: u16) -> Markup {
    html! {
        input type="hidden" name=(name) value=(minutes) data-schedule-input;
        button type="button" class="schedule-handle" data-schedule-handle data-schedule-name=(name)
            aria-label=(format!("{} daily allocation", label)) aria-valuemin="0" aria-valuemax="1440"
            aria-valuenow=(minutes) title=(format!("{} per day", format_schedule_hours(minutes)))
            data-on:pointerdown="scheduleDrag.start(el, evt)" data-on:keydown="scheduleDrag.key(el, evt)" {}
    }
}

fn format_schedule_hours(minutes: u16) -> String {
    let rounded = ((u32::from(minutes) + 7) / 15) * 15;
    let hours = rounded / 60;
    let fraction = match rounded % 60 {
        0 => "",
        15 => "¼",
        30 => "½",
        45 => "¾",
        _ => unreachable!("rounded schedule minute must be a quarter hour"),
    };
    format!("{hours}{fraction}h")
}

fn character_summary_rail(capability: Option<&CharacterCapability>) -> Markup {
    let tags = capability
        .map(CharacterCapability::summary_tags)
        .unwrap_or_default();
    html! {
        (sidebar_section("Summary", html! {
            @if tags.is_empty() {
                p class="text-muted small-copy" { "No notable capabilities." }
            } @else {
                div class="character-summary-tags" aria-label="Character capability summary" {
                    @for tag in tags { span class="character-summary-tag" { (tag) } }
                }
            }
        }))
    }
}

pub(crate) fn character_stats_panel(
    character: &Character,
    capability: Option<&CharacterCapability>,
    attributes: Option<&CharacterAttributes>,
    skills: Option<&CharacterSkills>,
    limbs: Option<&CharacterLimbs>,
) -> Markup {
    html! {
        (character_summary_rail(capability))
        (party_attributes_rail(&format!("{}'s attributes", character.name), attributes, limbs))
        (party_skills_rail(&format!("{}'s skills", character.name), skills, limbs, None, None))
    }
}

pub(crate) fn character_visual_preview(character: &Character) -> Markup {
    visual_stage(
        "npc",
        &character.name,
        &format!("TODO: {} portrait", character.name.to_lowercase()),
    )
}

fn character_bio_rail(character: &Character) -> Markup {
    html! {
        (sidebar_section("Bio", html! {
            dl class="character-bio" {
                div { dt { "Age" } dd { (character.age_years) " years" } }
            }
        }))
    }
}

fn strategic_condition_rail(condition: Option<&CharacterStrategicCondition>) -> Markup {
    let Some(condition) = condition else {
        return html! {};
    };
    let percent = |value: f32| format!("{:.0}%", value.max(0.0) * 100.0);
    html! {
        (sidebar_section("Condition", html! {
            dl class="character-bio strategic-condition-summary" {
                div { dt { "Status" } dd { (condition.status.to_uppercase()) } }
                div { dt { "Morale" } dd { (format!("{:.1}", condition.morale)) } }
                div { dt { "Positive morale" } dd { (format!("+{:.1}", condition.positive_morale)) } }
                div { dt { "Negative morale" } dd { (format!("-{:.1}", condition.negative_morale)) } }
                div { dt { "Incapacitation" } dd { (percent(condition.incapacitation)) } }
                div { dt { "Pain" } dd { (percent(condition.pain)) } }
                div { dt { "Blood loss" } dd { (percent(condition.blood_loss)) } }
                div { dt { "Fear" } dd { (percent(condition.fear)) } }
                div { dt { "Fatigue" } dd { (percent(condition.fatigue)) } }
                div { dt { "Check effectiveness" } dd { (percent(condition.check_multiplier)) } }
            }
        }))
    }
}

fn party_attributes_rail(
    title: &str,
    attributes: Option<&CharacterAttributes>,
    limbs: Option<&CharacterLimbs>,
) -> Markup {
    let Some(attributes) = attributes else {
        return html! {};
    };
    let head_health = limbs.map_or(1.0, |limbs| limbs.head_health);
    let chest_health = limbs.map_or(1.0, |limbs| limbs.chest_health);
    let stomach_health = limbs.map_or(1.0, |limbs| limbs.stomach_health);
    let left_arm_health = limbs.map_or(1.0, |limbs| limbs.left_arm_health);
    let right_arm_health = limbs.map_or(1.0, |limbs| limbs.right_arm_health);
    let left_leg_health = limbs.map_or(1.0, |limbs| limbs.left_leg_health);
    let right_leg_health = limbs.map_or(1.0, |limbs| limbs.right_leg_health);
    html! {
        (sidebar_section(title, html! {
            div class="party-attributes-list" aria-label="Character attributes" {
                (attribute_group("Head", head_health, &[
                    ("Intelligence", "intelligence", attributes.intelligence),
                    ("Instinct", "instinct", attributes.instinct),
                    ("Eyesight", "eyesight", attributes.eyesight),
                    ("Hearing", "hearing", attributes.hearing),
                ]))
                (attribute_group("Chest", chest_health, &[
                    ("Endurance", "endurance", attributes.endurance),
                ]))
                (attribute_group("Stomach", stomach_health, &[
                    ("Immunity", "immunity", attributes.immunity),
                    ("Gut", "gut", attributes.gut),
                ]))
                div class="limb-attribute-pair" {
                    (limb_attribute_column("Left arm", "limb-left", left_arm_health, &[
                        ("Strength", "strength-arm", attributes.left_arm_strength),
                        ("Agility", "agility-arm", attributes.left_arm_agility),
                    ]))
                    (limb_attribute_column("Right arm", "limb-right", right_arm_health, &[
                        ("Strength", "strength-arm", attributes.right_arm_strength),
                        ("Agility", "agility-arm", attributes.right_arm_agility),
                    ]))
                }
                div class="limb-attribute-pair" {
                    (limb_attribute_column("Left leg", "limb-left", left_leg_health, &[
                        ("Strength", "strength-leg", attributes.left_leg_strength),
                        ("Agility", "agility-leg", attributes.left_leg_agility),
                    ]))
                    (limb_attribute_column("Right leg", "limb-right", right_leg_health, &[
                        ("Strength", "strength-leg", attributes.right_leg_strength),
                        ("Agility", "agility-leg", attributes.right_leg_agility),
                    ]))
                }
            }
        }))
    }
}

fn limb_attribute_column(
    name: &str,
    side: &str,
    health: f32,
    rows: &[(&str, &str, f32)],
) -> Markup {
    attribute_group_with_labels(name, health, rows, false, Some(side))
}

fn attribute_group(name: &str, health: f32, rows: &[(&str, &str, f32)]) -> Markup {
    attribute_group_with_labels(name, health, rows, true, None)
}

fn attribute_group_with_labels(
    name: &str,
    health: f32,
    rows: &[(&str, &str, f32)],
    show_labels: bool,
    side: Option<&str>,
) -> Markup {
    let health = health.clamp(0.0, 1.0);
    let health_width = health * 100.0;
    let damage_width = (1.0 - health) * 100.0;
    html! {
        div class=(match side {
            Some(side) => format!("attribute-group limb-attribute-column {side}"),
            None => "attribute-group".to_owned(),
        }) {
            div class="attribute-group-heading" { (name) }
            div class="attribute-health-bar" title=(format!("{name} health: {health_width:.0}%")) {
                span class="attribute-health-current" style=(format!("width:{health_width:.1}%")) {}
                span class="attribute-health-damage" style=(format!("left:{health_width:.1}%;width:{damage_width:.1}%")) {}
            }
            @for (attribute_name, icon, value) in rows {
                (attribute_row(attribute_name, icon, *value, health, show_labels))
            }
        }
    }
}

fn attribute_row(name: &str, icon: &str, value: f32, health: f32, show_label: bool) -> Markup {
    let effective_value = value * health.clamp(0.0, 1.0);
    let current_width = (effective_value.clamp(0.0, 5.0) / 5.0) * 100.0;
    let damage_width = ((value - effective_value).max(0.0) / 5.0) * 100.0;
    html! {
        div class=(if show_label { "party-attribute-row" } else { "party-attribute-row party-attribute-icon-only" }) {
            (stat_icon(name, "attributes", icon))
            @if show_label { span class="party-attribute-name" { (name) } }
            div class="attribute-rank-bar" title=(format!("{effective_value:.1}")) {
                span class="rank-current" style=(format!("width:{current_width:.1}%")) {}
                span class="rank-damage" style=(format!("left:{current_width:.1}%;width:{damage_width:.1}%")) {}
            }
        }
    }
}

fn stat_icon(label: &str, category: &str, icon: &str) -> Markup {
    html! {
        span
            class=(format!("stat-icon stat-icon-{icon}"))
            style=(format!("--stat-icon: url('/static/icons/stats/{category}/{icon}.png')"))
            role="img"
            aria-label=(label)
            title=(label)
        {}
    }
}

pub(crate) fn visual_stage(kind: &str, title: &str, placeholder: &str) -> Markup {
    html! {
        figure class=(format!("service-visual service-visual-{}", kind)) {
            div class="service-visual-placeholder" role="img" aria-label=(placeholder) {
                @if kind == "map" {
                    span class="visual-symbol" { "⌖" }
                    span class="visual-label" { "Map placeholder" }
                } @else {
                    span class="visual-symbol" { (title.chars().next().unwrap_or('?')) }
                    span class="visual-label" { "Portrait placeholder" }
                }
            }
        }
    }
}

pub(crate) fn party_portrait_overlay(
    party_members: &[Character],
    active_character: Option<&Character>,
    location_path: &str,
    selected_character_id: Option<u64>,
) -> Markup {
    let members: Vec<&Character> = if party_members.is_empty() {
        active_character.into_iter().collect()
    } else {
        party_members.iter().collect()
    };
    let leader_id = members.first().map(|member| member.id);

    html! {
        @if !members.is_empty() {
            div class="party-portrait-overlay" aria-label="Active party" {
                div data-party-portrait-members {
                @if active_character.is_some() {
                    div class="party-portrait party-inventory-portrait" title="Party inventory" {
                        a class="party-portrait-select" href=(format!("{}/party-inventory", location_path)) {
                            span class="party-portrait-initial party-chest-face" role="img" aria-label="Party inventory" { "▣" }
                        }
                    }
                }
                @for member in members {
                    @let is_active = active_character.is_some_and(|character| character.id == member.id);
                    @let can_remove = Some(member.id) != leader_id;
                    div class=(if selected_character_id == Some(member.id) { "party-portrait active" } else { "party-portrait" })
                        data-character-id=(member.id)
                        data-active-character[is_active]
                        title=(&member.name) {
                        a class="party-portrait-select"
                            href=(if is_active {
                                format!("{}/party/{}", location_path, member.id)
                            } else {
                                format!("{}/party/{}/stats", location_path, member.id)
                            })
                            title=(format!("Inspect {}", member.name)) {
                            (incapacitation_wheel(member.id))
                            span class="party-portrait-initial" {
                                span class="party-portrait-face" { (member.name.chars().next().unwrap_or('?')) }
                                span class="party-portrait-name" { (&member.name) }
                            }
                        }
                        span class="party-portrait-actions" aria-label=(format!("Actions for {}", member.name)) {
                            a href=(format!("{}/party/{}/inventory", location_path, member.id))
                                class="party-portrait-action"
                                title=(if is_active { "Open inventory and discard items".to_string() } else { format!("Compare inventory with {}", member.name) }) {
                                span class="party-action-icon"
                                    style="--party-action-icon: url('/static/icons/character/inventory.png')"
                                    role="img" aria-label="Inventory" {}
                            }
                            @if can_remove {
                                form method="post" action=(format!("{}/party/{}/remove", location_path, member.id)) {
                                    button type="submit" class=(if is_active { "party-portrait-action party-member-remove party-member-leave" } else { "party-portrait-action party-member-remove party-member-kick-request" })
                                        title=(if is_active { "Leave party".to_string() } else { format!("Request to remove {} from the party", member.name) })
                                        aria-label=(if is_active { "Leave party".to_string() } else { format!("Request to remove {} from the party", member.name) }) {
                                        span aria-hidden="true" { "×" }
                                    }
                                }
                            }
                        }
                    }
                }
                }
            }
        }
    }
}

fn incapacitation_wheel(character_id: u64) -> Markup {
    html! {
        span class="incapacitation-wheel"
            data-strategic-condition-wheel=(character_id)
            role="img"
            aria-label="Loading strategic condition"
            title="Loading strategic condition" {}
    }
}

/// Layout-only chat panel matching the UX prototype. Channels, message history,
/// and sending are deliberately disabled until the strategic chat backend exists.
pub(crate) fn settlement_chat_area(location: &str, active_character: Option<&Character>) -> Markup {
    chat_area(location, active_character, None, None)
}

fn player_chat_area(subject: &Character, active_character: &Character) -> Markup {
    let context = ("player", subject.id.to_string());
    chat_area(&subject.name, Some(active_character), None, Some(context))
}

fn settlement_service_chat_area(
    location: &str,
    active_character: Option<&Character>,
    settlement_id: &str,
    service_id: &str,
) -> Markup {
    let subject_id = format!("{settlement_id}:{service_id}");
    chat_area(
        location,
        active_character,
        Some((settlement_id, service_id)),
        Some(("npc", subject_id)),
    )
}

fn chat_area(
    location: &str,
    _active_character: Option<&Character>,
    service_context: Option<(&str, &str)>,
    local_context: Option<(&str, String)>,
) -> Markup {
    html! {
        section class="settlement-chat" aria-label="Settlement chat"
            data-service-quest-settlement=[service_context.map(|context| context.0)]
            data-service-quest-id=[service_context.map(|context| context.1)]
            data-local-chat-kind=[local_context.as_ref().map(|context| context.0)]
            data-local-chat-subject=[local_context.as_ref().map(|context| context.1.as_str())] {
            div class="settlement-chat-resize" role="separator" aria-label="Resize chat"
                aria-orientation="horizontal" aria-valuemin="128" aria-valuemax="640"
                aria-valuenow="184" tabindex="0" title="Drag to resize chat" {
                span aria-hidden="true" {}
            }
            div class="settlement-chat-tabs" role="tablist" aria-label="Chat channels" {
                button type="button" class="settlement-chat-tab active" {
                    "Local"
                }
                button type="button" class="settlement-chat-tab" disabled
                    title="TODO: settlement chat requires real-time message delivery" {
                    "Settlement"
                }
                button type="button" class="settlement-chat-tab" disabled
                    title="TODO: guild chat requires guild membership and real-time message delivery" {
                    "Guild"
                }
            }
            div class="settlement-chat-messages" aria-live="polite" {
                @if local_context.is_none() { div class="chat-system-message" {
                    span class="chat-timestamp" { "[--:--]" }
                    " Select a local character or settlement service to begin talking."
                } }
            }
            div class="settlement-chat-composer" {
                input type="text" name="body" disabled[local_context.is_none()] placeholder=(format!("Message {location}"));
                button type="button" class="btn btn-primary btn-icon" disabled[local_context.is_none()]
                    aria-label="Send message" {
                    "➤"
                }
            }
        }
    }
}

fn merchant_offers_rail(title: &str, placeholder_offers: &[&str]) -> Markup {
    html! {
        (sidebar_section(title, html! {
            p class="text-muted small-copy" { "TODO: merchant inventory and prices are not available yet." }
            table class="trade-inventory-table" {
                (trade_inventory_table_header(false))
                tbody {
                @for offer in placeholder_offers {
                    tr class="trade-inventory-row trade-row-merchant"
                        title="TODO: buying requires merchant inventory, pricing, and trade reducers" {
                        td class="inventory-item-name" {
                            (offer)
                            button type="button" class="trade-transfer trade-transfer-right" disabled
                                aria-label=(format!("Buy {}", offer))
                                title="TODO: buying requires merchant inventory, pricing, and trade reducers" { "▶" }
                        }
                        td class="inventory-count" { "1" }
                        td class="inventory-weight" { "—" }
                        td class="inventory-gold" { "—" }
                    }
                }
                }
            }
        }))
    }
}

fn inventory_rail(
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    trade_action: Option<(&str, &str)>,
    show_repair: bool,
) -> Markup {
    let title = active_character
        .map(|character| format!("{}'s inventory", character.name))
        .unwrap_or_else(|| "Your inventory".to_string());

    html! {
        (sidebar_section(&title, html! {
            @if inventory.is_empty() {
                p class="text-muted small-copy" { "No items carried." }
            } @else {
                table class="trade-inventory-table" {
                    (trade_inventory_table_header(false))
                    tbody {
                    @for item in inventory {
                        tr class=(if trade_action.is_some() { "trade-inventory-row" } else { "trade-inventory-row inventory-row-readonly" }) {
                            td class="inventory-item-name" {
                                (&item.item_id)
                                @if let Some((action, tooltip)) = trade_action {
                                button type="button" class="trade-transfer trade-transfer-left" disabled
                                    aria-label=(format!("{} {}", action, item.item_id))
                                    title=(tooltip) { "◀" }
                                }
                                @if show_repair {
                                span class="repair-placeholder"
                                    style="--repair-icon: url('/static/icons/character/repair.png')"
                                    role="img"
                                    aria-label=(format!("Repair {}", item.item_id))
                                    title="TODO: repairs require durability, pricing, and repair reducers" {}
                                }
                            }
                            td class="inventory-count" { (item.qty) }
                            td class="inventory-weight" { "—" }
                            td class="inventory-gold" { "—" }
                        }
                    }
                }
                }
            }
        }))
    }
}

pub fn rest_result_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
    at_inn: bool,
    summary: &RestSummary,
) -> Markup {
    service_page(
        settlement,
        if at_inn { "inn" } else { "religion" },
        if at_inn { "The Inn" } else { "Church" },
        if at_inn { "Innkeeper" } else { "Priest" },
        "",
        active_character,
        inventory,
        party_members,
        logged_in_as,
        theme,
        None,
        Some(summary),
    )
}

fn rest_service_menu(
    location: &str,
    settlement_id: &str,
    kind: &str,
    healing_days: Option<u16>,
    summary: Option<&RestSummary>,
) -> Markup {
    html! {
    section class="rest-service-menu" aria-label=(format!("{} rest service", location)) {
        div class="rest-service-heading" { strong { "Rest" } }
        @if kind == "inn" {
            p class="rest-service-copy" { "A bed costs 1 gold per day. Injuries are tended before any downtime." }
        } @else {
            p class="rest-service-copy" { "Sanctuary is freely offered to those down on their luck. Injuries are tended before any downtime." }
        }
        form action=(format!("/settlements/{settlement_id}/rest/{kind}")) method="post" {
                div class="rest-days-control" {
                    button type="button" class="rest-days-step rest-days-decrease" aria-label="Decrease rest days"
                        onclick="const input=this.parentElement.querySelector('input'); input.value=Math.max(0, Number(input.value || 0)-1); input.dispatchEvent(new Event('input', {bubbles:true}));" { "−" }
                    input type="number" name="days" value="0" min="0" max="365" aria-label="Rest days"
                        oninput="this.form.querySelector('[type=submit]').disabled=Number(this.value || 0) <= 0;";
                    span class="rest-days-unit" { "days" }
                    button type="button" class="rest-days-step rest-days-increase" aria-label="Increase rest days"
                        onclick="const input=this.parentElement.querySelector('input'); input.value=Math.min(Number(input.max || 365), Number(input.value || 0)+1); input.dispatchEvent(new Event('input', {bubbles:true}));" { "+" }
                    button type="button" class="rest-days-heal" aria-label="Rest until fully healed"
                        title="Set the rest duration needed to fully heal"
                        onclick=(format!("const input=this.parentElement.querySelector('input'); input.value={}; input.dispatchEvent(new Event('input', {{bubbles:true}}));", healing_days.unwrap_or(0))) { "Until healed" }
                }
                button type="submit" class="btn btn-primary btn-small btn-block" data-rest-submit disabled { "Rest" }
        }
        @if let Some(summary) = summary {
            div class="rest-summary-overlay" role="dialog" aria-modal="true" aria-labelledby="rest-summary-title" {
                section class="rest-summary" {
                    div class="rest-summary-heading" {
                        strong id="rest-summary-title" { "Rest summary" }
                        a href=(format!("/settlements/{settlement_id}/{}", if kind == "inn" { "inn" } else { "religion" })) class="rest-summary-close" aria-label="Close rest summary" { "×" }
                    }
                    p { (summary.days) " day" @if summary.days != 1 { "s" } " passed." }
                    @if summary.gold_spent > 0 { p { (summary.gold_spent) " gold paid." } }
                    @if summary.healed.is_empty() { p { "No injuries needed tending." } } @else {
                        p { "Healed:" }
                        ul { @for (part, amount) in &summary.healed { li { (part) ": +" (format!("{amount:.0}%")) } } }
                    }
                    @if summary.trained.is_empty() { p { "No time remained for downtime." } } @else {
                        p { "Training:" }
                        ul { @for (skill, hours) in &summary.trained { li { (skill) ": +" (format!("{hours:.2}h")) } } }
                    }
                }
            }
        }
        }
    }
}

fn days_to_full_health(limbs: &CharacterLimbs) -> u16 {
    let lowest_health = [
        limbs.left_arm_health,
        limbs.right_arm_health,
        limbs.left_leg_health,
        limbs.right_leg_health,
        limbs.head_health,
        limbs.chest_health,
        limbs.stomach_health,
    ]
    .into_iter()
    .fold(1.0_f32, f32::min);
    ((1.0 - lowest_health).max(0.0) / 0.05).ceil() as u16
}
#[cfg(test)]
mod tests {
    use super::LocationKind;

    #[test]
    fn location_kind_rejects_unknown_path_segments() {
        assert_eq!("quest".parse(), Ok(LocationKind::Quest));
        assert!("merchant".parse::<LocationKind>().is_err());
    }
    use super::*;

    #[test]
    fn active_quest_destination_has_red_status_badge() {
        let destination = TravelDestination {
            id: "quest-location".to_string(),
            name: "Bandit camp".to_string(),
            description: "A camp beside the road.".to_string(),
            summary: Some("Active quest".to_string()),
            travel_action: "/quests/quest-location/travel".to_string(),
            distance_m: 1_000,
            journey_minutes: 48,
            quest_in_progress: true,
            turn_in_ready: false,
        };

        let markup = map_destination_list(&[destination], None, "/locations/settlement/test/map")
            .into_string();

        assert!(markup.contains("destination-quest-badge"));
        assert!(markup.contains("Active quest destination"));
        assert!(!markup.contains("destination-turn-in-badge"));
    }
}
