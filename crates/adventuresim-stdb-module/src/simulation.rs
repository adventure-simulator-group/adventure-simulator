//! Enforceable isolation for reducer-backed balance simulations.

use spacetimedb::{
    Identity, ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view,
};

use adventuresim_core::simulation_security::MAX_SIMULATION_SKILL_HOURS;
use adventuresim_core::strategic_time::{
    MINUTES_PER_DAY, MINUTES_PER_YEAR, real_micros_for_official_minutes,
};

use crate::character::character;
use crate::investigation::investigation_witness_referral__view;
use crate::local_problem::local_problem_receipt__view;
use crate::strategic::{case_authority__view, quest_generation_authority__view};
use crate::time::{character_time, world_clock};
use crate::{
    CharacterAttributes, CharacterSkills, CharacterTrainingSchedule, DeathCause, DeathSource,
    ScheduleAllocation, character_attributes, character_skills, character_training_schedule,
    infection_episode, party_authority, settlement, world_data_import,
};

/// Ordinary module builds deliberately contain no simulation capability. The
/// disposable launcher supplies this only to the one module build it owns.
const COMPILED_BOOTSTRAP_TOKEN: Option<&str> = option_env!("ADVENTURESIM_SIM_BOOTSTRAP_TOKEN");
const MAX_SIMULATION_CLOCK_ADVANCE_MINUTES: u64 = 100 * MINUTES_PER_YEAR;
const SIMULATION_STARTING_COIN: u32 = 100;

fn valid_simulation_clock_advance(delta_minutes: u64) -> bool {
    (1..=MAX_SIMULATION_CLOCK_ADVANCE_MINUTES).contains(&delta_minutes)
}

fn simulation_epoch_shift_micros(delta_minutes: u64) -> Option<i64> {
    i64::try_from(real_micros_for_official_minutes(delta_minutes)).ok()
}

fn simulation_religion_hours_valid(hours: adventuresim_world_schema::ReligionHours) -> bool {
    hours.direct_fields_valid(MAX_SIMULATION_SKILL_HOURS)
}

