use std::collections::BTreeSet;

use maud::{Markup, html};

use super::social::{npc_description_stage, npc_portrait_strip, settlement_resident_chat_area};
use super::{SoapRestPreview, corpse_medical_dialog, rest_service_menu};
use crate::spacetimedb::{
    BackendCharacterResidenceStatus, BackendCorpse, Character, ChildActivityFocus, ChildStage,
    CourtshipKind, ResidenceTenure, ResidenceTier, Settlement, SettlementAlias, SettlementCategory,
    SettlementDescription, SettlementDescriptionKind, SettlementResidenceOffer,
};
use crate::templates::{
    decorative_game_icon, game_icon, population_description, settlement_layout_with_session,
    sidebar_section,
};

fn settlement_has_keep(category: &SettlementCategory) -> bool {
    matches!(
        category,
        SettlementCategory::Town | SettlementCategory::City | SettlementCategory::Capital
    )
}

fn public_square_place_link(settlement: &Settlement, current: bool) -> Markup {
    use adventuresim_core::settlement_economy::{player_visible_npc_tabs, visible_npc_tab};

    let tabs = player_visible_npc_tabs(
        &settlement.economy,
        settlement_has_keep(&settlement.category),
        &settlement.id,
    );
    let tab = visible_npc_tab(&tabs, "overview")
        .expect("every settlement exposes its overview as a navigable NPC tab");
    html! {
        a href=(format!("/locations/settlement/{}", settlement.id))
            class=(if current { "active" } else { "" })
            aria-current=(if current { "page" } else { "false" }) {
            (tab.label)
        }
    }
}

pub fn settlement_overview_page(
    settlement: &Settlement,
    aliases: &[SettlementAlias],
    descriptions: &[SettlementDescription],
    active_character: Option<&Character>,
    party_members: &[Character],
    logged_in_as: Option<&str>,
    corpses: &[BackendCorpse],
    selected_corpse: Option<(&BackendCorpse, &str)>,
) -> Markup {
    let alias_labels = settlement_alias_labels(settlement, aliases);
    let historical_description = preferred_settlement_description(descriptions);
    let population_tooltip = if settlement.population_estimate == 0 {
        format!(
            "No imported headcount is available\nSettlement class: {}",
            population_description(settlement.population_level)
        )
    } else {
        format!(
            "Imported estimate: {} people\nSettlement class: {}",
            format_number(settlement.population_estimate),
            population_description(settlement.population_level),
        )
    };
    let faith_labels = settlement
        .religious_status
        .represented_religions()
        .iter()
        .map(|religion| religion.label())
        .collect::<Vec<_>>();
    let faiths = joined_or_dash(&faith_labels);
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Settlement", html! {
                div class="settlement-summary" {
                    dl class="location-stat-list" {
                        div tabindex="0"
                            data-strategic-tooltip=(&population_tooltip) {
                            dt { "Population" } dd { (format_population(settlement)) }
                        }
                        div tabindex="0"
                            data-strategic-tooltip=(format!(
                                "Prosperity score: {}/1000",
                                settlement.economy.prosperity_score,
                            )) {
                            dt { "Prosperity" } dd { (format!("{:?}", settlement.economy.prosperity_tier)) }
                        }
                        div { dt { "Faiths" } dd { (&faiths) } }
                        div data-developer-only { dt { "Services" } dd { (settlement.economy.services.iter().map(|v| format!("{:?}", v)).collect::<Vec<_>>().join(", ")) } }
                        div data-developer-only { dt { "Specialties" } dd { (settlement.economy.specializations.iter().map(|v| format!("{:?}", v)).collect::<Vec<_>>().join(", ")) } }
                        div data-developer-only { dt { "Coordinates" } dd { (format!("{:.6}, {:.6}", settlement.coord_x, settlement.coord_y)) } }
                        div data-developer-only { dt { "Languages" } dd { (format!(
                            "East-central {:.1}% · West-central {:.1}% · Low {:.1}%",
                            f32::from(settlement.languages.east_central_bp) / 100.0,
                            f32::from(settlement.languages.west_central_bp) / 100.0,
                            f32::from(settlement.languages.low_bp) / 100.0,
                        )) } }
                        @if !alias_labels.is_empty() {
                            div { dt { "Also known as" } dd { (alias_labels.join(", ")) } }
                        }
                    }
                }
            }))
        }
        main class="center-content settlement-main settlement-overview" {
            (party_portrait_overlay(party_members, active_character, &format!("/locations/settlement/{}", settlement.id), None, false))
            (npc_portrait_strip(&settlement.id, "overview"))
            @if !corpses.is_empty() {
                nav class="scene-interactable-strip corpse-strip" aria-label="Bodies held in the settlement" {
                    @for corpse in corpses {
                        @let corpse_label = if corpse.location == "interred" { "Buried body" } else { &corpse.display_name };
                        a class="scene-interactable scene-interactable--remains corpse-portrait"
                            href=(format!("/locations/settlement/{}?corpse={}&medical=physiology", settlement.id, corpse.corpse_id))
                            aria-label=(format!("Examine {corpse_label} with Physiology")) {
                            span class="scene-interactable-visual" aria-hidden="true" { "☠" }
                            span class="scene-interactable-label" { (corpse_label) }
                        }
                    }
                }
                @if let Some((corpse, _)) = selected_corpse {
                    div class="quest-combat-actions corpse-medical-actions" aria-label="Corpse medical windows" {
                        a class="btn btn-secondary" href=(format!("/locations/settlement/{}?corpse={}&medical=physiology", settlement.id, corpse.corpse_id)) { "Physiology" }
                        a class="btn btn-secondary" href=(format!("/locations/settlement/{}?corpse={}&medical=surgery", settlement.id, corpse.corpse_id)) { "Surgery" }
                    }
                }
            }
            (npc_description_stage(&settlement.name, "Select a local resident to see their visible description."))
            (settlement_resident_chat_area(&settlement.name, active_character, &settlement.id, "overview", None))
        }
        aside class="right-sidebar" {
            (sidebar_section("Description", html! {
                p { (settlement_description(settlement.population_level)) }
                @if let Some(description) = historical_description {
                    details class="settlement-historical-description" {
                        summary { (format!("Historical description — {}", language_label(description.language.as_deref()))) }
                        p { (description.body) }
                    }
                }
            }))
        }
        @if let Some((corpse, window)) = selected_corpse {
            (corpse_medical_dialog(
                corpse,
                &format!("/locations/settlement/{}", settlement.id),
                window,
            ))
        }
    };
    settlement_layout_with_session(
        &settlement.name,
        &settlement.name,
        &settlement.id,
        &settlement.category,
        "",
        Some(&settlement.religion_id),
        Some(&settlement.economy),
        content,
        logged_in_as,
    )
}

