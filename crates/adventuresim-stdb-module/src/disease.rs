//! Durable strategic disease facts and authoritative treatment.

use adventuresim_core::disease::{
    self, DiseaseEventKind, DiseaseId, InfectionEpisode, TerminalFailure,
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::character::character as _;
use crate::{character_attributes, character_capability, character_time};

/// The complete per-character disease state. This table is deliberately
/// private: strategic-web derives a viewer-specific presentation instead of
/// forwarding these rows to browsers.
#[derive(Clone, Debug)]
#[table(accessor = infection_episode)]
pub struct InfectionEpisodeRow {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub disease_id: String,
    pub contracted_at: u64,
    pub treated_at: Option<u64>,
}

/// Public world fact, not private medical information. Overlap with a
/// character's exact local clock is evaluated continuously and deterministically.
#[derive(Clone, Debug)]
#[table(accessor = settlement_outbreak, public)]
pub struct SettlementOutbreak {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub settlement_id: String,
    pub disease_id: String,
    pub start_minute: u64,
    pub end_minute: u64,
    pub intensity: f32,
}

/// Narrow durable provenance for committed cuts. No tactical tick state crosses
/// this boundary.
#[derive(Clone, Debug)]
#[table(accessor = committed_cut)]
pub struct CommittedCut {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub committed_at: u64,
    pub severity: f32,
    pub surgery_check: f32,
}

/// Delivery dedupe is explicitly separate from infection state and contains no
/// undiagnosed disease identity.
#[derive(Clone, Debug)]
#[table(accessor = disease_notice)]
pub struct DiseaseNotice {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    pub minute: u64,
    pub kind: String,
    pub message: String,
}

/// Raw infection facts for strategic-web. This view is intentionally absent
/// from browser subscriptions; the SSR backend sanitizes it against the active
/// character's Medicine capability before rendering any response. Deployments
/// must keep the strategic SpacetimeDB endpoint on the server network.
#[view(accessor = backend_infection_episodes, public)]
pub fn backend_infection_episodes(ctx: &ViewContext) -> Vec<InfectionEpisodeRow> {
    ctx.db
        .infection_episode()
        .character_id()
        .filter(0u64..)
        .collect()
}

/// Committed visible cut provenance for strategic-web's sanitizer.
#[view(accessor = backend_committed_cuts, public)]
pub fn backend_committed_cuts(ctx: &ViewContext) -> Vec<CommittedCut> {
    ctx.db
        .committed_cut()
        .character_id()
        .filter(0u64..)
        .collect()
}

fn notice(
    ctx: &ReducerContext,
    character_id: u64,
    infection_id: u64,
    minute: u64,
    kind: &str,
    message: &str,
) {
    let id = format!("disease-{infection_id}-{minute}-{kind}");
    if ctx.db.disease_notice().id().find(&id).is_none() {
        ctx.db.disease_notice().insert(DiseaseNotice {
            id,
            character_id,
            minute,
            kind: kind.into(),
            message: message.into(),
        });
    }
}

#[derive(Clone, Copy, Debug, SpacetimeType)]
pub enum DiseaseTerminalCause {
    Respiratory,
    Circulatory,
    Homeostatic,
    Neurologic,
}

fn parse_id(value: &str) -> Result<DiseaseId, String> {
    match value {
        "influenza" => Ok(DiseaseId::Influenza),
        "dysentery" => Ok(DiseaseId::Dysentery),
        "typhus" => Ok(DiseaseId::Typhus),
        "tetanus" => Ok(DiseaseId::Tetanus),
        "erysipelas" => Ok(DiseaseId::Erysipelas),
        "smallpox" => Ok(DiseaseId::Smallpox),
        "plague" => Ok(DiseaseId::Plague),
        "consumption" => Ok(DiseaseId::Consumption),
        _ => Err("Unknown disease".into()),
    }
}

fn episode(row: &InfectionEpisodeRow) -> Result<InfectionEpisode, String> {
    Ok(InfectionEpisode {
        id: row.id,
        character_id: row.character_id,
        disease_id: parse_id(&row.disease_id)?,
        contracted_at: row.contracted_at,
        treated_at: row.treated_at,
    })
}

pub fn character_episodes(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<Vec<InfectionEpisode>, String> {
    ctx.db
        .infection_episode()
        .character_id()
        .filter(character_id)
        .map(|row| episode(&row))
        .collect()
}

pub fn effective_attributes(
    ctx: &ReducerContext,
    character_id: u64,
    mut attributes: crate::CharacterAttributes,
) -> Result<crate::CharacterAttributes, String> {
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |t| t.minutes);
    let (penalty, _, _, _) = disease::combined_state(
        &character_episodes(ctx, character_id)?,
        now,
        attributes.immunity,
    );
    attributes.endurance = (attributes.endurance - penalty.endurance).max(0.0);
    attributes.immunity = (attributes.immunity - penalty.immunity).max(0.0);
    attributes.gut = (attributes.gut - penalty.gut).max(0.0);
    attributes.intelligence = (attributes.intelligence - penalty.intelligence).max(0.0);
    attributes.instinct = (attributes.instinct - penalty.instinct).max(0.0);
    for value in [
        &mut attributes.left_arm_agility,
        &mut attributes.right_arm_agility,
        &mut attributes.left_leg_agility,
        &mut attributes.right_leg_agility,
    ] {
        *value = (*value - penalty.limb_agility).max(0.0)
    }
    Ok(attributes)
}

fn outbreak_episodes_through(
    ctx: &ReducerContext,
    character_id: u64,
    from: u64,
    to: u64,
) -> Result<Vec<InfectionEpisode>, String> {
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Ok(Vec::new());
    };
    let Some(settlement_id) = character.current_settlement_id else {
        return Ok(Vec::new());
    };
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |a| a.immunity);
    let mut episodes = character_episodes(ctx, character_id)?;
    let existing_len = episodes.len();
    for outbreak in ctx
        .db
        .settlement_outbreak()
        .settlement_id()
        .filter(&settlement_id)
    {
        let disease_id = parse_id(&outbreak.disease_id)?;
        let prior = disease::acquired_immunity(&episodes, disease_id, from, immunity);
        let overlap_from = from.max(outbreak.start_minute);
        let overlap_to = to.min(outbreak.end_minute);
        if overlap_to <= overlap_from {
            continue;
        }
        let Some(at) = disease::first_presence_exposure_minute(
            character_id,
            &outbreak.id,
            overlap_from,
            overlap_to,
            outbreak.intensity,
            disease::definition(disease_id).base_acquisition,
            immunity,
            prior,
        ) else {
            continue;
        };
        if !episodes
            .iter()
            .any(|episode| episode.disease_id == disease_id && episode.contracted_at == at)
        {
            episodes.push(InfectionEpisode {
                id: disease::outbreak_exposure_seed(character_id, &format!("{}:{at}", outbreak.id)),
                character_id,
                disease_id,
                contracted_at: at,
                treated_at: None,
            });
        }
    }
    Ok(episodes.split_off(existing_len))
}

