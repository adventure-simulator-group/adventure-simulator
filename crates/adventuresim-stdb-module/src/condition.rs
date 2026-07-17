use adventuresim_core::prelude::*;
use spacetimedb::{ReducerContext, Table, reducer, table};
use std::collections::BTreeMap;

use crate::capability::StrategicEquipment;
use crate::character::character;
use crate::item::item;
use crate::strategic::{quest, settlement};
use crate::{
    CharacterAttributes, CharacterLimbs, CharacterSkills, CharacterStats, character_attributes,
    character_equip, character_limbs, character_skills, character_stats, character_time,
    character_training_schedule, inventory_item, party_member,
};

pub const DEFAULT_BODY_WEIGHT_KG: f32 = 70.0;
pub const BLOOD_ML_PER_KG: f32 = 70.0;
pub const BLOOD_RECOVERY_FRACTION_PER_DAY: f32 = 0.01;
pub const RECENT_MORALE_DURATION_MINUTES: u64 = 7 * 24 * 60;
const INJURY_MORALE_PER_HEALTH_DEFICIT: f32 = 5.0;
pub const TRAVEL_CALORIES_PER_DAY: f32 = STRATEGIC_TRAVEL_KCAL_PER_DAY;
pub const TRAVEL_WATER_ML_PER_DAY: f32 = STRATEGIC_TRAVEL_WATER_ML_PER_DAY;
pub const FOOD_RESERVE_KCAL: f32 = TRAVEL_CALORIES_PER_DAY;
pub const HYDRATION_RESERVE_ML: f32 = TRAVEL_WATER_ML_PER_DAY;
pub const PROVISION_BUFFER_PERCENT: u64 = STRATEGIC_PROVISION_BUFFER_PERCENT;
pub const TRAVEL_RATION_ID: &str = STANDARD_TRAVEL_RATION_ID;
pub const WATERSKIN_ID: &str = STANDARD_WATERSKIN_ID;

fn enemy_fear_multiplier(enemy_type: &str) -> f32 {
    let enemy = enemy_type.to_ascii_lowercase();
    if enemy.contains("demon") {
        3.0
    } else if enemy.contains("undead") || enemy.contains("skeleton") || enemy.contains("zombie") {
        1.5
    } else {
        1.0
    }
}

/// Durable strategic inputs for blood loss and religious morale relationships.
#[derive(Clone, Debug)]
#[table(accessor = character_condition, public)]
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
#[table(accessor = character_needs, public)]
pub struct CharacterNeeds {
    #[primary_key]
    pub character_id: u64,
    pub food_balance_kcal: f32,
    pub water_balance_ml: f32,
    pub carried_water_ml: f32,
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
#[table(accessor = character_strategic_condition, public)]
pub struct CharacterStrategicCondition {
    #[primary_key]
    pub character_id: u64,
    pub morale: f32,
    /// This character's allocated share of the party's ally-restoration fraction.
    pub morale_bonus: f32,
    /// Maximum party restoration fraction at the current aggregate Charisma check.
    pub morale_bonus_cap: f32,
    /// Bounded strategic pressure toward inflexible religious behavior.
    pub fervor: f32,
    pub pain: f32,
    pub blood_loss: f32,
    pub fear: f32,
    pub fatigue: f32,
    pub hunger: f32,
    pub thirst: f32,
    pub food_days: f32,
    pub water_days: f32,
    pub water_capacity_ml: u32,
    pub incapacitation: f32,
    pub check_multiplier: f32,
    pub status: String,
}

/// A signed contribution to the character's current projected morale.
#[derive(Clone, Debug)]
#[table(accessor = character_morale_source, public)]
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
    Ok(())
}

fn inventory_quantity(ctx: &ReducerContext, character_id: u64, item_id: &str) -> u32 {
    ctx.db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, item_id))
        .map(|entry| entry.quantity)
        .sum()
}

fn water_capacity_ml(ctx: &ReducerContext, character_id: u64) -> u32 {
    let capacity_per_container = ctx
        .db
        .item()
        .id()
        .find(WATERSKIN_ID.to_string())
        .map_or(0, |item| item.water_capacity_ml);
    inventory_quantity(ctx, character_id, WATERSKIN_ID).saturating_mul(capacity_per_container)
}

