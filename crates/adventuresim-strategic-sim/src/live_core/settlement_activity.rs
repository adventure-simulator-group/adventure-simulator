//! Settlement activity services, observations, and public inventory helpers.

use super::*;

pub(super) fn settlement_action_service_label(
    service: DomainSettlementActionService,
) -> &'static str {
    match service {
        DomainSettlementActionService::Inn => "inn",
        DomainSettlementActionService::Temple => "temple",
    }
}

pub(super) fn settlement_service_key(service: SettlementService) -> &'static str {
    match service {
        SettlementService::GeneralStore => "GeneralStore",
        SettlementService::Inn => "Inn",
        SettlementService::GeneralBlacksmith => "GeneralBlacksmith",
        SettlementService::Market => "Market",
        SettlementService::Weaponsmith => "Weaponsmith",
        SettlementService::Armorer => "Armorer",
        SettlementService::Tailor => "Tailor",
        SettlementService::Herbalist => "Herbalist",
        SettlementService::Temple => "Temple",
        SettlementService::Bookstore => "Bookstore",
    }
}

pub(super) fn death_cause_key(cause: DeathCause) -> &'static str {
    match cause {
        DeathCause::Combat => "Combat",
        DeathCause::Injury => "Injury",
        DeathCause::Disease => "Disease",
        DeathCause::RespiratoryFailure => "RespiratoryFailure",
        DeathCause::CirculatoryFailure => "CirculatoryFailure",
        DeathCause::HomeostaticFailure => "HomeostaticFailure",
        DeathCause::NeurologicFailure => "NeurologicFailure",
        DeathCause::Starvation => "Starvation",
        DeathCause::Dehydration => "Dehydration",
        DeathCause::Other => "Other",
        DeathCause::DevTest => "DevTest",
    }
}

pub(super) fn activity_preference_key(preference: ActivityPreference) -> &'static str {
    match preference {
        ActivityPreference::Labor => "Labor",
        ActivityPreference::Prayer => "Prayer",
        ActivityPreference::Thievery => "Thievery",
        ActivityPreference::Raiding => "Raiding",
    }
}

pub(super) fn stdb_settlement_action_service(
    service: DomainSettlementActionService,
) -> adventuresim_stdb_client::SettlementActionService {
    match service {
        DomainSettlementActionService::Inn => {
            adventuresim_stdb_client::SettlementActionService::Inn
        }
        DomainSettlementActionService::Temple => {
            adventuresim_stdb_client::SettlementActionService::Temple
        }
    }
}

pub(super) fn select_settlement_activity_venue(
    inn_available: bool,
    temple_available: bool,
    temple_food_covers_day: bool,
    purse: u64,
    committed_reserve: u64,
    inn_cost: Option<u64>,
) -> Option<DomainSettlementActionService> {
    if temple_available && temple_food_covers_day {
        return Some(DomainSettlementActionService::Temple);
    }
    if inn_available && inn_cost.is_some_and(|cost| purse >= committed_reserve.saturating_add(cost))
    {
        return Some(DomainSettlementActionService::Inn);
    }
    None
}