/// Shared authoritative shell for non-service public, residential, and keep locations.
pub fn settlement_resident_location_page(
    settlement: &Settlement,
    active_character: &Character,
    party_members: &[Character],
    location_id: &str,
    logged_in_as: Option<&str>,
) -> Markup {
    settlement_resident_location_page_with_panel(
        settlement,
        active_character,
        party_members,
        location_id,
        logged_in_as,
        None,
        false,
    )
}

pub fn settlement_residence_page(
    settlement: &Settlement,
    active_character: &Character,
    party_members: &[Character],
    logged_in_as: Option<&str>,
    offers: &[SettlementResidenceOffer],
    holdings: &[BackendCharacterResidenceStatus],
    relationship: Option<&RelationshipPresentation>,
    can_rest_at_home: bool,
    notice: Option<&str>,
) -> Markup {
    let panel = residence_offer_panel(
        settlement,
        active_character.id,
        offers,
        holdings,
        relationship,
        notice,
    );
    settlement_resident_location_page_with_panel(
        settlement,
        active_character,
        party_members,
        "residences",
        logged_in_as,
        Some(panel),
        can_rest_at_home,
    )
}

fn settlement_resident_location_page_with_panel(
    settlement: &Settlement,
    active_character: &Character,
    party_members: &[Character],
    location_id: &str,
    logged_in_as: Option<&str>,
    residence_panel: Option<Markup>,
    can_rest_at_home: bool,
) -> Markup {
    let chapter =
        adventuresim_core::organization::organization_chapter_at(&settlement.id, location_id);
    let (title, description) = match location_id {
        "residences" => (
            "Residential quarter",
            "Homes, courtyards, and narrow lanes where local households conduct their daily business.",
        ),
        "keep" => (
            "The keep",
            "The seat of local authority, occupied by retainers, servants, and petitioners.",
        ),
        _ if chapter.is_none() => (
            "Public square",
            "A public gathering place for residents and travelers.",
        ),
        _ => {
            let (organization, chapter) = chapter.expect("guarded chapter");
            (
                chapter.building_name.as_str(),
                organization.description.as_str(),
            )
        }
    };
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Places", html! {
                nav class="settlement-places-nav" aria-label="Settlement places" {
                    (public_square_place_link(settlement, false))
                    a href=(format!("/settlements/{}/places/residences", settlement.id))
                        class=(if location_id == "residences" { "active" } else { "" })
                        aria-current=(if location_id == "residences" { "page" } else { "false" }) {
                        "Residences"
                    }
                    @if settlement_has_keep(&settlement.category) {
                        a href=(format!("/settlements/{}/places/keep", settlement.id))
                            class=(if location_id == "keep" { "active" } else { "" })
                            aria-current=(if location_id == "keep" { "page" } else { "false" }) {
                            "Keep"
                        }
                    }
                    @for organization in adventuresim_core::organization::organizations_for_chapter(&settlement.id) {
                        @let chapter = organization.chapter(&settlement.id).expect("local chapter");
                        @if adventuresim_core::organization::chapter_has_standalone_building(organization, chapter, &settlement.economy) {
                        a href=(format!("/settlements/{}/places/{}", settlement.id, chapter.location_id))
                            class=(if location_id == chapter.location_id { "active" } else { "" })
                            aria-current=(if location_id == chapter.location_id { "page" } else { "false" }) {
                            (&chapter.building_name)
                        }
                        }
                    }
                }
            }))
        }
        main class="center-content settlement-main settlement-overview" {
            (party_portrait_overlay(party_members, Some(active_character), &format!("/locations/settlement/{}", settlement.id), None, false))
            (npc_portrait_strip(&settlement.id, location_id))
            (npc_description_stage(title, description))
            (settlement_resident_chat_area(title, Some(active_character), &settlement.id, location_id, None))
        }
        aside class="right-sidebar" {
            (sidebar_section("Location", html! { p { (description) } }))
            @if location_id == "residences" {
                @if let Some(panel) = residence_panel { (panel) }
                @if can_rest_at_home {
                    (sidebar_section("Home", html! {
                        p class="text-muted small-copy" { "Your active local home covers this stay." }
                        (rest_service_menu(
                            "Home",
                            &settlement.id,
                            "residence",
                            None,
                            None,
                            SoapRestPreview::default(),
                        ))
                    }))
                }
            }
        }
    };
    settlement_layout_with_session(
        title,
        &settlement.name,
        &settlement.id,
        &settlement.category,
        location_id,
        Some(&settlement.religion_id),
        Some(&settlement.economy),
        content,
        logged_in_as,
    )
}

#[derive(Debug, Clone)]
pub struct WeddingPresentation {
    pub days_remaining: u64,
    pub date_label: String,
}

#[derive(Debug, Clone, Default)]
pub struct RelationshipPresentation {
    pub spouse_name: Option<String>,
    pub courtship_partner_name: Option<String>,
    pub courtship_kind: Option<CourtshipKind>,
    pub courtship_exposed: bool,
    pub wedding: Option<WeddingPresentation>,
    pub pregnancy_due_days: Option<u64>,
    pub children: Vec<ChildPresentation>,
}

#[derive(Debug, Clone)]
pub struct ChildPresentation {
    pub name: String,
    pub stage: ChildStage,
    pub focus: ChildActivityFocus,
    pub maturity_basis_points: u16,
    pub adult_playable: bool,
    pub alive: bool,
}

fn child_stage_label(stage: ChildStage) -> &'static str {
    match stage {
        ChildStage::EarlyChildhood => "early childhood",
        ChildStage::MiddleChildhood => "middle childhood",
        ChildStage::Adolescence => "adolescence",
        ChildStage::Adult => "adult",
    }
}

fn child_stage_id(stage: ChildStage) -> &'static str {
    match stage {
        ChildStage::EarlyChildhood => "early",
        ChildStage::MiddleChildhood => "middle",
        ChildStage::Adolescence => "adolescent",
        ChildStage::Adult => "adult",
    }
}

fn child_activity_presentation(
    stage: ChildStage,
    focus: ChildActivityFocus,
) -> (&'static str, &'static str) {
    if stage == ChildStage::EarlyChildhood {
        return ("bed", "care and rest");
    }
    match focus {
        ChildActivityFocus::Play => ("sun", "play"),
        ChildActivityFocus::Study => ("open-book", "study"),
        ChildActivityFocus::HouseholdHelp => ("hammer-sickle", "household help"),
        ChildActivityFocus::SocialLearning => ("person", "social learning"),
    }
}

