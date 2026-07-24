use adventuresim_core::strategic_schedule::{
    ActivityOutcomeInputs, DailySchedule, SkillHours, apply_religion_training,
    apply_schedule_training, settlement_activity_outcome,
};
use adventuresim_core::strategic_time::{
    MINUTES_PER_DAY, MINUTES_PER_YEAR, allocated_schedule_minutes,
    elapsed_official_minutes as calculate_elapsed_official_minutes,
};
use adventuresim_core::{capability::aggregate_bounded_party_check, prelude::*};
use spacetimedb::{ReducerContext, ScheduleAt, SpacetimeType, Table, reducer, table};

use crate::capability::StrategicEquipment;
use crate::character::character;
use crate::condition::character_condition as _;
use crate::strategic::party_authority;
use crate::{
    CharacterAttributes, CharacterSkills, CharacterStats, character_attributes, character_equip,
    character_limbs, character_skills, character_stats, settlement,
};
use adventuresim_world_schema::{OfficialReligion, OralLanguage, WrittenLanguage};
use std::collections::BTreeMap;

/// Natural recovery without useful medical support while taking full
/// settlement downtime.
pub const BASE_HEALTH_RECOVERED_PER_DAY: f32 = 0.01;
/// Additional daily recovery supplied by each point of the party Medicine
/// check. Checks are capped at the five-point scale used by the strategic UI.
pub const HEALTH_RECOVERED_PER_MEDICINE_CHECK_PER_DAY: f32 = 0.01;
pub const INN_GOLD_PER_DAY: u32 = 1;
const MIN_SETTLEMENT_REST_MINUTES: u64 = 60;
const MAX_SETTLEMENT_REST_MINUTES: u64 = MINUTES_PER_YEAR;
/// The current authoritative strategic time. `official_minutes` is absolute;
/// calendar presentation wraps it into years without making comparisons wrap.
#[derive(Clone, Debug)]
#[table(accessor = world_clock, public)]
pub struct WorldClock {
    #[primary_key]
    pub id: u64,
    pub official_minutes: u64,
    pub epoch_micros: i64,
}

/// Legacy scheduler row retained so existing databases can migrate without
/// dropping a table. New clocks are derived on demand and no longer schedule
/// a write every second.
#[derive(Clone, Debug)]
#[table(accessor = world_clock_schedule, scheduled(refresh_world_clock))]
pub struct WorldClockSchedule {
    #[primary_key]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
}

#[derive(Clone, Debug)]
#[table(accessor = character_time, public)]
pub struct CharacterTime {
    #[primary_key]
    pub character_id: u64,
    #[index(btree)]
    pub minutes: u64,
}

/// One 24-hour daily budget. Leisure is always the unallocated remainder.
#[derive(Clone, Debug, Default, SpacetimeType)]
pub struct ScheduleAllocation {
    pub combat_training_minutes: u16,
    pub carousing_minutes: u16,
    pub apprenticeship_minutes: u16,
    pub apprenticeship_service_id: Option<String>,
    pub profession_practice_minutes: u16,
    pub profession_service_id: Option<String>,
    /// Paid physical work; also trains Will at reduced speed.
    pub labor_minutes: u16,
    pub prayer_minutes: u16,
    pub thievery_minutes: u16,
    pub raiding_minutes: u16,
}

/// An explicit settlement activity selected by the player.  Profession
/// variants use the separate `service_id` reducer argument so this remains a
/// small, stable discriminator in generated clients.
#[derive(Clone, Copy, Debug, SpacetimeType)]
pub enum ImmediateActivity {
    Prayer,
    CombatTraining,
    Carousing,
    Apprenticeship,
    ProfessionPractice,
    Labor,
    Thievery,
    Raiding,
}

/// Daily settlement plan. The empty travel allocation is retained temporarily
/// for database/client compatibility; travel never applies it.
#[derive(Clone, Debug)]
#[table(accessor = character_training_schedule, public)]
pub struct CharacterTrainingSchedule {
    #[primary_key]
    pub character_id: u64,
    pub downtime: ScheduleAllocation,
    /// Legacy compatibility field. Reducers keep this empty and travel ignores it.
    pub travel: ScheduleAllocation,
}

/// A profession learned from a settlement service. This records access only;
/// rank is always derived from the character's canonical skill hours.
#[derive(Clone, Debug)]
#[table(accessor = character_apprenticeship, public)]
pub struct CharacterApprenticeship {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub service_id: String,
    pub religion_id: Option<String>,
    pub started_minute: u64,
    pub apprenticeship_minutes_accrued: u64,
    pub practice_minutes_accrued: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = character_virtue, public)]
pub struct CharacterVirtue {
    #[primary_key]
    pub character_id: u64,
    pub value: f32,
}

#[derive(Clone, Debug)]
#[table(accessor = character_notoriety, public)]
pub struct CharacterNotoriety {
    #[primary_key]
    pub character_id: u64,
    pub value: f32,
}

impl ScheduleAllocation {
    pub fn allocated_minutes(&self) -> u64 {
        allocated_schedule_minutes([
            self.labor_minutes,
            self.prayer_minutes,
            self.thievery_minutes,
            self.raiding_minutes,
            self.combat_training_minutes,
            self.carousing_minutes,
            self.apprenticeship_minutes,
            self.profession_practice_minutes,
        ])
    }

    fn uses_quarter_hours(&self) -> bool {
        let values = vec![
            self.labor_minutes,
            self.prayer_minutes,
            self.thievery_minutes,
            self.raiding_minutes,
            self.combat_training_minutes,
            self.carousing_minutes,
            self.apprenticeship_minutes,
            self.profession_practice_minutes,
        ];
        values.into_iter().all(|minutes| minutes % 15 == 0)
    }
}

pub fn initialize_time(ctx: &ReducerContext) {
    if ctx.db.world_clock().id().find(0).is_none() {
        ctx.db.world_clock().insert(WorldClock {
            id: 0,
            official_minutes: 0,
            epoch_micros: ctx.timestamp.to_micros_since_unix_epoch(),
        });
    }
}

pub fn refresh_clock(ctx: &ReducerContext) -> Result<u64, String> {
    if ctx.db.world_clock().id().find(0).is_none() {
        initialize_time(ctx);
    }
    let mut clock = ctx
        .db
        .world_clock()
        .id()
        .find(0)
        .ok_or_else(|| "World clock is not initialized".to_string())?;
    let official_minutes = calculate_elapsed_official_minutes(
        clock.epoch_micros,
        ctx.timestamp.to_micros_since_unix_epoch(),
    );
    if official_minutes != clock.official_minutes {
        clock.official_minutes = official_minutes;
        ctx.db.world_clock().id().update(clock);
    }
    Ok(official_minutes)
}

#[reducer]
fn refresh_world_clock(ctx: &ReducerContext, schedule: WorldClockSchedule) -> Result<(), String> {
    if ctx.sender() != ctx.database_identity() {
        return Err("World clock may only be refreshed by its scheduler".into());
    }
    // Remove scheduler rows created by older module versions. The authoritative
    // value is calculated from `epoch_micros` whenever an action needs it, and
    // browsers advance their initial snapshot locally.
    ctx.db
        .world_clock_schedule()
        .scheduled_id()
        .delete(schedule.scheduled_id);
    Ok(())
}

pub fn initialize_character_time(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    ensure_character_time(ctx, character_id)
}

/// Record time spent travelling without applying recovery, activities, or
/// training. Travel time belongs only to the character's personal clock.
pub fn advance_character_time(
    ctx: &ReducerContext,
    character_id: u64,
    minutes: u64,
) -> Result<bool, String> {
    crate::character::require_living_character(ctx, character_id)?;
    ensure_character_time(ctx, character_id)?;
    let mut character_time = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character time record not found".to_string())?;
    let starting_minute = character_time.minutes;
    let injury_limit =
        crate::surgery::preview_elapsed_for_injuries(ctx, character_id, minutes, false)?;
    let (elapsed, terminal) =
        crate::disease::clip_elapsed_for_disease(ctx, character_id, injury_limit, false)?;
    let settled = crate::surgery::settle_injuries(ctx, character_id, elapsed, false)?;
    let elapsed = settled.elapsed;
    character_time.minutes = character_time.minutes.saturating_add(elapsed);
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
    crate::social::settle_shared_party_time(ctx, character_id);
    crate::disease::finish_disease_interval(ctx, character_id, terminal)?;
    if terminal.is_some() || !settled.alive {
        return Ok(false);
    }
    crate::condition::apply_travel_condition(ctx, character_id, starting_minute, elapsed, 0)?;
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(true)
}

