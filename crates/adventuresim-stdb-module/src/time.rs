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
use crate::strategic::party;
use crate::{
    CharacterAttributes, CharacterLimbs, CharacterSkills, CharacterStats, character_attributes,
    character_equip, character_limbs, character_skills, character_stats, settlement,
};
use adventuresim_world_schema::{OfficialReligion, ReligionMinutes};

/// Natural recovery without useful medical support while taking full
/// settlement downtime.
pub const BASE_HEALTH_RECOVERED_PER_DAY: f32 = 0.01;
/// Additional daily recovery supplied by each point of the party Medicine
/// check. Checks are capped at the five-point scale used by the strategic UI.
pub const HEALTH_RECOVERED_PER_MEDICINE_CHECK_PER_DAY: f32 = 0.01;
pub const INN_GOLD_PER_DAY: u32 = 1;
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
    pub minutes: u64,
}

/// One 24-hour daily budget. Leisure is always the unallocated remainder.
#[derive(Clone, Debug, Default, SpacetimeType)]
pub struct ScheduleAllocation {
    pub melee_minutes: u16,
    pub dodge_minutes: u16,
    pub block_minutes: u16,
    pub ranged_minutes: u16,
    pub will_minutes: u16,
    pub charisma_minutes: u16,
    pub medicine_minutes: u16,
    pub religion_minutes: u16,
    pub religion_auto_train: bool,
    pub religion_minutes_by_tradition: ReligionMinutes,
    pub stealth_minutes: u16,
    pub balance_minutes: u16,
    pub surgeon_minutes: u16,
    pub smithing_minutes: u16,
    /// Paid physical work; also trains Will at reduced speed.
    pub labor_minutes: u16,
    pub prayer_minutes: u16,
    pub thievery_minutes: u16,
    pub raiding_minutes: u16,
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
            self.melee_minutes,
            self.dodge_minutes,
            self.block_minutes,
            self.ranged_minutes,
            self.will_minutes,
            self.charisma_minutes,
            self.medicine_minutes,
            if self.religion_auto_train {
                self.religion_minutes
            } else {
                u16::try_from(self.religion_minutes_by_tradition.total()).unwrap_or(u16::MAX)
            },
            self.stealth_minutes,
            self.balance_minutes,
            self.surgeon_minutes,
            self.smithing_minutes,
            self.labor_minutes,
            self.prayer_minutes,
            self.thievery_minutes,
            self.raiding_minutes,
        ])
    }

    fn uses_quarter_hours(&self) -> bool {
        let mut values = vec![
            self.melee_minutes,
            self.dodge_minutes,
            self.block_minutes,
            self.ranged_minutes,
            self.will_minutes,
            self.charisma_minutes,
            self.medicine_minutes,
            self.religion_minutes,
            self.stealth_minutes,
            self.balance_minutes,
            self.surgeon_minutes,
            self.smithing_minutes,
            self.labor_minutes,
            self.prayer_minutes,
            self.thievery_minutes,
            self.raiding_minutes,
        ];
        values.extend(
            OfficialReligion::ALL
                .into_iter()
                .map(|r| self.religion_minutes_by_tradition.get(r)),
        );
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
    let (elapsed, terminal) = crate::disease::clip_elapsed_for_disease(ctx, character_id, minutes)?;
    character_time.minutes = character_time.minutes.saturating_add(elapsed);
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
    crate::disease::finish_disease_interval(ctx, character_id, terminal)?;
    if terminal.is_some() {
        return Ok(false);
    }
    crate::condition::apply_travel_condition(ctx, character_id, starting_minute, elapsed, 0)?;
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(true)
}