fn residence_meter(icon: &str, class_name: &str, label: &str, value: u32, maximum: u32) -> Markup {
    let percent = value.saturating_mul(100) / maximum.max(1);
    html! {
        span class=(format!("residence-meter residence-meter-{class_name}"))
            role="meter" aria-valuemin="0" aria-valuemax=(maximum) aria-valuenow=(value)
            aria-valuetext=(label) title=(label) data-strategic-tooltip=(label) {
            (decorative_game_icon(icon))
            span class="residence-meter-track" aria-hidden="true" {
                span class="residence-meter-fill" style=(format!("width: {percent}%")) {}
            }
        }
    }
}

fn household_progress(icon: &str, class_name: &str, label: &str, percent: u64) -> Markup {
    html! {
        span class=(format!("household-progress household-{class_name}"))
            role="meter" aria-valuemin="0" aria-valuemax="100" aria-valuenow=(percent.min(100))
            aria-valuetext=(label) title=(label) data-strategic-tooltip=(label) {
            (decorative_game_icon(icon))
            span class="household-progress-track" aria-hidden="true" {
                span class="household-progress-fill" style=(format!("width: {}%", percent.min(100))) {}
            }
        }
    }
}

fn residence_tier_label(tier: ResidenceTier) -> &'static str {
    match tier {
        ResidenceTier::Cheap => "Cheap",
        ResidenceTier::Moderate => "Moderate",
        ResidenceTier::Fancy => "Fancy",
    }
}

fn residence_tier_id(tier: ResidenceTier) -> &'static str {
    match tier {
        ResidenceTier::Cheap => "cheap",
        ResidenceTier::Moderate => "moderate",
        ResidenceTier::Fancy => "fancy",
    }
}