/// Actual strategic movement, split at exact dirt boundaries so filth and its
/// wound-risk multiplier are independent of caller chunking.
pub fn preview_travel_time(
    ctx: &ReducerContext,
    character_id: u64,
    requested: u64,
) -> Result<u64, String> {
    let injury = crate::surgery::preview_elapsed_for_injuries(ctx, character_id, requested, false)?;
    crate::disease::preview_elapsed_for_disease(ctx, character_id, injury, false)
}

/// Commit a terminal injury or disease event that falls exactly on the
/// character's current strategic minute. This intentionally grants no elapsed
/// travel time, condition use, filth, or training.
pub fn settle_travel_boundary(ctx: &ReducerContext, character_id: u64) -> Result<bool, String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if !character.alive {
        return Ok(false);
    }
    advance_character_time(ctx, character_id, 0)
}

pub fn advance_travel_time(
    ctx: &ReducerContext,
    character_id: u64,
    mut minutes: u64,
) -> Result<bool, String> {
    while minutes > 0 {
        let chunk = minutes.min(crate::filth::next_travel_dirt_boundary(ctx, character_id));
        let before = ctx
            .db
            .character_time()
            .character_id()
            .find(character_id)
            .map_or(0, |row| row.minutes);
        let alive = advance_character_time(ctx, character_id, chunk)?;
        let after = ctx
            .db
            .character_time()
            .character_id()
            .find(character_id)
            .map_or(before, |row| row.minutes);
        let elapsed = after.saturating_sub(before);
        crate::filth::record_travel_elapsed(ctx, character_id, elapsed, after)?;
        if !alive || elapsed < chunk {
            return Ok(false);
        }
        minutes -= elapsed;
    }
    Ok(true)
}

/// Stationary but strenuous strategic time used by investigation actions.
/// Unlike neutral waiting this applies ordinary needs and the same fatigue
/// reservoir as travel, but it never records movement filth or terrain
/// exposure.
pub fn advance_investigation_time(
    ctx: &ReducerContext,
    character_id: u64,
    minutes: u64,
) -> Result<bool, String> {
    let mut remaining = minutes;
    while remaining > 0 {
        let safe = preview_travel_time(ctx, character_id, remaining)?;
        if safe == 0 {
            return settle_travel_boundary(ctx, character_id);
        }
        let before = ctx
            .db
            .character_time()
            .character_id()
            .find(character_id)
            .map_or(0, |row| row.minutes);
        let alive = advance_character_time(ctx, character_id, safe)?;
        let after = ctx
            .db
            .character_time()
            .character_id()
            .find(character_id)
            .map_or(before, |row| row.minutes);
        let elapsed = after.saturating_sub(before);
        if !alive || elapsed < safe {
            return Ok(false);
        }
        remaining -= elapsed;
    }
    Ok(true)
}

/// Bring a co-located party to one strategic minute before an atomic shared
/// activity. The furthest-advanced member is authoritative; lagging members
/// settle ordinary stationary time before the strenuous interval begins.
pub(crate) fn synchronize_party_activity_time(
    ctx: &ReducerContext,
    member_ids: &[u64],
    leader_id: u64,
) -> Result<u64, String> {
    if !member_ids.contains(&leader_id) {
        return Err("Party leader is not a living activity participant".into());
    }
    for member_id in member_ids {
        synchronize_character_time(ctx, *member_id)?;
    }
    let start = member_ids
        .iter()
        .filter_map(|member_id| {
            ctx.db
                .character_time()
                .character_id()
                .find(*member_id)
                .map(|time| time.minutes)
        })
        .max()
        .ok_or("Party has no strategic clock")?;
    for member_id in member_ids {
        let minute = ctx
            .db
            .character_time()
            .character_id()
            .find(*member_id)
            .ok_or("Party member has no strategic clock")?
            .minutes;
        if minute < start
            && !advance_character_wait_time(ctx, *member_id, start.saturating_sub(minute))?
        {
            return Err("Every party member must survive clock synchronization".into());
        }
    }
    Ok(start)
}

/// Neutral/location-appropriate personal time for waiting and procedures. It
/// advances disease, wounds, blood, and ordinary recovery without applying
/// travel fatigue or travel needs.
pub fn advance_character_wait_time(
    ctx: &ReducerContext,
    character_id: u64,
    minutes: u64,
) -> Result<bool, String> {
    crate::character::require_living_character(ctx, character_id)?;
    ensure_character_time(ctx, character_id)?;
    let mut time = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Character time record not found")?;
    let injury_limit =
        crate::surgery::preview_elapsed_for_injuries(ctx, character_id, minutes, true)?;
    let (elapsed, terminal) =
        crate::disease::clip_elapsed_for_disease(ctx, character_id, injury_limit, true)?;
    let settled = crate::surgery::settle_injuries(ctx, character_id, elapsed, true)?;
    time.minutes = time.minutes.saturating_add(settled.elapsed);
    ctx.db.character_time().character_id().update(time);
    crate::social::settle_shared_party_time(ctx, character_id);
    crate::disease::finish_disease_interval(ctx, character_id, terminal)?;
    if terminal.is_some() || !settled.alive {
        return Ok(false);
    }
    let at_settlement = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .is_some_and(|row| row.current_settlement_id.is_some());
    if at_settlement {
        crate::condition::apply_rest_condition(ctx, character_id, settled.elapsed)?;
    } else {
        crate::condition::apply_elapsed_needs(ctx, character_id, settled.elapsed)?;
        crate::condition::apply_camp_rest_recovery_condition(ctx, character_id, settled.elapsed)?;
    }
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(true)
}

fn default_schedule(character_id: u64) -> CharacterTrainingSchedule {
    CharacterTrainingSchedule {
        character_id,
        downtime: ScheduleAllocation::default(),
        travel: ScheduleAllocation::default(),
    }
}

fn ensure_character_time(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let official_minutes = refresh_clock(ctx)?;
    if ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .is_none()
    {
        ctx.db.character_time().insert(CharacterTime {
            character_id,
            minutes: official_minutes,
        });
    }
    if ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
        .is_none()
    {
        ctx.db
            .character_training_schedule()
            .insert(default_schedule(character_id));
    }
    Ok(())
}

fn apprenticeship_for_service(
    ctx: &ReducerContext,
    character_id: u64,
    service_id: &str,
) -> Option<CharacterApprenticeship> {
    ctx.db
        .character_apprenticeship()
        .character_id()
        .filter(character_id)
        .find(|row| row.service_id == service_id)
}

fn valid_profession_service(service_id: &str) -> bool {
    matches!(
        service_id,
        "merchants" | "weapons" | "armor" | "clothing" | "herbalist" | "inn" | "religion"
    )
}

fn profession_training_hours(
    skills: &CharacterSkills,
    apprenticeship: &CharacterApprenticeship,
    skill: Skill,
) -> f32 {
    match skill {
        Skill::Insight => skills.insight_hours,
        Skill::SelfAwareness => skills.self_awareness_hours,
        Skill::Humor => skills.humor_hours,
        Skill::Command => skills.command_hours,
        Skill::Deception => skills.deception_hours,
        Skill::Seduction => skills.seduction_hours,
        Skill::Smithing => skills.smithing_hours,
        Skill::Medicine => skills.medicine_hours,
        Skill::Cooking => skills.cooking_hours,
        Skill::Anatomy => skills.anatomy_hours,
        Skill::Knife => skills.knife_hours,
        Skill::Tailoring => skills.tailoring_hours,
        Skill::Religion => apprenticeship
            .religion_id
            .as_deref()
            .and_then(OfficialReligion::from_id)
            .map_or(0.0, |religion| skills.religion_hours.direct(religion)),
        _ => 0.0,
    }
}

fn profession_tier_for(
    ctx: &ReducerContext,
    character_id: u64,
    service_id: &str,
) -> Result<adventuresim_core::profession::ProfessionTier, String> {
    let apprenticeship = apprenticeship_for_service(ctx, character_id, service_id)
        .ok_or("That profession has not been learned")?;
    let skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?;
    let profession = adventuresim_core::profession::profession_for_service(service_id)
        .ok_or("Unknown profession service")?;
    Ok(adventuresim_core::profession::profession_tier(
        profession,
        |skill| profession_training_hours(&skills, &apprenticeship, skill),
    ))
}

fn validate_profession_schedule(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
) -> Result<(), String> {
    if schedule.apprenticeship_minutes > 0 {
        let service = schedule
            .apprenticeship_service_id
            .as_deref()
            .ok_or("Apprenticeship time requires a profession")?;
        profession_tier_for(ctx, character_id, service)?;
    }
    if schedule.profession_practice_minutes > 0 {
        let service = schedule
            .profession_service_id
            .as_deref()
            .ok_or("Profession practice time requires a profession")?;
        let tier = profession_tier_for(ctx, character_id, service)?;
        if tier == adventuresim_core::profession::ProfessionTier::Apprentice {
            return Err("Independent practice requires Journeyman rank (2)".into());
        }
    }
    Ok(())
}

