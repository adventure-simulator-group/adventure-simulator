use adventuresim_core::prelude::*;
use spacetimedb::{ReducerContext, Table, reducer, table};

use crate::capability::StrategicEquipment;
use crate::character::character;
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
#[derive(Clone, Debug)]
#[table(name = character_strategic_condition, public)]
pub struct CharacterStrategicCondition {
    #[primary_key]
    pub character_id: u64,
    pub positive_morale: f32,
    pub negative_morale: f32,
    pub morale: f32,
    pub pain: f32,
    pub blood_loss: f32,
    pub fear: f32,
    pub fatigue: f32,
    pub incapacitation: f32,
    pub check_multiplier: f32,
    pub status: String,
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

fn faith_relation(listener: &CharacterCondition, speaker: &CharacterCondition) -> FaithRelation {
    match (&listener.religion_id, &speaker.religion_id) {
        (Some(listener), Some(speaker)) if listener == speaker => FaithRelation::Shared,
        (Some(_), Some(_)) => FaithRelation::Conflicting,
        _ => FaithRelation::Neutral,
    }
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

pub fn evaluate_strategic_condition(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<CharacterStrategicCondition, String> {
    initialize_character_condition(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let condition = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .ok_or("Character condition not found")?;
    let current_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |time| time.minutes);
    let (attributes, limbs, stats, _) = load_character_parts(ctx, character_id)?;
    let will = mental_check(ctx, character_id, Skill::Will)?;

    let mut positive_effects = Vec::new();
    let mut negative_effects = vec![total_damage(&limbs) * INJURY_MORALE_PER_HEALTH_DEFICIT];

    let party_members: Vec<u64> = character.party_id.as_ref().map_or_else(
        || vec![character_id],
        |party_id| {
            ctx.db
                .party_member()
                .party_id()
                .filter(party_id)
                .map(|member| member.character_id)
                .collect()
        },
    );
    for member_id in party_members {
        initialize_character_condition(ctx, member_id)?;
        let member_condition = ctx
            .db
            .character_condition()
            .character_id()
            .find(member_id)
            .ok_or("Party member condition not found")?;
        let charisma = mental_check(ctx, member_id, Skill::Charisma)?;
        let speaker_faith = mental_check(ctx, member_id, Skill::Faith)?;
        let listener_faith = mental_check(ctx, character_id, Skill::Faith)?;
        let contribution = faith_adjusted_charisma(
            charisma,
            speaker_faith,
            listener_faith,
            faith_relation(&condition, &member_condition),
        );
        if contribution >= 0.0 {
            positive_effects.push(contribution);
        } else {
            negative_effects.push(-contribution);
        }
    }

    for event in ctx.db.morale_event().character_id().filter(character_id) {
        let duration = event
            .expires_at_minute
            .saturating_sub(event.occurred_at_minute);
        let age = current_minute.saturating_sub(event.occurred_at_minute);
        let effect = event.magnitude * morale_event_decay(age, duration);
        if effect > 0.0 {
            positive_effects.push(effect);
        } else if effect < 0.0 {
            negative_effects.push(-effect);
        }
    }

    let positive_morale = cumulative_morale(positive_effects.iter().copied());
    let negative_morale = cumulative_morale(negative_effects.iter().copied());
    let morale = resolve_morale(positive_effects, negative_effects, will);
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
    Ok(CharacterStrategicCondition {
        character_id,
        positive_morale,
        negative_morale,
        morale,
        pain: incapacitation.pain,
        blood_loss: incapacitation.blood_loss,
        fear: incapacitation.fear,
        fatigue: incapacitation.fatigue,
        incapacitation: incapacitation.total(),
        check_multiplier: incapacitation.check_multiplier(),
        status: status.into(),
    })
}

pub fn refresh_character_strategic_condition(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<CharacterStrategicCondition, String> {
    let row = evaluate_strategic_condition(ctx, character_id)?;
    if ctx
        .db
        .character_strategic_condition()
        .character_id()
        .find(character_id)
        .is_some()
    {
        ctx.db
            .character_strategic_condition()
            .character_id()
            .update(row.clone());
    } else {
        ctx.db.character_strategic_condition().insert(row.clone());
    }
    Ok(row)
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