pub(super) fn select_generated_travel_action<'a>(
    profile: &AgentProfile,
    actions: &'a mut [BackendInvestigationAction],
    mut action_safe: impl FnMut(&BackendInvestigationAction) -> bool,
) -> Option<&'a BackendInvestigationAction> {
    sort_generated_actions(profile, actions);
    actions.iter().find(|action| {
        projected_investigation_action_state(&action.availability)
            == ProjectedInvestigationActionState::Travel
            && action_safe(action)
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PublicNpcCandidate {
    pub(super) resident_character_id: u64,
    pub(super) name: String,
    pub(super) profession: String,
    pub(super) conversation_id: String,
    pub(super) location_id: String,
}

pub(super) fn public_settlement_economy_profile(
    profile: &SettlementEconomyProfile,
) -> Option<adventuresim_world_schema::SettlementEconomyProfile> {
    use adventuresim_world_schema as world;

    // NPC tab visibility depends only on the canonical service set. Build the
    // smallest shared profile that preserves those inputs instead of trying to
    // serde-bridge SpacetimeDB's SATS-only generated client types.
    let mut navigability_profile = world::SettlementEconomyProfile::stage_placeholder();
    navigability_profile.rules_version = profile.rules_version;
    navigability_profile.prosperity_score = profile.prosperity_score;
    navigability_profile.services = profile
        .services
        .iter()
        .map(|service| match service {
            SettlementService::GeneralStore => world::SettlementService::GeneralStore,
            SettlementService::Inn => world::SettlementService::Inn,
            SettlementService::GeneralBlacksmith => world::SettlementService::GeneralBlacksmith,
            SettlementService::Market => world::SettlementService::Market,
            SettlementService::Weaponsmith => world::SettlementService::Weaponsmith,
            SettlementService::Armorer => world::SettlementService::Armorer,
            SettlementService::Tailor => world::SettlementService::Tailor,
            SettlementService::Herbalist => world::SettlementService::Herbalist,
            SettlementService::Temple => world::SettlementService::Temple,
            SettlementService::Bookstore => world::SettlementService::Bookstore,
        })
        .collect();
    navigability_profile.specializations = profile
        .specializations
        .iter()
        .copied()
        .map(public_stock_category)
        .collect();
    navigability_profile.stock = profile
        .stock
        .iter()
        .map(|stock| adventuresim_world_schema::SettlementStock {
            category: public_stock_category(stock.category),
            abundance: stock.abundance,
            provenance: adventuresim_world_schema::ProfileFactProvenance::DeterministicGapFill,
        })
        .collect();

    navigability_profile
        .validate()
        .ok()
        .map(|()| navigability_profile)
}

pub(super) fn public_stock_category(
    category: StockCategory,
) -> adventuresim_world_schema::StockCategory {
    use adventuresim_world_schema::StockCategory as World;
    match category {
        StockCategory::Grain => World::Grain,
        StockCategory::Dairy => World::Dairy,
        StockCategory::Meat => World::Meat,
        StockCategory::Fish => World::Fish,
        StockCategory::Cloth => World::Cloth,
        StockCategory::Hides => World::Hides,
        StockCategory::Timber => World::Timber,
        StockCategory::Fuel => World::Fuel,
        StockCategory::Stone => World::Stone,
        StockCategory::Pottery => World::Pottery,
        StockCategory::Salt => World::Salt,
        StockCategory::Metalwares => World::Metalwares,
        StockCategory::Weapons => World::Weapons,
        StockCategory::Armor => World::Armor,
        StockCategory::Herbs => World::Herbs,
        StockCategory::GeneralGoods => World::GeneralGoods,
        StockCategory::Books => World::Books,
    }
}

pub(super) fn public_economy_catalog_kind(
    kind: PersistedItemKind,
) -> adventuresim_core::settlement_economy::CatalogKind {
    use adventuresim_core::settlement_economy::CatalogKind as Catalog;
    match kind {
        PersistedItemKind::Simple | PersistedItemKind::Container => Catalog::Simple,
        PersistedItemKind::Weapon => Catalog::Weapon,
        PersistedItemKind::Armor => Catalog::Armor,
        PersistedItemKind::Shield => Catalog::Shield,
        PersistedItemKind::Clothing => Catalog::Clothing,
        PersistedItemKind::Currency => Catalog::Currency,
        PersistedItemKind::Ingredient => Catalog::Ingredient,
        PersistedItemKind::Medication => Catalog::Medication,
        PersistedItemKind::Food => Catalog::Food,
    }
}

pub(super) fn public_storefront_available(
    profile: &SettlementEconomyProfile,
    storefront: adventuresim_core::settlement_economy::Storefront,
) -> bool {
    public_settlement_economy_profile(profile).is_some_and(|profile| {
        adventuresim_core::settlement_economy::storefront_available(&profile, storefront)
    })
}

pub(super) fn public_storefront_stocks(
    profile: &SettlementEconomyProfile,
    storefront: adventuresim_core::settlement_economy::Storefront,
    item: &Item,
) -> bool {
    public_settlement_economy_profile(profile).is_some_and(|profile| {
        adventuresim_core::settlement_economy::storefront_stocks(
            &profile,
            storefront,
            &item.id,
            public_economy_catalog_kind(item.kind),
        )
    })
}

pub(super) fn storefront_offer_unchanged(
    selected: &(String, u64, u64),
    current: Option<(String, u64, u64)>,
) -> bool {
    current.as_ref() == Some(selected)
}

pub(super) fn visible_unique_default_provider(
    providers: &[(u64, u16, u16, bool, bool)],
    minute: u64,
) -> Option<u64> {
    let [(provider, start_minute, end_minute, context_suppressed, health_suppressed)] = providers
    else {
        return None;
    };
    npc_is_publicly_present(
        *start_minute,
        *end_minute,
        *context_suppressed,
        *health_suppressed,
        minute,
    )
    .then_some(*provider)
}

pub(super) fn retain_navigable_public_npc_candidates(
    candidates: Vec<PublicNpcCandidate>,
    profile: &adventuresim_world_schema::SettlementEconomyProfile,
    has_keep: bool,
    settlement_id: &str,
) -> Vec<PublicNpcCandidate> {
    candidates
        .into_iter()
        .filter(|candidate| {
            adventuresim_core::settlement_economy::npc_location_is_navigable(
                profile,
                has_keep,
                settlement_id,
                &candidate.location_id,
            )
        })
        .collect()
}

pub(super) fn signed_delta(after: u64, before: u64) -> String {
    if after >= before {
        format!("+{}", after - before)
    } else {
        format!("-{}", before - after)
    }
}

pub(super) fn signed_float_delta(after: f32, before: f32) -> String {
    format!("{:+.3}", after - before)
}

pub(super) fn bounded_event_field(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else if character == ';' {
                ','
            } else {
                character
            }
        })
        .take(240)
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ActivityPlanDiagnostic<'a> {
    pub(super) preferred_activity: &'a str,
    pub(super) effective_activity: &'a str,
    pub(super) schedule: &'a ScheduleAllocation,
    pub(super) fallback_reason: &'a str,
    pub(super) committed_reserve: u64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ActivityExecutionDiagnostic<'a> {
    pub(super) plan: ActivityPlanDiagnostic<'a>,
    pub(super) venue: DomainSettlementActionService,
}