/// Learn a profession from a service at the character's current settlement.
/// Repeating the same request is deliberately idempotent.
#[reducer]
pub fn begin_apprenticeship(
    ctx: &ReducerContext,
    character_id: u64,
    service_id: &str,
) -> Result<(), String> {
    let character = crate::character::require_living_character(ctx, character_id)?;
    // `Character.server` is a transient tactical-server assignment, not the
    // player's identity. Strategic-web authorizes the active character through
    // its session before invoking this reducer. Settlement characters normally
    // have `Identity::ZERO` here, so comparing it to the web connection identity
    // incorrectly rejects every legitimate apprenticeship request.
    if !valid_profession_service(service_id) {
        return Err("Unknown profession service".into());
    }
    let settlement_id = character
        .current_settlement_id
        .as_ref()
        .ok_or("Apprenticeships may only begin in a settlement")?;
    ensure_character_time(ctx, character_id)?;
    if apprenticeship_for_service(ctx, character_id, service_id).is_some() {
        select_profession_schedule(ctx, character_id, service_id);
        return Ok(());
    }
    let religion_id = if service_id == "religion" {
        ctx.db
            .settlement()
            .id()
            .find(settlement_id)
            .and_then(|settlement| {
                settlement
                    .religious_status
                    .represented_religions()
                    .first()
                    .map(|religion| religion.religion_id().to_string())
            })
            .ok_or("This settlement has no religious profession")?
            .into()
    } else {
        None
    };
    let started_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Character time record not found")?
        .minutes;
    ctx.db
        .character_apprenticeship()
        .insert(CharacterApprenticeship {
            id: 0,
            character_id,
            service_id: service_id.into(),
            religion_id,
            started_minute,
            apprenticeship_minutes_accrued: 0,
            practice_minutes_accrued: 0,
        });
    select_profession_schedule(ctx, character_id, service_id);
    Ok(())
}

fn select_profession_schedule(ctx: &ReducerContext, character_id: u64, service_id: &str) {
    if let Some(mut schedule) = ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
    {
        schedule.downtime.apprenticeship_service_id = Some(service_id.into());
        schedule.downtime.profession_service_id = Some(service_id.into());
        ctx.db
            .character_training_schedule()
            .character_id()
            .update(schedule);
    }
}