fn acquire_outbreaks_through(
    ctx: &ReducerContext,
    character_id: u64,
    from: u64,
    to: u64,
) -> Result<(), String> {
    for episode in outbreak_episodes_through(ctx, character_id, from, to)? {
        let disease_id = match episode.disease_id {
            DiseaseId::Influenza => "influenza",
            DiseaseId::Dysentery => "dysentery",
            DiseaseId::Typhus => "typhus",
            DiseaseId::Tetanus => "tetanus",
            DiseaseId::Erysipelas => "erysipelas",
            DiseaseId::Smallpox => "smallpox",
            DiseaseId::Plague => "plague",
            DiseaseId::Consumption => "consumption",
        };
        if !ctx
            .db
            .infection_episode()
            .character_id()
            .filter(character_id)
            .any(|row| row.disease_id == disease_id && row.contracted_at == episode.contracted_at)
        {
            ctx.db.infection_episode().insert(InfectionEpisodeRow {
                id: episode.id,
                character_id,
                disease_id: disease_id.into(),
                contracted_at: episode.contracted_at,
                treated_at: None,
            });
        }
    }
    Ok(())
}

/// Returns the safe prefix of an interval and a terminal mechanism, if any.
/// All boundary events at the earliest minute are considered together.
pub fn clip_elapsed_for_disease(
    ctx: &ReducerContext,
    character_id: u64,
    requested: u64,
) -> Result<(u64, Option<TerminalFailure>), String> {
    if requested == 0 {
        return Ok((0, None));
    }
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |t| t.minutes);
    acquire_outbreaks_through(ctx, character_id, now, now.saturating_add(requested))?;
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |a| a.immunity);
    let episodes = character_episodes(ctx, character_id)?;
    let mut events = episodes
        .iter()
        .copied()
        .flat_map(|e| disease::interval_events(e, now, now.saturating_add(requested), immunity))
        .collect::<Vec<_>>();
    events.sort_by_key(|e| e.minute);
    let terminal =
        disease::first_combined_terminal(&episodes, now, now.saturating_add(requested), immunity);
    let death_minute = terminal.map(|value| value.0);
    let through = death_minute.unwrap_or_else(|| now.saturating_add(requested));
    for event in events.iter().filter(|event| event.minute <= through) {
        match event.kind {
            DiseaseEventKind::SymptomOnset => notice(
                ctx,
                character_id,
                event.infection_id,
                event.minute,
                "symptom-onset",
                "New symptoms have appeared.",
            ),
            DiseaseEventKind::Peak => {}
            DiseaseEventKind::Critical(_) => notice(
                ctx,
                character_id,
                event.infection_id,
                event.minute,
                "critical",
                "A vital humour is failing.",
            ),
            DiseaseEventKind::Resolution => notice(
                ctx,
                character_id,
                event.infection_id,
                event.minute,
                "resolution",
                "The illness's visible effects have resolved.",
            ),
        }
    }
    let Some(death_minute) = death_minute else {
        return Ok((requested, None));
    };
    notice(
        ctx,
        character_id,
        0,
        death_minute,
        "critical",
        "A vital humour is failing.",
    );
    Ok((
        death_minute.saturating_sub(now),
        terminal.map(|value| value.1),
    ))
}

