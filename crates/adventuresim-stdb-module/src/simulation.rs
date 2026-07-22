//! Enforceable isolation for reducer-backed balance simulations.

use spacetimedb::{Identity, ReducerContext, Table, reducer, table};

use crate::character::character;
use crate::personality::character_personality;
use crate::time::character_time;
use crate::{
    CharacterAttributes, CharacterSkills, CharacterTrainingSchedule, DeathCause, DeathSource,
    ScheduleAllocation, character_attributes, character_skills, character_training_schedule,
    infection_episode, party, settlement,
};

/// Ordinary module builds deliberately contain no simulation capability. The
/// disposable launcher supplies this only to the one module build it owns.
const COMPILED_BOOTSTRAP_TOKEN: Option<&str> = option_env!("ADVENTURESIM_SIM_BOOTSTRAP_TOKEN");
const MAX_INITIAL_SKILL_HOURS: f32 = 1_000_000.0;

fn simulation_religion_hours_valid(hours: adventuresim_world_schema::ReligionHours) -> bool {
    hours.direct_fields_valid(MAX_INITIAL_SKILL_HOURS)
}

#[derive(Clone, Debug)]
#[table(accessor = simulation_run, public)]
pub struct SimulationRun {
    #[primary_key]
    pub id: u64,
    #[unique]
    pub nonce: String,
    pub owner: Identity,
    pub policy_seed: u64,
    pub claimed_micros: i64,
}

#[derive(Clone, Debug)]
#[table(accessor = simulation_character, public)]
pub struct SimulationCharacter {
    #[primary_key]
    pub character_id: u64,
    pub run_id: u64,
    pub agent_id: u32,
}