fn activity_training_profile(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<adventuresim_core::strategic_schedule::ActivityTrainingProfile, String> {
    let equip = ctx
        .db
        .character_equip()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character equipment not found".to_string())?;
    let equipment = StrategicEquipment::load(ctx, character_id, &equip);
    Ok(
        adventuresim_core::strategic_schedule::ActivityTrainingProfile {
            combat: equipment.combat_training_profile(),
        },
    )
}

fn apply_training(
    ctx: &ReducerContext,
    character_id: u64,
    skills: &mut CharacterSkills,
    schedule: &ScheduleAllocation,
    elapsed: u64,
    activities: adventuresim_core::strategic_schedule::ActivityTrainingProfile,
) {
    let mut hours = SkillHours {
        polearm: skills.polearm_hours,
        axe: skills.axe_hours,
        bludgeon: skills.bludgeon_hours,
        sword: skills.sword_hours,
        knife: skills.knife_hours,
        dodge: skills.dodge_hours,
        block: skills.block_hours,
        bow: skills.bow_hours,
        crossbow: skills.crossbow_hours,
        firearm: skills.firearm_hours,
        throw: skills.throw_hours,
        will: skills.will_hours,
        insight: skills.insight_hours,
        self_awareness: skills.self_awareness_hours,
        humor: skills.humor_hours,
        command: skills.command_hours,
        deception: skills.deception_hours,
        seduction: skills.seduction_hours,
        medicine: skills.medicine_hours,
        cooking: skills.cooking_hours,
        religion: skills.religion_hours,
        stealth: skills.stealth_hours,
        balance: skills.balance_hours,
        anatomy: skills.anatomy_hours,
        tailoring: skills.tailoring_hours,
        smithing: skills.smithing_hours,
    };
    apply_schedule_training(&mut hours, core_schedule(schedule), elapsed, activities);
    let prayer_religion = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .and_then(|condition| condition.religion_id)
        .as_deref()
        .and_then(OfficialReligion::from_id);
    apply_religion_training(
        &mut hours.religion,
        elapsed,
        prayer_religion,
        schedule.prayer_minutes,
    );
    if let Some(character) = ctx.db.character().id().find(character_id) {
        if let Some(settlement_id) = character.current_settlement_id {
            if let Some(settlement) = ctx.db.settlement().id().find(&settlement_id) {
                // Ordinary life supplies bounded ambient exposure during the
                // waking two-thirds of actual elapsed settlement time.
                let exposure = elapsed as f32 / 60.0 * (2.0 / 3.0);
                skills.oral_languages.add_direct(
                    OralLanguage::EastCentral,
                    exposure * f32::from(settlement.languages.east_central_bp) / 10_000.0,
                );
                skills.oral_languages.add_direct(
                    OralLanguage::WestCentral,
                    exposure * f32::from(settlement.languages.west_central_bp) / 10_000.0,
                );
                skills.oral_languages.add_direct(
                    OralLanguage::Low,
                    exposure * f32::from(settlement.languages.low_bp) / 10_000.0,
                );
            }
        }
    }
    for (minutes, service_id) in [
        (
            schedule.apprenticeship_minutes,
            schedule.apprenticeship_service_id.as_deref(),
        ),
        (
            schedule.profession_practice_minutes,
            schedule.profession_service_id.as_deref(),
        ),
    ] {
        if minutes > 0 {
            if let Some(service_id) = service_id {
                let work_hours =
                    elapsed as f32 / MINUTES_PER_DAY as f32 * f32::from(minutes) / 60.0;
                let profile =
                    adventuresim_core::profession::profession_literacy_profile(service_id);
                let vernacular = ctx
                    .db
                    .character()
                    .id()
                    .find(character_id)
                    .and_then(|character| character.current_settlement_id)
                    .and_then(|id| ctx.db.settlement().id().find(&id))
                    .map_or(WrittenLanguage::German, |settlement| {
                        if settlement.languages.dominant_german() == OralLanguage::Low {
                            WrittenLanguage::Low
                        } else {
                            WrittenLanguage::German
                        }
                    });
                skills
                    .written_languages
                    .add_direct(vernacular, work_hours * profile.vernacular);
                skills
                    .written_languages
                    .add_direct(WrittenLanguage::Latin, work_hours * profile.latin);
                if profile.religious {
                    let religion = apprenticeship_for_service(ctx, character_id, "religion")
                        .and_then(|row| row.religion_id)
                        .as_deref()
                        .and_then(OfficialReligion::from_id);
                    match religion {
                        Some(OfficialReligion::RomanCatholic) => skills
                            .written_languages
                            .add_direct(WrittenLanguage::Latin, work_hours),
                        Some(OfficialReligion::Judaism) => {
                            skills
                                .written_languages
                                .add_direct(WrittenLanguage::Hebrew, work_hours * 0.75);
                            skills
                                .written_languages
                                .add_direct(WrittenLanguage::Yiddish, work_hours * 0.25);
                        }
                        _ => skills
                            .written_languages
                            .add_direct(WrittenLanguage::German, work_hours),
                    }
                }
            }
        }
        if minutes == 0 || service_id != Some("religion") {
            continue;
        }
        if let Some(religion) = apprenticeship_for_service(ctx, character_id, "religion")
            .and_then(|row| row.religion_id)
            .as_deref()
            .and_then(OfficialReligion::from_id)
        {
            let trained = elapsed as f32 / MINUTES_PER_DAY as f32 * f32::from(minutes) / 60.0;
            hours.religion.add_direct(religion, trained);
        }
    }
    skills.polearm_hours = hours.polearm;
    skills.axe_hours = hours.axe;
    skills.bludgeon_hours = hours.bludgeon;
    skills.sword_hours = hours.sword;
    skills.knife_hours = hours.knife;
    skills.dodge_hours = hours.dodge;
    skills.block_hours = hours.block;
    skills.bow_hours = hours.bow;
    skills.crossbow_hours = hours.crossbow;
    skills.firearm_hours = hours.firearm;
    skills.throw_hours = hours.throw;
    skills.will_hours = hours.will;
    skills.insight_hours = hours.insight;
    skills.self_awareness_hours = hours.self_awareness;
    skills.humor_hours = hours.humor;
    skills.command_hours = hours.command;
    skills.deception_hours = hours.deception;
    skills.seduction_hours = hours.seduction;
    skills.medicine_hours = hours.medicine;
    skills.cooking_hours = hours.cooking;
    skills.religion_hours = hours.religion;
    skills.stealth_hours = hours.stealth;
    skills.balance_hours = hours.balance;
    skills.anatomy_hours = hours.anatomy;
    skills.tailoring_hours = hours.tailoring;
    skills.smithing_hours = hours.smithing;
}

pub(crate) fn core_schedule(schedule: &ScheduleAllocation) -> DailySchedule {
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

fn initialize_notoriety(ctx: &ReducerContext, character_id: u64) {
    if ctx
        .db
        .character_notoriety()
        .character_id()
        .find(character_id)
        .is_none()
    {
        ctx.db.character_notoriety().insert(CharacterNotoriety {
            character_id,
            value: 0.0,
        });
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ActivityRisks {
    pub thievery_discovery: f32,
    pub raiding_retaliation: f32,
}

fn apply_activity_outcomes(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
    elapsed: u64,
    interval_end_minute: u64,
) -> Result<ActivityRisks, String> {
    apply_activity_outcomes_inner(
        ctx,
        character_id,
        schedule,
        elapsed,
        interval_end_minute,
        true,
    )
}

fn apply_activity_outcomes_without_leisure(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
    elapsed: u64,
    interval_end_minute: u64,
) -> Result<ActivityRisks, String> {
    apply_activity_outcomes_inner(
        ctx,
        character_id,
        schedule,
        elapsed,
        interval_end_minute,
        false,
    )
}

fn apply_activity_outcomes_inner(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
    elapsed: u64,
    interval_end_minute: u64,
    apply_leisure: bool,
) -> Result<ActivityRisks, String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let Some(settlement_id) = character.current_settlement_id.as_ref() else {
        return Ok(ActivityRisks::default());
    };
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(settlement_id)
        .ok_or("Character's settlement not found")?;
    let attributes: CharacterAttributes = ctx
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
    let skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?;
    let stats: CharacterStats = ctx
        .db
        .character_stats()
        .character_id()
        .find(character_id)
        .ok_or("Character stats not found")?;
    let equip = ctx
        .db
        .character_equip()
        .character_id()
        .find(character_id)
        .ok_or("Character equipment not found")?;
    let equipment = StrategicEquipment::load(ctx, character_id, &equip);
    let strength = attributes.limb_attr_by_weight_by_parts(
        LimbAttribute::Strength,
        &limbs,
        LimbWeights::all_equal(),
    );
    let endurance = attributes.attr_by_parts(SimpleAttribute::Endurance, &limbs);
    let stealth = skills.skill_check_by_parts(
        Skill::Stealth,
        &attributes,
        &limbs,
        &stats,
        &equipment,
        LimbWeights::all_equal(),
    );
    let capability = crate::capability::evaluate_character(ctx, character_id)?;
    let population = adventuresim_core::activity::settlement_population_scale(
        settlement.population_level,
        settlement.population_estimate,
    );
    let combat = capability
        .weapon_precision
        .max(capability.athletics)
        .max(capability.endurance);
    let outcome = settlement_activity_outcome(
        core_schedule(schedule),
        elapsed,
        ActivityOutcomeInputs {
            strength_check: strength,
            endurance_check: endurance,
            stealth_check: stealth,
            combat_check: combat,
            population_scale: population,
        },
    );
    if outcome.gold_earned > 0 {
        crate::item::credit_personal_currency(
            ctx,
            character_id,
            &settlement.id,
            outcome.gold_earned,
        )?;
    }
    if outcome.carousing_morale > 0.0 {
        crate::condition::record_morale_event(
            ctx,
            character_id,
            "carousing",
            outcome.carousing_morale,
            Some("activity:carousing".into()),
        )?;
    }
    if outcome.virtue_lost > 0.0 {
        let existing = ctx.db.character_virtue().character_id().find(character_id);
        let mut virtue = existing.clone().unwrap_or(CharacterVirtue {
            character_id,
            value: 0.0,
        });
        virtue.value -= outcome.virtue_lost;
        if existing.is_some() {
            ctx.db.character_virtue().character_id().update(virtue);
        } else {
            ctx.db.character_virtue().insert(virtue);
        }
    }
    apply_profession_outcomes(ctx, character_id, schedule, elapsed, &settlement.id)?;
    initialize_notoriety(ctx, character_id);
    let notoriety_gain = outcome.notoriety_gained;
    if notoriety_gain > 0.0 {
        let mut notoriety = ctx
            .db
            .character_notoriety()
            .character_id()
            .find(character_id)
            .ok_or("Character notoriety not found")?;
        notoriety.value += notoriety_gain;
        ctx.db
            .character_notoriety()
            .character_id()
            .update(notoriety);
    }
    if apply_leisure {
        crate::condition::apply_settlement_leisure_condition(
            ctx,
            character_id,
            core_schedule(schedule),
            elapsed,
            interval_end_minute,
        )?;
    }
    Ok(ActivityRisks {
        thievery_discovery: outcome.thievery_discovery_chance,
        raiding_retaliation: outcome.raiding_retaliation_chance,
    })
}

fn immediate_activity_schedule(
    activity: ImmediateActivity,
    minutes: u16,
    service_id: Option<&str>,
) -> ScheduleAllocation {
    let mut schedule = ScheduleAllocation::default();
    match activity {
        ImmediateActivity::Prayer => schedule.prayer_minutes = minutes,
        ImmediateActivity::CombatTraining => schedule.combat_training_minutes = minutes,
        ImmediateActivity::Carousing => schedule.carousing_minutes = minutes,
        ImmediateActivity::Apprenticeship => {
            schedule.apprenticeship_minutes = minutes;
            schedule.apprenticeship_service_id = service_id.map(str::to_owned);
        }
        ImmediateActivity::ProfessionPractice => {
            schedule.profession_practice_minutes = minutes;
            schedule.profession_service_id = service_id.map(str::to_owned);
        }
        ImmediateActivity::Labor => schedule.labor_minutes = minutes,
        ImmediateActivity::Thievery => schedule.thievery_minutes = minutes,
        ImmediateActivity::Raiding => schedule.raiding_minutes = minutes,
    }
    schedule
}

/// Perform one selected activity continuously. Unlike settlement rest this
/// neither convalesces, repairs, washes, heals, supplies an inn, nor consults
/// or mutates the saved daily plan.
#[reducer]
pub fn perform_immediate_activity(
    ctx: &ReducerContext,
    character_id: u64,
    activity: ImmediateActivity,
    requested_minutes: u64,
    service_id: Option<&str>,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    let character = crate::character::require_living_character(ctx, character_id)?;
    if character.current_settlement_id.is_none() {
        return Err("Activities may only be performed at a settlement".into());
    }
    if !(60..=MINUTES_PER_DAY).contains(&requested_minutes) || requested_minutes % 60 != 0 {
        return Err("Activity duration must use whole hours from one to 24 hours".into());
    }
    ensure_character_time(ctx, character_id)?;
    let _ = refresh_clock(ctx)?;
    let minutes = u16::try_from(requested_minutes).map_err(|_| "Activity duration is too long")?;
    let schedule = immediate_activity_schedule(activity, minutes, service_id);
    validate_profession_schedule(ctx, character_id, &schedule)?;

    let mut character_time = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Character time record not found")?;
    let injury_limit =
        crate::surgery::preview_elapsed_for_injuries(ctx, character_id, requested_minutes, false)?;
    let (elapsed, terminal) =
        crate::disease::clip_elapsed_for_disease(ctx, character_id, injury_limit, false)?;
    let settled = crate::surgery::settle_injuries(ctx, character_id, elapsed, false)?;
    let elapsed = settled.elapsed;
    character_time.minutes = character_time
        .minutes
        .checked_add(elapsed)
        .ok_or("Character clock overflow")?;
    let interval_end = character_time.minutes;
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
    crate::social::settle_shared_party_time(ctx, character_id);
    crate::condition::apply_elapsed_needs(ctx, character_id, elapsed)?;
    crate::disease::finish_disease_interval(ctx, character_id, terminal)?;
    if terminal.is_some() || !settled.alive {
        return Ok(());
    }

    // The activity allocation describes this one interval directly. Applying
    // it over one canonical day makes both linear and saturating effects use
    // the selected number of minutes, while the personal clock advances only
    // by the actual interval (which may have been clipped by an incident).
    let effective_minutes = u16::try_from(elapsed.min(requested_minutes)).unwrap_or(minutes);
    let effective_schedule = immediate_activity_schedule(activity, effective_minutes, service_id);
    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?;
    let profile = activity_training_profile(ctx, character_id)?;
    apply_training(
        ctx,
        character_id,
        &mut skills,
        &effective_schedule,
        MINUTES_PER_DAY,
        profile,
    );
    ctx.db.character_skills().character_id().update(skills);
    let risks = apply_activity_outcomes_without_leisure(
        ctx,
        character_id,
        &effective_schedule,
        MINUTES_PER_DAY,
        interval_end,
    )?;
    if matches!(activity, ImmediateActivity::Prayer) {
        crate::condition::record_immediate_prayer_morale(ctx, character_id, effective_minutes)?;
    }
    if matches!(activity, ImmediateActivity::Labor) {
        let mut stats = ctx
            .db
            .character_stats()
            .character_id()
            .find(character_id)
            .ok_or("Character stats not found")?;
        stats.calories_used += f32::from(effective_minutes) / 60.0
            * adventuresim_core::strategic_schedule::LABOR_FATIGUE_PER_HOUR;
        ctx.db.character_stats().character_id().update(stats);
    }
    crate::strategic::maybe_trigger_activity_incident(ctx, character_id, risks)?;
    crate::capability::refresh_character_capability(ctx, character_id)?;
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    Ok(())
}

const ACTIVITY_MINUTE_SCALE: u64 = MINUTES_PER_DAY;
const APPRENTICE_COIN_INTERVAL: u64 = 8 * 60 * ACTIVITY_MINUTE_SCALE;
const JOURNEYMAN_REWARD_INTERVAL: u64 = 8 * 60 * ACTIVITY_MINUTE_SCALE;
const MASTER_REWARD_INTERVAL: u64 = 2 * 60 * ACTIVITY_MINUTE_SCALE;

fn apply_profession_outcomes(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
    elapsed: u64,
    settlement_id: &str,
) -> Result<(), String> {
    if schedule.apprenticeship_minutes > 0 {
        let service = schedule
            .apprenticeship_service_id
            .as_deref()
            .ok_or("Apprenticeship time requires a profession")?;
        let mut row = apprenticeship_for_service(ctx, character_id, service)
            .ok_or("That profession has not been learned")?;
        let old = row.apprenticeship_minutes_accrued;
        row.apprenticeship_minutes_accrued =
            old.saturating_add(elapsed.saturating_mul(u64::from(schedule.apprenticeship_minutes)));
        let due = row.apprenticeship_minutes_accrued / APPRENTICE_COIN_INTERVAL
            - old / APPRENTICE_COIN_INTERVAL;
        if due > 0 {
            crate::item::consume_personal_currency(ctx, character_id, due)?;
        }
        ctx.db.character_apprenticeship().id().update(row);
    }
    if schedule.profession_practice_minutes > 0 {
        let service = schedule
            .profession_service_id
            .as_deref()
            .ok_or("Profession practice time requires a profession")?;
        let tier = profession_tier_for(ctx, character_id, service)?;
        if tier == adventuresim_core::profession::ProfessionTier::Apprentice {
            return Err("Independent practice requires Journeyman rank (2)".into());
        }
        let mut row = apprenticeship_for_service(ctx, character_id, service)
            .ok_or("That profession has not been learned")?;
        let old = row.practice_minutes_accrued;
        row.practice_minutes_accrued = old.saturating_add(
            elapsed.saturating_mul(u64::from(schedule.profession_practice_minutes)),
        );
        let interval = if tier == adventuresim_core::profession::ProfessionTier::Master {
            MASTER_REWARD_INTERVAL
        } else {
            JOURNEYMAN_REWARD_INTERVAL
        };
        let reward = row.practice_minutes_accrued / interval - old / interval;
        let definition = adventuresim_core::profession::profession_for_service(service)
            .ok_or("Unknown profession service")?;
        match definition.practice_reward {
            adventuresim_core::profession::PracticeReward::Gold if reward > 0 => {
                crate::item::credit_personal_currency(
                    ctx,
                    character_id,
                    settlement_id,
                    u32::try_from(reward).unwrap_or(u32::MAX),
                )?;
            }
            adventuresim_core::profession::PracticeReward::Virtue if reward > 0 => {
                let mut virtue = ctx
                    .db
                    .character_virtue()
                    .character_id()
                    .find(character_id)
                    .unwrap_or(CharacterVirtue {
                        character_id,
                        value: 0.0,
                    });
                virtue.value += reward as f32;
                if ctx
                    .db
                    .character_virtue()
                    .character_id()
                    .find(character_id)
                    .is_some()
                {
                    ctx.db.character_virtue().character_id().update(virtue);
                } else {
                    ctx.db.character_virtue().insert(virtue);
                }
            }
            _ => {}
        }
        ctx.db.character_apprenticeship().id().update(row);
    }
    Ok(())
}

pub(crate) fn health_recovered_per_day(medicine_check: f32) -> f32 {
    BASE_HEALTH_RECOVERED_PER_DAY
        + medicine_check.clamp(0.0, 5.0) * HEALTH_RECOVERED_PER_MEDICINE_CHECK_PER_DAY
}

pub(crate) fn party_medicine_check(ctx: &ReducerContext, character_id: u64) -> Result<f32, String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or_else(|| "Character not found".to_string())?;
    let member_ids: Vec<u64> = if let Some(party_id) = character.party_id {
        crate::strategic::living_party_member_ids(ctx, &party_id)
    } else {
        vec![character_id]
    };
    let checks = member_ids
        .into_iter()
        .map(|member_id| {
            crate::capability::evaluate_character(ctx, member_id)
                .map(|capabilities| capabilities.medicine)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(aggregate_bounded_party_check(checks))
}

fn convalescence_minutes(ctx: &ReducerContext, character_id: u64, medicine_check: f32) -> u64 {
    crate::surgery::convalescence_minutes(ctx, character_id, medicine_check)
}

/// Spend completed game days at a settlement. Injuries receive all selected
/// rest first; only the remaining time is eligible for scheduled training.
#[reducer]
pub fn rest_at_settlement(
    ctx: &ReducerContext,
    character_id: u64,
    requested_days: u16,
    at_inn: bool,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    require_character_rest_service(ctx, character_id, at_inn)?;
    rest_for_minutes(
        ctx,
        character_id,
        u64::from(requested_days) * MINUTES_PER_DAY,
        at_inn,
        true,
    )
}

/// Spend an exact number of settlement minutes. This intentionally keeps
/// each character's clock independent: sharing a settlement does not force a
/// party to keep identical strategic times.
#[reducer]
pub fn rest_at_settlement_hours(
    ctx: &ReducerContext,
    character_id: u64,
    requested_minutes: u64,
    at_inn: bool,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    require_character_rest_service(ctx, character_id, at_inn)?;
    rest_for_minutes(ctx, character_id, requested_minutes, at_inn, true)
}

fn require_settlement_rest_service(
    profile: &adventuresim_world_schema::SettlementEconomyProfile,
    at_inn: bool,
) -> Result<(), String> {
    use adventuresim_core::settlement_economy::{
        SettlementDowntimeAccess, action_service_available, required_settlement_rest_service,
    };
    let service =
        required_settlement_rest_service(SettlementDowntimeAccess::PublicService { at_inn })
            .expect("public settlement rest always names a service");
    if action_service_available(profile, service) {
        Ok(())
    } else {
        Err("This settlement does not offer the requested rest service".into())
    }
}

fn require_character_rest_service(
    ctx: &ReducerContext,
    character_id: u64,
    at_inn: bool,
) -> Result<(), String> {
    let character = crate::character::require_living_character(ctx, character_id)?;
    let settlement_id = character
        .current_settlement_id
        .as_deref()
        .ok_or("Settlement rest requires the character to be at a settlement")?;
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(settlement_id.to_owned())
        .ok_or("Character's settlement not found")?;
    require_settlement_rest_service(&settlement.economy, at_inn)
}

fn rest_for_minutes(
    ctx: &ReducerContext,
    character_id: u64,
    requested_minutes: u64,
    at_inn: bool,
    explicit: bool,
) -> Result<(), String> {
    let character = crate::character::require_living_character(ctx, character_id)?;
    if character.current_settlement_id.is_none() {
        return Err("Settlement downtime requires the character to be at a settlement".into());
    }
    ensure_character_time(ctx, character_id)?;
    let _ = refresh_clock(ctx)?;
    let mut character_time = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character time record not found".to_string())?;
    if requested_minutes == 0 {
        return Ok(());
    }
    let conversation_choice = character.party_id.as_ref().and_then(|party_id| {
        let snapshot: Vec<_> = crate::strategic::living_party_member_ids(ctx, party_id)
            .into_iter()
            .filter_map(|id| {
                ctx.db
                    .character_skills()
                    .character_id()
                    .find(id)
                    .map(|skills| (id, skills.oral_languages))
            })
            .collect();
        adventuresim_world_schema::party_common_oral_choices(&snapshot)
            .into_iter()
            .find(|choice| choice.0 == character_id)
    });

    validate_settlement_rest_minutes(requested_minutes)?;

    let cost = requested_minutes
        .div_ceil(MINUTES_PER_DAY)
        .checked_mul(u64::from(INN_GOLD_PER_DAY))
        .ok_or("Inn cost overflow")?;
    if at_inn {
        crate::item::consume_personal_currency(ctx, character_id, cost)
            .map_err(|_| "Not enough coin to pay for the inn stay".to_string())?;
    }

    if explicit {
        crate::filth::wash_before_explicit_rest(ctx, character_id)?;
    }

    let injury_limit =
        crate::surgery::preview_elapsed_for_injuries(ctx, character_id, requested_minutes, true)?;
    let (elapsed, terminal) =
        crate::disease::clip_elapsed_for_disease(ctx, character_id, injury_limit, true)?;
    let medicine_check = party_medicine_check(ctx, character_id)?;
    let convalescing = convalescence_minutes(ctx, character_id, medicine_check).min(elapsed);
    let settled = crate::surgery::settle_injuries(ctx, character_id, elapsed, true)?;
    let elapsed = settled.elapsed;
    let starting_minute = character_time.minutes;
    character_time.minutes = character_time
        .minutes
        .checked_add(elapsed)
        .ok_or("Character clock overflow")?;
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
    crate::social::settle_shared_party_time(ctx, character_id);
    crate::condition::apply_elapsed_needs(ctx, character_id, elapsed)?;
    crate::disease::finish_disease_interval(ctx, character_id, terminal)?;
    if terminal.is_some() || !settled.alive {
        return Ok(());
    }
    crate::alcohol::process_rest_evenings(
        ctx,
        character_id,
        starting_minute,
        starting_minute.saturating_add(elapsed),
        true,
    )?;

    let (smithing_skill, tailoring_skill) = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .map(|skills| {
            (
                Skill::Smithing.training_rank(skills.smithing_hours).floor() as u8,
                Skill::Tailoring
                    .training_rank(skills.tailoring_hours)
                    .floor() as u8,
            )
        })
        .unwrap_or((0, 0));
    let maintenance_elapsed = crate::repair::field_repair(
        ctx,
        character_id,
        smithing_skill,
        tailoring_skill,
        elapsed.saturating_sub(convalescing),
    );
    let training_elapsed = elapsed
        .saturating_sub(convalescing)
        .saturating_sub(maintenance_elapsed);
    let priority_rest_elapsed = elapsed.saturating_sub(training_elapsed);
    if priority_rest_elapsed > 0 {
        crate::condition::apply_settlement_leisure_condition(
            ctx,
            character_id,
            DailySchedule::default(),
            priority_rest_elapsed,
            starting_minute.saturating_add(priority_rest_elapsed),
        )?;
    }
    if training_elapsed > 0 {
        let schedule = ctx
            .db
            .character_training_schedule()
            .character_id()
            .find(character_id)
            .ok_or_else(|| "Character training schedule not found".to_string())?;
        let mut skills = ctx
            .db
            .character_skills()
            .character_id()
            .find(character_id)
            .ok_or_else(|| "Character skill record not found".to_string())?;
        let activities = activity_training_profile(ctx, character_id)?;
        apply_training(
            ctx,
            character_id,
            &mut skills,
            &schedule.downtime,
            training_elapsed,
            activities,
        );
        if let Some((_, language, coefficient)) = conversation_choice {
            skills.oral_languages.add_direct(
                language,
                training_elapsed as f32 / 60.0 * (2.0 / 3.0) * coefficient,
            );
        }
        ctx.db.character_skills().character_id().update(skills);
        let risks = apply_activity_outcomes(
            ctx,
            character_id,
            &schedule.downtime,
            training_elapsed,
            starting_minute.saturating_add(elapsed),
        )?;
        crate::strategic::maybe_trigger_activity_incident(ctx, character_id, risks)?;
    }

    crate::condition::apply_rest_condition(ctx, character_id, elapsed)?;
    crate::food::clear_stomach_fullness(ctx, character_id);
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

fn validate_settlement_rest_minutes(requested_minutes: u64) -> Result<(), String> {
    if (MIN_SETTLEMENT_REST_MINUTES..=MAX_SETTLEMENT_REST_MINUTES).contains(&requested_minutes) {
        Ok(())
    } else {
        Err("Settlement rest must last between one hour and one year".into())
    }
}

/// Venue-neutral private downtime for system-owned clock synchronization,
/// convalescence, and private holy-day observance. Public service reducers
/// must authorize an Inn or Temple before entering `rest_for_minutes`.
pub(crate) fn spend_private_settlement_downtime(
    ctx: &ReducerContext,
    character_id: u64,
    requested_minutes: u64,
    explicit: bool,
) -> Result<(), String> {
    debug_assert_eq!(
        adventuresim_core::settlement_economy::required_settlement_rest_service(
            adventuresim_core::settlement_economy::SettlementDowntimeAccess::PrivateSystem,
        ),
        None
    );
    rest_for_minutes(ctx, character_id, requested_minutes, false, explicit)
}

/// Move living party clocks forward to their latest member without ever
/// rewinding chronology. Lagging members receive ordinary location-appropriate
/// downtime. A one-year cap rejects corrupt/pathological party skew.
pub(crate) fn synchronize_party_departure_time(
    ctx: &ReducerContext,
    member_ids: &[u64],
) -> Result<u64, String> {
    if member_ids.is_empty() {
        return Err("Party has no living members".into());
    }
    for member_id in member_ids {
        ensure_character_time(ctx, *member_id)?;
    }
    let times = member_ids
        .iter()
        .map(|member_id| {
            ctx.db
                .character_time()
                .character_id()
                .find(*member_id)
                .map(|time| (*member_id, time.minutes))
                .ok_or_else(|| "Party member time record not found".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let departure = times.iter().map(|(_, minute)| *minute).max().unwrap_or(0);
    let earliest = times
        .iter()
        .map(|(_, minute)| *minute)
        .min()
        .unwrap_or(departure);
    if departure.saturating_sub(earliest) > MINUTES_PER_YEAR {
        return Err("Party clocks differ by more than one strategic year".into());
    }
    for (member_id, minute) in times {
        let elapsed = departure.saturating_sub(minute);
        if elapsed == 0 {
            continue;
        }
        let at_settlement = ctx
            .db
            .character()
            .id()
            .find(member_id)
            .is_some_and(|character| character.current_settlement_id.is_some());
        if at_settlement {
            spend_private_settlement_downtime(ctx, member_id, elapsed, false)?;
        } else {
            advance_personal_camp_time(ctx, member_id, elapsed)?;
        }
    }
    Ok(departure)
}

pub(crate) fn allowed_camp_schedule(schedule: &ScheduleAllocation) -> ScheduleAllocation {
    let mut allowed = schedule.clone();
    allowed.labor_minutes = 0;
    allowed.thievery_minutes = 0;
    allowed.raiding_minutes = 0;
    allowed
}

fn advance_personal_camp_time(
    ctx: &ReducerContext,
    member_id: u64,
    elapsed: u64,
) -> Result<(), String> {
    ensure_character_time(ctx, member_id)?;
    let mut time = ctx
        .db
        .character_time()
        .character_id()
        .find(member_id)
        .ok_or("Character time record not found")?;
    let starting_minute = time.minutes;
    let injury_limit = crate::surgery::preview_elapsed_for_injuries(ctx, member_id, elapsed, true)?;
    let (elapsed, terminal) =
        crate::disease::clip_elapsed_for_disease(ctx, member_id, injury_limit, true)?;
    let convalescing =
        convalescence_minutes(ctx, member_id, party_medicine_check(ctx, member_id)?).min(elapsed);
    let settled = crate::surgery::settle_injuries(ctx, member_id, elapsed, true)?;
    let elapsed = settled.elapsed;
    time.minutes = time.minutes.saturating_add(elapsed);
    ctx.db.character_time().character_id().update(time);
    crate::social::settle_shared_party_time(ctx, member_id);
    crate::condition::apply_elapsed_needs(ctx, member_id, elapsed)?;
    crate::disease::finish_disease_interval(ctx, member_id, terminal)?;
    if terminal.is_some() || !settled.alive {
        return Ok(());
    }
    crate::alcohol::process_rest_evenings(
        ctx,
        member_id,
        starting_minute,
        starting_minute.saturating_add(elapsed),
        false,
    )?;
    let starting_fatigue = ctx
        .db
        .character_stats()
        .character_id()
        .find(member_id)
        .map_or(0.0, |stats| stats.calories_used.max(0.0));
    crate::condition::apply_camp_rest_recovery_condition(ctx, member_id, elapsed)?;
    let fatigue_rest =
        adventuresim_core::strategic_time::minutes_until_fatigue_clears(starting_fatigue)
            .min(elapsed);
    let schedule = ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(member_id)
        .ok_or("Character training schedule not found")?;
    let allowed = allowed_camp_schedule(&schedule.downtime);
    let downtime = elapsed.saturating_sub(fatigue_rest.max(convalescing));
    if downtime > 0 {
        let mut skills = ctx
            .db
            .character_skills()
            .character_id()
            .find(member_id)
            .ok_or("Character skill record not found")?;
        let activities = activity_training_profile(ctx, member_id)?;
        apply_training(ctx, member_id, &mut skills, &allowed, downtime, activities);
        ctx.db.character_skills().character_id().update(skills);
        crate::condition::apply_settlement_leisure_condition(
            ctx,
            member_id,
            core_schedule(&allowed),
            downtime,
            starting_minute.saturating_add(elapsed),
        )?;
    }
    crate::capability::refresh_character_capability(ctx, member_id)?;
    Ok(())
}

/// Companions generated by the strategic layer do not wait for a player to
/// select a rest duration. Once the party reaches a settlement, they use the
/// ordinary settlement-rest path until their wounds are healed. The leader is
/// deliberately excluded: even a temporary leader may be player-controlled in
/// local development.
pub(crate) fn rest_temporary_party_member_until_healed_at_settlement(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<(), String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if !character.temporary || character.current_settlement_id.is_none() {
        return Ok(());
    }
    let Some(party_id) = character.party_id.as_ref() else {
        return Ok(());
    };
    if ctx
        .db
        .party_authority()
        .id()
        .find(party_id)
        .is_some_and(|party| party.leader_id == character_id)
    {
        return Ok(());
    }

    let recovery_minutes =
        convalescence_minutes(ctx, character_id, party_medicine_check(ctx, character_id)?);
    if recovery_minutes > 0 {
        spend_private_settlement_downtime(ctx, character_id, recovery_minutes, false)?;
    }
    Ok(())
}

/// Field rest is a party action from the map at a settlement, an en-route camp,
/// or a quest destination: the leader chooses a duration and every party member
/// spends the same strategic time without settlement replenishment or prices.
#[reducer]
pub fn rest_at_camp(
    ctx: &ReducerContext,
    character_id: u64,
    requested_minutes: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    crate::strategic::require_character_no_unresolved_encounter(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    if requested_minutes == 0 {
        return Ok(());
    }
    if requested_minutes > MINUTES_PER_YEAR {
        return Err("Camp rest cannot exceed one year".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Must be in a party to camp")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can rest the party at camp".into());
    }
    if party.current_settlement_id.is_none()
        && party.camp_destination.is_none()
        && party.current_case_site_id.is_none()
    {
        return Err("The party is not at a field rest location".into());
    }
    let members = crate::strategic::living_party_member_ids(ctx, &party_id);
    // This reducer is an explicit player-chosen rest. Washing precedes disease
    // and injury interval clipping, and dead members were excluded above.
    crate::filth::wash_party_before_explicit_rest(ctx, &members)?;
    let elapsed = members
        .iter()
        .try_fold(requested_minutes, |limit, member_id| {
            let disease =
                crate::disease::preview_elapsed_for_disease(ctx, *member_id, limit, true)?;
            let injury =
                crate::surgery::preview_elapsed_for_injuries(ctx, *member_id, limit, true)?;
            Ok::<u64, String>(limit.min(disease).min(injury))
        })?;
    let fatigue_before = party_fatigue_summary(ctx, &members)?;
    let language_snapshot: Vec<_> = members
        .iter()
        .filter_map(|id| {
            ctx.db
                .character_skills()
                .character_id()
                .find(*id)
                .map(|skills| (*id, skills.oral_languages))
        })
        .collect();
    let language_choices: BTreeMap<_, _> =
        adventuresim_world_schema::party_common_oral_choices(&language_snapshot)
            .into_iter()
            .map(|(id, language, coefficient)| (id, (language, coefficient)))
            .collect();
    for member_id in members {
        ensure_character_time(ctx, member_id)?;
        let mut time = ctx
            .db
            .character_time()
            .character_id()
            .find(member_id)
            .ok_or("Character time record not found")?;
        let starting_fatigue = ctx
            .db
            .character_stats()
            .character_id()
            .find(member_id)
            .map_or(0.0, |stats| stats.calories_used.max(0.0));
        let medicine_check = party_medicine_check(ctx, member_id)?;
        let convalescing = convalescence_minutes(ctx, member_id, medicine_check).min(elapsed);
        let (_, terminal) =
            crate::disease::clip_elapsed_for_disease(ctx, member_id, elapsed, true)?;
        let settled = crate::surgery::settle_injuries(ctx, member_id, elapsed, true)?;
        let member_elapsed = settled.elapsed;
        time.minutes = time.minutes.saturating_add(member_elapsed);
        let interval_end_minute = time.minutes;
        ctx.db.character_time().character_id().update(time);
        crate::social::settle_shared_party_time(ctx, member_id);
        crate::condition::apply_elapsed_needs(ctx, member_id, member_elapsed)?;
        crate::disease::finish_disease_interval(ctx, member_id, terminal)?;
        if terminal.is_some() || !settled.alive {
            continue;
        }
        crate::alcohol::process_rest_evenings(
            ctx,
            member_id,
            interval_end_minute.saturating_sub(member_elapsed),
            interval_end_minute,
            false,
        )?;
        crate::condition::apply_camp_rest_recovery_condition(ctx, member_id, member_elapsed)?;
        crate::food::clear_stomach_fullness(ctx, member_id);
        let convalescing = convalescing.min(member_elapsed);
        let (smithing_skill, tailoring_skill) = ctx
            .db
            .character_skills()
            .character_id()
            .find(member_id)
            .map(|skills| {
                (
                    Skill::Smithing.training_rank(skills.smithing_hours).floor() as u8,
                    Skill::Tailoring
                        .training_rank(skills.tailoring_hours)
                        .floor() as u8,
                )
            })
            .unwrap_or((0, 0));
        let maintenance = crate::repair::field_repair(
            ctx,
            member_id,
            smithing_skill,
            tailoring_skill,
            adventuresim_core::durability::remaining_after_priority(member_elapsed, convalescing),
        );
        let fatigue_rest =
            adventuresim_core::strategic_time::minutes_until_fatigue_clears(starting_fatigue)
                .min(member_elapsed);
        let priority = fatigue_rest.max(convalescing.saturating_add(maintenance));
        let downtime = member_elapsed.saturating_sub(priority);
        if downtime > 0 {
            let schedule = ctx
                .db
                .character_training_schedule()
                .character_id()
                .find(member_id)
                .ok_or("Character training schedule not found")?;
            let allowed = allowed_camp_schedule(&schedule.downtime);
            let mut skills = ctx
                .db
                .character_skills()
                .character_id()
                .find(member_id)
                .ok_or("Character skill record not found")?;
            let activities = activity_training_profile(ctx, member_id)?;
            apply_training(ctx, member_id, &mut skills, &allowed, downtime, activities);
            if let Some((language, coefficient)) = language_choices.get(&member_id) {
                skills.oral_languages.add_direct(
                    *language,
                    downtime as f32 / 60.0 * (2.0 / 3.0) * coefficient,
                );
            }
            ctx.db.character_skills().character_id().update(skills);
            crate::condition::apply_settlement_leisure_condition(
                ctx,
                member_id,
                core_schedule(&allowed),
                downtime,
                interval_end_minute,
            )?;
        }
        crate::capability::refresh_character_capability(ctx, member_id)?;
    }
    let living_after = crate::strategic::living_party_member_ids(ctx, &party_id);
    let fatigue_after = party_fatigue_summary(ctx, &living_after)?;
    crate::strategic::record_party_camp_rest(
        ctx,
        &party_id,
        elapsed,
        fatigue_before.0,
        fatigue_after.0,
        fatigue_after.1,
    )?;
    // Reforecast the untravelled part from the fatigue that this particular
    // rest actually removed. The journey record retains all reached camps.
    crate::strategic::refresh_party_journey_forecast(ctx, &party_id)?;
    crate::strategic::reconcile_party_objective_continuity(ctx, &party_id)?;
    Ok(())
}

fn party_fatigue_summary(ctx: &ReducerContext, members: &[u64]) -> Result<(f32, f32), String> {
    if members.is_empty() {
        return Ok((0.0, 0.0));
    }
    let mut total = 0.0;
    let mut maximum = 0.0_f32;
    for member_id in members {
        let attributes = ctx
            .db
            .character_attributes()
            .character_id()
            .find(*member_id)
            .ok_or("Party member attributes not found")?;
        let limbs = ctx
            .db
            .character_limbs()
            .character_id()
            .find(*member_id)
            .ok_or("Party member limbs not found")?;
        let stats = ctx
            .db
            .character_stats()
            .character_id()
            .find(*member_id)
            .ok_or("Party member stats not found")?;
        let capacity = attributes
            .attr_by_parts(SimpleAttribute::Endurance, &limbs)
            .max(0.01)
            * 1_000.0;
        let fatigue = stats.calories_used.max(0.0) / capacity;
        total += fatigue;
        maximum = maximum.max(fatigue);
    }
    Ok((total / members.len() as f32, maximum))
}

/// Advance through elapsed time. Returns true when a character was forced to
/// catch up from more than a year behind; callers should skip their action.
pub fn synchronize_character(ctx: &ReducerContext, character_id: u64) -> Result<bool, String> {
    ensure_character_time(ctx, character_id)?;
    if ctx
        .db
        .character()
        .id()
        .find(character_id)
        .is_some_and(|character| !character.alive)
    {
        // A corpse's strategic minute remains the death minute. Lazy reads must
        // not train, recover, consume provisions, or advance it.
        return Ok(false);
    }
    let official_minutes = refresh_clock(ctx)?;
    let mut character_time = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character time record not found".to_string())?;
    let forced_catch_up =
        official_minutes.saturating_sub(character_time.minutes) > MINUTES_PER_YEAR;
    let target_minutes = if forced_catch_up {
        official_minutes.saturating_sub(MINUTES_PER_YEAR)
    } else {
        official_minutes
    };
    let requested_elapsed = target_minutes.saturating_sub(character_time.minutes);
    if requested_elapsed == 0 {
        return Ok(forced_catch_up);
    }
    let injury_limit =
        crate::surgery::preview_elapsed_for_injuries(ctx, character_id, requested_elapsed, true)?;
    let (elapsed, terminal) =
        crate::disease::clip_elapsed_for_disease(ctx, character_id, injury_limit, true)?;
    let convalescing =
        convalescence_minutes(ctx, character_id, party_medicine_check(ctx, character_id)?)
            .min(elapsed);
    let settled = crate::surgery::settle_injuries(ctx, character_id, elapsed, true)?;
    let elapsed = settled.elapsed;
    character_time.minutes = character_time.minutes.saturating_add(elapsed);
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
    crate::social::settle_shared_party_time(ctx, character_id);
    crate::disease::finish_disease_interval(ctx, character_id, terminal)?;
    if terminal.is_some() || !settled.alive {
        return Ok(forced_catch_up);
    }
    let schedule = ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character training schedule not found".to_string())?;
    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character skill record not found".to_string())?;
    let activities = activity_training_profile(ctx, character_id)?;
    let training_elapsed = elapsed.saturating_sub(convalescing);
    apply_training(
        ctx,
        character_id,
        &mut skills,
        &schedule.downtime,
        training_elapsed,
        activities,
    );
    ctx.db.character_skills().character_id().update(skills);
    let risks = apply_activity_outcomes(
        ctx,
        character_id,
        &schedule.downtime,
        training_elapsed,
        target_minutes,
    )?;
    crate::strategic::maybe_trigger_activity_incident(ctx, character_id, risks)?;
    if ctx
        .db
        .character()
        .id()
        .find(character_id)
        .is_some_and(|character| character.current_settlement_id.is_some())
    {
        crate::condition::replenish_needs_at_settlement(ctx, character_id)?;
        crate::capability::refresh_character_capability(ctx, character_id)?;
    }
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    Ok(forced_catch_up)
}

/// Explicitly synchronize an accessed character before strategic UI reads.
#[reducer]
pub fn synchronize_character_time(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    synchronize_character(ctx, character_id).map(|_| ())
}

#[reducer]
pub fn update_training_schedule(
    ctx: &ReducerContext,
    character_id: u64,
    downtime: ScheduleAllocation,
    _travel: ScheduleAllocation,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    if synchronize_character(ctx, character_id)? {
        return Ok(());
    }
    let schedule = CharacterTrainingSchedule {
        character_id,
        downtime,
        travel: ScheduleAllocation::default(),
    };
    if schedule.downtime.allocated_minutes() > MINUTES_PER_DAY {
        return Err("The downtime plan must fit within 24 hours".into());
    }
    if !schedule.downtime.uses_quarter_hours() {
        return Err("Schedule allocations must use 15-minute increments".into());
    }
    validate_profession_schedule(ctx, character_id, &schedule.downtime)?;
    ctx.db
        .character_training_schedule()
        .character_id()
        .update(schedule);
    crate::condition::refresh_character_strategic_condition(ctx, character_id).map(|_| ())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settlement_rest_rejects_unavailable_inn_and_temple_services() {
        use adventuresim_world_schema::SettlementService;

        let mut profile = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
        assert!(require_settlement_rest_service(&profile, true).is_ok());
        assert!(require_settlement_rest_service(&profile, false).is_err());
        profile.services.clear();
        assert!(require_settlement_rest_service(&profile, true).is_err());
        profile.services.push(SettlementService::Temple);
        assert!(require_settlement_rest_service(&profile, false).is_ok());
    }

    #[test]
    fn settlement_rest_accepts_exact_wake_minutes_with_bounded_duration() {
        assert!(validate_settlement_rest_minutes(36 * 60 + 32).is_ok());
        assert!(validate_settlement_rest_minutes(2 * MINUTES_PER_DAY).is_ok());
        assert!(validate_settlement_rest_minutes(MIN_SETTLEMENT_REST_MINUTES).is_ok());
        assert!(validate_settlement_rest_minutes(MAX_SETTLEMENT_REST_MINUTES).is_ok());
        assert!(validate_settlement_rest_minutes(MIN_SETTLEMENT_REST_MINUTES - 1).is_err());
        assert!(validate_settlement_rest_minutes(MAX_SETTLEMENT_REST_MINUTES + 1).is_err());
    }

    #[test]
    fn settlement_rest_consumes_elapsed_needs_once_in_terminal_safe_order() {
        let source = include_str!("time.rs");
        let rest = source
            .split("fn rest_for_minutes")
            .nth(1)
            .and_then(|tail| tail.split("fn validate_settlement_rest_minutes").next())
            .expect("settlement rest implementation");
        let needs = "crate::condition::apply_elapsed_needs(ctx, character_id, elapsed)?";
        assert_eq!(rest.matches(needs).count(), 1);
        assert!(rest.find("settle_shared_party_time").unwrap() < rest.find(needs).unwrap());
        assert!(rest.find(needs).unwrap() < rest.find("finish_disease_interval").unwrap());
        assert!(
            rest.find("finish_disease_interval").unwrap()
                < rest.find("terminal.is_some()").unwrap()
        );
        assert!(
            rest.find("terminal.is_some()").unwrap() < rest.find("clear_stomach_fullness").unwrap()
        );
    }

    #[test]
    fn immediate_activity_schedule_contains_only_the_selected_interval() {
        let schedule = immediate_activity_schedule(
            ImmediateActivity::ProfessionPractice,
            180,
            Some("weapons"),
        );
        assert_eq!(schedule.profession_practice_minutes, 180);
        assert_eq!(schedule.profession_service_id.as_deref(), Some("weapons"));
        assert_eq!(schedule.allocated_minutes(), 180);
        let prayer = immediate_activity_schedule(ImmediateActivity::Prayer, 60, None);
        assert_eq!(prayer.prayer_minutes, 60);
        assert_eq!(prayer.allocated_minutes(), 60);
    }

    #[test]
    fn activity_training_uses_the_daily_minute_allocation() {
        let allocation = ScheduleAllocation {
            combat_training_minutes: 90,
            labor_minutes: 480,
            prayer_minutes: 60,
            ..Default::default()
        };
        let mut hours = SkillHours::default();
        apply_schedule_training(
            &mut hours,
            core_schedule(&allocation),
            MINUTES_PER_DAY * 2,
            ActivityTrainingProfile {
                combat: adventuresim_core::strategic_schedule::CombatTrainingProfile {
                    weapons: adventuresim_core::equipment::WeaponSkillDistribution {
                        sword: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
        );
        assert!((hours.sword - 0.1875).abs() < f32::EPSILON);
        assert!((hours.dodge - 0.1875).abs() < f32::EPSILON);
        assert!((hours.balance - 0.1875).abs() < f32::EPSILON);
        assert!((hours.will - 4.1875).abs() < f32::EPSILON);
        assert_eq!(allocation.allocated_minutes(), 630);
    }

    #[test]
    fn medicine_check_sets_the_daily_healing_rate() {
        assert!((health_recovered_per_day(0.0) - 0.01).abs() < f32::EPSILON);
        assert!((health_recovered_per_day(2.5) - 0.035).abs() < f32::EPSILON);
        assert!((health_recovered_per_day(5.0) - 0.06).abs() < f32::EPSILON);
        assert!((health_recovered_per_day(8.0) - 0.06).abs() < f32::EPSILON);
    }
}