/// Side-effect-free party preflight. Acquisition and notice delivery happen
/// only in the subsequent committed interval.
pub fn preview_elapsed_for_disease(
    ctx: &ReducerContext,
    character_id: u64,
    requested: u64,
) -> Result<u64, String> {
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |t| t.minutes);
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |a| a.immunity);
    let mut episodes = character_episodes(ctx, character_id)?;
    episodes.extend(outbreak_episodes_through(
        ctx,
        character_id,
        now,
        now.saturating_add(requested),
    )?);
    Ok(
        disease::first_combined_terminal(&episodes, now, now.saturating_add(requested), immunity)
            .map_or(requested, |(minute, _)| minute.saturating_sub(now)),
    )
}

pub fn finish_disease_interval(
    ctx: &ReducerContext,
    character_id: u64,
    cause: Option<TerminalFailure>,
) -> Result<(), String> {
    let Some(cause) = cause else { return Ok(()) };
    crate::transition_character_to_dead(
        ctx,
        character_id,
        match cause {
            TerminalFailure::Respiratory => crate::DeathCause::RespiratoryFailure,
            TerminalFailure::Circulatory => crate::DeathCause::CirculatoryFailure,
            TerminalFailure::Homeostatic => crate::DeathCause::HomeostaticFailure,
            TerminalFailure::Neurologic => crate::DeathCause::NeurologicFailure,
        },
        crate::DeathSource::Disease,
        Some(
            match cause {
                TerminalFailure::Respiratory => "respiratory-failure",
                TerminalFailure::Circulatory => "circulatory-failure",
                TerminalFailure::Homeostatic => "homeostatic-failure",
                TerminalFailure::Neurologic => "neurologic-failure",
            }
            .into(),
        ),
    )?;
    Ok(())
}

pub fn record_committed_cut(
    ctx: &ReducerContext,
    character_id: u64,
    severity: f32,
    surgery_check: f32,
) -> Result<(), String> {
    if severity <= 0.0 {
        return Ok(());
    }
    let at = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |t| t.minutes);
    let cut = ctx.db.committed_cut().insert(CommittedCut {
        id: 0,
        character_id,
        committed_at: at,
        severity,
        surgery_check,
    });
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |a| a.immunity);
    let residual = (1.0 - (surgery_check / 5.0).clamp(0.0, 1.0) * 0.8) * severity.clamp(0.0, 1.0);
    for disease_id in [DiseaseId::Tetanus, DiseaseId::Erysipelas] {
        let d = disease::definition(disease_id);
        let seed = disease::outbreak_exposure_seed(
            character_id,
            &format!("cut-{}-{disease_id:?}", cut.id),
        );
        if disease::acquisition_succeeds(seed, d, immunity, 0.0, residual) {
            ctx.db.infection_episode().insert(InfectionEpisodeRow {
                id: 0,
                character_id,
                disease_id: format!("{disease_id:?}").to_ascii_lowercase(),
                contracted_at: at,
                treated_at: None,
            });
        }
    }
    Ok(())
}