fn residence_offer_panel(
    settlement: &Settlement,
    active_character_id: u64,
    offers: &[SettlementResidenceOffer],
    holdings: &[BackendCharacterResidenceStatus],
    relationship: Option<&RelationshipPresentation>,
    notice: Option<&str>,
) -> Markup {
    let max_rent = offers
        .iter()
        .map(|offer| offer.rent_per_period)
        .max()
        .unwrap_or(1);
    let max_purchase = offers
        .iter()
        .map(|offer| offer.purchase_price)
        .max()
        .unwrap_or(1);
    let max_owner_cost = offers
        .iter()
        .map(|offer| offer.owner_maintenance_per_period + offer.property_tax_per_period)
        .max()
        .unwrap_or(1);
    let max_leisure = offers
        .iter()
        .map(|offer| u32::from(offer.leisure_morale_basis_points))
        .max()
        .unwrap_or(1);
    sidebar_section(
        "Residences",
        html! {
            @if let Some(notice) = notice {
                p class="residence-notice" role="status" { (notice) }
            }
            @if holdings.is_empty() {
                p { "No residence holdings." }
            } @else {
                div class="residence-holding-list" {
                    @for holding in holdings {
                        @let owns_holding = holding.owner_character_id == active_character_id;
                        @let tier = residence_tier_label(holding.tier);
                        @let tenure = match holding.tenure { ResidenceTenure::Renter => "Rental", ResidenceTenure::Owner => "Owned property" };
                        @let payment_label = if holding.active { format!("Next payment {}", format_residence_date(holding.next_due_minute)) } else { format!("Payment overdue since {}", format_residence_date(holding.next_due_minute)) };
                        article class="residence-holding" data-holding-id=(&holding.holding_id) {
                            div class="residence-holding-heading" {
                                (decorative_game_icon("house"))
                                strong { (tier) }
                                span { (&holding.settlement_id) }
                                div class="residence-holding-states" role="group" aria-label=(format!("{tier} residence status")) {
                                    span class="residence-holding-state" role="img" title=(if owns_holding { tenure } else { "Household home" }) aria-label=(if owns_holding { tenure } else { "Household home" }) {
                                        (decorative_game_icon(if owns_holding && holding.tenure == ResidenceTenure::Renter { "bed" } else { "house" }))
                                    }
                                    @if holding.primary {
                                        span class="residence-holding-state" role="img" title="Designated home" aria-label="Designated home" { (decorative_game_icon("crown")) }
                                    }
                                    @if holding.occupied {
                                        span class="residence-holding-state" role="img" title="Occupied" aria-label="Occupied" { (decorative_game_icon("person")) }
                                    }
                                    span class=(if holding.active { "residence-holding-state" } else { "residence-holding-state residence-holding-state-inactive" })
                                        role="img" title=(&payment_label) aria-label=(&payment_label) data-strategic-tooltip=(&payment_label) {
                                        (decorative_game_icon(if holding.active { "calendar" } else { "cross-mark" }))
                                    }
                                }
                            }
                            div class="residence-holding-actions" {
                            @if owns_holding && holding.tenure == ResidenceTenure::Owner && !holding.active {
                                form action=(format!("/settlements/{}/residences/recover/current", settlement.id)) method="post" {
                                    input type="hidden" name="holding_id" value=(&holding.holding_id);
                                    button type="submit" class="residence-icon-action" title="Recover owned home" aria-label="Recover owned home" {
                                        (decorative_game_icon("hammer-nails")) span class="sr-only" { "Recover owned home" }
                                    }
                                }
                            }
                            @if owns_holding && holding.active && !holding.primary && holding.settlement_id == settlement.id {
                                form action=(format!("/settlements/{}/residences/designate/current", settlement.id)) method="post" {
                                    input type="hidden" name="holding_id" value=(&holding.holding_id);
                                    button type="submit" class="residence-icon-action" title="Designate as home" aria-label="Designate as home" {
                                        (decorative_game_icon("crown")) span class="sr-only" { "Designate as home" }
                                    }
                                }
                            }
                            @if owns_holding {
                                form action=(format!("/settlements/{}/residences/relinquish/current", settlement.id)) method="post"
                                    onsubmit="return confirm('Relinquish this property? This cannot be undone.')" {
                                    input type="hidden" name="holding_id" value=(&holding.holding_id);
                                    button type="submit" class="residence-icon-action residence-icon-action-danger" title="Relinquish property" aria-label="Relinquish property" {
                                        (decorative_game_icon("cross-mark")) span class="sr-only" { "Relinquish property" }
                                    }
                                }
                            }
                            }
                        }
                    }
                }
            }
            @if let Some(relationship) = relationship {
                div class="household-visual" role="group" aria-label="Household and family" {
                @if let Some(spouse_name) = &relationship.spouse_name {
                    div class="household-member" aria-label=(format!("Spouse: {spouse_name}")) title=(format!("Spouse: {spouse_name}")) {
                        (decorative_game_icon("heart-beats")) span class="household-member-name" { (spouse_name) }
                    }
                }
                @if let Some(partner_name) = &relationship.courtship_partner_name {
                    @let courtship_kind = relationship.courtship_kind.unwrap_or(CourtshipKind::Informal);
                    @let informal = courtship_kind == CourtshipKind::Informal;
                    @let courtship_kind_label = match courtship_kind { CourtshipKind::Formal => "formal", CourtshipKind::Informal => "informal" };
                    @let courtship_visibility = if !informal { "; formal and public" } else if relationship.courtship_exposed { "; known to family" } else { "; private" };
                    @let courtship_icon = if !informal { "rose" } else if relationship.courtship_exposed { "eye-target" } else { "lockpicks" };
                    @let courtship_label = format!("{courtship_kind_label} courtship with {partner_name}{courtship_visibility}");
                    div class="household-member" title=(&courtship_label) aria-label=(&courtship_label) {
                        (decorative_game_icon("rose")) span class="household-member-name" { (partner_name) }
                        span class="household-member-state" aria-hidden="true" {
                            (decorative_game_icon(courtship_icon))
                        }
                    }
                }
                @if let Some(wedding) = &relationship.wedding {
                    @let wedding_label = format!("Wedding {}, {} days remaining", wedding.date_label, wedding.days_remaining);
                    @let wedding_progress = 365_u64.saturating_sub(wedding.days_remaining.min(365)) * 100 / 365;
                    (household_progress("calendar", "wedding", &wedding_label, wedding_progress))
                }
                @if let Some(days) = relationship.pregnancy_due_days {
                    @let pregnancy_label = format!("Expected birth in about {days} days");
                    @let pregnancy_progress = 270_u64.saturating_sub(days.min(270)) * 100 / 270;
                    (household_progress("stomach", "pregnancy", &pregnancy_label, pregnancy_progress))
                }
                @for child in &relationship.children {
                    @let stage = child_stage_label(child.stage);
                    @let stage_id = child_stage_id(child.stage);
                    @let (focus_icon, focus) = child_activity_presentation(child.stage, child.focus);
                    @let state = if !child.alive { "deceased" } else if child.adult_playable { "adult and available in the character roster" } else { stage };
                    @let label = format!("Child: {}; {state}; focus: {focus}; maturity {} percent", child.name, child.maturity_basis_points / 100);
                    div class=(format!("household-child household-child-stage-{stage_id}")) aria-label=(&label) title=(&label) data-strategic-tooltip=(&label) {
                        div class="household-member" {
                            (decorative_game_icon("person")) span class="household-member-name" { (&child.name) }
                            span class="household-member-state" aria-hidden="true" {
                                (decorative_game_icon(if !child.alive { "death-skull" } else if child.adult_playable { "crown" } else { focus_icon }))
                            }
                        }
                        (household_progress("person", "maturity", &label, u64::from(child.maturity_basis_points) / 100))
                    }
                }
                }
            }
            div class="residence-offer-grid" {
            @for offer in offers {
                @let tier = residence_tier_label(offer.tier);
                @let owner_cost = offer.owner_maintenance_per_period + offer.property_tax_per_period;
                @let rent_label = format!("Rent: {} coin every 30 days", offer.rent_per_period);
                @let purchase_label = format!("Purchase: {} coin", offer.purchase_price);
                @let upkeep_label = format!("Owner upkeep: {} maintenance plus {} property tax every 30 days", offer.owner_maintenance_per_period, offer.property_tax_per_period);
                @let leisure_label = format!("Leisure morale bonus: {} percent", format_morale_percent(offer.leisure_morale_basis_points));
                @let rent_confirmation = format!("return confirm('Rent this {tier} residence for {} coin every 30 days?')", offer.rent_per_period);
                @let buy_confirmation = format!("return confirm('Buy this {tier} residence for {} coin? Ongoing owner costs are {} coin every 30 days.')", offer.purchase_price, owner_cost);
                article class="residence-offer" data-residence-tier=(residence_tier_id(offer.tier)) {
                    div class="residence-offer-heading" { (decorative_game_icon("house")) strong { (tier) } }
                    div class="residence-meter-list" {
                        (residence_meter("bed", "rent", &rent_label, offer.rent_per_period, max_rent))
                        (residence_meter("coins", "purchase", &purchase_label, offer.purchase_price, max_purchase))
                        (residence_meter("hammer-nails", "upkeep", &upkeep_label, owner_cost, max_owner_cost))
                        (residence_meter("sun", "leisure", &leisure_label, u32::from(offer.leisure_morale_basis_points), max_leisure))
                    }
                    div class="residence-meter-actions" {
                    form action=(format!("/settlements/{}/residences/rent/{}", settlement.id, residence_tier_id(offer.tier))) method="post" onsubmit=(&rent_confirmation) {
                        button type="submit" class="residence-icon-action" title=(&rent_label) aria-label=(format!("Rent {tier}. {rent_label}")) {
                            (decorative_game_icon("bed")) span class="sr-only" { "Rent " (tier) }
                        }
                    }
                    form action=(format!("/settlements/{}/residences/buy/{}", settlement.id, residence_tier_id(offer.tier))) method="post" onsubmit=(&buy_confirmation) {
                        button type="submit" class="residence-icon-action" title=(&purchase_label) aria-label=(format!("Buy {tier}. {purchase_label}. {upkeep_label}")) {
                            (decorative_game_icon("coins")) span class="sr-only" { "Buy " (tier) }
                        }
                    }
                    }
                }
            }
            }
        },
    )
}

fn format_morale_percent(basis_points: u16) -> String {
    let whole = basis_points / 100;
    let fraction = basis_points % 100;
    if fraction == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{fraction:02}")
    }
}

fn format_residence_date(minute: u64) -> String {
    let day = minute / adventuresim_core::strategic_time::MINUTES_PER_DAY;
    format!("year {}, day {}", 1544 + day / 365, day % 365 + 1)
}

