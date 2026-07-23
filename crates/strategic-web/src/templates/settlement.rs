//! Settlement templates.
//!
//! Settlement pages deliberately keep the same ownership model: services and
//! settlement-owned information on the left, service context in the center,
//! and the active player's party on the right.

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
    CharacterStrategicCondition, CharacterTrainingSchedule, FoodLot, InventoryItem,
    InventoryQuantityTarget, ItemDefinition, ItemSlot, JourneyTerrainKind, LimbInjury, LimbRegion,
    Party, PartyInventoryItem, PartyJourney, PartyJourneyItinerary, PartyJourneyRoute,
    ProjectileKind, Quest, QuestStatus, RetainedProjectile, ScheduleAllocation, Settlement,
    SettlementAlias, SettlementCategory, SettlementDescription, SettlementDescriptionKind,
    StrategicEncounter,
};

#[derive(Clone, Debug)]
pub struct LocationView {
    pub kind: LocationKind,
    pub id: String,
    pub name: String,
    pub religion_id: Option<String>,
    pub category: Option<SettlementCategory>,
    pub active_building: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ActivityPreviewRates {
    labor_gold_per_hour: f32,
    thievery_gold_per_hour: f32,
    thievery_virtue_per_hour: f32,
    raiding_gold_per_hour: f32,
    raiding_virtue_per_hour: f32,
    current_fatigue: f32,
    profession: std::collections::BTreeMap<String, ProfessionActivityPreview>,
}

#[derive(Clone, Debug)]
struct ProfessionActivityPreview {
    training_rates: Vec<(String, f32)>,
    apprenticeship_accrued: u64,
    practice_accrued: u64,
    practice_threshold: u64,
    practice_reward: &'static str,
    tier_label: &'static str,
}

const PROFESSION_ACCRUAL_SCALE: u64 = MINUTES_PER_DAY;
const APPRENTICESHIP_REWARD_THRESHOLD: u64 = 8 * 60 * PROFESSION_ACCRUAL_SCALE;

impl ProfessionActivityPreview {
    fn reward_delta(&self, allocation_name: &str, minutes: u16) -> [f32; 2] {
        let (accrued, threshold, sign, reward) = match allocation_name {
            "apprenticeship_minutes" => (
                self.apprenticeship_accrued,
                APPRENTICESHIP_REWARD_THRESHOLD,
                -1.0,
                "gold",
            ),
            "profession_practice_minutes" => (
                self.practice_accrued,
                self.practice_threshold,
                1.0,
                self.practice_reward,
            ),
            _ => return [0.0, 0.0],
        };
        let after = accrued.saturating_add(u64::from(minutes) * PROFESSION_ACCRUAL_SCALE);
        let delta = (after / threshold).saturating_sub(accrued / threshold) as f32 * sign;
        if reward == "virtue" {
            [0.0, delta]
        } else {
            [delta, 0.0]
        }
    }
}

impl ActivityPreviewRates {
    pub fn from_character(
        attributes: Option<&CharacterAttributes>,
        skills: Option<&CharacterSkills>,
        limbs: Option<&CharacterLimbs>,
        capability: Option<&CharacterCapability>,
        settlement: Option<&Settlement>,
        stats: Option<&CharacterStats>,
    ) -> Self {
        let current_fatigue = stats.map_or(0.0, |stats| stats.calories_used.max(0.0));
        let (Some(attributes), Some(skills), Some(limbs), Some(capability), Some(settlement)) =
            (attributes, skills, limbs, capability, settlement)
        else {
            return Self {
                current_fatigue,
                ..Self::default()
            };
        };
        let limb_health = [
            limbs.left_arm_health,
            limbs.right_arm_health,
            limbs.left_leg_health,
            limbs.right_leg_health,
        ];
        let strength = [
            attributes.left_arm_strength,
            attributes.right_arm_strength,
            attributes.left_leg_strength,
            attributes.right_leg_strength,
        ]
        .into_iter()
        .zip(limb_health)
        .map(|(value, health)| value * health.clamp(0.0, 1.0) * 0.25)
        .sum::<f32>();
        let agility = [
            attributes.left_arm_agility,
            attributes.right_arm_agility,
            attributes.left_leg_agility,
            attributes.right_leg_agility,
        ]
        .into_iter()
        .zip(limb_health)
        .map(|(value, health)| value * health.clamp(0.0, 1.0) * 0.25)
        .sum::<f32>();
        let usable_limbs = limb_health
            .into_iter()
            .map(|health| health.clamp(0.0, 1.0) * 0.25)
            .sum::<f32>();
        let precision = attributes.precision * usable_limbs;
        let stealth =
            (Skill::Stealth.training_rank(skills.stealth_hours) + agility + precision) * 0.5;
        let endurance = attributes.endurance * limbs.chest_health.clamp(0.0, 1.0);
        let population = settlement_population_scale(
            settlement.population_level,
            settlement.population_estimate,
        );
        let combat = capability
            .weapon_precision
            .max(capability.athletics)
            .max(capability.endurance);
        Self {
            labor_gold_per_hour: (strength.max(0.0) + endurance.max(0.0)) / 8.0,
            thievery_gold_per_hour: population.max(0.0) * (1.0 + stealth.max(0.0)) / 8.0,
            thievery_virtue_per_hour: -population.max(0.0) * 0.5 / (1.0 + stealth.max(0.0)),
            raiding_gold_per_hour: (2.0 + combat.max(0.0)) / 6.0,
            raiding_virtue_per_hour: -1.5,
            current_fatigue,
            profession: Default::default(),
        }
    }

    pub fn with_professions(
        mut self,
        skills: Option<&CharacterSkills>,
        apprenticeships: &[CharacterApprenticeship],
    ) -> Self {
        let Some(skills) = skills else { return self };
        for row in apprenticeships {
            let Some(definition) =
                adventuresim_core::profession::profession_for_service(&row.service_id)
            else {
                continue;
            };
            let hours = |skill: Skill| match skill {
                Skill::Command => skills.command_hours,
                Skill::Smithing => skills.smithing_hours,
                Skill::Tailoring => skills.tailoring_hours,
                Skill::Medicine => skills.medicine_hours,
                Skill::Anatomy => skills.anatomy_hours,
                Skill::Knife => skills.knife_hours,
                Skill::Cooking => skills.cooking_hours,
                Skill::Religion => row
                    .religion_id
                    .as_deref()
                    .and_then(OfficialReligion::from_id)
                    .map_or(0.0, |religion| skills.religion_hours.direct(religion)),
                _ => 0.0,
            };
            let tier = adventuresim_core::profession::profession_tier(definition, hours);
            let practice_threshold = match tier {
                adventuresim_core::profession::ProfessionTier::Master => 2 * 60 * MINUTES_PER_DAY,
                _ => 8 * 60 * MINUTES_PER_DAY,
            };
            let practice_reward = match definition.practice_reward {
                adventuresim_core::profession::PracticeReward::Gold => "gold",
                adventuresim_core::profession::PracticeReward::Virtue => "virtue",
            };
            self.profession.insert(
                row.service_id.clone(),
                ProfessionActivityPreview {
                    training_rates: definition
                        .skills
                        .iter()
                        .map(|entry| (format!("{:?}", entry.skill), entry.weight))
                        .collect(),
                    apprenticeship_accrued: row.apprenticeship_minutes_accrued,
                    practice_accrued: row.practice_minutes_accrued,
                    practice_threshold,
                    practice_reward,
                    tier_label: tier.title(definition.religious),
                },
            );
        }
        self
    }
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

    pub fn preserve_building(&self, path: String) -> String {
        self.active_building
            .as_deref()
            .map_or(path.clone(), |building| {
                format!(
                    "{path}{}building={building}",
                    if path.contains('?') { "&" } else { "?" }
                )
            })
    }

    fn render_layout(&self, title: &str, content: Markup, logged_in_as: Option<&str>) -> Markup {
        if self.kind == LocationKind::Settlement {
            settlement_layout_with_session(
                title,
                &self.name,
                &self.id,
                self.category
                    .as_ref()
                    .unwrap_or(&SettlementCategory::Unknown),
                self.active_building.as_deref().unwrap_or(""),
                self.religion_id.as_deref(),
                None,
                content,
                logged_in_as,
            )
        } else {
            quest_location_layout_with_session(
                title,
                &self.name,
                &self.id,
                "",
                content,
                logged_in_as,
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
    Herbalist,
    Inn,
}

pub struct RestSummary {
    pub minutes: u64,
    pub gold_spent: u32,
    pub gold_earned: u32,
    pub notoriety_gained: f32,
    pub healed: Vec<(String, f32)>,
    pub trained: Vec<(String, f32)>,
}

impl MerchantShop {
    pub fn storefront(self) -> adventuresim_core::settlement_economy::Storefront {
        use adventuresim_core::settlement_economy::Storefront as S;
        match self {
            Self::General => S::General,
            Self::Weapons => S::Weapons,
            Self::Armor => S::Armor,
            Self::Clothing => S::Clothing,
            Self::Herbalist => S::Herbalist,
            Self::Inn => S::Inn,
        }
    }

    pub fn available_at(self, settlement: &Settlement) -> bool {
        adventuresim_core::settlement_economy::storefront_available(
            &settlement.economy,
            self.storefront(),
        )
    }

    fn stocks_at(self, settlement: &Settlement, item: &crate::spacetimedb::ItemDefinition) -> bool {
        use adventuresim_core::settlement_economy::CatalogKind as C;
        let kind = match item.kind {
            crate::spacetimedb::ItemKind::Simple => C::Simple,
            crate::spacetimedb::ItemKind::Weapon => C::Weapon,
            crate::spacetimedb::ItemKind::Armor => C::Armor,
            crate::spacetimedb::ItemKind::Shield => C::Shield,
            crate::spacetimedb::ItemKind::Clothing => C::Clothing,
            crate::spacetimedb::ItemKind::Currency => C::Currency,
            crate::spacetimedb::ItemKind::Ingredient => C::Ingredient,
            crate::spacetimedb::ItemKind::Medication => C::Medication,
            crate::spacetimedb::ItemKind::Food => C::Food,
        };
        adventuresim_core::settlement_economy::storefront_stocks(
            &settlement.economy,
            self.storefront(),
            &item.id,
            kind,
        )
    }
    pub fn service_id(self) -> &'static str {
        match self {
            Self::General => "merchants",
            Self::Weapons => "weapons",
            Self::Armor => "armor",
            Self::Clothing => "clothing",
            Self::Herbalist => "herbalist",
            Self::Inn => "inn",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::General => "General Market",
            Self::Weapons => "Weaponsmith",
            Self::Armor => "Armourer",
            Self::Clothing => "Tailor",
            Self::Herbalist => "Herbalist",
            Self::Inn => "The Inn",
        }
    }

    fn stocks(self, item: &crate::spacetimedb::ItemDefinition) -> bool {
        let kind = item.kind;
        match self {
            Self::General => !matches!(
                kind,
                crate::spacetimedb::ItemKind::Currency
                    | crate::spacetimedb::ItemKind::Ingredient
                    | crate::spacetimedb::ItemKind::Medication
            ),
            Self::Weapons => matches!(
                kind,
                crate::spacetimedb::ItemKind::Weapon | crate::spacetimedb::ItemKind::Shield
            ),
            Self::Armor => kind == crate::spacetimedb::ItemKind::Armor,
            Self::Clothing => kind == crate::spacetimedb::ItemKind::Clothing,
            Self::Herbalist => matches!(
                kind,
                crate::spacetimedb::ItemKind::Ingredient | crate::spacetimedb::ItemKind::Medication
            ),
            Self::Inn => {
                adventuresim_core::food::definition(&item.id).is_some()
                    || matches!(
                        item.id.as_str(),
                        "cooking_pan" | "cooking_pot" | "portable_oven"
                    )
            }
        }
    }

    fn shows_inventory(self, item: &crate::spacetimedb::ItemDefinition) -> bool {
        item.kind == crate::spacetimedb::ItemKind::Currency || self.stocks(item)
    }
}

pub fn alchemy_page(
    settlement: &Settlement,
    character: &Character,
    party_members: &[Character],
    medicine: f32,
    selected: &adventuresim_core::disease::MedicationRecipe,
    inventory: &[InventoryItem],
    pooled: &[PartyInventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    personal_targets: &[InventoryQuantityTarget],
    party_targets: &[InventoryQuantityTarget],
    party_scope: bool,
) -> Markup {
    let scope = if party_scope { "party" } else { "personal" };
    let return_to = format!(
        "/locations/settlement/{}/alchemy?recipe={}&scope={scope}",
        settlement.id, selected.item_id
    );
    let herbalist_href = format!(
        "/settlements/{}/herbalist?return_to={}",
        settlement.id,
        return_to
            .replace('%', "%25")
            .replace('?', "%3F")
            .replace('&', "%26")
            .replace('=', "%3D")
    );
    let recipes: Vec<_> = adventuresim_core::disease::MEDICATION_RECIPES
        .iter()
        .filter(|recipe| adventuresim_core::disease::can_prepare_medication(medicine, recipe))
        .collect();
    let content = html! {
        aside class="left-sidebar alchemy-recipes" {
            (sidebar_section("Preparations", html! {
                nav class="alchemy-recipe-list" aria-label="Known medication recipes" {
                    @for recipe in recipes {
                        a class=[(recipe.item_id == selected.item_id).then_some("active")]
                            href=(format!("/locations/settlement/{}/alchemy?recipe={}&scope={}", settlement.id, recipe.item_id, if party_scope { "party" } else { "personal" })) {
                            strong { (recipe.name) }
                            span { "Medicine " (recipe.medicine_dc) " · " (recipe.preparation_minutes) " minutes" }
                        }
                    }
                }
            }))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(character), &format!("/locations/settlement/{}", settlement.id), Some(character.id), true))
            (visual_stage("alchemy", "Alchemy", "A working table of herbs, vessels, and prepared medicines"))
            a class="stage-context-link" href=(herbalist_href) {
                "Return to the herbalist"
            }
            (settlement_chat_area_with_info("Alchemy", Some(character), &[format!("Selected recipe: {}", selected.name)]))
            form method="post" action=(format!("/locations/settlement/{}/alchemy/craft", settlement.id)) class="alchemy-craft-form" {
                input type="hidden" name="disease_id" value=(format!("{:?}", selected.disease_id).to_ascii_lowercase());
                input type="hidden" name="party_scope" value=(party_scope);
                button type="submit" class="btn btn-primary" {
                    "Prepare " (selected.name) " · " (selected.preparation_minutes) " minutes"
                }
            }
        }
        aside class="right-sidebar inventory-owner-panel" data-inventory-tabs {
            nav class="inventory-owner-tabs" aria-label="Ingredient inventory" {
                a class=(if !party_scope { "inventory-owner-tab active" } else { "inventory-owner-tab" })
                    href=(format!("/locations/settlement/{}/alchemy?recipe={}&scope=personal", settlement.id, selected.item_id)) { "Player" }
                a class=(if party_scope { "inventory-owner-tab active" } else { "inventory-owner-tab" })
                    href=(format!("/locations/settlement/{}/alchemy?recipe={}&scope=party", settlement.id, selected.item_id)) { "Party" }
            }
            (sidebar_section("Required ingredients", html! {
                table class="trade-inventory-table" {
                    (trade_inventory_table_header(false, None))
                    tbody {
                    @for ingredient in selected.ingredients {
                        @let definition = items.iter().find(|item| item.id == ingredient.item_id);
                        @let quantity = if party_scope {
                            pooled.iter().filter(|item| item.item_id == ingredient.item_id).map(|item| item.quantity).sum()
                        } else {
                            inventory.iter().filter(|item| item.item_id == ingredient.item_id).map(|item| item.qty).sum()
                        };
                        @let target = target_quantity(if party_scope { party_targets } else { personal_targets }, ingredient.item_id);
                        tr class="trade-inventory-row trade-row-player" data-inventory-quantity=(quantity) data-target=(target) {
                            td class="inventory-item-type" { (item_type_icon(ingredient.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(ingredient.item_id, definition)) }
                            td class="inventory-count" { (quantity_target_control(quantity, target, ingredient.item_id, party_scope)) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (format!("need {}", ingredient.quantity)) }
                        }
                    }
                    }
                }
                p class="small-copy text-muted" { "Targets can be raised here for future purchasing. Crafting consumes the listed quantities from the selected inventory." }
            }))
        }
    };
    settlement_layout_with_session(
        "Alchemy",
        &settlement.name,
        &settlement.id,
        &settlement.category,
        "herbalist",
        Some(&settlement.religion_id),
        Some(&settlement.economy),
        content,
        Some(&character.name),
    )
}

/// Settlement information and the next destinations on the imported road and
/// ferry network.
pub fn settlement_overview_page(
    settlement: &Settlement,
    aliases: &[SettlementAlias],
    descriptions: &[SettlementDescription],
    active_character: Option<&Character>,
    party_members: &[Character],
    logged_in_as: Option<&str>,
) -> Markup {
    let alias_labels = settlement_alias_labels(settlement, aliases);
    let historical_description = preferred_settlement_description(descriptions);
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Settlement", html! {
                div class="settlement-summary" {
                    dl class="location-stat-list" {
                        div { dt { "Population" } dd { (format_population(settlement)) } }
                        div { dt { "Size" } dd { (population_description(settlement.population_level)) } }
                        div { dt { "Prosperity" } dd { (format!("{:?} ({}/1000)", settlement.economy.prosperity_tier, settlement.economy.prosperity_score)) } }
                        div { dt { "Services" } dd { (settlement.economy.services.iter().map(|v| format!("{:?}", v)).collect::<Vec<_>>().join(", ")) } }
                        div { dt { "Specialties" } dd { (settlement.economy.specializations.iter().map(|v| format!("{:?}", v)).collect::<Vec<_>>().join(", ")) } }
                        div { dt { "Faiths" } dd { (settlement.religious_status.represented_religions().iter().map(|r| r.label()).collect::<Vec<_>>().join(", ")) } }
                        div { dt { "Coordinates" } dd { (format!("{}, {}", settlement.coord_x as i32, settlement.coord_y as i32)) } }
                        div { dt { "Languages" } dd { (format!(
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
            (visual_stage("settlement", &settlement.name, "Streets, landmarks, and the settlement approach"))
            (settlement_chat_area(&settlement.name, active_character))
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

pub fn settlement_map_page(
    settlement: &Settlement,
    settlements: &[Settlement],
    quests: &[Quest],
    strategic_map: Option<&crate::strategic_map::StrategicMap>,
    destinations: &[TravelDestination],
    selected_id: Option<&str>,
    active_character: Option<&Character>,
    active_party: Option<&Party>,
    _party_members: &[Character],
    default_rest_minutes: u64,
    soap_preview: SoapRestPreview,
    can_travel: bool,
    provision_forecast: Option<&TravelProvisionForecast>,
    is_current_settlement: bool,
    current_open_quest_available: bool,
    current_turn_in_ready: bool,
    abandonable_quest: Option<&Quest>,
    logged_in_as: Option<&str>,
) -> Markup {
    let selected = selected_id.and_then(|id| destinations.iter().find(|entry| entry.id == id));
    let selected_settlement =
        selected_id.and_then(|id| settlements.iter().find(|entry| entry.id == id));
    let selected_quest = selected_id.and_then(|id| quests.iter().find(|entry| entry.id == id));
    let base_path = format!("/locations/settlement/{}/map", settlement.id);
    let connected_ids = destinations
        .iter()
        .filter(|destination| !destination.quest_in_progress)
        .map(|destination| destination.id.as_str())
        .collect::<BTreeSet<_>>();
    let content = html! {
        (map_destination_list_with_context(
            destinations,
            selected_id,
            &base_path,
            is_current_settlement.then_some(MapCurrentLocation {
                name: &settlement.name,
                open_quest_available: current_open_quest_available,
                turn_in_ready: current_turn_in_ready,
            }),
            abandonable_quest.map(|quest| MapAbandonableQuest {
                id: &quest.id,
                title: &quest.title,
            }),
            active_party.filter(|_| can_travel).map(|_| html! {
                section class="rest-service-menu map-rest-menu" aria-label="Rest" {
                    (party_rest_menu(
                        &format!("{base_path}/rest"),
                        "map-rest",
                        "Rest here",
                        "Rest party",
                        default_rest_minutes,
                        None,
                        soap_preview,
                    ))
                }
            }),
        ))
        main class="center-content settlement-main settlement-map-main" {
            @if settlement.source_node_id.is_some() {
                @if let Some(strategic_map) = strategic_map {
                    (crate::strategic_map::strategic_map(
                        strategic_map,
                        settlements,
                        quests,
                        &settlement.id,
                        &connected_ids,
                        selected_id,
                        &base_path,
                        selected.and_then(|destination| destination.terrain_route.as_ref()),
                    ))
                } @else {
                    (crate::strategic_map::strategic_map_bundle_unavailable())
                }
            } @else {
                (crate::strategic_map::strategic_map_unavailable(&settlement.name))
            }
        }
        (map_destination_detail(
            selected,
            selected_settlement,
            selected_quest,
            selected_settlement.is_some_and(|destination| destination.id == settlement.id),
            can_travel,
            true,
            provision_forecast,
            active_party,
            active_party.is_some_and(|party| party.leader_id == active_character.map_or(0, |character| character.id)),
            None,
            &base_path,
        ))
    };
    settlement_layout_with_session(
        &format!("{} map", settlement.name),
        &settlement.name,
        &settlement.id,
        &settlement.category,
        "map",
        Some(&settlement.religion_id),
        Some(&settlement.economy),
        content,
        logged_in_as,
    )
}

#[derive(Clone, Copy)]
struct MapCurrentLocation<'a> {
    name: &'a str,
    open_quest_available: bool,
    turn_in_ready: bool,
}

#[derive(Clone, Copy)]
struct MapAbandonableQuest<'a> {
    id: &'a str,
    title: &'a str,
}

#[cfg(test)]
pub(crate) fn map_destination_list(
    destinations: &[TravelDestination],
    selected_id: Option<&str>,
    base_path: &str,
) -> Markup {
    map_destination_list_with_context(destinations, selected_id, base_path, None, None, None)
}

pub(crate) fn map_destination_list_with_rest(
    destinations: &[TravelDestination],
    selected_id: Option<&str>,
    base_path: &str,
    rest_menu: Markup,
) -> Markup {
    map_destination_list_with_context(
        destinations,
        selected_id,
        base_path,
        None,
        None,
        Some(rest_menu),
    )
}

fn map_destination_list_with_context(
    destinations: &[TravelDestination],
    selected_id: Option<&str>,
    base_path: &str,
    current_location: Option<MapCurrentLocation<'_>>,
    abandonable_quest: Option<MapAbandonableQuest<'_>>,
    rest_menu: Option<Markup>,
) -> Markup {
    html! {
        aside class=(if rest_menu.is_some() { "left-sidebar map-rest-sidebar" } else { "left-sidebar" }) {
            div class=[rest_menu.is_some().then_some("map-rest-sidebar-content")] {
            (sidebar_section("Destinations", html! {
                @if destinations.is_empty() && current_location.is_none() {
                    (empty_state("No destinations are available from this location.", None, None))
                } @else {
                    nav class="location-destination-list" aria-label="Travel destinations" {
                        @if let Some(current) = current_location {
                            div class="list-item travel-destination-row current-location-row"
                                aria-current="location" {
                                strong { (current.name) }
                                span class="text-muted small-copy current-location-label" { "Current" }
                                @if current.turn_in_ready {
                                    span class="destination-quest-badge" title="Active quest ready to turn in here"
                                        aria-label="Active quest ready to turn in here" { "!" }
                                } @else if current.open_quest_available {
                                    span class="destination-open-quest-badge" title="Open quest available here"
                                        aria-label="Open quest available here" { "!" }
                                }
                            }
                        }
                        @for destination in destinations {
                            @let destination_tooltip = quest_destination_tooltip(destination);
                            a href=(format!("{}?destination={}", base_path, destination.id))
                                class=(if selected_id == Some(destination.id.as_str()) { "list-item travel-destination-row active" } else { "list-item travel-destination-row" })
                                title=[destination_tooltip.as_deref()]
                                data-travel-name=(&destination.name)
                                data-travel-description=[destination_tooltip.as_deref()]
                                data-travel-minutes=(destination.journey_minutes)
                                data-travel-round-trip=(destination.quest_in_progress)
                                data-travel-camp-stops=(format_camp_stops(&destination.camp_stop_minutes))
                                data-travel-camp-forecasts=(format_camp_forecasts(destination))
                                data-travel-distance=(format_distance(destination.distance_m)) {
                                @if let Some(forecast) = &destination.provision_forecast {
                                    span hidden data-provision-payload
                                        data-planning-minutes=(forecast.planning_minutes)
                                        data-living-members=(forecast.living_members)
                                        data-food-days=(forecast.food_days)
                                        data-water-days=(forecast.water_days)
                                        data-ordinary-water-days=(forecast.ordinary_water_days)
                                        data-emergency-alcohol-days=(forecast.emergency_alcohol_days)
                                        data-ration-kcal=(forecast.ration_kcal)
                                        data-waterskin-ml=(forecast.waterskin_capacity_ml) {}
                                }
                                strong { (&destination.name) }
                                @if destination.quest_in_progress {
                                    span class="destination-quest-badge" title=(destination_tooltip.as_deref().unwrap_or("Active quest destination"))
                                        aria-label="Active quest destination" { "!" }
                                } @else if destination.active_quest_route {
                                    span class="destination-quest-badge" title="Next settlement toward active quest"
                                        aria-label="Next settlement toward active quest" { "!" }
                                } @else if destination.turn_in_ready {
                                    span class="destination-quest-badge" title="Active quest ready to turn in here"
                                        aria-label="Active quest ready to turn in here" { "!" }
                                } @else if destination.open_quest_available {
                                    span class="destination-open-quest-badge" title="Open quest available here"
                                        aria-label="Open quest available here" { "!" }
                                }
                                span class="text-muted small-copy" { (format_distance(destination.distance_m)) }
                            }
                        }
                    }
                }
                @if let Some(quest) = abandonable_quest {
                    div class="map-active-quest-actions" {
                        p class="small-copy" { "Active quest: " strong { (quest.title) } }
                        form method="post" action=(format!("/quests/{}/abandon", quest.id)) {
                            button type="submit" class="btn btn-danger btn-small" { "Abandon active quest" }
                        }
                    }
                }
            }))
            }
            @if let Some(rest_menu) = rest_menu {
                (rest_menu)
            }
        }
    }
}

pub(crate) fn travel_preferences_form(party: &Party, action: &str) -> Markup {
    let walking_hours = f32::from(party.walking_minutes_per_day) / 60.0;
    let travel_at_night = party.travel_at_night;
    let walking_hours_title = if travel_at_night {
        "Walking is centered on midnight; shorter first and final days are forecast automatically."
    } else {
        "Walking is centered on solar noon; shorter first and final days are forecast automatically."
    };
    html! {
        form method="post" action=(action) class="travel-configuration-form" data-travel-configuration {
            div class="travel-setting-heading" title=(walking_hours_title) {
                label for="walking-hours" { "Walking hours per day" }
                span class="travel-walking-value" {
                    output for="walking-hours" data-walking-hours-output { (format!("{walking_hours}")) }
                    span aria-hidden="true" { " h" }
                }
            }
            div class="travel-fatigue-control" {
                input id="walking-hours" type="range" name="walking_hours" min="0" max="24" step="0.25" value=(walking_hours) data-walking-hours {}
            }
            div class="travel-period-control" {
                span { "Travel during" }
                label class="travel-period-toggle" title=(if travel_at_night { "Travel at night; camp time is centered on noon" } else { "Travel during the day; walking time is centered on noon" }) {
                    input type="checkbox" name="travel_at_night" value="true" checked[travel_at_night]
                        aria-label="Travel at night" data-travel-period-toggle;
                    span class="travel-period-track" aria-hidden="true" {
                        span class="travel-period-option travel-period-day" {}
                        span class="travel-period-option travel-period-night" {}
                        span class="travel-period-thumb" {}
                    }
                }
            }
        }
    }
}

pub(crate) fn map_destination_detail(
    selected: Option<&TravelDestination>,
    selected_settlement: Option<&Settlement>,
    selected_quest: Option<&Quest>,
    selected_is_current: bool,
    can_travel: bool,
    provisioning_available: bool,
    provision_forecast: Option<&TravelProvisionForecast>,
    party: Option<&Party>,
    can_configure_travel: bool,
    standalone_planner: Option<Markup>,
    map_path: &str,
) -> Markup {
    let camp_fatigue_percent = party.map_or(50, |party| party.camp_fatigue_percent);
    let travel_disabled = party.is_some_and(|party| party.walking_minutes_per_day == 0);
    let inspecting_nonroute = selected.is_none() && selected_settlement.is_some();
    let market_path = format!(
        "/settlements/{}/merchants",
        map_path
            .trim_end_matches("/map")
            .rsplit('/')
            .next()
            .unwrap_or("")
    );
    html! {
        aside class=(if party.is_some() && can_configure_travel && !inspecting_nonroute { "right-sidebar travel-configuration-sidebar" } else { "right-sidebar" }) {
            @if party.is_some() && can_configure_travel {
            (sidebar_section("Travel configuration", html! {
                div class=(if selected.is_some() { "travel-planner-vertical" } else { "travel-planner-vertical no-destination" }) {
                    (travel_planner_bar(selected, camp_fatigue_percent))
                }
                (travel_preferences_form(party.expect("party checked above"), &format!("{map_path}/travel-configuration")))
                @if provisioning_available {
                    div class="travel-provisioning-control" data-provisioning-control {
                        div class="travel-provisioning-input" {
                            input type="hidden" value="0" data-target-surplus;
                            span class="travel-provisioning-target" {
                                span id="target-surplus" class="travel-provisioning-value" data-target-surplus-display
                                    role="button" tabindex="0" aria-label="Target surplus in days"
                                    title="Click to edit target surplus" { "0" }
                                span class="travel-provisioning-unit" { "d surplus" }
                            }
                            span class="travel-provisioning-icons" {
                                span class="travel-provisioning-icon food" { (game_icon("Food", "meal")) }
                                span class="travel-provisioning-icon water" { (game_icon("Water", "water-drop")) }
                                @if let Some(forecast) = provision_forecast {
                                    span class="travel-provisioning-icon alcohol"
                                        title=(format!("Emergency alcohol adds {:.2} days of hydration", forecast.emergency_alcohol_days)) {
                                        (game_icon("Emergency alcohol hydration", "beer-stein"))
                                        span class="travel-provisioning-alcohol-days" { (format!("+{:.2}d", forecast.emergency_alcohol_days)) }
                                    }
                                }
                            }
                            @if let Some(forecast) = provision_forecast {
                                a class="btn btn-secondary" data-provision-buy
                                    data-market-path=(&market_path)
                                    data-initial-rations=(forecast.rations_to_buy)
                                    data-initial-waterskins=(forecast.waterskins_to_buy)
                                    href=(&market_path) { "Buy" }
                            } @else {
                                button type="button" class="btn btn-secondary" disabled title="Provision estimates are unavailable" { "Buy" }
                            }
                        }
                        @if selected.is_some() {
                            p class="text-muted small-copy" data-provisioning-status {
                                @if provision_forecast.is_none() { "Provision estimates are temporarily unavailable." }
                            }
                        }
                    }
                }
            }))
            }
            @if let Some(planner) = standalone_planner {
                (sidebar_section("Journey", html! {
                    div class="travel-planner-vertical" { (planner) }
                }))
            }
            @if let Some(destination) = selected {
                (sidebar_section("", html! {
                    @if can_travel {
                        form method="post" action=(&destination.travel_action) data-travel-submit {
                            button type="submit" class="btn btn-primary btn-block"
                                disabled[travel_disabled]
                                title=(if travel_disabled { "Increase walking hours above zero to begin the journey" } else { "Begin journey" }) {
                                "Begin journey"
                            }
                        }
                        p class="travel-action-status" data-travel-action-status role="alert" hidden {}
                    }
                    p class="text-muted small-copy" {
                        @if !destination.quest_in_progress {
                            @if let Some(summary) = &destination.summary { (summary) " · " }
                        }
                        (format_distance(destination.distance_m))
                        " · " (format_journey_time(destination.journey_minutes))
                        @if destination.route_fallback {
                            span class="travel-route-estimate-warning" { " · Legacy straight-line estimate" }
                        }
                    }
                }))
            } @else if let Some(quest) = selected_quest {
                (sidebar_section("Destination", html! {
                    h3 { (&quest.title) }
                    p { (&quest.description) }
                    @if !quest.location_description.is_empty() {
                        p class="text-muted small-copy" { (&quest.location_description) }
                    }
                    p class="no-direct-route" role="status" {
                        @match quest.status {
                            QuestStatus::Available => {
                                strong { "Quest destination." }
                                " Accept and activate this quest at its issuing settlement before travelling here."
                            }
                            QuestStatus::Accepted => {
                                strong { "Active quest destination." }
                                " Your party has accepted this quest, but cannot begin the journey from its current strategic location."
                            }
                            QuestStatus::Completed => {
                                strong { "Quest completed." }
                                " Return to the issuing settlement to report your success and claim the reward."
                            }
                        }
                    }
                }))
            } @else if let Some(destination) = selected_settlement {
                (sidebar_section("Destination", html! {
                    h3 { (&destination.name) }
                    p { (settlement_description(destination.population_level)) }
                    dl class="settlement-stats" {
                        div { dt { "Size" } dd { (format_population(destination)) } }
                    }
                    p class="no-direct-route" role="status" {
                        @if selected_is_current {
                            strong { "Current settlement." }
                            " Your party is already here."
                        } @else {
                            strong { "No direct route." }
                            " Travel is only available to settlements connected to the current location."
                        }
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

pub(crate) fn travel_planner_bar(
    selected: Option<&TravelDestination>,
    camp_fatigue_percent: u8,
) -> Markup {
    let selected_name = selected
        .map(|destination| destination.name.as_str())
        .unwrap_or("");
    let selected_description = selected
        .and_then(quest_destination_tooltip)
        .unwrap_or_default();
    let selected_minutes = selected.map_or(0, |destination| destination.journey_minutes);
    let selected_camp_stops = selected.map_or_else(String::new, |destination| {
        format_camp_stops(&destination.camp_stop_minutes)
    });
    let selected_camp_forecasts = selected.map_or_else(String::new, format_camp_forecasts);
    let provision_forecast =
        selected.and_then(|destination| destination.provision_forecast.as_ref());
    travel_planner_bar_for(
        selected_name,
        &selected_description,
        selected.is_some_and(|destination| {
            destination.quest_in_progress && destination.return_terrain_route.is_none()
        }),
        selected_minutes,
        &selected_camp_stops,
        &selected_camp_forecasts,
        camp_fatigue_percent,
        None,
        None,
        provision_forecast,
        selected
            .map(|destination| destination.departure_minute)
            .unwrap_or(0),
        selected
            .map(|destination| destination.itinerary_total_elapsed_minutes)
            .unwrap_or(selected_minutes),
        &selected.map_or_else(String::new, |destination| {
            format_itinerary_segments(&destination.itinerary_segments)
        }),
        &selected.map_or_else(String::new, |destination| format_terrain_spans(destination)),
    )
}

fn quest_destination_tooltip(destination: &TravelDestination) -> Option<String> {
    destination.quest_in_progress.then(|| {
        destination.summary.as_ref().map_or_else(
            || destination.description.clone(),
            |summary| format!("{}\n{summary}", destination.description),
        )
    })
}

pub(crate) fn travel_planner_bar_for(
    destination_name: &str,
    destination_description: &str,
    selected_round_trip: bool,
    journey_minutes: u64,
    camp_stop_minutes: &str,
    camp_forecasts: &str,
    camp_fatigue_percent: u8,
    journey: Option<&PartyJourney>,
    journey_route: Option<&PartyJourneyRoute>,
    provision_forecast: Option<&TravelProvisionForecast>,
    preview_departure_minute: u64,
    preview_elapsed_minutes: u64,
    preview_segments: &str,
    terrain_spans: &str,
) -> Markup {
    let journey_origin_name = journey.map_or("", |item| item.origin_name.as_str());
    let journey_destination_name = journey.map_or("", |item| item.destination_name.as_str());
    let journey_turnaround_minutes = journey
        .filter(|item| item.destination_kind == "quest")
        .map_or(0, |item| item.total_minutes);
    let journey_total_minutes = journey.map_or(0, |item| {
        if item.destination_kind == "quest" {
            item.total_minutes.saturating_add(
                journey_route
                    .and_then(|route| route.return_route.as_ref())
                    .map_or(item.total_minutes, |route| route.minutes),
            )
        } else {
            item.total_minutes
        }
    });
    let journey_completed_minutes = journey.map_or(0, |item| item.completed_minutes);
    let journey_camp_stops = journey.map_or_else(String::new, |item| {
        format_camp_stops(&item.camp_stop_minutes)
    });
    let journey_forecast_stops = journey.map_or_else(String::new, |item| {
        let mut stops = item.forecast_camp_stop_minutes.clone();
        if item.destination_kind == "quest" {
            stops.extend(
                item.camp_stop_minutes
                    .iter()
                    .chain(item.forecast_camp_stop_minutes.iter())
                    .rev()
                    .map(|minute| journey_total_minutes.saturating_sub(*minute)),
            );
        }
        format_camp_stops(&stops)
    });
    html! {
        section class="travel-planner" data-travel-planner
            data-camp-fatigue-percent=(camp_fatigue_percent)
            data-selected-name=(destination_name)
            data-selected-description=(destination_description)
            data-selected-round-trip=(selected_round_trip)
            data-selected-minutes=(journey_minutes)
            data-selected-camp-stops=(camp_stop_minutes)
            data-selected-camp-forecasts=(camp_forecasts)
            data-journey-origin-name=(journey_origin_name)
            data-journey-destination-name=(journey_destination_name)
            data-journey-total-minutes=(journey_total_minutes)
            data-journey-turnaround-minutes=(journey_turnaround_minutes)
            data-journey-completed-minutes=(journey_completed_minutes)
            data-departure-minute=(journey.map_or(preview_departure_minute, |item| item.departure_minute))
            data-total-elapsed-minutes=(journey.map_or(preview_elapsed_minutes, |item| item.total_elapsed_minutes))
            data-completed-elapsed-minutes=(journey.map_or(0, |item| item.completed_elapsed_minutes))
            data-itinerary-segments=(preview_segments)
            data-terrain-spans=(terrain_spans)
            data-journey-camp-stops=(journey_camp_stops)
            data-journey-forecast-stops=(journey_forecast_stops)
            data-provision-planning-minutes=[provision_forecast.map(|row| row.planning_minutes)]
            data-provision-living-members=[provision_forecast.map(|row| row.living_members)]
            data-provision-food-days=[provision_forecast.map(|row| row.food_days)]
            data-provision-water-days=[provision_forecast.map(|row| row.water_days)]
            data-provision-ordinary-water-days=[provision_forecast.map(|row| row.ordinary_water_days)]
            data-provision-emergency-alcohol-days=[provision_forecast.map(|row| row.emergency_alcohol_days)]
            data-provision-food-reserve=[provision_forecast.map(|row| row.food_reserve_kcal)]
            data-provision-water-reserve=[provision_forecast.map(|row| row.water_reserve_ml)]
            data-provision-rations=[provision_forecast.map(|row| row.ration_count)]
            data-provision-waterskins=[provision_forecast.map(|row| row.waterskin_count)]
            data-provision-ration-kcal=[provision_forecast.map(|row| row.ration_kcal)]
            data-provision-waterskin-ml=[provision_forecast.map(|row| row.waterskin_capacity_ml)]
            aria-live="polite" hidden {
            div class="travel-track" {
                div class="travel-planner-route" data-travel-planner-route {}
                div class="travel-resource-meters" data-travel-resource-meters {
                    div class="travel-resource-row food" aria-label="Food provisions" {
                        span class="travel-resource-icon" { (game_icon("Food", "meal")) }
                        svg class="travel-resource-track" viewBox="0 0 32 100" preserveAspectRatio="none" aria-hidden="true" {
                            path class="travel-resource-path base" d="M 16 0 V 100" pathLength="100" {}
                            path class="travel-resource-path target" data-resource-target pathLength="100" {}
                            path class="travel-resource-path actual" data-resource-fill pathLength="100" {}
                        }
                        span class="sr-only" data-surplus-summary="food" {}
                    }
                    div class="travel-resource-row water" aria-label="Water provisions" {
                        span class="travel-resource-icon" { (game_icon("Water", "water-drop")) }
                        svg class="travel-resource-track" viewBox="0 0 32 100" preserveAspectRatio="none" aria-hidden="true" {
                            path class="travel-resource-path base" d="M 16 0 V 100" pathLength="100" {}
                            path class="travel-resource-path target" data-resource-target pathLength="100" {}
                            path class="travel-resource-path actual" data-resource-fill pathLength="100" {}
                        }
                        span class="sr-only" data-surplus-summary="water" {}
                    }
                    div class="travel-resource-row fatigue" aria-label="Party fatigue" {
                        span class="travel-resource-icon" { (game_icon("Fatigue", "heart-minus")) }
                        div class="travel-fatigue-track" data-fatigue-track {}
                        span class="sr-only" data-fatigue-summary aria-live="polite" {}
                    }
                    div class="travel-resource-row terrain" aria-label="Terrain along route" {
                        span class="travel-resource-icon" { (game_icon("Terrain", "mountains")) }
                        div class="travel-terrain-track" data-terrain-track aria-describedby="terrain-course-description" {}
                        span class="sr-only" data-terrain-summary aria-live="polite" {}
                        ol id="terrain-course-description" class="sr-only" data-terrain-course-description {}
                    }
                    div class="travel-resource-row daylight" aria-label="Day and night" {
                        span class="travel-resource-icon" { (game_icon("Day and night", "sun")) }
                        div class="travel-daylight-track" data-daylight-track {}
                    }
                }
                svg class="travel-progress-track" viewBox="0 0 32 100" preserveAspectRatio="none" aria-hidden="true" {
                    path class="travel-progress-path" data-travel-progress pathLength="100" {}
                }
            }
        }
    }
}

fn format_camp_stops(stops: &[u64]) -> String {
    stops
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn format_terrain_spans(destination: &TravelDestination) -> String {
    destination
        .terrain_route
        .as_ref()
        .map_or_else(String::new, |route| {
            route
                .spans
                .iter()
                .map(|span| (span, 0_u64))
                .chain(
                    destination
                        .return_terrain_route
                        .iter()
                        .flat_map(|return_route| {
                            return_route.spans.iter().map(|span| (span, route.minutes))
                        }),
                )
                .filter_map(|(span, offset)| {
                    let kind = match span.surface {
                        adventuresim_terrain::Surface::Road => "road",
                        adventuresim_terrain::Surface::Open => "open",
                        adventuresim_terrain::Surface::SparseWoods => "sparse-woods",
                        adventuresim_terrain::Surface::DeepWoods => "deep-woods",
                        adventuresim_terrain::Surface::Wetland => "wetland",
                        adventuresim_terrain::Surface::Water => return None,
                    };
                    Some(format!(
                        "{kind},{},{},{},{},{},{},{}",
                        span.start_minute.saturating_add(offset),
                        span.duration_minutes,
                        span.check_millirank,
                        span.terrain.plains,
                        span.terrain.forest,
                        span.terrain.hills,
                        span.terrain.urban,
                    ))
                })
                .collect::<Vec<_>>()
                .join("|")
        })
}

fn format_camp_forecasts(destination: &TravelDestination) -> String {
    destination
        .camp_forecasts
        .iter()
        .map(|forecast| {
            format!(
                "{}:{}",
                forecast.fatigue_percent,
                format_camp_stops(&forecast.camp_stop_minutes)
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn format_itinerary_segments(segments: &[ItinerarySegment]) -> String {
    segments
        .iter()
        .map(|segment| {
            format!(
                "{},{},{},{},{},{:.4},{:.4},{:.4},{}",
                if matches!(segment.kind, ItinerarySegmentKind::Walking) {
                    "w"
                } else {
                    "c"
                },
                segment.elapsed_start,
                segment.elapsed_minutes,
                segment.movement_start,
                segment.movement_minutes,
                segment.average_fatigue_start,
                segment.average_fatigue_end,
                segment.maximum_fatigue_end,
                segment.required_rest_minutes,
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn format_persisted_itinerary(journey: &PartyJourney, itinerary: &PartyJourneyItinerary) -> String {
    let mut camps: Vec<_> = itinerary
        .actual_camp_intervals
        .iter()
        .cloned()
        .map(|camp| (camp, true, false))
        .chain(
            itinerary
                .forecast_camp_intervals
                .iter()
                .cloned()
                .map(|camp| (camp, false, true)),
        )
        .collect();
    camps.sort_by_key(|(camp, _, _)| camp.elapsed_start_minute);
    let mut merged: Vec<(crate::spacetimedb::JourneyCampInterval, bool, bool)> = Vec::new();
    for (camp, actual, forecast) in camps {
        if let Some((last, was_actual, was_forecast)) = merged.last_mut()
            && last.movement_minute == camp.movement_minute
            && camp.elapsed_start_minute
                <= last
                    .elapsed_start_minute
                    .saturating_add(last.elapsed_minutes)
        {
            let end = last
                .elapsed_start_minute
                .saturating_add(last.elapsed_minutes)
                .max(
                    camp.elapsed_start_minute
                        .saturating_add(camp.elapsed_minutes),
                );
            last.elapsed_minutes = end.saturating_sub(last.elapsed_start_minute);
            last.average_fatigue_end = camp.average_fatigue_end;
            last.maximum_fatigue_end = last.maximum_fatigue_end.max(camp.maximum_fatigue_end);
            *was_actual |= actual;
            *was_forecast |= forecast;
        } else {
            merged.push((camp, actual, forecast));
        }
    }
    let total_movement = if journey.destination_kind == "quest" {
        journey.total_minutes.saturating_mul(2)
    } else {
        journey.total_minutes
    };
    let mut output = Vec::new();
    let mut elapsed_cursor = 0;
    let mut movement_cursor = 0;
    let mut fatigue = merged
        .first()
        .map_or(0.0, |(camp, _, _)| camp.average_fatigue_start);
    for (camp, actual, forecast) in merged {
        if camp.elapsed_start_minute > elapsed_cursor {
            let movement = camp.movement_minute.saturating_sub(movement_cursor);
            output.push(format!(
                "w,{},{},{},{},{:.4},{:.4},{:.4},0",
                elapsed_cursor,
                camp.elapsed_start_minute - elapsed_cursor,
                movement_cursor,
                movement,
                fatigue,
                camp.average_fatigue_start,
                camp.average_fatigue_start
            ));
        }
        let kind = if actual && forecast {
            "m"
        } else if actual {
            "a"
        } else {
            "f"
        };
        output.push(format!(
            "{kind},{},{},{},0,{:.4},{:.4},{:.4},{}",
            camp.elapsed_start_minute,
            camp.elapsed_minutes,
            camp.movement_minute,
            camp.average_fatigue_start,
            camp.average_fatigue_end,
            camp.maximum_fatigue_end,
            camp.elapsed_minutes
        ));
        elapsed_cursor = camp
            .elapsed_start_minute
            .saturating_add(camp.elapsed_minutes);
        movement_cursor = camp.movement_minute;
        fatigue = camp.average_fatigue_end;
    }
    if elapsed_cursor < journey.total_elapsed_minutes {
        output.push(format!(
            "w,{},{},{},{},{:.4},{:.4},{:.4},0",
            elapsed_cursor,
            journey.total_elapsed_minutes - elapsed_cursor,
            movement_cursor,
            total_movement.saturating_sub(movement_cursor),
            fatigue,
            fatigue,
            fatigue
        ));
    }
    output.join("|")
}

fn format_legacy_persisted_itinerary(journey: &PartyJourney) -> String {
    let total_movement = if journey.destination_kind == "quest" {
        journey.total_minutes.saturating_mul(2)
    } else {
        journey.total_minutes
    };
    format!(
        "w,0,{},{},{},0.0000,0.0000,0.0000,0",
        journey.total_elapsed_minutes, 0, total_movement
    )
}

pub(crate) struct CampTravelDestination {
    pub id: String,
    pub name: String,
    pub journey_minutes: u64,
    pub current: bool,
}

fn camp_fire_is_lit(
    journey: Option<&PartyJourney>,
    itinerary: Option<&PartyJourneyItinerary>,
) -> bool {
    !matches!(
        (journey, itinerary),
        (Some(journey), Some(itinerary))
            if itinerary
                .actual_camp_intervals
                .last()
                .is_some_and(|interval| interval.movement_minute == journey.completed_minutes)
    )
}

/// The transient strategic location between planned travel legs.
pub fn camp_page(
    party: &Party,
    journey: Option<&PartyJourney>,
    itinerary: Option<&PartyJourneyItinerary>,
    terrain_route: Option<&PartyJourneyRoute>,
    destination_name: &str,
    active_character: Option<&Character>,
    party_members: &[Character],
    camp_destinations: &[CampTravelDestination],
    provision_forecast: Option<&TravelProvisionForecast>,
    default_rest_minutes: u64,
    soap_preview: SoapRestPreview,
    planned_wake_minute: u16,
    can_continue_travel: bool,
    encounter: Option<&StrategicEncounter>,
    logged_in_as: Option<&str>,
) -> Markup {
    let camp_fire_lit = camp_fire_is_lit(journey, itinerary);
    let content = html! {
        aside class="left-sidebar map-rest-sidebar" {
            div class="map-rest-sidebar-content" {
            (sidebar_section("Camp", html! {
                p { "The party has made camp between travel legs." }
                p class="text-muted small-copy" { "Destination: " (destination_name) }
                p class="text-muted small-copy" { (format_journey_time(party.camp_remaining_minutes)) " remaining" }
            }))
            @if !camp_destinations.is_empty() {
                (sidebar_section("Destinations", html! {
                    nav class="location-destination-list camp-destination-list" aria-label="Available camp destinations" {
                        @for destination in camp_destinations {
                            form action=(format!("/camp/destination/{}", destination.id)) method="post" {
                                button type="submit" class="list-item travel-destination-row camp-destination-row"
                                    disabled[destination.current] {
                                    strong { (&destination.name) }
                                    span class="text-muted small-copy" {
                                        @if destination.current { "Current" }
                                        @else { (format_journey_time(destination.journey_minutes)) }
                                    }
                                }
                            }
                        }
                    }
                }))
            }
            }
            @if encounter.is_none_or(|encounter| encounter.status != "awaiting_choice") {
                section class="rest-service-menu camp-rest-menu" aria-label="Camp rest" {
                    (party_rest_menu(
                        "/camp/rest",
                        "camp-rest",
                        "Rest at camp",
                        "Rest party",
                        default_rest_minutes,
                        Some(planned_wake_minute),
                        soap_preview,
                    ))
                }
            }
        }
        main class="center-content settlement-main settlement-overview" {
            (party_portrait_overlay(party_members, active_character, "/camp", None, false))
            (visual_stage("camp", "Camp", "A sheltered fire beside the party's onward route"))
            (settlement_chat_area("Camp", active_character))
        }
        aside class="right-sidebar camp-journey-sidebar" {
            @if let Some(encounter) = encounter.filter(|encounter| encounter.status == "awaiting_choice") {
                (strategic_encounter_panel(encounter))
            }
            div class="sidebar-section camp-journey-section" {
                h3 class="sidebar-header" { "Journey" }
                div class="travel-planner-vertical" {
                    (travel_planner_bar_for(destination_name, "", false, party.camp_remaining_minutes, "", "", party.camp_fatigue_percent, journey, terrain_route, provision_forecast, journey.map_or(0, |item| item.departure_minute), journey.map_or(party.camp_remaining_minutes, |item| item.total_elapsed_minutes), &match (journey, itinerary) { (Some(journey), Some(itinerary)) => format_persisted_itinerary(journey, itinerary), (Some(journey), None) => format_legacy_persisted_itinerary(journey), _ => String::new() }, &format_persisted_terrain_spans(terrain_route)))
                }
                form action="/camp/continue" method="post" {
                    button type="submit" class="btn btn-primary btn-small btn-block"
                        disabled[!can_continue_travel]
                        title=(if can_continue_travel { "Continue travel" } else { "Rest until the planned walking window begins" }) {
                        "Continue travel"
                    }
                }
                p class="travel-action-status" data-travel-action-status role="alert" hidden {}
            }
            (sidebar_section("Travel preferences", travel_preferences_form(party, "/camp/travel-configuration")))
        }
    };
    camp_location_layout_with_session(
        "Camp",
        "Camp",
        &party.id,
        camp_fire_lit,
        content,
        logged_in_as,
    )
}

fn strategic_encounter_panel(encounter: &StrategicEncounter) -> Markup {
    let threat = encounter.archetype.parse::<ThreatId>().ok();
    let threat_name = threat
        .map(|id| id.display_name(u32::from(encounter.enemy_count)))
        .unwrap_or_else(|| "Unknown threats".to_string());
    let awareness = match (encounter.party_aware, encounter.enemy_aware) {
        (true, false) => "Your party spotted them first",
        (false, true) => "The enemy surprised your party",
        (true, true) => "Both sides are aware",
        (false, false) => "Neither side is aware",
    };
    html! {
        section class="sidebar-section strategic-encounter" aria-label="Random encounter" {
            h3 class="sidebar-header" { "Encounter" }
            p class="encounter-summary" {
                strong { (encounter.enemy_count) " " (threat_name) }
                " on " (encounter.terrain.as_str())
            }
            @if let Some(threat) = threat {
                p class="text-muted small-copy" {
                    "Preparation: " (threat.profile().investigation.preparation_advice)
                }
            }
            p { (awareness) }
            p class="text-muted small-copy" { (encounter.selection_explanation.as_str()) }
            @if let Some(reason) = encounter.run_ineligibility.as_deref() {
                p class="encounter-warning" { "Cannot run: " (reason) }
            }
            @if !encounter.loss_preview.is_empty() {
                details class="encounter-surrender-preview" {
                    summary { "Exact surrender losses" }
                    ul {
                        @for loss in &encounter.loss_preview {
                            li {
                                (loss.quantity) " × " (loss.item_id.as_str())
                                " (" (loss.value_each) " value each, " (loss.owner_kind.as_str()) ")"
                            }
                        }
                    }
                }
            }
            div class="encounter-actions" {
                @for choice in &encounter.available_choices {
                    form action="/camp/encounter" method="post" {
                        input type="hidden" name="choice" value=(choice);
                        button type="submit" class="btn btn-primary btn-small btn-block" {
                            (match choice.as_str() {
                                "sneak" => "Sneak past",
                                "detour" => "Take a detour",
                                "attack" => "Attack",
                                "run" => "Run",
                                "surrender" => "Surrender",
                                _ => choice.as_str(),
                            })
                        }
                    }
                }
            }
        }
    }
}

fn format_persisted_terrain_spans(route: Option<&PartyJourneyRoute>) -> String {
    route.map_or_else(String::new, |route| {
        route
            .spans
            .iter()
            .map(|span| (span, 0_u64))
            .chain(route.return_route.iter().flat_map(|return_route| {
                return_route.spans.iter().map(|span| (span, route.minutes))
            }))
            .map(|(span, offset)| {
                let kind = match span.kind {
                    JourneyTerrainKind::Road => "road",
                    JourneyTerrainKind::Open => "open",
                    JourneyTerrainKind::SparseWoods => "sparse-woods",
                    JourneyTerrainKind::DeepWoods => "deep-woods",
                };
                format!(
                    "{kind},{},{},{},{},{},{},{}",
                    span.start_minute.saturating_add(offset),
                    span.duration_minutes,
                    span.check_millirank,
                    span.terrain.plains,
                    span.terrain.forest,
                    span.terrain.hills,
                    span.terrain.urban,
                )
            })
            .collect::<Vec<_>>()
            .join("|")
    })
}

pub(crate) fn party_rest_menu(
    action: &str,
    id_prefix: &str,
    heading: &str,
    submit_label: &str,
    default_minutes: u64,
    scheduled_wake_minute: Option<u16>,
    soap_preview: SoapRestPreview,
) -> Markup {
    html! {
        div class="rest-service-heading" { strong { (heading) } }
        form action=(action) method="post" {
            (wake_time_rest_duration_control(
                id_prefix,
                default_minutes.max(1),
                "hours",
                1,
                Some(default_minutes.max(1)),
                scheduled_wake_minute,
            ))
            button type="submit" class="btn btn-primary btn-small btn-block" data-rest-submit {
                (submit_label)
            }
        }
        (soap_wash_preview(soap_preview))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SoapRestPreview {
    pub total_units: u32,
    pub personal_units: u32,
    pub shared_units: u32,
    pub available_units: u32,
    pub alcohol_available: bool,
    pub alcohol_will_be_consumed: bool,
}

fn soap_wash_preview(preview: SoapRestPreview) -> Markup {
    let soap_tooltip = if preview.total_units > 0 {
        let source = if preview.personal_units > 0 && preview.shared_units > 0 {
            format!(
                " ({} personal, {} shared)",
                preview.personal_units, preview.shared_units
            )
        } else if preview.shared_units > 0 {
            " (shared)".to_string()
        } else {
            " (personal)".to_string()
        };
        format!(
            "Washing before rest will use {} soft soap{}. Soap is also a surgical supply.",
            preview.total_units, source
        )
    } else if preview.available_units > 0 {
        "Soft soap is available, but none is needed for washing before this rest. Soap is also a surgical supply."
            .to_string()
    } else {
        "No soft soap is available for washing before rest. Soap is also a surgical supply."
            .to_string()
    };
    let alcohol_tooltip = if preview.alcohol_will_be_consumed {
        "Alcohol is available and will be consumed automatically during nightly rest."
    } else if preview.alcohol_available {
        "Alcohol is available, but no eligible character will drink it. Temperate characters do not drink."
    } else {
        "No alcohol is available for automatic consumption during nightly rest."
    };
    html! {
        div class="rest-consumable-indicators" aria-label="Automatic rest supplies" {
            span class=(if preview.available_units > 0 { "rest-consumable-indicator available" } else { "rest-consumable-indicator unavailable" })
                role="img" tabindex="0" aria-label="Soap" title=(soap_tooltip) {
                (decorative_game_icon("water-drop"))
            }
            span class=(if preview.alcohol_will_be_consumed { "rest-consumable-indicator available" } else { "rest-consumable-indicator unavailable" })
                role="img" tabindex="0" aria-label="Alcohol" title=(alcohol_tooltip) {
                (decorative_game_icon("beer-stein"))
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

/// Market interface shown while settlement stock is unavailable.
pub fn merchants_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    logged_in_as: Option<&str>,
) -> Markup {
    service_page(
        settlement,
        "merchants",
        "Market Square",
        "Market Steward",
        "The market steward has no listed stock at present.",
        active_character,
        inventory,
        &[],
        party_members,
        logged_in_as,
        None,
        None,
        SoapRestPreview::default(),
    )
}

/// Church interface.
pub fn religion_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
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
    selected_encumbrance: EncumbranceSummary,
    active_encumbrance: EncumbranceSummary,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (party_trade_inventory_rail(selected, selected_inventory, items, active_character.id, "right", selected_equip, active_targets, selected_encumbrance))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(selected.id), false))
            (visual_stage("character", &selected.name, "Party member and trading companion"))
            (player_chat_area(selected, active_character))
            form id="party-offer" class="party-offer" action=(format!("{}/party/{}/inventory/offer", location.base_path(), selected.id)) method="post" hidden
                role="dialog" aria-modal="true" aria-label="Confirm party item offer" tabindex="-1" {
                span class="party-offer-summary" { "Review and send the staged item offer." }
                button type="button" class="party-offer-cancel" data-cancel-trade="party" { "Cancel" }
                button type="submit" disabled { "Offer" }
            }
        }
        aside class="right-sidebar" {
            (party_trade_inventory_rail(active_character, active_inventory, items, selected.id, "left", active_equip, selected_targets, active_encumbrance))
        }
    };
    location.render_layout("Party", content, Some(&active_character.name))
}

/// The active character's inventory with a staged discard list.
pub fn party_discard_page(
    location: &LocationView,
    active_character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    equip: Option<&CharacterEquip>,
    encumbrance: EncumbranceSummary,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Discard", html! {
                p class="text-muted small-copy" data-discard-empty { "Stage carried items here before discarding them." }
                div data-discard-table hidden {
                    (trade_inventory_table("discard-left", InventoryColumnSet::All, true, false, false, html! {}))
                }
            }))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(active_character.id), false))
            (visual_stage("character", &active_character.name, "Your carried equipment and supplies"))
            (settlement_chat_area(&active_character.name, Some(active_character)))
            form id="inventory-discard" class="party-offer"
                action=(format!("{}/party/{}/inventory/discard", location.base_path(), active_character.id))
                method="post" hidden role="dialog" aria-modal="true" aria-label="Confirm discarded items" tabindex="-1" {
                span class="party-offer-summary" data-discard-confirmation { "Discard the staged items?" }
                button type="button" class="party-offer-cancel" data-cancel-trade="discard" { "Cancel" }
                button type="submit" disabled { "Discard" }
            }
        }
        aside class="right-sidebar" {
            (discard_inventory_rail(active_character, inventory, items, equip, encumbrance))
        }
    };
    location.render_layout("Inventory", content, Some(&active_character.name))
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
    morale_sources: &[crate::spacetimedb::CharacterMoraleSource],
    religion_id: Option<&str>,
    prayer_religion_check: f32,
    schedule: Option<&CharacterTrainingSchedule>,
    combat_profile: CombatTrainingProfile,
    activity_preview: ActivityPreviewRates,
    religious_demand: Option<&crate::spacetimedb::ReligiousDemand>,
    notoriety: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    medical: &MedicalPresentation,
    can_examine: bool,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    filth: &[crate::spacetimedb::CharacterFilth],
    cooking: bool,
    inventory: &[InventoryItem],
    food_lots: &[FoodLot],
    item_definitions: &[ItemDefinition],
    character_action_dialog: Option<Markup>,
    surgery_open: Option<&str>,
    social_open: bool,
) -> Markup {
    let cooking_href = location.preserve_building(format!(
        "{}/party/{}?cook=true",
        location.base_path(),
        active_character.id
    ));
    let examination_action = location.preserve_building(format!(
        "{}/party/{}/examine",
        location.base_path(),
        active_character.id
    ));
    let cooking_open = cooking && medical.examination_id.is_none();
    let surgery_path_template = location.preserve_building(format!(
        "{}/party/{}/surgery/__limb__",
        location.base_path(),
        active_character.id
    ));
    let content = html! {
        aside class="left-sidebar" {
            (party_attributes_rail("Your attributes", attributes, limbs, medical, Some((&surgery_path_template, surgery_open)), injuries, projectiles))
            (strategic_condition_rail(condition, morale_sources, filth, &location.preserve_building(format!("{}/party/{}/social", location.base_path(), active_character.id)), social_open))
            (medical_rail(medical, &location.base_path(), active_character.id, active_character.id, true))
            @if let Some(demand) = religious_demand {
                (religious_demand_rail(demand, &location.base_path(), active_character.id))
            }
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(
                party_members,
                Some(active_character),
                &location.base_path(),
                Some(active_character.id),
                can_examine,
            ))
            (visual_stage("character", &active_character.name, "Your identity, condition, and capabilities"))
            (settlement_chat_area(&active_character.name, Some(active_character)))
            (medical_examination_popup(medical, location, active_character.id, limbs, injuries, projectiles))
        }
        aside class="right-sidebar" {
            (character_summary_rail(capability))
            (character_bio_rail(active_character, religion_id, notoriety, personality, true, &location.base_path()))
            @let schedule_action = format!("{}/party/{}/schedule", location.base_path(), active_character.id);
            (party_skills_rail(
                "Your skills", skills, limbs, schedule, Some(&schedule_action),
                Some(activity_preview), religion_id.is_some(), prayer_religion_check,
                religion_id.or(location.religion_id.as_deref()),
                combat_profile,
                CharacterSkillActions {
                    cooking_href: Some(&cooking_href),
                    cooking_open,
                    examination_action: can_examine.then_some(examination_action.as_str()),
                    examination_open: medical.examination_id.is_some(),
                },
            ))
        }
        @if cooking_open {
            (cooking_activity_dialog(location, active_character, inventory, food_lots, item_definitions))
        } @else if medical.examination_id.is_none() {
            @if let Some(dialog) = character_action_dialog { (dialog) }
        }
    };
    location.render_layout("Party", content, Some(&active_character.name))
}

fn cooking_activity_dialog(
    location: &LocationView,
    active_character: &Character,
    inventory: &[InventoryItem],
    food_lots: &[FoodLot],
    item_definitions: &[ItemDefinition],
) -> Markup {
    let close_href = location.preserve_building(format!(
        "{}/party/{}",
        location.base_path(),
        active_character.id
    ));
    let cook_action = location.preserve_building(format!(
        "{}/party/{}/cook",
        location.base_path(),
        active_character.id
    ));
    let owns = |item_id: &str| {
        inventory
            .iter()
            .any(|row| row.item_id == item_id && row.qty > 0)
    };
    let pan = owns("cooking_pan");
    let pot = owns("cooking_pot");
    let oven = owns("portable_oven");
    let ingredients = inventory
        .iter()
        .filter(|item| {
            food_lots
                .iter()
                .any(|lot| lot.inventory_item_id == Some(item.id))
        })
        .collect::<Vec<_>>();
    html! {
        div class="character-action-overlay" data-character-action-dialog data-initial-focus="[data-cooking-method]:checked" {
            a class="character-action-backdrop" href=(&close_href) aria-label="Close cooking dialog" {}
            section class="character-action-dialog cooking-dialog" role="dialog" aria-modal="true" aria-labelledby="cooking-dialog-title" tabindex="-1" {
            header class="character-action-dialog-header" {
                h2 id="cooking-dialog-title" { "Cooking" }
                a class="character-action-dialog-close" href=(&close_href) aria-label="Close cooking dialog" { "×" }
            }
            div class="cooking-activity" data-cooking-activity {
            aside class="cooking-pot" aria-label="Cooking pot" {
                (sidebar_section("Pot", html! {
                    p class="text-muted small-copy cooking-pot-empty" data-cooking-pot-empty {
                        "Transfer ingredients here to prepare a meal."
                    }
                    (trade_inventory_table("cooking-pot-left", InventoryColumnSet::Basic, true, false, false, html! {}))
                }))
            }
            main class="cooking-stage" {
                section class="cooking-workspace" aria-label="Cooking workspace" {
                    div class="cooking-method-list" aria-label="Cooking instrument" {
                        (cooking_method("pan-fry", "Pan-fry", "meal", pan, "A pan is required", false))
                        (cooking_method("stew", "Stew", "water-bottle", pot, "A pot and water are required", false))
                        (cooking_method("roast", "Roast / skewer", "campfire", true, "", true))
                        (cooking_method("bake", "Bake", "bread", oven, "A portable oven is required", false))
                    }
                    img class="cooking-stage-placeholder" src="/static/icons/game/campfire.svg"
                        alt="Placeholder for the cooking vessel and fire";
                    p class="text-muted small-copy" { "Cooking scene placeholder" }
                }
                form id="cooking-submit-form" class="cooking-submit-form" method="post"
                    action=(&cook_action) {
                    input type="hidden" name="inventory_item_ids" value="" data-cooking-ids;
                    input type="hidden" name="quantities" value="" data-cooking-quantities;
                    div class="party-offer cooking-actions" {
                        a class="party-offer-cancel" href=(&close_href) { "Cancel" }
                        button type="submit" disabled title="Select at least one ingredient" data-cook-submit { "Cook" }
                    }
                }
            }
            aside class="cooking-ingredients" aria-label="Ingredient inventory" {
                @let title = format!("{}'s inventory", active_character.name);
                (sidebar_section(&title, html! {
                    @if ingredients.is_empty() {
                        (empty_state("No food carried.", None, None))
                    } @else {
                        (trade_inventory_table("cooking-inventory-right", InventoryColumnSet::Basic, true, false, false, html! {
                            @for item in ingredients {
                                @let definition = item_definitions.iter().find(|definition| definition.id == item.item_id);
                                @let food_lot = food_lots.iter().find(|lot| lot.inventory_item_id == Some(item.id));
                                @let display_name = food_lot.map_or_else(|| item_display_name(&item.item_id), |lot| lot.display_name.clone());
                                @let unit_mass = food_lot.map_or_else(|| definition.map_or(0.0, |definition| definition.weight), |lot| lot.mass_kg / item.qty.max(1) as f32);
                                @let value = food_lot.map_or_else(|| item_value(definition), |lot| weight_display(lot.total_value));
                                tr class="trade-inventory-row trade-row-player" data-cooking-source=(item.id) data-item-key=(&item.item_id) {
                                    td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                    td class="inventory-item-name" {
                                        (item_name_with_display(&item.item_id, &display_name, definition))
                                        span class="inventory-row-actions" {
                                            @if food_lot.is_some() {
                                                @let safety = adventuresim_core::food::definition(&item.item_id).map_or(5, |food| food.cooking_minutes);
                                                button type="button" class="trade-transfer trade-transfer-left"
                                                    data-cooking-stage=(item.id) data-cooking-name=(&display_name)
                                                    data-count=(item.qty) data-mass=(format!("{unit_mass:.4}")) data-safety=(safety)
                                                    data-dynamic-transfer data-default-transfer-mode="one" data-transfer-mode="one"
                                                    data-label-one=(format!("Add one {display_name} to the pot"))
                                                    data-label-target=(format!("Add {display_name} to the pot"))
                                                    data-label-all=(format!("Add all {display_name} to the pot"))
                                                    aria-label=(format!("Add one {display_name} to the pot"))
                                                    title=(format!("Add one {display_name} to the pot")) { (transfer_glyph(1)) }
                                            } @else {
                                                (disabled_transfer_button("left", "Only food ingredients can be added to the pot"))
                                            }
                                        }
                                    }
                                    td class="inventory-count" { (item.qty) }
                                    td class="inventory-weight" { (weight_display(unit_mass)) }
                                    td class="inventory-gold" { (value) }
                                }
                            }
                        }))
                    }
                }))
            }
            }
            }
        }
    }
}

fn cooking_method(
    value: &str,
    label: &str,
    icon: &str,
    available: bool,
    reason: &str,
    selected: bool,
) -> Markup {
    html! {
        label class=(if available { "cooking-method" } else { "cooking-method disabled" })
            title=(if available { label } else { reason }) {
            input type="radio" name="method" value=(value) form="cooking-submit-form"
                checked[selected] disabled[!available]
                data-cooking-method data-unavailable-reason=[(!available).then_some(reason)];
            span class="cooking-method-icon"
                style=(format!("--cooking-method-icon: url('/static/icons/game/{icon}.svg')"))
                aria-hidden="true" {}
            span class="sr-only" { (label) }
            @if !available { span class="sr-only" { (reason) } }
        }
    }
}

fn filth_status_bar(deposits: &[crate::spacetimedb::CharacterFilth]) -> Markup {
    use crate::spacetimedb::{FilthOrigin, FilthSubstance};
    let dirt: u16 = deposits
        .iter()
        .filter(|d| d.substance == FilthSubstance::Dirt)
        .map(|d| d.amount)
        .fold(0, u16::saturating_add);
    let blood: u16 = deposits
        .iter()
        .filter(|d| d.substance == FilthSubstance::Blood)
        .map(|d| d.amount)
        .fold(0, u16::saturating_add);
    let total = dirt
        .saturating_add(blood)
        .min(adventuresim_core::filth::MAX_FILTH);
    let dirt_width = f32::from(dirt.min(total));
    let blood_width = f32::from(blood.min(total.saturating_sub(dirt.min(total))));
    let (own_blood, foreign_blood, unknown_blood) = deposits
        .iter()
        .filter(|d| d.substance == FilthSubstance::Blood)
        .fold((0_u16, 0_u16, 0_u16), |mut amounts, deposit| {
            match deposit.origin {
                FilthOrigin::Own => amounts.0 = amounts.0.saturating_add(deposit.amount),
                FilthOrigin::Foreign => amounts.1 = amounts.1.saturating_add(deposit.amount),
                FilthOrigin::Unknown => amounts.2 = amounts.2.saturating_add(deposit.amount),
            }
            amounts
        });
    let summary = format!(
        "Current: {total}/100 — {dirt} dirt, {blood} blood ({own_blood} own, {foreign_blood} foreign, {unknown_blood} unknown)."
    );
    let details = format!(
        "Filth accumulates from travel, combat, and medical treatment. Dirt and blood fill this bar. Foreign blood can transmit bloodborne disease, with greater risk through open cuts and lesser risk through bandaged cuts. Soap is used automatically before rest to wash filth away.\n\n{summary}"
    );
    html! {
        div class="filth-status" tabindex="0" role="meter" aria-valuemin="0" aria-valuemax="100"
            aria-valuenow=(total) aria-label=(format!("Filth {total} out of 100"))
            data-strategic-tooltip=(&details) {
            strong class="metric-label filth-status-label" { "Filth" }
            span class="filth-track" aria-hidden="true" {
                @if dirt > 0 {
                    span class="filth-segment filth-dirt" style=(format!("width:{dirt_width}%"))
                        data-strategic-tooltip=(format!("Dirt\n{dirt}")) {}
                }
                @if blood > 0 {
                    span class="filth-segment filth-blood" style=(format!("width:{blood_width}%"))
                        data-strategic-tooltip=(format!("Blood\n{blood}")) {}
                }
            }
        }
    }
}

fn religious_demand_rail(
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
    combat_profile: CombatTrainingProfile,
    condition: Option<&CharacterStrategicCondition>,
    morale_sources: &[crate::spacetimedb::CharacterMoraleSource],
    religion_id: Option<&str>,
    active_party: Option<&Party>,
    selected_party: Option<&Party>,
    notoriety: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    medical: &MedicalPresentation,
    can_examine: bool,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    filth: &[crate::spacetimedb::CharacterFilth],
    character_action_dialog: Option<Markup>,
    surgery_open: Option<&str>,
    social_open: bool,
) -> Markup {
    let selected_attributes_title = format!("{}'s attributes", selected.name);
    let selected_skills_title = format!("{}'s skills", selected.name);
    let examination_action = location.preserve_building(format!(
        "{}/party/{}/examine",
        location.base_path(),
        selected.id
    ));
    let surgery_path_template = location.preserve_building(format!(
        "{}/party/{}/surgery/__limb__",
        location.base_path(),
        selected.id
    ));
    let content = html! {
        aside class="left-sidebar" {
            (party_attributes_rail(&selected_attributes_title, selected_attributes, selected_limbs, medical, Some((&surgery_path_template, surgery_open)), injuries, projectiles))
            (strategic_condition_rail(condition, morale_sources, filth, &location.preserve_building(format!("{}/party/{}/social", location.base_path(), selected.id)), social_open))
            (medical_rail(medical, &location.base_path(), active_character.id, selected.id, true))
        }
        @if medical.examination_id.is_none() {
            @if let Some(dialog) = character_action_dialog { (dialog) }
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(
                party_members,
                Some(active_character),
                &location.base_path(),
                Some(selected.id),
                can_examine,
            ))
            (visual_stage("character", &selected.name, "Party member identity and capabilities"))
            (player_chat_area(selected, active_character))
            (medical_examination_popup(medical, location, selected.id, selected_limbs, injuries, projectiles))
        }
        aside class="right-sidebar" {
            (character_summary_rail(capability))
            (character_bio_rail(
                selected,
                religion_id,
                notoriety,
                personality,
                selected.id == active_character.id,
                &location.base_path(),
            ))
            (party_skills_rail(
                &selected_skills_title, selected_skills, selected_limbs, None, None, None,
                religion_id.is_some(), 0.0, religion_id.or(location.religion_id.as_deref()),
                combat_profile,
                CharacterSkillActions {
                    examination_action: can_examine.then_some(examination_action.as_str()),
                    examination_open: medical.examination_id.is_some(),
                    ..Default::default()
                },
            ))
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
    location.render_layout("Party stats", content, Some(&active_character.name))
}

#[derive(Debug, Clone, Default)]
pub struct SocialPresentation {
    pub affinity: f32,
    pub familiarity_hours: f32,
    pub religion_id: Option<String>,
    pub virtue: f32,
    pub beliefs: Vec<crate::spacetimedb::SocialBelief>,
    pub shared_concerns: Vec<adventuresim_core::social::SocialTopic>,
    pub unavailable: bool,
}

fn social_actions(
    is_self: bool,
    topic: adventuresim_core::social::SocialTopic,
) -> Vec<(
    &'static str,
    adventuresim_core::social::SocialActionKind,
    &'static str,
)> {
    use adventuresim_core::social::SocialActionKind::*;
    if is_self {
        return vec![("inner-self", Reflect, "reflect")];
    }
    [
        ("awareness", Listen, "listen"),
        ("awareness", Commiserate, "commiserate"),
        ("juggler", LightenMood, "humor"),
        ("crown", Rally, "command"),
        ("conversation", Reframe, "deception"),
        ("rose", Flirt, "seduction"),
    ]
    .into_iter()
    .filter(|(_, action, _)| action.available_for(topic))
    .collect()
}

fn perceived_trait(axis: &str, value: i8) -> (&'static str, &'static str) {
    match (axis, value.signum()) {
        ("drive", 1) => ("Drive", "Ambitious"),
        ("drive", -1) => ("Drive", "Content"),
        ("self_regard", 1) => ("Self-regard", "Proud"),
        ("self_regard", -1) => ("Self-regard", "Humble"),
        ("conviction", 1) => ("Conviction", "Zealous"),
        ("conviction", -1) => ("Conviction", "Irreverent"),
        ("hygiene", 1) => ("Hygiene", "Cleanly"),
        ("hygiene", -1) => ("Hygiene", "Slovenly"),
        ("drive", _) => ("Drive", "Neutral"),
        ("self_regard", _) => ("Self-regard", "Neutral"),
        ("conviction", _) => ("Conviction", "Neutral"),
        ("hygiene", _) => ("Hygiene", "Neutral"),
        _ => ("Personality", "Uncertain"),
    }
}

fn familiarity_label(hours: f32) -> String {
    if hours.is_finite() && hours > 0.0 && hours < 1.0 {
        "<1 hours".into()
    } else {
        format!("{:.0} hours", hours.max(0.0))
    }
}

fn belief_style(confidence: f32) -> String {
    format!(
        "--belief-confidence:{:.0}%",
        confidence.clamp(0.0, 1.0) * 100.0
    )
}

fn personality_reaction_hint(axis: &str, value: i8) -> &'static str {
    match (axis, value.signum()) {
        ("drive", 1) => {
            "Likely reaction: Rallying can motivate them after defeat; pity or flippancy may offend."
        }
        ("drive", -1) => {
            "Likely reaction: Listening and commiseration are safer than pressuring them to prove themselves."
        }
        ("self_regard", 1) => {
            "Likely reaction: Injury is touchy; admiration may land better than pity or minimizing the wound."
        }
        ("self_regard", -1) => {
            "Likely reaction: Plain sympathy is safer; conspicuous flattery may feel insincere."
        }
        ("conviction", 1) => {
            "Likely reaction: Treat moral concerns seriously; jokes and false reassurance are especially risky."
        }
        ("conviction", -1) => {
            "Likely reaction: Gentle reframing may work better than appeals to duty or conviction."
        }
        ("hygiene", 1) => {
            "Likely reaction: Filth is genuinely upsetting; acknowledge it rather than dismissing the concern."
        }
        ("hygiene", -1) => {
            "Likely reaction: They may not share strong concern about grime, so forceful reassurance can seem strange."
        }
        _ => "Likely reaction: Their response to riskier social actions remains uncertain.",
    }
}

fn belief_tooltip(belief: &crate::spacetimedb::SocialBelief) -> String {
    format!(
        "Confidence: {:.0}%\n{}",
        belief.confidence.clamp(0.0, 1.0) * 100.0,
        personality_reaction_hint(&belief.axis, belief.perceived_value)
    )
}

/// Dedicated social view. It intentionally receives observer-specific beliefs
/// rather than authoritative personality.
pub fn party_social_dialog(
    location: &LocationView,
    selected: &Character,
    active_character: &Character,
    morale_sources: &[crate::spacetimedb::CharacterMoraleSource],
    social: &SocialPresentation,
) -> Markup {
    let social_href = location.preserve_building(format!(
        "{}/party/{}/social",
        location.base_path(),
        selected.id
    ));
    let affinity_label = match social.affinity {
        value if value >= 50.0 => "Devoted",
        value if value >= 15.0 => "Warm",
        value if value <= -50.0 => "Hostile",
        value if value <= -15.0 => "Cold",
        _ => "Neutral",
    };
    let is_self = selected.id == active_character.id;
    let affinity_certainty = if social.familiarity_hours >= 48.0 {
        "fairly certain"
    } else if social.familiarity_hours >= 8.0 {
        "tentative"
    } else {
        "uncertain"
    };
    let close_href = location.preserve_building(if is_self {
        format!("{}/party/{}", location.base_path(), selected.id)
    } else {
        format!("{}/party/{}/stats", location.base_path(), selected.id)
    });
    html! {
        div class="character-action-overlay" data-character-action-dialog {
            a class="character-action-backdrop" href=(&close_href) aria-label="Close social dialog" {}
            section class="character-action-dialog social-dialog" role="dialog" aria-modal="true" aria-labelledby="social-dialog-title" tabindex="-1" {
                header class="character-action-dialog-header" {
                    h2 id="social-dialog-title" { "Social — " (selected.name) }
                    a class="character-action-dialog-close" href=(&close_href) aria-label="Close social dialog" { "×" }
                }
                div class="social-rail" data-social-panel data-target-id=(selected.id) {
            (sidebar_section("What you believe", html! {
                dl class="social-biography" {
                    div { dt { "Age" } dd { (selected.age_years) } }
                    div { dt { "Religion" } dd { (religion_name(social.religion_id.as_deref())) } }
                    div { dt { "Virtue" } dd { (format!("{:+.0}", social.virtue)) } }
                    @if !is_self {
                        div { dt { "Affinity toward you" } dd { (affinity_label) " (" (affinity_certainty) ")" } }
                        div { dt { "Familiarity" } dd { (familiarity_label(social.familiarity_hours)) } }
                    }
                }
                @if social.unavailable {
                    p class="social-unavailable" role="status" { "Your impressions are unavailable right now." }
                } @else if social.beliefs.is_empty() {
                    p class="text-muted small-copy" { "You have not formed a confident impression of their personality yet." }
                } @else {
                    ul class="perceived-traits" aria-label="Perceived personality traits" {
                        @for belief in &social.beliefs {
                            @let (_, value) = perceived_trait(&belief.axis, belief.perceived_value);
                            li class="perceived-trait" style=(belief_style(belief.confidence))
                                tabindex="0" data-strategic-tooltip=(belief_tooltip(belief)) {
                                (value)
                            }
                        }
                    }
                }
            }))
            (sidebar_section("Morale sources", html! {
                @if morale_sources.is_empty() { p class="text-muted" { "No current morale effects." } }
                div class="social-source-list" {
                    @for source in morale_sources {
                        @let topic = adventuresim_core::social::topic_for_source_kind(&source.kind);
                        article class=(if source.magnitude < 0.0 { "social-source social-source-negative" } else { "social-source social-source-positive" }) {
                            div class="social-source-context" {
                                div { strong { (&source.label) } span { (format!("{:+.1}", source.magnitude)) } }
                                @if let Some(axis) = topic.and_then(adventuresim_core::social::axis_for_topic) {
                                    @if let Some(belief) = social.beliefs.iter().find(|belief| belief.axis == axis.slug()) {
                                        @let (axis_name, value) = perceived_trait(&belief.axis, belief.perceived_value);
                                        p class="belief-copy" style=(belief_style(belief.confidence))
                                            tabindex="0" data-strategic-tooltip=(belief_tooltip(belief)) {
                                            "You think their " (axis_name) " is " (value) "."
                                        }
                                    } @else {
                                        p { "The relevant personality trait is uncertain." }
                                    }
                                } @else {
                                    p { "No specific personality trait is known to govern this concern." }
                                }
                            }
                            @if source.magnitude < 0.0 {
                                @if let Some(topic) = topic {
                                  div class="social-actions" aria-label=(format!("Actions for {}", source.label)) {
                                    @let shares_concern = social.shared_concerns.contains(&topic);
                                    @for (default_icon, action, value) in social_actions(is_self, topic) {
                                      @let action_shares_concern = action != adventuresim_core::social::SocialActionKind::Commiserate || shares_concern;
                                      @let icon = if action == adventuresim_core::social::SocialActionKind::Commiserate && !shares_concern { "conversation" } else { default_icon };
                                      @let description = action.description(topic, action_shares_concern);
                                    form method="post" action=(&social_href) {
                                        input type="hidden" name="source_id" value=(&source.id);
                                        button type="submit" name="action_kind" value=(value) class="social-action"
                                            aria-label=(description) title=(description) data-strategic-tooltip=(format!("{}\n{} · {} risk", description, action.skill_name(action_shares_concern), if action.risk() >= 0.6 { "high" } else if action.risk() >= 0.3 { "moderate" } else { "low" })) {
                                            (decorative_game_icon(icon))
                                        }
                                    }
                                    }
                                  }
                                }
                            }
                        }
                    }
                }
            }))
                }
            }
        }
    }
}

fn surgery_limb_name(limb: LimbRegion) -> &'static str {
    match limb {
        LimbRegion::LeftArm => "Left arm",
        LimbRegion::RightArm => "Right arm",
        LimbRegion::LeftLeg => "Left leg",
        LimbRegion::RightLeg => "Right leg",
        LimbRegion::Chest => "Chest",
        LimbRegion::Stomach => "Stomach",
        LimbRegion::Head => "Head",
    }
}

fn surgery_limb_slug(limb: LimbRegion) -> &'static str {
    match limb {
        LimbRegion::LeftArm => "left-arm",
        LimbRegion::RightArm => "right-arm",
        LimbRegion::LeftLeg => "left-leg",
        LimbRegion::RightLeg => "right-leg",
        LimbRegion::Chest => "chest",
        LimbRegion::Stomach => "stomach",
        LimbRegion::Head => "head",
    }
}

fn surgery_duration(procedure: &str, skill: f32, dc: f32) -> u64 {
    adventuresim_core::surgery::procedure_duration_minutes(procedure, skill, dc)
}

fn surgery_procedure_skill(procedure: &str, checks: [f32; 3], self_treatment: bool) -> f32 {
    adventuresim_core::surgery::procedure_skill(
        procedure,
        checks[0],
        checks[1],
        checks[2],
        self_treatment,
    )
}

#[derive(Clone, Copy)]
enum SurgeryItemRequirement {
    BandageConsumed,
    SurgeryKitReusable,
    SplintEquipped,
}

fn surgery_supply(label: &str, icon: &str, quantity: u32) -> Markup {
    let description = format!("{label}: {quantity} available");
    html! {
        div class="surgery-supply" data-strategic-tooltip=(&description)
            aria-label=(&description) tabindex="0" {
            (decorative_game_icon(icon))
            span class="surgery-item-overlay surgery-item-quantity" aria-hidden="true" { "x" (quantity) }
        }
    }
}

fn surgery_item_requirement(requirement: SurgeryItemRequirement) -> Markup {
    let (label, accessible_label, icon) = match requirement {
        SurgeryItemRequirement::BandageConsumed => {
            ("Expend one bandage", "Expend one bandage", "bandage-roll")
        }
        SurgeryItemRequirement::SurgeryKitReusable => (
            "Requires surgery kit",
            "Requires surgery kit; reusable and not consumed",
            "medical-pack",
        ),
        SurgeryItemRequirement::SplintEquipped => {
            ("Equips 1 splint", "Equips 1 splint", "arm-bandage")
        }
    };
    html! {
        span class="surgery-item-requirement" data-strategic-tooltip=(label)
            aria-label=(accessible_label) tabindex="0" {
            (decorative_game_icon(icon))
            @match requirement {
                SurgeryItemRequirement::BandageConsumed => {
                    span class="surgery-item-overlay surgery-item-quantity" aria-hidden="true" { "x1" }
                }
                SurgeryItemRequirement::SurgeryKitReusable => {}
                SurgeryItemRequirement::SplintEquipped => {
                    span class="surgery-item-overlay surgery-item-equipped" aria-hidden="true" {
                        (decorative_game_icon("check-mark"))
                    }
                }
            }
        }
    }
}

fn surgery_difficulty_meter(procedure_label: &str, dc: f32, effective_skill: f32) -> Markup {
    let difficulty = dc.max(0.0);
    let over_cap = difficulty > 5.0;
    let meter_label = format!("{procedure_label} procedure difficulty");
    let accessible_label = format!(
        "{procedure_label}: requires {difficulty:.1} procedure skill; current effective skill {:.1}",
        effective_skill.max(0.0)
    );
    html! {
        div class=(if over_cap { "surgery-difficulty surgery-difficulty-over-cap" } else { "surgery-difficulty" })
            title=[over_cap.then_some("Difficulty exceeds the normal procedure skill scale")] {
            (stat_icon(&meter_label, "skills", "surgeon", true))
            (skill_rank_bar(
                difficulty,
                effective_skill.min(difficulty),
                &meter_label,
                SkillRankBarOptions {
                    show_value: false,
                    extra_class: Some("surgery-difficulty-meter"),
                    aria_label: Some(&accessible_label),
                },
            ))
            @if over_cap {
                span class="surgery-difficulty-over-cap-marker" aria-hidden="true" { "+" }
            }
        }
    }
}

fn surgery_procedure_row(
    action: &str,
    label: &str,
    icon: &str,
    procedure: &str,
    item_requirements: &[SurgeryItemRequirement],
    duration: u64,
    dc: f32,
    effective_skill: f32,
    unavailable: Option<&str>,
    disabled: Option<&str>,
    projectile_id: Option<u64>,
    soap_available: bool,
    soap_applicable: bool,
    selected_alcohol: Option<&str>,
) -> Markup {
    let row_class = if unavailable.is_some() {
        "surgery-procedure surgery-procedure-unavailable"
    } else {
        "surgery-procedure"
    };
    let unavailable_label = unavailable.map(|reason| format!("{label}: {reason}"));
    html! {
        form method="post" action=(action) class=(row_class)
            data-strategic-tooltip=[unavailable] aria-label=[unavailable_label.as_deref()]
            tabindex=[unavailable.map(|_| "0")] {
            input type="hidden" name="procedure" value=(procedure);
            @if let Some(projectile_id) = projectile_id {
                input type="hidden" name="projectile_id" value=(projectile_id);
            }
            @if soap_applicable {
                label class="surgery-soap-option" title="Consumes one unit; lowers contamination risk independently of other supplies" {
                    input type="checkbox" name="use_soap" value="true" disabled[!soap_available];
                    " Use 1 soft soap"
                }
            }
            @if icon == "bullet-visual" {
                span class="procedure-projectile-visual projectile-ball" role="img" aria-label=(label) {}
            } @else {
                (game_icon(label, icon))
            }
            div class="surgery-procedure-copy" {
                strong { (label) }
            }
            dl class="surgery-procedure-facts" {
                div { dt { "Time" } dd { (duration) " min" } }
                div class="surgery-procedure-difficulty" {
                    dt class="sr-only" { "Difficulty" }
                    dd { (surgery_difficulty_meter(label, dc, effective_skill)) }
                }
            }
            @if !item_requirements.is_empty() {
                ul class="surgery-item-requirements" aria-label="Required items" {
                    @for requirement in item_requirements {
                        li { (surgery_item_requirement(*requirement)) }
                    }
                }
            }
            @if let Some(item_id) = selected_alcohol {
                div class="surgery-alcohol-consumption" aria-label=(format!("Consumes one {} for disinfection", item_display_name(item_id))) {
                    (game_icon(&format!("Consumes one {}", item_display_name(item_id)), "beer-stein"))
                    span { "Consumes 1 " (item_display_name(item_id)) }
                }
            }
            @if let Some(reason) = disabled {
                button type="submit" class="btn btn-block" disabled title=(reason) aria-label=(format!("{label}: {reason}")) { (label) }
            } @else {
                button type="submit" class="btn btn-primary" { (label) }
            }
        }
    }
}

/// Manual limb treatment is an SSR-open dialog over the ordinary character rails.
#[allow(clippy::too_many_arguments)]
pub fn surgery_dialog(
    location: &LocationView,
    active_character: &Character,
    patient: &Character,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    selected_limb: LimbRegion,
    bandages: u32,
    surgery_kits: u32,
    splints: u32,
    soaps: u32,
    alcohol_units: u32,
    selected_alcohol: Option<&str>,
    procedure_checks: [f32; 3],
) -> Markup {
    let action = location.preserve_building(format!(
        "{}/party/{}/surgery/{}/procedure",
        location.base_path(),
        patient.id,
        surgery_limb_slug(selected_limb)
    ));
    let selected = injuries.iter().find(|injury| injury.limb == selected_limb);
    let cut = selected.map_or(0.0, |injury| injury.cut_damage.max(0.0));
    let bruise = selected.map_or(0.0, |injury| injury.bruise_damage.max(0.0));
    let fracture = selected.map_or(0.0, |injury| injury.fracture_damage.max(0.0));
    let bandaged = selected.is_some_and(|injury| injury.bandaged);
    let stitched = selected.is_some_and(|injury| injury.stitched);
    let splinted = selected.is_some_and(|injury| injury.splint_inventory_item_id.is_some());
    let has_kit = surgery_kits > 0;
    let self_treatment = active_character.id == patient.id;
    let procedure_skill =
        |procedure| surgery_procedure_skill(procedure, procedure_checks, self_treatment);
    let anatomy_skill = procedure_skill("bandage");
    let extraction_skill = procedure_skill("extract");
    let stitching_skill = procedure_skill("stitch");
    let close_href = location.preserve_building(if self_treatment {
        format!("{}/party/{}", location.base_path(), patient.id)
    } else {
        format!("{}/party/{}/stats", location.base_path(), patient.id)
    });
    html! {
        div class="character-action-overlay" data-character-action-dialog {
            a class="character-action-backdrop" href=(&close_href) aria-label="Close surgery dialog" {}
            section class="character-action-dialog surgery-dialog" role="dialog" aria-modal="true" aria-labelledby="surgery-dialog-title" tabindex="-1" {
                header class="character-action-dialog-header" {
                    h2 id="surgery-dialog-title" { (patient.name) " — " (surgery_limb_name(selected_limb)) }
                    a class="character-action-dialog-close" href=(&close_href) aria-label="Close surgery dialog" { "×" }
                }
                div class="surgery-rail" {
                div class="surgery-supplies" aria-label="Surgery supplies" {
                    (surgery_supply("Bandages", "bandage-roll", bandages))
                    (surgery_supply("Surgery kits", "medical-pack", surgery_kits))
                    (surgery_supply("Splints", "arm-bandage", splints))
                    (surgery_supply("Soft soap", "water-drop", soaps))
                    (surgery_supply("Disinfecting alcohol", "beer-stein", alcohol_units))
                }
                div class="surgery-procedures" {
                    @for projectile in projectiles.iter().filter(|projectile| projectile.limb == selected_limb) {
                        @let requires_kit = adventuresim_core::surgery::extraction_requires_surgery_kit(projectile.extraction_dc);
                        (surgery_procedure_row(&action, match projectile.kind { ProjectileKind::Arrowhead => "Remove arrowhead", ProjectileKind::Ball => "Remove ball" }, match projectile.kind { ProjectileKind::Arrowhead => "plain-arrow", ProjectileKind::Ball => "bullet-visual" }, "extract", if requires_kit { &[SurgeryItemRequirement::SurgeryKitReusable] } else { &[] }, surgery_duration("extract", extraction_skill, projectile.extraction_dc), projectile.extraction_dc,
                            extraction_skill, None, if extraction_skill < projectile.extraction_dc { Some("Insufficient Anatomy + Knife skill") } else if requires_kit && !has_kit { Some("No surgery kit") } else { None }, Some(projectile.id), soaps > 0, true, selected_alcohol))
                    }
                    (surgery_procedure_row(&action, "Bandage", "bandage-roll", "bandage", &[SurgeryItemRequirement::BandageConsumed], surgery_duration("bandage", anatomy_skill, 0.0), 0.0,
                        anatomy_skill, if cut <= 0.0 { Some("No injury is present") } else { None }, if cut <= 0.0 { Some("No injury is present") } else if bandaged { Some("Already bandaged") } else if bandages == 0 { Some("No bandages") } else { None }, None, soaps > 0, true, selected_alcohol))
                    (surgery_procedure_row(&action, "Stitch", "scalpel", "stitch", &[SurgeryItemRequirement::SurgeryKitReusable], surgery_duration("stitch", stitching_skill, 2.0), 2.0,
                        stitching_skill, if cut <= 0.0 { Some("No injury is present") } else { None }, if cut <= 0.0 { Some("No injury is present") } else if stitched { Some("Already stitched") } else if stitching_skill < 2.0 { Some("Insufficient Anatomy + Tailoring skill") } else if !has_kit { Some("No surgery kit") } else { None }, None, soaps > 0, true, selected_alcohol))
                    @if splinted {
                        (surgery_procedure_row(&action, "Remove splint", "arm-bandage", "remove-splint", &[], surgery_duration("remove-splint", anatomy_skill, 0.0), 0.0, anatomy_skill, None, None, None, false, false, None))
                    } @else {
                        (surgery_procedure_row(&action, "Splint", "arm-bandage", "splint", &[SurgeryItemRequirement::SplintEquipped], surgery_duration("splint", anatomy_skill, 1.0), 1.0,
                            anatomy_skill, if fracture <= 0.0 { Some("No injury is present") } else { None }, if fracture <= 0.0 { Some("No injury is present") } else if anatomy_skill < 1.0 { Some("Insufficient Anatomy skill") } else if splints == 0 { Some("No splints") } else { None }, None, false, false, None))
                    }
                    @if cut <= 0.0 && bruise > 0.0 && fracture <= 0.0 {
                        p class="text-muted small-copy" { "Bruising must heal on its own." }
                    }
                }
                }
            }
        }
    }
}

fn service_page(
    settlement: &Settlement,
    service_id: &str,
    title: &str,
    npc_name: &str,
    service_summary: &str,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    rest_default_minutes: Option<u64>,
    rest_summary: Option<&RestSummary>,
    soap_preview: SoapRestPreview,
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
                    (rest_service_menu("Inn", &settlement.id, "inn", rest_default_minutes, rest_summary, soap_preview))
                }
            } @else if service_id == "religion" {
                div class="service-left-stack" {
                    div class="service-inventory-area" {
                        (sidebar_section("Church services", html! {
                            p title=[active_character.is_some().then_some("Speak with the priest to profess this faith. Renunciation is available from your biography. Shared conviction strengthens allied Command; conflicting conviction penalizes morale.")] {
                                "Faith: " strong { (religion_name(Some(&settlement.religion_id))) }
                            }
                        }))
                    }
                    (rest_service_menu("Temple", &settlement.id, "temple", rest_default_minutes, rest_summary, soap_preview))
                }
            } @else if let Some((stock_title, offers)) = trade_offers {
                (merchant_offers_rail(stock_title, offers))
            } @else {
                (sidebar_section("Service", html! {
                    p class="small-copy" { (service_summary) }
                }))
            }
        }
        main class="center-content settlement-main" {
            (party_portrait_overlay(party_members, active_character, &format!("/locations/settlement/{}", settlement.id), None, false))
            (visual_stage("service", npc_name, &format!("{title} host and service counter")))
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
                    items,
                    None,
                    matches!(service_id, "weapons" | "armor" | "clothing"),
                ))
            } @else if service_id == "smith" {
                (inventory_rail(
                    active_character,
                    inventory,
                    items,
                    None,
                    true,
                ))
            } @else if service_id == "religion" {
                (inventory_rail(active_character, inventory, items, None, false))
            } @else {
                (sidebar_section("Service", html! {
                    p class="small-copy" { (service_summary) }
                }))
            }
        }
    };
    settlement_layout_with_session(
        title,
        &settlement.name,
        &settlement.id,
        &settlement.category,
        service_id,
        Some(&settlement.religion_id),
        Some(&settlement.economy),
        content,
        logged_in_as,
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
    encumbrance: EncumbranceSummary,
) -> Markup {
    let title = format!("{}'s inventory", character.name);
    html! {
        (sidebar_section(&title, html! {
            (encumbrance_inventory_rail(html! {
                @if inventory.is_empty() {
                    p class="text-muted small-copy" { "No items carried." }
                } @else {
                    (trade_inventory_table(if direction == "left" { "party-transfer-right" } else { "party-transfer-left" }, InventoryColumnSet::All, true, true, false, html! {
                        @for item in inventory {
                            @let is_equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let target = target_quantity(recipient_targets, &item.item_id);
                            @let item_name = item_display_name(&item.item_id);
                                tr class=(if direction == "left" { "trade-inventory-row trade-row-player" } else { "trade-inventory-row trade-row-merchant" }) data-item-key=(&item.item_id) {
                                    td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                    td class="inventory-item-name" {
                                        (item_name_with_quality(&item.item_id, definition))
                                        span class="inventory-row-actions" {
                                            @if is_equipped {
                                                (disabled_transfer_button(direction, "Equipped items cannot be transferred"))
                                            } @else {
                                                button type="button" class=(format!("trade-transfer trade-transfer-{direction} party-draft-transfer")) data-dynamic-transfer data-default-transfer-mode="one" data-from=(character.id) data-to=(recipient_id) data-item=(item.id) data-key=(&item.item_id) data-count=(item.qty) data-target=(target) data-transfer-mode="one" data-label-one=(format!("Transfer one {item_name}")) data-label-target=(format!("Transfer {item_name} to target")) data-label-all=(format!("Transfer all {item_name}")) aria-label=(format!("Transfer one {item_name}")) title=(format!("Transfer one {item_name}")) { (transfer_glyph(1)) }
                                            }
                                        }
                                    }
                                    td class="inventory-count" { (item.qty) }
                                    td class="inventory-equipped" { (equipment_checkbox(item, definition, is_equipped)) }
                                    td class="inventory-weight" { (item_weight(definition)) }
                                    td class="inventory-gold" { (item_value(definition)) }
                                }
                            }
                    }))
                }
            }, inventory_footer_controls(if direction == "left" { "party-left" } else { "party-right" }, "Transfer to targets", "Transfer everything"), encumbrance))
        }))
    }
}

fn discard_inventory_rail(
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    equip: Option<&CharacterEquip>,
    encumbrance: EncumbranceSummary,
) -> Markup {
    let title = format!("{}'s inventory", character.name);
    html! {
        (sidebar_section(&title, html! {
            (encumbrance_inventory_rail(html! {
                @if inventory.is_empty() {
                    p class="text-muted small-copy" { "No items carried." }
                } @else {
                    (trade_inventory_table("discard-right", InventoryColumnSet::All, true, true, false, html! {
                        @for item in inventory {
                            @let is_equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let item_name = item_display_name(&item.item_id);
                            tr class="trade-inventory-row trade-row-player" data-discard-source=(item.id) data-item-key=(&item.item_id) {
                                td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                td class="inventory-item-name" {
                                    (item_name_with_quality(&item.item_id, definition))
                                    span class="inventory-row-actions" {
                                        @if is_equipped {
                                            (disabled_transfer_button("left", "Equipped items cannot be discarded"))
                                        } @else {
                                            button type="button" class="trade-transfer trade-transfer-left"
                                            data-discard-item=(item.id) data-count=(item.qty)
                                            data-dynamic-transfer data-default-transfer-mode="one" data-transfer-mode="one"
                                            data-label-one=(format!("Discard one {item_name}"))
                                            data-label-target=(format!("Discard {item_name} down to target"))
                                            data-label-all=(format!("Discard all {item_name}"))
                                            aria-label=(format!("Discard {item_name}"))
                                            title=(format!("Discard one {item_name}")) { (transfer_glyph(1)) }
                                        }
                                    }
                                }
                                td class="inventory-count" { (item.qty) }
                                td class="inventory-equipped" { (equipment_checkbox(item, definition, is_equipped)) }
                                td class="inventory-weight" { (item_weight(definition)) }
                                td class="inventory-gold" { (item_value(definition)) }
                            }
                        }
                    }))
                }
            }, html! {}, encumbrance))
        }))
    }
}

pub fn live_merchant_shop_page(
    settlement: &Settlement,
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    food_lots: &[FoodLot],
    party_members: &[Character],
    equip: Option<&CharacterEquip>,
    personal_targets: &[InventoryQuantityTarget],
    party_targets: &[InventoryQuantityTarget],
    pooled: &[PartyInventoryItem],
    shop: MerchantShop,
    shared_language: f32,
    conditions: &[crate::spacetimedb::ItemCondition],
    smith: Option<&crate::spacetimedb::SettlementSmith>,
    repair_orders: &[crate::spacetimedb::RepairOrder],
    now_minutes: u64,
    personal_encumbrance: EncumbranceSummary,
    party_encumbrance: EncumbranceSummary,
    rest_default_minutes: Option<u64>,
    soap_preview: SoapRestPreview,
) -> Markup {
    let title = shop.title();
    let service_id = shop.service_id();
    // Herbalist purchases use a separate reducer and retain their specialized quote.
    let trade_language = if matches!(shop, MerchantShop::Herbalist) {
        1.0
    } else {
        shared_language
    };
    let smith_skill = smith
        .map(|smith| {
            if matches!(shop, MerchantShop::Armor) {
                smith.armourer_skill
            } else if matches!(shop, MerchantShop::Clothing) {
                smith.tailor_skill
            } else {
                smith.weaponsmith_skill
            }
        })
        .unwrap_or(0);
    let player_footer = if matches!(shop, MerchantShop::Herbalist) {
        html! {}
    } else {
        inventory_footer_controls_with_leading(
            matches!(
                shop,
                MerchantShop::Weapons | MerchantShop::Armor | MerchantShop::Clothing
            )
            .then(|| repair_all_control(settlement, service_id)),
            "sell",
            "Sell surplus",
            "Sell everything",
        )
    };
    let content = html! {
        aside class=(if matches!(shop, MerchantShop::Inn) { "left-sidebar smith-wares-column service-left-sidebar" } else { "left-sidebar smith-wares-column" }) {
        div class=(if matches!(shop, MerchantShop::Inn) { "service-left-stack" } else { "merchant-stock-stack" }) {
        div class=(if matches!(shop, MerchantShop::Inn) { "service-inventory-area" } else { "merchant-stock-area" }) {
        (sidebar_section(if matches!(shop, MerchantShop::Herbalist) { "Prepared medicines and ingredients" } else if matches!(shop, MerchantShop::Inn) { "Cooking supplies" } else { "Merchant stock" }, html! {
            div class="smith-wares-scroll" {
            (trade_inventory_table("merchant-left", if matches!(shop, MerchantShop::Weapons) { InventoryColumnSet::Weapons } else if matches!(shop, MerchantShop::Armor) { InventoryColumnSet::Armor } else { InventoryColumnSet::Basic }, false, false, false, html! {
                @for item in items.iter().filter(|item| shop.stocks_at(settlement, item)) {
                    @let is_currency = item.kind == crate::spacetimedb::ItemKind::Currency;
                    @let medication_recipe = adventuresim_core::disease::medication_recipe_for_item(&item.id);
                    @let buy_price = adventuresim_core::strategic_economy::language_adjusted_buy_price(medication_recipe.map_or_else(
                        || adventuresim_core::strategic_economy::merchant_buy_price(item.base_value.unwrap_or(1)),
                        adventuresim_core::strategic_economy::herbalist_medication_price,
                    ), trade_language);
                    @let sell_price = adventuresim_core::strategic_economy::language_adjusted_sell_price((item.base_value.unwrap_or(1) as f32 / 1.25).floor().max(1.0) as u32, trade_language);
                    @let target = target_quantity(personal_targets, &item.id);
                    @let display_name = medication_recipe.map_or_else(|| item_display_name(&item.id), |recipe| recipe.name.to_owned());
                    tr class="trade-inventory-row trade-row-merchant" data-merchant-item=(&item.id) data-merchant-sell-price=(sell_price) data-group-summary="catalog" data-herbalist-medication-name=[medication_recipe.map(|recipe| recipe.name)] { td class="inventory-item-type" { (item_type_icon(&item.id)) } td class="inventory-item-name" { (item_name_with_display(&item.id, &display_name, Some(item))) @if !is_currency { (merchant_buy_controls(&item.id, buy_price, target, 999)) } } td class="inventory-count" hidden { "999" } td class="inventory-weight" { (weight_display(item.weight)) } td class="inventory-gold" { (buy_price) } }
                }
            }))
            (inventory_footer_controls("buy", "Buy to targets", "Buy everything"))
            @if matches!(shop, MerchantShop::Herbalist) {
                p class="small-copy text-muted" { "Prepared courses are sold into your personal inventory as separate, equippable items. Party-inventory purchasing is unavailable here." }
            }
            }
        }))
        }
        @if matches!(shop, MerchantShop::Inn) {
            (rest_service_menu("Inn", &settlement.id, "inn", rest_default_minutes, None, soap_preview))
        }
        }
        @if matches!(shop, MerchantShop::Weapons | MerchantShop::Armor | MerchantShop::Clothing) {
            (repair_custody_panel(settlement, shop, repair_orders, conditions, items, now_minutes, smith_skill))
        }
        }
        main class="center-content settlement-main" { (party_portrait_overlay(party_members, Some(character), &format!("/locations/settlement/{}", settlement.id), None, false)) (visual_stage("service", title, "Merchant counter and attending craftsperson")) (settlement_service_chat_area(title, Some(character), &settlement.id, service_id)) form # "merchant-offer" class="party-offer" action=(if matches!(shop, MerchantShop::Herbalist) { format!("/settlements/{}/herbalist/purchase", settlement.id) } else { format!("/settlements/{}/merchants/offer", settlement.id) }) method="post" hidden role="dialog" aria-modal="true" aria-label="Confirm merchant offer" tabindex="-1" { span class="party-offer-summary" { "Review and submit the staged trade." } input type="hidden" name="return_to" value=(format!("/settlements/{}/{}", settlement.id, service_id)); input type="hidden" name="inventory_scope" value="player"; button type="button" class="party-offer-cancel" data-cancel-trade="merchant" { "Cancel" } button type="submit" disabled { "Offer" } } }
        aside class="right-sidebar inventory-owner-panel" data-inventory-tabs {
            nav class="inventory-owner-tabs" aria-label="Trading inventory" {
                button type="button" class="inventory-owner-tab active" data-inventory-tab="player" { "Player" }
                @if !matches!(shop, MerchantShop::Herbalist) {
                    button type="button" class="inventory-owner-tab" data-inventory-tab="party" { "Party" }
                }
            }
            div data-inventory-pane="player" {
            div class="sidebar-section" {
                (encumbrance_inventory_rail(html! {
                (trade_inventory_table("merchant-player-right", if matches!(shop, MerchantShop::Weapons) { InventoryColumnSet::Weapons } else if matches!(shop, MerchantShop::Armor) { InventoryColumnSet::Armor } else { InventoryColumnSet::Basic }, true, true, matches!(shop, MerchantShop::Weapons | MerchantShop::Armor | MerchantShop::Clothing), html! {
                    @for item in inventory.iter().filter(|item| items.iter().find(|definition| definition.id == item.item_id).is_some_and(|definition| shop.shows_inventory(definition))) {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let food_lot = food_lots.iter().find(|lot| lot.inventory_item_id == Some(item.id));
                        @let is_currency = definition.is_some_and(|definition| definition.kind == crate::spacetimedb::ItemKind::Currency);
                        @let is_equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                        @let sell_price = adventuresim_core::strategic_economy::language_adjusted_sell_price(merchant_inventory_sell_price(definition, food_lot), trade_language);
                        @let target = target_quantity(personal_targets, &item.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&item.item_id) data-merchant-equipped=(is_equipped) data-inventory-quantity=(item.qty) data-target=(target) {
                        @let condition = conditions.iter().find(|condition| condition.inventory_item_id == item.id);
                        @let repair_skill = smith_skill;
                        @let durable_item = definition.is_some_and(|definition| matches!(definition.kind, crate::spacetimedb::ItemKind::Weapon | crate::spacetimedb::ItemKind::Armor | crate::spacetimedb::ItemKind::Shield | crate::spacetimedb::ItemKind::Clothing));
                        @let service_matches = definition.is_some_and(|definition| if matches!(shop, MerchantShop::Armor) { definition.kind == crate::spacetimedb::ItemKind::Armor } else if matches!(shop, MerchantShop::Clothing) { definition.kind == crate::spacetimedb::ItemKind::Clothing } else { matches!(definition.kind, crate::spacetimedb::ItemKind::Weapon | crate::spacetimedb::ItemKind::Shield) });
                        @let can_sell = !is_currency && !is_equipped;
                        td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                        td class="inventory-item-name" { (item_name_with_quality(&item.item_id, definition)) @if !matches!(shop, MerchantShop::Herbalist) && (can_sell || service_matches) { (merchant_sell_repair_controls(item.id, &item.item_id, sell_price, item.qty, target, can_sell, service_matches.then(|| repair_submit_control(settlement, service_id, item.id, condition, repair_skill)))) } }
                        td class="inventory-count" { (quantity_target_control(item.qty, target, &item.item_id, false)) } td class="inventory-equipped" { (equipment_checkbox(item, definition, is_equipped)) } td class="inventory-durability" { @if durable_item { (condition_bar(condition, service_matches.then_some(repair_skill))) } @else { "—" } } td class="inventory-weight" { (merchant_inventory_weight(definition, food_lot)) } td class="inventory-gold" { (sell_price) }
                    }}
                    @for target in personal_targets.iter().filter(|target| target.quantity > 0 && !inventory.iter().any(|item| item.item_id == target.item_id) && items.iter().find(|definition| definition.id == target.item_id).is_some_and(|definition| shop.shows_inventory(definition))) {
                        @let definition = items.iter().find(|definition| definition.id == target.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&target.item_id) data-inventory-quantity="0" data-target=(target.quantity) {
                            td class="inventory-item-type" { (item_type_icon(&target.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(&target.item_id, definition)) }
                            td class="inventory-count" { (quantity_target_control(0, target.quantity, &target.item_id, false)) }
                            td class="inventory-equipped" { input type="checkbox" disabled; }
                            td class="inventory-durability" { "—" }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
                }, player_footer, personal_encumbrance))
            }
            }
            @if !matches!(shop, MerchantShop::Herbalist) { div data-inventory-pane="party" hidden {
            div class="sidebar-section" {
                (encumbrance_inventory_rail(html! {
                (trade_inventory_table("merchant-party-right", if matches!(shop, MerchantShop::Weapons) { InventoryColumnSet::Weapons } else if matches!(shop, MerchantShop::Armor) { InventoryColumnSet::Armor } else { InventoryColumnSet::Basic }, true, false, false, html! {
                    @for item in pooled.iter().filter(|item| items.iter().find(|definition| definition.id == item.item_id).is_some_and(|definition| shop.shows_inventory(definition))) {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let food_lot = food_lots.iter().find(|lot| lot.party_inventory_item_id == Some(item.id));
                        @let is_currency = definition.is_some_and(|definition| definition.kind == crate::spacetimedb::ItemKind::Currency);
                        @let sell_price = adventuresim_core::strategic_economy::language_adjusted_sell_price(merchant_inventory_sell_price(definition, food_lot), trade_language);
                        @let target = target_quantity(party_targets, &item.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&item.item_id) data-party-inventory-id=(item.id) data-inventory-quantity=(item.quantity) data-target=(target) {
                            td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(&item.item_id, definition)) @if !is_currency { (merchant_sell_controls(item.id, &item.item_id, sell_price, item.quantity, target)) } }
                            td class="inventory-count" { (quantity_target_control(item.quantity, target, &item.item_id, true)) }
                            td class="inventory-weight" { (merchant_inventory_weight(definition, food_lot)) }
                            td class="inventory-gold" { (sell_price) }
                        }
                    }
                    // Party purchases may spend pooled coin first and the active
                    // character's coin second. Show both funding sources as the
                    // same collapsed Coin row in this scope.
                    @for item in inventory.iter().filter(|item| items.iter().find(|definition| definition.id == item.item_id).is_some_and(|definition| definition.kind == crate::spacetimedb::ItemKind::Currency)) {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        tr class="trade-inventory-row trade-row-player party-personal-currency" data-merchant-item=(&item.item_id) data-inventory-quantity=(item.qty) data-target="0" title="Personal coin available for party purchases" {
                            td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(&item.item_id, definition)) }
                            td class="inventory-count" { (item.qty) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                    @for target in party_targets.iter().filter(|target| target.quantity > 0 && !pooled.iter().any(|item| item.item_id == target.item_id) && items.iter().find(|definition| definition.id == target.item_id).is_some_and(|definition| shop.shows_inventory(definition))) {
                        @let definition = items.iter().find(|definition| definition.id == target.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&target.item_id) data-inventory-quantity="0" data-target=(target.quantity) {
                            td class="inventory-item-type" { (item_type_icon(&target.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(&target.item_id, definition)) }
                            td class="inventory-count" { (quantity_target_control(0, target.quantity, &target.item_id, true)) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
                }, inventory_footer_controls("sell", "Sell surplus", "Sell everything"), party_encumbrance))
            }
            }
            }
        }
    };
    settlement_layout_with_session(
        title,
        &settlement.name,
        &settlement.id,
        &settlement.category,
        service_id,
        Some(&settlement.religion_id),
        Some(&settlement.economy),
        content,
        Some(&character.name),
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
    party_encumbrance: EncumbranceSummary,
    personal_encumbrance: EncumbranceSummary,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Party inventory", html! {
                (encumbrance_inventory_rail(html! {
                    div class="party-stake-summary" {
                        span { "Your available stake" }
                        strong { (stake) " coin" }
                    }
                    p class="small-copy text-muted" { "Withdrawals use your stake. Personal coin automatically covers an indivisible item's shortfall." }
                    (trade_inventory_table("party-pool-left", InventoryColumnSet::All, true, false, false, html! {
                        @for item in pooled {
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let value = definition.and_then(|definition| definition.base_value).unwrap_or(0) as u64;
                            @let target = target_quantity(personal_targets, &item.item_id);
                            @let current = inventory.iter().find(|personal| personal.item_id == item.item_id).map_or(0, |personal| personal.qty);
                            @let item_name = item_display_name(&item.item_id);
                            tr class="trade-inventory-row" {
                                td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                td class="inventory-item-name" {
                                    (item_name_with_quality(&item.item_id, definition))
                                span class="inventory-row-actions" { button type="button" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-pool-stage=(item.id) data-pool-direction="withdraw" data-transfer-mode="one" data-count=(item.quantity) data-current=(current) data-target=(target) data-label-one=(format!("Withdraw one {item_name}")) data-label-target=(format!("Withdraw {item_name} to target")) data-label-all=(format!("Withdraw all {item_name}")) title=(if value > stake { format!("Withdraw one {item_name}; {} personal coin required", value - stake) } else { format!("Withdraw one {item_name} using your stake") }) aria-label=(format!("Withdraw one {item_name}")) { (transfer_glyph(1)) } }
                                }
                                td class="inventory-count" { (quantity_target_control(item.quantity, target_quantity(party_targets, &item.item_id), &item.item_id, true)) }
                                td class="inventory-weight" { (item_weight(definition)) }
                                td class="inventory-gold" { (item_value(definition)) }
                            }
                        }
                    }))
                }, inventory_footer_controls("withdraw", "Withdraw to personal targets", "Withdraw everything"), party_encumbrance))
            }))
        }
        main class="center-content settlement-main" {
            (party_portrait_overlay(party_members, Some(character), &location.base_path(), None, false))
            (visual_stage("chest", "Party chest", "Shared supplies and each member's stake"))
            (settlement_chat_area("Party inventory", Some(character)))
        }
        aside class="right-sidebar" {
            (sidebar_section(&format!("{}'s inventory", character.name), html! {
                (encumbrance_inventory_rail(html! {
                    p class="small-copy text-muted" { "Add items at their objective coin value." }
                    (trade_inventory_table("party-pool-right", InventoryColumnSet::All, true, true, false, html! {
                        @for item in inventory {
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                            @let target = target_quantity(party_targets, &item.item_id);
                            @let current = pooled.iter().find(|pooled| pooled.item_id == item.item_id).map_or(0, |pooled| pooled.quantity);
                            @let item_name = item_display_name(&item.item_id);
                            tr class="trade-inventory-row" {
                                td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                td class="inventory-item-name" {
                                    (item_name_with_quality(&item.item_id, definition))
                                    span class="inventory-row-actions" {
                                        @if equipped {
                                            (disabled_transfer_button("left", "Equipped items cannot be deposited"))
                                        } @else {
                                            button type="button" class="trade-transfer trade-transfer-left" data-dynamic-transfer data-default-transfer-mode="one" data-pool-stage=(item.id) data-pool-direction="deposit" data-transfer-mode="one" data-count=(item.qty) data-current=(current) data-target=(target) data-label-one=(format!("Deposit one {item_name}")) data-label-target=(format!("Deposit {item_name} to target")) data-label-all=(format!("Deposit all {item_name}")) aria-label=(format!("Deposit one {item_name}")) title=(format!("Deposit one {item_name}")) { (transfer_glyph(1)) }
                                        }
                                    }
                                }
                                td class="inventory-count" { (quantity_target_control(item.qty, target_quantity(personal_targets, &item.item_id), &item.item_id, false)) }
                                td class="inventory-equipped" { (equipment_checkbox(item, definition, equipped)) }
                                td class="inventory-weight" { (item_weight(definition)) }
                                td class="inventory-gold" { (item_value(definition)) }
                            }
                        }
                    }))
                }, inventory_footer_controls("deposit", "Deposit to party targets", "Deposit everything"), personal_encumbrance))
            }))
        }
        form method="post" action=(format!("{}/party-inventory/deposit", location.base_path())) id="pool-transfer-offer" class="party-offer" hidden role="dialog" aria-modal="true" aria-label="Confirm party inventory transfer" tabindex="-1" { span class="party-offer-summary" { "Apply the staged party inventory transfer?" } button type="button" data-cancel-pool class="party-offer-cancel" { "Cancel" } button type="submit" disabled { "Offer" } }
    };
    location.render_layout("Party inventory", content, Some(&character.name))
}

fn item_weight(item: Option<&crate::spacetimedb::ItemDefinition>) -> String {
    item.map_or_else(|| "—".to_owned(), |item| weight_display(item.weight))
}

fn merchant_inventory_weight(
    definition: Option<&crate::spacetimedb::ItemDefinition>,
    food_lot: Option<&FoodLot>,
) -> String {
    food_lot.map_or_else(
        || item_weight(definition),
        |lot| weight_display(lot.mass_kg),
    )
}

fn merchant_inventory_sell_price(
    definition: Option<&crate::spacetimedb::ItemDefinition>,
    food_lot: Option<&FoodLot>,
) -> u32 {
    food_lot.map_or_else(
        || {
            definition.map_or(0, |definition| {
                adventuresim_core::strategic_economy::merchant_sell_price(
                    definition.base_value.unwrap_or(1),
                )
            })
        },
        |lot| {
            adventuresim_core::strategic_economy::merchant_sell_food_lot_value(lot.total_value)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(0)
        },
    )
}

fn encumbrance_inventory_rail(
    content: Markup,
    footer_controls: Markup,
    summary: EncumbranceSummary,
) -> Markup {
    html! {
        div class="encumbrance-inventory-rail" {
            div class="encumbrance-inventory-scroll" { (content) }
            (footer_controls)
            (encumbrance_meter(summary))
        }
    }
}

fn encumbrance_meter(summary: EncumbranceSummary) -> Markup {
    let penalty_percent = summary.penalty_fraction() * 100.0;
    let weight_text = format!("{:.1} / {:.1} kg", summary.burden_kg, summary.capacity_kg);
    let penalty_text = format!("-{penalty_percent:.1}%");
    let accessible_text = format!(
        "Weight {:.1} / {:.1} kilograms; Penalty -{penalty_percent:.1}%",
        summary.burden_kg, summary.capacity_kg
    );
    html! {
        div class="encumbrance" {
            div class="encumbrance-values" aria-hidden="true" {
                span class="encumbrance-weight" { (weight_text) }
                span class="encumbrance-penalty" { (penalty_text) }
            }
            div class="encumbrance-visual" {
                div class="encumbrance-meter"
                    role="meter"
                    aria-label="Encumbrance"
                    aria-valuemin="0"
                    aria-valuemax="100"
                    aria-valuenow=(format!("{penalty_percent:.1}"))
                    aria-valuetext=(accessible_text) {
                    span class="encumbrance-marker"
                        style=(format!("--encumbrance-position: {penalty_percent:.4}%")) {}
                }
            }
        }
    }
}

fn equipment_checkbox(
    inventory: &InventoryItem,
    definition: Option<&crate::spacetimedb::ItemDefinition>,
    equipped: bool,
) -> Markup {
    let equippable = definition.is_some_and(|definition| {
        definition.slot != ItemSlot::None
            || definition.kind == crate::spacetimedb::ItemKind::Medication
    });
    let item_name = item_display_name(&inventory.item_id);
    let label = if equipped {
        format!("Unequip {item_name}")
    } else {
        format!("Equip {item_name}")
    };
    html! {
        input type="checkbox"
            checked[equipped]
            disabled[!equippable]
            data-equipment-toggle
            data-inventory-item-id=(inventory.id)
            aria-label=(label)
            title=(if equippable { "Equip or unequip this item" } else { "This item cannot be equipped" });
    }
}

fn item_value(item: Option<&crate::spacetimedb::ItemDefinition>) -> String {
    item.and_then(|item| item.base_value)
        .map_or_else(|| "—".to_owned(), |value| value.to_string())
}

pub(super) fn item_name_with_quality(
    item_id: &str,
    definition: Option<&crate::spacetimedb::ItemDefinition>,
) -> Markup {
    let currency_name = adventuresim_core::strategic_currency::currency_name(item_id);
    if let Some(currency_name) = currency_name {
        html! {
            span class="inventory-item-label" data-item-name="Coin"
                data-item-kind="currency" data-currency-name=(currency_name) { "Coin" }
        }
    } else {
        let display_name = item_display_name(item_id);
        item_name_with_display(item_id, &display_name, definition)
    }
}

fn item_name_with_display(
    item_id: &str,
    display_name: &str,
    definition: Option<&crate::spacetimedb::ItemDefinition>,
) -> Markup {
    let alcohol_group = definition
        .filter(|item| item.alcohol_serving_ml > 0)
        .map(|_| "alcohol");
    let quality = definition
        .filter(|item| {
            matches!(
                item.kind,
                crate::spacetimedb::ItemKind::Weapon
                    | crate::spacetimedb::ItemKind::Armor
                    | crate::spacetimedb::ItemKind::Shield
            )
        })
        .map(|item| item.quality.clamp(1, 5));
    let label = quality.map(|quality| match quality {
        1 => "Quality 1",
        2 => "Quality 2",
        3 => "Quality 3 — munition grade",
        4 => "Quality 4 — knightly commission",
        5 => "Quality 5 — royal or heroic commission",
        _ => unreachable!(),
    });
    let damage_types = definition.map(|item| {
        [
            item.blunt.then_some("Blunt"),
            item.slash.then_some("Slash"),
            item.pierce.then_some("Pierce"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ")
    });
    html! {
        span class=(quality.map_or_else(|| "inventory-item-label".to_string(), |quality| format!("inventory-item-label item-quality-{quality}"))) title=[label]
            data-item-name=(item_id)
            data-item-kind=[definition.map(|item| format!("{:?}", item.kind).to_ascii_lowercase())]
            data-item-group=[alcohol_group]
            data-group-name=[alcohol_group.map(|_| "Alcohol")]
            data-food-lot=[adventuresim_core::food::definition(item_id).map(|_| "true")]
            data-stat-accuracy=[definition.map(|item| weight_display(item.accuracy))]
            data-stat-reach=[definition.map(|item| weight_display(item.reach))]
            data-stat-penetration=[definition.map(|item| weight_display(item.penetration))]
            data-stat-damage=[damage_types]
            data-stat-block=[definition.map(|item| weight_display(item.block))]
            data-stat-coverage=[definition.map(|item| weight_display(item.coverage))]
            data-stat-resistance=[definition.map(|item| weight_display(item.resistance))]
            data-stat-padding=[definition.map(|item| weight_display(item.padding))]
            data-stat-flexibility=[definition.map(|item| weight_display(item.flexibility))]
            data-stat-range-of-motion=[definition.map(|item| weight_display(item.range_of_motion))]
            data-detail-slot=[definition.map(|item| format!("{:?}", item.slot))]
            data-detail-balance=[definition.map(|item| weight_display(item.balance))]
            data-detail-mode=[definition.map(|item| match (item.melee, item.ranged, item.precise) { (true, true, true) => "Melee, ranged, precise", (true, true, false) => "Melee and ranged", (true, false, true) => "Melee, precise", (false, true, true) => "Ranged, precise", (true, false, false) => "Melee", (false, true, false) => "Ranged", (false, false, true) => "Precise", _ => "—" }.to_string())] {
            (display_name)
        }
    }
}

fn weight_display(weight: f32) -> String {
    let display = format!("{weight:.2}");
    display
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn trade_inventory_table(
    namespace: &str,
    optional_columns: InventoryColumnSet,
    show_quantities: bool,
    show_equipped: bool,
    show_condition: bool,
    rows: Markup,
) -> Markup {
    InventoryBrowser {
        namespace,
        show_quantities,
        show_equipped,
        show_condition,
        optional_columns,
        rows,
    }
    .render()
}

fn target_quantity(targets: &[InventoryQuantityTarget], item_id: &str) -> u32 {
    targets
        .iter()
        .find(|target| target.item_id == item_id)
        .map_or(0, |target| target.quantity)
}

fn quantity_target_control(quantity: u32, target: u32, item_id: &str, party_scope: bool) -> Markup {
    let item_name = item_display_name(item_id);
    html! {
        span class="inventory-target-control" data-target-control data-quantity=(quantity) data-item-id=(item_id) data-party-scope=(party_scope) title=(format!("Carrying {quantity}; target {target}")) {
            span class="inventory-target-value" data-target-value role="button" tabindex="0"
                aria-label=(format!("Target quantity for {item_name}"))
                title=(format!("Click to edit the target quantity for {item_name}")) { (target) }
        }
    }
}

pub(crate) fn transfer_glyph(count: usize) -> Markup {
    html! { span class=(format!("inventory-transfer-glyph arrows-{count}")) aria-hidden="true" { @for _ in 0..count { i {} } } }
}

fn disabled_transfer_button(direction: &str, explanation: &str) -> Markup {
    html! {
        button type="button" class=(format!("trade-transfer trade-transfer-{direction}")) disabled title=(explanation) aria-label=(explanation) { (transfer_glyph(1)) }
    }
}

fn merchant_buy_controls(item_id: &str, price: u32, target: u32, available: u32) -> Markup {
    let item_name = item_display_name(item_id);
    html! { span class="inventory-row-actions" {
        button type="button" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-merchant-buy=(item_id) data-merchant-buy-price=(price) data-transfer-mode="one" data-target=(target) data-count=(available) data-label-one=(format!("Buy one {item_name}")) data-label-target=(format!("Buy {item_name} to target")) data-label-all=(format!("Buy all {item_name}")) aria-label=(format!("Buy one {item_name}")) title=(format!("Buy one {item_name}")) { (transfer_glyph(1)) }
    } }
}

fn merchant_sell_controls(
    id: u64,
    item_id: &str,
    price: u32,
    quantity: u32,
    target: u32,
) -> Markup {
    let item_name = item_display_name(item_id);
    html! { span class="inventory-row-actions" {
        button type="button" class="trade-transfer trade-transfer-left" data-dynamic-transfer data-default-transfer-mode="one" data-merchant-sell=(id) data-item-name=(item_id) data-merchant-sell-price=(price) data-transfer-mode="one" data-count=(quantity) data-target=(target) data-label-one=(format!("Sell one {item_name}")) data-label-target=(format!("Sell surplus {item_name}")) data-label-all=(format!("Sell all {item_name}")) aria-label=(format!("Sell one {item_name}")) title=(format!("Sell one {item_name}")) { (transfer_glyph(1)) }
    } }
}

fn merchant_sell_repair_controls(
    id: u64,
    item_id: &str,
    price: u32,
    quantity: u32,
    target: u32,
    can_sell: bool,
    repair: Option<Markup>,
) -> Markup {
    let has_repair = repair.is_some();
    let item_name = item_display_name(item_id);
    html! { div class=(if has_repair { "inventory-row-actions smith-player-actions" } else { "inventory-row-actions" }) {
        @if let Some(repair) = repair { (repair) }
        @if can_sell {
            button type="button" class="trade-transfer trade-transfer-left" data-dynamic-transfer data-default-transfer-mode="one" data-merchant-sell=(id) data-item-name=(item_id) data-merchant-sell-price=(price) data-transfer-mode="one" data-count=(quantity) data-target=(target) data-label-one=(format!("Sell one {item_name}")) data-label-target=(format!("Sell surplus {item_name}")) data-label-all=(format!("Sell all {item_name}")) aria-label=(format!("Sell one {item_name}")) title=(format!("Sell one {item_name}")) { (transfer_glyph(1)) }
        } @else if has_repair {
            (disabled_transfer_button("left", "Equipped items cannot be sold"))
        }
    } }
}

fn condition_bar(
    condition: Option<&crate::spacetimedb::ItemCondition>,
    repair_skill: Option<u8>,
) -> Markup {
    let bins = condition.map(|value| value.bins()).unwrap_or([0.0; 5]);
    let total = bins.iter().sum::<f32>().clamp(0.0, 1.0);
    let green = (1.0 - total).max(0.0);
    let label = if total <= f32::EPSILON {
        "Full durability".to_string()
    } else if repair_skill
        .is_some_and(|skill| bins.iter().take(skill.min(5) as usize).sum::<f32>() > f32::EPSILON)
    {
        "Damaged; the flashing portion can be repaired by this smith".to_string()
    } else {
        "Damaged beyond this smith's skill".to_string()
    };
    html! {
        span class="condition-bar" data-sort-value=(weight_display(green)) title=(&label) aria-label=(&label) {
            span class="condition-green" style=(format!("width:{}%", green * 100.0)) {}
            @for (index, amount) in bins.iter().enumerate() {
                @let repairable = repair_skill.is_some_and(|skill| index < skill.min(5) as usize);
                span class=(format!("condition-tier-{}{}", index + 1, if repairable { " condition-repairable" } else { "" })) style=(format!("width:{}%", amount.clamp(0.0, 1.0) * 100.0)) {}
            }
        }
    }
}

fn completed_repair_condition_bar(
    condition: Option<&crate::spacetimedb::ItemCondition>,
    smith_skill: u8,
) -> Markup {
    let Some(condition) = condition else {
        return condition_bar(None, None);
    };
    let mut repaired = condition.clone();
    let mut bins = [
        &mut repaired.tier_1,
        &mut repaired.tier_2,
        &mut repaired.tier_3,
        &mut repaired.tier_4,
        &mut repaired.tier_5,
    ];
    for amount in bins.iter_mut().take(smith_skill.min(5) as usize) {
        **amount = 0.0;
    }
    condition_bar(Some(&repaired), None)
}

fn repair_all_control(settlement: &Settlement, service_id: &str) -> Markup {
    html! {
        form class="repair-all-form inventory-footer-repair" action=(format!("/settlements/{}/{}/repair-all", settlement.id, service_id)) method="post" {
            button type="submit" class="repair-all-button" title="Entrust all eligible items for repair" aria-label="Repair all eligible items" {
                span class="repair-action-icon" aria-hidden="true" {}
            }
        }
    }
}

fn repair_submit_control(
    settlement: &Settlement,
    service_id: &str,
    inventory_item_id: u64,
    condition: Option<&crate::spacetimedb::ItemCondition>,
    skill: u8,
) -> Markup {
    let total = condition.map_or(0.0, |value| value.total());
    let repairable = condition.map_or(0.0, |value| value.repairable(skill));
    let residual = condition.map_or(0.0, |value| value.residual(skill));
    let disabled = total <= f32::EPSILON || repairable <= f32::EPSILON;
    let explanation = if total <= f32::EPSILON {
        "Item is already in full condition".to_string()
    } else if repairable <= f32::EPSILON {
        format!("All damage requires Smithing above this smith's level {skill}")
    } else if residual > f32::EPSILON {
        "Repair all damage within this smith's skill; harder damage will remain".to_string()
    } else {
        format!("Repair all damage (smith level {skill})")
    };
    html! {
        form class="row-repair-form" action=(format!("/settlements/{}/{}/repair", settlement.id, service_id)) method="post" {
            input type="hidden" name="inventory_item_id" value=(inventory_item_id);
            @if disabled {
                span class="disabled-repair-explanation" tabindex="0" title=(&explanation) aria-label=(&explanation) {
                    button type="submit" class="repair-item-button" disabled { span class="repair-action-icon" aria-hidden="true" {} }
                }
            } @else {
                button type="submit" class="repair-item-button" title=(&explanation) aria-label=(&explanation) { span class="repair-action-icon" aria-hidden="true" {} }
            }
        }
    }
}

fn repair_custody_panel(
    settlement: &Settlement,
    shop: MerchantShop,
    orders: &[crate::spacetimedb::RepairOrder],
    conditions: &[crate::spacetimedb::ItemCondition],
    items: &[crate::spacetimedb::ItemDefinition],
    now: u64,
    smith_skill: u8,
) -> Markup {
    let service_id = shop.service_id();
    let mut matching: Vec<_> = orders
        .iter()
        .filter(|order| {
            order.settlement_id == settlement.id
                && items
                    .iter()
                    .find(|item| item.id == order.item_id)
                    .is_some_and(|item| shop.stocks(item))
        })
        .collect();
    matching.sort_by_key(|order| (order.submitted_at_minutes, order.id));
    html! {
        section class="repair-custody-panel" aria-label="Items entrusted for repair" {
            header class="repair-custody-header" {
                h3 { @if matches!(shop, MerchantShop::Clothing) { "In the tailor's care" } @else { "In the smith's care" } }
                @let craft = if matches!(shop, MerchantShop::Clothing) { "Tailoring" } else { "Smithing" };
                span class="repair-custody-skill" title=(format!("{craft} {smith_skill}")) {
                    (stat_icon(craft, "skills", if craft == "Tailoring" { "sewing-needle" } else { "smithing" }, false))
                    (skill_rank_bar(f32::from(smith_skill), f32::from(smith_skill), &format!("{craft} {smith_skill}"), SkillRankBarOptions::default()))
                }
            }
            div class="repair-custody-scroll" {
                @if matching.is_empty() { p class="text-muted small-copy" { "No items entrusted." } }
                div class="repair-custody-list" {
                    table class="trade-inventory-table repair-custody-table" {
                        colgroup {
                            col class="inventory-column-type";
                            col class="inventory-column-item";
                            col class="inventory-column-durability";
                            col class="repair-column-eta";
                            col class="inventory-column-gold";
                            col class="inventory-column-actions";
                        }
                        thead { tr {
                            (item_type_header())
                            th scope="col" class="inventory-column-item" { "Item" }
                            th scope="col" class="inventory-column-durability" { "Durability" }
                            th scope="col" class="repair-column-eta" { "ETA" }
                            th scope="col" class="inventory-column-gold" title="Full repair cost (Currency)" { (currency_header("Full repair cost in Currency")) }
                            th class="inventory-actions-header" aria-label="Repair retrieval actions" {
                                div class="inventory-footer-actions repair-custody-header-actions" {
                                    form class="repair-retrieve-all-form" data-repair-retrieve-form data-bulk-action=(format!("/settlements/{}/{}/repairs/retrieve", settlement.id, service_id)) action=(format!("/settlements/{}/{}/repairs/retrieve", settlement.id, service_id)) method="post" {
                                        input type="hidden" name="limit" value="2";
                                        button type="submit" class="trade-transfer trade-transfer-right inventory-footer-transfer repair-retrieve-all" data-dynamic-transfer data-default-transfer-mode="target" data-transfer-mode="target" data-label-target="Retrieve up to two completed repairs" data-label-all="Retrieve all completed repairs" title="Retrieve up to two completed repairs" aria-label="Retrieve up to two completed repairs" { (transfer_glyph(2)) }
                                    }
                                }
                            }
                        } }
                        tbody {
                        @for order in matching {
                            @let condition = conditions.iter().find(|condition| condition.inventory_item_id == order.inventory_item_id);
                            @let definition = items.iter().find(|item| item.id == order.item_id);
                            @let ready = now >= order.ready_at_minutes;
                            @let remaining = order.ready_at_minutes.saturating_sub(now);
                            tr class="trade-inventory-row trade-row-merchant repair-order-row" {
                                td class="inventory-item-type" { (item_type_icon(&order.item_id)) }
                                td class="inventory-item-name" { (item_name_with_quality(&order.item_id, definition)) }
                                td class="inventory-durability" {
                                    @if ready { (completed_repair_condition_bar(condition, order.smith_skill)) }
                                    @else { (condition_bar(condition, Some(order.smith_skill))) }
                                }
                                td class="repair-column-eta" { @if ready { "Ready" } @else { (format!("{}h {}m", remaining / 60, remaining % 60)) } }
                                td class="inventory-gold" title="Quoted full-job cost, paid on retrieval" { (order.quoted_cost) }
                                td class="inventory-actions-cell" aria-label="Item actions" {
                                    span class="inventory-row-actions repair-retrieve-actions" {
                                        form data-repair-retrieve-form data-single-action=(format!("/settlements/{}/{}/repairs/{}/retrieve", settlement.id, service_id, order.id)) data-bulk-action=(format!("/settlements/{}/{}/repairs/retrieve", settlement.id, service_id)) action=(format!("/settlements/{}/{}/repairs/{}/retrieve", settlement.id, service_id, order.id)) method="post" {
                                            input type="hidden" name="item_id" value=(&order.item_id);
                                            input type="hidden" name="limit" value="1" disabled;
                                            button type="submit" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-transfer-mode="one" data-label-one="Retrieve this completed item" data-label-target="Retrieve up to two completed matching items" data-label-all="Retrieve all completed matching items" disabled[!ready] title=(if ready { "Retrieve this completed item" } else { "Repair is still underway" }) aria-label="Retrieve this completed item" { (transfer_glyph(1)) }
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
}

pub(crate) fn inventory_footer_controls(
    action: &str,
    target_label: &str,
    all_label: &str,
) -> Markup {
    inventory_footer_controls_with_leading(None, action, target_label, all_label)
}

fn inventory_footer_controls_with_leading(
    leading: Option<Markup>,
    action: &str,
    target_label: &str,
    all_label: &str,
) -> Markup {
    let grouped = leading.is_some();
    html! { div class=(if grouped { "inventory-footer-actions inventory-footer-actions-grouped" } else { "inventory-footer-actions" }) {
        @if let Some(leading) = leading { (leading) }
        button type="button" class="trade-transfer inventory-footer-transfer" data-dynamic-transfer data-default-transfer-mode="target" data-inventory-bulk=(action) data-transfer-mode="target" data-label-target=(target_label) data-label-all=(all_label) aria-label=(target_label) title=(target_label) { (transfer_glyph(2)) }
    } }
}

fn currency_header(label: &str) -> Markup {
    game_icon(label, "coins")
}

// Kept for one-sided placeholder/service tables that are intentionally not
// inventory browsers.
fn trade_inventory_table_header(show_equipped: bool, condition_header: Option<Markup>) -> Markup {
    html! { thead { tr {
        (item_type_header())
        th scope="col" class="inventory-column-item" { "Item" }
        th scope="col" class="inventory-column-count" { "#" }
        @if show_equipped { th scope="col" class="inventory-column-equipped" title="Equipped" { (game_icon("Equipped", "check-mark")) } }
        @if let Some(condition_header) = condition_header { th scope="col" class="inventory-column-durability" { (condition_header) } }
        th scope="col" class="inventory-column-weight" title="Weight" { (game_icon("Weight", "weight")) }
        th scope="col" class="inventory-column-gold" title="Currency" { (currency_header("Currency")) }
    } } }
}

#[derive(Clone, Copy, Default)]
struct CharacterSkillActions<'a> {
    cooking_href: Option<&'a str>,
    cooking_open: bool,
    examination_action: Option<&'a str>,
    examination_open: bool,
}

#[derive(Clone, Copy)]
enum SkillAction<'a> {
    Get {
        href: &'a str,
        label: &'a str,
        open: bool,
    },
    Post {
        href: &'a str,
        label: &'a str,
        open: bool,
    },
}

fn skill_action_icon(name: &str, icon: &str, action: SkillAction<'_>, inside_form: bool) -> Markup {
    let (href, label, open) = match action {
        SkillAction::Get { href, label, open } | SkillAction::Post { href, label, open } => {
            (href, label, open)
        }
    };
    html! {
        @match action {
            SkillAction::Get { .. } => {
                a class=(if open { "character-menu-button is-open" } else { "character-menu-button" })
                    href=(href) title=(label) aria-label=(label) aria-haspopup="dialog" aria-expanded=(open)
                    data-dialog-opener=(href) {
                    span class="stat-icon" style=(format!("--stat-icon: url('/static/icons/game/{icon}.svg')")) aria-hidden="true" {}
                    @if open { span class="sr-only" { " (open)" } }
                }
            }
            SkillAction::Post { .. } => {
                @if inside_form {
                    button type="submit" class=(if open { "character-menu-button is-open" } else { "character-menu-button" })
                        formaction=(href) formmethod="post"
                        title=(label) aria-label=(label) aria-haspopup="dialog" aria-expanded=(open)
                        data-dialog-opener=(href) {
                        span class="stat-icon" style=(format!("--stat-icon: url('/static/icons/game/{icon}.svg')")) aria-hidden="true" {}
                        @if open { span class="sr-only" { " (open)" } }
                    }
                } @else {
                    form method="post" action=(href) class="character-menu-button-form" {
                        button type="submit" class=(if open { "character-menu-button is-open" } else { "character-menu-button" })
                            title=(label) aria-label=(label) aria-haspopup="dialog" aria-expanded=(open)
                            data-dialog-opener=(href) {
                            span class="stat-icon" style=(format!("--stat-icon: url('/static/icons/game/{icon}.svg')")) aria-hidden="true" {}
                            @if open { span class="sr-only" { " (open)" } }
                        }
                    }
                }
            }
        }
        span class="sr-only" { (name) }
    }
}

fn party_skills_rail(
    title: &str,
    skills: Option<&CharacterSkills>,
    limbs: Option<&CharacterLimbs>,
    schedule: Option<&CharacterTrainingSchedule>,
    schedule_action: Option<&str>,
    activity_preview: Option<ActivityPreviewRates>,
    professes_religion: bool,
    prayer_religion_check: f32,
    training_religion_id: Option<&str>,
    combat_profile: CombatTrainingProfile,
    actions: CharacterSkillActions<'_>,
) -> Markup {
    let head_health = limbs.map_or(1.0, |limbs| limbs.head_health);
    let upper_health = limbs.map_or(1.0, |limbs| {
        (limbs.left_arm_health + limbs.right_arm_health) / 2.0
    });
    let lower_health = limbs.map_or(1.0, |limbs| {
        (limbs.left_leg_health + limbs.right_leg_health) / 2.0
    });
    html! {
        (sidebar_section("", html! {
            @if let Some(skills) = skills {
                h3 class="sr-only" { (title) }
                @if let (Some(schedule), Some(action)) = (schedule, schedule_action) {
                    form class="skill-schedule" data-skill-schedule action=(action) method="post" {
                        (skills_table(
                            title, skills, head_health, upper_health, lower_health, Some(schedule),
                            activity_preview, professes_religion, prayer_religion_check,
                            training_religion_id.and_then(OfficialReligion::from_id),
                            combat_profile, action.starts_with("/locations/settlement/"),
                            actions,
                        ))
                        div class="schedule-save-status" data-schedule-save-status role="status" aria-live="polite" hidden {
                            span { "Schedule could not be saved." }
                            button type="button" data-schedule-retry { "Retry" }
                        }
                    }
                    @if action.starts_with("/locations/settlement/") {
                        (immediate_activity_dialog(&action.replace("/schedule", "/activity")))
                    }
                    script src="/static/training-schedule.js?v=apprentice-system-1" {}
                    script src="/static/immediate-activity.js?v=manual-activities-1" {}
                } @else {
                    (skills_table(
                        title, skills, head_health, upper_health, lower_health, None, None,
                        professes_religion, prayer_religion_check,
                        training_religion_id.and_then(OfficialReligion::from_id),
                        combat_profile, false,
                        actions,
                    ))
                    script src="/static/training-schedule.js?v=apprentice-system-1" {}
                }
            } @else {
                h3 class="sidebar-header" { (title) }
                p class="text-muted small-copy" { "Skill records have not been created yet." }
            }
        }))
    }
}

fn skills_table(
    title: &str,
    skills: &CharacterSkills,
    head_health: f32,
    upper_health: f32,
    lower_health: f32,
    schedule: Option<&CharacterTrainingSchedule>,
    activity_preview: Option<ActivityPreviewRates>,
    professes_religion: bool,
    prayer_religion_check: f32,
    training_religion: Option<OfficialReligion>,
    combat_profile: CombatTrainingProfile,
    immediate_actions: bool,
    actions: CharacterSkillActions<'_>,
) -> Markup {
    html! {
            table class="party-skills-table" {
                colgroup {
                    col class="party-skill-name-column";
                    @if schedule.is_some() {
                        col class="schedule-effect-column";
                        col class="schedule-effect-column schedule-training-column";
                        col class="schedule-effect-column";
                        col class="schedule-effect-column";
                        col class="schedule-effect-column";
                    } @else {
                        col class="party-skill-meter-column";
                    }
                }
                @if schedule.is_some() {
                    colgroup {
                        col class="religion-auto-column";
                        col class="party-skill-time-column";
                        col class="religion-expand-column";
                    }
                } @else {
                    colgroup { col class="religion-expand-column"; }
                }
                thead { tr class="schedule-context-heading" {
                        th scope="colgroup" colspan=(if schedule.is_some() { "8" } else { "2" }) class="schedule-table-title" { (title) }
                    th scope="col" aria-label="Skill details" {}
                } }
                tbody {
                    @if skills.will_hours > 0.0 { (party_skill_row("Will", "will", Skill::Will, skills.will_hours, head_health, schedule.is_some(), None)) }
                    (social_skill_rows(skills, head_health, schedule))
                    @if skills.medicine_hours > 0.0 { (party_skill_row("Medicine", "medicine", Skill::Medicine, skills.medicine_hours, head_health, schedule.is_some(), actions.examination_action.map(|href| SkillAction::Post { href, label: "Perform medical examination (15 minutes)", open: actions.examination_open }))) }
                    (party_skill_row("Cooking", "cooking", Skill::Cooking, skills.cooking_hours, head_health, schedule.is_some(), actions.cooking_href.map(|href| SkillAction::Get { href, label: "Open cooking menu", open: actions.cooking_open })))
                    (religion_skill_rows(skills, head_health, schedule, training_religion))
                    (language_skill_rows(skills, schedule.is_some()))
                    (combat_skill_rows(skills, head_health, upper_health, lower_health, schedule, combat_profile))
                    @if skills.stealth_hours > 0.0 { (party_skill_row("Stealth", "stealth", Skill::Stealth, skills.stealth_hours, upper_health, schedule.is_some(), None)) }
                    (terrain_skill_rows(skills, schedule.is_some()))
                    @if skills.anatomy_hours > 0.0 { (party_skill_row("Anatomy", "surgeon", Skill::Anatomy, skills.anatomy_hours, head_health, schedule.is_some(), None)) }
                    @if skills.tailoring_hours > 0.0 { (party_skill_row("Tailoring", "sewing-needle", Skill::Tailoring, skills.tailoring_hours, upper_health, schedule.is_some(), None)) }
                    @if skills.smithing_hours > 0.0 { (party_skill_row("Smithing", "smithing", Skill::Smithing, skills.smithing_hours, upper_health, schedule.is_some(), None)) }
                    @if let Some(schedule) = schedule {
                        @let preview = activity_preview.unwrap_or_default();
                        tr class="schedule-divider" { td colspan="9" {} }
                        tr class="schedule-section-heading" {
                            th { span class="sr-only" { "Activities" } }
                            th scope="col" title="Currency" { (schedule_header_icon("coins", "Currency")) }
                            th scope="col" title="Virtue" { (schedule_header_icon("scales", "Virtue")) }
                            th scope="col" title="Morale" { (schedule_header_icon("sun", "Morale")) }
                            th scope="col" title="Fatigue" { (schedule_header_icon("night-sleep", "Fatigue")) }
                            th scope="col" title="Effective skill-hours gained at the current daily allocation" { (schedule_header_icon("open-book", "Skill-hours")) }
                            th scope="col" {}
                            th scope="col" title="Daily allocation" { (schedule_header_icon("duration", "Daily allocation")) }
                            th scope="col" aria-label="Skill details" {}
                        }
                        (schedule_special_row(
                            if professes_religion { "Prayer" } else { "Meditate" },
                            if professes_religion { "prayer" } else { "inner-self" },
                            "prayer_minutes", schedule.downtime.prayer_minutes, true, immediate_actions,
                            if professes_religion { ActivityEffectRates::prayer(prayer_religion_check / 5.0) } else { ActivityEffectRates::meditation() }, None,
                            None,
                            if professes_religion {
                                "Prayer trains the professed Religion at 25% speed; morale depends on party knowledge and satisfies Fervor-driven needs."
                            } else {
                                "Meditation gives modest morale independently of party Religion knowledge and does not train Religion or create Fervor."
                            },
                        ))
                        (schedule_special_row("Combat Training", "crossed-swords", "combat_training_minutes", schedule.downtime.combat_training_minutes, true, immediate_actions, ActivityEffectRates::default(), None, None, "Sparring and target practice train equipped Combat skills together with Will and Balance."))
                        (schedule_special_row("Carousing", "beer-stein", "carousing_minutes", schedule.downtime.carousing_minutes, true, immediate_actions, ActivityEffectRates::carousing(), None, None, "Drink and socialize to improve morale and train Humor at 25% speed, at a small cost to Virtue."))
                        @if let Some(service_id) = schedule.downtime.apprenticeship_service_id.as_deref() {
                            (schedule_service_selection("apprenticeship_service_id", service_id))
                            (schedule_special_row(&format!("Apprenticeship — {}", profession_label(service_id)), "open-book", "apprenticeship_minutes", schedule.downtime.apprenticeship_minutes, true, immediate_actions && preview.profession.contains_key(service_id), ActivityEffectRates::default(), None, preview.profession.get(service_id), "Pay one coin per completed eight hours of instruction in an enrolled profession. Religious students are called novices."))
                        }
                        @if let Some(service_id) = schedule.downtime.profession_service_id.as_deref() {
                            (schedule_service_selection("profession_service_id", service_id))
                            @if let Some(profession) = preview.profession.get(service_id) {
                                @if profession.tier_label != "apprentice" && profession.tier_label != "novice" {
                                    @let religious = service_id == "religion";
                                    (schedule_special_row(&format!("Profession Practice — {}", profession_label(service_id)), if religious { "holy-symbol" } else { "anvil" }, "profession_practice_minutes", schedule.downtime.profession_practice_minutes, true, immediate_actions, ActivityEffectRates::default(), None, Some(profession), if religious { "Practice as a cleric or teacher to serve the community and earn Virtue; teachers earn faster than clerics." } else { "Practice an enrolled profession independently. Journeymen earn one coin per eight hours; masters earn one per two hours." }))
                                }
                            }
                        }
                        (schedule_special_row("Labor", "hammer-sickle", "labor_minutes", schedule.downtime.labor_minutes, true, immediate_actions, ActivityEffectRates::linear(preview.labor_gold_per_hour, 0.0, 0.0, LABOR_FATIGUE_PER_HOUR / FATIGUE_RESERVOIR_PER_PREVIEW_POINT), None, None, "Earn coin during settlement downtime from Strength and Endurance checks; trains Will at 25% speed and generates fatigue."))
                        (schedule_special_row("Thievery", "lockpicks", "thievery_minutes", schedule.downtime.thievery_minutes, true, immediate_actions, ActivityEffectRates::linear(preview.thievery_gold_per_hour, preview.thievery_virtue_per_hour, 0.0, 0.0), None, None, "Settlement downtime can earn coin and risk discovery while training Stealth at 25% speed."))
                        (schedule_special_row("Raiding", "mounted-knight", "raiding_minutes", schedule.downtime.raiding_minutes, true, immediate_actions, ActivityEffectRates::linear(preview.raiding_gold_per_hour, preview.raiding_virtue_per_hour, 0.0, 0.0), None, None, "Settlement downtime can earn coin and risk retaliation while feeding the equipment-derived Combat training distribution at 25% speed."))
                        @let leisure = leisure_preview(&schedule.downtime, preview.current_fatigue);
                        (schedule_special_row("Leisure", "bed", "leisure_minutes", 0, false, false, ActivityEffectRates::default(), Some(leisure), None, "Unallocated downtime first offsets baseline and activity fatigue; only surplus recovery improves morale."))
                    }
            }
        }
    }
}

fn terrain_skill_rows(skills: &CharacterSkills, schedule_context: bool) -> Markup {
    let entries = [
        (
            "Plains",
            "plains",
            Skill::TerrainPlains,
            skills.terrain_plains_hours,
        ),
        (
            "Forest",
            "forest",
            Skill::TerrainForest,
            skills.terrain_forest_hours,
        ),
        (
            "Hills",
            "hills",
            Skill::TerrainHills,
            skills.terrain_hills_hours,
        ),
        (
            "Urban",
            "urban",
            Skill::TerrainUrban,
            skills.terrain_urban_hours,
        ),
    ];
    let rank = entries
        .iter()
        .map(|entry| entry.2.training_rank(entry.3))
        .sum::<f32>()
        / 4.0;
    html! {
        tr class="party-skill-row terrain-primary-row" data-terrain-primary {
            th scope="row" class="party-skill-name party-skill-icon-cell" { (stat_icon("Terrain", "terrain", "terrain", false)) }
            td class="party-skill-meter" colspan=[schedule_context.then_some("7")] {
                (skill_rank_bar(rank, rank, "Unweighted mean; route previews use the local terrain mixture", skill_rail_bar_options()))
            }
            td class="religion-expand-cell" {
                button type="button" class="religion-expand-button" data-terrain-expand aria-expanded="false" aria-label="Expand Terrain skills" title="Expand Terrain" {
                    span class="religion-expand-chevron" aria-hidden="true" { "›" }
                }
            }
        }
        @for (name, icon, skill, hours) in entries {
            tr class="party-skill-row terrain-detail-row" data-terrain-detail hidden {
                th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" {
                    (stat_icon(name, "terrain", icon, false))
                }
                td class="party-skill-meter" colspan=[schedule_context.then_some("7")] {
                    @let sub_rank = skill.training_rank(hours);
                    (skill_rank_bar(sub_rank, sub_rank, &format!("{:.1} hours invested", hours.max(0.0)), skill_rail_bar_options()))
                }
                td class="religion-expand-cell" {}
            }
        }
    }
}

fn language_skill_rows(skills: &CharacterSkills, schedule_context: bool) -> Markup {
    use adventuresim_world_schema::{OralLanguage, WrittenLanguage};
    let oral_effective = OralLanguage::ALL
        .into_iter()
        .map(|language| skills.oral_languages.effective(language))
        .fold(0.0, f32::max);
    let oral_direct = OralLanguage::ALL
        .into_iter()
        .map(|language| skills.oral_languages.direct(language).max(0.0))
        .sum::<f32>();
    let written_effective = WrittenLanguage::ALL
        .into_iter()
        .map(|language| skills.written_languages.effective(language))
        .fold(0.0, f32::max);
    let written_direct = WrittenLanguage::ALL
        .into_iter()
        .map(|language| skills.written_languages.direct(language).max(0.0))
        .sum::<f32>();
    html! {
        @for (family, effective, direct, kind) in [("Oral",oral_effective,oral_direct,"oral"),("Written",written_effective,written_direct,"written")] {
            @if effective.is_finite() && effective > 0.0 {
                tr class=(format!("party-skill-row language-primary-row language-{kind}")) {
                    th scope="row" class="party-skill-name party-skill-icon-cell" { span class=(format!("language-monogram language-{kind}")) title=(format!("{family} languages")) aria-hidden="true" { (if kind=="oral" {"O"} else {"W"}) } span class="sr-only" { (family) } }
                    td class="party-skill-meter" colspan=[schedule_context.then_some("7")] { (skill_rank_bar((effective/1000.0).clamp(0.0,5.0),(effective/1000.0).clamp(0.0,5.0),&format!("{effective:.1} effective hours; {direct:.1} directly studied hours across {family} languages"),skill_rail_bar_options())) }
                    td class="religion-expand-cell" { button type="button" class="religion-expand-button" data-language-expand=(kind) aria-expanded="false" aria-label=(format!("Expand {family} languages")) { span class="religion-expand-chevron" aria-hidden="true" { "›" } } }
                }
                @if kind=="oral" { @for language in OralLanguage::ALL { @let descriptor=language.descriptor(); @let effective=skills.oral_languages.effective(language);
                    @if effective.is_finite() && effective > 0.0 {
                        tr class="party-skill-row language-detail-row" data-language-detail="oral" hidden { th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" { span class=(if descriptor.germanic_style {"language-monogram language-oral language-blackletter"} else {"language-monogram language-oral"}) title=(format!("{} — {}",descriptor.english,descriptor.native)) aria-hidden="true" { (descriptor.monogram) } span class="sr-only" { (descriptor.english) } } td class="party-skill-meter" colspan=[schedule_context.then_some("7")] { @let direct=skills.oral_languages.direct(language).max(0.0); (skill_rank_bar((effective/1000.0).clamp(0.0,5.0),(effective/1000.0).clamp(0.0,5.0),&format!("{effective:.1} effective hours; {direct:.1} directly studied hours"),skill_rail_bar_options())) } td class="religion-expand-cell" {} }
                    }
                }} @else { @for language in WrittenLanguage::ALL { @let descriptor=language.descriptor(); @let effective=skills.written_languages.effective(language);
                    @if effective.is_finite() && effective > 0.0 {
                        tr class="party-skill-row language-detail-row" data-language-detail="written" hidden { th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" { span class=(if descriptor.germanic_style {"language-monogram language-written language-blackletter"} else {"language-monogram language-written"}) title=(format!("{} — {}",descriptor.english,descriptor.native)) aria-hidden="true" { (descriptor.monogram) } span class="sr-only" { (descriptor.english) } } td class="party-skill-meter" colspan=[schedule_context.then_some("7")] { @let direct=skills.written_languages.direct(language).max(0.0); (skill_rank_bar((effective/1000.0).clamp(0.0,5.0),(effective/1000.0).clamp(0.0,5.0),&format!("{effective:.1} effective hours; {direct:.1} directly studied hours"),skill_rail_bar_options())) } td class="religion-expand-cell" {} }
                    }
                }}
            }
        }
    }
}

fn religion_skill_rows(
    skills: &CharacterSkills,
    health: f32,
    schedule: Option<&CharacterTrainingSchedule>,
    training_religion: Option<OfficialReligion>,
) -> Markup {
    if !OfficialReligion::ALL.into_iter().any(|religion| {
        let direct = skills.religion_hours.direct(religion);
        direct.is_finite() && direct > 0.0
    }) {
        return html! {};
    }
    let primary = training_religion.unwrap_or_else(|| {
        OfficialReligion::ALL
            .into_iter()
            .max_by(|left, right| {
                skills
                    .religion_hours
                    .effective(*left)
                    .total_cmp(&skills.religion_hours.effective(*right))
            })
            .unwrap_or(OfficialReligion::RomanCatholic)
    });
    let primary_id = primary.religion_id();
    let primary_effective = skills.religion_hours.effective(primary);
    let primary_direct = skills.religion_hours.direct(primary);
    let has_details = OfficialReligion::ALL.into_iter().any(|religion| {
        let direct = skills.religion_hours.direct(religion);
        religion != primary && direct.is_finite() && direct > 0.0
    });
    html! {
        tr class="party-skill-row religion-primary-row" data-religion-primary=(primary_id) {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                span class="religion-tradition-icon" title=(primary.label()) {
                    (religion_icon(primary.label(), Some(primary_id), false))
                }
            }
            td class="party-skill-meter" colspan=[schedule.map(|_| "7")] {
                (skill_rank_bar(
                    Skill::Religion.training_rank(primary_effective),
                    Skill::Religion.training_rank(primary_effective) * health.clamp(0.0, 1.0),
                    &format!("{primary_effective:.1} effective hours; {primary_direct:.1} directly studied hours"),
                    skill_rail_bar_options(),
                ))
            }
            td class="religion-expand-cell" {
                @if has_details {
                    (religion_expand_button(primary))
                }
            }
        }
        @for religion in OfficialReligion::ALL {
          @let direct = skills.religion_hours.direct(religion);
          @if religion != primary && direct.is_finite() && direct > 0.0 {
            @let id = religion.religion_id();
            @let effective = skills.religion_hours.effective(religion);
            tr class="party-skill-row religion-detail-row" data-religion-detail hidden {
                th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" {
                    span class="religion-tradition-icon" {
                        (religion_icon(religion.label(), Some(id), false))
                    }
                }
                td class="party-skill-meter" colspan=[schedule.map(|_| "7")] {
                    (skill_rank_bar(
                        Skill::Religion.training_rank(effective),
                        Skill::Religion.training_rank(effective) * health.clamp(0.0, 1.0),
                        &format!("{effective:.1} effective hours; {direct:.1} directly studied hours"),
                        skill_rail_bar_options(),
                    ))
                }
                td class="religion-expand-cell" {}
            }
          }
        }
    }
}

fn social_skill_rows(
    skills: &CharacterSkills,
    health: f32,
    schedule: Option<&CharacterTrainingSchedule>,
) -> Markup {
    let entries = [
        ("Insight", "insight", Skill::Insight, skills.insight_hours),
        (
            "Self-awareness",
            "self-awareness",
            Skill::SelfAwareness,
            skills.self_awareness_hours,
        ),
        ("Humor", "humor", Skill::Humor, skills.humor_hours),
        ("Command", "command", Skill::Command, skills.command_hours),
        (
            "Deception",
            "deception",
            Skill::Deception,
            skills.deception_hours,
        ),
        (
            "Seduction",
            "seduction",
            Skill::Seduction,
            skills.seduction_hours,
        ),
    ];
    if entries.iter().all(|entry| entry.3 <= 0.0) {
        return html! {};
    }
    let rank = entries
        .iter()
        .map(|entry| entry.2.training_rank(entry.3))
        .sum::<f32>()
        / entries.len() as f32;
    let effective_rank = rank * health.clamp(0.0, 1.0);
    html! {
        tr class="party-skill-row social-primary-row" data-social-primary {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                (stat_icon("Social", "skills", "social", false))
            }
            td class="party-skill-meter" colspan=[schedule.map(|_| "7")] {
                (skill_rank_bar(rank, effective_rank, "Average of all six Social skills", skill_rail_bar_options()))
            }
            td class="religion-expand-cell" {
                button type="button" class="religion-expand-button" data-social-expand
                    aria-expanded="false" aria-label="Expand Social skills" title="Expand Social" {
                    span class="religion-expand-chevron" aria-hidden="true" { "›" }
                }
            }
        }
        @for (name, icon, skill, hours) in entries {
            tr class="party-skill-row social-detail-row" data-social-detail hidden {
                th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" {
                    (stat_icon(name, "skills", icon, false))
                }
                td class="party-skill-meter" colspan=[schedule.map(|_| "7")] {
                    @let sub_rank = skill.training_rank(hours);
                    (skill_rank_bar(sub_rank, sub_rank * health.clamp(0.0, 1.0), &format!("{:.0} hours invested", hours.max(0.0)), skill_rail_bar_options()))
                }
                td class="religion-expand-cell" {}
            }
        }
    }
}

fn combat_skill_rows(
    skills: &CharacterSkills,
    head_health: f32,
    upper_health: f32,
    lower_health: f32,
    schedule: Option<&CharacterTrainingSchedule>,
    profile: CombatTrainingProfile,
) -> Markup {
    let weights = profile.weights();
    html! {
        (combat_meta_group("Melee", "crossed-swords", schedule, &[
            ("Polearm", "spear-hook", Skill::Polearm, skills.polearm_hours, upper_health, weights[0]),
            ("Axe", "battle-axe", Skill::Axe, skills.axe_hours, upper_health, weights[1]),
            ("Bludgeon", "flanged-mace", Skill::Bludgeon, skills.bludgeon_hours, upper_health, weights[2]),
            ("Sword", "sword", Skill::Sword, skills.sword_hours, upper_health, weights[3]),
            ("Knife", "bowie-knife", Skill::Knife, skills.knife_hours, upper_health, weights[4]),
        ]))
        (combat_meta_group("Ranged", "archery-target", schedule, &[
            ("Bow", "bow-arrow", Skill::Bow, skills.bow_hours, upper_health, weights[5]),
            ("Crossbow", "crossbow", Skill::Crossbow, skills.crossbow_hours, upper_health, weights[6]),
            ("Firearm", "musket", Skill::Firearm, skills.firearm_hours, upper_health, weights[7]),
            ("Throw", "throwing-ball", Skill::Throw, skills.throw_hours, upper_health, weights[8]),
        ]))
        (combat_meta_group("Defense", "shield", schedule, &[
            ("Dodge", "dodge", Skill::Dodge, skills.dodge_hours, lower_health, weights[9]),
            ("Block", "block", Skill::Block, skills.block_hours, upper_health, weights[10]),
            ("Balance", "balance", Skill::Balance, skills.balance_hours, lower_health, weights[11]),
            ("Will", "will", Skill::Will, skills.will_hours, head_health, weights[12]),
        ]))
    }
}

fn combat_meta_group(
    name: &str,
    icon: &str,
    schedule: Option<&CharacterTrainingSchedule>,
    entries: &[(&str, &str, Skill, f32, f32, f32)],
) -> Markup {
    let relevant: Vec<_> = entries.iter().filter(|entry| entry.5 > 0.0).collect();
    let rank = relevant
        .iter()
        .map(|entry| entry.2.training_rank(entry.3))
        .sum::<f32>()
        / relevant.len().max(1) as f32;
    let effective_rank = relevant
        .iter()
        .map(|entry| entry.2.training_rank(entry.3) * entry.4.clamp(0.0, 1.0))
        .sum::<f32>()
        / relevant.len().max(1) as f32;
    let included = relevant
        .iter()
        .map(|entry| entry.0)
        .collect::<Vec<_>>()
        .join(", ");
    html! {
        tr class="party-skill-row combat-primary-row" data-combat-primary=(name.to_ascii_lowercase()) {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                (stat_icon(name, "skills", icon, false))
            }
            td class="party-skill-meter" colspan=[schedule.map(|_| "7")] {
                (skill_rank_bar(rank, effective_rank, &format!("Relevant skills: {included}"), skill_rail_bar_options()))
            }
            td class="religion-expand-cell" {
                button type="button" class="religion-expand-button" data-combat-expand=(name.to_ascii_lowercase())
                    aria-expanded="false" aria-label=(format!("Expand {name} skills")) title=(format!("Expand {name}")) {
                    span class="religion-expand-chevron" aria-hidden="true" { "›" }
                }
            }
        }
        @for &(leaf_name, leaf_icon, skill, hours, health, weight) in entries {
            tr class="party-skill-row combat-detail-row" data-combat-detail=(name.to_ascii_lowercase()) data-combat-weight=(weight) hidden {
                th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" {
                    span title=[(skill == Skill::Knife).then_some("Knife means short weapons: knives, daggers, and short blades.")] {
                        (stat_icon(leaf_name, "skills", leaf_icon, false))
                    }
                }
                td class="party-skill-meter" colspan=[schedule.map(|_| "7")] {
                    @let sub_rank = skill.training_rank(hours);
                    (skill_rank_bar(sub_rank, sub_rank * health.clamp(0.0, 1.0), &format!("{:.0} hours invested", hours.max(0.0)), skill_rail_bar_options()))
                }
                td class="religion-expand-cell" {}
            }
        }
    }
}

fn religion_expand_button(primary: OfficialReligion) -> Markup {
    html! {
        button type="button" class="religion-expand-button" data-religion-expand
            aria-expanded="false"
            aria-label=(format!("Expand {} Religion skill", primary.label()))
            title=(format!("Expand {}", primary.label())) {
            span class="religion-expand-chevron" aria-hidden="true" { "›" }
        }
    }
}

fn schedule_header_icon(icon: &str, label: &str) -> Markup {
    html! { span class="schedule-header-icon" { (game_icon(label, icon)) } }
}

fn party_skill_row(
    name: &str,
    icon: &str,
    skill: Skill,
    hours: f32,
    health: f32,
    schedule_context: bool,
    action: Option<SkillAction<'_>>,
) -> Markup {
    let rank = skill.training_rank(hours);
    let effective_rank = rank * health.clamp(0.0, 1.0);
    let invested_hours = hours.max(0.0).floor() as u64;
    html! {
        tr class="party-skill-row" {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                @if let Some(action) = action {
                    (skill_action_icon(name, icon, action, schedule_context))
                } @else {
                    (stat_icon(name, "skills", icon, false))
                }
            }
            td class="party-skill-meter" colspan=[schedule_context.then_some("7")] {
                (skill_rank_bar(rank, effective_rank, &format!("{invested_hours} hours invested"), skill_rail_bar_options()))
            }
            td class="religion-expand-cell" {}
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SkillRankBarOptions<'a> {
    show_value: bool,
    extra_class: Option<&'a str>,
    aria_label: Option<&'a str>,
}

impl Default for SkillRankBarOptions<'_> {
    fn default() -> Self {
        Self {
            show_value: true,
            extra_class: None,
            aria_label: None,
        }
    }
}

fn skill_rail_bar_options() -> SkillRankBarOptions<'static> {
    SkillRankBarOptions {
        show_value: false,
        ..SkillRankBarOptions::default()
    }
}

fn skill_rank_bar(
    rank: f32,
    effective_rank: f32,
    title: &str,
    options: SkillRankBarOptions<'_>,
) -> Markup {
    let rank = rank.clamp(0.0, 5.0);
    let effective_rank = effective_rank.clamp(0.0, rank);
    let class = options.extra_class.map_or_else(
        || "skill-rank-bar".to_owned(),
        |extra| format!("skill-rank-bar {extra}"),
    );
    let aria_label = options
        .aria_label
        .map_or_else(|| format!("{effective_rank:.1} out of 5"), str::to_owned);
    html! {
        div class=(class) title=(title) aria-label=(aria_label)
            role="meter" aria-valuemin="0" aria-valuemax="5" aria-valuenow=(format!("{effective_rank:.1}")) {
            span class="skill-rank-track" aria-hidden="true" {
                @for tier in 1..=5 {
                    @let offset = (tier - 1) as f32;
                    @let current = (effective_rank - offset).clamp(0.0, 1.0) * 100.0;
                    @let trained = (rank - offset).clamp(0.0, 1.0) * 100.0;
                    @let damaged = (trained - current).max(0.0);
                    span class=(format!("skill-rank-segment skill-rank-segment-{tier}")) {
                        span class="rank-current" style=(format!("width:{current:.1}%")) {}
                        span class="rank-damage" style=(format!("left:{current:.1}%;width:{damaged:.1}%")) {}
                    }
                }
            }
            @if options.show_value {
                span class="skill-rank-value" aria-hidden="true" { (format!("{effective_rank:.1}")) }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ActivityEffectRates {
    gold_per_hour: f32,
    virtue_per_hour: f32,
    morale_per_hour: f32,
    fatigue_per_hour: f32,
    prayer_morale: bool,
    prayer_morale_multiplier: f32,
    morale_limit: f32,
    morale_scale_minutes: f32,
}

#[derive(Clone, Copy, Debug)]
struct LeisurePreview {
    current_fatigue: f32,
    outcome: LeisureOutcome,
    fatigue_display: f32,
}

fn core_daily_schedule(schedule: &ScheduleAllocation) -> DailySchedule {
    DailySchedule {
        combat_training_minutes: schedule.combat_training_minutes,
        carousing_minutes: schedule.carousing_minutes,
        apprenticeship_minutes: schedule.apprenticeship_minutes,
        apprenticeship_service_id: schedule
            .apprenticeship_service_id
            .as_deref()
            .and_then(adventuresim_core::profession::ProfessionId::from_service_id),
        profession_practice_minutes: schedule.profession_practice_minutes,
        profession_service_id: schedule
            .profession_service_id
            .as_deref()
            .and_then(adventuresim_core::profession::ProfessionId::from_service_id),
        labor: schedule.labor_minutes,
        prayer: schedule.prayer_minutes,
        thievery: schedule.thievery_minutes,
        raiding: schedule.raiding_minutes,
    }
}

fn leisure_preview(schedule: &ScheduleAllocation, current_fatigue: f32) -> LeisurePreview {
    let outcome = settlement_leisure_outcome(
        core_daily_schedule(schedule),
        MINUTES_PER_DAY,
        current_fatigue,
    );
    let labor_fatigue = f32::from(schedule.labor_minutes) / 60.0 * LABOR_FATIGUE_PER_HOUR;
    LeisurePreview {
        current_fatigue,
        outcome,
        fatigue_display: (outcome.fatigue_delta - labor_fatigue)
            / FATIGUE_RESERVOIR_PER_PREVIEW_POINT,
    }
}

impl ActivityEffectRates {
    const fn linear(gold: f32, virtue: f32, morale: f32, fatigue: f32) -> Self {
        Self {
            gold_per_hour: gold,
            virtue_per_hour: virtue,
            morale_per_hour: morale,
            fatigue_per_hour: fatigue,
            prayer_morale: false,
            prayer_morale_multiplier: 1.0,
            morale_limit: PRAYER_MORALE_LIMIT,
            morale_scale_minutes: PRAYER_MORALE_SCALE_MINUTES,
        }
    }

    fn prayer(multiplier: f32) -> Self {
        Self {
            prayer_morale: true,
            prayer_morale_multiplier: multiplier.clamp(0.0, 1.0),
            ..Self::linear(0.0, 0.0, 0.0, 0.0)
        }
    }

    const fn meditation() -> Self {
        Self {
            prayer_morale_multiplier: 0.25,
            prayer_morale: true,
            ..Self::linear(0.0, 0.0, 0.0, 0.0)
        }
    }

    const fn carousing() -> Self {
        Self {
            gold_per_hour: 0.0,
            virtue_per_hour: -0.125,
            prayer_morale: true,
            prayer_morale_multiplier: 1.0,
            morale_limit: adventuresim_core::activity::CAROUSING_MORALE_LIMIT,
            morale_scale_minutes: adventuresim_core::activity::CAROUSING_MORALE_SCALE_MINUTES,
            ..Self::linear(0.0, 0.0, 0.0, 0.0)
        }
    }

    fn values(self, minutes: u16) -> [f32; 4] {
        let hours = f32::from(minutes) / 60.0;
        let morale = if self.prayer_morale {
            self.prayer_morale_multiplier
                * self.morale_limit
                * (1.0 - (-f32::from(minutes) / self.morale_scale_minutes).exp())
        } else {
            self.morale_per_hour * hours
        };
        [
            (self.gold_per_hour * hours).round(),
            self.virtue_per_hour * hours,
            morale,
            self.fatigue_per_hour * hours,
        ]
    }
}

fn activity_effect_cell(kind: &str, value: f32) -> Markup {
    let rounded = if kind == "gold" {
        value.round()
    } else {
        (value * 10.0).round() / 10.0
    };
    let state = if rounded > 0.0 {
        "positive"
    } else if rounded < 0.0 {
        "negative"
    } else {
        "neutral"
    };
    let display = if state == "neutral" {
        "0".to_string()
    } else if kind == "gold" {
        format!("{rounded:+.0}")
    } else {
        format!("{rounded:+.1}")
    };
    html! {
        td class=(format!("schedule-effect schedule-effect-{state}")) data-activity-effect=(kind) {
            (display)
        }
    }
}

fn activity_training_cell(
    label: &str,
    allocation_name: &str,
    minutes: u16,
    profession: Option<&ProfessionActivityPreview>,
) -> Markup {
    let hours = f32::from(minutes) / 60.0;
    let rates: Vec<(String, f32)> = match allocation_name {
        "combat_training_minutes" => vec![("Relevant combat skills".into(), 1.0)],
        "carousing_minutes" => vec![("Humor".into(), 0.25)],
        "labor_minutes" => vec![("Will".into(), 0.25)],
        "thievery_minutes" => vec![("Stealth".into(), 0.25)],
        "raiding_minutes" => vec![("Relevant combat skills".into(), 0.25)],
        "prayer_minutes" if label == "Prayer" => vec![("Religion".into(), 0.25)],
        "apprenticeship_minutes" | "profession_practice_minutes" => profession
            .map(|preview| preview.training_rates.clone())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let breakdown = rates
        .iter()
        .map(|(skill, rate)| (skill.clone(), hours * rate))
        .collect::<Vec<_>>();
    let total = breakdown.iter().map(|(_, value)| value).sum::<f32>();
    let title = if breakdown.is_empty() {
        "No skill training".to_string()
    } else {
        breakdown
            .iter()
            .map(|(skill, value)| format!("{skill}: +{value:.2}h"))
            .collect::<Vec<_>>()
            .join("; ")
    };
    html! {
        td class="schedule-effect schedule-training-effect" data-activity-effect="training"
            data-training-rates=(rates.iter().map(|(skill, rate)| format!("{skill}={rate}")).collect::<Vec<_>>().join("|"))
            title=(title) aria-label=(format!("Effective skill training: {total:.2} hours")) {
            @if total > 0.0 { (format!("+{total:.2}h")) } @else { "—" }
        }
    }
}

fn schedule_special_row(
    label: &str,
    icon: &str,
    allocation_name: &str,
    allocation_minutes: u16,
    editable: bool,
    actionable: bool,
    effects: ActivityEffectRates,
    leisure: Option<LeisurePreview>,
    profession: Option<&ProfessionActivityPreview>,
    description: &str,
) -> Markup {
    let mut values = leisure.map_or_else(
        || effects.values(allocation_minutes),
        |preview| [0.0, 0.0, preview.outcome.morale, preview.fatigue_display],
    );
    if let Some(profession) = profession {
        let reward = profession.reward_delta(allocation_name, allocation_minutes);
        values[0] = reward[0];
        values[1] = reward[1];
    }
    html! {
        tr class="party-skill-row schedule-special-row" title=(description)
            data-activity-row data-activity-allocation=(allocation_name)
            data-gold-rate=(effects.gold_per_hour)
            data-virtue-rate=(effects.virtue_per_hour)
            data-morale-rate=(effects.morale_per_hour)
            data-fatigue-rate=(effects.fatigue_per_hour)
            data-prayer-morale=[effects.prayer_morale.then_some("true")]
            data-prayer-morale-limit=[effects.prayer_morale.then_some(effects.morale_limit)]
            data-prayer-morale-scale=[effects.prayer_morale.then_some(effects.morale_scale_minutes)]
            data-prayer-morale-multiplier=[effects.prayer_morale.then_some(effects.prayer_morale_multiplier)]
            data-profession-accrued=[profession.map(|preview| if allocation_name == "apprenticeship_minutes" { preview.apprenticeship_accrued } else { preview.practice_accrued })]
            data-profession-threshold=[profession.map(|preview| if allocation_name == "apprenticeship_minutes" { APPRENTICESHIP_REWARD_THRESHOLD } else { preview.practice_threshold })]
            data-profession-reward=[profession.map(|preview| if allocation_name == "apprenticeship_minutes" { "gold" } else { preview.practice_reward })]
            data-profession-sign=[profession.map(|_| if allocation_name == "apprenticeship_minutes" { -1 } else { 1 })]
            data-profession-tier=[profession.map(|preview| preview.tier_label)]
            data-leisure-current-fatigue=[leisure.map(|preview| preview.current_fatigue)]
            data-leisure-baseline-fatigue=[leisure.map(|_| BASELINE_FATIGUE_PER_DAY)]
            data-leisure-labor-fatigue-rate=[leisure.map(|_| LABOR_FATIGUE_PER_HOUR)]
            data-leisure-recovery-rate=[leisure.map(|_| LEISURE_FATIGUE_RECOVERY_PER_HOUR)]
            data-leisure-morale-limit=[leisure.map(|_| LEISURE_MORALE_LIMIT)]
            data-leisure-morale-scale=[leisure.map(|_| LEISURE_MORALE_SCALE_FATIGUE)]
            data-leisure-fatigue-preview-divisor=[leisure.map(|_| FATIGUE_RESERVOIR_PER_PREVIEW_POINT)] {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                (schedule_icon(label, icon, actionable, allocation_name))
                span class="sr-only" { (label) }
            }
            (activity_effect_cell("gold", values[0]))
            (activity_effect_cell("virtue", values[1]))
            (activity_effect_cell("morale", values[2]))
            (activity_effect_cell("fatigue", values[3]))
            (activity_training_cell(label, allocation_name, allocation_minutes, profession))
            td class="religion-auto-toggle-cell" {}
            (schedule_allocation_cell(allocation_name, allocation_minutes, editable))
            td class="religion-expand-cell" {}
        }
    }
}

fn schedule_service_selection(name: &str, service_id: &str) -> Markup {
    html! {
        tr hidden aria-hidden="true" {
            td colspan="9" { input type="hidden" name=(name) value=(service_id); }
        }
    }
}

fn profession_label(service_id: &str) -> &'static str {
    adventuresim_core::profession::profession_for_service(service_id)
        .map_or("profession", |profession| profession.label)
}

fn schedule_allocation_cell(name: &str, minutes: u16, editable: bool) -> Markup {
    html! {
        td class="party-skill-allocation" data-schedule-value=(name) {
            @if editable {
                input type="hidden" name=(name) value=(minutes) data-schedule-input;
                span data-schedule-display tabindex="0" role="button" title="Click to enter a time such as 8, 8:30, or 830" {
                    (format_schedule_hours(minutes))
                }
            } @else {
                span data-schedule-display { "0h" }
            }
        }
    }
}

fn schedule_icon(label: &str, icon: &str, actionable: bool, activity: &str) -> Markup {
    html! {
        @if actionable {
            button type="button" class="schedule-activity-button" data-activity-open=(activity)
                aria-label=(format!("Perform {label} now")) title=(format!("Perform {label} now"))
                aria-haspopup="dialog" aria-expanded="false" {
                span class="stat-icon schedule-special-icon"
                    style=(format!("--stat-icon: url('/static/icons/game/{icon}.svg')"))
                    aria-hidden="true" {}
            }
        } @else {
            span class="stat-icon schedule-special-icon"
                style=(format!("--stat-icon: url('/static/icons/game/{icon}.svg')"))
                aria-hidden="true" {}
        }
    }
}

fn immediate_activity_dialog(action: &str) -> Markup {
    html! {
        div class="activity-modal" data-activity-modal hidden {
            button type="button" class="activity-modal-backdrop" data-activity-close
                aria-label="Close activity dialog" {}
            form class="activity-modal-panel" action=(action) method="post" role="dialog"
                aria-modal="true" aria-labelledby="activity-modal-title" tabindex="-1"
                data-activity-form {
                header class="activity-modal-header" {
                    h3 id="activity-modal-title" data-activity-title { "Perform activity" }
                    button type="button" class="activity-modal-close" data-activity-close
                        aria-label="Close activity dialog" { "x" }
                }
                input type="hidden" name="activity" data-activity-kind;
                input type="hidden" name="service_id" data-activity-service;
                input type="hidden" name="requested_minutes" value="60" data-activity-minutes;
                div class="activity-duration-control" {
                    label for="immediate-activity-duration" { "Duration" }
                    input id="immediate-activity-duration" type="range" min="1" max="24"
                        step="1" value="1" data-activity-duration;
                    p class="activity-duration-summary" aria-live="polite" data-activity-duration-summary {
                        span data-activity-end { "Ends at --:--" }
                        span aria-hidden="true" { " / " }
                        span data-activity-hours { "1 h spent" }
                    }
                }
                table class="party-skills-table activity-preview-table" aria-label="Activity result preview" {
                    thead { tr {
                        th scope="col" { "Activity" }
                        th scope="col" { (schedule_header_icon("coins", "Currency")) }
                        th scope="col" { (schedule_header_icon("scales", "Virtue")) }
                        th scope="col" { (schedule_header_icon("sun", "Morale")) }
                        th scope="col" { (schedule_header_icon("night-sleep", "Fatigue")) }
                        th scope="col" { (schedule_header_icon("open-book", "Skill-hours")) }
                    } }
                    tbody { tr class="party-skill-row schedule-special-row" data-activity-preview-row {
                        th scope="row" data-activity-preview-label { "Activity" }
                        @for kind in ["gold", "virtue", "morale", "fatigue"] {
                            td class="schedule-effect schedule-effect-neutral" data-activity-effect=(kind) { "0" }
                        }
                        td class="schedule-effect schedule-training-effect" data-activity-effect="training" { "--" }
                    } }
                }
                button type="submit" class="activity-submit" data-activity-submit { "Spend 1 hour" }
            }
        }
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
    medical: &MedicalPresentation,
) -> Markup {
    html! {
        (character_summary_rail(capability))
        (party_attributes_rail(&format!("{}'s attributes", character.name), attributes, limbs, medical, None, &[], &[]))
        (party_skills_rail(
            &format!("{}'s skills", character.name), skills, limbs, None, None, None,
            false, 0.0, None, CombatTrainingProfile::default(), CharacterSkillActions::default(),
        ))
        (medical_rail(medical, "", 0, character.id, false))
    }
}

pub(crate) fn character_visual_preview(character: &Character) -> Markup {
    visual_stage("character", &character.name, "Adventurer profile")
}

fn religion_name(religion_id: Option<&str>) -> &'static str {
    match religion_id {
        Some("western_church") => "Western Church",
        Some("roman_catholic") => "Roman Catholic",
        Some("lutheran") => "Lutheran",
        Some("reformed") => "Reformed",
        Some("anglican") => "Anglican",
        Some("eastern_orthodox") => "Eastern Orthodox",
        Some("islamic") => "Islamic",
        Some("judaism") => "Jewish",
        Some("old_faith") => "Old Faith",
        Some(_) => "Unknown faith",
        None => "None",
    }
}

fn character_bio_rail(
    character: &Character,
    religion_id: Option<&str>,
    notoriety: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    can_renounce: bool,
    location_path: &str,
) -> Markup {
    let virtue = if notoriety.abs() < 0.0005 {
        0.0
    } else {
        -notoriety
    };
    html! {
        (sidebar_section("Bio", html! {
            dl class="character-bio" {
                div { dt class="metric-label" { (decorative_game_icon("calendar")) span { "Age" } } dd { (character.age_years) " years" } }
                div { dt class="metric-label" { (decorative_game_icon("spiked-halo")) span { "Virtue" } } dd title="Immoral activities reduce Virtue; consequences will be added later." { (format!("{virtue:+.1}")) } }
                @if let Some(personality) = personality {
                    @let tags = personality_tags(personality);
                    @if !tags.is_empty() {
                        div { dt { "Personality" } dd class="personality-tags" {
                            @for (name, description) in tags {
                                span class="personality-tag" title=(description) { (name) }
                            }
                        } }
                    }
                }
                div class="character-religion" {
                    dt class="metric-label" { (decorative_game_icon("holy-symbol")) span { "Religion" } }
                    dd {
                        (religion_name(religion_id))
                        @if can_renounce && religion_id.is_some() {
                            form method="post" action=(format!("{location_path}/party/{}/religion/renounce", character.id)) class="character-religion-action" {
                                button type="submit" class="btn btn-danger" title="Renounce this faith" { "Renounce" }
                            }
                        }
                    }
                }
            }
        }))
    }
}

fn personality_tags(
    personality: &crate::spacetimedb::CharacterPersonality,
) -> Vec<(&'static str, &'static str)> {
    use crate::spacetimedb::{
        Conscience::*, Conviction::*, Drive::*, Hygiene::*, Nerve::*, Outlook::*, SelfRegard::*,
        Sociability::*, Temperance::*,
    };
    let mut tags = Vec::new();
    match personality.nerve {
        Brave => tags.push(("Brave", "Morale loss from being outmatched ×0.5.")),
        Fearful => tags.push(("Fearful", "Morale loss from being outmatched ×2.")),
        _ => {}
    }
    match personality.drive {
        Ambitious => tags.push(("Ambitious", "Morale from victories and defeats ×1.5.")),
        Content => tags.push(("Content", "Morale from victories and defeats ×0.5.")),
        _ => {}
    }
    match personality.outlook {
        Sanguine => tags.push((
            "Sanguine",
            "Positive morale ×1.25; negative morale ×0.75; negative-event duration ×0.5.",
        )),
        Brooding => tags.push((
            "Brooding",
            "Positive morale ×0.75; negative morale ×1.25; negative-event duration ×2.",
        )),
        _ => {}
    }
    match personality.sociability {
        Gregarious => tags.push(("Gregarious", "Morale restored by allies ×1.5.")),
        Solitary => tags.push(("Solitary", "Morale restored by allies ×0.5.")),
        _ => {}
    }
    match personality.conscience {
        Compassionate => tags.push((
            "Compassionate",
            "Current morale effect ×1.0: no outcomes carry moral context yet.",
        )),
        Callous => tags.push((
            "Callous",
            "Current morale effect ×1.0: no outcomes carry moral context yet.",
        )),
        Cruel => tags.push((
            "Cruel",
            "Current morale effect ×1.0: no outcomes carry moral context yet.",
        )),
        _ => {}
    }
    match personality.self_regard {
        Proud => tags.push(("Proud", "Morale from victory ×1.5; morale from defeat ×3.")),
        Humble => tags.push(("Humble", "Morale from victories and defeats ×0.75.")),
        _ => {}
    }
    match personality.conviction {
        Zealous => tags.push(("Zealous", "Morale from religious sources and events ×1.5.")),
        Irreverent => tags.push((
            "Irreverent",
            "Morale from religious sources and events ×0.5.",
        )),
        _ => {}
    }
    match personality.hygiene {
        Slovenly => tags.push(("Slovenly", "Filth morale penalty ×0.")),
        Cleanly => tags.push((
            "Cleanly",
            "Filth morale penalty ×2.5; +2 morale while completely clean.",
        )),
        _ => {}
    }
    match personality.temperance {
        Temperate => tags.push((
            "Temperate",
            "Automatic alcohol morale bonus +0; missed-drink morale penalty -0.",
        )),
        Drunkard => tags.push((
            "Drunkard",
            "Wants a heavy drink every evening: +5 morale when satisfied, -5 when missed.",
        )),
        _ => {}
    }
    tags
}

#[cfg(test)]
mod personality_tests {
    use super::*;
    use crate::spacetimedb::*;

    #[test]
    fn neutral_axes_are_omitted_from_bio_tags() {
        let personality = CharacterPersonality {
            character_id: 1,
            nerve: Nerve::Brave,
            drive: Drive::Neutral,
            outlook: Outlook::Neutral,
            sociability: Sociability::Neutral,
            conscience: Conscience::Cruel,
            self_regard: SelfRegard::Neutral,
            conviction: Conviction::Neutral,
            hygiene: Hygiene::Neutral,
            temperance: Temperance::Neutral,
        };
        let tags = personality_tags(&personality);
        assert_eq!(
            tags.iter().map(|tag| tag.0).collect::<Vec<_>>(),
            ["Brave", "Cruel"]
        );
    }

    #[test]
    fn every_visible_tag_explains_its_numeric_morale_effect() {
        let profiles = [
            CharacterPersonality {
                character_id: 1,
                nerve: Nerve::Brave,
                drive: Drive::Ambitious,
                outlook: Outlook::Sanguine,
                sociability: Sociability::Gregarious,
                conscience: Conscience::Compassionate,
                self_regard: SelfRegard::Proud,
                conviction: Conviction::Zealous,
                hygiene: Hygiene::Cleanly,
                temperance: Temperance::Temperate,
            },
            CharacterPersonality {
                character_id: 2,
                nerve: Nerve::Fearful,
                drive: Drive::Content,
                outlook: Outlook::Brooding,
                sociability: Sociability::Solitary,
                conscience: Conscience::Callous,
                self_regard: SelfRegard::Humble,
                conviction: Conviction::Irreverent,
                hygiene: Hygiene::Slovenly,
                temperance: Temperance::Drunkard,
            },
            CharacterPersonality {
                character_id: 3,
                nerve: Nerve::Neutral,
                drive: Drive::Neutral,
                outlook: Outlook::Neutral,
                sociability: Sociability::Neutral,
                conscience: Conscience::Cruel,
                self_regard: SelfRegard::Neutral,
                conviction: Conviction::Neutral,
                hygiene: Hygiene::Neutral,
                temperance: Temperance::Neutral,
            },
        ];

        for profile in &profiles {
            for (tag, description) in personality_tags(profile) {
                assert!(
                    description
                        .chars()
                        .any(|character| character.is_ascii_digit()),
                    "{tag} tooltip lacks a numeric morale effect: {description}"
                );
            }
        }
    }
}

fn strategic_condition_rail(
    condition: Option<&CharacterStrategicCondition>,
    _morale_sources: &[crate::spacetimedb::CharacterMoraleSource],
    filth: &[crate::spacetimedb::CharacterFilth],
    social_href: &str,
    social_open: bool,
) -> Markup {
    let Some(condition) = condition else {
        return html! {};
    };
    let percent = |value: f32| format!("{:.0}%", value.max(0.0) * 100.0);
    let fear_fill = (condition.fear.clamp(0.0, 1.0) * 100.0).round();
    let bonus_fill = if condition.morale_bonus_cap > 0.0 {
        (condition.morale_bonus / condition.morale_bonus_cap * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    }
    .round();
    let meter_style = format!("--morale-fear: {fear_fill}%; --morale-bonus: {bonus_fill}%");
    let incapacitation_segments = [
        ("Pain", "broken-heart", "pain", condition.pain),
        (
            "Blood loss",
            "bleeding-wound",
            "blood",
            condition.blood_loss,
        ),
        ("Fear", "terror", "fear", condition.fear),
        ("Fatigue", "night-sleep", "fatigue", condition.fatigue),
        ("Hunger", "meal", "hunger", condition.hunger),
        ("Thirst", "water-drop", "thirst", condition.thirst),
    ];
    let incapacitation_sources = [
        ("Pain", "broken-heart", "pain", condition.pain),
        (
            "Blood loss",
            "bleeding-wound",
            "blood",
            condition.blood_loss,
        ),
        ("Fear", "terror", "fear", condition.fear),
        ("Fatigue", "night-sleep", "fatigue", condition.fatigue),
    ];
    html! {
        (sidebar_section("Status", html! {
            div class=(if condition.fear > 0.0 { "morale-meter is-fearful" } else { "morale-meter" }) style=(meter_style) role="meter" aria-valuemin="-5" aria-valuemax="5" aria-valuenow=(format!("{:.1}", condition.morale)) aria-label=(format!(
                "Morale {:.1}; fear {}; inspiration {:.1}%",
                condition.morale,
                percent(condition.fear),
                condition.morale_bonus * 100.0,
            )) {
                div class="morale-meter-heading" {
                    strong class="metric-label" { (decorative_game_icon("sun")) span { "Morale" } }
                    span class="morale-meter-value" { (format!("{:+.1}", condition.morale)) }
                    a class=(if social_open { "character-menu-button is-open" } else { "character-menu-button" })
                        href=(social_href) title="Open social menu" aria-label="Open social menu"
                        aria-haspopup="dialog" aria-expanded=(social_open) {
                        span class="stat-icon" style="--stat-icon: url('/static/icons/game/social.svg')" aria-hidden="true" {}
                        @if social_open { span class="sr-only" { " (open)" } }
                    }
                }
                div class="morale-meter-track" aria-hidden="true" {
                    span class="morale-meter-fear" {}
                    span class="morale-meter-neutral" {}
                    span class="morale-meter-bonus" {}
                }
                div class="morale-meter-labels" {
                    span { "100% fear" }
                    span { "Neutral" }
                    span { (format!("{:.1}% inspiration", condition.morale_bonus * 100.0)) }
                }
            }
            div class="fervor-meter" tabindex="0" style=(format!("--fervor: {:.0}%", condition.fervor.clamp(0.0, 1.0) * 100.0)) aria-label=(format!("Fervor {}", percent(condition.fervor))) {
                div class="fervor-meter-heading" {
                    strong class="metric-label" { (decorative_game_icon("holy-symbol")) span { "Fervor" } }
                    span { (percent(condition.fervor)) }
                }
                div class="fervor-meter-track" aria-hidden="true" { span {} }
                div class="fervor-meter-labels" {
                    span { "Calm" }
                    span { "Fervent" }
                    span { "Frenzy" }
                }
                p class="fervor-help" role="tooltip" {
                    "Personality Conviction, a strong same-profession cohort, and surplus morale raise Fervor. Party Command restrains it. Characters without a professed religion have no Fervor."
                }
            }
            div class="incapacitation-overview" tabindex="0" title=(format!("{} incapacitation", percent(condition.incapacitation))) {
                div class="incapacitation-heading" {
                    strong class="metric-label" { (decorative_game_icon("coma")) span { "Incapacitation" } }
                    span class="incapacitation-status" { (&condition.status) }
                }
                div class="incapacitation-total-track" role="meter"
                    aria-label=(format!("Incapacitation {}; {}", percent(condition.incapacitation), condition.status))
                    aria-valuemin="0" aria-valuemax="100"
                    aria-valuenow=(condition.incapacitation.clamp(0.0, 1.0) * 100.0) {
                    @for (_, _, color, value) in incapacitation_segments {
                        span class=(format!("incapacitation-segment incapacitation-{color}"))
                            style=(format!("--incap-amount: {:.1}%", value.max(0.0) * 100.0)) {}
                    }
                }
            }
            div class="incapacitation-sources" aria-label="Sources of incapacitation" {
                @for (label, icon, color, value) in incapacitation_sources {
                    div class=(format!("incapacitation-source incapacitation-{color}"))
                        title=(format!("{label}: {} incapacitation", percent(value))) {
                        strong class="metric-label" { (decorative_game_icon(icon)) span { (label) } }
                        div class="incapacitation-source-track" role="meter"
                            aria-label=(format!("{label}: {} incapacitation", percent(value)))
                            aria-valuemin="0" aria-valuemax="100"
                            aria-valuenow=(value.clamp(0.0, 1.0) * 100.0) {
                            span style=(format!("--incap-amount: {:.1}%", value.clamp(0.0, 1.0) * 100.0)) {}
                        }
                    }
                }
            }
            div class="need-balance-meters" aria-label="Food and water reserves" {
                (need_balance_meter("Food", "meal", "Hunger", "Full", "hunger", condition.food_days, condition.hunger))
                (need_balance_meter("Water", "water-drop", "Thirst", "Hydrated", "thirst", condition.water_days, condition.thirst))
            }
            (filth_status_bar(filth))
        }))
    }
}

fn need_balance_meter(
    label: &str,
    icon: &str,
    deficit_label: &str,
    reserve_label: &str,
    color: &str,
    reserve_days: f32,
    incapacitation: f32,
) -> Markup {
    let reserve_days = reserve_days.max(0.0);
    let reserve_fill = (reserve_days * 100.0).clamp(0.0, 100.0);
    let deficit_fill = (incapacitation.max(0.0) * 100.0).clamp(0.0, 100.0);
    let signed_value = if deficit_fill > 0.0 {
        -deficit_fill
    } else {
        reserve_fill
    };
    let description = format!(
        "{label}: {reserve_days:.1} travel days reserve; {deficit_label} {deficit_fill:.0}% incapacitation"
    );
    html! {
        div class=(format!("need-balance incapacitation-{color}"))
            style=(format!("--need-reserve: {reserve_fill:.1}%; --need-deficit: {deficit_fill:.1}%"))
            title=(&description) {
            strong class="metric-label" { (decorative_game_icon(icon)) span { (label) } }
            div class="need-balance-track" role="meter" aria-label=(description)
                aria-valuemin="-100" aria-valuemax="100" aria-valuenow=(format!("{signed_value:.0}")) {
                span class="need-balance-half need-balance-deficit" { span {} }
                span class="need-balance-half need-balance-reserve" { span {} }
                i aria-hidden="true" {}
            }
            div class="need-balance-labels" aria-hidden="true" {
                span { (deficit_label) }
                span { "0" }
                span { (reserve_label) }
            }
        }
    }
}

fn medical_rail(
    medical: &MedicalPresentation,
    location_path: &str,
    doctor_id: u64,
    target_id: u64,
    _allow_treatment: bool,
) -> Markup {
    html! {
        (sidebar_section("Symptoms", html! {
            @if medical.unavailable {p class="text-muted small-copy" {"Medical examination unavailable."}} @else if medical.symptoms.is_empty(){p class="text-muted small-copy" { "No visible symptoms." }}@else{p class="medical-symptoms" {(medical.symptoms.join(" · "))}}
            @for medication in &medical.medications {
                p class="medical-treatment-status" {
                    "Taking medication for " (medication.disease_name) "."
                    @if doctor_id == target_id {
                        form method="post" action=(format!("{location_path}/party/{target_id}/medication/{}/unequip", medication.equipment_id)) {
                            button type="submit" class="medical-medication-remove" aria-label=(format!("Stop medication for {}", medication.disease_name)) title="Stop taking this medication; the course will be discarded" { "×" }
                        }
                    }
                }
            }
        }))
    }
}

fn medical_examination_popup(
    medical: &MedicalPresentation,
    location: &LocationView,
    target_id: u64,
    limbs: Option<&CharacterLimbs>,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
) -> Markup {
    let Some(examination_id) = medical.examination_id else {
        return html! {};
    };
    let dismiss_url = location.preserve_building(format!(
        "{}/party/{target_id}/examination/{examination_id}/dismiss",
        location.base_path()
    ));
    html! {
        div class="medical-examination-overlay" role="dialog" aria-modal="true" aria-labelledby="medical-examination-title"
            data-medical-examination
            data-dismiss-url=(&dismiss_url) {
            section class="medical-examination-popup" {
                header class="medical-examination-heading" {
                    div {
                        h2 id="medical-examination-title" { "Examination findings" }
                        @if let Some(examined_at) = medical.examined_at {
                            p class="text-muted small-copy" { "Observed at personal minute " (examined_at) "." }
                        }
                    }
                    form method="post" action=(&dismiss_url) {
                        button type="submit" class="medical-examination-close" aria-label="Close examination findings" { "×" }
                    }
                }
                @if medical.regional_humours.is_some() {
                    div class="examination-region-bars" aria-label="Examined body regions" {
                        h3 { "Body regions" }
                        @let health = regional_health_values(limbs);
                        @for (index, name) in ["Left arm", "Right arm", "Left leg", "Right leg", "Chest", "Stomach", "Head"].into_iter().enumerate() {
                            @let reading = medical.regional_humours.map(|regions| regions[index]).unwrap_or_default();
                            @if health[index] < 1.0 || reading.sanguine + reading.phlegmatic + reading.choleric + reading.melancholic > 0.0 {
                                div class="examination-region-row" {
                                    strong { (name) }
                                    (regional_health_bar(name, health[index], medical, index, injuries, projectiles))
                                }
                            }
                        }
                    }
                }
                @if !medical.findings.is_empty() {
                    h3 { "Observed signs" }
                    p class="medical-symptoms" { (medical.findings.join(" · ")) }
                }
                @if !medical.possible_diagnoses.is_empty() {
                    div class="medical-diagnoses" {
                        h3 { "Possible ailments" }
                        p class="small-copy" { "The findings do not permit a confident distinction." }
                        ul { @for possibility in &medical.possible_diagnoses { li { (possibility) } } }
                    }
                }
                @if !medical.diagnoses.is_empty() {
                    div class="medical-diagnoses" {
                        h3 { "Diagnosed conditions" }
                        @for diagnosis in &medical.diagnoses {
                            article {
                                strong { (diagnosis.period_name) }
                                span class="condition-stage" { " — " (diagnosis.stage) }
                                p class="small-copy" { (diagnosis.contagion) }
                            }
                        }
                    }
                }
                @if medical.findings.is_empty() && medical.possible_diagnoses.is_empty() && medical.diagnoses.is_empty() {
                    p class="text-muted" { "The examination did not reveal an identifiable internal cause." }
                }
            }
        }
    }
}

fn regional_health_values(limbs: Option<&CharacterLimbs>) -> [f32; 7] {
    limbs.map_or([1.0; 7], |limbs| {
        [
            limbs.left_arm_health,
            limbs.right_arm_health,
            limbs.left_leg_health,
            limbs.right_leg_health,
            limbs.chest_health,
            limbs.stomach_health,
            limbs.head_health,
        ]
    })
}

fn party_attributes_rail(
    title: &str,
    attributes: Option<&CharacterAttributes>,
    limbs: Option<&CharacterLimbs>,
    medical: &MedicalPresentation,
    surgery: Option<(&str, Option<&str>)>,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
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
                (attribute_group("Head", "head", head_health, medical, 6, surgery, injuries, projectiles, &[
                    ("Intelligence", "intelligence", attributes.intelligence),
                    ("Instinct", "instinct", attributes.instinct),
                    ("Eyesight", "eyesight", attributes.eyesight),
                    ("Hearing", "hearing", attributes.hearing),
                ]))
                (attribute_group("Chest", "chest", chest_health, medical, 4, surgery, injuries, projectiles, &[
                    ("Endurance", "endurance", attributes.endurance),
                ]))
                (attribute_group("Stomach", "stomach", stomach_health, medical, 5, surgery, injuries, projectiles, &[
                    ("Immunity", "immunity", attributes.immunity),
                    ("Gut", "gut", attributes.gut),
                ]))
                div class="limb-attribute-pair" {
                    (limb_attribute_column("Left arm", "left-arm", "limb-left", left_arm_health, medical, 0, surgery, injuries, projectiles, &[
                        ("Strength", "strength-arm", attributes.left_arm_strength),
                        ("Agility", "agility-arm", attributes.left_arm_agility),
                    ]))
                    (limb_attribute_column("Right arm", "right-arm", "limb-right", right_arm_health, medical, 1, surgery, injuries, projectiles, &[
                        ("Strength", "strength-arm", attributes.right_arm_strength),
                        ("Agility", "agility-arm", attributes.right_arm_agility),
                    ]))
                }
                div class="limb-attribute-pair" {
                    (limb_attribute_column("Left leg", "left-leg", "limb-left", left_leg_health, medical, 2, surgery, injuries, projectiles, &[
                        ("Strength", "strength-leg", attributes.left_leg_strength),
                        ("Agility", "agility-leg", attributes.left_leg_agility),
                    ]))
                    (limb_attribute_column("Right leg", "right-leg", "limb-right", right_leg_health, medical, 3, surgery, injuries, projectiles, &[
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
    slug: &str,
    side: &str,
    health: f32,
    medical: &MedicalPresentation,
    region: usize,
    surgery: Option<(&str, Option<&str>)>,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    rows: &[(&str, &str, f32)],
) -> Markup {
    attribute_group_with_labels(
        name,
        slug,
        health,
        medical,
        region,
        surgery,
        injuries,
        projectiles,
        rows,
        false,
        Some(side),
    )
}

fn attribute_group(
    name: &str,
    slug: &str,
    health: f32,
    medical: &MedicalPresentation,
    region: usize,
    surgery: Option<(&str, Option<&str>)>,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    rows: &[(&str, &str, f32)],
) -> Markup {
    attribute_group_with_labels(
        name,
        slug,
        health,
        medical,
        region,
        surgery,
        injuries,
        projectiles,
        rows,
        true,
        None,
    )
}

fn attribute_group_with_labels(
    name: &str,
    slug: &str,
    health: f32,
    medical: &MedicalPresentation,
    region: usize,
    surgery: Option<(&str, Option<&str>)>,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    rows: &[(&str, &str, f32)],
    show_labels: bool,
    side: Option<&str>,
) -> Markup {
    let health = health.clamp(0.0, 1.0);
    html! {
        div class=(match side {
            Some(side) => format!("attribute-group limb-attribute-column {side}"),
            None => "attribute-group".to_owned(),
        }) {
            div class="attribute-group-heading" {
                span { (name) }
                @if let Some((path_template, open_limb)) = surgery {
                    @let open = open_limb == Some(slug);
                    a class=(if open { "character-menu-button limb-surgery-button is-open" } else { "character-menu-button limb-surgery-button" })
                        href=(path_template.replace("__limb__", slug)) title=(format!("Treat {name}")) aria-label=(format!("Open surgery menu for {name}"))
                        aria-haspopup="dialog" aria-expanded=(open) {
                        span class="stat-icon" style="--stat-icon: url('/static/icons/game/scalpel.svg')" aria-hidden="true" {}
                        @if open { span class="sr-only" { " (open)" } }
                    }
                }
            }
            (regional_health_bar(name, health, medical, region, injuries, projectiles))
            @for (attribute_name, icon, value) in rows {
                (attribute_row(attribute_name, icon, *value, health, show_labels))
            }
        }
    }
}

fn regional_health_bar(
    name: &str,
    physical_health: f32,
    medical: &MedicalPresentation,
    region: usize,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
) -> Markup {
    let physical_health = physical_health.clamp(0.0, 1.0);
    let physical_damage = 1.0 - physical_health;
    let limb = [
        LimbRegion::LeftArm,
        LimbRegion::RightArm,
        LimbRegion::LeftLeg,
        LimbRegion::RightLeg,
        LimbRegion::Chest,
        LimbRegion::Stomach,
        LimbRegion::Head,
    ][region];
    let injury = injuries.iter().find(|injury| injury.limb == limb);
    let cut = injury
        .map_or(0.0, |row| row.cut_damage)
        .min(physical_damage);
    let total_blunt = injury
        .map_or(physical_damage - cut, |row| {
            row.bruise_damage.max(row.fracture_damage)
        })
        .min((physical_damage - cut).max(0.0));
    let fracture = injury
        .map_or(0.0, |row| row.fracture_damage)
        .min(total_blunt);
    let blunt = (total_blunt - fracture).max(0.0);
    let bandaged = injury.is_some_and(|row| row.bandaged);
    let splinted = injury.is_some_and(|row| row.splint_inventory_item_id.is_some());
    let fracture_label = if splinted {
        "splinted fracture"
    } else {
        "fracture"
    };
    let humour = medical.regional_humours.map(|values| values[region]);
    let values = humour.unwrap_or_default();
    let humour_total = if humour.is_some() {
        values.sanguine + values.phlegmatic + values.choleric + values.melancholic
    } else {
        medical.concealed_other[region]
    };
    let other = physical_health * humour_total.clamp(0.0, 1.0);
    let okay = (physical_health - other).max(0.0);
    let scale = if humour.is_some() && humour_total > 1.0 {
        other / humour_total
    } else {
        physical_health
    };
    let segments = if humour.is_some() {
        vec![
            (
                "Sanguine",
                "attribute-health-sanguine",
                values.sanguine * scale,
            ),
            (
                "Phlegmatic",
                "attribute-health-phlegmatic",
                values.phlegmatic * scale,
            ),
            (
                "Choleric",
                "attribute-health-choleric",
                values.choleric * scale,
            ),
            (
                "Melancholic",
                "attribute-health-melancholic",
                values.melancholic * scale,
            ),
        ]
    } else {
        vec![("Other impairment", "attribute-health-other", other)]
    };
    let reading = if humour.is_some() {
        format!(
            "{name}: {:.0}% sound, {:.0}% cut, {:.0}% blunt, {:.0}% {fracture_label}, {:.0}% sanguine, {:.0}% phlegmatic, {:.0}% choleric, {:.0}% melancholic impairment",
            okay * 100.0,
            cut * 100.0,
            blunt * 100.0,
            fracture * 100.0,
            values.sanguine * scale * 100.0,
            values.phlegmatic * scale * 100.0,
            values.choleric * scale * 100.0,
            values.melancholic * scale * 100.0,
        )
    } else {
        format!(
            "{name}: {:.0}% sound, {:.0}% cut, {:.0}% blunt, {:.0}% {fracture_label}, {:.0}% other impairment",
            okay * 100.0,
            cut * 100.0,
            blunt * 100.0,
            fracture * 100.0,
            other * 100.0,
        )
    };
    html! {
        div class="attribute-health-bar" role="meter"
            aria-label=(reading)
            aria-valuemin="0" aria-valuemax="100" aria-valuenow=(okay * 100.0) {
            span class="attribute-health-current" title="Sound" style=(format!("width:{:.1}%", okay * 100.0)) {}
            span class=(if bandaged { "attribute-health-cut bandaged-cut" } else { "attribute-health-cut" }) title=(if bandaged { "Bandaged cut damage" } else { "Cut damage" }) style=(format!("width:{:.1}%", cut * 100.0)) {}
            span class="attribute-health-blunt" title="Blunt damage" style=(format!("width:{:.1}%", blunt * 100.0)) {}
            span class=(if splinted { "attribute-health-fracture splinted-fracture" } else { "attribute-health-fracture" })
                title=(if splinted { "Splinted fracture" } else { "Fracture" })
                style=(format!("width:{:.1}%", fracture * 100.0)) {}
            @for (label, class, amount) in segments {
                @if amount > 0.0 {
                    span class=(class) title=(label) style=(format!("width:{:.1}%", amount * 100.0)) {}
                }
            }
            @for (projectile_index, projectile) in projectiles.iter().filter(|projectile| projectile.limb == limb).enumerate() {
                span class=(match projectile.kind { ProjectileKind::Arrowhead => "surgery-projectile-icon projectile-arrowhead", ProjectileKind::Ball => "surgery-projectile-icon projectile-ball" })
                    style=(format!("right:{:.2}rem", 0.2 + projectile_index as f32 * 0.75))
                    title=(match projectile.kind { ProjectileKind::Arrowhead => "Retained arrowhead", ProjectileKind::Ball => "Retained ball" }) aria-hidden="true" {}
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
            (stat_icon(name, "attributes", icon, show_label))
            @if show_label { span class="party-attribute-name" { (name) } }
            div class="attribute-rank-bar" title=(format!("{effective_value:.1}"))
                role="meter" aria-valuemin="0" aria-valuemax="5" aria-valuenow=(format!("{effective_value:.1}"))
                aria-label=(format!("{name}: {effective_value:.1} out of 5")) {
                span class="rank-current" style=(format!("width:{current_width:.1}%")) {}
                span class="rank-damage" style=(format!("left:{current_width:.1}%;width:{damage_width:.1}%")) {}
            }
            span class="attribute-rank-value" aria-hidden="true" { (format!("{effective_value:.1}")) }
        }
    }
}

fn stat_icon(label: &str, category: &str, icon: &str, decorative: bool) -> Markup {
    let path = stat_icon_path(category, icon);
    html! {
        span
            class=(format!("stat-icon stat-icon-{icon}"))
            style=(format!("--stat-icon: url('{path}')"))
            role=[(!decorative).then_some("img")]
            aria-label=[(!decorative).then_some(label)]
            title=[(!decorative).then_some(label)]
            aria-hidden=[decorative.then_some("true")]
        {}
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

    html! {
        @if !members.is_empty() {
            div class="party-portrait-overlay" aria-label="Active party" {
                div data-party-portrait-members {
                @if active_character.is_some() {
                    div class="party-portrait party-inventory-portrait" title="Party inventory" {
                        a class="party-portrait-select" href=(format!("{}/party-inventory", location_path)) {
                            span class="party-portrait-initial party-chest-face" { (game_icon("Party inventory", "knapsack")) }
                        }
                    }
                }
                @for member in members {
                    @let is_active = active_character.is_some_and(|character| character.id == member.id);
                    @let can_remove = Some(member.id) != leader_id;
                    div class=(format!("party-portrait{}{}", if selected_character_id == Some(member.id) { " active" } else { "" }, if !member.alive { " dead" } else { "" }))
                        data-character-id=(member.id)
                        data-character-alive=(member.alive)
                        data-active-character[is_active]
                        title=(&member.name) {
                        a class="party-portrait-select"
                            href=(if is_active {
                                format!("{}/party/{}", location_path, member.id)
                            } else {
                                format!("{}/party/{}/stats", location_path, member.id)
                            })
                            title=(format!("Inspect {}", member.name)) {
                            span class="party-portrait-initial" {
                                span class="party-portrait-face" { (member.name.chars().next().unwrap_or('?')) }
                                span class="party-portrait-name" { (&member.name) @if !member.alive { " (dead)" } }
                            }
                        }
                        @if member.alive && active_character.is_some_and(|character| character.alive) {
                        span class="party-portrait-actions" aria-label=(format!("Actions for {}", member.name)) {
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
                    }
                }
                }
            }
        }
    }
}

/// Shared chat panel. Local conversations are live; the remaining channel
/// filters are present so their messages can join the same stream as their
/// backends become available.
pub(crate) fn settlement_chat_area(location: &str, active_character: Option<&Character>) -> Markup {
    chat_area(location, active_character, None, None, &[])
}

pub(crate) fn settlement_chat_area_with_info(
    location: &str,
    active_character: Option<&Character>,
    info_messages: &[String],
) -> Markup {
    chat_area(location, active_character, None, None, info_messages)
}

fn player_chat_area(subject: &Character, active_character: &Character) -> Markup {
    let context = ("player", subject.id.to_string());
    chat_area(
        &subject.name,
        Some(active_character),
        None,
        Some(context),
        &[],
    )
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
        &[],
    )
}

fn chat_area(
    location: &str,
    _active_character: Option<&Character>,
    service_context: Option<(&str, &str)>,
    local_context: Option<(&str, String)>,
    info_messages: &[String],
) -> Markup {
    html! {
        section class="settlement-chat" aria-label="Settlement chat"
            data-service-quest-settlement=[service_context.map(|context| context.0)]
            data-service-quest-id=[service_context.map(|context| context.1)]
            data-dialogue-catalog-revision=[service_context.map(|_| adventuresim_dialogue::CATALOG_DIGEST)]
            data-herbalist-exam-fee=[service_context
                .filter(|context| context.1 == "herbalist")
                .map(|_| adventuresim_core::strategic_economy::NPC_HERBALIST_EXAM_FEE)]
            data-local-chat-kind=[local_context.as_ref().map(|context| context.0)]
            data-local-chat-subject=[local_context.as_ref().map(|context| context.1.as_str())] {
            div class="settlement-chat-resize" role="separator" aria-label="Resize chat"
                aria-orientation="horizontal" aria-valuemin="128" aria-valuemax="640"
                aria-valuenow="184" tabindex="0" title="Drag to resize chat" {
                span aria-hidden="true" {}
            }
            div class="settlement-chat-layout" {
                div class="settlement-chat-conversation" {
                    div class="settlement-chat-filters" role="group" aria-label="Visible chat channels" {
                        @for (channel, label, abbreviation) in [
                            ("local", "Local", "L"),
                            ("party", "Party", "P"),
                            ("settlement", "Settlement", "S"),
                            ("dm", "DMs", "D"),
                            ("guild", "Guild", "G"),
                            ("info", "Info", "I"),
                        ] {
                            label class=(format!("chat-channel-filter chat-channel-filter-{channel}")) title=(label) {
                                input type="checkbox" checked data-chat-filter=(channel)
                                    aria-label=(label) title=(label);
                                span aria-hidden="true" { (abbreviation) }
                            }
                        }
                    }
                    div class="settlement-chat-messages" aria-live="polite" {
                        @if local_context.is_none() { div class="chat-system-message" data-chat-channel="info" {
                            span class="chat-timestamp" { "[--:--] " }
                            " Select a local character or settlement service to begin talking."
                        } }
                        @for message in info_messages {
                            div class="chat-system-message" data-chat-channel="info" {
                                span class="chat-timestamp" { "[--:--] " }
                                (message)
                            }
                        }
                    }
                    div class="settlement-chat-composer" {
                        div class="settlement-chat-input-shell" {
                            span class="settlement-chat-completion" data-dialogue-completion aria-hidden="true" {}
                            input type="text" name="body" disabled[local_context.is_none()]
                                aria-label="Local message"
                                autocomplete="off"
                                placeholder=(format!("Message {location} (Local)"));
                        }
                        button type="button" class="btn btn-primary btn-icon" disabled[local_context.is_none()]
                            aria-label="Send message" {
                            (decorative_game_icon("plain-arrow"))
                        }
                    }
                }
                aside class="settlement-chat-topics" data-dialogue-topic-pane hidden
                    aria-label="Dialogue topics" {
                    h3 { "Topics" }
                    ul data-dialogue-topic-list {}
                }
            }
        }
    }
}

fn merchant_offers_rail(title: &str, unavailable_offers: &[&str]) -> Markup {
    html! {
        (sidebar_section(title, html! {
            ul class="service-offering-list" aria-label="Expected offerings" title="No stock is listed at present" {
                @for offer in unavailable_offers {
                    li { (decorative_game_icon("shop")) span { (offer) } }
                }
            }
        }))
    }
}

fn inventory_rail(
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    trade_action: Option<(&str, &str)>,
    _show_repair: bool,
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
                    (trade_inventory_table_header(false, None))
                    tbody {
                    @for item in inventory {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let item_name = item_display_name(&item.item_id);
                        tr class=(if trade_action.is_some() { "trade-inventory-row" } else { "trade-inventory-row inventory-row-readonly" }) {
                            td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                            td class="inventory-item-name" {
                                (item_name_with_quality(&item.item_id, definition))
                                @if let Some((action, tooltip)) = trade_action {
                                button type="button" class="trade-transfer trade-transfer-left" disabled
                                    aria-label=(format!("{action} {item_name}"))
                                    title=(tooltip) { "◀" }
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
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    at_inn: bool,
    summary: &RestSummary,
    soap_preview: SoapRestPreview,
) -> Markup {
    service_page(
        settlement,
        if at_inn { "inn" } else { "religion" },
        if at_inn { "The Inn" } else { "Church" },
        if at_inn { "Innkeeper" } else { "Priest" },
        "",
        active_character,
        inventory,
        items,
        party_members,
        logged_in_as,
        None,
        Some(summary),
        soap_preview,
    )
}

fn rest_service_menu(
    location: &str,
    settlement_id: &str,
    kind: &str,
    default_minutes: Option<u64>,
    summary: Option<&RestSummary>,
    soap_preview: SoapRestPreview,
) -> Markup {
    html! {
    section class="rest-service-menu" aria-label=(format!("{} rest service", location))
        title=(if kind == "inn" { "A bed costs 1 coin per day. Injuries are tended before downtime." } else { "Sanctuary is free. Injuries are tended before downtime." }) {
        div class="rest-service-heading" { strong { "Rest" } }
        @if kind == "inn" {
            p class="rest-service-copy" { "1 coin / day · treatment included" }
        } @else {
            p class="rest-service-copy" { "Free · treatment included" }
        }
        form action=(format!("/settlements/{settlement_id}/rest/{kind}")) method="post" {
                @let minutes = default_minutes.unwrap_or(0);
                @let unit = if minutes >= 1_440 { "days" } else { "hours" };
                @let initial_minutes = if minutes == 0 { 1_440 } else { minutes.max(1_440) };
                (settlement_rest_duration_control(initial_minutes, unit))
                /*
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
                */
                button type="submit" class="btn btn-primary btn-small btn-block" data-rest-submit disabled[unit == "hours"] title="Rest for the selected duration" {
                    (decorative_game_icon("night-sleep"))
                    span class="sr-only" { "Rest" }
                }
        }
        (soap_wash_preview(soap_preview))
        @if let Some(summary) = summary {
            div class="rest-summary-overlay" role="dialog" aria-modal="true" aria-labelledby="rest-summary-title" {
                section class="rest-summary" {
                    div class="rest-summary-heading" {
                        strong id="rest-summary-title" { "Rest summary" }
                        a href=(format!("/settlements/{settlement_id}/{}", if kind == "inn" { "inn" } else { "religion" })) class="rest-summary-close" aria-label="Close rest summary" { "×" }
                    }
                    p { (format_rest_duration(summary.minutes)) " passed." }
                    @if summary.gold_spent > 0 { p { (summary.gold_spent) " coin paid." } }
                    @if summary.gold_earned > 0 { p { (summary.gold_earned) " coin earned from activities." } }
                    @if summary.notoriety_gained > 0.0 { p class="schedule-effect-negative" { (format!("-{:.1}", summary.notoriety_gained)) " Virtue from activities." } }
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

fn settlement_rest_duration_control(initial_minutes: u64, unit: &str) -> Markup {
    wake_time_rest_duration_control("settlement-rest", initial_minutes, unit, 1_440, None, None)
}

fn wake_time_rest_duration_control(
    id_prefix: &str,
    initial_minutes: u64,
    unit: &str,
    minimum_minutes: u64,
    default_minutes: Option<u64>,
    scheduled_wake_minute: Option<u16>,
) -> Markup {
    let hours_active = unit == "hours";
    let wake_id = format!("{id_prefix}-wake-time");
    let value = if hours_active {
        format!("{:02}:{:02}", initial_minutes / 60, initial_minutes % 60)
    } else {
        initial_minutes.div_ceil(1_440).max(1).to_string()
    };
    html! {
        div class="rest-duration-control settlement-rest-duration" data-rest-duration data-wake-time
            data-rest-minimum-minutes=(minimum_minutes)
            data-rest-default-minutes=[default_minutes]
            data-rest-scheduled-wake-minute=[scheduled_wake_minute] {
            div class="rest-duration-units" role="radiogroup" aria-label="Rest duration" {
                label class=(if hours_active { "rest-duration-unit active" } else { "rest-duration-unit" }) {
                    input type="radio" name="unit" value="hours" checked[hours_active] {}
                    "Hours"
                }
                label class=(if !hours_active { "rest-duration-unit active" } else { "rest-duration-unit" }) {
                    input type="radio" name="unit" value="days" checked[!hours_active] {}
                    "Days"
                }
            }
            div class="rest-wake-time" data-wake-time-panel aria-disabled=(!hours_active) {
                div class="rest-wake-heading" {
                    label for=(&wake_id) { "Wake time" }
                    output for=(&wake_id) data-wake-time-output { "08:00" }
                }
                input id=(&wake_id) type="range" min="0" max="1439" step="60" value="480"
                    aria-label="Wake time" aria-valuetext="08:00" disabled[!hours_active] data-wake-time-slider;
            }
            div class="rest-days-control" {
                button type="button" class="rest-days-step rest-days-decrease" aria-label="Decrease rest duration" data-rest-step="-1" { "−" }
                input type=(if hours_active { "text" } else { "number" }) name="duration"
                    value=(value)
                    inputmode=(if hours_active { "text" } else { "numeric" })
                    pattern="[0-9]+:[0-5][0-9]" min="1" max="365" step="1"
                    aria-label="Rest duration" data-rest-duration-input;
                span class="rest-days-unit" data-rest-unit-label { (unit) }
                button type="button" class="rest-days-step rest-days-increase" aria-label="Increase rest duration" data-rest-step="1" { "+" }
            }
            input type="hidden" name="requested_minutes" disabled[!hours_active] data-rest-exact-minutes;
        }
    }
}

fn format_rest_duration(minutes: u64) -> String {
    let days = minutes / 1_440;
    let hours = minutes % 1_440 / 60;
    let minutes = minutes % 60;
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days} day{}", if days == 1 { "" } else { "s" }));
    }
    if hours > 0 {
        parts.push(format!("{hours} hour{}", if hours == 1 { "" } else { "s" }));
    }
    if minutes > 0 || parts.is_empty() {
        parts.push(format!(
            "{minutes} minute{}",
            if minutes == 1 { "" } else { "s" }
        ));
    }
    parts.join(" ")
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

pub(crate) fn rest_default_minutes(
    limbs: Option<&CharacterLimbs>,
    stats: Option<&CharacterStats>,
    condition: Option<&CharacterCondition>,
    field_repair_minutes: u64,
    smith_wait_minutes: u64,
) -> Option<u64> {
    let healing_days = limbs.map(days_to_full_health).unwrap_or(0);
    let healing_minutes = u64::from(healing_days) * 1_440;
    let fatigue_minutes = stats
        .map(|stats| ((stats.calories_used / 2_000.0) * 1_440.0).ceil() as u64)
        .unwrap_or(0);
    let blood_recovery_minutes = condition.map_or(0, blood_recovery_minutes);
    (limbs.is_some() || stats.is_some() || condition.is_some()).then_some(
        healing_minutes
            .max(fatigue_minutes)
            .max(blood_recovery_minutes)
            .saturating_add(field_repair_minutes)
            .max(smith_wait_minutes),
    )
}

/// This must match the strategic module's `BLOOD_RECOVERY_FRACTION_PER_DAY`.
const BLOOD_RECOVERY_FRACTION_PER_DAY: f32 = 0.01;

fn blood_recovery_minutes(condition: &CharacterCondition) -> u64 {
    if condition.maximum_blood_ml <= 0.0 {
        return 0;
    }
    let missing_fraction = ((condition.maximum_blood_ml - condition.current_blood_ml)
        / condition.maximum_blood_ml)
        .clamp(0.0, 1.0);
    (missing_fraction / BLOOD_RECOVERY_FRACTION_PER_DAY * 1_440.0).ceil() as u64
}
#[cfg(test)]
mod tests {
    use super::{
        Character, CharacterCondition, LocationKind, MerchantShop, encumbrance_inventory_rail,
        encumbrance_meter, filth_status_bar, format_rest_duration, live_merchant_shop_page,
        merchant_inventory_sell_price, merchant_inventory_weight, need_balance_meter,
        repair_custody_panel, repair_submit_control, rest_default_minutes,
        settlement_rest_duration_control, strategic_condition_rail, strategic_encounter_panel,
    };
    use crate::spacetimedb::{
        CharacterFilth, CharacterStrategicCondition, FilthOrigin, FilthSubstance, FoodLot,
        FoodPreparation, ItemKind, StrategicEncounter, StrategicEncounterLoss,
    };
    use adventuresim_core::equipment::EncumbranceSummary;

    #[test]
    fn encounter_panel_renders_only_authoritative_choices_and_exact_losses() {
        let encounter = StrategicEncounter {
            party_id: "party".into(),
            encounter_id: "party:3".into(),
            archetype: "bandits".into(),
            enemy_count: 4,
            roll_index: 3,
            journey_movement_minute: 540,
            journey_elapsed_minute: 700,
            absolute_minute: 1_700,
            longitude_e7: 1,
            latitude_e7: 2,
            terrain: "road".into(),
            party_aware: false,
            enemy_aware: true,
            available_choices: vec!["attack".into(), "surrender".into()],
            status: "awaiting_choice".into(),
            selected_choice: None,
            selection_explanation: "deterministic awareness".into(),
            party_speed_m_per_minute: 60,
            enemy_speed_m_per_minute: 80,
            run_ineligibility: Some("too slow".into()),
            penalty_minutes: 0,
            loss_preview: vec![StrategicEncounterLoss {
                owner_kind: "member".into(),
                owner_id: 7,
                inventory_id: 8,
                item_id: "gold_coin".into(),
                quantity: 12,
                value_each: 1,
            }],
            outcome: None,
        };
        let rendered = strategic_encounter_panel(&encounter).into_string();
        assert!(rendered.contains("The enemy surprised your party"));
        assert!(rendered.contains("Cannot run: too slow"));
        assert!(rendered.contains("12 × gold_coin"));
        assert!(rendered.contains("value=\"attack\""));
        assert!(rendered.contains("value=\"surrender\""));
        assert!(!rendered.contains("value=\"run\""));
        assert!(!rendered.contains("value=\"sneak\""));
    }

    #[test]
    fn inferred_general_blacksmith_exposes_limited_weapon_and_armor_stock() {
        let industries = adventuresim_world_schema::InferredIndustryProfile::new(vec![
            adventuresim_world_schema::IndustryEvidence::Fallback(
                adventuresim_world_schema::FallbackIndustry::CommonAggregate,
            ),
        ])
        .unwrap();
        let economy =
            adventuresim_world_schema::infer_settlement_economy(2, 500, 1, false, &industries)
                .unwrap();

        assert!(adventuresim_core::settlement_economy::storefront_stocks(
            &economy,
            adventuresim_core::settlement_economy::Storefront::Weapons,
            "club",
            adventuresim_core::settlement_economy::CatalogKind::Weapon,
        ));
        assert!(adventuresim_core::settlement_economy::storefront_stocks(
            &economy,
            adventuresim_core::settlement_economy::Storefront::Armor,
            "leather vest",
            adventuresim_core::settlement_economy::CatalogKind::Armor,
        ));
        for category in [
            adventuresim_world_schema::StockCategory::Weapons,
            adventuresim_world_schema::StockCategory::Armor,
        ] {
            assert_eq!(
                economy
                    .stock
                    .iter()
                    .find(|stock| stock.category == category)
                    .unwrap()
                    .abundance,
                1
            );
        }
    }

    #[test]
    fn merchant_food_quote_and_weight_follow_remaining_lot() {
        let mut lot = FoodLot {
            id: 1,
            inventory_item_id: Some(9),
            party_inventory_item_id: None,
            display_name: "Cooked meal".into(),
            preparation: FoodPreparation::Stewed,
            ingredient_item_ids: vec!["raw_venison".into()],
            ingredient_quantities: vec![1.0],
            mass_kg: 25.0,
            nutrition_kcal: 5_000.0,
            total_value: 10.0,
            created_at_minute: 1,
        };
        assert_eq!(merchant_inventory_weight(None, Some(&lot)), "25");
        assert_eq!(merchant_inventory_sell_price(None, Some(&lot)), 8);
        lot.mass_kg = 6.25;
        lot.total_value = 2.5;
        assert_eq!(merchant_inventory_weight(None, Some(&lot)), "6.25");
        assert_eq!(merchant_inventory_sell_price(None, Some(&lot)), 2);
        lot.total_value = 0.5;
        let zero = merchant_inventory_sell_price(None, Some(&lot));
        assert_eq!(zero, 0);
        assert_eq!(
            adventuresim_core::strategic_economy::language_adjusted_sell_price(zero, 0.0),
            0
        );
    }

    #[test]
    fn public_filth_serialization_and_template_expose_only_aggregate_origin() {
        let deposit = CharacterFilth {
            id: 1,
            character_id: 7,
            substance: FilthSubstance::Blood,
            origin: FilthOrigin::Foreign,
            amount: 2,
            deposited_at: 10,
        };
        let serialized = serde_json::to_value(&deposit).unwrap();
        assert!(serialized.get("source_character_id").is_none());
        assert_eq!(
            serialized.get("origin").and_then(|value| value.as_str()),
            Some("Foreign")
        );
        let markup = filth_status_bar(&[deposit]).into_string();
        assert!(markup.contains("2 foreign"));
        assert!(!markup.contains("source_character_id"));
        assert!(!markup.contains("filth-legend"));
        assert!(!markup.contains("/100 filth</span>"));
        assert!(markup.contains("data-strategic-tooltip=\"Filth accumulates"));
        assert!(markup.contains("data-strategic-tooltip=\"Blood\n2\""));
    }

    #[test]
    fn status_rail_places_filth_after_water() {
        let condition = CharacterStrategicCondition {
            character_id: 7,
            morale: 0.0,
            morale_bonus: 0.0,
            morale_bonus_cap: 0.0,
            fervor: 0.0,
            pain: 0.0,
            blood_loss: 0.0,
            fear: 0.0,
            fatigue: 0.0,
            hunger: 0.0,
            thirst: 0.0,
            food_days: 1.0,
            water_days: 1.0,
            water_capacity_ml: 2_000,
            incapacitation: 0.0,
            check_multiplier: 1.0,
            status: "ready".into(),
        };
        let markup =
            strategic_condition_rail(Some(&condition), &[], &[], "/social", false).into_string();
        assert!(markup.contains("class=\"morale-meter\""));
        assert!(markup.contains("href=\"/social\" title=\"Open social menu\""));
        assert!(markup.contains("aria-haspopup=\"dialog\" aria-expanded=\"false\""));
        let water = markup.find("Water").expect("water meter");
        let filth = markup.find("Filth").expect("filth meter");
        assert!(water < filth);
    }

    #[test]
    fn social_catalog_labels_are_generic_grounded_and_accessible() {
        use adventuresim_core::social::{SocialActionKind, SocialTopic};
        let defeat = SocialActionKind::Commiserate.description(SocialTopic::Defeat, true);
        assert_eq!(defeat, "Commiserate about the defeat");
        assert!(!defeat.to_ascii_lowercase().contains("goblin"));
        let actions = social_actions(false, SocialTopic::Defeat);
        assert_eq!(actions.len(), 6);
        assert!(
            actions
                .iter()
                .any(|(_, action, _)| *action == SocialActionKind::Listen)
        );
        assert_eq!(
            social_actions(true, SocialTopic::Defeat),
            vec![("inner-self", SocialActionKind::Reflect, "reflect")]
        );
        assert_eq!(social_actions(false, SocialTopic::Hunger).len(), 3);
        assert_eq!(social_actions(false, SocialTopic::Faith).len(), 4);
        assert_eq!(SocialActionKind::Commiserate.skill_name(false), "Deception");
        assert_eq!(
            SocialActionKind::Flirt.description(SocialTopic::Injury, false),
            "Tell them the scar makes them look striking"
        );
        assert_eq!(familiarity_label(0.0), "0 hours");
        assert_eq!(familiarity_label(0.4), "<1 hours");
        assert_eq!(familiarity_label(9.4), "9 hours");
        let tooltip = belief_tooltip(&crate::spacetimedb::SocialBelief {
            id: "belief".into(),
            observer_id: 1,
            subject_id: 2,
            axis: "self_regard".into(),
            perceived_value: 1,
            confidence: 0.64,
            observed_at_minute: 0,
        });
        assert!(tooltip.contains("Confidence: 64%"));
        assert!(tooltip.contains("Injury is touchy"));
    }

    #[test]
    fn social_skill_family_has_an_average_and_six_expandable_icon_rows() {
        let skills = CharacterSkills {
            character_id: 7,
            insight_hours: 100.0,
            self_awareness_hours: 80.0,
            humor_hours: 60.0,
            command_hours: 40.0,
            deception_hours: 20.0,
            seduction_hours: 10.0,
            ..CharacterSkills::default()
        };
        let markup = social_skill_rows(&skills, 1.0, None).into_string();
        assert!(markup.contains("data-social-primary"));
        assert!(markup.contains("Average of all six Social skills"));
        assert_eq!(markup.matches("data-social-detail").count(), 6);
        for icon in [
            "conversation.svg",
            "awareness.svg",
            "inner-self.svg",
            "juggler.svg",
            "crown.svg",
            "rose.svg",
        ] {
            assert!(markup.contains(icon), "missing social icon {icon}");
        }
    }

    #[test]
    fn herbalist_stock_template_includes_every_prepared_course_and_ingredients() {
        let ingredient = crate::spacetimedb::ItemDefinition {
            kind: ItemKind::Ingredient,
            ..Default::default()
        };
        let medication = crate::spacetimedb::ItemDefinition {
            kind: ItemKind::Medication,
            ..Default::default()
        };
        assert!(MerchantShop::Herbalist.stocks(&ingredient));
        assert!(MerchantShop::Herbalist.stocks(&medication));
        let apple = crate::spacetimedb::ItemDefinition {
            id: "apple".into(),
            kind: ItemKind::Food,
            ..Default::default()
        };
        let pan = crate::spacetimedb::ItemDefinition {
            id: "cooking_pan".into(),
            ..Default::default()
        };
        assert!(MerchantShop::Inn.stocks(&apple));
        assert!(MerchantShop::Inn.stocks(&pan));
        assert!(!MerchantShop::Inn.stocks(&medication));
        assert_eq!(adventuresim_core::disease::MEDICATION_RECIPES.len(), 8);
        let definition = crate::spacetimedb::ItemDefinition {
            id: "black_death_tonic".into(),
            kind: ItemKind::Medication,
            ..Default::default()
        };
        let rendered =
            item_name_with_display("black_death_tonic", "Black Death tonic", Some(&definition))
                .into_string();
        assert!(rendered.contains("data-item-name=\"black_death_tonic\""));
        assert!(rendered.contains("data-item-kind=\"medication\""));
        assert!(rendered.contains(">Black Death tonic</span>"));
    }

    #[test]
    fn location_kind_rejects_unknown_path_segments() {
        assert_eq!("quest".parse(), Ok(LocationKind::Quest));
        assert!("merchant".parse::<LocationKind>().is_err());
    }

    #[test]
    fn encumbrance_meter_formats_exact_text_and_accessible_linear_position() {
        let markup = encumbrance_meter(EncumbranceSummary::new(85.36, 150.0)).into_string();
        assert!(markup.contains(">85.4 / 150.0 kg<"));
        assert!(markup.contains(">-56.9%<"));
        assert!(!markup.contains(">Weight"));
        assert!(!markup.contains(">Penalty"));
        assert!(markup.contains("Weight 85.4 / 150.0 kilograms; Penalty -56.9%"));
        assert!(markup.contains("class=\"encumbrance-values\" aria-hidden=\"true\""));
        assert!(markup.contains(
            "<span class=\"encumbrance-weight\">85.4 / 150.0 kg</span><span class=\"encumbrance-penalty\">-56.9%</span>"
        ));
        assert!(
            markup.contains(
                "</div><div class=\"encumbrance-visual\"><div class=\"encumbrance-meter\""
            )
        );
        assert!(markup.contains("role=\"meter\""));
        assert!(markup.contains("aria-valuenow=\"56.9\""));
        assert!(markup.contains("--encumbrance-position: 56.9067%"));
    }

    #[test]
    fn overloaded_meter_keeps_burden_but_clamps_penalty_and_marker() {
        let markup = encumbrance_meter(EncumbranceSummary::new(185.4, 150.0)).into_string();
        assert!(markup.contains(">185.4 / 150.0 kg<"));
        assert!(markup.contains(">-100.0%<"));
        assert!(markup.contains("--encumbrance-position: 100.0000%"));
    }

    #[test]
    fn encumbrance_css_uses_a_linear_midpoint_gradient_and_contrast_marker() {
        let css = include_str!("../../static/css/strategic.css");
        assert!(css.contains("linear-gradient(90deg, #238b45 0%, #f4d03f 50%, #c62828 100%)"));
        assert!(css.contains(".encumbrance-marker"));
        assert!(css.contains("background: #fff"));
    }

    #[test]
    fn encumbrance_rail_scrolls_items_but_keeps_footer_and_meter_outside() {
        let markup = encumbrance_inventory_rail(
            maud::html! { table class="test-items" {} },
            maud::html! { button class="test-footer" {} },
            EncumbranceSummary::new(10.0, 100.0),
        )
        .into_string();
        assert!(markup.contains(
            "<div class=\"encumbrance-inventory-scroll\"><table class=\"test-items\"></table></div><button class=\"test-footer\"></button><div class=\"encumbrance\">"
        ));

        let css = include_str!("../../static/css/strategic.css");
        assert!(css.contains(".sidebar-section:has(> .encumbrance-inventory-rail)"));
        assert!(css.contains(".encumbrance-inventory-scroll"));
        assert!(css.contains("overflow-y: auto"));
        assert!(css.contains("padding-left: 3.25rem"));
        assert!(css.contains("padding-right: 1.75rem"));
        assert!(css.contains("container-type: inline-size"));
        assert!(css.contains("flex: 0 0 50%"));
        assert!(css.contains("width: 50%"));
        assert!(css.contains("font-size: clamp(0.55rem, 4cqi, 0.78rem)"));
        assert!(css.contains(".encumbrance-meter"));
        assert!(css.contains("width: 100%"));
        assert!(css.contains("@container (max-width: 12rem)"));
        assert!(css.contains("padding-inline: 0.2rem"));
        assert!(css.contains("font-size: 0.5rem"));
        assert!(css.contains("@container (max-width: 10rem)"));
        assert!(css.contains("padding-inline: 0.1rem"));
        assert!(css.contains("padding-right: 0.05rem"));
        assert!(css.contains("padding-left: 0.05rem"));
        assert!(css.contains("font-size: 0.43rem"));
    }

    #[test]
    fn merchant_tabs_render_personal_and_party_encumbrance_as_applicable() {
        let character = Character {
            id: 1,
            name: "Trader".into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: Some("viabundus-1".into()),
            current_quest_location_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive: true,
            temporary: false,
        };
        let render = |shop| {
            live_merchant_shop_page(
                &settlement(),
                &character,
                &[],
                &[],
                &[],
                &[],
                None,
                &[],
                &[],
                &[],
                shop,
                1.0,
                &[],
                None,
                &[],
                0,
                EncumbranceSummary::new(10.0, 100.0),
                EncumbranceSummary::new(30.0, 200.0),
                None,
                SoapRestPreview::default(),
            )
            .into_string()
        };
        let merchant = render(MerchantShop::Weapons);
        assert!(merchant.contains("data-inventory-pane=\"player\""));
        assert!(merchant.contains("data-inventory-pane=\"party\""));
        assert!(merchant.contains(">10.0 / 100.0 kg<"));
        assert!(merchant.contains(">30.0 / 200.0 kg<"));

        let herbalist = render(MerchantShop::Herbalist);
        assert!(herbalist.contains(">10.0 / 100.0 kg<"));
        assert!(!herbalist.contains("data-inventory-pane=\"party\""));
        assert!(!herbalist.contains(">30.0 / 200.0 kg<"));

        let inn = render(MerchantShop::Inn);
        assert!(inn.contains("Cooking supplies"));
        assert!(inn.contains("aria-label=\"Inn rest service\""));
    }

    #[test]
    fn rest_recommendation_includes_blood_recovery() {
        let condition = CharacterCondition {
            character_id: 1,
            body_weight_kg: 70.0,
            current_blood_ml: 4_900.0,
            maximum_blood_ml: 5_000.0,
            religion_id: None,
        };

        assert_eq!(
            rest_default_minutes(None, None, Some(&condition), 0, 0),
            Some(2_880)
        );
    }

    #[test]
    fn settlement_wake_control_is_accessible_and_defaults_to_eight() {
        let markup = settlement_rest_duration_control(1_440, "hours").into_string();
        assert!(markup.contains("data-wake-time"));
        assert!(markup.contains("type=\"range\""));
        assert!(markup.contains("step=\"60\""));
        assert!(markup.contains("value=\"480\""));
        assert!(markup.contains("type=\"text\""));
        assert!(markup.contains("value=\"24:00\""));
        assert!(markup.contains("pattern=\"[0-9]+:[0-5][0-9]\""));
        assert!(markup.contains("aria-label=\"Wake time\""));
        assert!(markup.contains("aria-valuetext=\"08:00\""));
        assert!(markup.contains("name=\"requested_minutes\""));
    }

    #[test]
    fn rest_supplies_are_icons_with_hover_only_details() {
        let markup = soap_wash_preview(SoapRestPreview {
            total_units: 1,
            personal_units: 1,
            available_units: 1,
            alcohol_available: true,
            alcohol_will_be_consumed: false,
            ..SoapRestPreview::default()
        })
        .into_string();
        assert!(markup.contains("aria-label=\"Soap\""));
        assert!(markup.contains("aria-label=\"Alcohol\""));
        assert!(markup.contains("water-drop.svg"));
        assert!(markup.contains("beer-stein.svg"));
        assert!(markup.contains("rest-consumable-indicator available"));
        assert!(markup.contains("rest-consumable-indicator unavailable"));
        assert!(!markup.contains("rest-soap-preview"));
        assert!(markup.contains("Temperate characters do not drink"));
    }

    #[test]
    fn days_recommendation_keeps_slider_disabled_and_minimum_one() {
        let markup = settlement_rest_duration_control(3 * 1_440, "days").into_string();
        assert!(markup.contains("value=\"days\" checked"));
        assert!(markup.contains("aria-disabled=\"true\""));
        assert!(
            markup.contains(
                "value=\"480\" aria-label=\"Wake time\" aria-valuetext=\"08:00\" disabled"
            )
        );
        assert!(markup.contains("type=\"number\" name=\"duration\" value=\"3\""));
        assert!(markup.contains("min=\"1\" max=\"365\" step=\"1\""));
        assert!(markup.contains("name=\"requested_minutes\" disabled"));
    }

    #[test]
    fn rest_summary_duration_keeps_subday_hours_and_minutes() {
        assert_eq!(format_rest_duration(1_441), "1 day 1 minute");
        assert_eq!(format_rest_duration(1_920), "1 day 8 hours");
        assert_eq!(format_rest_duration(2_879), "1 day 23 hours 59 minutes");
    }

    #[test]
    fn need_meter_places_reserve_right_and_incapacitation_left() {
        let reserve =
            need_balance_meter("Food", "meal", "Hunger", "Full", "hunger", 0.5, 0.0).into_string();
        assert!(reserve.contains("--need-reserve: 50.0%; --need-deficit: 0.0%"));
        assert!(reserve.contains("aria-valuenow=\"50\""));
        assert!(reserve.contains(">Full</span>"));

        let hydration = need_balance_meter(
            "Water",
            "water-drop",
            "Thirst",
            "Hydrated",
            "thirst",
            1.0,
            0.0,
        )
        .into_string();
        assert!(hydration.contains(">Hydrated</span>"));

        let deficit =
            need_balance_meter("Food", "meal", "Hunger", "Full", "hunger", 0.0, 1.0 / 9.0)
                .into_string();
        assert!(deficit.contains("--need-reserve: 0.0%; --need-deficit: 11.1%"));
        assert!(deficit.contains("aria-valuenow=\"-11\""));
    }

    #[test]
    fn canonical_imported_religions_have_ui_labels() {
        for (id, label) in [
            ("roman_catholic", "Roman Catholic"),
            ("lutheran", "Lutheran"),
            ("reformed", "Reformed"),
            ("anglican", "Anglican"),
            ("eastern_orthodox", "Eastern Orthodox"),
            ("islamic", "Islamic"),
            ("judaism", "Jewish"),
        ] {
            assert_eq!(religion_name(Some(id)), label);
        }
    }
    fn settlement() -> Settlement {
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

    #[test]
    fn disabled_repair_explanation_is_hoverable_and_focusable() {
        let condition = crate::spacetimedb::ItemCondition {
            inventory_item_id: 4,
            tier_1: 0.0,
            tier_2: 0.0,
            tier_3: 0.0,
            tier_4: 0.2,
            tier_5: 0.0,
        };
        let rendered =
            repair_submit_control(&settlement(), "weapons", 4, Some(&condition), 3).into_string();
        assert!(rendered.contains("disabled-repair-explanation"));
        assert!(rendered.contains("tabindex=\"0\""));
        assert!(rendered.contains("All damage requires Smithing"));
        assert!(rendered.contains("disabled"));
    }

    #[test]
    fn tailor_repair_control_targets_the_clothing_service() {
        let condition = crate::spacetimedb::ItemCondition {
            inventory_item_id: 4,
            tier_1: 0.25,
            tier_2: 0.0,
            tier_3: 0.0,
            tier_4: 0.0,
            tier_5: 0.0,
        };
        let rendered =
            repair_submit_control(&settlement(), "clothing", 4, Some(&condition), 2).into_string();
        assert!(rendered.contains("/clothing/repair"));
        assert!(rendered.contains("row-repair-form"));
        assert!(!rendered.contains("disabled"));
    }

    #[test]
    fn collapsed_currency_label_hides_the_historical_denomination() {
        let definition = crate::spacetimedb::ItemDefinition {
            id: "lubeck_mark".into(),
            kind: crate::spacetimedb::ItemKind::Currency,
            base_value: Some(1),
            weight: 0.01,
            ..Default::default()
        };
        let rendered = item_name_with_quality(&definition.id, Some(&definition)).into_string();
        assert!(rendered.contains(">Coin<"));
        assert!(rendered.contains("data-currency-name=\"Lübeck mark\""));
        assert!(!rendered.contains(">Lübeck mark<"));
    }

    #[test]
    fn alcohol_labels_expose_a_shared_inventory_group() {
        let definition = crate::spacetimedb::ItemDefinition {
            id: "small_beer".into(),
            kind: crate::spacetimedb::ItemKind::Simple,
            alcohol_serving_ml: 500,
            ..Default::default()
        };
        let rendered = item_name_with_quality(&definition.id, Some(&definition)).into_string();
        assert!(rendered.contains("data-item-name=\"small_beer\""));
        assert!(rendered.contains("data-item-group=\"alcohol\""));
        assert!(rendered.contains("data-group-name=\"Alcohol\""));
    }

    #[test]
    fn smith_player_actions_keep_sell_and_repair_in_one_hover_area() {
        let repair = repair_submit_control(&settlement(), "weapons", 4, None, 3);
        let rendered =
            merchant_sell_repair_controls(4, "torch", 2, 3, 1, true, Some(repair)).into_string();

        assert!(rendered.starts_with("<div class=\"inventory-row-actions smith-player-actions\">"));
        assert_eq!(rendered.matches("data-merchant-sell=\"").count(), 1);
        assert!(rendered.contains("data-dynamic-transfer"));
        assert!(rendered.contains("data-default-transfer-mode=\"one\""));
        assert!(rendered.contains("data-label-target=\"Sell surplus Torch\""));
        assert!(rendered.contains("data-label-all=\"Sell all Torch\""));
        assert!(rendered.contains("row-repair-form"));
        assert_eq!(rendered.matches("smith-player-actions").count(), 1);
    }

    #[test]
    fn equipped_smith_items_retain_the_repair_action_without_sell_controls() {
        let repair = repair_submit_control(&settlement(), "weapons", 4, None, 3);
        let rendered =
            merchant_sell_repair_controls(4, "sword", 10, 1, 0, false, Some(repair)).into_string();

        assert!(rendered.contains("smith-player-actions"));
        assert!(rendered.contains("row-repair-form"));
        assert!(!rendered.contains("data-merchant-sell"));
        assert!(rendered.contains("Equipped items cannot be sold"));
        assert!(rendered.contains("trade-transfer trade-transfer-left"));
        assert!(rendered.contains("disabled"));
    }

    #[test]
    fn non_smith_sell_controls_do_not_reserve_a_repair_slot() {
        let rendered = merchant_sell_repair_controls(4, "shirt", 3, 1, 0, true, None).into_string();

        assert!(rendered.starts_with("<div class=\"inventory-row-actions\">"));
        assert!(!rendered.contains("smith-player-actions"));
        assert!(rendered.contains("data-merchant-sell"));
    }

    #[test]
    fn unavailable_transfer_button_keeps_a_disabled_action_slot() {
        let rendered =
            disabled_transfer_button("left", "Equipped items cannot be transferred").into_string();

        assert!(rendered.contains("trade-transfer trade-transfer-left"));
        assert!(rendered.contains("Equipped items cannot be transferred"));
        assert!(rendered.contains("disabled"));
        assert!(rendered.contains("inventory-transfer-glyph"));
    }

    #[test]
    fn durability_bar_uses_qualitative_copy_and_marks_smith_repairable_damage() {
        let condition = crate::spacetimedb::ItemCondition {
            inventory_item_id: 4,
            tier_1: 0.1,
            tier_2: 0.0,
            tier_3: 0.2,
            tier_4: 0.1,
            tier_5: 0.0,
        };
        let rendered = condition_bar(Some(&condition), Some(3)).into_string();
        assert!(rendered.contains("condition-repairable"));
        for tier in 1..=5 {
            assert!(rendered.contains(&format!("condition-tier-{tier}")));
        }
        assert!(rendered.contains("flashing portion can be repaired"));
        assert!(!rendered.contains("condition-number"));
        assert!(!rendered.contains("% condition"));
    }

    #[test]
    fn durable_item_names_expose_quality_color_and_description() {
        let definition = crate::spacetimedb::ItemDefinition {
            id: "commissioned_sword".into(),
            weight: 1.0,
            slot: ItemSlot::AnyHolding,
            kind: crate::spacetimedb::ItemKind::Weapon,
            base_value: None,
            nutrition_kcal: 0.0,
            water_capacity_ml: 0,
            quality: 4,
            durability_yield: 0.0,
            durability_fracture: 0.0,
            durability_wear: 0.0,
            durability_failure_share: 0.0,
            edge_sensitivity: 0.0,
            handling_sensitivity: 0.0,
            ..Default::default()
        };

        let rendered = item_name_with_quality(&definition.id, Some(&definition)).into_string();
        assert!(rendered.contains("item-quality-4"));
        assert!(rendered.contains("knightly commission"));
    }

    #[test]
    fn completed_repair_bar_projects_the_condition_before_retrieval() {
        let condition = crate::spacetimedb::ItemCondition {
            inventory_item_id: 4,
            tier_1: 0.1,
            tier_2: 0.2,
            tier_3: 0.0,
            tier_4: 0.0,
            tier_5: 0.0,
        };

        let rendered = completed_repair_condition_bar(Some(&condition), 3).into_string();
        assert!(rendered.contains("Full durability"));
        assert!(rendered.contains("width:100%"));
    }

    #[test]
    fn smith_player_inventory_uses_the_compact_seven_column_table() {
        let rendered = trade_inventory_table(
            "test",
            InventoryColumnSet::Weapons,
            true,
            true,
            true,
            html! {},
        )
        .into_string();
        assert!(rendered.contains("smith-player-inventory-table"));
        assert!(rendered.contains("inventory-column-type"));
        assert!(rendered.contains("aria-label=\"Item type\""));
        assert!(rendered.contains("inventory-column-durability"));
        assert!(rendered.contains("hammer-nails.svg"));
        assert!(!rendered.contains("Repair all eligible items"));
        assert!(!rendered.contains("durability-header-label"));
    }

    #[test]
    fn repair_all_precedes_the_sell_bulk_control() {
        let rendered = inventory_footer_controls_with_leading(
            Some(repair_all_control(&settlement(), "weapons")),
            "sell",
            "Sell surplus",
            "Sell everything",
        )
        .into_string();
        let repair = rendered.find("inventory-footer-repair").unwrap();
        let sell = rendered.find("data-inventory-bulk=\"sell\"").unwrap();
        assert!(rendered.contains("inventory-footer-actions-grouped"));
        assert!(repair < sell);
    }

    #[test]
    fn skill_meter_and_schedule_use_segmented_rank_and_text_time_controls() {
        let meter =
            skill_rank_bar(3.5, 2.75, "Skill test", SkillRankBarOptions::default()).into_string();
        for tier in 1..=5 {
            assert!(meter.contains(&format!("skill-rank-segment-{tier}")));
        }
        assert!(meter.contains("role=\"meter\""));
        assert!(meter.contains("aria-valuenow=\"2.8\""));
        assert!(meter.contains("class=\"skill-rank-value\""));
        assert!(!meter.contains("tabindex"));
        let allocation = schedule_allocation_cell("smithing_minutes", 75, true).into_string();
        assert!(allocation.contains("data-schedule-input"));
        assert!(allocation.contains("data-schedule-display"));
        assert!(allocation.contains("Click to enter a time such as 8, 8:30, or 830"));
        assert!(!allocation.contains("data-schedule-step"));
        assert!(!allocation.contains("type=\"range\""));
        assert!(!allocation.contains("schedule-handle"));
    }

    #[test]
    fn surgery_supplies_are_icon_counts_with_hover_labels() {
        let supply = surgery_supply("Bandages", "bandage-roll", 8).into_string();
        assert!(supply.contains("class=\"surgery-supply\""));
        assert!(supply.contains("data-strategic-tooltip=\"Bandages: 8 available\""));
        assert!(supply.contains("bandage-roll.svg"));
        assert!(supply.contains(">x8</span>"));
        assert!(!supply.contains(">Bandages</span>"));
    }

    #[test]
    fn surgery_item_icons_explain_consumed_reusable_and_equipped_supplies() {
        let bandage =
            surgery_item_requirement(SurgeryItemRequirement::BandageConsumed).into_string();
        assert!(bandage.contains("data-strategic-tooltip=\"Expend one bandage\""));
        assert!(bandage.contains(">x1</span>"));

        let kit =
            surgery_item_requirement(SurgeryItemRequirement::SurgeryKitReusable).into_string();
        assert!(kit.contains("data-strategic-tooltip=\"Requires surgery kit\""));
        assert!(kit.contains("aria-label=\"Requires surgery kit; reusable and not consumed\""));
        assert!(kit.contains("medical-pack.svg"));
        assert!(!kit.contains("surgery-item-overlay"));

        let splint = surgery_item_requirement(SurgeryItemRequirement::SplintEquipped).into_string();
        assert!(splint.contains("data-strategic-tooltip=\"Equips 1 splint\""));
        assert!(splint.contains("check-mark.svg"));
    }

    #[test]
    fn surgery_difficulty_uses_shared_skill_meter_for_met_and_unmet_ranks() {
        let meter = surgery_difficulty_meter("Remove ball", 4.0, 2.0).into_string();
        assert!(meter.contains("stat-icon-surgeon"));
        assert!(meter.contains("role=\"meter\""));
        for tier in 1..=5 {
            assert!(meter.contains(&format!("skill-rank-segment-{tier}")));
        }
        assert_eq!(
            meter
                .matches("class=\"rank-current\" style=\"width:100.0%\"")
                .count(),
            2
        );
        assert_eq!(meter.matches("left:0.0%;width:100.0%").count(), 2);
        assert!(!meter.contains("skill-rank-value"));
        assert!(!meter.contains(">4.0<"));
        assert!(meter.contains(
            "aria-label=\"Remove ball: requires 4.0 procedure skill; current effective skill 2.0\""
        ));
        let over_cap = surgery_difficulty_meter("Remove ball", 7.2, 5.0).into_string();
        assert!(over_cap.contains("surgery-difficulty-over-cap-marker"));
        assert!(over_cap.contains("requires 7.2 procedure skill; current effective skill 5.0"));
        assert!(!adventuresim_core::surgery::extraction_requires_surgery_kit(1.0));
        assert!(adventuresim_core::surgery::extraction_requires_surgery_kit(
            1.01
        ));
    }

    #[test]
    fn surgery_preview_uses_the_same_asymmetric_procedure_composition_as_reducers() {
        let checks = [5.0, 5.0, 0.0];
        let extraction = surgery_procedure_skill("extract", checks, false);
        let stitching = surgery_procedure_skill("stitch", checks, false);
        assert_eq!(
            extraction,
            adventuresim_core::surgery::procedure_skill("extract", 5.0, 5.0, 0.0, false)
        );
        assert_eq!(
            stitching,
            adventuresim_core::surgery::procedure_skill("stitch", 5.0, 5.0, 0.0, false)
        );
        assert!(extraction >= 4.0);
        assert!(stitching < 4.0);
        assert_eq!(surgery_procedure_skill("extract", checks, true), 2.5);
    }

    #[test]
    fn unavailable_surgery_rows_are_greyed_and_buttons_keep_procedure_names() {
        let row = surgery_procedure_row(
            "/test",
            "Stitch",
            "scalpel",
            "stitch",
            &[SurgeryItemRequirement::SurgeryKitReusable],
            10,
            2.0,
            0.0,
            Some("No injury is present"),
            Some("No injury is present"),
            None,
            true,
            true,
            None,
        )
        .into_string();
        assert!(row.contains("surgery-procedure-unavailable"));
        assert!(row.contains("data-strategic-tooltip=\"No injury is present\""));
        assert!(row.contains("aria-label=\"Stitch: No injury is present\" tabindex=\"0\""));
        assert!(row.contains("disabled title=\"No injury is present\""));
        assert!(row.contains(">Stitch</button>"));
        assert!(!row.contains(">No injury is present</button>"));
    }

    #[test]
    fn bloody_procedure_names_concrete_automatic_alcohol_without_risk_numbers() {
        let row = surgery_procedure_row(
            "/test",
            "Bandage",
            "bandage-roll",
            "bandage",
            &[],
            10,
            0.0,
            1.0,
            None,
            None,
            None,
            false,
            true,
            Some("aqua_vitae"),
        )
        .into_string();
        assert!(row.contains("Consumes 1 Aqua vitae"));
        assert!(row.contains("beer-stein.svg"));
        assert!(!row.contains("infection probability"));
        assert!(!row.contains("use_alcohol"));
    }

    #[test]
    fn schedule_table_uses_compact_accessible_icon_headers() {
        let skills = CharacterSkills {
            character_id: 1,
            polearm_hours: 0.0,
            axe_hours: 0.0,
            bludgeon_hours: 0.0,
            sword_hours: 0.0,
            knife_hours: 0.0,
            dodge_hours: 0.0,
            block_hours: 0.0,
            bow_hours: 0.0,
            crossbow_hours: 0.0,
            firearm_hours: 0.0,
            throw_hours: 0.0,
            will_hours: 0.0,
            insight_hours: 0.0,
            self_awareness_hours: 0.0,
            humor_hours: 0.0,
            command_hours: 0.0,
            deception_hours: 0.0,
            seduction_hours: 0.0,
            medicine_hours: 0.0,
            cooking_hours: 0.0,
            religion_hours: adventuresim_world_schema::ReligionHours {
                roman_catholic: 1_000.0,
                ..Default::default()
            },
            oral_languages: Default::default(),
            written_languages: Default::default(),
            stealth_hours: 0.0,
            balance_hours: 0.0,
            terrain_plains_hours: 0.0,
            terrain_forest_hours: 0.0,
            terrain_hills_hours: 0.0,
            terrain_urban_hours: 0.0,
            anatomy_hours: 0.0,
            tailoring_hours: 0.0,
            smithing_hours: 0.0,
        };
        let schedule = CharacterTrainingSchedule {
            character_id: 1,
            downtime: crate::spacetimedb::ScheduleAllocation {
                combat_training_minutes: 90,
                prayer_minutes: 120,
                ..Default::default()
            },
            travel: crate::spacetimedb::ScheduleAllocation::default(),
        };
        let rendered = skills_table(
            "Your skills",
            &skills,
            1.0,
            1.0,
            1.0,
            Some(&schedule),
            None,
            false,
            0.0,
            Some(OfficialReligion::Judaism),
            CombatTrainingProfile::default(),
            false,
            CharacterSkillActions::default(),
        )
        .into_string();

        assert!(rendered.contains(
            "scope=\"colgroup\" colspan=\"8\" class=\"schedule-table-title\">Your skills"
        ));
        assert_eq!(rendered.matches("<colgroup>").count(), 2);
        assert!(rendered.contains(
            "<col class=\"religion-auto-column\"><col class=\"party-skill-time-column\"><col class=\"religion-expand-column\">"
        ));
        assert_eq!(
            rendered.matches("aria-label=\"Daily allocation\"").count(),
            1
        );
        assert!(!rendered.contains("aria-label=\"Automatic training\""));
        for label in ["Currency", "Virtue", "Morale", "Fatigue"] {
            assert!(rendered.contains(&format!("aria-label=\"{label}\"")));
        }
        assert!(rendered.contains("data-religion-expand"));
        assert!(!rendered.contains("class=\"skill-rank-value\""));
        assert_eq!(
            rendered.matches("class=\"party-skill-row").count(),
            rendered.matches("class=\"religion-expand-cell\"").count(),
        );
        assert!(rendered.contains("aria-expanded=\"false\""));
        assert!(rendered.contains("data-religion-primary=\"judaism\""));
        assert!(rendered.contains("Expand Judaism Religion skill"));
        assert!(rendered.contains("title=\"Judaism\""));
        assert!(rendered.contains("/static/icons/religion/fontawesome-star-of-david.svg"));
        assert!(!rendered.contains("data-combat-auto-toggle"));
        for group in ["melee", "ranged", "defense"] {
            assert!(rendered.contains(&format!("data-combat-expand=\"{group}\"")));
            assert!(rendered.contains(&format!("data-combat-detail=\"{group}\"")));
        }
        assert!(!rendered.contains("aria-label=\"Religion details\""));
        assert!(rendered.contains("aria-label=\"Skill details\""));
        assert!(rendered.contains("Sparring and target practice"));
        assert!(rendered.contains("Carousing"));
        assert_eq!(rendered.matches("data-religion-detail").count(), 1);
        assert!(!rendered.contains("title=\"Lutheranism\""));
        assert!(!rendered.contains("religion_judaism_minutes"));
        assert!(!rendered.contains("effective /"));
        assert!(rendered.contains("100.0 effective hours; 0.0 directly studied hours"));
        let primary_icon = rendered
            .find("/static/icons/religion/fontawesome-star-of-david.svg")
            .unwrap();
        let expand = rendered.find("data-religion-expand").unwrap();
        assert!(primary_icon < expand);
        assert!(rendered.contains("class=\"religion-expand-cell\"><button"));
        assert!(rendered.contains("aria-label=\"Will\""));
        assert!(!rendered.contains("data-religion-auto-budget disabled"));
        assert!(!rendered.contains("data-religion-manual-budget disabled"));
        assert!(rendered.contains("/static/icons/game/coins.svg"));
        assert!(!rendered.contains(">Gold</th>"));
        assert!(!rendered.contains(">Virt.</th>"));

        let rail = party_skills_rail(
            "Your skills",
            Some(&skills),
            None,
            Some(&schedule),
            Some("/schedule"),
            None,
            false,
            0.0,
            Some("judaism"),
            CombatTrainingProfile::default(),
            CharacterSkillActions::default(),
        )
        .into_string();
        assert!(!rail.contains("class=\"sidebar-header\">Your skills"));
        assert!(rail.contains("<h3 class=\"sr-only\">Your skills</h3>"));
        assert!(rail.contains("data-schedule-save-status"));
        assert!(rail.contains("role=\"status\" aria-live=\"polite\" hidden"));
        assert!(rail.contains("data-schedule-retry>Retry</button>"));
        assert!(!rail.contains("data-activity-modal"));
        assert!(!rail.contains("data-activity-open"));
        let settlement_rail = party_skills_rail(
            "Your skills",
            Some(&skills),
            None,
            Some(&schedule),
            Some("/locations/settlement/lubeck/party/1/schedule"),
            None,
            false,
            0.0,
            Some("judaism"),
            CombatTrainingProfile::default(),
            CharacterSkillActions::default(),
        )
        .into_string();
        assert!(settlement_rail.contains("data-activity-modal"));
        assert!(settlement_rail.contains("data-activity-open"));
        assert!(!rail.contains(">⚙</span>"));
        assert!(!rail.contains("aria-label=\"Automatic training\""));
    }

    #[test]
    fn defense_will_uses_head_health_for_its_injury_adjusted_rank() {
        let skills = CharacterSkills {
            will_hours: 5_000.0,
            ..Default::default()
        };
        let rendered = combat_skill_rows(
            &skills,
            0.2,
            0.8,
            1.0,
            None,
            CombatTrainingProfile::default(),
        )
        .into_string();
        let will = rendered.find("aria-label=\"Will\"").unwrap();
        let start = rendered[..will].rfind("<tr").unwrap();
        let end = will + rendered[will..].find("</tr>").unwrap() + "</tr>".len();
        let will_row = &rendered[start..end];
        let rank = Skill::Will.training_rank(5_000.0);
        let expected = skill_rank_bar(
            rank,
            rank * 0.2,
            "5000 hours invested",
            skill_rail_bar_options(),
        )
        .into_string();
        assert!(will_row.contains(&expected));
    }

    #[test]
    fn language_families_are_expandable_accessible_and_color_coded() {
        let skills = CharacterSkills {
            oral_languages: adventuresim_world_schema::OralLanguageHours {
                east_central: 5_000.0,
                ..Default::default()
            },
            written_languages: adventuresim_world_schema::WrittenLanguageHours {
                german: 1_000.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let rendered = language_skill_rows(&skills, false).into_string();
        assert!(rendered.contains("Expand Oral languages"));
        assert!(rendered.contains("Expand Written languages"));
        assert!(rendered.contains("language-oral language-blackletter"));
        assert!(rendered.contains("language-written language-blackletter"));
        assert!(rendered.contains(
            "5000.0 effective hours; 5000.0 directly studied hours across Oral languages"
        ));
        assert!(rendered.contains(
            "1000.0 effective hours; 1000.0 directly studied hours across Written languages"
        ));
        assert!(rendered.contains("title=\"East-central — Ostmitteldeutsch\""));
        assert!(rendered.contains("5000.0 effective hours; 5000.0 directly studied hours"));
        assert!(rendered.contains("title=\"Latin — Latine\""));
        assert!(!rendered.contains("title=\"Romani — Romani\""));
        assert_eq!(rendered.matches("data-language-detail=\"oral\"").count(), 4);
        assert_eq!(
            rendered.matches("data-language-detail=\"written\"").count(),
            3
        );
    }

    #[test]
    fn language_families_are_hidden_without_effective_hours() {
        let rendered = language_skill_rows(&CharacterSkills::default(), false).into_string();
        assert!(!rendered.contains("Expand Oral languages"));
        assert!(!rendered.contains("Expand Written languages"));
        assert!(!rendered.contains("data-language-detail"));
    }

    #[test]
    fn activity_rows_show_signed_daily_effects_instead_of_allocation_bars() {
        let rendered = schedule_special_row(
            "Thievery",
            "market",
            "thievery_minutes",
            120,
            true,
            true,
            ActivityEffectRates::linear(2.0, -1.0, 0.0, 0.0),
            None,
            None,
            "Test activity",
        )
        .into_string();
        for effect in ["gold", "virtue", "morale", "fatigue"] {
            assert!(rendered.contains(&format!("data-activity-effect=\"{effect}\"")));
        }
        assert!(rendered.contains("schedule-effect-positive"));
        assert!(rendered.contains(">+4</td>"));
        assert!(rendered.contains("schedule-effect-negative"));
        assert!(rendered.contains(">-2.0</td>"));
        assert!(rendered.contains("<span class=\"sr-only\">Thievery</span>"));
        assert!(!rendered.contains("<strong>Thievery</strong>"));
        assert!(!rendered.contains("schedule-allocation-fill"));
        assert!(!rendered.contains("schedule-special-track"));
    }

    #[test]
    fn activity_training_column_totals_and_explains_effective_skill_hours() {
        let combat =
            activity_training_cell("Combat Training", "combat_training_minutes", 120, None)
                .into_string();
        assert!(combat.contains(">+2.00h<"));
        assert!(combat.contains("Relevant combat skills: +2.00h"));

        let carousing =
            activity_training_cell("Carousing", "carousing_minutes", 120, None).into_string();
        assert!(carousing.contains(">+0.50h<"));
        assert!(carousing.contains("Humor: +0.50h"));

        let profession = ProfessionActivityPreview {
            training_rates: vec![
                ("Medicine".into(), 0.5),
                ("Anatomy".into(), 1.0 / 6.0),
                ("Knife".into(), 1.0 / 6.0),
                ("Tailoring".into(), 1.0 / 6.0),
            ],
            apprenticeship_accrued: 0,
            practice_accrued: 0,
            practice_threshold: 8 * 60 * PROFESSION_ACCRUAL_SCALE,
            practice_reward: "gold",
            tier_label: "apprentice",
        };
        let apprenticeship = activity_training_cell(
            "Apprenticeship — herbalist",
            "apprenticeship_minutes",
            120,
            Some(&profession),
        )
        .into_string();
        assert!(apprenticeship.contains(">+2.00h<"));
        assert!(apprenticeship.contains("Medicine: +1.00h"));
        assert!(apprenticeship.contains("Anatomy: +0.33h"));
        assert!(apprenticeship.contains("Knife: +0.33h"));
        assert!(apprenticeship.contains("Tailoring: +0.33h"));

        let leisure = activity_training_cell("Leisure", "leisure_minutes", 480, None).into_string();
        assert!(leisure.contains(">—<"));
        assert!(leisure.contains("No skill training"));
    }

    #[test]
    fn profession_preview_uses_accrual_tier_reward_and_training_distribution() {
        let threshold = APPRENTICESHIP_REWARD_THRESHOLD;
        let row = CharacterApprenticeship {
            id: 1,
            character_id: 7,
            service_id: "weapons".into(),
            religion_id: None,
            started_minute: 0,
            apprenticeship_minutes_accrued: threshold - 60 * PROFESSION_ACCRUAL_SCALE,
            practice_minutes_accrued: 0,
        };
        let journeyman = CharacterSkills {
            smithing_hours: 4_000.0,
            ..Default::default()
        };
        let preview = ActivityPreviewRates::default()
            .with_professions(Some(&journeyman), std::slice::from_ref(&row));
        let smith = preview.profession.get("weapons").unwrap();
        assert_eq!(smith.tier_label, "journeyman");
        assert_eq!(smith.practice_threshold, 8 * 60 * PROFESSION_ACCRUAL_SCALE);
        assert_eq!(
            smith.reward_delta("apprenticeship_minutes", 60),
            [-1.0, 0.0]
        );
        assert_eq!(smith.training_rates, vec![("Smithing".into(), 1.0)]);

        let master = CharacterSkills {
            smithing_hours: 25_000.0,
            ..Default::default()
        };
        let preview = ActivityPreviewRates::default().with_professions(Some(&master), &[row]);
        let smith = preview.profession.get("weapons").unwrap();
        assert_eq!(smith.tier_label, "master");
        assert_eq!(smith.practice_threshold, 2 * 60 * PROFESSION_ACCRUAL_SCALE);
        assert_eq!(
            smith.reward_delta("profession_practice_minutes", 240),
            [2.0, 0.0]
        );
    }

    #[test]
    fn server_rendered_effects_normalize_negative_zero() {
        let rendered = activity_effect_cell("fatigue", -0.0006).into_string();
        assert!(rendered.contains("schedule-effect-neutral"));
        assert!(rendered.contains(">0</td>"));
        assert!(!rendered.contains("-0.0"));

        let negative = activity_effect_cell("fatigue", -0.06).into_string();
        assert!(negative.contains("schedule-effect-negative"));
        assert!(negative.contains(">-0.1</td>"));
    }

    #[test]
    fn prayer_preview_uses_zero_partial_and_full_party_religion_checks() {
        let minutes = 240;
        let full = ActivityEffectRates::prayer(1.0).values(minutes)[2];
        assert_eq!(ActivityEffectRates::prayer(0.0).values(minutes)[2], 0.0);
        assert!((ActivityEffectRates::prayer(0.5).values(minutes)[2] - full * 0.5).abs() < 0.001);
        assert!(full > 0.0);
        assert!((ActivityEffectRates::meditation().values(minutes)[2] - full * 0.25).abs() < 0.001);
    }

    #[test]
    fn leisure_and_labor_previews_decompose_the_shared_fatigue_outcome() {
        let schedule = ScheduleAllocation {
            labor_minutes: 240,
            combat_training_minutes: 720,
            ..Default::default()
        };
        let leisure = leisure_preview(&schedule, 0.0);
        assert_eq!(leisure.outcome.leisure_hours, 8.0);
        assert_eq!(leisure.outcome.fatigue_delta, 0.0);
        assert_eq!(leisure.outcome.morale, 0.0);
        assert_eq!(leisure.fatigue_display, -2.0);
        assert_eq!(
            ActivityEffectRates::linear(
                0.0,
                0.0,
                0.0,
                LABOR_FATIGUE_PER_HOUR / FATIGUE_RESERVOIR_PER_PREVIEW_POINT,
            )
            .values(schedule.labor_minutes)[3],
            2.0
        );
        let rendered = schedule_special_row(
            "Leisure",
            "inn",
            "leisure_minutes",
            0,
            false,
            false,
            ActivityEffectRates::default(),
            Some(leisure),
            None,
            "Test leisure",
        )
        .into_string();
        for attribute in [
            "data-leisure-baseline-fatigue",
            "data-leisure-labor-fatigue-rate",
            "data-leisure-recovery-rate",
            "data-leisure-morale-limit",
            "data-leisure-morale-scale",
            "data-leisure-fatigue-preview-divisor",
        ] {
            assert!(rendered.contains(attribute));
        }
        assert!(rendered.contains(">-2.0</td>"));
    }

    #[test]
    fn equipment_checkbox_is_enabled_only_for_equippable_items() {
        let inventory = InventoryItem {
            id: 7,
            character_id: 9,
            item_id: "sword".into(),
            qty: 1,
        };
        let mut definition = crate::spacetimedb::ItemDefinition {
            id: "sword".into(),
            weight: 1.0,
            slot: ItemSlot::AnyHolding,
            kind: crate::spacetimedb::ItemKind::Weapon,
            base_value: None,
            nutrition_kcal: 0.0,
            water_capacity_ml: 0,
            quality: 3,
            durability_yield: 0.0,
            durability_fracture: 0.0,
            durability_wear: 0.0,
            durability_failure_share: 0.0,
            edge_sensitivity: 0.0,
            handling_sensitivity: 0.0,
            ..Default::default()
        };
        let enabled = equipment_checkbox(&inventory, Some(&definition), false).into_string();
        assert!(enabled.contains("data-equipment-toggle"));
        assert!(!enabled.contains(" disabled"));
        definition.slot = ItemSlot::None;
        let disabled = equipment_checkbox(&inventory, Some(&definition), false).into_string();
        assert!(disabled.contains(" disabled"));
    }

    #[test]
    fn merchant_stock_table_hides_quantity_and_target_columns() {
        let rendered = trade_inventory_table(
            "merchant-left",
            InventoryColumnSet::Basic,
            false,
            false,
            false,
            html! {},
        )
        .into_string();
        assert!(rendered.contains("<colgroup>"));
        assert!(!rendered.contains("inventory-column-count"));
        assert!(!rendered.contains("inventory-column-target"));
        assert!(rendered.contains("inventory-column-type"));
        assert!(rendered.contains("inventory-column-weight"));
        assert!(rendered.contains("inventory-column-gold"));
        assert!(rendered.contains("title=\"Currency\""));
        assert!(rendered.contains("aria-label=\"Currency\""));
        assert!(rendered.contains("/static/icons/game/coins.svg"));
    }

    #[test]
    fn inventory_type_header_and_row_share_the_first_column() {
        let rendered = trade_inventory_table(
            "test",
            InventoryColumnSet::Basic,
            true,
            false,
            false,
            html! {
                tr class="trade-inventory-row" {
                    td class="inventory-item-type" { (item_type_icon("arming_sword")) }
                    td class="inventory-item-name" { "Arming sword" }
                    td { "1" } td { "1" } td { "12" }
                }
            },
        )
        .into_string();
        let header = rendered.find("inventory-column-type").unwrap();
        let item_header = rendered.find("inventory-column-item").unwrap();
        let type_cell = rendered.find("inventory-item-type").unwrap();
        let item_cell = rendered.find("inventory-item-name").unwrap();
        assert!(header < item_header && type_cell < item_cell);
        assert!(rendered.contains("/static/icons/game/broadsword.svg"));
    }

    #[test]
    fn smith_custody_panel_shows_only_matching_service_orders() {
        let orders = [
            crate::spacetimedb::RepairOrder {
                id: 1,
                owner_character_id: 9,
                inventory_item_id: 11,
                item_id: "sword".into(),
                settlement_id: "viabundus-1".into(),
                smith_skill: 3,
                submitted_at_minutes: 0,
                ready_at_minutes: 10,
                target_condition: 1.0,
                quoted_cost: 12,
            },
            crate::spacetimedb::RepairOrder {
                id: 2,
                owner_character_id: 9,
                inventory_item_id: 12,
                item_id: "cuirass".into(),
                settlement_id: "viabundus-1".into(),
                smith_skill: 3,
                submitted_at_minutes: 0,
                ready_at_minutes: 10,
                target_condition: 1.0,
                quoted_cost: 24,
            },
        ];
        let items = [
            crate::spacetimedb::ItemDefinition {
                id: "sword".into(),
                weight: 1.0,
                slot: ItemSlot::AnyHolding,
                kind: crate::spacetimedb::ItemKind::Weapon,
                base_value: None,
                nutrition_kcal: 0.0,
                water_capacity_ml: 0,
                quality: 3,
                durability_yield: 0.0,
                durability_fracture: 0.0,
                durability_wear: 0.0,
                durability_failure_share: 0.0,
                edge_sensitivity: 0.0,
                handling_sensitivity: 0.0,
                ..Default::default()
            },
            crate::spacetimedb::ItemDefinition {
                id: "cuirass".into(),
                weight: 1.0,
                slot: ItemSlot::Chest,
                kind: crate::spacetimedb::ItemKind::Armor,
                base_value: None,
                nutrition_kcal: 0.0,
                water_capacity_ml: 0,
                quality: 3,
                durability_yield: 0.0,
                durability_fracture: 0.0,
                durability_wear: 0.0,
                durability_failure_share: 0.0,
                edge_sensitivity: 0.0,
                handling_sensitivity: 0.0,
                ..Default::default()
            },
        ];
        let weapons = repair_custody_panel(
            &settlement(),
            MerchantShop::Weapons,
            &orders,
            &[],
            &items,
            0,
            4,
        )
        .into_string();
        let armor = repair_custody_panel(
            &settlement(),
            MerchantShop::Armor,
            &orders,
            &[],
            &items,
            0,
            3,
        )
        .into_string();
        assert!(weapons.contains("sword"));
        assert!(!weapons.contains("cuirass"));
        assert!(weapons.contains("repair-custody-table"));
        assert!(weapons.contains("Smithing 4"));
        assert!(weapons.contains("stat-icon-smithing"));
        for tier in 1..=5 {
            assert!(weapons.contains(&format!("skill-rank-segment-{tier}")));
        }
        assert!(weapons.contains("repair-custody-header-actions"));
        assert!(weapons.contains("inventory-actions-header"));
        assert!(weapons.contains("inventory-actions-cell"));
        assert!(weapons.contains("Durability"));
        assert!(weapons.contains("ETA"));
        assert!(weapons.contains("Full repair cost"));
        assert!(weapons.contains("repair-retrieve-all"));
        assert!(weapons.contains("Retrieve up to two completed matching items"));
        assert!(!weapons.to_lowercase().contains("affordable prefix"));
        assert!(weapons.contains("/repairs/1/retrieve"));
        assert!(weapons.contains(">12<"));
        assert!(!weapons.contains("Target "));
        assert!(armor.contains("cuirass"));
        assert!(!armor.contains("sword"));
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

        let markup =
            settlement_overview_page(&settlement(), &aliases, &descriptions, None, &[], None)
                .into_string();

        assert!(markup.contains("Also known as"));
        assert!(markup.contains("Lubeke"));
        assert!(markup.contains("Historical description — German"));
        assert!(markup.contains("Burg &amp; Markt &lt;alt&gt;"));
        assert!(!markup.contains("<alt>"));
    }
    use super::*;

    fn quest_destination() -> TravelDestination {
        TravelDestination {
            id: "quest-location".to_string(),
            name: "Bandit camp".to_string(),
            description: "A camp beside the road.".to_string(),
            summary: Some("Active quest".to_string()),
            travel_action: "/quests/quest-location/travel".to_string(),
            distance_m: 1_000,
            journey_minutes: 48,
            camp_stop_minutes: Vec::new(),
            camp_forecasts: Vec::new(),
            departure_minute: 0,
            itinerary_total_elapsed_minutes: 96,
            itinerary_segments: Vec::new(),
            quest_in_progress: true,
            active_quest_route: false,
            turn_in_ready: false,
            open_quest_available: false,
            provision_forecast: None,
            terrain_route: None,
            return_terrain_route: None,
            route_fallback: true,
        }
    }

    #[test]
    fn active_quest_destination_has_red_status_badge() {
        let destination = quest_destination();

        let markup = map_destination_list(&[destination], None, "/locations/settlement/test/map")
            .into_string();

        assert!(markup.contains("destination-quest-badge"));
        assert!(markup.contains("aria-label=\"Active quest destination\""));
        assert!(markup.contains("title=\"A camp beside the road.\nActive quest\""));
        assert!(!markup.contains("destination-turn-in-badge"));
    }

    #[test]
    fn available_quest_destination_has_gold_status_badge() {
        let mut destination = quest_destination();
        destination.quest_in_progress = false;
        destination.open_quest_available = true;

        let markup = map_destination_list(&[destination], None, "/locations/settlement/test/map")
            .into_string();

        assert!(markup.contains("destination-open-quest-badge"));
        assert!(markup.contains("Open quest available here"));
        assert!(!markup.contains("destination-quest-badge"));
    }

    #[test]
    fn completed_active_quest_destination_remains_red() {
        let mut destination = quest_destination();
        destination.quest_in_progress = false;
        destination.turn_in_ready = true;
        destination.open_quest_available = true;

        let markup = map_destination_list(&[destination], None, "/locations/settlement/test/map")
            .into_string();

        assert!(markup.contains("destination-quest-badge"));
        assert!(markup.contains("Active quest ready to turn in here"));
        assert!(!markup.contains("destination-open-quest-badge"));
        assert!(!markup.contains("destination-turn-in-badge"));
    }

    #[test]
    fn current_settlement_with_open_quest_has_non_traveling_gold_marker() {
        let markup = map_destination_list_with_context(
            &[],
            None,
            "/locations/settlement/market/map",
            Some(MapCurrentLocation {
                name: "Market",
                open_quest_available: true,
                turn_in_ready: false,
            }),
            None,
            None,
        )
        .into_string();

        assert!(markup.contains("current-location-row"));
        assert!(markup.contains("aria-current=\"location\""));
        assert!(markup.contains("destination-open-quest-badge"));
        assert!(!markup.contains("href="));
    }

    #[test]
    fn completed_active_quest_at_current_issuer_is_red_even_when_open_quest_exists() {
        let markup = map_destination_list_with_context(
            &[],
            None,
            "/locations/settlement/issuer/map",
            Some(MapCurrentLocation {
                name: "Issuer",
                open_quest_available: true,
                turn_in_ready: true,
            }),
            None,
            None,
        )
        .into_string();

        assert!(markup.contains("destination-quest-badge"));
        assert!(markup.contains("Active quest ready to turn in here"));
        assert!(!markup.contains("destination-open-quest-badge"));
    }

    #[test]
    fn map_exposes_abandon_action_for_an_eligible_active_quest() {
        let markup = map_destination_list_with_context(
            &[],
            None,
            "/locations/settlement/issuer/map",
            Some(MapCurrentLocation {
                name: "Issuer",
                open_quest_available: false,
                turn_in_ready: false,
            }),
            Some(MapAbandonableQuest {
                id: "active",
                title: "Drive off the bandits",
            }),
            None,
        )
        .into_string();

        assert!(markup.contains("Active quest: "));
        assert!(markup.contains("Drive off the bandits"));
        assert!(markup.contains("action=\"/quests/active/abandon\""));
        assert!(markup.contains("Abandon active quest"));
    }

    #[test]
    fn map_rest_menu_is_pinned_below_the_destination_list() {
        let markup = map_destination_list_with_rest(
            &[],
            None,
            "/locations/quest/active/map",
            html! { section class="rest-service-menu" { "Rest party" } },
        )
        .into_string();

        assert!(markup.contains("left-sidebar map-rest-sidebar"));
        assert!(markup.contains("map-rest-sidebar-content"));
        assert!(markup.contains("rest-service-menu"));
        assert!(markup.contains("Rest party"));
    }

    #[test]
    fn quest_location_travel_has_one_plain_action_without_settlement_buying() {
        let destination = quest_destination();
        let markup = map_destination_detail(
            Some(&destination),
            None,
            None,
            false,
            true,
            false,
            None,
            None,
            false,
            None,
            "/map",
        )
        .into_string();

        assert!(markup.contains("Begin journey"));
        assert!(!markup.contains("<p>A camp beside the road.</p>"));
        assert!(!markup.contains("Active quest"));
        assert!(!markup.contains("name=\"provisioning\""));
        assert!(!markup.contains("data-provision-buy"));
    }

    #[test]
    fn nonconnected_map_selection_has_detail_but_no_travel_form() {
        let mut destination = settlement();
        destination.id = "viabundus-99".into();
        destination.name = "Distant town".into();
        let markup = map_destination_detail(
            None,
            Some(&destination),
            None,
            false,
            true,
            true,
            None,
            None,
            false,
            None,
            "/locations/settlement/viabundus-1/map",
        )
        .into_string();

        assert!(markup.contains("Distant town"));
        assert!(markup.contains("No direct route."));
        assert!(!markup.contains("Begin journey"));
        assert!(!markup.contains("data-travel-submit"));
    }

    #[test]
    fn available_quest_selection_has_detail_but_no_travel_form() {
        let quest = Quest {
            id: "quest-bandits".into(),
            title: "Drive off the bandits".into(),
            description: "Bandits have occupied the old watchtower.".into(),
            difficulty: 2,
            gold_reward: 50,
            xp_reward: 20,
            settlement_id: "viabundus-1".into(),
            status: crate::spacetimedb::QuestStatus::Available,
            accepted_by: None,
            enemy_type: "bandit".into(),
            enemy_count: 4,
            location_description: "An abandoned tower beyond the fields.".into(),
            location_scene_key: "watchtower".into(),
            location_coord_x: 10.2,
            location_coord_y: 53.1,
            coordinates_are_geographic: true,
            distance_m: 8_000,
        };
        let markup = map_destination_detail(
            None,
            None,
            Some(&quest),
            false,
            false,
            false,
            None,
            None,
            false,
            None,
            "/locations/settlement/viabundus-1/map",
        )
        .into_string();

        assert!(markup.contains("Drive off the bandits"));
        assert!(markup.contains("Bandits have occupied the old watchtower."));
        assert!(markup.contains("An abandoned tower beyond the fields."));
        assert!(markup.contains("Quest destination."));
        assert!(!markup.contains("Begin journey"));
        assert!(!markup.contains("data-travel-submit"));
    }

    #[test]
    fn nontravelable_active_quest_selection_uses_its_actual_status() {
        let mut quest = Quest {
            id: "quest-bandits".into(),
            title: "Drive off the bandits".into(),
            description: "Bandits have occupied the old watchtower.".into(),
            difficulty: 2,
            gold_reward: 50,
            xp_reward: 20,
            settlement_id: "viabundus-1".into(),
            status: QuestStatus::Accepted,
            accepted_by: Some("party-1".into()),
            enemy_type: "bandit".into(),
            enemy_count: 4,
            location_description: "An abandoned tower beyond the fields.".into(),
            location_scene_key: "watchtower".into(),
            location_coord_x: 10.2,
            location_coord_y: 53.1,
            coordinates_are_geographic: true,
            distance_m: 8_000,
        };

        let accepted = map_destination_detail(
            None,
            None,
            Some(&quest),
            false,
            false,
            false,
            None,
            None,
            false,
            None,
            "/locations/settlement/viabundus-1/map",
        )
        .into_string();
        assert!(accepted.contains("Active quest destination."));
        assert!(!accepted.contains("Accept and activate"));

        quest.status = QuestStatus::Completed;
        let completed = map_destination_detail(
            None,
            None,
            Some(&quest),
            false,
            false,
            false,
            None,
            None,
            false,
            None,
            "/locations/settlement/viabundus-1/map",
        )
        .into_string();
        assert!(completed.contains("Quest completed."));
        assert!(completed.contains("Return to the issuing settlement"));
        assert!(!completed.contains("Accept and activate"));
    }

    #[test]
    fn connected_settlement_selection_keeps_existing_travel_action() {
        let mut destination = quest_destination();
        destination.id = "viabundus-2".into();
        destination.name = "Connected town".into();
        destination.quest_in_progress = false;
        destination.travel_action = "/settlements/viabundus-2/travel".into();
        let markup = map_destination_detail(
            Some(&destination),
            None,
            None,
            false,
            true,
            true,
            None,
            None,
            false,
            None,
            "/locations/settlement/viabundus-1/map",
        )
        .into_string();

        assert!(markup.contains("action=\"/settlements/viabundus-2/travel\""));
        assert!(markup.contains("data-travel-submit"));
        assert!(markup.contains("Begin journey"));
        assert!(!markup.contains("No direct route"));
    }

    #[test]
    fn chat_uses_one_stream_with_all_channel_filters() {
        let markup = chat_area("Lubeck", None, None, None, &[]).into_string();

        assert!(!markup.contains("role=\"tablist\""));
        for channel in ["local", "party", "settlement", "dm", "guild", "info"] {
            assert!(
                markup.contains(&format!("data-chat-filter=\"{channel}\"")),
                "missing {channel} filter"
            );
        }
        assert!(markup.contains("data-chat-channel=\"info\""));
        assert!(!markup.contains("chat-channel-badge"));
        assert!(markup.contains("class=\"settlement-chat-layout\""));
        assert!(markup.contains("data-dialogue-topic-pane"));
        assert!(markup.contains("data-dialogue-topic-list"));
        assert!(markup.contains("data-dialogue-completion"));
        assert!(markup.contains("autocomplete=\"off\""));
        for label in ["Local", "Party", "Settlement", "DMs", "Guild", "Info"] {
            assert!(markup.contains(&format!("aria-label=\"{label}\" title=\"{label}\"")));
            assert!(!markup.contains(&format!(">{label}</")));
        }
    }

    #[test]
    fn chat_palette_meets_contrast_across_every_supported_theme() {
        fn linear_channel(channel: u8) -> f64 {
            let channel = f64::from(channel) / 255.0;
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        }

        fn luminance([red, green, blue]: [u8; 3]) -> f64 {
            0.2126 * linear_channel(red)
                + 0.7152 * linear_channel(green)
                + 0.0722 * linear_channel(blue)
        }

        fn contrast(first: [u8; 3], second: [u8; 3]) -> f64 {
            let (lighter, darker) = if luminance(first) > luminance(second) {
                (luminance(first), luminance(second))
            } else {
                (luminance(second), luminance(first))
            };
            (lighter + 0.05) / (darker + 0.05)
        }

        fn mix(accent: [u8; 3], text: [u8; 3], accent_percent: u16) -> [u8; 3] {
            std::array::from_fn(|index| {
                let mixed = u16::from(accent[index]) * accent_percent
                    + u16::from(text[index]) * (100 - accent_percent);
                ((mixed + 50) / 100) as u8
            })
        }

        // Dark themes use the lightest possible 88% panel composite (over
        // white); light themes use the darkest possible composite (over
        // black). This brackets the image content beneath the translucent chat.
        let legacy_palettes = [
            (
                "Dark Arcanum",
                [46, 49, 67],
                [200, 202, 208],
                [154, 158, 176],
                [96, 165, 250],
                [251, 191, 36],
                [215, 169, 239],
                [52, 211, 153],
            ),
            (
                "Fraktur Nocturne",
                [60, 49, 44],
                [241, 227, 207],
                [205, 185, 157],
                [125, 159, 197],
                [213, 166, 76],
                [213, 167, 237],
                [120, 173, 114],
            ),
            (
                "Fraktur Texturina",
                [216, 209, 190],
                [42, 31, 20],
                [74, 60, 44],
                [58, 106, 138],
                [184, 134, 11],
                [116, 66, 141],
                [74, 124, 63],
            ),
            (
                "Imperial Crimson",
                [217, 213, 204],
                [26, 26, 26],
                [61, 61, 61],
                [26, 74, 138],
                [196, 136, 11],
                [113, 63, 140],
                [45, 106, 48],
            ),
            (
                "Northern Frost",
                [211, 215, 220],
                [28, 40, 51],
                [52, 73, 94],
                [46, 109, 164],
                [212, 160, 23],
                [115, 66, 147],
                [39, 174, 96],
            ),
            (
                "Renaissance Gold",
                [216, 209, 190],
                [42, 31, 20],
                [74, 60, 44],
                [58, 106, 138],
                [184, 134, 11],
                [123, 63, 145],
                [74, 124, 63],
            ),
            (
                "Verdant Chronicle",
                [218, 214, 202],
                [26, 60, 26],
                [45, 90, 45],
                [74, 122, 106],
                [184, 115, 51],
                [116, 66, 141],
                [58, 122, 58],
            ),
        ];
        for (palette, surface, primary, secondary, info, gold, dm, success) in legacy_palettes {
            let channels = [
                ("Local", primary),
                ("Party", mix(info, primary, 40)),
                ("Settlement", mix(gold, primary, 35)),
                ("DM", mix(dm, primary, 35)),
                ("Guild", mix(success, primary, 40)),
                ("Info", secondary),
            ];
            let distinct = channels
                .iter()
                .map(|(_, color)| color)
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(
                distinct.len(),
                channels.len(),
                "{palette} channels must remain visually distinct"
            );
            for (channel, color) in channels {
                assert!(
                    contrast(color, surface) >= 4.5,
                    "{palette} {channel} does not meet WCAG AA text contrast"
                );
            }
        }
    }

    #[test]
    fn chat_css_keeps_fallbacks_and_mobile_message_space() {
        let css = include_str!("../../static/css/strategic.css");
        let utilities = include_str!("../../static/css/utilities.css");
        let trade_script = include_str!("../../static/party-trade.js");
        let fallback = css
            .find("background: rgb(33 21 15 / 88%);")
            .expect("chat needs a background fallback");
        let enhanced = css
            .find("background: color-mix(in srgb, var(--panel-bg) 88%, transparent);")
            .expect("chat should derive its translucent surface from the fixed palette");

        assert!(fallback < enhanced);
        assert!(css.contains("background: color-mix(in srgb, var(--header-bg) 86%, transparent);"));
        assert!(css.contains(".chat-channel-filter input::after"));
        assert!(css.contains("text-decoration: line-through"));
        assert!(css.contains("outline: 2px solid var(--text-primary);"));
        assert!(css.contains("0 0 0 2px var(--panel-bg)"));
        assert!(css.contains(
            "--chat-party-color: color-mix(in srgb, var(--info) 40%, var(--text-primary));"
        ));
        assert!(css.contains("--chat-settlement-color: color-mix(in srgb, var(--gold-color) 35%, var(--text-primary));"));
        assert!(css.contains("--chat-dm-color: color-mix(in srgb, var(--icon-instinct, #7b3f91) 35%, var(--text-primary));"));
        assert!(css.contains(
            "--chat-guild-color: color-mix(in srgb, var(--success) 40%, var(--text-primary));"
        ));
        for variable in ["local", "party", "settlement", "dm", "guild", "info"] {
            assert!(css.contains(&format!("var(--chat-{variable}-color)")));
        }
        assert!(!css.contains(".chat-channel-badge"));
        assert!(css.contains("@media (max-width: 768px)"));
        assert!(css.contains("flex-wrap: nowrap;"));
        assert!(css.contains("min-height: 10rem;"));
        assert!(css.contains(".repair-custody-list { margin-top: auto; }"));
        assert!(css.contains("max-height: 50%;"));
        assert!(css.contains("@keyframes repairable-damage-pulse"));
        assert!(css.contains("@media (prefers-reduced-motion: reduce)"));
        let repairable_rule = css
            .split(".condition-repairable {")
            .nth(1)
            .and_then(|tail| tail.split('}').next())
            .expect("repairable condition segments need a style rule");
        assert!(!repairable_rule.contains("background-image"));
        assert!(!repairable_rule.contains("box-shadow"));
        for tier in 1..=5 {
            assert!(css.contains(&format!(".condition-tier-{tier}")));
            assert!(css.contains(&format!(".item-quality-{tier}")));
        }
        assert!(css.contains("color-mix(in srgb, var(--quality-color) 50%, var(--text-primary))"));
        assert!(css.contains("filter: brightness(1.15)"));
        assert!(css.contains("0%, 58%, 82%, 100%"));
        assert!(css.contains("66%, 74%"));
        assert!(!css.contains("left: -7rem;"));
        assert!(css.contains(".smith-wares-scroll .trade-inventory-table"));
        assert!(css.contains("--inventory-merchant-action-overhang"));
        assert!(css.contains("--inventory-merchant-scrollbar-reserve: 8px;"));
        assert!(css.contains("padding-left: var(--inventory-merchant-scrollbar-reserve);"));
        assert!(css.contains("padding-right: var(--inventory-merchant-action-overhang);"));
        assert!(css.contains("direction: rtl;"));
        assert!(css.contains(".smith-wares-scroll > * { direction: ltr; }"));
        assert!(css.contains("scrollbar-gutter: stable;"));
        assert!(css.contains("overflow-x: clip;"));
        assert!(css.contains("col.inventory-column-item { width: auto; }"));
        assert!(css.contains(".smith-player-inventory-table"));
        assert!(css.contains("width: 3.65rem;"));
        assert!(css.contains("--repair-custody-action-overhang"));
        assert!(css.contains("width: calc(100% + var(--repair-custody-action-overhang));"));
        assert!(css.contains("padding-right: var(--repair-custody-action-overhang);"));
        assert!(css.contains("scrollbar-gutter: stable;"));
        assert!(utilities.contains(".inventory-row-actions.smith-player-actions"));
        assert!(utilities.contains("--inventory-action-bridge:.3rem"));
        assert!(!utilities.contains(".smith-wares-scroll .inventory-row-actions"));
        assert!(utilities.contains(".inventory-actions-cell"));
        assert!(
            utilities.contains(".left-sidebar .inventory-actions-cell > .inventory-row-actions")
        );
        assert!(
            utilities.contains(".right-sidebar .inventory-actions-cell > .inventory-row-actions")
        );
        assert!(utilities.contains("background:var(--inventory-row-background"));
        assert!(utilities.contains("top:0; bottom:0;"));
        assert!(utilities.contains(
            ".trade-inventory-row:not(:last-child) .inventory-row-actions { bottom:-1px; }"
        ));
        assert!(utilities.contains(".inventory-row-actions .trade-transfer:disabled"));
        assert!(utilities.contains("opacity:.42; transform:none;"));
        assert!(utilities.contains("left:100%; padding-left:var(--inventory-action-bridge);"));
        assert!(utilities.contains("right:100%; padding-right:var(--inventory-action-bridge);"));
        assert!(css.contains(".inventory-browser-table-frame"));
        assert!(css.contains("width:max-content;"));
        assert!(utilities.contains(".inventory-footer-repair .repair-all-button"));
        assert!(utilities.contains("grid-template-columns:repeat(2,1.35rem)"));
        assert!(utilities.contains(".inventory-actions-header > .inventory-footer-actions"));
        assert!(utilities.contains("thead:hover .inventory-footer-actions"));
        assert!(utilities.contains("background:var(--panel-bg)"));
        assert!(
            utilities
                .contains(".smith-player-actions .row-repair-form { position:static; order:0;")
        );
        assert!(trade_script.contains("if (stockRow) changeTradeDraftCount(stockRow, amount);"));
        assert!(trade_script.contains("function applyDynamicTransferModifiers(event)"));
        assert!(trade_script.contains("event.key === \"Shift\" || event.key === \"Control\""));
        assert!(trade_script.contains("controlKey ? \"all\""));
    }

    #[test]
    fn schedule_and_equipment_scripts_use_the_new_interactions() {
        let schedule = include_str!("../../static/training-schedule.js");
        let numeric = include_str!("../../static/numeric-editor.js");
        let equipment = include_str!("../../static/equipment-toggle.js");
        let live_regions = include_str!("../../static/live-regions.js");
        let immediate_activity = include_str!("../../static/immediate-activity.js");
        let css = include_str!("../../static/css/strategic.css");
        assert!(schedule.contains("function parseClock(value)"));
        assert!(schedule.contains("window.StrategicNumericEditor.open"));
        assert!(numeric.contains("input.type = 'text'"));
        assert!(numeric.contains("confirm.addEventListener('click', () => finish(true))"));
        assert!(numeric.contains("cancel.addEventListener('click', () => finish(false))"));
        assert!(numeric.contains("input.addEventListener('wheel'"));
        assert!(!numeric.contains("document.addEventListener('wheel'"));
        assert!(schedule.contains("/^\\d{3,4}$/"));
        assert!(schedule.contains("Math.round(wanted / STEP) * STEP"));
        assert!(schedule.contains("function renderActivityPreview(row, minutes)"));
        assert!(schedule.contains("function calculateLeisurePreview"));
        assert!(schedule.contains("row.dataset.leisureFatiguePreviewDivisor"));
        assert!(schedule.contains("function mountSchedules(root = document)"));
        assert!(schedule.contains("[data-social-expand]"));
        assert!(schedule.contains(".social-detail-row"));
        assert!(schedule.contains("'strategic-live-regions-refreshed'"));
        assert!(schedule.contains("event.detail.regions.includes('left-sidebar')"));
        assert!(schedule.contains("function createLatestSaveQueue(send"));
        assert!(schedule.contains("data-schedule-pending"));
        assert!(schedule.contains("retry()"));
        assert!(schedule.contains("data-schedule-save-status"));
        assert!(schedule.contains("data-schedule-retry"));
        assert!(schedule.contains("strategic-live-refresh-requested"));
        assert!(schedule.contains("schedule-effect-positive"));
        assert!(!schedule.contains("scheduleDrag"));
        assert!(!schedule.contains("travel_"));
        assert!(equipment.contains("'/api/equipment'"));
        assert!(equipment.contains("window.location.reload()"));
        assert!(live_regions.contains("const scrollOffsets = (selector)"));
        assert!(live_regions.contains("region.scrollTop = offsets.top"));
        assert!(live_regions.contains("replaced.includes(\"left-sidebar\")"));
        assert!(live_regions.contains("document.querySelector('.numeric-editor')"));
        assert!(live_regions.contains("[data-activity-modal]:not([hidden])"));
        assert!(live_regions.contains("scheduleEditorIsPending"));
        assert!(live_regions.contains("const schedulePendingAtStart = scheduleEditorIsPending()"));
        assert!(live_regions.contains("!schedulePendingAtStart && !scheduleEditorIsPending()"));
        assert!(immediate_activity.contains("typeof window === 'undefined'"));
        assert!(immediate_activity.contains("input:not([type=\"hidden\"]):not(:disabled)"));
        assert!(immediate_activity.contains("wrappedFocusTarget"));
        assert!(immediate_activity.contains("strategic-editor-idle"));
        assert!(css.contains(".numeric-editor-input {"));
        assert!(css.contains("position: fixed;"));
        assert!(css.contains("z-index: 80;"));
        assert!(css.contains(".numeric-editor {"));
        assert!(css.contains("right: auto;"));
        assert!(css.contains("left: 50%;"));
        assert!(css.contains("transform: translate(-50%, -50%);"));
        assert!(css.contains(".numeric-editor-input::selection {"));
        assert!(numeric.contains("document.body.append(editor)"));
        assert!(numeric.contains("display.style.visibility = 'hidden'"));
        assert!(!numeric.contains("display.hidden = true"));
        assert!(numeric.contains("window.addEventListener('resize', positionEditor)"));
        assert!(!css.contains(".party-skill-icon-column"));
        assert!(css.contains(".numeric-editor-action {"));
        assert!(css.contains(".numeric-editor-confirm { background: #2f7d3d; }"));
        assert!(css.contains(".numeric-editor-cancel { background: #9c3434; }"));
        assert!(css.contains(".schedule-save-status"));
    }

    #[test]
    fn low_medicine_medical_html_contains_no_hidden_payload() {
        let presentation = crate::medical::MedicalPresentation {
            unavailable: false,
            symptoms: vec!["coughing"],
            diagnoses: Vec::new(),
            ..Default::default()
        };
        let markup = medical_rail(&presentation, "/location", 1, 2, true).into_string();
        assert!(markup.contains("coughing"));
        assert!(!markup.contains("Examine"));
        assert!(!markup.contains("Visible injuries"));
        for forbidden in ["Vitals", "influenza", "infection_id", "disease", "humour-"] {
            assert!(!markup.contains(forbidden), "leaked {forbidden}: {markup}");
        }
    }

    #[test]
    fn active_medication_is_listed_beneath_symptoms() {
        let presentation = crate::medical::MedicalPresentation {
            symptoms: vec!["coughing"],
            medications: vec![crate::medical::MedicationPresentation {
                equipment_id: 11,
                disease_name: "Consumption",
            }],
            ..Default::default()
        };
        let markup = medical_rail(&presentation, "/location", 1, 2, false).into_string();
        let symptoms_at = markup.find("coughing").unwrap();
        let medication_at = markup.find("Taking medication for Consumption.").unwrap();
        assert!(medication_at > symptoms_at);
    }

    #[test]
    fn medicine_action_moves_from_portrait_to_the_skill_icon() {
        let doctor = Character {
            id: 1,
            name: "Doctor".into(),
            xp: 0,
            level: 1,
            gold: 100,
            current_settlement_id: Some("willowmere".into()),
            current_quest_location_id: None,
            party_id: Some("demo".into()),
            age_years: 30,
            alive: true,
            temporary: false,
        };
        let portrait = party_portrait_overlay(
            &[doctor.clone()],
            Some(&doctor),
            "/locations/settlement/willowmere",
            Some(1),
            true,
        )
        .into_string();
        assert!(!portrait.contains("/examine"));
        assert!(!portrait.contains("party-medical-examine"));
        assert!(portrait.contains("party-alchemy-action"));

        let skill = skill_action_icon(
            "Medicine",
            "medicine",
            SkillAction::Post {
                href: "/place/party/1/examine",
                label: "Perform medical examination (15 minutes)",
                open: false,
            },
            false,
        )
        .into_string();
        assert!(skill.contains("/place/party/1/examine"));
        assert!(skill.contains("Perform medical examination (15 minutes)"));
        assert!(skill.contains("aria-haspopup=\"dialog\" aria-expanded=\"false\""));
    }

    #[test]
    fn pending_examination_is_a_one_shot_center_popup_not_sidebar_history() {
        let presentation = crate::medical::MedicalPresentation {
            findings: vec!["coughing".into(), "fatigued".into()],
            examination_id: Some(44),
            examined_at: Some(8_640),
            regional_humours: Some(
                [crate::medical::HumourVitals {
                    sanguine: 0.9,
                    phlegmatic: 0.6,
                    choleric: 0.8,
                    melancholic: 1.0,
                }; 7],
            ),
            possible_diagnoses: vec!["Catarrhal fever", "Consumption"],
            ..Default::default()
        };
        let sidebar = medical_rail(&presentation, "/location", 1, 2, true).into_string();
        assert!(!sidebar.contains("Four humours"));
        assert!(!sidebar.contains("Possible ailments"));
        assert!(!sidebar.contains("Observed at personal minute"));

        let location = LocationView {
            kind: LocationKind::Quest,
            id: "location".into(),
            name: "Location".into(),
            religion_id: None,
            category: None,
            active_building: Some("inn".into()),
        };
        let popup =
            medical_examination_popup(&presentation, &location, 2, None, &[], &[]).into_string();
        assert!(popup.contains("medical-examination-overlay"));
        assert!(popup.contains("aria-modal=\"true\""));
        assert!(popup.contains("Possible ailments"));
        assert!(popup.contains("Body regions"));
        assert!(popup.contains("attribute-health-phlegmatic"));
        assert!(popup.contains("Catarrhal fever"));
        assert!(popup.contains("Close examination findings"));
        assert!(popup.contains('×'));
        assert!(!popup.contains("This result is discarded"));
        assert!(popup.contains("/examination/44/dismiss"));
        assert!(popup.contains("?building=inn"));
        assert!(popup.contains("data-medical-examination"));
        let lifecycle = include_str!("../../static/medical-examination.js");
        assert!(lifecycle.contains("pagehide"));
        assert!(lifecycle.contains("navigator.sendBeacon"));
        assert!(lifecycle.contains("event.key !== \"Escape\""));
        assert!(lifecycle.contains("restoreFocus"));
        assert!(lifecycle.contains(".party-offer[role='dialog']"));
    }

    #[test]
    fn examined_region_meter_has_text_and_aria_not_color_alone() {
        let presentation = crate::medical::MedicalPresentation {
            regional_humours: Some(
                [crate::medical::HumourVitals {
                    phlegmatic: 0.4,
                    ..Default::default()
                }; 7],
            ),
            ..Default::default()
        };
        let markup = regional_health_bar("Chest", 1.0, &presentation, 4, &[], &[]).into_string();
        assert!(markup.contains("Phlegmatic"));
        assert!(markup.contains("role=\"meter\""));
        assert!(markup.contains("Chest:"));
    }

    #[test]
    fn surgery_button_is_explicit_and_the_health_meter_is_not_clickable() {
        let markup = attribute_group(
            "Head",
            "head",
            0.75,
            &crate::medical::MedicalPresentation::default(),
            6,
            Some(("/place/party/1/surgery", None)),
            &[],
            &[],
            &[("Intelligence", "intelligence", 3.0)],
        )
        .into_string();

        let link_start = markup.find("limb-surgery-button").unwrap();
        let health_bar = markup.find("class=\"attribute-health-bar\"").unwrap();
        let link_end = markup[link_start..].find("</a>").unwrap() + link_start;
        let attribute_row = markup.find("class=\"party-attribute-row\"").unwrap();

        assert!(link_start < link_end);
        assert!(link_end < health_bar);
        assert!(link_end < attribute_row);
        assert!(markup.contains("aria-label=\"Open surgery menu for Head\""));
        assert!(markup.contains("aria-haspopup=\"dialog\" aria-expanded=\"false\""));
    }

    #[test]
    fn treated_cuts_and_fractures_expose_banded_health_bar_states() {
        let injury = LimbInjury {
            id: "1:chest".into(),
            character_id: 1,
            limb: LimbRegion::Chest,
            cut_damage: 0.2,
            bruise_damage: 0.2,
            fracture_damage: 0.2,
            bandaged: true,
            stitched: false,
            stitch_quality: 0.0,
            splint_owner_id: Some(2),
            splint_inventory_item_id: Some(3),
            infection_exposure: 0.0,
            infection_checks: 0,
            infection_origin_minute: None,
        };
        let markup = regional_health_bar(
            "Chest",
            0.6,
            &crate::medical::MedicalPresentation::default(),
            4,
            &[injury],
            &[],
        )
        .into_string();

        assert!(markup.contains("attribute-health-cut bandaged-cut"));
        assert!(markup.contains("title=\"Bandaged cut damage\""));
        assert!(markup.contains("attribute-health-fracture splinted-fracture"));
        assert!(markup.contains("title=\"Splinted fracture\""));
        assert!(markup.contains("20% splinted fracture"));
    }

    #[test]
    fn persisted_quest_camp_keeps_turnaround_movement_after_elapsed_rest() {
        let mut journey = PartyJourney {
            party_id: "party".into(),
            origin_kind: "settlement".into(),
            origin_id: "home".into(),
            origin_name: "Home".into(),
            destination_kind: "quest".into(),
            destination_id: "quest".into(),
            destination_name: "Quest".into(),
            total_minutes: 720,
            completed_minutes: 480,
            camp_stop_minutes: vec![480],
            forecast_camp_stop_minutes: vec![480],
            fatigue_percent: 50,
            plan_version: 1,
            departure_minute: 10_000,
            total_elapsed_minutes: 2_040,
            completed_elapsed_minutes: 780,
            walking_minutes_per_day: 480,
            travel_at_night: false,
            camp_duration_mode: crate::spacetimedb::CampDurationMode::Auto,
            fixed_camp_minutes: 0,
        };
        let camp = |start, duration, from, to| crate::spacetimedb::JourneyCampInterval {
            movement_minute: 480,
            elapsed_start_minute: start,
            elapsed_minutes: duration,
            average_fatigue_start: from,
            average_fatigue_end: to,
            maximum_fatigue_end: to,
        };
        let itinerary = PartyJourneyItinerary {
            party_id: "party".into(),
            actual_camp_intervals: vec![camp(480, 300, 0.5, 0.2)],
            forecast_camp_intervals: vec![camp(780, 300, 0.2, 0.0)],
        };
        assert!(
            !camp_fire_is_lit(Some(&journey), Some(&itinerary)),
            "resting at the current movement checkpoint leaves smoke-only embers"
        );
        journey.completed_minutes = 600;
        assert!(
            camp_fire_is_lit(Some(&journey), Some(&itinerary)),
            "reaching a later camp relights the fire"
        );
        journey.completed_minutes = 480;
        let encoded = format_persisted_itinerary(&journey, &itinerary);
        assert!(encoded.contains("w,0,480,0,480"));
        assert!(encoded.contains("m,480,600,480,0"));
        assert!(encoded.contains("w,1080,960,480,960"));
        assert_eq!(
            encoded
                .split('|')
                .filter(|segment| segment.starts_with("m,"))
                .count(),
            1,
            "one physical camp marker"
        );
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
    }

    #[test]
    fn responsive_and_hidden_control_rules_keep_content_available() {
        let layout = include_str!("../../static/css/layout.css");
        let strategic = include_str!("../../static/css/strategic.css");
        let utilities = include_str!("../../static/css/utilities.css");
        assert!(layout.contains("grid-template-areas: \"main\" \"left\" \"right\""));
        assert!(layout.contains(".right-sidebar {\n    display: block;"));
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
