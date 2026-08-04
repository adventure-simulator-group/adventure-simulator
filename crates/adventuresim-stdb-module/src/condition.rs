use adventuresim_core::prelude::*;
use spacetimedb::{ReducerContext, Table, reducer, table};
use std::collections::BTreeMap;

use crate::capability::StrategicEquipment;
use crate::character::character;
use crate::filth::character_filth;
use crate::investigation::case_site_authority;
use crate::item::item;
use crate::strategic::{
    PartyJourneyRoute, hostile_group_authority, party_authority, party_inventory_item,
    party_journey_authority, party_journey_route_authority, settlement,
};
use crate::surgery::LimbRegion;
use crate::{
    CharacterAttributes, CharacterLimbs, CharacterSkills, CharacterStats, character_attributes,
    character_limbs, character_skills, character_stats, character_time,
    character_training_schedule, inventory_item,
};

pub const DEFAULT_BODY_WEIGHT_KG: f32 = 70.0;
pub const BLOOD_ML_PER_KG: f32 = 70.0;
pub const BLOOD_RECOVERY_FRACTION_PER_DAY: f32 = 0.01;
pub const RECENT_MORALE_DURATION_MINUTES: u64 = 7 * 24 * 60;
const LEISURE_MORALE_SOURCE_ID: &str = "settlement-leisure";
const MASTERY_MORALE_SOURCE_ID: &str = "mastery-enjoyment";
const INJURY_MORALE_PER_HEALTH_DEFICIT: f32 = 5.0;
pub const TRAVEL_CALORIES_PER_DAY: f32 = STRATEGIC_TRAVEL_KCAL_PER_DAY;
pub const TRAVEL_WATER_ML_PER_DAY: f32 = STRATEGIC_TRAVEL_WATER_ML_PER_DAY;
pub const FOOD_RESERVE_KCAL: f32 = TRAVEL_CALORIES_PER_DAY;
pub const HYDRATION_RESERVE_ML: f32 = TRAVEL_WATER_ML_PER_DAY;
pub const TRAVEL_RATION_ID: &str = STANDARD_TRAVEL_RATION_ID;
pub const WATERSKIN_ID: &str = STANDARD_WATERSKIN_ID;

fn enemy_fear_multiplier(enemy_type: &str) -> Result<f32, String> {
    enemy_type
        .parse::<adventuresim_core::bestiary::ThreatId>()
        .map(|id| 1.0 + f32::from(id.profile().combat.fear) / 50.0)
        .map_err(|_| format!("Unknown threat ID in quest: {enemy_type}"))
}

/// Durable strategic inputs for blood loss and religious morale relationships.
#[derive(Clone, Debug)]
#[table(accessor = character_condition)]
pub struct CharacterCondition {
    #[primary_key]
    pub character_id: u64,
    pub body_weight_kg: f32,
    pub current_blood_ml: f32,
    pub maximum_blood_ml: f32,
    pub religion_id: Option<String>,
}

/// Durable strategic food and water state. Positive balances are short-term
/// physiological reserves; negative balances represent unsupported need.
#[derive(Clone, Debug)]
#[table(accessor = character_needs)]
pub struct CharacterNeeds {
    #[primary_key]
    pub character_id: u64,
    pub food_balance_kcal: f32,
    pub water_balance_ml: f32,
    pub carried_water_ml: f32,
}

/// Durable strategic coating and temperature state. Wetness is intentionally
/// separate from filth: water changes exposure but carries no dirt/blood
/// provenance and washing never consumes it.
#[derive(Clone, Debug, PartialEq)]
#[table(accessor = character_exposure)]
pub struct CharacterExposure {
    #[primary_key]
    pub character_id: u64,
    pub wetness_bps: u16,
    /// Signed: negative is cold, positive is hot.
    pub thermal_strain: i32,
    pub frostbite_progress_minutes: u32,
}

/// A recent success or setback which decays linearly over strategic time.
#[derive(Clone, Debug)]
#[table(accessor = morale_event, public)]
pub struct MoraleEvent {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub kind: String,
    /// Positive values are successes; negative values are setbacks.
    pub magnitude: f32,
    pub occurred_at_minute: u64,
    pub expires_at_minute: u64,
    pub source_id: Option<String>,
}

/// Refreshable server-authoritative projection used by strategic clients.
#[derive(Clone, Debug, PartialEq)]
#[table(accessor = character_strategic_condition)]
pub struct CharacterStrategicCondition {
    #[primary_key]
    pub character_id: u64,
    pub morale: f32,
    /// This character's allocated share of the party's ally-restoration fraction.
    pub morale_bonus: f32,
    /// Maximum party restoration fraction at the current aggregate Command check.
    pub morale_bonus_cap: f32,
    /// Bounded strategic pressure toward inflexible religious behavior.
    pub fervor: f32,
    pub pain: f32,
    pub blood_loss: f32,
    pub fear: f32,
    pub fatigue: f32,
    pub hunger: f32,
    pub thirst: f32,
    pub thermal: f32,
    pub wetness_bps: u16,
    pub thermal_strain: i32,
    /// Positive physiological food reserve, expressed in travel days.
    pub food_days: f32,
    /// Positive physiological hydration reserve, expressed in travel days.
    pub water_days: f32,
    pub water_capacity_ml: u32,
    pub incapacitation: f32,
    pub check_multiplier: f32,
    pub status: String,
}

/// A signed contribution to the character's current projected morale.
#[derive(Clone, Debug)]
#[table(accessor = character_morale_source)]
pub struct CharacterMoraleSource {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    pub kind: String,
    pub label: String,
    pub magnitude: f32,
}

/// A strategic choice created when conviction begins demanding costly action.
#[derive(Clone, Debug)]
#[table(accessor = religious_demand, public)]
pub struct ReligiousDemand {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub fervor: f32,
    pub status: String,
    pub created_at_minute: u64,
    pub resolved_at_minute: Option<u64>,
    pub resolution: Option<String>,
}

pub fn initialize_character_condition(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<(), String> {
    if ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .is_none()
    {
        let maximum_blood_ml = DEFAULT_BODY_WEIGHT_KG * BLOOD_ML_PER_KG;
        ctx.db.character_condition().insert(CharacterCondition {
            character_id,
            body_weight_kg: DEFAULT_BODY_WEIGHT_KG,
            current_blood_ml: maximum_blood_ml,
            maximum_blood_ml,
            religion_id: None,
        });
    }
    if ctx
        .db
        .character_needs()
        .character_id()
        .find(character_id)
        .is_none()
    {
        ctx.db.character_needs().insert(CharacterNeeds {
            character_id,
            food_balance_kcal: FOOD_RESERVE_KCAL,
            water_balance_ml: HYDRATION_RESERVE_ML,
            carried_water_ml: 0.0,
        });
    }
    if ctx
        .db
        .character_exposure()
        .character_id()
        .find(character_id)
        .is_none()
    {
        ctx.db.character_exposure().insert(CharacterExposure {
            character_id,
            wetness_bps: 0,
            thermal_strain: 0,
            frostbite_progress_minutes: 0,
        });
    }
    Ok(())
}

enum ExposureLocation {
    Fixed(i32, i32, i16),
    Route {
        route: PartyJourneyRoute,
        completed_minutes: u64,
    },
}

impl ExposureLocation {
    fn position(&self, movement_offset: u64) -> (i32, i32, i16) {
        match self {
            Self::Fixed(latitude, longitude, elevation) => (*latitude, *longitude, *elevation),
            Self::Route {
                route,
                completed_minutes,
            } => crate::strategic::route_position_at_minute(
                route,
                completed_minutes.saturating_add(movement_offset),
            )
            .map(|(longitude, latitude)| {
                (
                    (latitude * 1_000_000.0).round() as i32,
                    (longitude * 1_000_000.0).round() as i32,
                    0,
                )
            })
            .unwrap_or((53_000_000, 10_000_000, 0)),
        }
    }
}

/// Load the durable location/route authority once. The potentially long
/// minute stepping below is then pure and performs no database queries.
fn exposure_location(ctx: &ReducerContext, character_id: u64) -> ExposureLocation {
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return ExposureLocation::Fixed(53_000_000, 10_000_000, 0);
    };
    if let Some(settlement_id) = character.current_settlement_id
        && let Some(place) = ctx.db.settlement().id().find(settlement_id)
    {
        return ExposureLocation::Fixed(
            (place.coord_y * 1_000_000.0).round() as i32,
            (place.coord_x * 1_000_000.0).round() as i32,
            place.elevation.get(),
        );
    }
    if let Some(party_id) = character.party_id {
        if let Some(party) = ctx.db.party_authority().id().find(&party_id) {
            if let Some(site_id) = party.current_case_site_id
                && let Some(site) = ctx.db.case_site_authority().id_key().find(site_id.value)
            {
                return ExposureLocation::Fixed(site.latitude_e7 / 10, site.longitude_e7 / 10, 0);
            }
        }
        if let (Some(journey), Some(route)) = (
            ctx.db.party_journey_authority().party_id().find(&party_id),
            ctx.db
                .party_journey_route_authority()
                .party_id()
                .find(&party_id),
        ) {
            return ExposureLocation::Route {
                route,
                completed_minutes: journey.completed_minutes,
            };
        }
    }
    ExposureLocation::Fixed(53_000_000, 10_000_000, 0)
}

