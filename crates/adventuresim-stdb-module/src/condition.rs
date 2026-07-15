use adventuresim_core::prelude::*;
use spacetimedb::{ReducerContext, Table, reducer, table};
use std::collections::BTreeMap;

use crate::capability::StrategicEquipment;
use crate::character::character;
use crate::strategic::quest;
use crate::{
    CharacterAttributes, CharacterLimbs, CharacterSkills, CharacterStats, character_attributes,
    character_equip, character_limbs, character_skills, character_stats, character_time,
    party_member,
};

pub const DEFAULT_BODY_WEIGHT_KG: f32 = 70.0;
pub const BLOOD_ML_PER_KG: f32 = 70.0;
pub const BLOOD_RECOVERY_FRACTION_PER_DAY: f32 = 0.01;
pub const RECENT_MORALE_DURATION_MINUTES: u64 = 7 * 24 * 60;
const INJURY_MORALE_PER_HEALTH_DEFICIT: f32 = 5.0;
const TRAVEL_CALORIES_PER_DAY: f32 = 6_000.0;

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
#[table(name = character_condition, public)]
pub struct CharacterCondition {
    #[primary_key]
    pub character_id: u64,
    pub body_weight_kg: f32,
    pub current_blood_ml: f32,
    pub maximum_blood_ml: f32,
    pub religion_id: Option<String>,
}

/// A recent success or setback which decays linearly over strategic time.
#[derive(Clone, Debug)]
#[table(name = morale_event, public)]
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
#[table(name = character_strategic_condition, public)]
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
    pub incapacitation: f32,
    pub check_multiplier: f32,
    pub status: String,
}

/// A signed contribution to the character's current projected morale.
#[derive(Clone, Debug)]
#[table(name = character_morale_source, public)]
pub struct CharacterMoraleSource {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    pub kind: String,
    pub label: String,
    pub magnitude: f32,
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
    let incapacitation = StrategicIncapacitation {
        pain,
        blood_loss,
        fear: fear_incapacitation(morale),
        fatigue: fatigue_incapacitation(fatigue_ratio),
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

pub fn refresh_character_strategic_condition(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<CharacterStrategicCondition, String> {
    let party_members = party_character_ids(ctx, character_id)?;
    let (morale_bonus_cap, morale_bonus_shares) = party_morale_support(ctx, &party_members)?;
    let mut requested = None;
    for member_id in party_members {
        let row = refresh_one_strategic_condition(
            ctx,
            member_id,
            morale_bonus_cap,
            &morale_bonus_shares,
        )?;
        if member_id == character_id {
            requested = Some(row);
        }
    }
    requested.ok_or_else(|| "Character is not a member of their party".into())
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

/// Advance fatigue for strategic travel. The existing `calories_used` field is
/// treated as a recoverable fatigue reservoir until food/day-boundary state is
/// implemented.
pub fn apply_travel_condition(
    ctx: &ReducerContext,
    character_id: u64,
    elapsed_minutes: u64,
) -> Result<(), String> {
    let mut stats = ctx
        .db
        .character_stats()
        .character_id()
        .find(character_id)
        .ok_or("Character stats not found")?;
    stats.calories_used += elapsed_minutes as f32 / (24.0 * 60.0) * TRAVEL_CALORIES_PER_DAY;
    ctx.db.character_stats().character_id().update(stats);
    refresh_character_strategic_condition(ctx, character_id).map(|_| ())
}

pub fn apply_rest_condition(
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
    let mut condition = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .ok_or("Character condition not found")?;
    condition.religion_id = (!religion_id.trim().is_empty()).then(|| religion_id.trim().into());
    ctx.db
        .character_condition()
        .character_id()
        .update(condition);
    refresh_character_strategic_condition(ctx, character_id).map(|_| ())
}