fn valid_nonce(nonce: &str) -> bool {
    (16..=96).contains(&nonce.len())
        && nonce
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// Atomically claim a newly published, empty module database for one SDK
/// identity. A populated game database cannot be converted into a simulator.
#[reducer]
pub fn claim_simulation_run(
    ctx: &ReducerContext,
    bootstrap_token: String,
    nonce: String,
    policy_seed: u64,
) -> Result<(), String> {
    // This check must remain first: a normal production build cannot reveal
    // or depend on database freshness through this public reducer.
    if !adventuresim_core::simulation_security::simulation_bootstrap_authorized(
        COMPILED_BOOTSTRAP_TOKEN,
        &bootstrap_token,
    ) {
        return Err("Simulation bootstrap is disabled or unauthorized".into());
    }
    if !valid_nonce(&nonce) {
        return Err("Simulation nonce must be 16..=96 ASCII alphanumeric/dash characters".into());
    }
    if ctx.db.simulation_run().iter().next().is_some() {
        return Err("Simulation database has already been claimed".into());
    }
    if ctx.db.character().iter().next().is_some()
        || ctx.db.party().iter().next().is_some()
        || ctx.db.settlement().iter().next().is_some()
    {
        return Err("Simulation claim requires a freshly published empty database".into());
    }
    ctx.db.simulation_run().insert(SimulationRun {
        id: 0,
        nonce,
        owner: ctx.sender(),
        policy_seed,
        claimed_micros: ctx.timestamp.to_micros_since_unix_epoch(),
    });
    Ok(())
}

pub(crate) fn owned_run(ctx: &ReducerContext, nonce: &str) -> Result<SimulationRun, String> {
    let run = ctx
        .db
        .simulation_run()
        .id()
        .find(0)
        .ok_or("Simulation database has not been claimed")?;
    if run.owner != ctx.sender() || run.nonce != nonce {
        return Err("Simulation run is owned by a different identity or nonce".into());
    }
    Ok(run)
}

pub(crate) fn sender_owns_simulation_character(ctx: &ReducerContext, character_id: u64) -> bool {
    ctx.db
        .simulation_run()
        .id()
        .find(0)
        .is_some_and(|run| run.owner == ctx.sender())
        && ctx
            .db
            .simulation_character()
            .character_id()
            .find(character_id)
            .is_some()
}

#[reducer]
pub fn seed_simulation_world(ctx: &ReducerContext, nonce: String) -> Result<(), String> {
    owned_run(ctx, &nonce)?;
    crate::strategic::seed_world(ctx)
}

/// Deterministic death path available only in a capability-owned disposable
/// simulation database. Production databases cannot claim that capability.
#[reducer]
pub fn kill_simulation_character(
    ctx: &ReducerContext,
    nonce: String,
    character_id: u64,
) -> Result<(), String> {
    owned_run(ctx, &nonce)?;
    ctx.db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found in this disposable simulation database")?;
    crate::character::transition_character_to_dead(
        ctx,
        character_id,
        DeathCause::DevTest,
        DeathSource::DevTest,
        Some(nonce),
    )?;
    Ok(())
}

/// Configure a fresh character only inside the claimed isolated run. Combat
/// entropy is intentionally absent; autoresolve continues to use server RNG.
#[reducer]
pub fn configure_simulation_character(
    ctx: &ReducerContext,
    nonce: String,
    character_id: u64,
    agent_id: u32,
    settlement_id: String,
    attributes: CharacterAttributes,
    skills: CharacterSkills,
    downtime: ScheduleAllocation,
    personality: crate::personality::CharacterPersonality,
) -> Result<(), String> {
    let run = owned_run(ctx, &nonce)?;
    let mut character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Simulation character not found")?;
    if character.temporary
        || ctx
            .db
            .simulation_character()
            .character_id()
            .find(character_id)
            .is_some()
    {
        return Err("Only a fresh ordinary solo character may be configured once".into());
    }
    if attributes.character_id != character_id
        || skills.character_id != character_id
        || personality.character_id != character_id
    {
        return Err("Simulation profile character IDs must match".into());
    }
    let attributes_valid = [
        attributes.endurance,
        attributes.immunity,
        attributes.gut,
        attributes.precision,
        attributes.intelligence,
        attributes.instinct,
        attributes.eyesight,
        attributes.hearing,
        attributes.left_arm_strength,
        attributes.right_arm_strength,
        attributes.left_leg_strength,
        attributes.right_leg_strength,
        attributes.left_arm_agility,
        attributes.right_arm_agility,
        attributes.left_leg_agility,
        attributes.right_leg_agility,
    ]
    .into_iter()
    .all(|value| value.is_finite() && (0.5..=5.0).contains(&value));
    let skills_valid = [
        skills.melee_hours,
        skills.dodge_hours,
        skills.block_hours,
        skills.ranged_hours,
        skills.will_hours,
        skills.insight_hours,
        skills.self_awareness_hours,
        skills.humor_hours,
        skills.command_hours,
        skills.deception_hours,
        skills.seduction_hours,
        skills.medicine_hours,
        skills.stealth_hours,
        skills.balance_hours,
        skills.surgeon_hours,
    ]
    .into_iter()
    .all(|value| value.is_finite() && (0.0..=MAX_INITIAL_SKILL_HOURS).contains(&value))
        && simulation_religion_hours_valid(skills.religion_hours);
    if !attributes_valid || !skills_valid || downtime.allocated_minutes() > 1_440 {
        return Err("Simulation profile is outside bounded gameplay ranges".into());
    }
    if ctx.db.settlement().id().find(&settlement_id).is_none() {
        return Err("Simulation settlement not found".into());
    }
    let party_id = character
        .party_id
        .clone()
        .ok_or("Simulation character has no party")?;
    let mut solo_party = ctx
        .db
        .party()
        .id()
        .find(&party_id)
        .ok_or("Simulation party not found")?;
    if solo_party.leader_id != character_id || !solo_party.is_solo {
        return Err("Simulation character must still lead its fresh solo party".into());
    }
    character.current_settlement_id = Some(settlement_id.clone());
    character.current_quest_location_id = None;
    ctx.db.character().id().update(character);
    solo_party.current_settlement_id = Some(settlement_id);
    solo_party.current_quest_location_id = None;
    ctx.db.party().id().update(solo_party);
    ctx.db
        .character_attributes()
        .character_id()
        .update(attributes);
    ctx.db.character_skills().character_id().update(skills);
    let mut schedule: CharacterTrainingSchedule = ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
        .ok_or("Simulation schedule not found")?;
    schedule.downtime = downtime;
    ctx.db
        .character_training_schedule()
        .character_id()
        .update(schedule);
    ctx.db.simulation_character().insert(SimulationCharacter {
        character_id,
        run_id: run.id,
        agent_id,
    });
    ctx.db
        .character_personality()
        .character_id()
        .update(personality);
    crate::capability::refresh_character_capability(ctx, character_id)?;
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    Ok(())
}

/// Deterministic illness fixture for the claimed disposable simulator. It
/// supplies only an infection seed; diagnosis and treatment remain ordinary
/// player-facing actions and the simulator never subscribes to this private row.
#[reducer]
pub fn seed_simulation_disease(
    ctx: &ReducerContext,
    nonce: String,
    character_id: u64,
) -> Result<(), String> {
    let run = owned_run(ctx, &nonce)?;
    let sim = ctx
        .db
        .simulation_character()
        .character_id()
        .find(character_id)
        .ok_or("Only configured simulation characters may use this fixture")?;
    if sim.run_id != run.id {
        return Err("Simulation character belongs to another run".into());
    }
    crate::require_living_character(ctx, character_id)?;
    if ctx
        .db
        .infection_episode()
        .character_id()
        .filter(character_id)
        .next()
        .is_some()
    {
        return Err("Simulation disease fixture may only be seeded once".into());
    }
    ctx.db
        .infection_episode()
        .insert(crate::disease::InfectionEpisodeRow {
            id: 0,
            character_id,
            disease_id: "influenza".into(),
            contracted_at: 0,
            treated_at: None,
        });
    let requested =
        adventuresim_core::disease::definition(adventuresim_core::disease::DiseaseId::Influenza)
            .incubation_minutes
            .saturating_add(60);
    // Advance through the same disease interval hooks as ordinary gameplay so
    // the simulator observes symptom onset instead of receiving hidden fixture
    // knowledge from the private infection row.
    let injury_limit =
        crate::surgery::preview_elapsed_for_injuries(ctx, character_id, requested, false)?;
    let (elapsed, terminal) =
        crate::disease::clip_elapsed_for_disease(ctx, character_id, injury_limit, false)?;
    let mut time = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Simulation character time not found")?;
    let settled = crate::surgery::settle_injuries(ctx, character_id, elapsed, false)?;
    time.minutes = time.minutes.saturating_add(settled.elapsed);
    ctx.db.character_time().character_id().update(time);
    crate::disease::finish_disease_interval(ctx, character_id, terminal)?;
    if terminal.is_some() || !settled.alive {
        return Ok(());
    }
    crate::capability::refresh_character_capability(ctx, character_id)?;
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    Ok(())
}

pub(crate) fn same_simulation_scope(ctx: &ReducerContext, left: u64, right: u64) -> bool {
    let left = ctx.db.simulation_character().character_id().find(left);
    let right = ctx.db.simulation_character().character_id().find(right);
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left.run_id == right.run_id,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::simulation_religion_hours_valid;
    use adventuresim_world_schema::ReligionHours;

    #[test]
    fn simulation_rejects_invalid_individual_religion_fields() {
        assert!(simulation_religion_hours_valid(ReligionHours::default()));
        for invalid in [-1.0, f32::NAN, f32::INFINITY] {
            assert!(!simulation_religion_hours_valid(ReligionHours {
                lutheran: invalid,
                ..Default::default()
            }));
        }
    }
}