fn settlement_alias_labels(settlement: &Settlement, aliases: &[SettlementAlias]) -> Vec<String> {
    let canonical = settlement.name.to_lowercase();
    let mut labels = BTreeSet::new();
    for alias in aliases {
        let label = alias.prefix.as_ref().map_or_else(
            || alias.name.trim().to_owned(),
            |prefix| format!("{} {}", prefix.trim(), alias.name.trim()),
        );
        if !label.is_empty() && label.to_lowercase() != canonical {
            labels.insert(label);
        }
    }
    let total = labels.len();
    let mut labels: Vec<_> = labels.into_iter().take(8).collect();
    if total > labels.len() {
        labels.push(format!("and {} more", total - labels.len()));
    }
    labels
}

fn joined_or_dash(labels: &[&str]) -> String {
    if labels.is_empty() {
        "—".into()
    } else {
        labels.join(", ")
    }
}

fn preferred_settlement_description(
    descriptions: &[SettlementDescription],
) -> Option<&SettlementDescription> {
    descriptions.iter().min_by_key(|description| {
        (
            description.language.as_deref() != Some("eng"),
            description.kind != SettlementDescriptionKind::Settlement,
            description.id.as_str(),
        )
    })
}

fn language_label(language: Option<&str>) -> &str {
    match language {
        Some("dan") => "Danish",
        Some("deu") => "German",
        Some("eng") => "English",
        Some("fin") => "Finnish",
        Some("nld") => "Dutch",
        Some(code) => code,
        None => "Unspecified language",
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

pub(super) fn format_distance(distance_m: u64) -> String {
    format!("{:.1} km", distance_m as f64 / 1_000.0)
}

pub(super) fn format_population(settlement: &Settlement) -> String {
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
    for group in digits.as_bytes()[first_group..].chunks(3) {
        formatted.push(',');
        formatted.push_str(std::str::from_utf8(group).expect("population digits are valid UTF-8"));
    }
    formatted
}

pub(super) fn format_journey_time(minutes: u64) -> String {
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

pub(crate) fn visual_stage(kind: &str, title: &str, description: &str) -> Markup {
    let scene_label = match kind {
        "settlement" => "At the settlement gates",
        "map" | "route" => "Roads and destinations",
        "camp" => "Camp beside the road",
        "quest" => "Encounter ground",
        "alchemy" => "The apothecary workbench",
        "service" => "At the counter",
        "chest" => "Shared party stores",
        _ => "Adventurer profile",
    };
    html! {
        figure class=(format!("service-visual service-visual-{}", kind)) {
            div class="service-visual-scene" role="img" aria-label=(format!("{title}. {description}")) {
                span class="visual-scene-sky" aria-hidden="true" {}
                span class="visual-scene-horizon" aria-hidden="true" {}
                span class="visual-scene-route" aria-hidden="true" {}
                span class="visual-scene-caption" {
                    strong { (title) }
                    span { (scene_label) }
                }
            }
            figcaption { (description) }
        }
    }
}

pub(crate) struct CharacterPortraitView<'a> {
    pub id: u64,
    pub name: &'a str,
    pub alive: bool,
    pub active: bool,
    pub selected: bool,
    pub href: String,
    pub title: String,
    pub aria_label: String,
    pub decoration: Option<Markup>,
    pub badge: Option<Markup>,
    pub actions: Option<Markup>,
}

pub(crate) fn character_portrait_overlay(
    label: &str,
    inventory: Option<Markup>,
    members: &[CharacterPortraitView<'_>],
) -> Markup {
    html! {
        @if !members.is_empty() {
            div class="party-portrait-overlay" aria-label=(label) {
                div data-party-portrait-members {
                    @if let Some(inventory) = inventory {
                        (inventory)
                    }
                    @for member in members {
                        div class=(format!("scene-interactable scene-interactable--person party-portrait{}{}", if member.selected { " active" } else { "" }, if !member.alive { " dead" } else { "" }))
                            data-character-id=(member.id)
                            data-character-alive=(member.alive)
                            data-active-character[member.active]
                            title=(member.name) {
                            a class="party-portrait-select"
                                href=(&member.href)
                                title=(&member.title)
                                aria-label=(&member.aria_label) {
                                @if let Some(decoration) = &member.decoration {
                                    (decoration)
                                }
                                span class="scene-interactable-visual party-portrait-initial" {
                                    span class="party-portrait-face" { (member.name.chars().next().unwrap_or('?')) }
                                    span class="scene-interactable-label party-portrait-name" { (member.name) @if !member.alive { " (dead)" } }
                                    @if let Some(badge) = &member.badge {
                                        (badge)
                                    }
                                }
                            }
                            @if let Some(actions) = &member.actions {
                                (actions)
                            }
                        }
                    }
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
    can_examine: bool,
) -> Markup {
    let members: Vec<&Character> = if party_members.is_empty() {
        active_character.into_iter().collect()
    } else {
        party_members.iter().collect()
    };
    let leader_id = members.first().map(|member| member.id);

    let inventory = active_character.map(|_| {
        html! {
            div class="scene-interactable scene-interactable--fixture party-portrait party-inventory-portrait" title="Party inventory" {
                a class="party-portrait-select" href=(format!("{}/party-inventory", location_path)) {
                    span class="scene-interactable-visual party-portrait-initial party-chest-face" { (game_icon("Party inventory", "knapsack")) }
                }
            }
        }
    });
    let portraits = members
        .into_iter()
        .map(|member| {
            let is_active = active_character.is_some_and(|character| character.id == member.id);
            let can_remove = Some(member.id) != leader_id;
            let notified = member.alive && member.social_notification_count > 0;
            let persistently_notified = notified && !member.automatic_social_chat_enabled;
            let inspection_href = if is_active {
                format!("{}/party/{}", location_path, member.id)
            } else {
                format!("{}/party/{}/stats", location_path, member.id)
            };
            let actions = (member.alive
                && active_character.is_some_and(|character| character.alive))
            .then(|| {
                html! {
                    span class="party-portrait-actions" aria-label=(format!("Actions for {}", member.name)) {
                            a href=(format!("{}/party/{}/social", location_path, member.id))
                                class=(format!("party-portrait-action party-social-action{}", if persistently_notified { " party-social-notified" } else { "" }))
                                title=(if notified { format!("Open {}'s Recent Tidings ({} morale concerns)", member.name, member.social_notification_count) } else { format!("Talk to {}", member.name) })
                                aria-label=(if notified { format!("Open conversation with {} to Recent Tidings; {} unaddressed morale concerns", member.name, member.social_notification_count) } else { format!("Open conversation with {}", member.name) }) {
                                span class="party-action-icon"
                                    style="--party-action-icon: url('/static/icons/game/conversation.svg')"
                                    aria-hidden="true" {}
                                @if notified {
                                    span class="party-social-notification" aria-hidden="true" {
                                        (member.social_notification_count)
                                    }
                                }
                            }
                            @if is_active && can_examine && location_path.starts_with("/locations/settlement/") {
                                a href=(format!("{location_path}/alchemy"))
                                    class="party-portrait-action party-alchemy-action"
                                    title="Prepare medication"
                                    aria-label="Prepare medication" {
                                    span class="party-action-icon"
                                        style="--party-action-icon: url('/static/icons/game/medical-pack.svg')"
                                        role="img" aria-label="Alchemy" {}
                                }
                            }
                            a href=(format!("{}/party/{}/inventory", location_path, member.id))
                                class="party-portrait-action"
                                title=(if is_active { "Open inventory and discard items".to_string() } else { format!("Compare inventory with {}", member.name) }) {
                                span class="party-action-icon"
                                    style="--party-action-icon: url('/static/icons/game/knapsack.svg')"
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
            });
            CharacterPortraitView {
                id: member.id,
                name: &member.name,
                alive: member.alive,
                active: is_active,
                selected: selected_character_id == Some(member.id),
                href: inspection_href,
                title: format!("Inspect {}", member.name),
                aria_label: format!("Inspect {}", member.name),
                decoration: Some(html! {
                    span class="incapacitation-wheel"
                        data-strategic-condition-wheel=(member.id)
                        role="img"
                        aria-label="Loading strategic condition"
                        title="Loading strategic condition" {}
                }),
                badge: None,
                actions,
            }
        })
        .collect::<Vec<_>>();
    character_portrait_overlay("Active party", inventory, &portraits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacetimedb::*;
    use crate::templates::settlement::test_support::*;

    #[test]
    fn notified_social_action_stays_visible_while_portrait_keeps_inspection() {
        let member = Character {
            id: 12,
            name: "Greta".into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: Some("lubeck".into()),
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 24,
            alive: true,
            temporary: false,
            social_notification_count: 2,
            automatic_social_chat_enabled: false,
        };
        let markup = party_portrait_overlay(
            std::slice::from_ref(&member),
            Some(&member),
            "/locations/settlement/lubeck",
            None,
            false,
        )
        .into_string();
        assert!(markup.contains(
            "class=\"party-portrait-select\" href=\"/locations/settlement/lubeck/party/12\""
        ));
        assert!(
            markup.contains(
                "class=\"party-portrait-action party-social-action party-social-notified\""
            )
        );
        assert!(markup.contains("href=\"/locations/settlement/lubeck/party/12/social\""));
        assert!(markup.contains("class=\"party-social-notification\""));
        assert!(markup.contains("2 unaddressed morale concerns"));
        assert!(markup.contains("/static/icons/game/conversation.svg"));
        assert!(markup.contains("class=\"incapacitation-wheel\""));
        assert!(markup.contains("data-strategic-condition-wheel=\"12\""));

        let mut quiet = member;
        quiet.social_notification_count = 0;
        let quiet_markup = party_portrait_overlay(
            &[quiet.clone()],
            Some(&quiet),
            "/locations/settlement/lubeck",
            None,
            false,
        )
        .into_string();
        assert!(!quiet_markup.contains("party-social-notification"));
        assert!(quiet_markup.contains("class=\"party-portrait-action party-social-action\""));
        assert!(quiet_markup.contains("/party/12/social"));
        assert!(quiet_markup.contains("aria-label=\"Open conversation with Greta\""));

        let mut automatic = quiet;
        automatic.social_notification_count = 2;
        automatic.automatic_social_chat_enabled = true;
        let automatic_markup = party_portrait_overlay(
            &[automatic.clone()],
            Some(&automatic),
            "/locations/settlement/lubeck",
            None,
            false,
        )
        .into_string();
        assert!(automatic_markup.contains("class=\"party-social-notification\""));
        assert!(automatic_markup.contains("2 unaddressed morale concerns"));
        assert!(!automatic_markup.contains("party-social-notified"));
    }

    #[test]
    fn aliases_are_deduplicated_and_do_not_repeat_the_canonical_name() {
        let aliases = [
            SettlementAlias {
                id: "1".into(),
                settlement_id: "viabundus-1".into(),
                name: "Lubeke".into(),
                prefix: None,
                language: Some("deu".into()),
            },
            SettlementAlias {
                id: "2".into(),
                settlement_id: "viabundus-1".into(),
                name: "Lübeck".into(),
                prefix: None,
                language: None,
            },
        ];

        assert_eq!(settlement_alias_labels(&settlement(), &aliases), ["Lubeke"]);
    }

    #[test]
    fn english_settlement_description_is_preferred_deterministically() {
        let descriptions = [
            SettlementDescription {
                id: "1".into(),
                settlement_id: "viabundus-1".into(),
                kind: SettlementDescriptionKind::Settlement,
                language: Some("deu".into()),
                body: "Deutsch".into(),
            },
            SettlementDescription {
                id: "2".into(),
                settlement_id: "viabundus-1".into(),
                kind: SettlementDescriptionKind::City,
                language: Some("eng".into()),
                body: "English city".into(),
            },
            SettlementDescription {
                id: "3".into(),
                settlement_id: "viabundus-1".into(),
                kind: SettlementDescriptionKind::Settlement,
                language: Some("eng".into()),
                body: "English settlement".into(),
            },
        ];

        assert_eq!(
            preferred_settlement_description(&descriptions)
                .unwrap()
                .body,
            "English settlement"
        );
    }

    #[test]
    fn settlement_overview_renders_enrichment_as_escaped_text() {
        let aliases = [SettlementAlias {
            id: "1".into(),
            settlement_id: "viabundus-1".into(),
            name: "Lubeke".into(),
            prefix: None,
            language: Some("deu".into()),
        }];
        let descriptions = [SettlementDescription {
            id: "1".into(),
            settlement_id: "viabundus-1".into(),
            kind: SettlementDescriptionKind::Settlement,
            language: Some("deu".into()),
            body: "Burg & Markt <alt>".into(),
        }];

        let markup = settlement_overview_page(
            &settlement(),
            &aliases,
            &descriptions,
            None,
            &[],
            None,
            &[],
            None,
        )
        .into_string();

        assert!(markup.contains("Also known as"));
        assert!(markup.contains("Lubeke"));
        assert!(markup.contains("Historical description — German"));
        assert!(markup.contains("Burg &amp; Markt &lt;alt&gt;"));
        assert!(!markup.contains("<alt>"));
    }

    #[test]
    fn settlement_overview_treats_zero_population_as_missing_and_empty_faiths_as_unknown() {
        let mut settlement = settlement();
        settlement.population_estimate = 0;
        let markup = settlement_overview_page(&settlement, &[], &[], None, &[], None, &[], None)
            .into_string();
        assert!(markup.contains("No imported headcount is available"));
        assert!(!markup.contains("Imported estimate: 0 people"));
        assert_eq!(joined_or_dash(&[]), "—");
    }

    #[test]
    fn settlement_overview_exposes_moved_corpses_in_existing_medical_windows() {
        let corpse = BackendCorpse {
            owner_character_id: 7,
            corpse_id: "corpse:quest:1".into(),
            display_name: "Unknown victim".into(),
            creature_kind: "human".into(),
            source_id: "quest:1".into(),
            location: "local_custody".into(),
            decomposition: "early".into(),
            case_site_id: "site:1".into(),
            settlement_id: "viabundus-1".into(),
            opened: false,
            permission: "none".into(),
            exhumation_permission: false,
            penalty_free_burning: false,
            revision: 0,
            findings: Vec::new(),
        };
        let markup = settlement_overview_page(
            &settlement(),
            &[],
            &[],
            None,
            &[],
            None,
            std::slice::from_ref(&corpse),
            Some((&corpse, "physiology")),
        )
        .into_string();

        assert!(markup.contains("Bodies held in the settlement"));
        assert!(markup.contains("corpse-portrait"));
        assert!(markup.contains(
            "/locations/settlement/viabundus-1?corpse=corpse:quest:1&amp;medical=surgery"
        ));
        assert!(markup.contains("physiology-dialog"));
        assert!(markup.contains("action=\"/corpses/corpse:quest:1/action\""));
        assert!(markup.contains("name=\"return_to\""));
    }

    #[test]
    fn intentional_stages_have_distinct_semantics_and_no_prototype_copy() {
        for (kind, label) in [
            ("settlement", "At the settlement gates"),
            ("route", "Roads and destinations"),
            ("camp", "Camp beside the road"),
            ("character", "Adventurer profile"),
            ("service", "At the counter"),
            ("quest", "Encounter ground"),
            ("alchemy", "The apothecary workbench"),
            ("chest", "Shared party stores"),
        ] {
            let markup = visual_stage(kind, "A Place", "An intentional scene").into_string();
            assert!(markup.contains(label));
            assert!(markup.contains("role=\"img\""));
            assert!(!markup.contains("placeholder"));
            assert!(!markup.contains("TODO"));
            assert!(!markup.contains("visual-scene-emblem"));
            assert!(!markup.contains("/static/icons/game/"));
        }

        let character = visual_stage("character", "Ada", "Character sheet").into_string();
        assert!(character.contains("role=\"img\" aria-label=\"Ada. Character sheet\""));
        let css = include_str!("../../../static/css/strategic.css");
        let character_figure = css
            .split(".service-visual-character .visual-scene-horizon {")
            .nth(1)
            .and_then(|tail| tail.split('}').next())
            .expect("character stage needs a restrained silhouette");
        assert!(character_figure.contains("/static/icons/game/person.svg"));
        assert!(character_figure.contains("width: clamp(5rem, 22%, 8rem);"));
        assert!(character_figure.contains("top: 8%;"));
        assert!(character_figure.contains("clip-path: inset(0 0 22%);"));
        assert!(!character_figure.contains("border-radius"));
    }

    #[test]
    fn places_navigation_exposes_the_generated_public_square_referral_tab() {
        use adventuresim_core::settlement_economy::{player_visible_npc_tabs, visible_npc_tab};

        let settlement = settlement();
        let tabs = player_visible_npc_tabs(&settlement.economy, true, &settlement.id);
        let public_square = visible_npc_tab(&tabs, "overview").unwrap();
        assert_eq!(public_square.label, "Public square");

        let overview =
            settlement_overview_page(&settlement, &[], &[], None, &[], Some("Visitor"), &[], None)
                .into_string();
        assert!(overview.contains("aria-label=\"Settlement services\""));
        assert!(overview.contains("aria-label=\"Public square\""));
        assert!(overview.contains("href=\"/locations/settlement/viabundus-1\""));
        assert!(!overview.contains("aria-label=\"Settlement places\""));
        assert!(overview.contains("data-strategic-tooltip=\"Imported estimate:"));
        assert!(overview.contains("data-developer-only"));

        let character = Character {
            id: 1,
            name: "Visitor".into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: Some(settlement.id.clone()),
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        };
        let residences = settlement_resident_location_page(
            &settlement,
            &character,
            &[],
            "residences",
            Some("Visitor"),
        )
        .into_string();
        let residence_places = residences
            .split("aria-label=\"Settlement places\"")
            .nth(1)
            .and_then(|tail| tail.split("</nav>").next())
            .expect("residence Places navigation");
        assert!(residences.contains("class=\"settlement-places-nav\""));
        assert!(residence_places.contains("href=\"/locations/settlement/viabundus-1\""));
        assert!(residence_places.contains(&format!(">{}</a>", public_square.label)));
        assert!(residence_places.contains("aria-current=\"false\""));
        assert!(residence_places.contains("class=\"active\" aria-current=\"page\">Residences</a>"));

        let mut colocated = settlement;
        colocated.id = "viabundus-0".into();
        colocated.economy.services = vec![adventuresim_world_schema::SettlementService::Market];
        let mut colocated_character = character;
        colocated_character.current_settlement_id = Some(colocated.id.clone());
        let colocated_places = settlement_resident_location_page(
            &colocated,
            &colocated_character,
            &[],
            "residences",
            Some("Visitor"),
        )
        .into_string();
        assert!(!colocated_places.contains("organization-merchant-guild"));
        assert!(colocated_places.contains("organization-physicians-college"));
        assert!(colocated_places.contains("organization-surgeons-guild"));

        let components = include_str!("../../../static/css/components.css");
        let components = components.replace("\r\n", "\n");
        assert!(components.contains(".settlement-places-nav {\n  display: grid;\n  gap: 0.3rem;"));
        assert!(components.contains(
            ".settlement-places-nav a:is(:hover, :focus-visible, .active, [aria-current=\"page\"])"
        ));
    }

    #[test]
    fn residence_panel_compares_three_tiers_and_names_family_without_raw_time_or_ids() {
        let settlement = settlement();
        let offers = [
            ResidenceTier::Cheap,
            ResidenceTier::Moderate,
            ResidenceTier::Fancy,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, tier)| SettlementResidenceOffer {
            id: format!("home-{index}"),
            settlement_id: settlement.id.clone(),
            tier,
            purchase_price: 100 + index as u32 * 100,
            rent_per_period: 10 + index as u32 * 10,
            owner_maintenance_per_period: 3 + index as u32,
            property_tax_per_period: 2 + index as u32,
            leisure_morale_basis_points: 40 + index as u16 * 40,
        })
        .collect::<Vec<_>>();
        let relationship = RelationshipPresentation {
            spouse_name: Some("Anna".into()),
            courtship_partner_name: Some("Bea".into()),
            courtship_kind: Some(CourtshipKind::Formal),
            courtship_exposed: false,
            wedding: Some(WeddingPresentation {
                days_remaining: 365,
                date_label: "year 1546, day 120".into(),
            }),
            pregnancy_due_days: Some(270),
            children: vec![ChildPresentation {
                name: "Elsa".into(),
                stage: ChildStage::MiddleChildhood,
                focus: ChildActivityFocus::Study,
                maturity_basis_points: 4_000,
                adult_playable: false,
                alive: true,
            }],
        };
        let holdings = vec![
            BackendCharacterResidenceStatus {
                character_id: 1,
                holding_id: "holding-primary".into(),
                owner_character_id: 1,
                settlement_id: settlement.id.clone(),
                tier: ResidenceTier::Moderate,
                tenure: ResidenceTenure::Owner,
                active: true,
                primary: true,
                occupied: true,
                acquired_minute: 0,
                last_billed_minute: 0,
                next_due_minute: 43_200,
            },
            BackendCharacterResidenceStatus {
                character_id: 1,
                holding_id: "holding-dormant".into(),
                owner_character_id: 1,
                settlement_id: "elsewhere".into(),
                tier: ResidenceTier::Cheap,
                tenure: ResidenceTenure::Owner,
                active: false,
                primary: false,
                occupied: false,
                acquired_minute: 0,
                last_billed_minute: 0,
                next_due_minute: 43_200,
            },
            BackendCharacterResidenceStatus {
                character_id: 1,
                holding_id: "holding-household".into(),
                owner_character_id: 2,
                settlement_id: settlement.id.clone(),
                tier: ResidenceTier::Fancy,
                tenure: ResidenceTenure::Owner,
                active: true,
                primary: false,
                occupied: true,
                acquired_minute: 0,
                last_billed_minute: 0,
                next_due_minute: 43_200,
            },
        ];
        let markup = residence_offer_panel(
            &settlement,
            1,
            &offers,
            &holdings,
            Some(&relationship),
            None,
        )
        .into_string();

        assert_eq!(markup.matches("class=\"residence-offer\"").count(), 3);
        assert_eq!(markup.matches("class=\"residence-holding\"").count(), 3);
        assert!(markup.contains("name=\"holding_id\" value=\"holding-primary\""));
        assert!(markup.contains("name=\"holding_id\" value=\"holding-dormant\""));
        assert!(!markup.contains("name=\"holding_id\" value=\"holding-household\""));
        assert!(markup.contains("Fancy"));
        assert!(markup.contains("Household home"));
        assert!(markup.contains("Recover owned home"));
        for tier in ["cheap", "moderate", "fancy"] {
            assert!(markup.contains(&format!("data-residence-tier=\"{tier}\"")));
        }
        assert_eq!(markup.matches("class=\"residence-meter-list\"").count(), 3);
        assert_eq!(markup.matches("residence-meter-leisure").count(), 3);
        assert!(markup.contains("data-strategic-tooltip=\"Owner upkeep:"));
        assert!(markup.contains("Leisure morale bonus:"));
        assert!(markup.contains("onsubmit=\"return confirm("));
        assert!(markup.contains("Rent this Cheap residence"));
        assert!(markup.contains("Relinquish this property? This cannot be undone."));
        assert!(markup.contains("Spouse: Anna"));
        assert!(markup.contains("Child: Elsa"));
        assert!(markup.contains("middle childhood; focus: study"));
        assert!(markup.contains("household-child-stage-middle"));
        assert!(markup.contains("household-progress household-maturity"));
        assert!(markup.contains("role=\"meter\""));
        assert!(!markup.contains("Elsa, age"));
        assert!(markup.contains("formal courtship with Bea; formal and public"));
        assert!(!markup.contains("formal courtship with Bea; private"));
        assert!(markup.contains("household-progress household-wedding"));
        assert!(markup.contains("household-progress household-pregnancy"));
        assert!(markup.contains("role=\"meter\""));
        assert!(markup.contains("aria-valuenow="));
        assert!(markup.contains("role=\"img\""));
        assert!(markup.contains("aria-label=\"Spouse: Anna\""));
        assert!(markup.contains(
            "aria-label=\"Child: Elsa; middle childhood; focus: study; maturity 40 percent\""
        ));
        assert!(!markup.contains("character #"));
        assert!(!markup.contains("strategic minute"));
    }

    #[test]
    fn early_child_activity_is_care_regardless_of_future_focus() {
        assert_eq!(
            child_activity_presentation(ChildStage::EarlyChildhood, ChildActivityFocus::Study),
            ("bed", "care and rest")
        );
        assert_eq!(
            child_activity_presentation(ChildStage::MiddleChildhood, ChildActivityFocus::Study),
            ("open-book", "study")
        );
    }

    #[test]
    fn responsive_and_hidden_control_rules_keep_content_available() {
        let layout = include_str!("../../../static/css/layout.css").replace("\r\n", "\n");
        let strategic = include_str!("../../../static/css/strategic.css");
        let utilities = include_str!("../../../static/css/utilities.css");
        assert!(layout.contains("grid-template-areas: \"main\" \"left\" \"right\""));
        assert!(
            layout.contains(".right-sidebar {\n    display: block;")
                || layout.contains(".right-sidebar {\n    display: block;")
                || layout.contains(".right-sidebar {\n  display: block;")
        );
        assert!(strategic.contains("@media (hover: none), (pointer: coarse)"));
        assert!(utilities.contains(".inventory-count:focus-within"));
        assert!(utilities.contains("@media (hover:none), (pointer:coarse)"));
        assert!(utilities.contains("width: 2.75rem"));
        assert!(utilities.contains("height: 2.75rem"));
        assert!(utilities.contains("grid-template-columns: 2.75rem 1.4rem 2.75rem"));
        for scene in [
            "settlement",
            "route",
            "camp",
            "quest",
            "character",
            "service",
            "alchemy",
            "chest",
        ] {
            assert!(strategic.contains(&format!(".service-visual-{scene}")));
        }
    }
}