/// The one strategic exposure seam. Call only after the authoritative clock
/// has committed its actually elapsed (possibly terminal-clipped) interval.
pub fn apply_weather_exposure(
    ctx: &ReducerContext,
    character_id: u64,
    starting_minute: u64,
    elapsed_minutes: u64,
    moving: bool,
    shelter: adventuresim_core::survival::FieldShelter,
) -> Result<(), String> {
    initialize_character_condition(ctx, character_id)?;
    if elapsed_minutes == 0 {
        return Ok(());
    }
    let row = ctx
        .db
        .character_exposure()
        .character_id()
        .find(character_id)
        .ok_or("Character exposure not found")?;
    let clothing = StrategicEquipment::load(ctx, character_id).survival_clothing();
    let location = exposure_location(ctx, character_id);
    let weather = (0..elapsed_minutes).map(|offset| {
        let (latitude, longitude, elevation) =
            location.position(moving.then_some(offset).unwrap_or(0));
        adventuresim_core::weather::weather_at(
            adventuresim_core::weather::WORLD_WEATHER_SEED,
            starting_minute.saturating_add(offset),
            latitude,
            longitude,
            elevation,
        )
    });
    let outcome = adventuresim_core::survival::advance_exposure(
        adventuresim_core::survival::SurvivalState {
            wetness_bps: row.wetness_bps,
            thermal_strain: row.thermal_strain,
            frostbite_progress_minutes: row.frostbite_progress_minutes,
        },
        weather,
        clothing,
        shelter,
    );
    ctx.db
        .character_exposure()
        .character_id()
        .update(CharacterExposure {
            character_id,
            wetness_bps: outcome.state.wetness_bps,
            thermal_strain: outcome.state.thermal_strain,
            frostbite_progress_minutes: outcome.state.frostbite_progress_minutes,
        });
    // Replay each threshold at its canonical absolute minute. Frostbite is
    // durable non-bleeding tissue damage and is deliberately not healed by
    // ordinary injury settlement, so replay commutes with the already-clipped
    // interval while preserving identical limb projections across partitions.
    for event_offset in outcome.frostbite_event_offsets {
        let event_minute = starting_minute.saturating_add(event_offset);
        let peripheral = adventuresim_core::survival::frostbite_peripheral_index(
            clothing.peripheral_protection_bps,
            event_minute,
        );
        let limb = [
            LimbRegion::LeftArm,
            LimbRegion::RightArm,
            LimbRegion::LeftLeg,
            LimbRegion::RightLeg,
        ][peripheral];
        crate::surgery::commit_frostbite_injury(
            ctx,
            character_id,
            limb,
            adventuresim_core::survival::FROSTBITE_DAMAGE_PER_THRESHOLD,
        )?;
    }
    refresh_character_strategic_condition_projection(ctx, character_id).map(|_| ())
}

/// Reusable authoritative water-entry impulse for future ford/immersion
/// locations. Route terrain currently has no ford coordinates, so no caller
/// guesses immersion from wetlands.
pub fn apply_immersion_impulse(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    initialize_character_condition(ctx, character_id)?;
    let row = ctx
        .db
        .character_exposure()
        .character_id()
        .find(character_id)
        .ok_or("Character exposure not found")?;
    let next =
        adventuresim_core::survival::apply_immersion(adventuresim_core::survival::SurvivalState {
            wetness_bps: row.wetness_bps,
            thermal_strain: row.thermal_strain,
            frostbite_progress_minutes: row.frostbite_progress_minutes,
        });
    ctx.db
        .character_exposure()
        .character_id()
        .update(CharacterExposure {
            character_id,
            wetness_bps: next.wetness_bps,
            thermal_strain: next.thermal_strain,
            frostbite_progress_minutes: next.frostbite_progress_minutes,
        });
    Ok(())
}

fn inventory_quantity(ctx: &ReducerContext, character_id: u64, item_id: &str) -> u32 {
    ctx.db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, item_id))
        .filter(|entry| {
            !crate::inventory_container::row_is_fireplace_rooted(ctx, "personal", entry.id)
        })
        .map(|entry| entry.quantity)
        .sum()
}

fn food_reserve_days(needs: &CharacterNeeds) -> f32 {
    needs.food_balance_kcal.max(0.0) / TRAVEL_CALORIES_PER_DAY
}

fn water_reserve_days(needs: &CharacterNeeds) -> f32 {
    needs.water_balance_ml.max(0.0) / TRAVEL_WATER_ML_PER_DAY
}

pub(crate) fn water_capacity_ml(ctx: &ReducerContext, character_id: u64) -> u32 {
    let capacity_per_container = ctx
        .db
        .item()
        .id()
        .find(WATERSKIN_ID.to_string())
        .map_or(0, |item| item.water_capacity_ml);
    inventory_quantity(ctx, character_id, WATERSKIN_ID).saturating_mul(capacity_per_container)
}

pub(crate) fn party_water_capacity_ml(ctx: &ReducerContext, party_id: &str) -> u32 {
    let capacity = ctx
        .db
        .item()
        .id()
        .find(WATERSKIN_ID.to_string())
        .map_or(0, |item| item.water_capacity_ml);
    ctx.db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .filter(|row| row.item_id == WATERSKIN_ID)
        .filter(|row| !crate::inventory_container::row_is_fireplace_rooted(ctx, "party", row.id))
        .map(|row| row.quantity)
        .sum::<u32>()
        .saturating_mul(capacity)
}

pub fn prepare_party_waterskins(
    ctx: &ReducerContext,
    party_id: &str,
    from_settlement: bool,
) -> Result<(), String> {
    let capacity = ctx
        .db
        .item()
        .id()
        .find(WATERSKIN_ID.to_string())
        .map_or(0, |item| item.water_capacity_ml);
    let skins: u32 = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .filter(|row| row.item_id == WATERSKIN_ID)
        .filter(|row| !crate::inventory_container::row_is_fireplace_rooted(ctx, "party", row.id))
        .map(|row| row.quantity)
        .sum();
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    party.pooled_water_ml = departure_water_volume(
        party.pooled_water_ml,
        skins.saturating_mul(capacity),
        from_settlement,
    );
    ctx.db.party_authority().id().update(party);
    Ok(())
}

pub fn prepare_character_waterskins(
    ctx: &ReducerContext,
    character_id: u64,
    from_settlement: bool,
) -> Result<(), String> {
    initialize_character_condition(ctx, character_id)?;
    let mut needs = ctx
        .db
        .character_needs()
        .character_id()
        .find(character_id)
        .ok_or("Character needs not found")?;
    needs.carried_water_ml = departure_water_volume(
        needs.carried_water_ml,
        water_capacity_ml(ctx, character_id),
        from_settlement,
    );
    ctx.db.character_needs().character_id().update(needs);
    Ok(())
}