pub(super) fn format_activity_detail(
    diagnostic: ActivityExecutionDiagnostic<'_>,
    before: &ActivityObservation,
    after: &ActivityObservation,
) -> String {
    let ActivityExecutionDiagnostic {
        plan:
            ActivityPlanDiagnostic {
                preferred_activity,
                effective_activity,
                schedule,
                fallback_reason,
                committed_reserve,
            },
        venue,
    } = diagnostic;
    format!(
        "outcome=completed;preferred={preferred_activity};effective={effective_activity};fallback={fallback_reason};venue={};committed_reserve={committed_reserve};schedule=combat:{},carousing:{},apprenticeship:{},profession:{},labor:{},prayer:{},thievery:{},raiding:{};purse_before={};purse_after={};purse_delta={};condition_before={};condition_after={};hunger_before={:.3};hunger_after={:.3};hunger_delta={};thirst_before={:.3};thirst_after={:.3};thirst_delta={};food_kcal_before={:.0};food_kcal_after={:.0};food_kcal_delta={};water_ml_before={:.0};water_ml_after={:.0};water_ml_delta={};elapsed_before={};elapsed_after={};elapsed_delta={}",
        settlement_action_service_label(venue),
        schedule.combat_training_minutes,
        schedule.carousing_minutes,
        schedule.apprenticeship_minutes,
        schedule.profession_practice_minutes,
        schedule.labor_minutes,
        schedule.prayer_minutes,
        schedule.thievery_minutes,
        schedule.raiding_minutes,
        before.personal_gold_coin,
        after.personal_gold_coin,
        signed_delta(after.personal_gold_coin, before.personal_gold_coin),
        before.condition_status,
        after.condition_status,
        before.hunger,
        after.hunger,
        signed_float_delta(after.hunger, before.hunger),
        before.thirst,
        after.thirst,
        signed_float_delta(after.thirst, before.thirst),
        before.visible_food_kcal,
        after.visible_food_kcal,
        signed_float_delta(after.visible_food_kcal, before.visible_food_kcal),
        before.visible_water_ml,
        after.visible_water_ml,
        signed_float_delta(after.visible_water_ml, before.visible_water_ml),
        before.elapsed_minutes,
        after.elapsed_minutes,
        signed_delta(after.elapsed_minutes, before.elapsed_minutes),
    )
}