fn consume_inventory(
    ctx: &ReducerContext,
    character_id: u64,
    item_id: &str,
    mut quantity: u32,
) -> u32 {
    let requested = quantity;
    let stacks: Vec<_> = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, item_id))
        .collect();
    for mut stack in stacks {
        if quantity == 0 {
            break;
        }
        let consumed = stack.quantity.min(quantity);
        quantity -= consumed;
        stack.quantity -= consumed;
        if stack.quantity == 0 {
            ctx.db.inventory_item().id().delete(stack.id);
        } else {
            ctx.db.inventory_item().id().update(stack);
        }
    }
    requested - quantity
}

fn provision_units(
    planning_minutes: u64,
    food_balance_kcal: f32,
    water_balance_ml: f32,
    ration_kcal: f32,
    waterskin_capacity_ml: u32,
) -> (u32, u32) {
    let units = ProvisioningInputs {
        planning_minutes,
        buffer_percent: PROVISION_BUFFER_PERCENT,
        food_balance_kcal,
        water_balance_ml,
        travel_kcal_per_day: TRAVEL_CALORIES_PER_DAY,
        travel_water_ml_per_day: TRAVEL_WATER_ML_PER_DAY,
        ration_kcal,
        waterskin_capacity_ml,
    }
    .required_units();
    (units.rations, units.waterskins)
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
    needs.food_balance_kcal = FOOD_RESERVE_KCAL;
    needs.water_balance_ml = HYDRATION_RESERVE_ML;
    needs.carried_water_ml = water_capacity_ml(ctx, character_id) as f32;
    ctx.db.character_needs().character_id().update(needs);
    Ok(())
}

/// Purchase enough personal provisions for the supplied planning duration and
/// fill all owned water containers. The duration should include the expected
/// return leg where the destination cannot resupply the party.
pub fn provision_character_for_travel(
    ctx: &ReducerContext,
    character_id: u64,
    planning_minutes: u64,
) -> Result<(), String> {
    initialize_character_condition(ctx, character_id)?;
    let needs = ctx
        .db
        .character_needs()
        .character_id()
        .find(character_id)
        .ok_or("Character needs not found")?;
    let ration = ctx
        .db
        .item()
        .id()
        .find(TRAVEL_RATION_ID.to_string())
        .ok_or("Travel ration item is not defined")?;
    let waterskin = ctx
        .db
        .item()
        .id()
        .find(WATERSKIN_ID.to_string())
        .ok_or("Waterskin item is not defined")?;
    let (required_rations, required_waterskins) = provision_units(
        planning_minutes,
        needs.food_balance_kcal,
        needs.water_balance_ml,
        ration.nutrition_kcal,
        waterskin.water_capacity_ml,
    );
    let owned_rations = inventory_quantity(ctx, character_id, TRAVEL_RATION_ID);
    let rations_to_buy = required_rations.saturating_sub(owned_rations);
    let owned_waterskins = inventory_quantity(ctx, character_id, WATERSKIN_ID);
    let waterskins_to_buy = required_waterskins.saturating_sub(owned_waterskins);

    let cost = rations_to_buy
        .saturating_mul(ration.base_value.unwrap_or(0))
        .saturating_add(waterskins_to_buy.saturating_mul(waterskin.base_value.unwrap_or(0)));
    let mut character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if character.gold < cost {
        return Err(format!(
            "{} needs {cost} gold for food and water provisions but has only {}",
            character.name, character.gold
        ));
    }
    character.gold -= cost;
    ctx.db.character().id().update(character);
    crate::add_inventory_item(ctx, character_id, TRAVEL_RATION_ID, rations_to_buy);
    crate::add_inventory_item(ctx, character_id, WATERSKIN_ID, waterskins_to_buy);

    let mut needs = ctx
        .db
        .character_needs()
        .character_id()
        .find(character_id)
        .ok_or("Character needs not found")?;
    needs.carried_water_ml = water_capacity_ml(ctx, character_id) as f32;
    ctx.db.character_needs().character_id().update(needs);
    Ok(())
}