fn simulation_bestiary_hours_valid(hours: adventuresim_world_schema::BestiaryHours) -> bool {
    hours.direct_fields_valid(MAX_SIMULATION_SKILL_HOURS)
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

/// Private fixture authority. The generated identifier is canonical quest
/// authority and must not be published before ordinary rumor intake creates
/// the owner's public journal case.
#[derive(Clone, Debug)]
#[table(accessor = simulation_quest_fixture_authority)]
pub struct SimulationQuestFixtureAuthority {
    #[primary_key]
    pub id: u64,
    pub run_id: u64,
    pub direct_contract_id: String,
    pub generated_canonical_case_id: String,
    pub direct_leader_id: u64,
    pub generated_leader_id: u64,
    pub direct_party_id: String,
    pub generated_party_id: String,
}

/// Gateway-only provenance for the deterministic quest acceptance fixture.
/// The generated case ID is projected only from a validated initial-rumor
/// referral, so it is the exact owner-visible journal ID observed by agents.
#[derive(Clone, Debug, SpacetimeType)]
pub struct SimulationQuestFixture {
    pub id: u64,
    pub run_id: u64,
    pub direct_contract_id: String,
    pub generated_case_id: String,
    pub direct_leader_id: u64,
    pub generated_leader_id: u64,
    pub direct_party_id: String,
    pub generated_party_id: String,
}

#[view(accessor = simulation_quest_fixture, public)]
pub fn simulation_quest_fixture(ctx: &ViewContext) -> Vec<SimulationQuestFixture> {
    let Some(run) = ctx
        .db
        .simulation_run()
        .id()
        .find(0)
        .filter(|run| run.owner == ctx.sender())
    else {
        return Vec::new();
    };
    let Some(fixture) = ctx.db.simulation_quest_fixture_authority().id().find(0) else {
        return Vec::new();
    };
    if fixture.run_id != run.id {
        return Vec::new();
    }
    let Some(authority) = ctx
        .db
        .quest_generation_authority()
        .case_id()
        .find(&fixture.generated_canonical_case_id)
    else {
        return Vec::new();
    };
    let Some(case) = ctx
        .db
        .case_authority()
        .id()
        .find(&fixture.generated_canonical_case_id)
    else {
        return Vec::new();
    };
    let Some(manifest) =
        crate::strategic::validated_generated_dialogue_manifest(&case, Some(&authority))
            .ok()
            .flatten()
    else {
        return Vec::new();
    };
    let Some(witness) = manifest.witnesses.first() else {
        return Vec::new();
    };
    let referral = ctx
        .db
        .investigation_witness_referral()
        .owner_character_id()
        .filter(fixture.generated_leader_id)
        .find(|referral| {
            referral.canonical_case_id == fixture.generated_canonical_case_id
                && referral.public_case_id == manifest.public_case_id
                && referral.catalog_revision == authority.catalog_revision
                && referral.grant_kind == "initial_rumor"
                && !referral.source_receipt_id.is_empty()
                && referral.witness_resident_character_id == witness.resident_character_id
                && referral.expected_settlement_id == authority.settlement_id
                && referral.expected_location_id == witness.expected_location
                && referral.source_witness_id.is_empty()
                && referral.source_testimony_index == 0
                && referral.source_proposition_id.is_empty()
        });
    let Some(referral) = referral else {
        return Vec::new();
    };
    if !ctx
        .db
        .local_problem_receipt()
        .id()
        .find(&referral.source_receipt_id)
        .is_some_and(|receipt| {
            receipt.character_id == fixture.generated_leader_id
                && receipt.opaque_case_ref == fixture.generated_canonical_case_id
                && receipt.problem_id == manifest.problem_id
                && receipt.settlement_id == authority.settlement_id
                && receipt.contact_resident_character_id == witness.resident_character_id
                && receipt.expected_location_id == witness.expected_location
                && referral.source_witness_resident_character_id
                    == receipt.source_resident_character_id
        })
    {
        return Vec::new();
    }
    vec![SimulationQuestFixture {
        id: fixture.id,
        run_id: run.id,
        direct_contract_id: fixture.direct_contract_id,
        generated_case_id: referral.public_case_id,
        direct_leader_id: fixture.direct_leader_id,
        generated_leader_id: fixture.generated_leader_id,
        direct_party_id: fixture.direct_party_id,
        generated_party_id: fixture.generated_party_id,
    }]
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
        || ctx.db.party_authority().iter().next().is_some()
    {
        return Err("Simulation claim refuses player-bearing character or party state".into());
    }
    let imported_world_ready = ctx
        .db
        .world_data_import()
        .id()
        .find(0)
        .is_some_and(|import| import.completed);
    if ctx.db.settlement().iter().next().is_some() && !imported_world_ready {
        return Err(
            "Simulation claim permits settlements only from a completed world-data import".into(),
        );
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
    crate::strategic::seed_world(ctx, false)
}

/// Install deterministic quest coverage through private authority only. The
/// simulator must still discover, accept, travel, fight, and report through
/// the same public surfaces and ordinary reducers as a player.
#[reducer]
pub fn seed_simulation_quest_fixture(
    ctx: &ReducerContext,
    nonce: String,
    direct_leader_id: u64,
    generated_leader_id: u64,
) -> Result<(), String> {
    let run = owned_run(ctx, &nonce)?;
    if direct_leader_id == generated_leader_id {
        return Err("Quest coverage requires two distinct party leaders".into());
    }
    for character_id in [direct_leader_id, generated_leader_id] {
        let simulation_character = ctx
            .db
            .simulation_character()
            .character_id()
            .find(character_id)
            .ok_or("Quest coverage leader is not owned by this simulation run")?;
        if simulation_character.run_id != run.id {
            return Err("Quest coverage leader belongs to another simulation run".into());
        }
        let character = ctx
            .db
            .character()
            .id()
            .find(character_id)
            .ok_or("Quest coverage leader does not exist")?;
        let party = character
            .party_id
            .as_deref()
            .and_then(|party_id| ctx.db.party_authority().id().find(party_id.to_owned()))
            .ok_or("Quest coverage leader is not in a party")?;
        if party.leader_id != character_id {
            return Err("Quest coverage character is not its party leader".into());
        }
    }
    if let Some(existing) = ctx.db.simulation_quest_fixture_authority().id().find(0) {
        return if existing.run_id == run.id
            && existing.direct_leader_id == direct_leader_id
            && existing.generated_leader_id == generated_leader_id
        {
            Ok(())
        } else {
            Err("Simulation quest fixture is already bound to different leaders".into())
        };
    }
    let party_power = |leader_id| -> Result<u64, String> {
        let party_id = ctx
            .db
            .character()
            .id()
            .find(leader_id)
            .and_then(|character| character.party_id)
            .ok_or("Quest coverage leader has no party")?;
        let party = ctx
            .db
            .party_authority()
            .id()
            .find(&party_id)
            .ok_or("Quest coverage party does not exist")?;
        crate::condition::refresh_character_strategic_condition(ctx, leader_id)?;
        for member_id in crate::strategic::living_party_member_ids(ctx, &party_id) {
            crate::capability::refresh_character_capability(ctx, member_id)?;
        }
        crate::strategic::publicly_ready_party_combat_power(ctx, &party).map(|(_, power)| power)
    };
    let direct_power = party_power(direct_leader_id)?;
    let fixture_enemy_power = crate::strategic::simulation_quest_fixture_enemy_power()?;
    if direct_power == 0
        || !adventuresim_core::autoresolve::combat_power_meets_safety_margin(
            direct_power,
            fixture_enemy_power,
        )
        .unwrap_or(false)
    {
        return Err("Quest coverage has no party safe for its ordinary fixture opponent".into());
    }
    let seeded = crate::strategic::seed_simulation_quest_fixture_inner(
        ctx,
        run.policy_seed,
        direct_leader_id,
        generated_leader_id,
    )?;
    ctx.db
        .simulation_quest_fixture_authority()
        .insert(SimulationQuestFixtureAuthority {
            id: 0,
            run_id: run.id,
            direct_contract_id: seeded.direct_contract_id,
            generated_canonical_case_id: seeded.generated_canonical_case_id,
            direct_leader_id,
            generated_leader_id,
            direct_party_id: seeded.direct_party_id,
            generated_party_id: seeded.generated_party_id,
        });
    Ok(())
}

/// Advance authoritative world time only in a capability-owned disposable
/// simulation. Production time remains derived exclusively from wall time.
#[reducer]
pub fn advance_simulation_world_time(
    ctx: &ReducerContext,
    nonce: String,
    delta_minutes: u64,
) -> Result<(), String> {
    owned_run(ctx, &nonce)?;
    if !valid_simulation_clock_advance(delta_minutes) {
        return Err("Simulation world-time advance is outside the bounded range".into());
    }
    crate::time::refresh_clock(ctx)?;
    let mut clock = ctx
        .db
        .world_clock()
        .id()
        .find(0)
        .ok_or("Simulation world clock is not initialized")?;
    // Move the epoch by the inverse authoritative-clock transform, rounding up
    // so every requested official minute is observed.
    let delta_micros = simulation_epoch_shift_micros(delta_minutes)
        .ok_or("Simulation world-time advance overflow")?;
    clock.epoch_micros = clock
        .epoch_micros
        .checked_sub(delta_micros)
        .ok_or("Simulation world epoch underflow")?;
    clock.official_minutes = clock.official_minutes.saturating_add(delta_minutes);
    ctx.db.world_clock().id().update(clock);
    Ok(())
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
#[expect(
    clippy::too_many_arguments,
    reason = "the simulation reducer exposes each independently controlled fixture field"
)]
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
        skills.polearm_hours,
        skills.axe_hours,
        skills.bludgeon_hours,
        skills.sword_hours,
        skills.knife_hours,
        skills.dodge_hours,
        skills.block_hours,
        skills.bow_hours,
        skills.crossbow_hours,
        skills.firearm_hours,
        skills.throw_hours,
        skills.will_hours,
        skills.insight_hours,
        skills.charm_hours,
        skills.command_hours,
        skills.deception_hours,
        skills.physiology_hours,
        skills.stealth_hours,
        skills.balance_hours,
        skills.tailoring_hours,
    ]
    .into_iter()
    .all(|value| value.is_finite() && (0.0..=MAX_SIMULATION_SKILL_HOURS).contains(&value))
        && simulation_religion_hours_valid(skills.religion_hours)
        && simulation_bestiary_hours_valid(skills.bestiary_hours)
        && skills
            .oral_languages
            .direct_fields_valid(MAX_SIMULATION_SKILL_HOURS)
        && skills
            .written_languages
            .direct_fields_valid(MAX_SIMULATION_SKILL_HOURS);
    if !attributes_valid || !skills_valid || downtime.allocated_minutes() > MINUTES_PER_DAY {
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
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Simulation party not found")?;
    if solo_party.leader_id != character_id || !solo_party.is_solo {
        return Err("Simulation character must still lead its fresh solo party".into());
    }
    character.current_settlement_id = Some(settlement_id.clone());
    crate::investigation::set_character_case_site(ctx, character.id, None)?;
    ctx.db.character().id().update(character);
    solo_party.current_settlement_id = Some(settlement_id.clone());
    solo_party.current_case_site_id = None;
    ctx.db.party_authority().id().update(solo_party);
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
    crate::personality::reset_personality_from_visible(ctx, personality);
    // A configured evaluation adventurer needs a small working purse so an
    // inn-only seed settlement does not deadlock before its first scheduled
    // labor day can be applied. All ordinary costs still go through the same
    // currency reducers used by players.
    crate::item::credit_personal_currency(
        ctx,
        character_id,
        &settlement_id,
        SIMULATION_STARTING_COIN,
    )?;
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    crate::capability::refresh_character_capability(ctx, character_id)?;
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
    disease_id: String,
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
    let disease = crate::disease::parse_id(&disease_id)
        .map_err(|_| "Unknown simulation fixture disease".to_owned())?;
    ctx.db
        .infection_episode()
        .insert(crate::disease::InfectionEpisodeRow {
            id: 0,
            character_id,
            disease_id,
            contracted_at: 0,
            ruleset_version: adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION,
            phenotype_key_version: adventuresim_core::physiology::PHENOTYPE_KEY_VERSION,
        });
    let requested = adventuresim_core::disease::definition(disease)
        .incubation_minutes
        .saturating_add(60);
    // Advance through the same disease interval hooks as ordinary gameplay so
    // the simulator observes symptom onset instead of receiving hidden fixture
    // knowledge from the private infection row.
    let injury_limit = crate::surgery::preview_injury_boundary(
        ctx,
        character_id,
        requested,
        crate::surgery::InjuryRecoveryMinutes::NONE,
    )?
    .elapsed;
    let (elapsed, terminal) =
        crate::disease::clip_elapsed_for_disease(ctx, character_id, injury_limit, false)?;
    let mut time = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Simulation character time not found")?;
    let settled = crate::surgery::settle_injuries(
        ctx,
        character_id,
        elapsed,
        crate::surgery::InjuryRecoveryMinutes::NONE,
    )?;
    time.minutes = time.minutes.saturating_add(settled.elapsed);
    let interval_end = time.minutes;
    ctx.db.character_time().character_id().update(time);
    crate::disease::finish_disease_interval(ctx, character_id, terminal)?;
    crate::time::settle_lifecycle_after_character_time_write(ctx, character_id, interval_end)?;
    if terminal.is_some() || !settled.alive {
        return Ok(());
    }
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    crate::capability::refresh_character_capability(ctx, character_id)?;
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
    use super::{
        MAX_SIMULATION_CLOCK_ADVANCE_MINUTES, simulation_epoch_shift_micros,
        simulation_religion_hours_valid, valid_simulation_clock_advance,
    };
    use adventuresim_world_schema::ReligionHours;

    #[test]
    fn claim_checks_bootstrap_capability_before_world_freshness() {
        let source = crate::production_source(include_str!("simulation.rs"));
        let claim = source
            .split("pub fn claim_simulation_run")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn owned_run")
            .next()
            .unwrap();
        let authorization = claim.find("simulation_bootstrap_authorized").unwrap();
        let character_guard = claim.find("ctx.db.character()").unwrap();
        let imported_world_guard = claim.find("world_data_import()").unwrap();
        assert!(authorization < character_guard);
        assert!(authorization < imported_world_guard);
    }

    #[test]
    fn quest_fixture_public_provenance_requires_an_initial_rumor_receipt() {
        let source = crate::production_source(include_str!("simulation.rs"));
        let authority = source
            .split("pub struct SimulationQuestFixtureAuthority")
            .nth(1)
            .unwrap()
            .split("pub struct SimulationQuestFixture")
            .next()
            .unwrap();
        assert!(authority.contains("generated_canonical_case_id"));
        let projection = source
            .split("pub fn simulation_quest_fixture")
            .nth(1)
            .unwrap()
            .split("fn valid_nonce")
            .next()
            .unwrap();
        assert!(projection.contains("investigation_witness_referral()"));
        assert!(projection.contains("validated_generated_dialogue_manifest("));
        assert!(projection.contains("referral.grant_kind == \"initial_rumor\""));
        assert!(projection.contains("!referral.source_receipt_id.is_empty()"));
        assert!(projection.contains("local_problem_receipt()"));
        assert!(projection.contains("receipt.problem_id == manifest.problem_id"));
        assert!(projection.contains("receipt.expected_location_id == witness.expected_location"));
        assert!(projection.contains("generated_case_id: referral.public_case_id"));
    }

    #[test]
    fn quest_fixture_preserves_designated_direct_and_generated_leaders() {
        let source = crate::production_source(include_str!("simulation.rs"));
        let reducer = source
            .split("pub fn seed_simulation_quest_fixture")
            .nth(1)
            .unwrap()
            .split("pub fn advance_simulation_world_time")
            .next()
            .unwrap();
        assert!(reducer.contains("let direct_power = party_power(direct_leader_id)?"));
        assert!(!reducer.contains("second_power"));
        assert!(!reducer.contains("(generated_leader_id, direct_leader_id"));
        assert!(reducer.contains(
            "seed_simulation_quest_fixture_inner(\n        ctx,\n        run.policy_seed,\n        direct_leader_id,\n        generated_leader_id"
        ));
        assert!(reducer.contains("existing.direct_leader_id == direct_leader_id"));
        assert!(reducer.contains("existing.generated_leader_id == generated_leader_id"));
    }

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

    #[test]
    fn simulation_world_time_advance_is_positive_and_bounded() {
        assert!(!valid_simulation_clock_advance(0));
        assert!(valid_simulation_clock_advance(
            adventuresim_core::strategic_time::MINUTES_PER_DAY
        ));
        assert!(valid_simulation_clock_advance(
            MAX_SIMULATION_CLOCK_ADVANCE_MINUTES
        ));
        assert!(!valid_simulation_clock_advance(
            MAX_SIMULATION_CLOCK_ADVANCE_MINUTES + 1
        ));
        let shift =
            simulation_epoch_shift_micros(adventuresim_core::strategic_time::MINUTES_PER_DAY)
                .unwrap();
        assert_eq!(
            adventuresim_core::strategic_time::elapsed_official_minutes(-shift, 0),
            adventuresim_core::strategic_time::MINUTES_PER_DAY
        );
    }
}