pub fn replenish_needs_at_settlement(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<(), String> {
    initialize_character_condition(ctx, character_id)?;
    let mut needs = ctx
        .db
        .character_needs()
        .character_id()
        .find(character_id)
        .ok_or("Character needs not found")?;
    // Arrival grants no free provisions and clears any travel surplus so the
    // character can immediately eat a deliberate dinner.
    needs.food_balance_kcal = needs.food_balance_kcal.min(0.0);
    needs.water_balance_ml = HYDRATION_RESERVE_ML;
    needs.carried_water_ml = water_capacity_ml(ctx, character_id) as f32;
    ctx.db.character_needs().character_id().update(needs);
    Ok(())
}

pub fn apply_elapsed_needs(
    ctx: &ReducerContext,
    character_id: u64,
    elapsed_minutes: u64,
) -> Result<(), String> {
    apply_elapsed_needs_with_provision(
        ctx,
        character_id,
        elapsed_minutes,
        ElapsedNeedsProvision::PersonalSupplies,
    )
}

/// Applies settlement-rest needs exactly once. Every settlement provides
/// ordinary drinking water; paid inn stays additionally provide food.
pub fn apply_settlement_rest_elapsed_needs(
    ctx: &ReducerContext,
    character_id: u64,
    elapsed_minutes: u64,
    at_inn: bool,
) -> Result<(), String> {
    let provision = if at_inn {
        ElapsedNeedsProvision::InnBoard
    } else {
        ElapsedNeedsProvision::SettlementWater
    };
    apply_elapsed_needs_with_provision(ctx, character_id, elapsed_minutes, provision)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ElapsedNeedsProvision {
    PersonalSupplies,
    SettlementWater,
    InnBoard,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ElapsedNeedsPlan {
    food_balance_kcal: f32,
    consume_stored_food: bool,
    water_balance_ml: f32,
    consume_stored_water: bool,
}

fn elapsed_needs_plan(
    starting_food_balance_kcal: f32,
    starting_water_balance_ml: f32,
    elapsed_minutes: u64,
    provision: ElapsedNeedsProvision,
) -> ElapsedNeedsPlan {
    match provision {
        ElapsedNeedsProvision::PersonalSupplies => {
            let elapsed_days = elapsed_minutes as f32 / (24.0 * 60.0);
            ElapsedNeedsPlan {
                food_balance_kcal: starting_food_balance_kcal
                    - elapsed_days * TRAVEL_CALORIES_PER_DAY,
                consume_stored_food: true,
                water_balance_ml: starting_water_balance_ml
                    - elapsed_days * TRAVEL_WATER_ML_PER_DAY,
                consume_stored_water: true,
            }
        }
        // Public wells and ordinary settlement hospitality cover drinking
        // water during downtime. Food still comes from personal or party
        // stores unless the character pays for full board at an inn.
        ElapsedNeedsProvision::SettlementWater => {
            let elapsed_days = elapsed_minutes as f32 / (24.0 * 60.0);
            ElapsedNeedsPlan {
                food_balance_kcal: starting_food_balance_kcal
                    - elapsed_days * TRAVEL_CALORIES_PER_DAY,
                consume_stored_food: true,
                water_balance_ml: starting_water_balance_ml.max(0.0),
                consume_stored_water: false,
            }
        }
        // Full board covers the elapsed interval and brings an underfed guest
        // back to neutral, without creating surplus fullness or consuming
        // provisions carried by the guest or party.
        ElapsedNeedsProvision::InnBoard => ElapsedNeedsPlan {
            food_balance_kcal: starting_food_balance_kcal.max(0.0),
            consume_stored_food: false,
            water_balance_ml: starting_water_balance_ml.max(0.0),
            consume_stored_water: false,
        },
    }
}

fn apply_elapsed_needs_with_provision(
    ctx: &ReducerContext,
    character_id: u64,
    elapsed_minutes: u64,
    provision: ElapsedNeedsProvision,
) -> Result<(), String> {
    initialize_character_condition(ctx, character_id)?;
    let mut needs = ctx
        .db
        .character_needs()
        .character_id()
        .find(character_id)
        .ok_or("Character needs not found")?;

    let needs_plan = elapsed_needs_plan(
        needs.food_balance_kcal,
        needs.water_balance_ml,
        elapsed_minutes,
        provision,
    );
    needs.food_balance_kcal = needs_plan.food_balance_kcal;
    ctx.db
        .character_needs()
        .character_id()
        .update(needs.clone());
    if needs_plan.consume_stored_food {
        crate::food::consume_travel_food_to_zero(ctx, character_id)?;
    }
    needs = ctx
        .db
        .character_needs()
        .character_id()
        .find(character_id)
        .ok_or("Character needs not found")?;

    needs.water_balance_ml = needs_plan.water_balance_ml;
    if needs_plan.consume_stored_water && needs.water_balance_ml < 0.0 {
        if let Some(party_id) = ctx
            .db
            .character()
            .id()
            .find(character_id)
            .and_then(|row| row.party_id)
            && let Some(mut party) = ctx.db.party_authority().id().find(&party_id)
        {
            let (pooled_drunk, _) = shared_then_personal_volume(
                -needs.water_balance_ml,
                party.pooled_water_ml,
                needs.carried_water_ml,
            );
            party.pooled_water_ml -= pooled_drunk;
            ctx.db.party_authority().id().update(party);
            needs.water_balance_ml += pooled_drunk;
            let contained = crate::inventory_container::consume_contained_water(
                ctx,
                "party",
                &party_id,
                (-needs.water_balance_ml).max(0.0).ceil() as u64,
            )?;
            needs.water_balance_ml += contained as f32;
        }
        let drunk = (-needs.water_balance_ml)
            .max(0.0)
            .min(needs.carried_water_ml);
        needs.carried_water_ml -= drunk;
        needs.water_balance_ml += drunk;
        let contained = crate::inventory_container::consume_contained_water(
            ctx,
            "personal",
            &character_id.to_string(),
            (-needs.water_balance_ml).max(0.0).ceil() as u64,
        )?;
        needs.water_balance_ml += contained as f32;
    }
    ctx.db.character_needs().character_id().update(needs);
    Ok(())
}

fn total_damage(limbs: &CharacterLimbs) -> f32 {
    [
        limbs.left_arm_health,
        limbs.right_arm_health,
        limbs.left_leg_health,
        limbs.right_leg_health,
        limbs.head_health,
        limbs.chest_health,
        limbs.stomach_health,
    ]
    .into_iter()
    .map(|health| (1.0 - health).max(0.0))
    .sum()
}

pub(crate) fn mental_check(
    ctx: &ReducerContext,
    character_id: u64,
    skill: Skill,
) -> Result<f32, String> {
    let attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .ok_or("Character attributes not found")?;
    let limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(character_id)
        .ok_or("Character limbs not found")?;
    let stats = ctx
        .db
        .character_stats()
        .character_id()
        .find(character_id)
        .ok_or("Character stats not found")?;
    let skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?;
    let equipment = StrategicEquipment::load(ctx, character_id);
    Ok(skills.skill_check_by_parts(
        skill,
        &attributes,
        &limbs,
        &stats,
        &equipment,
        LimbWeights::all_equal(),
    ))
}

fn load_character_parts(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<
    (
        CharacterAttributes,
        CharacterLimbs,
        CharacterStats,
        CharacterSkills,
    ),
    String,
> {
    Ok((
        ctx.db
            .character_attributes()
            .character_id()
            .find(character_id)
            .ok_or("Character attributes not found")?,
        ctx.db
            .character_limbs()
            .character_id()
            .find(character_id)
            .ok_or("Character limbs not found")?,
        ctx.db
            .character_stats()
            .character_id()
            .find(character_id)
            .ok_or("Character stats not found")?,
        ctx.db
            .character_skills()
            .character_id()
            .find(character_id)
            .ok_or("Character skills not found")?,
    ))
}

#[derive(Clone, Debug)]
struct ProjectedMoraleSource {
    key: String,
    kind: String,
    label: String,
    magnitude: f32,
}

fn rank_morale_sources(raw_sources: &mut [ProjectedMoraleSource], will: f32) {
    let mut positive: Vec<_> = raw_sources
        .iter_mut()
        .filter(|source| source.magnitude > 0.0)
        .collect();
    positive.sort_by(|left, right| right.magnitude.total_cmp(&left.magnitude));
    for (index, source) in positive.into_iter().enumerate() {
        source.magnitude /= (index + 1) as f32;
    }
    let mut negative: Vec<_> = raw_sources
        .iter_mut()
        .filter(|source| source.magnitude < 0.0)
        .collect();
    negative.sort_by(|left, right| left.magnitude.total_cmp(&right.magnitude));
    for (index, source) in negative.into_iter().enumerate() {
        source.magnitude /= (index + 1) as f32 * will;
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct PartyReligionContext {
    own_cohort: f32,
    foreign_pressure: f32,
    party_command: f32,
    knowledge: f32,
}

fn religion_label(religion_id: &str) -> String {
    religion_id
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn party_character_ids(ctx: &ReducerContext, character_id: u64) -> Result<Vec<u64>, String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    Ok(condition_projection_member_ids(
        character_id,
        character.alive,
        character
            .party_id
            .as_ref()
            .map(|party_id| crate::strategic::living_party_member_ids(ctx, party_id)),
    ))
}

fn condition_projection_member_ids(
    character_id: u64,
    alive: bool,
    living_party_members: Option<Vec<u64>>,
) -> Vec<u64> {
    if !alive {
        // A corpse still has a durable condition projection, but must not be
        // reintroduced into living party morale/capability aggregation.
        vec![character_id]
    } else {
        living_party_members.unwrap_or_else(|| vec![character_id])
    }
}

fn religion_knowledge_check(
    ctx: &ReducerContext,
    character_id: u64,
    religion: adventuresim_world_schema::OfficialReligion,
) -> Result<f32, String> {
    let (attributes, limbs, stats, skills) = load_character_parts(ctx, character_id)?;
    Ok(adventuresim_core::capability::religion_knowledge_check(
        skills.religion_hours.effective(religion),
        attributes.instinct,
        attributes.intelligence,
        stats.focus,
        limbs.head_health,
    ))
}

fn party_religion_context(
    ctx: &ReducerContext,
    character_id: u64,
    party_members: &[u64],
) -> Result<Option<(String, PartyReligionContext)>, String> {
    let mut cohorts: BTreeMap<String, Vec<f32>> = BTreeMap::new();
    let mut commands = Vec::with_capacity(party_members.len());
    for member_id in party_members.iter().copied() {
        initialize_character_condition(ctx, member_id)?;
        commands.push(adventuresim_world_schema::language_scaled_effect(
            mental_check(ctx, member_id, Skill::Command)?,
            crate::character::shared_language_coefficient(ctx, member_id, character_id),
        ));
        if let Some(religion_id) = ctx
            .db
            .character_condition()
            .character_id()
            .find(member_id)
            .and_then(|condition| condition.religion_id)
        {
            cohorts.entry(religion_id).or_default().push(
                crate::personality::personality_or_neutral(ctx, member_id)
                    .conviction
                    .strength(),
            );
        }
    }
    let own_religion = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .and_then(|condition| condition.religion_id);
    let Some(own_religion) = own_religion else {
        return Ok(None);
    };
    let religion = adventuresim_world_schema::OfficialReligion::from_id(&own_religion)
        .ok_or_else(|| "Character has an unknown religion".to_string())?;
    let (own_cohort, foreign_pressure) = religion_cohort_pressure(cohorts, &own_religion);
    let knowledge_checks = party_members
        .iter()
        .copied()
        .map(|member_id| religion_knowledge_check(ctx, member_id, religion))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some((
        own_religion,
        PartyReligionContext {
            own_cohort,
            foreign_pressure,
            party_command: aggregate_party_command(commands),
            knowledge: aggregate_party_check(knowledge_checks).clamp(0.0, 5.0),
        },
    )))
}

fn religion_cohort_pressure(cohorts: BTreeMap<String, Vec<f32>>, own_religion: &str) -> (f32, f32) {
    let cohort_checks: BTreeMap<_, _> = cohorts
        .into_iter()
        .map(|(religion, checks)| (religion, aggregate_party_check(checks).clamp(0.0, 5.0)))
        .collect();
    let own_cohort = cohort_checks.get(own_religion).copied().unwrap_or(0.0);
    let foreign_pressure = aggregate_party_check(
        cohort_checks
            .iter()
            .filter_map(|(religion, check)| (religion != own_religion).then_some(*check)),
    )
    .clamp(0.0, 5.0);
    (own_cohort, foreign_pressure)
}

fn base_morale(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<(f32, Vec<ProjectedMoraleSource>), String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let current_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |time| time.minutes);
    let (_, limbs, _, _) = load_character_parts(ctx, character_id)?;
    let will = mental_check(ctx, character_id, Skill::Will)?.max(MINIMUM_WILL_CHECK);
    let personality = crate::personality::personality_or_neutral(ctx, character_id);
    let mut raw_sources = Vec::new();
    let mut add_source = |key: String,
                          kind: String,
                          label: String,
                          magnitude: f32,
                          stimulus: crate::personality::MoraleStimulus| {
        // True personality changes the authoritative magnitude, but labels are
        // public presentation data and must never reveal that private truth.
        let (magnitude, _) =
            crate::personality::react_raw_for_character(ctx, character_id, stimulus, magnitude);
        raw_sources.push(ProjectedMoraleSource {
            key,
            kind,
            label,
            magnitude,
        });
    };

    let injury = total_damage(&limbs) * INJURY_MORALE_PER_HEALTH_DEFICIT;
    if injury > 0.0 {
        add_source(
            "injuries".into(),
            "injury".into(),
            "Injuries".into(),
            -injury,
            crate::personality::MoraleStimulus::Other,
        );
    }

    let filth_total = ctx
        .db
        .character_filth()
        .character_id()
        .filter(character_id)
        .map(|deposit| f32::from(deposit.amount))
        .sum::<f32>()
        .min(f32::from(adventuresim_core::filth::MAX_FILTH));
    let filth_fraction = filth_total / f32::from(adventuresim_core::filth::MAX_FILTH);
    let hygiene_score =
        crate::personality::personality_scores_or_neutral(ctx, character_id).hygiene;
    let baseline_hygiene_morale = -8.0 * filth_fraction;
    let hygiene_endpoint = if hygiene_score >= 0 {
        if filth_total == 0.0 {
            2.0
        } else {
            -20.0 * filth_fraction
        }
    } else {
        0.0
    };
    let hygiene_ratio = f32::from(hygiene_score.unsigned_abs())
        / f32::from(crate::personality::PERSONALITY_SCORE_LIMIT as u16);
    let hygiene_morale =
        baseline_hygiene_morale + (hygiene_endpoint - baseline_hygiene_morale) * hygiene_ratio;
    if hygiene_morale != 0.0 {
        add_source(
            "cleanliness".into(),
            "cleanliness".into(),
            if hygiene_morale > 0.0 {
                "Clean".into()
            } else {
                "Filthy".into()
            },
            hygiene_morale,
            crate::personality::MoraleStimulus::Other,
        );
    }

    let party_members = party_character_ids(ctx, character_id)?;
    if let Some((religion_id, religion)) =
        party_religion_context(ctx, character_id, &party_members)?
    {
        if personality.conviction == crate::personality::Conviction::Zealous
            && religion.knowledge > 0.0
        {
            add_source(
                format!("religion-{religion_id}"),
                "religion".into(),
                format!("Religious leadership for {}", religion_label(&religion_id)),
                religion.knowledge,
                crate::personality::MoraleStimulus::Religious,
            );
        }
        let discord = religious_discord(religion.foreign_pressure, religion.party_command);
        if discord > 0.0 {
            add_source(
                "religious-discord".into(),
                "religious_discord".into(),
                "Religious discord".into(),
                -discord,
                crate::personality::MoraleStimulus::Religious,
            );
        }
        let prayer_minutes = ctx
            .db
            .character_training_schedule()
            .character_id()
            .find(character_id)
            .map_or(0, |schedule| schedule.downtime.prayer_minutes);
        if prayer_minutes > 0 {
            add_source(
                "daily-prayer".into(),
                "prayer".into(),
                "Daily prayer".into(),
                led_prayer_morale(prayer_minutes, religion.knowledge),
                crate::personality::MoraleStimulus::Religious,
            );
        }
        let prayer_fervor = fervor_fraction(
            crate::personality::conviction_strength_for_character(ctx, character_id),
            religion.own_cohort,
            0.0,
            religion.party_command,
        );
        let neglect = religious_neglect_morale(prayer_fervor, religion.party_command)
            * (1.0 - prayer_observance(prayer_fervor, prayer_minutes));
        if neglect > 0.0 {
            add_source(
                "neglected-prayer".into(),
                "prayer".into(),
                "Insufficient daily prayer".into(),
                -neglect,
                crate::personality::MoraleStimulus::Religious,
            );
        }
    } else {
        let meditation_minutes = ctx
            .db
            .character_training_schedule()
            .character_id()
            .find(character_id)
            .map_or(0, |schedule| schedule.downtime.prayer_minutes);
        if meditation_minutes > 0 {
            // Meditation is independent of religious knowledge and Conviction.
            add_source(
                "daily-meditation".into(),
                "meditation".into(),
                "Daily meditation".into(),
                meditation_morale(meditation_minutes),
                crate::personality::MoraleStimulus::Other,
            );
        }
    }
    let mut allied_power = 0.0;
    for member_id in party_members {
        let capability = crate::capability::refresh_character_capability(ctx, member_id)?;
        allied_power += capability.athletics
            + capability.endurance
            + capability.weapon_precision
            + if capability.melee || capability.ranged {
                2.0
            } else {
                0.0
            }
            + if capability.full_armor {
                2.0
            } else if capability.half_armor || capability.three_quarter_armor {
                1.0
            } else if capability.quarter_armor {
                0.5
            } else {
                0.0
            };
    }

    if let Some(case_site_id) = crate::investigation::character_case_site_id(ctx, character.id)
        && let Some(site) = ctx.db.case_site_authority().id_key().find(&case_site_id)
        && let Some(group) = ctx
            .db
            .hostile_group_authority()
            .iter()
            .find(|group| group.case_site_id == site.id)
    {
        let enemy_power = group.enemy_count.max(1) as f32 * (group.difficulty.max(1) as f32 + 4.0);
        let difference = allied_power - enemy_power;
        if difference != 0.0 {
            add_source(
                format!("power-{}", group.id),
                "power".into(),
                if difference > 0.0 {
                    "Superior allied strength".into()
                } else {
                    format!("Outmatched by {}", group.enemy_type)
                },
                if difference > 0.0 {
                    difference
                } else {
                    difference.abs() * -enemy_fear_multiplier(&group.enemy_type)?
                },
                if difference < 0.0 {
                    crate::personality::MoraleStimulus::Threat
                } else {
                    crate::personality::MoraleStimulus::Other
                },
            );
        }
    }

    for event in ctx.db.morale_event().character_id().filter(character_id) {
        let duration = event
            .expires_at_minute
            .saturating_sub(event.occurred_at_minute);
        let age = current_minute.saturating_sub(event.occurred_at_minute);
        let effect = if event.source_id.as_deref() == Some(LEISURE_MORALE_SOURCE_ID) {
            leisure_morale_effect(event.magnitude, age as f32, duration)
        } else if event.source_id.as_deref() == Some(MASTERY_MORALE_SOURCE_ID) {
            event.magnitude * adventuresim_core::morale::mastery_enjoyment_decay(age, duration)
        } else {
            event.magnitude * morale_event_decay(age, duration)
        };
        if effect != 0.0 {
            let stimulus = crate::personality::morale_event_stimulus(&event.kind);
            add_source(
                format!("event-{}", event.id),
                event.kind.clone(),
                match event.kind.as_str() {
                    "victory" => "Recent victory".into(),
                    "defeat" => "Recent defeat".into(),
                    "leisure" => "Restful leisure".into(),
                    "mastery_enjoyment" => "Mastery enjoyment".into(),
                    other => other.replace('_', " "),
                },
                effect,
                stimulus,
            );
        }
    }

    rank_morale_sources(&mut raw_sources, will);
    let morale = raw_sources.iter().map(|source| source.magnitude).sum();
    Ok((morale, raw_sources))
}

/// Feed all rejected effective skill training into one shared, durable morale
/// source. Callers aggregate every award in one logical clock interval before
/// recording it, so skill choice and award order cannot multiply enjoyment.
pub fn record_mastery_training_morale(
    ctx: &ReducerContext,
    character_id: u64,
    elapsed_minutes: u64,
    excess_effective_hours: f32,
) {
    if excess_effective_hours <= 0.0 || !excess_effective_hours.is_finite() {
        return;
    }
    let interval_end = character_minute(ctx, character_id);
    let interval_start = interval_end.saturating_sub(elapsed_minutes);
    let existing = ctx
        .db
        .morale_event()
        .character_id()
        .filter(character_id)
        .find(|event| event.source_id.as_deref() == Some(MASTERY_MORALE_SOURCE_ID));
    let at_interval_start = existing.as_ref().map_or(0.0, |event| {
        let duration = event
            .expires_at_minute
            .saturating_sub(event.occurred_at_minute);
        event.magnitude
            * adventuresim_core::morale::mastery_enjoyment_decay(
                interval_start.saturating_sub(event.occurred_at_minute),
                duration,
            )
    });
    // Endpoint semantics: the helper decays this interval-start magnitude
    // through `elapsed_minutes` before applying the one aggregated award.
    let magnitude = adventuresim_core::morale::mastery_enjoyment_after_interval(
        at_interval_start,
        excess_effective_hours,
        elapsed_minutes,
        RECENT_MORALE_DURATION_MINUTES,
    );
    let event = MoraleEvent {
        id: existing.as_ref().map_or(0, |event| event.id),
        character_id,
        kind: "mastery_enjoyment".into(),
        magnitude,
        occurred_at_minute: interval_end,
        expires_at_minute: interval_end.saturating_add(RECENT_MORALE_DURATION_MINUTES),
        source_id: Some(MASTERY_MORALE_SOURCE_ID.into()),
    };
    if existing.is_some() {
        ctx.db.morale_event().id().update(event);
    } else {
        ctx.db.morale_event().insert(event);
    }
}

fn party_morale_support(
    ctx: &ReducerContext,
    party_members: &[u64],
) -> Result<(f32, Vec<(u64, f32)>), String> {
    let mut commands = Vec::new();
    let mut surplus_weights = Vec::new();
    for member_id in party_members.iter().copied() {
        commands.push(mental_check(ctx, member_id, Skill::Command)?);
        let (member_base_morale, _) = base_morale(ctx, member_id)?;
        let surplus = member_base_morale.max(0.0);
        if surplus > 0.0 {
            surplus_weights.push((member_id, surplus));
        }
    }
    let party_command = aggregate_party_command(commands);
    let bonus_cap = MORALE_BONUS_PER_COMMAND * party_command;
    let combined_surplus = cumulative_morale(surplus_weights.iter().map(|(_, surplus)| *surplus));
    let total_bonus = morale_bonus_fraction(combined_surplus, party_command);
    let total_weight: f32 = surplus_weights.iter().map(|(_, surplus)| *surplus).sum();
    let shares = surplus_weights
        .into_iter()
        .map(|(member_id, surplus)| (member_id, total_bonus * surplus / total_weight))
        .collect();
    Ok((bonus_cap, shares))
}

fn evaluate_strategic_condition(
    ctx: &ReducerContext,
    character_id: u64,
    morale_bonus_cap: f32,
    morale_bonus_shares: &[(u64, f32)],
) -> Result<(CharacterStrategicCondition, Vec<ProjectedMoraleSource>), String> {
    initialize_character_condition(ctx, character_id)?;
    let condition = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .ok_or("Character condition not found")?;
    let (attributes, limbs, stats, _) = load_character_parts(ctx, character_id)?;
    let will = mental_check(ctx, character_id, Skill::Will)?;
    let (listener_base_morale, mut sources) = base_morale(ctx, character_id)?;
    let party_members = party_character_ids(ctx, character_id)?;
    let fervor =
        if let Some((_, religion)) = party_religion_context(ctx, character_id, &party_members)? {
            fervor_fraction(
                crate::personality::personality_or_neutral(ctx, character_id)
                    .conviction
                    .strength(),
                religion.own_cohort,
                listener_base_morale.max(0.0),
                religion.party_command,
            )
        } else {
            0.0
        };

    if listener_base_morale < 0.0 {
        let deficit = -listener_base_morale;
        let mut ally_lifts = Vec::new();
        for (member_id, fraction) in morale_bonus_shares.iter().copied() {
            if member_id != character_id && fraction > 0.0 {
                let ally = ctx
                    .db
                    .character()
                    .id()
                    .find(member_id)
                    .ok_or("Party member not found")?;
                let (social_multiplier, social_trait) =
                    crate::personality::ally_restoration_multiplier_for_character(
                        ctx,
                        character_id,
                    );
                ally_lifts.push((
                    member_id,
                    ally.name,
                    deficit * fraction * social_multiplier,
                    social_trait,
                ));
            }
        }
        let total_lift: f32 = ally_lifts.iter().map(|(_, _, lift, _)| *lift).sum();
        let scale = if total_lift > deficit {
            deficit / total_lift
        } else {
            1.0
        };
        for (member_id, name, lift, _social_trait) in ally_lifts {
            sources.push(ProjectedMoraleSource {
                key: format!("ally-{member_id}"),
                kind: "ally".into(),
                label: format!("Encouraged by {name}"),
                magnitude: lift * scale,
            });
        }
    }

    let morale = sources
        .iter()
        .map(|source| source.magnitude)
        .sum::<f32>()
        .min(listener_base_morale.max(0.0));
    let morale_bonus = morale_bonus_shares
        .iter()
        .find_map(|(member_id, bonus)| (*member_id == character_id).then_some(*bonus))
        .unwrap_or(0.0);
    let pain = pain_incapacitation(total_damage(&limbs), will);
    let blood_loss =
        blood_loss_incapacitation(condition.current_blood_ml, condition.maximum_blood_ml);
    let fatigue_ratio = stats.fatigue_by_parts(&attributes, &limbs);
    let mut needs = ctx
        .db
        .character_needs()
        .character_id()
        .find(character_id)
        .ok_or("Character needs not found")?;
    let water_capacity = water_capacity_ml(ctx, character_id);
    if needs.carried_water_ml > water_capacity as f32 {
        needs.carried_water_ml = water_capacity as f32;
        ctx.db
            .character_needs()
            .character_id()
            .update(needs.clone());
    }
    let hunger = hunger_incapacitation(needs.food_balance_kcal, TRAVEL_CALORIES_PER_DAY);
    let thirst = thirst_incapacitation(needs.water_balance_ml, TRAVEL_WATER_ML_PER_DAY);
    let exposure = ctx
        .db
        .character_exposure()
        .character_id()
        .find(character_id)
        .ok_or("Character exposure not found")?;
    let thermal = adventuresim_core::survival::thermal_incapacitation(exposure.thermal_strain);
    let incapacitation = StrategicIncapacitation {
        pain,
        blood_loss,
        fear: fear_incapacitation(morale),
        fatigue: fatigue_incapacitation(fatigue_ratio),
        hunger,
        thirst,
        thermal,
    };
    let status = match incapacitation.status() {
        IncapacitationStatus::Ready => "ready",
        IncapacitationStatus::Staggered => "staggered",
        IncapacitationStatus::Incapacitated => "incapacitated",
    };
    Ok((
        CharacterStrategicCondition {
            character_id,
            morale,
            morale_bonus,
            morale_bonus_cap,
            fervor,
            pain: incapacitation.pain,
            blood_loss: incapacitation.blood_loss,
            fear: incapacitation.fear,
            fatigue: incapacitation.fatigue,
            hunger: incapacitation.hunger,
            thirst: incapacitation.thirst,
            thermal: incapacitation.thermal,
            wetness_bps: exposure.wetness_bps,
            thermal_strain: exposure.thermal_strain,
            food_days: food_reserve_days(&needs),
            water_days: water_reserve_days(&needs),
            water_capacity_ml: water_capacity,
            incapacitation: incapacitation.total(),
            check_multiplier: incapacitation.check_multiplier(),
            status: status.into(),
        },
        sources,
    ))
}

fn refresh_one_strategic_condition(
    ctx: &ReducerContext,
    character_id: u64,
    morale_bonus_cap: f32,
    morale_bonus_shares: &[(u64, f32)],
) -> Result<CharacterStrategicCondition, String> {
    let (row, sources) =
        evaluate_strategic_condition(ctx, character_id, morale_bonus_cap, morale_bonus_shares)?;
    if let Some(existing) = ctx
        .db
        .character_strategic_condition()
        .character_id()
        .find(character_id)
    {
        if existing != row {
            ctx.db
                .character_strategic_condition()
                .character_id()
                .update(row.clone());
        }
    } else {
        ctx.db.character_strategic_condition().insert(row.clone());
    }
    let old_source_ids: Vec<String> = ctx
        .db
        .character_morale_source()
        .character_id()
        .filter(character_id)
        .map(|source| source.id)
        .collect();
    for id in old_source_ids {
        ctx.db.character_morale_source().id().delete(&id);
    }
    for source in sources {
        ctx.db
            .character_morale_source()
            .insert(CharacterMoraleSource {
                id: format!("{character_id}:{}", source.key),
                character_id,
                kind: source.kind,
                label: source.label,
                magnitude: source.magnitude,
            });
    }
    crate::social::prune_social_addresses(ctx, character_id);
    Ok(row)
}

fn character_minute(ctx: &ReducerContext, character_id: u64) -> u64 {
    ctx.db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |time| time.minutes)
}

fn ensure_holy_day_demand(
    ctx: &ReducerContext,
    condition: &CharacterStrategicCondition,
) -> Result<(), String> {
    if condition.fervor <= 0.0 {
        return Ok(());
    }
    let professes_religion = ctx
        .db
        .character_condition()
        .character_id()
        .find(condition.character_id)
        .is_some_and(|condition| condition.religion_id.is_some());
    if !professes_religion {
        return Ok(());
    }
    let current_minute = character_minute(ctx, condition.character_id);
    let current_day = current_minute / MINUTES_PER_DAY;
    if !is_sunday(current_day) {
        return Ok(());
    }
    let demands: Vec<_> = ctx
        .db
        .religious_demand()
        .character_id()
        .filter(condition.character_id)
        .collect();
    if demands.iter().any(|demand| demand.status == "pending") {
        return Ok(());
    }
    if demands.iter().any(|demand| {
        demand.kind == "holy_day" && demand.created_at_minute / MINUTES_PER_DAY == current_day
    }) {
        return Ok(());
    }
    let at_settlement = ctx
        .db
        .character()
        .id()
        .find(condition.character_id)
        .is_some_and(|character| character.current_settlement_id.is_some());
    if !at_settlement {
        return Ok(());
    }
    ctx.db.religious_demand().insert(ReligiousDemand {
        id: 0,
        character_id: condition.character_id,
        kind: "holy_day".into(),
        title: "Keep the holy day".into(),
        description: "Sunday is a day of worship and rest. Conviction demands a full day away from the road and worldly business; daily prayer is managed through the activity schedule.".into(),
        fervor: condition.fervor,
        status: "pending".into(),
        created_at_minute: current_minute,
        resolved_at_minute: None,
        resolution: None,
    });
    Ok(())
}

fn refresh_character_strategic_condition_projection(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<CharacterStrategicCondition, String> {
    let party_members = party_character_ids(ctx, character_id)?;
    let rows = refresh_party_strategic_condition_projection(ctx, &party_members)?;
    rows.into_iter()
        .find(|row| row.character_id == character_id)
        .ok_or_else(|| "Character is not a member of their party".to_string())
}

fn refresh_party_strategic_condition_projection(
    ctx: &ReducerContext,
    party_members: &[u64],
) -> Result<Vec<CharacterStrategicCondition>, String> {
    let (morale_bonus_cap, morale_bonus_shares) = party_morale_support(ctx, party_members)?;
    let rows = party_members
        .iter()
        .copied()
        .map(|member_id| {
            refresh_one_strategic_condition(ctx, member_id, morale_bonus_cap, &morale_bonus_shares)
        })
        .collect::<Result<Vec<_>, _>>()?;
    // Combat power consumes this projection. Keep its public aggregate in the
    // same transaction as the condition rows so disease, time, and recovery
    // changes cannot leave a stale readiness snapshot behind.
    for row in &rows {
        crate::capability::refresh_character_capability(ctx, row.character_id)?;
    }
    Ok(rows)
}

pub fn refresh_character_strategic_condition(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<CharacterStrategicCondition, String> {
    let mut requested = refresh_character_strategic_condition_projection(ctx, character_id)?;
    if refuse_expired_holy_day_demands(ctx, character_id, false)? {
        requested = refresh_character_strategic_condition_projection(ctx, character_id)?;
    }
    ensure_holy_day_demand(ctx, &requested)?;
    Ok(requested)
}

fn holy_day_demand_has_expired(created_day: u64, current_day: u64, departing: bool) -> bool {
    created_day < current_day || (departing && created_day == current_day)
}

fn refuse_expired_holy_day_demands(
    ctx: &ReducerContext,
    character_id: u64,
    departing: bool,
) -> Result<bool, String> {
    let current_minute = character_minute(ctx, character_id);
    let current_day = current_minute / MINUTES_PER_DAY;
    let pending: Vec<_> = ctx
        .db
        .religious_demand()
        .character_id()
        .filter(character_id)
        .filter(|demand| {
            demand.kind == "holy_day"
                && demand.status == "pending"
                && holy_day_demand_has_expired(
                    demand.created_at_minute / MINUTES_PER_DAY,
                    current_day,
                    departing,
                )
        })
        .collect();
    if pending.is_empty() {
        return Ok(false);
    }

    let command = party_command(ctx, character_id)?;
    for mut demand in pending {
        demand.status = "resolved".into();
        demand.resolved_at_minute = Some(current_minute);
        demand.resolution = Some("refuse".into());
        let penalty = religious_neglect_morale(demand.fervor, command);
        let source_id = format!("religious-demand:{}", demand.id);
        ctx.db.religious_demand().id().update(demand);
        if penalty > 0.0 && !has_morale_source(ctx, character_id, &source_id) {
            insert_morale_event_without_refresh(
                ctx,
                character_id,
                "religious_observance_neglected",
                -penalty,
                source_id,
            );
        }
    }
    Ok(true)
}

#[reducer]
pub fn resolve_religious_demand(
    ctx: &ReducerContext,
    demand_id: u64,
    choice: String,
) -> Result<(), String> {
    let mut demand = ctx
        .db
        .religious_demand()
        .id()
        .find(demand_id)
        .ok_or("Religious demand not found")?;
    crate::character::require_living_character(ctx, demand.character_id)?;
    if demand.status != "pending" {
        return Err("Religious demand has already been resolved".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(demand.character_id)
        .ok_or("Character not found")?;
    if character.server != ctx.sender() {
        return Err("Only this character's player may answer the demand".into());
    }
    if !matches!(choice.as_str(), "observe" | "refuse") {
        return Err("Unknown religious-demand choice".into());
    }
    if choice == "observe" && demand.kind == "holy_day" {
        if character.current_settlement_id.is_none() {
            return Err("A holy day can only be observed at a settlement".into());
        }
        let current_day = character_minute(ctx, demand.character_id) / MINUTES_PER_DAY;
        if current_day != demand.created_at_minute / MINUTES_PER_DAY {
            return Err("This holy day has already passed".into());
        }
    }
    demand.status = "resolved".into();
    demand.resolved_at_minute = Some(character_minute(ctx, demand.character_id));
    demand.resolution = Some(choice.clone());
    ctx.db.religious_demand().id().update(demand.clone());

    match choice.as_str() {
        "observe" if demand.kind == "holy_day" => {
            // Holy-day demand represents private observance and abstention
            // from work; it does not imply access to a Church service.
            crate::time::spend_private_settlement_downtime(
                ctx,
                demand.character_id,
                adventuresim_core::strategic_time::MINUTES_PER_DAY,
                true,
            )?;
            record_morale_event(
                ctx,
                demand.character_id,
                "holy_day_observed",
                2.0,
                Some(format!("religious-demand:{}", demand.id)),
            )?;
        }
        "refuse" => {
            let party_ids = party_character_ids(ctx, demand.character_id)?;
            let party_command = aggregate_party_command(
                party_ids
                    .into_iter()
                    .map(|id| mental_check(ctx, id, Skill::Command))
                    .collect::<Result<Vec<_>, _>>()?,
            );
            let penalty = religious_neglect_morale(demand.fervor, party_command);
            if penalty > 0.0 {
                record_morale_event(
                    ctx,
                    demand.character_id,
                    "religious_observance_neglected",
                    -penalty,
                    Some(format!("religious-demand:{}", demand.id)),
                )?;
            }
        }
        _ => return Err("Religious demand kind cannot be observed".into()),
    }
    refresh_character_strategic_condition(ctx, demand.character_id).map(|_| ())
}

pub fn record_morale_event(
    ctx: &ReducerContext,
    character_id: u64,
    kind: &str,
    magnitude: f32,
    source_id: Option<String>,
) -> Result<(), String> {
    if magnitude == 0.0 || !magnitude.is_finite() {
        return Ok(());
    }
    let occurred_at_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |time| time.minutes);
    let duration = stored_morale_event_duration(ctx, character_id, magnitude);
    ctx.db.morale_event().insert(MoraleEvent {
        id: 0,
        character_id,
        kind: kind.into(),
        magnitude,
        occurred_at_minute,
        expires_at_minute: occurred_at_minute.saturating_add(duration),
        source_id,
    });
    refresh_character_strategic_condition(ctx, character_id)?;
    Ok(())
}

/// Replace one durable, refreshable morale stimulus at an explicit strategic
/// minute. Callers performing a chronological batch should refresh the derived
/// condition once after the batch completes.
pub fn upsert_refreshable_morale_event_at_without_refresh(
    ctx: &ReducerContext,
    character_id: u64,
    kind: &str,
    magnitude: f32,
    occurred_at_minute: u64,
    source_id: &str,
) -> Result<(), String> {
    if magnitude == 0.0 || !magnitude.is_finite() {
        return Ok(());
    }
    let duration = stored_morale_event_duration(ctx, character_id, magnitude);
    let existing = ctx
        .db
        .morale_event()
        .character_id()
        .filter(character_id)
        .find(|event| event.source_id.as_deref() == Some(source_id));
    let event = MoraleEvent {
        id: existing.as_ref().map_or(0, |event| event.id),
        character_id,
        kind: kind.into(),
        magnitude,
        occurred_at_minute,
        expires_at_minute: occurred_at_minute.saturating_add(duration),
        source_id: Some(source_id.into()),
    };
    if existing.is_some() {
        ctx.db.morale_event().id().update(event);
    } else {
        ctx.db.morale_event().insert(event);
    }
    Ok(())
}

/// Replace a bounded morale source with rules-owned magnitude and duration.
/// Lifecycle systems use this when personality must not alter a contractual
/// effect's retention window.
pub(crate) fn upsert_fixed_morale_event_without_refresh(
    ctx: &ReducerContext,
    character_id: u64,
    kind: &str,
    magnitude: f32,
    occurred_at_minute: u64,
    expires_at_minute: u64,
    source_id: &str,
) {
    let existing = ctx
        .db
        .morale_event()
        .character_id()
        .filter(character_id)
        .find(|event| event.source_id.as_deref() == Some(source_id));
    let event = MoraleEvent {
        id: existing.as_ref().map_or(0, |event| event.id),
        character_id,
        kind: kind.into(),
        magnitude,
        occurred_at_minute,
        expires_at_minute,
        source_id: Some(source_id.into()),
    };
    if existing.is_some() {
        ctx.db.morale_event().id().update(event);
    } else {
        ctx.db.morale_event().insert(event);
    }
}

fn insert_morale_event_without_refresh(
    ctx: &ReducerContext,
    character_id: u64,
    kind: &str,
    magnitude: f32,
    source_id: String,
) {
    if magnitude == 0.0 || !magnitude.is_finite() {
        return;
    }
    let occurred_at_minute = character_minute(ctx, character_id);
    let duration = stored_morale_event_duration(ctx, character_id, magnitude);
    ctx.db.morale_event().insert(MoraleEvent {
        id: 0,
        character_id,
        kind: kind.into(),
        magnitude,
        occurred_at_minute,
        expires_at_minute: occurred_at_minute.saturating_add(duration),
        source_id: Some(source_id),
    });
}

fn stored_morale_event_duration(ctx: &ReducerContext, character_id: u64, magnitude: f32) -> u64 {
    if magnitude < 0.0 {
        crate::personality::negative_event_duration_for_character(
            ctx,
            character_id,
            RECENT_MORALE_DURATION_MINUTES,
        )
    } else {
        RECENT_MORALE_DURATION_MINUTES
    }
}

/// Record the one-off nonlinear morale result of an explicit prayer or
/// meditation interval. This deliberately does not inspect or alter the saved
/// daily activity schedule.
pub(crate) fn record_immediate_prayer_morale(
    ctx: &ReducerContext,
    character_id: u64,
    minutes: u16,
) -> Result<(), String> {
    let party_members = party_character_ids(ctx, character_id)?;
    let (kind, magnitude) = if let Some((_religion_id, religion)) =
        party_religion_context(ctx, character_id, &party_members)?
    {
        (
            "prayer",
            adventuresim_core::activity::led_prayer_morale(minutes, religion.knowledge),
        )
    } else {
        (
            "meditation",
            adventuresim_core::activity::meditation_morale(minutes),
        )
    };
    record_morale_event(
        ctx,
        character_id,
        kind,
        magnitude,
        Some(format!("activity:{kind}")),
    )
}

fn party_command(ctx: &ReducerContext, character_id: u64) -> Result<f32, String> {
    Ok(aggregate_party_command(
        party_character_ids(ctx, character_id)?
            .into_iter()
            .map(|id| mental_check(ctx, id, Skill::Command))
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

fn has_morale_source(ctx: &ReducerContext, character_id: u64, source_id: &str) -> bool {
    ctx.db
        .morale_event()
        .character_id()
        .filter(character_id)
        .any(|event| event.source_id.as_deref() == Some(source_id))
}

/// Advance fatigue for strategic travel. The existing `calories_used` field is
/// treated as a recoverable fatigue reservoir until food/day-boundary state is
/// implemented.
pub fn apply_travel_condition(
    ctx: &ReducerContext,
    character_id: u64,
    starting_minute: u64,
    elapsed_minutes: u64,
    prayer_minutes: u16,
) -> Result<(), String> {
    if elapsed_minutes > adventuresim_core::alcohol::MAX_ALCOHOL_INTERVAL_MINUTES {
        return Err("Travel condition interval cannot exceed one year".into());
    }
    let interval_end = starting_minute
        .checked_add(elapsed_minutes)
        .ok_or("Travel condition interval overflow")?;
    for (segment_start, segment_end, history_minute) in
        adventuresim_core::alcohol::travel_evening_segments(starting_minute, interval_end)
            .map_err(str::to_string)?
    {
        apply_elapsed_needs(ctx, character_id, segment_end - segment_start)?;
        // Movement alone may spend potable alcohol as emergency hydration.
        // Attribute each whole serving to the evening in which the deficit
        // arose; generic waits and camp downtime never invoke this fallback.
        if let Some(mut needs) = ctx.db.character_needs().character_id().find(character_id)
            && needs.water_balance_ml < 0.0
        {
            let supplied = crate::alcohol::consume_emergency_hydration(
                ctx,
                character_id,
                -needs.water_balance_ml,
                history_minute,
            );
            needs.water_balance_ml += supplied as f32;
            ctx.db.character_needs().character_id().update(needs);
        }
    }
    let mut stats = ctx
        .db
        .character_stats()
        .character_id()
        .find(character_id)
        .ok_or("Character stats not found")?;
    stats.calories_used += elapsed_minutes as f32 / (24.0 * 60.0) * TRAVEL_CALORIES_PER_DAY;
    ctx.db.character_stats().character_id().update(stats);

    refuse_expired_holy_day_demands(ctx, character_id, true)?;
    let condition = refresh_character_strategic_condition_projection(ctx, character_id)?;
    let professes_religion = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .is_some_and(|row| row.religion_id.is_some());
    if professes_religion && condition.fervor > 0.0 {
        let command = party_command(ctx, character_id)?;
        let daily_penalty = religious_neglect_morale(condition.fervor, command);
        let missed_prayer = 1.0 - prayer_observance(condition.fervor, prayer_minutes);
        let elapsed_days = elapsed_minutes as f32 / MINUTES_PER_DAY as f32;
        let prayer_penalty = daily_penalty * missed_prayer * elapsed_days;
        if prayer_penalty > 0.0 {
            insert_morale_event_without_refresh(
                ctx,
                character_id,
                "travel_prayer_neglected",
                -prayer_penalty,
                format!(
                    "travel-prayer:{starting_minute}:{}",
                    starting_minute.saturating_add(elapsed_minutes)
                ),
            );
        }
        for sunday in sundays_overlapping(starting_minute, elapsed_minutes) {
            let existing_demand = ctx
                .db
                .religious_demand()
                .character_id()
                .filter(character_id)
                .find(|demand| {
                    demand.kind == "holy_day"
                        && demand.created_at_minute / MINUTES_PER_DAY == sunday
                });
            let source_id = if let Some(mut demand) = existing_demand {
                if demand.status != "pending" {
                    continue;
                }
                demand.status = "resolved".into();
                demand.resolved_at_minute = Some(character_minute(ctx, character_id));
                demand.resolution = Some("refuse".into());
                let id = demand.id;
                ctx.db.religious_demand().id().update(demand);
                format!("religious-demand:{id}")
            } else {
                format!("missed-sunday:{sunday}")
            };
            if daily_penalty > 0.0 && !has_morale_source(ctx, character_id, &source_id) {
                insert_morale_event_without_refresh(
                    ctx,
                    character_id,
                    "religious_observance_neglected",
                    -daily_penalty,
                    source_id,
                );
            }
        }
    }
    refresh_character_strategic_condition_projection(ctx, character_id).map(|_| ())
}

pub fn apply_rest_condition(
    ctx: &ReducerContext,
    character_id: u64,
    elapsed_minutes: u64,
) -> Result<(), String> {
    initialize_character_condition(ctx, character_id)?;
    let _ = elapsed_minutes;
    refresh_character_strategic_condition(ctx, character_id).map(|_| ())
}

fn leisure_morale_effect(magnitude: f32, age_minutes: f32, duration: u64) -> f32 {
    if duration == 0 {
        return 0.0;
    }
    (magnitude - LEISURE_MORALE_LIMIT * age_minutes.max(0.0) / duration as f32).max(0.0)
}

fn accumulated_leisure_morale(
    existing: Option<(f32, u64, u64)>,
    earned: f32,
    morale_earning_minutes: f32,
    interval_end_minute: u64,
) -> f32 {
    let retained = existing.map_or(0.0, |(magnitude, occurred_at, expires_at)| {
        let duration = expires_at.saturating_sub(occurred_at);
        let age_at_interval_end = interval_end_minute.saturating_sub(occurred_at) as f32;
        let age_before_earning = (age_at_interval_end - morale_earning_minutes.max(0.0)).max(0.0);
        leisure_morale_effect(magnitude, age_before_earning, duration)
    });
    (retained + earned).clamp(0.0, LEISURE_MORALE_LIMIT)
}

fn upsert_leisure_morale(
    ctx: &ReducerContext,
    character_id: u64,
    earned: f32,
    morale_earning_minutes: f32,
    interval_end_minute: u64,
) {
    if earned <= 0.0 || !earned.is_finite() {
        return;
    }
    let existing = ctx
        .db
        .morale_event()
        .character_id()
        .filter(character_id)
        .find(|event| event.source_id.as_deref() == Some(LEISURE_MORALE_SOURCE_ID));
    let magnitude = accumulated_leisure_morale(
        existing.as_ref().map(|event| {
            (
                event.magnitude,
                event.occurred_at_minute,
                event.expires_at_minute,
            )
        }),
        earned,
        morale_earning_minutes,
        interval_end_minute,
    );
    if let Some(mut event) = existing {
        event.kind = "leisure".into();
        event.magnitude = magnitude;
        event.occurred_at_minute = interval_end_minute;
        event.expires_at_minute =
            interval_end_minute.saturating_add(RECENT_MORALE_DURATION_MINUTES);
        ctx.db.morale_event().id().update(event);
    } else {
        ctx.db.morale_event().insert(MoraleEvent {
            id: 0,
            character_id,
            kind: "leisure".into(),
            magnitude,
            occurred_at_minute: interval_end_minute,
            expires_at_minute: interval_end_minute.saturating_add(RECENT_MORALE_DURATION_MINUTES),
            source_id: Some(LEISURE_MORALE_SOURCE_ID.into()),
        });
    }
}

/// Apply the shared settlement Leisure outcome to durable fatigue and to one
/// stable morale source. Morale is earned from the interval's starting state;
/// it is never recomputed prospectively from the post-rest fatigue value.
/// This is separate from healing because only time that reaches the saved
/// downtime schedule receives its Leisure allocation.
pub fn apply_settlement_leisure_condition(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: DailySchedule,
    elapsed_minutes: u64,
    interval_end_minute: u64,
) -> Result<(), String> {
    let mut stats = ctx
        .db
        .character_stats()
        .character_id()
        .find(character_id)
        .ok_or("Character stats not found")?;
    let outcome = settlement_leisure_outcome(schedule, elapsed_minutes, stats.calories_used);
    stats.calories_used = (stats.calories_used + outcome.fatigue_delta).max(0.0);
    ctx.db.character_stats().character_id().update(stats);
    upsert_leisure_morale(
        ctx,
        character_id,
        outcome.morale,
        outcome.morale_earning_minutes,
        interval_end_minute,
    );
    crate::residence::apply_residence_leisure_morale(
        ctx,
        character_id,
        outcome.morale,
        interval_end_minute,
    )?;
    Ok(())
}

/// Rest performed away from a settlement. Camps relieve fatigue and permit
/// natural recovery but do not refill rations or water.
pub fn apply_camp_rest_condition(
    ctx: &ReducerContext,
    character_id: u64,
    elapsed_minutes: u64,
) -> Result<(), String> {
    initialize_character_condition(ctx, character_id)?;
    apply_elapsed_needs(ctx, character_id, elapsed_minutes)?;
    apply_camp_rest_recovery_condition(ctx, character_id, elapsed_minutes)
}

/// Apply only the recovery portion of camp rest. Callers that must process a
/// disease terminal boundary consume needs first, then skip this after death.
pub fn apply_camp_rest_recovery_condition(
    ctx: &ReducerContext,
    character_id: u64,
    elapsed_minutes: u64,
) -> Result<(), String> {
    let days = elapsed_minutes as f32 / (24.0 * 60.0);
    let mut stats = ctx
        .db
        .character_stats()
        .character_id()
        .find(character_id)
        .ok_or("Character stats not found")?;
    stats.calories_used = (stats.calories_used - TRAVEL_CALORIES_PER_DAY * days).max(0.0);
    ctx.db.character_stats().character_id().update(stats);
    refresh_character_strategic_condition(ctx, character_id).map(|_| ())
}

pub fn apply_blood_loss(
    ctx: &ReducerContext,
    character_id: u64,
    fraction_of_maximum: f32,
) -> Result<(), String> {
    initialize_character_condition(ctx, character_id)?;
    let mut condition = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .ok_or("Character condition not found")?;
    condition.current_blood_ml = (condition.current_blood_ml
        - condition.maximum_blood_ml * fraction_of_maximum.max(0.0))
    .max(0.0);
    let circulatory_failure = condition.maximum_blood_ml > 0.0
        && condition.current_blood_ml / condition.maximum_blood_ml <= 0.10;
    ctx.db
        .character_condition()
        .character_id()
        .update(condition);
    if circulatory_failure {
        crate::transition_character_to_dead(
            ctx,
            character_id,
            crate::DeathCause::CirculatoryFailure,
            crate::DeathSource::Strategic,
            Some("critical-blood-loss".into()),
        )?;
        Ok(())
    } else {
        refresh_character_strategic_condition(ctx, character_id).map(|_| ())
    }
}

/// Set the authoritative blood fraction after a combined bleeding/recovery
/// interval and commit circulatory death at the caller's already-clipped clock.
pub fn set_blood_fraction(
    ctx: &ReducerContext,
    character_id: u64,
    fraction: f32,
) -> Result<(), String> {
    initialize_character_condition(ctx, character_id)?;
    let mut condition = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .ok_or("Character condition not found")?;
    condition.current_blood_ml = condition.maximum_blood_ml * fraction.clamp(0.0, 1.0);
    let terminal = condition.maximum_blood_ml > 0.0
        && condition.current_blood_ml / condition.maximum_blood_ml <= 0.10;
    ctx.db
        .character_condition()
        .character_id()
        .update(condition);
    if terminal {
        crate::transition_character_to_dead(
            ctx,
            character_id,
            crate::DeathCause::CirculatoryFailure,
            crate::DeathSource::Strategic,
            Some("critical-blood-loss".into()),
        )?;
        Ok(())
    } else {
        refresh_character_strategic_condition(ctx, character_id).map(|_| ())
    }
}

pub fn require_character_ready(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    let condition = refresh_character_strategic_condition(ctx, character_id)?;
    if condition.status == "incapacitated" {
        Err("Character is incapacitated and must recover before acting".into())
    } else {
        Ok(())
    }
}

pub fn require_characters_ready(ctx: &ReducerContext, character_ids: &[u64]) -> Result<(), String> {
    for character_id in character_ids {
        crate::character::require_living_character(ctx, *character_id)?;
    }
    let conditions = refresh_party_strategic_condition_projection(ctx, character_ids)?;
    for condition in &conditions {
        ensure_holy_day_demand(ctx, condition)?;
        if condition.status == "incapacitated" {
            return Err("A party member is incapacitated and must recover before acting".into());
        }
    }
    Ok(())
}

#[reducer]
pub fn refresh_strategic_condition(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    refresh_character_strategic_condition(ctx, character_id).map(|_| ())
}

#[reducer]
pub fn set_character_religion(
    ctx: &ReducerContext,
    character_id: u64,
    religion_id: String,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    initialize_character_condition(ctx, character_id)?;
    let religion_id = religion_id.trim();
    if !religion_id.is_empty() {
        let character = ctx
            .db
            .character()
            .id()
            .find(character_id)
            .ok_or("Character not found")?;
        let settlement_id = character
            .current_settlement_id
            .ok_or("A religion can only be professed at a settlement")?;
        let settlement = ctx
            .db
            .settlement()
            .id()
            .find(&settlement_id)
            .ok_or("Character's settlement not found")?;
        require_profession_service(&settlement.economy)?;
        if !settlement
            .religious_status
            .represented_religions()
            .iter()
            .any(|religion| religion.religion_id() == religion_id)
        {
            return Err("This settlement's priest cannot receive that profession of faith".into());
        }
    }
    let mut condition = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .ok_or("Character condition not found")?;
    condition.religion_id = (!religion_id.is_empty()).then(|| religion_id.into());
    ctx.db
        .character_condition()
        .character_id()
        .update(condition);
    refresh_character_strategic_condition(ctx, character_id).map(|_| ())
}

fn require_profession_service(
    profile: &adventuresim_world_schema::SettlementEconomyProfile,
) -> Result<(), String> {
    use adventuresim_core::settlement_economy::{
        SettlementActionService, action_service_available,
    };
    if action_service_available(profile, SettlementActionService::Temple) {
        Ok(())
    } else {
        Err("This settlement has no church to receive a profession of faith".into())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CharacterNeeds, ElapsedNeedsProvision, ProjectedMoraleSource, TRAVEL_CALORIES_PER_DAY,
        TRAVEL_WATER_ML_PER_DAY, accumulated_leisure_morale, condition_projection_member_ids,
        elapsed_needs_plan, food_reserve_days, holy_day_demand_has_expired, leisure_morale_effect,
        rank_morale_sources, religion_cohort_pressure, require_profession_service,
        water_reserve_days,
    };
    use std::collections::BTreeMap;

    #[test]
    fn profession_requires_an_available_temple_service() {
        let mut profile = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
        assert!(require_profession_service(&profile).is_err());
        profile
            .services
            .push(adventuresim_world_schema::SettlementService::Temple);
        assert!(require_profession_service(&profile).is_ok());
    }

    #[test]
    fn irreverent_only_cohorts_create_no_religious_pressure() {
        let cohorts = BTreeMap::from([
            ("roman_catholic".to_string(), vec![0.0]),
            ("lutheran".to_string(), vec![0.0]),
        ]);
        assert_eq!(
            religion_cohort_pressure(cohorts, "roman_catholic"),
            (0.0, 0.0)
        );
        assert_eq!(
            religion_cohort_pressure(BTreeMap::new(), "judaism"),
            (0.0, 0.0)
        );
    }

    #[test]
    fn corpse_projection_contains_the_requested_character_without_living_party_support() {
        assert_eq!(
            condition_projection_member_ids(7, false, Some(vec![8, 9])),
            vec![7]
        );
        assert_eq!(
            condition_projection_member_ids(7, true, Some(vec![7, 8])),
            vec![7, 8]
        );
    }
    use adventuresim_core::strategic_schedule::{
        DailySchedule, LEISURE_MORALE_LIMIT, settlement_leisure_outcome,
    };
    use adventuresim_core::strategic_time::MINUTES_PER_DAY;

    #[test]
    fn condition_reserve_days_exclude_carried_provisions() {
        let needs = CharacterNeeds {
            character_id: 1,
            food_balance_kcal: 3_000.0,
            water_balance_ml: 1_000.0,
            carried_water_ml: 40_000.0,
        };

        assert_eq!(food_reserve_days(&needs), 0.5);
        assert_eq!(water_reserve_days(&needs), 0.25);
    }

    #[test]
    fn inn_board_clears_food_and_water_deficits_without_consuming_provisions() {
        let plan = elapsed_needs_plan(
            -1_450.0,
            -900.0,
            MINUTES_PER_DAY,
            ElapsedNeedsProvision::InnBoard,
        );

        assert_eq!(plan.food_balance_kcal, 0.0);
        assert_eq!(plan.water_balance_ml, 0.0);
        assert!(!plan.consume_stored_food);
        assert!(!plan.consume_stored_water);
    }

    #[test]
    fn inn_board_preserves_existing_food_and_water_fullness_without_creating_more() {
        let plan = elapsed_needs_plan(
            250.0,
            375.0,
            MINUTES_PER_DAY * 3,
            ElapsedNeedsProvision::InnBoard,
        );

        assert_eq!(plan.food_balance_kcal, 250.0);
        assert_eq!(plan.water_balance_ml, 375.0);
        assert!(!plan.consume_stored_food);
        assert!(!plan.consume_stored_water);
    }

    #[test]
    fn non_inn_elapsed_needs_still_draw_from_carried_provisions() {
        let plan = elapsed_needs_plan(
            -500.0,
            250.0,
            MINUTES_PER_DAY,
            ElapsedNeedsProvision::PersonalSupplies,
        );

        assert_eq!(plan.food_balance_kcal, -500.0 - TRAVEL_CALORIES_PER_DAY);
        assert_eq!(plan.water_balance_ml, 250.0 - TRAVEL_WATER_ML_PER_DAY);
        assert!(plan.consume_stored_food);
        assert!(plan.consume_stored_water);
    }

    #[test]
    fn settlement_rest_supplies_water_but_not_food() {
        let plan = elapsed_needs_plan(
            -500.0,
            -900.0,
            MINUTES_PER_DAY,
            ElapsedNeedsProvision::SettlementWater,
        );

        assert_eq!(plan.food_balance_kcal, -500.0 - TRAVEL_CALORIES_PER_DAY);
        assert_eq!(plan.water_balance_ml, 0.0);
        assert!(plan.consume_stored_food);
        assert!(!plan.consume_stored_water);
    }

    #[test]
    fn holy_day_demands_expire_after_their_day_or_on_departure() {
        assert!(!holy_day_demand_has_expired(6, 6, false));
        assert!(holy_day_demand_has_expired(6, 6, true));
        assert!(holy_day_demand_has_expired(6, 7, false));
        assert!(!holy_day_demand_has_expired(13, 12, true));
    }

    #[test]
    fn raw_personality_reaction_is_ranked_before_will() {
        let mut sources = vec![
            ProjectedMoraleSource {
                key: "trait-adjusted".into(),
                kind: "test".into(),
                label: "Defeat (Proud)".into(),
                magnitude: -30.0,
            },
            ProjectedMoraleSource {
                key: "other".into(),
                kind: "test".into(),
                label: "Other".into(),
                magnitude: -10.0,
            },
        ];
        rank_morale_sources(&mut sources, 2.0);
        assert_eq!(sources[0].magnitude, -15.0);
        assert_eq!(sources[1].magnitude, -2.5);
    }

    #[test]
    fn leisure_morale_upsert_is_capped_independent_of_sync_frequency() {
        let duration = super::RECENT_MORALE_DURATION_MINUTES;
        let mut magnitude = 0.0;
        let mut occurred_at = 0;
        for interval_end in 1..=1_000 {
            magnitude = accumulated_leisure_morale(
                Some((magnitude, occurred_at, occurred_at + duration)),
                LEISURE_MORALE_LIMIT / 100.0,
                1.0,
                interval_end,
            );
            occurred_at = interval_end;
        }
        assert_eq!(magnitude, LEISURE_MORALE_LIMIT);
    }

    #[test]
    fn zero_earned_leisure_does_not_create_morale() {
        assert_eq!(accumulated_leisure_morale(None, 0.0, 0.0, 1_440), 0.0);
    }

    #[test]
    fn carried_fatigue_prevents_morale_until_the_next_qualifying_interval() {
        let schedule = DailySchedule {
            combat_training_minutes: 16 * 60,
            ..Default::default()
        };
        let first = settlement_leisure_outcome(schedule, MINUTES_PER_DAY, 200.0);
        let after_first = accumulated_leisure_morale(
            None,
            first.morale,
            first.morale_earning_minutes,
            MINUTES_PER_DAY,
        );
        assert_eq!(first.fatigue_delta, -200.0);
        assert_eq!(after_first, 0.0);

        let second = settlement_leisure_outcome(schedule, MINUTES_PER_DAY, 0.0);
        let after_second = accumulated_leisure_morale(
            None,
            second.morale,
            second.morale_earning_minutes,
            MINUTES_PER_DAY.saturating_mul(2),
        );
        assert!(after_second > 0.0);
    }

    fn apply_partitioned_leisure(
        schedule: DailySchedule,
        step_minutes: u64,
        total_minutes: u64,
        starting_minute: u64,
        starting_fatigue: f32,
        starting_morale: f32,
    ) -> (f32, f32) {
        let duration = super::RECENT_MORALE_DURATION_MINUTES;
        let mut fatigue = starting_fatigue;
        let mut morale = starting_morale;
        let mut occurred_at = 0;
        let mut elapsed = 0;
        while elapsed < total_minutes {
            let interval = step_minutes.min(total_minutes - elapsed);
            let interval_end = starting_minute + elapsed + interval;
            let outcome = settlement_leisure_outcome(schedule, interval, fatigue);
            fatigue += outcome.fatigue_delta;
            if outcome.morale > 0.0 {
                morale = accumulated_leisure_morale(
                    Some((morale, occurred_at, occurred_at + duration)),
                    outcome.morale,
                    outcome.morale_earning_minutes,
                    interval_end,
                );
                occurred_at = interval_end;
            }
            elapsed += interval;
        }
        let effect = leisure_morale_effect(
            morale,
            starting_minute
                .saturating_add(total_minutes)
                .saturating_sub(occurred_at) as f32,
            duration,
        );
        (fatigue, effect)
    }

    #[test]
    fn leisure_source_with_carried_fatigue_is_partition_independent() {
        let schedule = DailySchedule {
            combat_training_minutes: 16 * 60,
            ..Default::default()
        };
        let total = 4 * MINUTES_PER_DAY;
        let start = 2 * MINUTES_PER_DAY;
        let aggregate = apply_partitioned_leisure(schedule, total, total, start, 350.0, 2.0);
        let daily = apply_partitioned_leisure(schedule, MINUTES_PER_DAY, total, start, 350.0, 2.0);
        let hourly = apply_partitioned_leisure(schedule, 60, total, start, 350.0, 2.0);

        assert!((aggregate.0 - daily.0).abs() < 0.001);
        assert!((aggregate.0 - hourly.0).abs() < 0.001);
        assert!((aggregate.1 - daily.1).abs() < 0.001);
        assert!((aggregate.1 - hourly.1).abs() < 0.001);
        assert_eq!(aggregate.1, LEISURE_MORALE_LIMIT);
    }

    #[test]
    fn leisure_source_decay_before_earning_is_partition_independent_below_cap() {
        let schedule = DailySchedule {
            combat_training_minutes: 17 * 60,
            ..Default::default()
        };
        let total = 2 * MINUTES_PER_DAY;
        let start = MINUTES_PER_DAY;
        let aggregate = apply_partitioned_leisure(schedule, total, total, start, 150.0, 2.0);
        let daily = apply_partitioned_leisure(schedule, MINUTES_PER_DAY, total, start, 150.0, 2.0);
        let hourly = apply_partitioned_leisure(schedule, 60, total, start, 150.0, 2.0);

        assert!((aggregate.1 - daily.1).abs() < 0.001);
        assert!((aggregate.1 - hourly.1).abs() < 0.001);
        assert!(aggregate.1 > 0.0 && aggregate.1 < LEISURE_MORALE_LIMIT);
    }
}