fn apply_travel_needs(
    ctx: &ReducerContext,
    character_id: u64,
    elapsed_minutes: u64,
) -> Result<(), String> {
    initialize_character_condition(ctx, character_id)?;
    let elapsed_days = elapsed_minutes as f32 / (24.0 * 60.0);
    let mut needs = ctx
        .db
        .character_needs()
        .character_id()
        .find(character_id)
        .ok_or("Character needs not found")?;

    needs.food_balance_kcal -= elapsed_days * TRAVEL_CALORIES_PER_DAY;
    let ration_kcal = ctx
        .db
        .item()
        .id()
        .find(TRAVEL_RATION_ID.to_string())
        .map_or(TRAVEL_CALORIES_PER_DAY, |item| item.nutrition_kcal);
    if needs.food_balance_kcal < 0.0 && ration_kcal > 0.0 {
        let wanted = ((-needs.food_balance_kcal) / ration_kcal).ceil() as u32;
        let eaten = consume_inventory(ctx, character_id, TRAVEL_RATION_ID, wanted);
        needs.food_balance_kcal += eaten as f32 * ration_kcal;
    }

    needs.water_balance_ml -= elapsed_days * TRAVEL_WATER_ML_PER_DAY;
    if needs.water_balance_ml < 0.0 {
        let drunk = (-needs.water_balance_ml).min(needs.carried_water_ml);
        needs.carried_water_ml -= drunk;
        needs.water_balance_ml += drunk;
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

fn mental_check(ctx: &ReducerContext, character_id: u64, skill: Skill) -> Result<f32, String> {
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
    let equip = ctx
        .db
        .character_equip()
        .character_id()
        .find(character_id)
        .ok_or("Character equipment not found")?;
    let equipment = StrategicEquipment::load(ctx, character_id, &equip);
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

#[derive(Clone, Copy, Debug, Default)]
struct PartyFaithContext {
    own_cohort: f32,
    foreign_pressure: f32,
    party_charisma: f32,
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
    Ok(character.party_id.as_ref().map_or_else(
        || vec![character_id],
        |party_id| {
            ctx.db
                .party_member()
                .party_id()
                .filter(party_id)
                .map(|member| member.character_id)
                .collect()
        },
    ))
}

fn party_faith_context(
    ctx: &ReducerContext,
    character_id: u64,
    party_members: &[u64],
) -> Result<Option<(String, PartyFaithContext)>, String> {
    let mut cohorts: BTreeMap<String, Vec<f32>> = BTreeMap::new();
    let mut charismas = Vec::with_capacity(party_members.len());
    for member_id in party_members.iter().copied() {
        initialize_character_condition(ctx, member_id)?;
        charismas.push(mental_check(ctx, member_id, Skill::Charisma)?);
        if let Some(religion_id) = ctx
            .db
            .character_condition()
            .character_id()
            .find(member_id)
            .and_then(|condition| condition.religion_id)
        {
            cohorts.entry(religion_id).or_default().push(mental_check(
                ctx,
                member_id,
                Skill::Faith,
            )?);
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
    let cohort_checks: BTreeMap<_, _> = cohorts
        .into_iter()
        .map(|(religion, checks)| (religion, aggregate_party_check(checks).clamp(1.0, 5.0)))
        .collect();
    let own_cohort = cohort_checks.get(&own_religion).copied().unwrap_or(1.0);
    let foreign_pressure = aggregate_party_check(
        cohort_checks
            .iter()
            .filter_map(|(religion, check)| (religion != &own_religion).then_some(*check)),
    )
    .clamp(0.0, 5.0);
    Ok(Some((
        own_religion,
        PartyFaithContext {
            own_cohort,
            foreign_pressure,
            party_charisma: aggregate_party_charisma(charismas),
        },
    )))
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
    let mut raw_sources = Vec::new();

    let injury = total_damage(&limbs) * INJURY_MORALE_PER_HEALTH_DEFICIT;
    if injury > 0.0 {
        raw_sources.push(ProjectedMoraleSource {
            key: "injuries".into(),
            kind: "injury".into(),
            label: "Injuries".into(),
            magnitude: -injury,
        });
    }

    let party_members = party_character_ids(ctx, character_id)?;
    if let Some((religion_id, faith)) = party_faith_context(ctx, character_id, &party_members)? {
        raw_sources.push(ProjectedMoraleSource {
            key: format!("faith-{religion_id}"),
            kind: "faith".into(),
            label: format!(
                "Conviction among the {} faithful",
                religion_label(&religion_id)
            ),
            magnitude: faith.own_cohort,
        });
        let discord = religious_discord(faith.foreign_pressure, faith.party_charisma);
        if discord > 0.0 {
            raw_sources.push(ProjectedMoraleSource {
                key: "religious-discord".into(),
                kind: "religious_discord".into(),
                label: "Religious discord".into(),
                magnitude: -discord,
            });
        }
        let prayer_minutes = ctx
            .db
            .character_training_schedule()
            .character_id()
            .find(character_id)
            .map_or(0, |schedule| schedule.downtime.prayer_minutes);
        if prayer_minutes > 0 {
            raw_sources.push(ProjectedMoraleSource {
                key: "daily-prayer".into(),
                kind: "prayer".into(),
                label: "Daily prayer".into(),
                magnitude: prayer_morale(prayer_minutes),
            });
        }
        let prayer_fervor = fervor_fraction(
            mental_check(ctx, character_id, Skill::Faith)?,
            faith.own_cohort,
            0.0,
            faith.party_charisma,
        );
        let neglect = religious_neglect_morale(prayer_fervor, faith.party_charisma)
            * (1.0 - prayer_observance(prayer_fervor, prayer_minutes));
        if neglect > 0.0 {
            raw_sources.push(ProjectedMoraleSource {
                key: "neglected-prayer".into(),
                kind: "prayer".into(),
                label: "Insufficient daily prayer".into(),
                magnitude: -neglect,
            });
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

    if let Some(quest_id) = character.current_quest_location_id
        && let Some(quest) = ctx.db.quest().id().find(&quest_id)
    {
        let enemy_power = quest.enemy_count.max(1) as f32 * (quest.difficulty.max(1) as f32 + 4.0);
        let difference = allied_power - enemy_power;
        if difference != 0.0 {
            raw_sources.push(ProjectedMoraleSource {
                key: format!("power-{quest_id}"),
                kind: "power".into(),
                label: if difference > 0.0 {
                    "Superior allied strength".into()
                } else {
                    format!("Outmatched by {}", quest.enemy_type)
                },
                magnitude: if difference > 0.0 {
                    difference
                } else {
                    difference.abs() * -enemy_fear_multiplier(&quest.enemy_type)
                },
            });
        }
    }

    for event in ctx.db.morale_event().character_id().filter(character_id) {
        let duration = event
            .expires_at_minute
            .saturating_sub(event.occurred_at_minute);
        let age = current_minute.saturating_sub(event.occurred_at_minute);
        let effect = event.magnitude * morale_event_decay(age, duration);
        if effect != 0.0 {
            raw_sources.push(ProjectedMoraleSource {
                key: format!("event-{}", event.id),
                kind: "event".into(),
                label: match event.kind.as_str() {
                    "victory" => "Recent victory".into(),
                    "defeat" => "Recent defeat".into(),
                    other => other.replace('_', " "),
                },
                magnitude: effect,
            });
        }
    }

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
    let morale = raw_sources.iter().map(|source| source.magnitude).sum();
    Ok((morale, raw_sources))
}

fn party_morale_support(
    ctx: &ReducerContext,
    party_members: &[u64],
) -> Result<(f32, Vec<(u64, f32)>), String> {
    let mut charismas = Vec::new();
    let mut surplus_weights = Vec::new();
    for member_id in party_members.iter().copied() {
        charismas.push(mental_check(ctx, member_id, Skill::Charisma)?);
        let (member_base_morale, _) = base_morale(ctx, member_id)?;
        let surplus = member_base_morale.max(0.0);
        if surplus > 0.0 {
            surplus_weights.push((member_id, surplus));
        }
    }
    let party_charisma = aggregate_party_charisma(charismas);
    let bonus_cap = MORALE_BONUS_PER_CHARISMA * party_charisma;
    let combined_surplus = cumulative_morale(surplus_weights.iter().map(|(_, surplus)| *surplus));
    let total_bonus = morale_bonus_fraction(combined_surplus, party_charisma);
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
    let fervor = if let Some((_, faith)) = party_faith_context(ctx, character_id, &party_members)? {
        fervor_fraction(
            mental_check(ctx, character_id, Skill::Faith)?,
            faith.own_cohort,
            listener_base_morale.max(0.0),
            faith.party_charisma,
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
                ally_lifts.push((member_id, ally.name, deficit * fraction));
            }
        }
        let total_lift: f32 = ally_lifts.iter().map(|(_, _, lift)| *lift).sum();
        let scale = if total_lift > deficit {
            deficit / total_lift
        } else {
            1.0
        };
        for (member_id, name, lift) in ally_lifts {
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
    let incapacitation = StrategicIncapacitation {
        pain,
        blood_loss,
        fear: fear_incapacitation(morale),
        fatigue: fatigue_incapacitation(fatigue_ratio),
        hunger,
        thirst,
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
            food_days: (needs.food_balance_kcal.max(0.0) / TRAVEL_CALORIES_PER_DAY)
                + inventory_quantity(ctx, character_id, TRAVEL_RATION_ID) as f32,
            water_days: (needs.water_balance_ml.max(0.0) + needs.carried_water_ml)
                / TRAVEL_WATER_ML_PER_DAY,
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
    party_members
        .iter()
        .copied()
        .map(|member_id| {
            refresh_one_strategic_condition(ctx, member_id, morale_bonus_cap, &morale_bonus_shares)
        })
        .collect()
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

    let charisma = party_charisma(ctx, character_id)?;
    for mut demand in pending {
        demand.status = "resolved".into();
        demand.resolved_at_minute = Some(current_minute);
        demand.resolution = Some("refuse".into());
        let penalty = religious_neglect_morale(demand.fervor, charisma);
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
            crate::time::rest_at_settlement(ctx, demand.character_id, 1, false)?;
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
            let party_charisma = aggregate_party_charisma(
                party_ids
                    .into_iter()
                    .map(|id| mental_check(ctx, id, Skill::Charisma))
                    .collect::<Result<Vec<_>, _>>()?,
            );
            let penalty = religious_neglect_morale(demand.fervor, party_charisma);
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
    ctx.db.morale_event().insert(MoraleEvent {
        id: 0,
        character_id,
        kind: kind.into(),
        magnitude,
        occurred_at_minute,
        expires_at_minute: occurred_at_minute + RECENT_MORALE_DURATION_MINUTES,
        source_id,
    });
    refresh_character_strategic_condition(ctx, character_id)?;
    Ok(())
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
    ctx.db.morale_event().insert(MoraleEvent {
        id: 0,
        character_id,
        kind: kind.into(),
        magnitude,
        occurred_at_minute,
        expires_at_minute: occurred_at_minute + RECENT_MORALE_DURATION_MINUTES,
        source_id: Some(source_id),
    });
}

fn party_charisma(ctx: &ReducerContext, character_id: u64) -> Result<f32, String> {
    Ok(aggregate_party_charisma(
        party_character_ids(ctx, character_id)?
            .into_iter()
            .map(|id| mental_check(ctx, id, Skill::Charisma))
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
    apply_travel_needs(ctx, character_id, elapsed_minutes)?;
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
        let charisma = party_charisma(ctx, character_id)?;
        let daily_penalty = religious_neglect_morale(condition.fervor, charisma);
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
    replenish_needs_at_settlement(ctx, character_id)?;
    let days = elapsed_minutes as f32 / (24.0 * 60.0);
    let mut condition = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .ok_or("Character condition not found")?;
    condition.current_blood_ml = (condition.current_blood_ml
        + condition.maximum_blood_ml * BLOOD_RECOVERY_FRACTION_PER_DAY * days)
        .min(condition.maximum_blood_ml);
    ctx.db
        .character_condition()
        .character_id()
        .update(condition);

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

/// Rest performed away from a settlement. Camps relieve fatigue and permit
/// natural recovery but do not refill rations or water.
pub fn apply_camp_rest_condition(
    ctx: &ReducerContext,
    character_id: u64,
    elapsed_minutes: u64,
) -> Result<(), String> {
    initialize_character_condition(ctx, character_id)?;
    let days = elapsed_minutes as f32 / (24.0 * 60.0);
    let mut condition = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .ok_or("Character condition not found")?;
    condition.current_blood_ml = (condition.current_blood_ml
        + condition.maximum_blood_ml * BLOOD_RECOVERY_FRACTION_PER_DAY * days)
        .min(condition.maximum_blood_ml);
    ctx.db
        .character_condition()
        .character_id()
        .update(condition);
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
    ctx.db
        .character_condition()
        .character_id()
        .update(condition);
    refresh_character_strategic_condition(ctx, character_id).map(|_| ())
}

pub fn require_character_ready(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let condition = refresh_character_strategic_condition(ctx, character_id)?;
    if condition.status == "incapacitated" {
        Err("Character is incapacitated and must recover before acting".into())
    } else {
        Ok(())
    }
}

pub fn require_characters_ready(ctx: &ReducerContext, character_ids: &[u64]) -> Result<(), String> {
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
        if settlement.religion_id != religion_id {
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

#[cfg(test)]
mod tests {
    use super::holy_day_demand_has_expired;

    #[test]
    fn holy_day_demands_expire_after_their_day_or_on_departure() {
        assert!(!holy_day_demand_has_expired(6, 6, false));
        assert!(holy_day_demand_has_expired(6, 6, true));
        assert!(holy_day_demand_has_expired(6, 7, false));
        assert!(!holy_day_demand_has_expired(13, 12, true));
    }
}
