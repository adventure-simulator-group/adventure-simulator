use adventuresim_core::strategic_schedule::{
    ActivityOutcomeInputs, DailySchedule, SkillHours, apply_religion_training,
    apply_schedule_training, settlement_activity_outcome,
};
use adventuresim_core::strategic_time::{
    MINUTES_PER_DAY, MINUTES_PER_YEAR, WORLD_START_MINUTE, allocated_schedule_minutes,
    official_minutes as calculate_official_minutes,
};
use adventuresim_core::{capability::aggregate_bounded_party_check, prelude::*};
use spacetimedb::{ReducerContext, ScheduleAt, SpacetimeType, Table, reducer, table};

use crate::capability::StrategicEquipment;
use crate::character::character;
use crate::condition::{character_condition as _, character_strategic_condition as _};
use crate::disease::character_illness_status as _;
use crate::organization::organization_membership as _;
use crate::strategic::{party_authority, party_member as _};
use crate::{
    CharacterAttributes, CharacterSkills, CharacterStats, character_attributes, character_equip,
    character_limbs, character_skills, character_stats, settlement,
};
use adventuresim_world_schema::{OfficialReligion, OralLanguage};
use std::collections::BTreeMap;

/// Natural recovery without useful medical support while taking full
/// settlement downtime.
pub const BASE_HEALTH_RECOVERED_PER_DAY: f32 = 0.01;
/// Additional daily recovery supplied by each point of the party Physiology
/// check. Checks are capped at the five-point scale used by the strategic UI.
pub const HEALTH_RECOVERED_PER_PHYSIOLOGY_CHECK_PER_DAY: f32 = 0.01;
pub const INN_GOLD_PER_DAY: u32 = adventuresim_core::strategic_economy::INN_FULL_BOARD_GOLD_PER_DAY;
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
    pub apprenticeship_organization_id: Option<String>,
    pub profession_practice_minutes: u16,
    pub practice_organization_id: Option<String>,
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
            official_minutes: WORLD_START_MINUTE,
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
    let official_minutes = calculate_official_minutes(
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
    crate::organization::settle_membership_dues(ctx, character_id)?;
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
    crate::organization::settle_membership_dues(ctx, character_id)?;
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

fn validate_organization_schedule(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
) -> Result<(), String> {
    if schedule.apprenticeship_minutes > 0 {
        let organization_id = schedule
            .apprenticeship_organization_id
            .as_deref()
            .ok_or("Apprenticeship time requires an organization")?;
        crate::organization::require_activity_membership(ctx, character_id, organization_id)?;
    }
    if schedule.profession_practice_minutes > 0 {
        let organization_id = schedule
            .practice_organization_id
            .as_deref()
            .ok_or("Professional practice time requires an organization")?;
        let row =
            crate::organization::require_activity_membership(ctx, character_id, organization_id)?;
        let definition = adventuresim_core::organization::organization(organization_id)
            .ok_or("Unknown organization")?;
        let rank = definition
            .rank(&row.rank_id)
            .ok_or("Membership references an unknown organization rank")?;
        if !rank.practice_allowed {
            return Err("This organization rank does not permit independent practice".into());
        }
    }
    Ok(())
}

/// Sample organization eligibility at the beginning of an interval. Invalid
/// saved allocations become leisure without mutating the player's saved plan.
fn effective_organization_schedule(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
) -> ScheduleAllocation {
    let mut effective = schedule.clone();
    if effective.apprenticeship_minutes > 0
        && effective
            .apprenticeship_organization_id
            .as_deref()
            .is_none_or(|organization_id| {
                crate::organization::require_activity_membership(ctx, character_id, organization_id)
                    .is_err()
            })
    {
        effective.apprenticeship_minutes = 0;
    }
    if effective.profession_practice_minutes > 0 {
        let eligible = effective
            .practice_organization_id
            .as_deref()
            .and_then(|organization_id| {
                let membership = crate::organization::require_activity_membership(
                    ctx,
                    character_id,
                    organization_id,
                )
                .ok()?;
                let definition = adventuresim_core::organization::organization(organization_id)?;
                definition.rank(&membership.rank_id)
            })
            .is_some_and(|rank| rank.practice_allowed);
        if !eligible {
            effective.profession_practice_minutes = 0;
        }
    }
    effective
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

fn apply_oral_language_training(
    ctx: &ReducerContext,
    character_id: u64,
    languages: &mut adventuresim_world_schema::OralLanguageHours,
    language: OralLanguage,
    real_hours: f32,
) -> f32 {
    let instinct = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(0.0, |attributes| attributes.instinct);
    adventuresim_core::skill::apply_language_training(
        languages.direct_mut(language),
        real_hours,
        instinct,
    )
    .excess_effective_hours
}

fn apply_training(
    ctx: &ReducerContext,
    character_id: u64,
    skills: &mut CharacterSkills,
    schedule: &ScheduleAllocation,
    elapsed: u64,
    activities: adventuresim_core::strategic_schedule::ActivityTrainingProfile,
) -> f32 {
    let attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .expect("character attributes must exist while training");
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
        charm: skills.charm_hours,
        command: skills.command_hours,
        deception: skills.deception_hours,
        physiology: skills.physiology_hours,
        cooking: skills.cooking_hours,
        religion: skills.religion_hours,
        bestiary: skills.bestiary_hours,
        anatomy: skills.anatomy_hours,
        stealth: skills.stealth_hours,
        balance: skills.balance_hours,
        terrain_plains: skills.terrain_plains_hours,
        terrain_forest: skills.terrain_forest_hours,
        terrain_hills: skills.terrain_hills_hours,
        terrain_wetlands: skills.terrain_wetlands_hours,
        terrain_urban: skills.terrain_urban_hours,
        terrain_snow: skills.terrain_snow_hours,
        tailoring: skills.tailoring_hours,
        smithing: skills.smithing_hours,
    };
    let mut excess = apply_schedule_training(
        &mut hours,
        core_schedule(schedule),
        elapsed,
        activities,
        &attributes,
    );
    let prayer_religion = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .and_then(|condition| condition.religion_id)
        .as_deref()
        .and_then(OfficialReligion::from_id);
    excess += apply_religion_training(
        &mut hours.religion,
        elapsed,
        prayer_religion,
        schedule.prayer_minutes,
        &attributes,
    );
    if let Some(character) = ctx.db.character().id().find(character_id) {
        if let Some(settlement_id) = character.current_settlement_id {
            if let Some(settlement) = ctx.db.settlement().id().find(&settlement_id) {
                // Ordinary life supplies bounded ambient exposure during the
                // waking two-thirds of actual elapsed settlement time.
                let exposure = elapsed as f32 / 60.0 * (2.0 / 3.0);
                for (language, coefficient) in [
                    (
                        OralLanguage::EastCentral,
                        settlement.languages.east_central_bp,
                    ),
                    (
                        OralLanguage::WestCentral,
                        settlement.languages.west_central_bp,
                    ),
                    (OralLanguage::Low, settlement.languages.low_bp),
                ] {
                    excess += adventuresim_core::skill::apply_language_training(
                        skills.oral_languages.direct_mut(language),
                        exposure * f32::from(coefficient) / 10_000.0,
                        attributes.instinct,
                    )
                    .excess_effective_hours;
                }
            }
        }
    }
    for (minutes, organization_id) in [
        (
            schedule.apprenticeship_minutes,
            schedule.apprenticeship_organization_id.as_deref(),
        ),
        (
            schedule.profession_practice_minutes,
            schedule.practice_organization_id.as_deref(),
        ),
    ] {
        if minutes == 0 {
            continue;
        }
        if let Some(definition) =
            organization_id.and_then(adventuresim_core::organization::organization)
        {
            let work_hours = elapsed as f32 / MINUTES_PER_DAY as f32 * f32::from(minutes) / 60.0;
            excess += apply_organization_training(
                &mut hours,
                work_hours,
                definition,
                activities,
                &attributes,
            );
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
    skills.charm_hours = hours.charm;
    skills.command_hours = hours.command;
    skills.deception_hours = hours.deception;
    skills.physiology_hours = hours.physiology;
    skills.cooking_hours = hours.cooking;
    skills.religion_hours = hours.religion;
    skills.bestiary_hours = hours.bestiary;
    skills.anatomy_hours = hours.anatomy;
    skills.stealth_hours = hours.stealth;
    skills.balance_hours = hours.balance;
    skills.terrain_plains_hours = hours.terrain_plains;
    skills.terrain_forest_hours = hours.terrain_forest;
    skills.terrain_hills_hours = hours.terrain_hills;
    skills.terrain_wetlands_hours = hours.terrain_wetlands;
    skills.terrain_urban_hours = hours.terrain_urban;
    skills.terrain_snow_hours = hours.terrain_snow;
    skills.tailoring_hours = hours.tailoring;
    skills.smithing_hours = hours.smithing;
    excess
}

pub(crate) fn core_schedule(schedule: &ScheduleAllocation) -> DailySchedule {
    DailySchedule {
        combat_training_minutes: schedule.combat_training_minutes,
        carousing_minutes: schedule.carousing_minutes,
        apprenticeship_minutes: schedule.apprenticeship_minutes,
        profession_practice_minutes: schedule.profession_practice_minutes,
        labor: schedule.labor_minutes,
        prayer: schedule.prayer_minutes,
        thievery: schedule.thievery_minutes,
        raiding: schedule.raiding_minutes,
    }
}

fn apply_organization_training(
    hours: &mut SkillHours,
    work_hours: f32,
    definition: &adventuresim_core::organization::OrganizationDefinition,
    activities: adventuresim_core::strategic_schedule::ActivityTrainingProfile,
    attributes: &impl PlayerAttributes,
) -> f32 {
    use adventuresim_core::organization::TrainingTarget;
    let mut excess = 0.0;
    let mut award_direct = |skill: Skill, stored: &mut f32, real_hours: f32| {
        excess +=
            adventuresim_core::skill::apply_direct_training(skill, stored, real_hours, attributes)
                .excess_effective_hours;
    };
    for entry in &definition.activity.training {
        let award = work_hours * entry.weight;
        match &entry.target {
            TrainingTarget::FixedSkill { skill } => match skill.as_str() {
                "will" => award_direct(Skill::Will, &mut hours.will, award),
                "insight" => award_direct(Skill::Insight, &mut hours.insight, award),
                "charm" => award_direct(Skill::Charm, &mut hours.charm, award),
                "command" => award_direct(Skill::Command, &mut hours.command, award),
                "deception" => award_direct(Skill::Deception, &mut hours.deception, award),
                "physiology" => award_direct(Skill::Physiology, &mut hours.physiology, award),
                "cooking" => award_direct(Skill::Cooking, &mut hours.cooking, award),
                "anatomy" => award_direct(Skill::Anatomy, &mut hours.anatomy, award),
                "polearm" => award_direct(Skill::Polearm, &mut hours.polearm, award),
                "axe" => award_direct(Skill::Axe, &mut hours.axe, award),
                "bludgeon" => award_direct(Skill::Bludgeon, &mut hours.bludgeon, award),
                "sword" => award_direct(Skill::Sword, &mut hours.sword, award),
                "knife" => award_direct(Skill::Knife, &mut hours.knife, award),
                "bow" => award_direct(Skill::Bow, &mut hours.bow, award),
                "crossbow" => award_direct(Skill::Crossbow, &mut hours.crossbow, award),
                "firearm" => award_direct(Skill::Firearm, &mut hours.firearm, award),
                "throw" => award_direct(Skill::Throw, &mut hours.throw, award),
                "block" => award_direct(Skill::Block, &mut hours.block, award),
                "dodge" => award_direct(Skill::Dodge, &mut hours.dodge, award),
                "stealth" => award_direct(Skill::Stealth, &mut hours.stealth, award),
                "balance" => award_direct(Skill::Balance, &mut hours.balance, award),
                "terrain_plains" => {
                    award_direct(Skill::TerrainPlains, &mut hours.terrain_plains, award)
                }
                "terrain_forest" => {
                    award_direct(Skill::TerrainForest, &mut hours.terrain_forest, award)
                }
                "terrain_hills" => {
                    award_direct(Skill::TerrainHills, &mut hours.terrain_hills, award)
                }
                "terrain_wetlands" => {
                    award_direct(Skill::TerrainWetlands, &mut hours.terrain_wetlands, award)
                }
                "terrain_urban" => {
                    award_direct(Skill::TerrainUrban, &mut hours.terrain_urban, award)
                }
                "terrain_snow" => award_direct(Skill::TerrainSnow, &mut hours.terrain_snow, award),
                "tailoring" => award_direct(Skill::Tailoring, &mut hours.tailoring, award),
                "smithing" => award_direct(Skill::Smithing, &mut hours.smithing, award),
                _ => {}
            },
            TrainingTarget::Religion { religion } => {
                if let Some(religion) = OfficialReligion::from_id(religion) {
                    award_direct(Skill::Religion, hours.religion.direct_mut(religion), award);
                }
            }
            TrainingTarget::Bestiary { category } => {
                if let Some(category) = adventuresim_world_schema::BestiaryCategory::ALL
                    .into_iter()
                    .find(|value| format!("{value:?}").eq_ignore_ascii_case(category))
                {
                    award_direct(Skill::Bestiary, hours.bestiary.direct_mut(category), award);
                }
            }
            TrainingTarget::Terrain { terrain } => match terrain.as_str() {
                "plains" => award_direct(Skill::TerrainPlains, &mut hours.terrain_plains, award),
                "forest" => award_direct(Skill::TerrainForest, &mut hours.terrain_forest, award),
                "hills" => award_direct(Skill::TerrainHills, &mut hours.terrain_hills, award),
                "wetlands" => {
                    award_direct(Skill::TerrainWetlands, &mut hours.terrain_wetlands, award)
                }
                "urban" => award_direct(Skill::TerrainUrban, &mut hours.terrain_urban, award),
                "snow" => award_direct(Skill::TerrainSnow, &mut hours.terrain_snow, award),
                _ => {}
            },
            TrainingTarget::EquippedWeaponSkills => {
                let weights = activities.combat.weapons.weights();
                let total = weights.into_iter().sum::<f32>();
                if total > 0.0 {
                    for ((skill, target), weight) in [
                        (Skill::Polearm, &mut hours.polearm),
                        (Skill::Axe, &mut hours.axe),
                        (Skill::Bludgeon, &mut hours.bludgeon),
                        (Skill::Sword, &mut hours.sword),
                        (Skill::Knife, &mut hours.knife),
                        (Skill::Bow, &mut hours.bow),
                        (Skill::Crossbow, &mut hours.crossbow),
                        (Skill::Firearm, &mut hours.firearm),
                        (Skill::Throw, &mut hours.throw),
                    ]
                    .into_iter()
                    .zip(weights)
                    {
                        award_direct(skill, target, award * weight / total);
                    }
                }
            }
        }
    }
    excess
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ActivityRisks {
    pub thievery_discovery: f32,
    pub raiding_retaliation: f32,
    pub carousing_disorder: f32,
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
    apply_organization_outcomes(ctx, character_id, schedule, elapsed, &settlement.id)?;
    if outcome.infamy_gained > 0.0 {
        crate::reputation::record_event(
            ctx,
            format!("activity:{character_id}:{interval_end_minute}:crime"),
            character_id,
            &settlement.id,
            "criminal_activity",
            &interval_end_minute.to_string(),
            0,
            (outcome.infamy_gained * 100.0).round() as i32,
            interval_end_minute,
        )?;
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
        carousing_disorder: {
            let multiplier =
                match crate::personality::personality_or_neutral(ctx, character_id).temperance {
                    crate::personality::Temperance::Drunkard => 3.0,
                    crate::personality::Temperance::Temperate => 0.35,
                    crate::personality::Temperance::Neutral => 1.0,
                };
            (outcome.carousing_disorder_chance * multiplier).clamp(0.0, 0.95)
        },
    })
}

fn immediate_activity_schedule(
    activity: ImmediateActivity,
    minutes: u16,
    organization_id: Option<&str>,
) -> ScheduleAllocation {
    let mut schedule = ScheduleAllocation::default();
    match activity {
        ImmediateActivity::Prayer => schedule.prayer_minutes = minutes,
        ImmediateActivity::CombatTraining => schedule.combat_training_minutes = minutes,
        ImmediateActivity::Carousing => schedule.carousing_minutes = minutes,
        ImmediateActivity::Apprenticeship => {
            schedule.apprenticeship_minutes = minutes;
            schedule.apprenticeship_organization_id = organization_id.map(str::to_owned);
        }
        ImmediateActivity::ProfessionPractice => {
            schedule.profession_practice_minutes = minutes;
            schedule.practice_organization_id = organization_id.map(str::to_owned);
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
    organization_id: Option<&str>,
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
    let schedule = immediate_activity_schedule(activity, minutes, organization_id);
    validate_organization_schedule(ctx, character_id, &schedule)?;

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
        crate::organization::settle_membership_dues(ctx, character_id)?;
        return Ok(());
    }

    // The activity allocation describes this one interval directly. Applying
    // it over one canonical day makes both linear and saturating effects use
    // the selected number of minutes, while the personal clock advances only
    // by the actual interval (which may have been clipped by an incident).
    let effective_minutes = u16::try_from(elapsed.min(requested_minutes)).unwrap_or(minutes);
    let effective_schedule =
        immediate_activity_schedule(activity, effective_minutes, organization_id);
    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?;
    let profile = activity_training_profile(ctx, character_id)?;
    let excess = apply_training(
        ctx,
        character_id,
        &mut skills,
        &effective_schedule,
        MINUTES_PER_DAY,
        profile,
    );
    crate::condition::record_mastery_training_morale(
        ctx,
        character_id,
        u64::from(effective_minutes),
        excess,
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
    crate::organization::settle_membership_dues(ctx, character_id)?;
    Ok(())
}

const ACTIVITY_MINUTE_SCALE: u64 = MINUTES_PER_DAY;

fn apply_organization_outcomes(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
    elapsed: u64,
    settlement_id: &str,
) -> Result<(), String> {
    if schedule.apprenticeship_minutes > 0 {
        let organization_id = schedule
            .apprenticeship_organization_id
            .as_deref()
            .ok_or("Apprenticeship time requires an organization")?;
        crate::organization::increment_activity_accrual(
            ctx,
            character_id,
            organization_id,
            elapsed.saturating_mul(u64::from(schedule.apprenticeship_minutes)),
            0,
        );
    }
    if schedule.profession_practice_minutes > 0 {
        let organization_id = schedule
            .practice_organization_id
            .as_deref()
            .ok_or("Professional practice time requires an organization")?;
        let mut row = crate::organization::membership(ctx, character_id, organization_id)
            .ok_or("Eligible organization membership disappeared during the interval")?;
        let definition = adventuresim_core::organization::organization(organization_id)
            .ok_or("Unknown organization")?;
        let rank = definition
            .rank(&row.rank_id)
            .ok_or("Membership references an unknown organization rank")?;
        let old = row.practice_minutes_accrued;
        row.practice_minutes_accrued = old.saturating_add(
            elapsed.saturating_mul(u64::from(schedule.profession_practice_minutes)),
        );
        let interval =
            u64::from(rank.practice_reward_interval_minutes).saturating_mul(ACTIVITY_MINUTE_SCALE);
        if interval == 0 {
            return Err("Eligible organization rank has no practice reward cadence".into());
        }
        let reward = row.practice_minutes_accrued / interval - old / interval;
        match definition.activity.reward {
            adventuresim_core::organization::ActivityReward::Gold if reward > 0 => {
                crate::item::credit_personal_currency(
                    ctx,
                    character_id,
                    settlement_id,
                    u32::try_from(reward).unwrap_or(u32::MAX),
                )?;
            }
            adventuresim_core::organization::ActivityReward::Fame if reward > 0 => {
                let minute = ctx
                    .db
                    .character_time()
                    .character_id()
                    .find(character_id)
                    .map_or(0, |time| time.minutes);
                crate::reputation::record_event(
                    ctx,
                    format!("profession:{character_id}:{organization_id}:{minute}"),
                    character_id,
                    settlement_id,
                    "religious_practice",
                    organization_id,
                    i32::try_from(reward.saturating_mul(100)).unwrap_or(i32::MAX),
                    0,
                    minute,
                )?;
            }
            _ => {}
        }
        ctx.db.organization_membership().id().update(row);
    }
    Ok(())
}

pub(crate) fn health_recovered_per_day(physiology_check: f32) -> f32 {
    BASE_HEALTH_RECOVERED_PER_DAY
        + physiology_check.clamp(0.0, 5.0) * HEALTH_RECOVERED_PER_PHYSIOLOGY_CHECK_PER_DAY
}

pub(crate) fn party_physiology_check(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<f32, String> {
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
                .map(|capabilities| capabilities.physiology)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(aggregate_bounded_party_check(checks))
}

fn convalescence_minutes(ctx: &ReducerContext, character_id: u64, physiology_check: f32) -> u64 {
    crate::surgery::convalescence_minutes(ctx, character_id, physiology_check)
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
        true,
        None,
    )
    .map(|_| ())
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
    rest_for_minutes(
        ctx,
        character_id,
        requested_minutes,
        at_inn,
        true,
        true,
        None,
    )
    .map(|_| ())
}

fn patient_publicly_needs_rest(ctx: &ReducerContext, patient_id: u64) -> Result<bool, String> {
    let condition = ctx
        .db
        .character_strategic_condition()
        .character_id()
        .find(patient_id)
        .ok_or("Patient's public strategic condition is unavailable")?;
    let publicly_ill = ctx
        .db
        .character_illness_status()
        .character_id()
        .find(patient_id)
        .is_some_and(|illness| illness.symptomatic || illness.critical);
    Ok(condition.status != "ready" || publicly_ill)
}

/// Pay an inn directly for one day of a co-located party member's medically
/// necessary convalescence. The payer authorizes only the exact public quote;
/// no currency is transferred to the patient.
#[reducer]
pub fn sponsor_party_member_inn_rest(
    ctx: &ReducerContext,
    payer_id: u64,
    patient_id: u64,
    settlement_id: String,
    expected_cost: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, payer_id)?;
    if payer_id == patient_id {
        return Err("A patient who can pay must use ordinary settlement rest".into());
    }
    let payer = crate::character::require_living_character(ctx, payer_id)?;
    let patient = crate::character::require_living_character(ctx, patient_id)?;
    let party_id = payer
        .party_id
        .as_deref()
        .filter(|party_id| patient.party_id.as_deref() == Some(*party_id))
        .ok_or("Payer and patient must belong to the same party")?;
    for character_id in [payer_id, patient_id] {
        if !ctx
            .db
            .party_member()
            .party_id()
            .filter(party_id)
            .any(|member| member.character_id == character_id)
        {
            return Err("Payer and patient must have current party membership".into());
        }
    }
    if payer.current_settlement_id.as_deref() != Some(&settlement_id)
        || patient.current_settlement_id.as_deref() != Some(&settlement_id)
    {
        return Err("Payer and patient must be together at the named settlement".into());
    }
    require_character_rest_service(ctx, patient_id, true)?;
    if !patient_publicly_needs_rest(ctx, patient_id)? {
        return Err("Sponsored inn rest requires a patient who publicly needs recovery".into());
    }
    let authoritative_cost = inn_stay_cost(MINUTES_PER_DAY)?;
    if expected_cost != authoritative_cost {
        return Err("Sponsored inn quote is stale or invalid".into());
    }
    let patient_funds = crate::item::personal_currency_total(ctx, patient_id);
    if patient_funds >= authoritative_cost {
        return Err("Patient can afford ordinary inn rest without sponsorship".into());
    }
    let sponsorship_gap = authoritative_cost.saturating_sub(patient_funds);
    if crate::item::personal_currency_total(ctx, payer_id) < sponsorship_gap {
        return Err("Payer cannot afford the authoritative inn gap".into());
    }
    rest_for_minutes(
        ctx,
        patient_id,
        MINUTES_PER_DAY,
        true,
        true,
        true,
        Some(payer_id),
    )
    .map(|_| ())
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
    automatic_social: bool,
    inn_sponsor_id: Option<u64>,
) -> Result<u64, String> {
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
        return Ok(0);
    }
    let saved_schedule = ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character training schedule not found".to_string())?;
    let effective_schedule =
        effective_organization_schedule(ctx, character_id, &saved_schedule.downtime);
    let conversation_choice = character.party_id.as_ref().and_then(|party_id| {
        let snapshot: Vec<_> = crate::strategic::living_party_member_ids(ctx, party_id)
            .into_iter()
            .filter_map(|id| {
                ctx.db
                    .character_skills()
                    .character_id()
                    .find(id)
                    .map(|skills| {
                        let cap = ctx
                            .db
                            .character_attributes()
                            .character_id()
                            .find(id)
                            .map_or(0.0, |attributes| attributes.instinct * 1_000.0);
                        (id, skills.oral_languages, cap)
                    })
            })
            .collect();
        adventuresim_world_schema::party_common_oral_choices_capped(&snapshot)
            .into_iter()
            .find(|choice| choice.0 == character_id)
    });

    validate_settlement_rest_minutes(requested_minutes)?;

    let requested_cost = inn_stay_cost(requested_minutes)?;
    if at_inn {
        let patient_funds = crate::item::personal_currency_total(ctx, character_id);
        let sponsor_gap = requested_cost.saturating_sub(patient_funds);
        let payment_available = if sponsor_gap == 0 {
            true
        } else {
            inn_sponsor_id.is_some_and(|sponsor_id| {
                sponsor_id != character_id
                    && crate::item::personal_currency_total(ctx, sponsor_id) >= sponsor_gap
            })
        };
        if !payment_available {
            return Err("Not enough coin to pay for the inn stay".into());
        }
    }

    if explicit {
        crate::filth::wash_before_explicit_rest(ctx, character_id)?;
    }

    let schedule = ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character training schedule not found".to_string())?;
    let starting_minute = character_time.minutes;
    let requested_recovery = adventuresim_core::strategic_schedule::restorative_leisure_minutes(
        core_schedule(&schedule.downtime),
        starting_minute,
        requested_minutes,
    );
    let injury_limit = crate::surgery::preview_elapsed_for_injuries_with_rest_minutes(
        ctx,
        character_id,
        requested_minutes,
        requested_recovery,
    )?;
    let (elapsed, terminal) =
        crate::disease::clip_elapsed_for_disease(ctx, character_id, injury_limit, true)?;
    let physiology_check = party_physiology_check(ctx, character_id)?;
    let recovery_elapsed = adventuresim_core::strategic_schedule::restorative_leisure_minutes(
        core_schedule(&schedule.downtime),
        starting_minute,
        elapsed,
    );
    let convalescing =
        convalescence_minutes(ctx, character_id, physiology_check).min(recovery_elapsed);
    let settled = crate::surgery::settle_injuries_with_rest_minutes(
        ctx,
        character_id,
        elapsed,
        recovery_elapsed,
    )?;
    let elapsed = settled.elapsed;
    character_time.minutes = character_time
        .minutes
        .checked_add(elapsed)
        .ok_or("Character clock overflow")?;
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
    if at_inn {
        let elapsed_cost = inn_stay_cost(elapsed)?;
        let patient_contribution =
            crate::item::personal_currency_total(ctx, character_id).min(elapsed_cost);
        let sponsor_contribution = elapsed_cost.saturating_sub(patient_contribution);
        crate::item::consume_personal_currency(ctx, character_id, patient_contribution)
            .map_err(|_| "Not enough coin to pay for the inn stay".to_string())?;
        if sponsor_contribution > 0 {
            let sponsor_id = inn_sponsor_id.ok_or("Inn sponsorship became unavailable")?;
            crate::item::consume_personal_currency(ctx, sponsor_id, sponsor_contribution)
                .map_err(|_| "Not enough coin to pay for the inn stay".to_string())?;
        }
    }
    crate::social::settle_shared_party_time(ctx, character_id);
    crate::condition::apply_settlement_rest_elapsed_needs(ctx, character_id, elapsed, at_inn)?;
    crate::disease::finish_disease_interval(ctx, character_id, terminal)?;
    if terminal.is_some() || !settled.alive {
        crate::organization::settle_membership_dues(ctx, character_id)?;
        return Ok(0);
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
        .and_then(|skills| {
            let attributes = ctx
                .db
                .character_attributes()
                .character_id()
                .find(character_id)?;
            Some((
                Skill::Smithing
                    .capped_training_rank(skills.smithing_hours, &attributes)
                    .floor() as u8,
                Skill::Tailoring
                    .capped_training_rank(skills.tailoring_hours, &attributes)
                    .floor() as u8,
            ))
        })
        .unwrap_or((0, 0));
    let _maintenance_elapsed = crate::repair::field_repair(
        ctx,
        character_id,
        smithing_skill,
        tailoring_skill,
        recovery_elapsed.saturating_sub(convalescing),
    );
    let training_elapsed = elapsed;
    if training_elapsed > 0 {
        let mut skills = ctx
            .db
            .character_skills()
            .character_id()
            .find(character_id)
            .ok_or_else(|| "Character skill record not found".to_string())?;
        let activities = activity_training_profile(ctx, character_id)?;
        let mut excess = apply_training(
            ctx,
            character_id,
            &mut skills,
            &effective_schedule,
            training_elapsed,
            activities,
        );
        if let Some((_, language, coefficient)) = conversation_choice {
            excess += apply_oral_language_training(
                ctx,
                character_id,
                &mut skills.oral_languages,
                language,
                training_elapsed as f32 / 60.0 * (2.0 / 3.0) * coefficient,
            );
        }
        crate::condition::record_mastery_training_morale(
            ctx,
            character_id,
            training_elapsed,
            excess,
        );
        ctx.db.character_skills().character_id().update(skills);
        let risks = apply_activity_outcomes(
            ctx,
            character_id,
            &effective_schedule,
            training_elapsed,
            starting_minute.saturating_add(elapsed),
        )?;
        crate::strategic::maybe_trigger_activity_incident(ctx, character_id, risks)?;
    }

    crate::condition::apply_rest_condition(ctx, character_id, recovery_elapsed)?;
    crate::food::clear_stomach_fullness(ctx, character_id);
    crate::capability::refresh_character_capability(ctx, character_id)?;
    if automatic_social && recovery_elapsed > 0 {
        crate::social::apply_automatic_social_chats(ctx, character_id, recovery_elapsed)?;
    }
    crate::organization::settle_membership_dues(ctx, character_id)?;
    Ok(training_elapsed)
}

fn inn_stay_cost(requested_minutes: u64) -> Result<u64, String> {
    adventuresim_core::strategic_economy::inn_full_board_cost(requested_minutes)
        .ok_or_else(|| "Inn cost overflow".to_string())
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
    rest_for_minutes(
        ctx,
        character_id,
        requested_minutes,
        false,
        explicit,
        true,
        None,
    )
    .map(|_| ())
}

fn spend_private_settlement_downtime_deferred_social(
    ctx: &ReducerContext,
    character_id: u64,
    requested_minutes: u64,
) -> Result<u64, String> {
    rest_for_minutes(
        ctx,
        character_id,
        requested_minutes,
        false,
        false,
        false,
        None,
    )
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
    let mut automatic_chat_downtime = Vec::new();
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
            let downtime =
                spend_private_settlement_downtime_deferred_social(ctx, member_id, elapsed)?;
            if downtime > 0 {
                automatic_chat_downtime.push((member_id, downtime));
            }
        } else {
            let downtime = advance_personal_camp_time(ctx, member_id, elapsed, false)?;
            if downtime > 0 {
                automatic_chat_downtime.push((member_id, downtime));
            }
        }
    }
    automatic_chat_downtime.sort_by_key(|(member_id, _)| *member_id);
    for (member_id, downtime) in automatic_chat_downtime {
        crate::social::apply_automatic_social_chats(ctx, member_id, downtime)?;
    }
    Ok(departure)
}

pub(crate) fn allowed_camp_schedule(schedule: &ScheduleAllocation) -> ScheduleAllocation {
    let mut allowed = schedule.clone();
    allowed.apprenticeship_minutes = 0;
    allowed.apprenticeship_organization_id = None;
    allowed.profession_practice_minutes = 0;
    allowed.practice_organization_id = None;
    allowed.labor_minutes = 0;
    allowed.thievery_minutes = 0;
    allowed.raiding_minutes = 0;
    allowed
}

fn advance_personal_camp_time(
    ctx: &ReducerContext,
    member_id: u64,
    elapsed: u64,
    automatic_social: bool,
) -> Result<u64, String> {
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
        convalescence_minutes(ctx, member_id, party_physiology_check(ctx, member_id)?).min(elapsed);
    let settled = crate::surgery::settle_injuries(ctx, member_id, elapsed, true)?;
    let elapsed = settled.elapsed;
    time.minutes = time.minutes.saturating_add(elapsed);
    ctx.db.character_time().character_id().update(time);
    crate::organization::settle_membership_dues(ctx, member_id)?;
    crate::social::settle_shared_party_time(ctx, member_id);
    crate::condition::apply_elapsed_needs(ctx, member_id, elapsed)?;
    crate::disease::finish_disease_interval(ctx, member_id, terminal)?;
    if terminal.is_some() || !settled.alive {
        return Ok(0);
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
        let excess = apply_training(ctx, member_id, &mut skills, &allowed, downtime, activities);
        crate::condition::record_mastery_training_morale(ctx, member_id, downtime, excess);
        ctx.db.character_skills().character_id().update(skills);
        crate::condition::apply_settlement_leisure_condition(
            ctx,
            member_id,
            core_schedule(&allowed),
            downtime,
            starting_minute.saturating_add(elapsed),
        )?;
        if automatic_social {
            crate::social::apply_automatic_social_chats(ctx, member_id, downtime)?;
        }
    }
    crate::capability::refresh_character_capability(ctx, member_id)?;
    Ok(downtime)
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

    let recovery_minutes = convalescence_minutes(
        ctx,
        character_id,
        party_physiology_check(ctx, character_id)?,
    );
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
    if !crate::strategic::party_member_can_direct_field_rest(ctx, &party, character_id) {
        return Err(
            "Only the party leader, or a ready companion aiding an unready leader, can rest the party at camp"
                .into(),
        );
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
                .map(|skills| {
                    let cap = ctx
                        .db
                        .character_attributes()
                        .character_id()
                        .find(*id)
                        .map_or(0.0, |attributes| attributes.instinct * 1_000.0);
                    (*id, skills.oral_languages, cap)
                })
        })
        .collect();
    let language_choices: BTreeMap<_, _> =
        adventuresim_world_schema::party_common_oral_choices_capped(&language_snapshot)
            .into_iter()
            .map(|(id, language, coefficient)| (id, (language, coefficient)))
            .collect();
    let mut automatic_chat_downtime = Vec::new();
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
        let physiology_check = party_physiology_check(ctx, member_id)?;
        let convalescing = convalescence_minutes(ctx, member_id, physiology_check).min(elapsed);
        let (_, terminal) =
            crate::disease::clip_elapsed_for_disease(ctx, member_id, elapsed, true)?;
        let settled = crate::surgery::settle_injuries(ctx, member_id, elapsed, true)?;
        let member_elapsed = settled.elapsed;
        time.minutes = time.minutes.saturating_add(member_elapsed);
        let interval_end_minute = time.minutes;
        ctx.db.character_time().character_id().update(time);
        crate::organization::settle_membership_dues(ctx, member_id)?;
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
        let attributes = ctx
            .db
            .character_attributes()
            .character_id()
            .find(member_id)
            .ok_or("Character attributes not found")?;
        let (smithing_skill, tailoring_skill) = ctx
            .db
            .character_skills()
            .character_id()
            .find(member_id)
            .map(|skills| {
                (
                    Skill::Smithing
                        .capped_training_rank(skills.smithing_hours, &attributes)
                        .floor() as u8,
                    Skill::Tailoring
                        .capped_training_rank(skills.tailoring_hours, &attributes)
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
            let mut excess =
                apply_training(ctx, member_id, &mut skills, &allowed, downtime, activities);
            if let Some((language, coefficient)) = language_choices.get(&member_id) {
                excess += apply_oral_language_training(
                    ctx,
                    member_id,
                    &mut skills.oral_languages,
                    *language,
                    downtime as f32 / 60.0 * (2.0 / 3.0) * coefficient,
                );
            }
            crate::condition::record_mastery_training_morale(ctx, member_id, downtime, excess);
            ctx.db.character_skills().character_id().update(skills);
            crate::condition::apply_settlement_leisure_condition(
                ctx,
                member_id,
                core_schedule(&allowed),
                downtime,
                interval_end_minute,
            )?;
            automatic_chat_downtime.push((member_id, downtime));
        }
        crate::capability::refresh_character_capability(ctx, member_id)?;
    }
    // Resolve chats after every member's clock has reached the end of the
    // shared interval so target-clock cooldowns receive the full cadence.
    automatic_chat_downtime.sort_by_key(|(member_id, _)| *member_id);
    for (member_id, downtime) in automatic_chat_downtime {
        crate::social::apply_automatic_social_chats(ctx, member_id, downtime)?;
    }
    let living_after = crate::strategic::living_party_member_ids(ctx, &party_id);
    if living_after.is_empty() {
        crate::strategic::teardown_all_dead_strategic_party(ctx, &party_id)?;
        return Ok(());
    }
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
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if !character.alive {
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
    let saved_schedule = ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character training schedule not found".to_string())?;
    let effective_schedule =
        effective_organization_schedule(ctx, character_id, &saved_schedule.downtime);
    let injury_limit =
        crate::surgery::preview_elapsed_for_injuries(ctx, character_id, requested_elapsed, true)?;
    let (elapsed, terminal) =
        crate::disease::clip_elapsed_for_disease(ctx, character_id, injury_limit, true)?;
    let convalescing = convalescence_minutes(
        ctx,
        character_id,
        party_physiology_check(ctx, character_id)?,
    )
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
        crate::organization::settle_membership_dues(ctx, character_id)?;
        return Ok(forced_catch_up);
    }
    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character skill record not found".to_string())?;
    let activities = activity_training_profile(ctx, character_id)?;
    let training_elapsed = elapsed.saturating_sub(convalescing);
    let excess = apply_training(
        ctx,
        character_id,
        &mut skills,
        &effective_schedule,
        training_elapsed,
        activities,
    );
    crate::condition::record_mastery_training_morale(ctx, character_id, training_elapsed, excess);
    ctx.db.character_skills().character_id().update(skills);
    let risks = apply_activity_outcomes(
        ctx,
        character_id,
        &effective_schedule,
        training_elapsed,
        target_minutes,
    )?;
    crate::strategic::maybe_trigger_activity_incident(ctx, character_id, risks)?;
    let at_settlement = character.current_settlement_id.is_some();
    if at_settlement && training_elapsed > 0 {
        crate::social::apply_automatic_social_chats(ctx, character_id, training_elapsed)?;
    }
    if at_settlement {
        crate::condition::replenish_needs_at_settlement(ctx, character_id)?;
        crate::capability::refresh_character_capability(ctx, character_id)?;
    }
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    crate::organization::settle_membership_dues(ctx, character_id)?;
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
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
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
    validate_organization_schedule(ctx, character_id, &schedule.downtime)?;
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
    fn inn_stay_cost_only_rounds_up_partial_days() {
        assert_eq!(inn_stay_cost(0), Ok(0));
        assert_eq!(inn_stay_cost(MINUTES_PER_DAY), Ok(2));
        assert_eq!(inn_stay_cost(2 * MINUTES_PER_DAY), Ok(4));
        assert_eq!(inn_stay_cost(1), Ok(2));
        assert_eq!(inn_stay_cost(MINUTES_PER_DAY + 1), Ok(4));
    }

    #[test]
    fn sponsored_inn_rest_is_one_day_exact_cost_and_never_transfers_coin() {
        let source = include_str!("time.rs");
        let sponsored = source
            .split("pub fn sponsor_party_member_inn_rest")
            .nth(1)
            .and_then(|tail| tail.split("fn require_settlement_rest_service").next())
            .expect("sponsored rest reducer");
        for gate in [
            "require_strategic_character_authority(ctx, payer_id)",
            "payer_id == patient_id",
            "same party",
            "current party membership",
            "named settlement",
            "require_character_rest_service(ctx, patient_id, true)",
            "patient_publicly_needs_rest(ctx, patient_id)",
            "expected_cost != authoritative_cost",
            "Patient can afford ordinary inn rest",
            "payer_id,",
        ] {
            assert!(sponsored.contains(gate), "missing sponsorship gate {gate}");
        }
        assert!(sponsored.contains("MINUTES_PER_DAY"));
        assert!(sponsored.contains("sponsorship_gap"));
        assert!(sponsored.contains("Some(payer_id)"));
        assert!(!sponsored.contains("credit_personal_currency"));
        assert!(!sponsored.contains("party_inventory_item().insert"));

        let rest = source
            .split("fn rest_for_minutes")
            .nth(1)
            .and_then(|tail| tail.split("fn inn_stay_cost").next())
            .expect("settlement rest payment boundary");
        assert!(rest.contains("patient_contribution"));
        assert!(rest.contains("sponsor_contribution"));
        assert!(rest.contains("consume_personal_currency(ctx, character_id"));
        assert!(rest.contains("consume_personal_currency(ctx, sponsor_id"));
    }

    #[test]
    fn settlement_rest_consumes_elapsed_needs_once_in_terminal_safe_order() {
        let source = include_str!("time.rs");
        let rest = source
            .split("fn rest_for_minutes")
            .nth(1)
            .and_then(|tail| tail.split("fn validate_settlement_rest_minutes").next())
            .expect("settlement rest implementation");
        assert_eq!(rest.matches("inn_stay_cost(requested_minutes)?").count(), 1);
        assert_eq!(rest.matches("inn_stay_cost(elapsed)?").count(), 1);
        assert!(
            rest.find("personal_currency_total").unwrap()
                < rest
                    .find("preview_elapsed_for_injuries_with_rest_minutes")
                    .unwrap()
        );
        assert!(
            rest.find("inn_stay_cost(elapsed)?").unwrap()
                < rest
                    .find("crate::condition::apply_settlement_rest_elapsed_needs(")
                    .unwrap()
        );
        let needs = "crate::condition::apply_settlement_rest_elapsed_needs(";
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
    fn automatic_social_chats_run_only_after_positive_discretionary_downtime() {
        let source = include_str!("time.rs");
        let rest = source
            .split("fn rest_for_minutes")
            .nth(1)
            .and_then(|tail| tail.split("fn inn_stay_cost").next())
            .expect("settlement rest implementation");
        assert!(rest.contains("if training_elapsed > 0"));
        assert!(rest.contains("apply_automatic_social_chats(ctx, character_id,"));

        let camp = source
            .split("fn advance_personal_camp_time")
            .nth(1)
            .and_then(|tail| {
                tail.split("rest_temporary_party_member_until_healed_at_settlement")
                    .next()
            })
            .expect("camp downtime implementation");
        assert!(camp.contains("if downtime > 0"));
        assert!(camp.contains("apply_automatic_social_chats(ctx, member_id,"));

        for (start, end) in [
            (
                "pub fn advance_travel_time",
                "pub fn advance_character_wait_time",
            ),
            ("pub fn advance_character_wait_time", "fn default_schedule"),
        ] {
            let ordinary = source
                .split(start)
                .nth(1)
                .and_then(|tail| tail.split(end).next())
                .expect("non-downtime interval");
            assert!(!ordinary.contains("apply_automatic_social_chats"));
        }
    }

    #[test]
    fn departure_synchronization_defers_social_until_all_clocks_advance() {
        let source = include_str!("time.rs");
        let synchronization = source
            .split("pub(crate) fn synchronize_party_departure_time")
            .nth(1)
            .and_then(|tail| tail.split("pub(crate) fn allowed_camp_schedule").next())
            .expect("party departure synchronization");
        let loop_end = synchronization
            .find("automatic_chat_downtime.sort_by_key")
            .expect("deferred stable automatic pass");
        assert!(
            synchronization[..loop_end]
                .contains("spend_private_settlement_downtime_deferred_social")
        );
        assert!(synchronization[..loop_end].contains("advance_personal_camp_time"));
        assert!(
            synchronization[loop_end..]
                .contains("apply_automatic_social_chats(ctx, member_id, downtime)?")
        );
        assert!(!synchronization.contains("spend_private_settlement_downtime(ctx, member_id"));
    }

    #[test]
    fn immediate_activity_schedule_contains_only_the_selected_interval() {
        let schedule = immediate_activity_schedule(
            ImmediateActivity::ProfessionPractice,
            180,
            Some("weaponsmith_guild"),
        );
        assert_eq!(schedule.profession_practice_minutes, 180);
        assert_eq!(
            schedule.practice_organization_id.as_deref(),
            Some("weaponsmith_guild")
        );
        assert_eq!(schedule.allocated_minutes(), 180);
        let prayer = immediate_activity_schedule(ImmediateActivity::Prayer, 60, None);
        assert_eq!(prayer.prayer_minutes, 60);
        assert_eq!(prayer.allocated_minutes(), 60);
    }

    #[test]
    fn camp_schedule_excludes_organization_training_and_activity() {
        let schedule = ScheduleAllocation {
            apprenticeship_minutes: 120,
            apprenticeship_organization_id: Some("lodge_hart_king".into()),
            profession_practice_minutes: 180,
            practice_organization_id: Some("lodge_hart_king".into()),
            prayer_minutes: 60,
            ..Default::default()
        };
        let allowed = allowed_camp_schedule(&schedule);
        assert_eq!(allowed.apprenticeship_minutes, 0);
        assert!(allowed.apprenticeship_organization_id.is_none());
        assert_eq!(allowed.profession_practice_minutes, 0);
        assert!(allowed.practice_organization_id.is_none());
        assert_eq!(allowed.prayer_minutes, 60);
    }

    #[test]
    fn stale_saved_organization_allocations_are_converted_to_leisure() {
        let source = include_str!("time.rs");
        let effective = source
            .split("fn effective_organization_schedule")
            .nth(1)
            .and_then(|tail| tail.split("fn activity_training_profile").next())
            .expect("effective organization schedule");
        assert!(effective.contains("effective.apprenticeship_minutes = 0"));
        assert!(effective.contains("effective.profession_practice_minutes = 0"));
        assert!(effective.contains("rank.practice_allowed"));
        assert!(!effective.contains("return Err"));
    }

    #[test]
    fn organization_interval_samples_eligibility_before_advancing_and_settles_after_outcomes() {
        let source = include_str!("time.rs");
        for (start, end) in [
            ("fn rest_for_minutes", "fn inn_stay_cost"),
            (
                "pub fn synchronize_character(",
                "/// Explicitly synchronize",
            ),
        ] {
            let interval = source
                .split(start)
                .nth(1)
                .and_then(|tail| tail.split(end).next())
                .expect("organization time interval");
            let sample = interval
                .find("effective_organization_schedule")
                .expect("start eligibility sample");
            let advance = interval
                .find(".update(character_time)")
                .expect("clock advance");
            let outcomes = interval
                .rfind("apply_activity_outcomes")
                .expect("activity outcomes");
            let settle = interval
                .rfind("settle_membership_dues")
                .expect("post-interval dues settlement");
            assert!(sample < advance);
            assert!(outcomes < settle);
        }
        let immediate = source
            .split("pub fn perform_immediate_activity")
            .nth(1)
            .and_then(|tail| tail.split("const ACTIVITY_MINUTE_SCALE").next())
            .expect("immediate organization interval");
        assert!(
            immediate.find("validate_organization_schedule").unwrap()
                < immediate.find(".update(character_time)").unwrap()
        );
        assert!(
            immediate.rfind("apply_activity_outcomes").unwrap()
                < immediate.rfind("settle_membership_dues").unwrap()
        );
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
            &adventuresim_core::stub::StubAttributes,
        );
        assert!((hours.sword - 0.046875).abs() < f32::EPSILON);
        assert!((hours.dodge - 0.046875).abs() < f32::EPSILON);
        assert!((hours.balance - 0.046875).abs() < f32::EPSILON);
        assert!((hours.will - 1.046875).abs() < f32::EPSILON);
        assert_eq!(allocation.allocated_minutes(), 630);
    }

    #[test]
    fn physiology_check_sets_the_daily_healing_rate() {
        assert!((health_recovered_per_day(0.0) - 0.01).abs() < f32::EPSILON);
        assert!((health_recovered_per_day(2.5) - 0.035).abs() < f32::EPSILON);
        assert!((health_recovered_per_day(5.0) - 0.06).abs() < f32::EPSILON);
        assert!((health_recovered_per_day(8.0) - 0.06).abs() < f32::EPSILON);
    }
}