/// Create or reset the visibly ill character used by the default development
/// seed. The fixture starts at influenza's peak so its symptoms and penalties
/// are immediately available for UI and treatment testing.
#[reducer]
pub fn seed_sick_character(ctx: &ReducerContext) -> Result<(), String> {
    const SICK_CHARACTER_ID: u64 = 9_999_999_999_999_998;
    const DEMO_DISEASE_AGE: u64 = 3 * 1_440;

    if ctx.db.character().id().find(SICK_CHARACTER_ID).is_none() {
        crate::character::insert_new_character(
            ctx,
            "Sick Demo".to_string(),
            SICK_CHARACTER_ID,
            false,
        )?;
    }

    let mut character_time = ctx
        .db
        .character_time()
        .character_id()
        .find(SICK_CHARACTER_ID)
        .ok_or_else(|| "Sick demo character is missing time data".to_string())?;
    character_time.minutes = character_time.minutes.max(DEMO_DISEASE_AGE);
    let contracted_at = character_time.minutes - DEMO_DISEASE_AGE;
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);

    let existing: Vec<_> = ctx
        .db
        .infection_episode()
        .character_id()
        .filter(SICK_CHARACTER_ID)
        .map(|row| row.id)
        .collect();
    for id in existing {
        ctx.db.infection_episode().id().delete(id);
    }
    ctx.db.infection_episode().insert(InfectionEpisodeRow {
        id: 0,
        character_id: SICK_CHARACTER_ID,
        disease_id: "influenza".into(),
        contracted_at,
        treated_at: None,
    });
    crate::capability::refresh_character_capability(ctx, SICK_CHARACTER_ID)?;
    Ok(())
}

#[reducer]
pub fn treat_disease(
    ctx: &ReducerContext,
    doctor_id: u64,
    target_id: u64,
    infection_id: u64,
) -> Result<(), String> {
    // Character selection/ownership is enforced by strategic-web's session
    // POST boundary, matching the rest of the strategic reducer surface.
    let doctor = crate::require_living_character(ctx, doctor_id)?;
    let target = crate::require_living_character(ctx, target_id)?;
    let same_place = doctor.current_settlement_id.is_some()
        && doctor.current_settlement_id == target.current_settlement_id
        || doctor.current_quest_location_id.is_some()
            && doctor.current_quest_location_id == target.current_quest_location_id;
    if !same_place {
        return Err("Doctor and patient must be together".into());
    }
    if doctor_id != target_id && (doctor.party_id.is_none() || doctor.party_id != target.party_id) {
        return Err("A doctor may treat only themselves or a member of their party".into());
    }
    let medicine = ctx
        .db
        .character_capability()
        .character_id()
        .find(doctor_id)
        .ok_or("Doctor capability not found")?
        .medicine;
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(target_id)
        .ok_or("Patient time not found")?
        .minutes;
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(target_id)
        .ok_or("Patient attributes not found")?
        .immunity;
    let mut row = ctx
        .db
        .infection_episode()
        .id()
        .find(infection_id)
        .ok_or("Infection not found")?;
    if row.character_id != target_id {
        return Err("Infection does not belong to this patient".into());
    }
    if row.treated_at.is_some() {
        return Err("This illness has already been treated".into());
    }
    let state = disease::evaluate(episode(&row)?, now, immunity);
    if matches!(
        state.stage,
        disease::DiseaseStage::Resolved | disease::DiseaseStage::Incubating
    ) {
        return Err("This illness cannot currently be treated".into());
    }
    if medicine < state.diagnosis_dc {
        return Err("Medicine skill is too low to identify this illness".into());
    }
    row.treated_at = Some(now);
    ctx.db.infection_episode().id().update(row);
    notice(
        ctx,
        target_id,
        infection_id,
        now,
        "treatment",
        "Treatment was administered.",
    );
    Ok(())
}