fn default_schedule(character_id: u64) -> CharacterTrainingSchedule {
    CharacterTrainingSchedule {
        character_id,
        downtime: ScheduleAllocation {
            religion_auto_train: true,
            ..Default::default()
        },
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

fn activity_training_profile(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<adventuresim_core::strategic_schedule::ActivityTrainingProfile, String> {
    let capability = crate::capability::evaluate_character(ctx, character_id)?;
    Ok(
        adventuresim_core::strategic_schedule::ActivityTrainingProfile {
            raiding_melee: capability.melee && !capability.ranged,
            raiding_ranged: capability.ranged,
            raiding_block: capability.half_armor
                || capability.three_quarter_armor
                || capability.full_armor,
            raiding_dodge: !capability.full_armor && !capability.three_quarter_armor,
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
        melee: skills.melee_hours,
        dodge: skills.dodge_hours,
        block: skills.block_hours,
        ranged: skills.ranged_hours,
        will: skills.will_hours,
        charisma: skills.charisma_hours,
        medicine: skills.medicine_hours,
        religion: skills.religion_hours,
        stealth: skills.stealth_hours,
        balance: skills.balance_hours,
        surgeon: skills.surgeon_hours,
        smithing: skills.smithing_hours,
    };
    apply_schedule_training(&mut hours, core_schedule(schedule), elapsed, activities);
    let (allocations, prayer_religion) = resolve_religion_training(ctx, character_id, schedule);
    apply_religion_training(
        &mut hours.religion,
        allocations,
        elapsed,
        prayer_religion,
        schedule.prayer_minutes,
    );
    skills.melee_hours = hours.melee;
    skills.dodge_hours = hours.dodge;
    skills.block_hours = hours.block;
    skills.ranged_hours = hours.ranged;
    skills.will_hours = hours.will;
    skills.charisma_hours = hours.charisma;
    skills.medicine_hours = hours.medicine;
    skills.religion_hours = hours.religion;
    skills.stealth_hours = hours.stealth;
    skills.balance_hours = hours.balance;
    skills.surgeon_hours = hours.surgeon;
    skills.smithing_hours = hours.smithing;
}

fn resolve_religion_training(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
) -> (ReligionMinutes, Option<OfficialReligion>) {
    let profession = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .and_then(|condition| condition.religion_id)
        .as_deref()
        .and_then(OfficialReligion::from_id);
    if !schedule.religion_auto_train {
        return (schedule.religion_minutes_by_tradition, profession);
    }

    let targets = if let Some(religion) = profession {
        vec![religion]
    } else {
        ctx.db
            .character()
            .id()
            .find(character_id)
            .and_then(|character| character.current_settlement_id)
            .and_then(|settlement_id| ctx.db.settlement().id().find(&settlement_id))
            .map(|settlement| settlement.religious_status.represented_religions())
            .unwrap_or_default()
    };
    (
        ReligionMinutes::split_evenly(schedule.religion_minutes, &targets),
        profession,
    )
}

pub(crate) fn core_schedule(schedule: &ScheduleAllocation) -> DailySchedule {
    DailySchedule {
        melee: schedule.melee_minutes,
        dodge: schedule.dodge_minutes,
        block: schedule.block_minutes,
        ranged: schedule.ranged_minutes,
        will: schedule.will_minutes,
        charisma: schedule.charisma_minutes,
        medicine: schedule.medicine_minutes,
        religion: schedule.religion_minutes,
        religion_auto_train: schedule.religion_auto_train,
        religions: schedule.religion_minutes_by_tradition,
        stealth: schedule.stealth_minutes,
        balance: schedule.balance_minutes,
        surgeon: schedule.surgeon_minutes,
        smithing: schedule.smithing_minutes,
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
        let mut character = character;
        character.gold = character.gold.saturating_add(outcome.gold_earned);
        ctx.db.character().id().update(character);
    }
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
    crate::condition::apply_settlement_leisure_condition(
        ctx,
        character_id,
        core_schedule(schedule),
        elapsed,
        interval_end_minute,
    )?;
    Ok(ActivityRisks {
        thievery_discovery: outcome.thievery_discovery_chance,
        raiding_retaliation: outcome.raiding_retaliation_chance,
    })
}

fn health_recovered_per_day(medicine_check: f32) -> f32 {
    BASE_HEALTH_RECOVERED_PER_DAY
        + medicine_check.clamp(0.0, 5.0) * HEALTH_RECOVERED_PER_MEDICINE_CHECK_PER_DAY
}

fn party_medicine_check(ctx: &ReducerContext, character_id: u64) -> Result<f32, String> {
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

fn heal_limbs(limbs: &mut CharacterLimbs, elapsed: u64, medicine_check: f32) {
    let recovery =
        elapsed as f32 / MINUTES_PER_DAY as f32 * health_recovered_per_day(medicine_check);
    for health in [
        &mut limbs.left_arm_health,
        &mut limbs.right_arm_health,
        &mut limbs.left_leg_health,
        &mut limbs.right_leg_health,
        &mut limbs.head_health,
        &mut limbs.chest_health,
        &mut limbs.stomach_health,
    ] {
        *health = (*health + recovery).min(1.0);
    }
}

fn convalescence_minutes(limbs: &CharacterLimbs, medicine_check: f32) -> u64 {
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
    if lowest_health >= 1.0 {
        0
    } else {
        ((1.0 - lowest_health) / health_recovered_per_day(medicine_check) * MINUTES_PER_DAY as f32)
            .ceil() as u64
    }
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
    rest_for_minutes(
        ctx,
        character_id,
        u64::from(requested_days) * MINUTES_PER_DAY,
        at_inn,
    )
}

/// Spend settlement time in hour-sized increments.  This intentionally keeps
/// each character's clock independent: sharing a settlement does not force a
/// party to keep identical strategic times.
#[reducer]
pub fn rest_at_settlement_hours(
    ctx: &ReducerContext,
    character_id: u64,
    requested_minutes: u64,
    at_inn: bool,
) -> Result<(), String> {
    rest_for_minutes(ctx, character_id, requested_minutes, at_inn)
}

fn rest_for_minutes(
    ctx: &ReducerContext,
    character_id: u64,
    requested_minutes: u64,
    at_inn: bool,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
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

    let cost =
        (requested_minutes.div_ceil(MINUTES_PER_DAY) as u32).saturating_mul(INN_GOLD_PER_DAY);
    if at_inn {
        let mut character = ctx
            .db
            .character()
            .id()
            .find(character_id)
            .ok_or_else(|| "Character not found".to_string())?;
        if character.gold < cost {
            return Err("Not enough gold to pay for the inn stay".into());
        }
        character.gold -= cost;
        ctx.db.character().id().update(character);
    }

    let (elapsed, terminal) =
        crate::disease::clip_elapsed_for_disease(ctx, character_id, requested_minutes)?;
    let mut limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character limb record not found".to_string())?;
    let medicine_check = party_medicine_check(ctx, character_id)?;
    let convalescing = convalescence_minutes(&limbs, medicine_check).min(elapsed);
    heal_limbs(&mut limbs, elapsed, medicine_check);
    ctx.db.character_limbs().character_id().update(limbs);

    let smithing_skill = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .map(|skills| Skill::Smithing.training_rank(skills.smithing_hours).floor() as u8)
        .unwrap_or(0);
    let maintenance_elapsed = crate::repair::field_repair(
        ctx,
        character_id,
        smithing_skill,
        elapsed.saturating_sub(convalescing),
    );
    let training_elapsed = elapsed
        .saturating_sub(convalescing)
        .saturating_sub(maintenance_elapsed);
    let priority_rest_elapsed = elapsed.saturating_sub(training_elapsed);
    let starting_minute = character_time.minutes;
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

    character_time.minutes += elapsed;
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
    crate::disease::finish_disease_interval(ctx, character_id, terminal)?;
    if terminal.is_some() {
        return Ok(());
    }
    crate::condition::apply_rest_condition(ctx, character_id, elapsed)?;
    crate::capability::refresh_character_capability(ctx, character_id)?;
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
        .party()
        .id()
        .find(party_id)
        .is_some_and(|party| party.leader_id == character_id)
    {
        return Ok(());
    }

    let limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(character_id)
        .ok_or("Character limb record not found")?;
    let recovery_minutes = convalescence_minutes(&limbs, party_medicine_check(ctx, character_id)?);
    if recovery_minutes > 0 {
        rest_for_minutes(ctx, character_id, recovery_minutes, false)?;
    }
    Ok(())
}

/// Camp rest is a party action: the leader chooses a duration and every party
/// member spends the same strategic time. It restores fatigue and natural
/// recovery without settlement provisions, prices, or downtime training.
#[reducer]
pub fn rest_at_camp(
    ctx: &ReducerContext,
    character_id: u64,
    requested_minutes: u64,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    if requested_minutes == 0 {
        return Ok(());
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
        .party()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can rest the party at camp".into());
    }
    if party.camp_destination_id.is_none() {
        return Err("The party is not camped while travelling".into());
    }
    let members = crate::strategic::living_party_member_ids(ctx, &party_id);
    let elapsed = members
        .iter()
        .try_fold(requested_minutes, |limit, member_id| {
            crate::disease::preview_elapsed_for_disease(ctx, *member_id, limit)
                .map(|safe| limit.min(safe))
        })?;
    for member_id in members {
        ensure_character_time(ctx, member_id)?;
        let mut time = ctx
            .db
            .character_time()
            .character_id()
            .find(member_id)
            .ok_or("Character time record not found")?;
        let (_, terminal) = crate::disease::clip_elapsed_for_disease(ctx, member_id, elapsed)?;
        time.minutes = time.minutes.saturating_add(elapsed);
        ctx.db.character_time().character_id().update(time);
        crate::disease::finish_disease_interval(ctx, member_id, terminal)?;
        if terminal.is_some() {
            continue;
        }
        crate::condition::apply_camp_rest_condition(ctx, member_id, elapsed)?;
        let mut limbs = ctx
            .db
            .character_limbs()
            .character_id()
            .find(member_id)
            .ok_or("Character limb record not found")?;
        let medicine_check = party_medicine_check(ctx, member_id)?;
        let convalescing = convalescence_minutes(&limbs, medicine_check).min(elapsed);
        heal_limbs(&mut limbs, elapsed, medicine_check);
        ctx.db.character_limbs().character_id().update(limbs);
        let smithing_skill = ctx
            .db
            .character_skills()
            .character_id()
            .find(member_id)
            .map(|skills| Skill::Smithing.training_rank(skills.smithing_hours).floor() as u8)
            .unwrap_or(0);
        crate::repair::field_repair(
            ctx,
            member_id,
            smithing_skill,
            adventuresim_core::durability::remaining_after_priority(elapsed, convalescing),
        );
        crate::capability::refresh_character_capability(ctx, member_id)?;
    }
    // Reforecast the untravelled part from the fatigue that this particular
    // rest actually removed. The journey record retains all reached camps.
    crate::strategic::refresh_party_journey_forecast(ctx, &party_id)?;
    Ok(())
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
    let (elapsed, terminal) =
        crate::disease::clip_elapsed_for_disease(ctx, character_id, requested_elapsed)?;
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
        elapsed,
        activities,
    );
    ctx.db.character_skills().character_id().update(skills);
    let risks = apply_activity_outcomes(
        ctx,
        character_id,
        &schedule.downtime,
        elapsed,
        target_minutes,
    )?;
    crate::strategic::maybe_trigger_activity_incident(ctx, character_id, risks)?;
    character_time.minutes = character_time.minutes.saturating_add(elapsed);
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
    crate::disease::finish_disease_interval(ctx, character_id, terminal)?;
    if terminal.is_some() {
        return Ok(forced_catch_up);
    }
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
    fn training_uses_the_daily_minute_allocation() {
        let mut skills = CharacterSkills {
            character_id: 1,
            melee_hours: 0.0,
            dodge_hours: 0.0,
            block_hours: 0.0,
            ranged_hours: 0.0,
            will_hours: 0.0,
            charisma_hours: 0.0,
            medicine_hours: 0.0,
            religion_hours: adventuresim_world_schema::ReligionHours::default(),
            stealth_hours: 0.0,
            balance_hours: 0.0,
            surgeon_hours: 0.0,
            smithing_hours: 0.0,
        };
        let allocation = ScheduleAllocation {
            melee_minutes: 90,
            dodge_minutes: 30,
            block_minutes: 0,
            ranged_minutes: 0,
            will_minutes: 0,
            charisma_minutes: 0,
            medicine_minutes: 0,
            religion_minutes: 0,
            religion_auto_train: false,
            religion_minutes_by_tradition: ReligionMinutes::default(),
            stealth_minutes: 0,
            balance_minutes: 0,
            surgeon_minutes: 0,
            smithing_minutes: 0,
            labor_minutes: 480,
            prayer_minutes: 60,
            thievery_minutes: 0,
            raiding_minutes: 0,
        };
        let mut hours = SkillHours {
            melee: skills.melee_hours,
            dodge: skills.dodge_hours,
            ..Default::default()
        };
        apply_schedule_training(
            &mut hours,
            core_schedule(&allocation),
            MINUTES_PER_DAY * 2,
            ActivityTrainingProfile::default(),
        );
        skills.melee_hours = hours.melee;
        skills.dodge_hours = hours.dodge;
        skills.will_hours = hours.will;
        assert_eq!(skills.melee_hours, 3.0);
        assert_eq!(skills.dodge_hours, 1.0);
        assert_eq!(skills.will_hours, 4.0);
        assert_eq!(allocation.allocated_minutes(), 660);
    }

    #[test]
    fn convalescence_blocks_training_until_the_slowest_limb_recovers() {
        let limbs = CharacterLimbs {
            character_id: 1,
            left_arm_health: 0.9,
            right_arm_health: 1.0,
            left_leg_health: 1.0,
            right_leg_health: 1.0,
            head_health: 1.0,
            chest_health: 1.0,
            stomach_health: 1.0,
        };
        assert_eq!(convalescence_minutes(&limbs, 4.0), MINUTES_PER_DAY * 2);
    }

    #[test]
    fn healing_is_capped_at_full_health() {
        let mut limbs = CharacterLimbs {
            character_id: 1,
            left_arm_health: 0.98,
            right_arm_health: 1.0,
            left_leg_health: 1.0,
            right_leg_health: 1.0,
            head_health: 1.0,
            chest_health: 1.0,
            stomach_health: 1.0,
        };
        heal_limbs(&mut limbs, MINUTES_PER_DAY, 4.0);
        assert_eq!(limbs.left_arm_health, 1.0);
    }

    #[test]
    fn medicine_check_sets_the_daily_healing_rate() {
        assert!((health_recovered_per_day(0.0) - 0.01).abs() < f32::EPSILON);
        assert!((health_recovered_per_day(2.5) - 0.035).abs() < f32::EPSILON);
        assert!((health_recovered_per_day(5.0) - 0.06).abs() < f32::EPSILON);
        assert!((health_recovered_per_day(8.0) - 0.06).abs() < f32::EPSILON);
    }
}