pub(super) fn format_failed_activity_detail(
    diagnostic: ActivityExecutionDiagnostic<'_>,
    before: &ActivityObservation,
    error_category: &str,
) -> String {
    let ActivityExecutionDiagnostic {
        plan:
            ActivityPlanDiagnostic {
                preferred_activity,
                effective_activity,
                schedule,
                fallback_reason,
                committed_reserve,
            },
        venue,
    } = diagnostic;
    format!(
        "outcome=failed;stage=rest_at_settlement;error_category={error_category};preferred={preferred_activity};effective={effective_activity};fallback={fallback_reason};venue={};committed_reserve={committed_reserve};schedule=combat:{},carousing:{},apprenticeship:{},profession:{},labor:{},prayer:{},thievery:{},raiding:{};requested_minutes={MINUTES_PER_DAY};purse_before={};condition_before={};hunger_before={:.3};thirst_before={:.3};food_kcal_before={:.0};water_ml_before={:.0};elapsed_before={}",
        settlement_action_service_label(venue),
        schedule.combat_training_minutes,
        schedule.carousing_minutes,
        schedule.apprenticeship_minutes,
        schedule.profession_practice_minutes,
        schedule.labor_minutes,
        schedule.prayer_minutes,
        schedule.thievery_minutes,
        schedule.raiding_minutes,
        before.personal_gold_coin,
        before.condition_status,
        before.hunger,
        before.thirst,
        before.visible_food_kcal,
        before.visible_water_ml,
        before.elapsed_minutes,
    )
}

pub(super) fn format_deferred_activity_detail(
    diagnostic: ActivityPlanDiagnostic<'_>,
    before: &ActivityObservation,
) -> String {
    let ActivityPlanDiagnostic {
        preferred_activity,
        effective_activity,
        schedule,
        fallback_reason,
        committed_reserve,
    } = diagnostic;
    format!(
        "outcome=deferred;reason=insufficient_visible_resources;preferred={preferred_activity};effective={effective_activity};fallback={fallback_reason};venue=unavailable;committed_reserve={committed_reserve};schedule=combat:{},carousing:{},apprenticeship:{},profession:{},labor:{},prayer:{},thievery:{},raiding:{};purse_before={};condition_before={};hunger_before={:.3};thirst_before={:.3};food_kcal_before={:.0};water_ml_before={:.0};elapsed_before={}",
        schedule.combat_training_minutes,
        schedule.carousing_minutes,
        schedule.apprenticeship_minutes,
        schedule.profession_practice_minutes,
        schedule.labor_minutes,
        schedule.prayer_minutes,
        schedule.thievery_minutes,
        schedule.raiding_minutes,
        before.personal_gold_coin,
        before.condition_status,
        before.hunger,
        before.thirst,
        before.visible_food_kcal,
        before.visible_water_ml,
        before.elapsed_minutes,
    )
}
