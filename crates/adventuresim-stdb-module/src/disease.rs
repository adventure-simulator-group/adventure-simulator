//! Durable strategic disease facts and authoritative treatment.

use adventuresim_core::disease::{
    self, DiseaseEventKind, DiseaseId, InfectionEpisode, TerminalFailure,
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::character::character as _;
use crate::{
    character_attributes, character_capability, character_condition, character_skills,
    character_time,
    item::{inventory_item, item},
    strategic::{party, party_inventory_item, settlement},
};

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

/// Public agent knowledge distilled from symptom notices. It deliberately
/// carries no infection ID, disease identity, differential, or private vitals.
#[derive(Clone, Debug)]
#[table(accessor = character_illness_status, public)]
pub struct CharacterIllnessStatus {
    #[primary_key]
    pub character_id: u64,
    pub symptomatic: bool,
    pub critical: bool,
    pub updated_at_minute: u64,
}

/// Pending one-shot result of a completed fifteen-minute examination. It is
/// removed when the doctor treats or declines and is never medical history.
#[derive(Clone, Debug)]
#[table(accessor = medical_examination)]
pub struct MedicalExamination {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub doctor_id: u64,
    #[index(btree)]
    pub target_id: u64,
    pub examined_at: u64,
    pub findings: Vec<String>,
    pub reveals_vitals: bool,
    pub sanguine: f32,
    pub phlegmatic: f32,
    pub choleric: f32,
    pub melancholic: f32,
    pub possible_disease_ids: Vec<String>,
    pub confirmed_infection_ids: Vec<u64>,
    pub confirmed_disease_ids: Vec<String>,
    pub confirmed_stages: Vec<String>,
}

/// Persistent settlement service parameters. This is private because the
/// browser needs the herbalist's offer and result, not their hidden skill roll.
#[derive(Clone, Debug)]
#[table(accessor = settlement_herbalist)]
pub struct SettlementHerbalist {
    #[primary_key]
    pub settlement_id: String,
    pub medicine_skill: u8,
}

/// Narrow, one-shot NPC result. Unlike a player-doctor examination it never
/// contains symptoms, vitals, stages, infection IDs, or a differential.
#[derive(Clone, Debug)]
#[table(
    accessor = herbalist_examination,
    index(accessor = patient_id, btree(columns = [patient_id]))
)]
pub struct HerbalistExamination {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub patient_id: u64,
    pub settlement_id: String,
    pub disease_names: Vec<String>,
    pub medication_names: Vec<String>,
}

/// Medication courses currently being taken. This is deliberately a separate,
/// unbounded equipment list rather than one of the character's physical slots.
#[derive(Clone, Debug)]
#[table(
    accessor = equipped_medication, public,
    index(accessor = medication_character_id, btree(columns = [character_id])),
)]
pub struct EquippedMedication {
    #[primary_key]
    pub inventory_item_id: u64,
    pub character_id: u64,
    pub disease_id: String,
    pub equipped_at: u64,
}

/// Narrow raw facts for the trusted SSR presentation boundary. The database
/// endpoint is server-network-only; browsers never subscribe to these views.
#[view(accessor = backend_infection_episodes, public)]
pub fn backend_infection_episodes(ctx: &ViewContext) -> Vec<InfectionEpisodeRow> {
    ctx.db
        .infection_episode()
        .character_id()
        .filter(0u64..)
        .collect()
}

#[view(accessor = backend_committed_cuts, public)]
pub fn backend_committed_cuts(ctx: &ViewContext) -> Vec<CommittedCut> {
    ctx.db
        .committed_cut()
        .character_id()
        .filter(0u64..)
        .collect()
}

#[view(accessor = backend_medical_examinations, public)]
pub fn backend_medical_examinations(ctx: &ViewContext) -> Vec<MedicalExamination> {
    ctx.db
        .medical_examination()
        .doctor_id()
        .filter(0u64..)
        .collect()
}

/// Server-network-only SSR boundary for the one-shot name-only NPC result.
#[view(accessor = backend_herbalist_examinations, public)]
pub fn backend_herbalist_examinations(ctx: &ViewContext) -> Vec<HerbalistExamination> {
    ctx.db
        .herbalist_examination()
        .patient_id()
        .filter(0u64..)
        .collect()
}

pub(crate) fn ensure_settlement_herbalist(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> SettlementHerbalist {
    if let Some(row) = ctx
        .db
        .settlement_herbalist()
        .settlement_id()
        .find(settlement_id.to_owned())
    {
        return row;
    }
    ctx.db.settlement_herbalist().insert(SettlementHerbalist {
        settlement_id: settlement_id.to_owned(),
        medicine_skill: adventuresim_core::strategic_economy::settlement_herbalist_medicine_skill(
            settlement_id,
        ),
    })
}

#[reducer]
pub fn backfill_settlement_herbalists(ctx: &ReducerContext) {
    for settlement in ctx.db.settlement().iter().collect::<Vec<_>>() {
        ensure_settlement_herbalist(ctx, &settlement.id);
    }
}

fn notice(
    ctx: &ReducerContext,
    character_id: u64,
    infection_id: u64,
    minute: u64,
    kind: &str,
    message: &str,
) -> Result<(), String> {
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
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |attributes| attributes.immunity);
    let states = character_episodes(ctx, character_id)?
        .into_iter()
        .map(|episode| disease::evaluate(episode, minute, immunity))
        .collect::<Vec<_>>();
    let symptomatic = states.iter().any(|state| {
        !matches!(
            state.stage,
            disease::DiseaseStage::Incubating | disease::DiseaseStage::Resolved
        )
    });
    let critical = states
        .iter()
        .any(|state| state.stage == disease::DiseaseStage::Critical);
    let row = CharacterIllnessStatus {
        character_id,
        symptomatic,
        critical,
        updated_at_minute: minute,
    };
    if ctx
        .db
        .character_illness_status()
        .character_id()
        .find(character_id)
        .is_some()
    {
        ctx.db.character_illness_status().character_id().update(row);
    } else {
        ctx.db.character_illness_status().insert(row);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, SpacetimeType)]
pub enum DiseaseTerminalCause {
    Respiratory,
    Circulatory,
    Homeostatic,
    Neurologic,
}

pub(crate) fn parse_id(value: &str) -> Result<DiseaseId, String> {
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

fn stage_key(stage: disease::DiseaseStage) -> &'static str {
    match stage {
        disease::DiseaseStage::Incubating => "hidden",
        disease::DiseaseStage::Early => "early",
        disease::DiseaseStage::Established => "established",
        disease::DiseaseStage::Critical => "critical",
        disease::DiseaseStage::Convalescent => "recovering",
        disease::DiseaseStage::Resolved => "resolved",
    }
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
    let mut outbreaks = ctx
        .db
        .settlement_outbreak()
        .settlement_id()
        .filter(&settlement_id)
        .collect::<Vec<_>>();
    outbreaks.sort_by(|left, right| {
        (left.start_minute, left.id.as_str()).cmp(&(right.start_minute, right.id.as_str()))
    });
    for outbreak in outbreaks {
        let disease_id = parse_id(&outbreak.disease_id)?;
        let overlap_from = from.max(outbreak.start_minute);
        let overlap_to = to.min(outbreak.end_minute);
        if overlap_to <= overlap_from {
            continue;
        }
        let Some(at) = disease::first_eligible_presence_exposure_minute(
            &episodes,
            disease_id,
            character_id,
            &outbreak.id,
            overlap_from,
            overlap_to,
            outbreak.intensity,
            disease::definition(disease_id).base_acquisition,
            immunity,
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

fn persist_outbreak_episodes(
    ctx: &ReducerContext,
    character_id: u64,
    episodes: impl IntoIterator<Item = InfectionEpisode>,
) -> Result<(), String> {
    for episode in episodes {
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
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |a| a.immunity);
    let mut episodes = character_episodes(ctx, character_id)?;
    let proposed =
        outbreak_episodes_through(ctx, character_id, now, now.saturating_add(requested))?;
    episodes.extend(proposed.iter().copied());
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
    // The terminal minute is inclusive: infections and notices occurring at
    // that boundary are committed; later effects from the requested interval
    // are never persisted.
    persist_outbreak_episodes(
        ctx,
        character_id,
        proposed
            .into_iter()
            .filter(|episode| disease::infection_occurs_through(*episode, through)),
    )?;
    for event in events.iter().filter(|event| event.minute <= through) {
        match event.kind {
            DiseaseEventKind::SymptomOnset => notice(
                ctx,
                character_id,
                event.infection_id,
                event.minute,
                "symptom-onset",
                "New symptoms have appeared.",
            )?,
            DiseaseEventKind::Peak => {}
            DiseaseEventKind::Critical(_) => notice(
                ctx,
                character_id,
                event.infection_id,
                event.minute,
                "critical",
                "A vital humour is failing.",
            )?,
            DiseaseEventKind::Resolution => notice(
                ctx,
                character_id,
                event.infection_id,
                event.minute,
                "resolution",
                "The illness's visible effects have resolved.",
            )?,
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
    )?;
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
    reconcile_medications(ctx, character_id)?;
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

fn disease_key(id: DiseaseId) -> &'static str {
    match id {
        DiseaseId::Influenza => "influenza",
        DiseaseId::Dysentery => "dysentery",
        DiseaseId::Typhus => "typhus",
        DiseaseId::Tetanus => "tetanus",
        DiseaseId::Erysipelas => "erysipelas",
        DiseaseId::Smallpox => "smallpox",
        DiseaseId::Plague => "plague",
        DiseaseId::Consumption => "consumption",
    }
}

fn reconcile_medications(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |time| time.minutes);
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |attributes| attributes.immunity);
    let active: Vec<String> = character_episodes(ctx, character_id)?
        .into_iter()
        .filter(|episode| {
            !matches!(
                disease::evaluate(*episode, now, immunity).stage,
                disease::DiseaseStage::Resolved
            )
        })
        .map(|episode| disease_key(episode.disease_id).to_owned())
        .collect();
    for medication in ctx
        .db
        .equipped_medication()
        .medication_character_id()
        .filter(character_id)
        .collect::<Vec<_>>()
    {
        if !active.contains(&medication.disease_id) {
            ctx.db
                .equipped_medication()
                .inventory_item_id()
                .delete(medication.inventory_item_id);
            ctx.db
                .inventory_item()
                .id()
                .delete(medication.inventory_item_id);
        }
    }
    Ok(())
}

pub fn medication_is_equipped(ctx: &ReducerContext, inventory_item_id: u64) -> bool {
    ctx.db
        .equipped_medication()
        .inventory_item_id()
        .find(inventory_item_id)
        .is_some()
}

#[reducer]
pub fn equip_medication(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
) -> Result<(), String> {
    crate::require_living_character(ctx, character_id)?;
    let inventory = ctx
        .db
        .inventory_item()
        .id()
        .find(inventory_item_id)
        .ok_or("Medication is not in this inventory")?;
    if inventory.character_id != character_id || inventory.quantity != 1 {
        return Err("Medication must be an individual course in the patient's inventory".into());
    }
    let recipe = disease::medication_recipe_for_item(&inventory.item_id)
        .ok_or("This item is not medication")?;
    if medication_is_equipped(ctx, inventory_item_id) {
        return Ok(());
    }
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Patient time not found")?
        .minutes;
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |attributes| attributes.immunity);
    let disease_id = disease_key(recipe.disease_id);
    let mut matched = false;
    for mut infection in ctx
        .db
        .infection_episode()
        .character_id()
        .filter(character_id)
        .filter(|infection| infection.disease_id == disease_id)
        .collect::<Vec<_>>()
    {
        let state = disease::evaluate(episode(&infection)?, now, immunity);
        if !matches!(state.stage, disease::DiseaseStage::Resolved) {
            matched = true;
            if infection.treated_at.is_none() {
                infection.treated_at = Some(now);
                ctx.db.infection_episode().id().update(infection);
            }
        }
    }
    if !matched {
        ctx.db.inventory_item().id().delete(inventory_item_id);
        return Ok(());
    }
    ctx.db.equipped_medication().insert(EquippedMedication {
        inventory_item_id,
        character_id,
        disease_id: disease_id.into(),
        equipped_at: now,
    });
    ctx.db.inventory_item().id().delete(inventory_item_id);
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

#[reducer]
pub fn unequip_medication(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
) -> Result<(), String> {
    crate::require_living_character(ctx, character_id)?;
    let medication = ctx
        .db
        .equipped_medication()
        .inventory_item_id()
        .find(inventory_item_id)
        .ok_or("Medication is not equipped")?;
    if medication.character_id != character_id {
        return Err("Medication belongs to another character".into());
    }
    ctx.db
        .equipped_medication()
        .inventory_item_id()
        .delete(inventory_item_id);
    Ok(())
}

fn consume_personal_ingredient(
    ctx: &ReducerContext,
    character_id: u64,
    item_id: &str,
    mut quantity: u32,
) -> Result<(), String> {
    let stacks: Vec<_> = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, item_id))
        .collect();
    if stacks.iter().map(|stack| stack.quantity).sum::<u32>() < quantity {
        return Err(format!("Not enough {item_id}"));
    }
    for mut stack in stacks {
        let taken = quantity.min(stack.quantity);
        stack.quantity -= taken;
        quantity -= taken;
        if stack.quantity == 0 {
            ctx.db.inventory_item().id().delete(stack.id);
        } else {
            ctx.db.inventory_item().id().update(stack);
        }
        if quantity == 0 {
            break;
        }
    }
    Ok(())
}

fn consume_party_ingredient(
    ctx: &ReducerContext,
    party_id: &str,
    item_id: &str,
    mut quantity: u32,
) -> Result<(), String> {
    let stacks: Vec<_> = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .filter(|stack| stack.item_id == item_id)
        .collect();
    if stacks.iter().map(|stack| stack.quantity).sum::<u32>() < quantity {
        return Err(format!("Not enough {item_id}"));
    }
    for mut stack in stacks {
        let taken = quantity.min(stack.quantity);
        stack.quantity -= taken;
        quantity -= taken;
        if stack.quantity == 0 {
            ctx.db.party_inventory_item().id().delete(stack.id);
        } else {
            ctx.db.party_inventory_item().id().update(stack);
        }
        if quantity == 0 {
            break;
        }
    }
    Ok(())
}

#[reducer]
pub fn craft_medication(
    ctx: &ReducerContext,
    character_id: u64,
    disease_id: String,
    party_scope: bool,
) -> Result<(), String> {
    let character = crate::require_living_character(ctx, character_id)?;
    if character.current_settlement_id.is_none() {
        return Err("Medication can only be prepared in a settlement".into());
    }
    let disease_id = parse_id(&disease_id)?;
    let recipe = disease::medication_recipe(disease_id);
    let medicine = ctx
        .db
        .character_capability()
        .character_id()
        .find(character_id)
        .ok_or("Medicine capability not found")?
        .medicine;
    if !disease::can_prepare_medication(medicine, recipe) {
        return Err(format!("Medicine {} is required", recipe.medicine_dc));
    }
    let party_id = party_scope
        .then(|| character.party_id.clone().ok_or("Character has no party"))
        .transpose()?;
    if let Some(party_id) = party_id.as_deref() {
        let party = ctx
            .db
            .party()
            .id()
            .find(&party_id.to_owned())
            .ok_or("Party not found")?;
        if party.leader_id != character_id {
            return Err("Only the party leader can consume shared ingredients".into());
        }
    }
    for ingredient in recipe.ingredients {
        let available = if let Some(party_id) = party_id.as_deref() {
            ctx.db
                .party_inventory_item()
                .party_id()
                .filter(party_id)
                .filter(|stack| stack.item_id == ingredient.item_id)
                .map(|stack| stack.quantity)
                .sum::<u32>()
        } else {
            ctx.db
                .inventory_item()
                .character_and_item_id()
                .filter((character_id, ingredient.item_id))
                .map(|stack| stack.quantity)
                .sum::<u32>()
        };
        if available < ingredient.quantity {
            return Err(format!("Not enough {}", ingredient.item_id));
        }
    }
    if !crate::time::advance_character_time(ctx, character_id, recipe.preparation_minutes)? {
        return Ok(());
    }
    for ingredient in recipe.ingredients {
        if let Some(party_id) = party_id.as_deref() {
            consume_party_ingredient(ctx, party_id, ingredient.item_id, ingredient.quantity)?;
        } else {
            consume_personal_ingredient(
                ctx,
                character_id,
                ingredient.item_id,
                ingredient.quantity,
            )?;
        }
    }
    if let Some(party_id) = party_id.as_deref() {
        crate::strategic::add_to_party_inventory(ctx, party_id, recipe.item_id, 1);
    } else {
        crate::add_inventory_item(ctx, character_id, recipe.item_id, 1);
    }
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

/// Deterministic standing-wound exposure. The stable token is derived from a
/// limb key and monotonically increasing exposure checkpoint, so changing the
/// size of elapsed-time chunks cannot change acquisition outcomes.
pub fn record_standing_cut_exposure(
    ctx: &ReducerContext,
    character_id: u64,
    severity: f32,
    surgery_check: f32,
    token: &str,
    contracted_at: u64,
) -> Result<(), String> {
    if severity <= 0.0 {
        return Ok(());
    }
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |a| a.immunity);
    let residual = (1.0 - (surgery_check / 5.0).clamp(0.0, 1.0) * 0.8) * severity.clamp(0.0, 1.0);
    for disease_id in [DiseaseId::Tetanus, DiseaseId::Erysipelas] {
        let definition = disease::definition(disease_id);
        let seed = disease::outbreak_exposure_seed(
            character_id,
            &format!("standing-cut-{token}-{disease_id:?}"),
        );
        if disease::acquisition_succeeds(seed, definition, immunity, 0.0, residual) {
            ctx.db.infection_episode().insert(InfectionEpisodeRow {
                id: 0,
                character_id,
                disease_id: format!("{disease_id:?}").to_ascii_lowercase(),
                contracted_at,
                treated_at: None,
            });
        }
    }
    Ok(())
}

/// Create or reset the diagnostic party used by the default development seed.
/// It includes a healthy physician and patients with staggered disease ages.
pub(crate) fn seed_sick_character(ctx: &ReducerContext) -> Result<(), String> {
    const SICK_CHARACTER_ID: u64 = 9_999_999_999_999_998;
    const PHYSICIAN_ID: u64 = 9_999_999_999_999_997;
    const AMBIGUOUS_PHYSICIAN_ID: u64 = 9_999_999_999_999_989;
    const DAY: u64 = 1_440;
    const FIXTURE_NOW: u64 = 60 * DAY;
    const PATIENTS: [(u64, &str, DiseaseId, u64); 8] = [
        (
            SICK_CHARACTER_ID,
            "Sick Demo",
            DiseaseId::Influenza,
            2 * DAY,
        ),
        (
            9_999_999_999_999_996,
            "Patient B",
            DiseaseId::Dysentery,
            3 * DAY,
        ),
        (
            9_999_999_999_999_995,
            "Patient C",
            DiseaseId::Typhus,
            8 * DAY,
        ),
        (
            9_999_999_999_999_994,
            "Patient D",
            DiseaseId::Tetanus,
            10 * DAY,
        ),
        (
            9_999_999_999_999_993,
            "Patient E",
            DiseaseId::Erysipelas,
            5 * DAY,
        ),
        (
            9_999_999_999_999_992,
            "Patient F",
            DiseaseId::Smallpox,
            12 * DAY,
        ),
        (
            9_999_999_999_999_991,
            "Patient G",
            DiseaseId::Plague,
            6 * DAY,
        ),
        (
            9_999_999_999_999_990,
            "Patient H",
            DiseaseId::Consumption,
            50 * DAY,
        ),
    ];

    for (id, name) in [
        (PHYSICIAN_ID, "Physician Demo"),
        (AMBIGUOUS_PHYSICIAN_ID, "Physician Demo (Medicine 3)"),
    ]
    .into_iter()
    .chain(PATIENTS.iter().map(|(id, name, _, _)| (*id, *name)))
    {
        if ctx.db.character().id().find(id).is_none() {
            crate::character::insert_new_character(ctx, name.into(), id, false)?;
        }
        let mut character_time = ctx
            .db
            .character_time()
            .character_id()
            .find(id)
            .ok_or_else(|| format!("{name} is missing time data"))?;
        character_time.minutes = character_time.minutes.max(FIXTURE_NOW);
        ctx.db
            .character_time()
            .character_id()
            .update(character_time);
    }

    let fixture_ids = [PHYSICIAN_ID, AMBIGUOUS_PHYSICIAN_ID]
        .into_iter()
        .chain(PATIENTS.iter().map(|(id, _, _, _)| *id))
        .collect::<Vec<_>>();
    for examination in ctx
        .db
        .medical_examination()
        .iter()
        .filter(|exam| {
            fixture_ids.contains(&exam.doctor_id) || fixture_ids.contains(&exam.target_id)
        })
        .collect::<Vec<_>>()
    {
        ctx.db.medical_examination().id().delete(examination.id);
    }
    for medication in ctx
        .db
        .equipped_medication()
        .iter()
        .filter(|medication| fixture_ids.contains(&medication.character_id))
        .collect::<Vec<_>>()
    {
        ctx.db
            .equipped_medication()
            .inventory_item_id()
            .delete(medication.inventory_item_id);
    }

    for (id, _, _, _) in PATIENTS.iter().skip(1) {
        crate::strategic::attach_seeded_party_member(ctx, SICK_CHARACTER_ID, *id, "Patient")?;
    }
    crate::strategic::attach_seeded_party_member(
        ctx,
        SICK_CHARACTER_ID,
        PHYSICIAN_ID,
        "Physician",
    )?;
    crate::strategic::attach_seeded_party_member(
        ctx,
        SICK_CHARACTER_ID,
        AMBIGUOUS_PHYSICIAN_ID,
        "Physician",
    )?;

    let mut physician_skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(PHYSICIAN_ID)
        .ok_or_else(|| "Physician Demo is missing skill data".to_string())?;
    physician_skills.medicine_hours = 1_000_000.0;
    physician_skills.surgeon_hours = 1_000_000.0;
    ctx.db
        .character_skills()
        .character_id()
        .update(physician_skills);
    let mut physician_attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(PHYSICIAN_ID)
        .ok_or_else(|| "Physician Demo is missing attributes".to_string())?;
    physician_attributes.intelligence = 5.0;
    physician_attributes.instinct = 5.0;
    ctx.db
        .character_attributes()
        .character_id()
        .update(physician_attributes);

    let mut ambiguous_physician_skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(AMBIGUOUS_PHYSICIAN_ID)
        .ok_or_else(|| "Medicine 3 physician is missing skill data".to_string())?;
    ambiguous_physician_skills.medicine_hours = 7_500.0;
    ctx.db
        .character_skills()
        .character_id()
        .update(ambiguous_physician_skills);

    for (id, _, disease_id, age) in PATIENTS {
        for episode in ctx
            .db
            .infection_episode()
            .character_id()
            .filter(id)
            .collect::<Vec<_>>()
        {
            ctx.db.infection_episode().id().delete(episode.id);
        }
        ctx.db.infection_episode().insert(InfectionEpisodeRow {
            id: 0,
            character_id: id,
            disease_id: format!("{disease_id:?}").to_ascii_lowercase(),
            contracted_at: FIXTURE_NOW - age,
            treated_at: None,
        });
        crate::capability::refresh_character_capability(ctx, id)?;
    }
    for recipe in disease::MEDICATION_RECIPES {
        for ingredient in recipe.ingredients {
            crate::add_inventory_item(ctx, PHYSICIAN_ID, ingredient.item_id, ingredient.quantity);
        }
    }
    let patient_h = 9_999_999_999_999_990;
    let medication_id = crate::add_inventory_item(
        ctx,
        patient_h,
        disease::medication_recipe(DiseaseId::Consumption).item_id,
        1,
    )
    .ok_or("Could not seed Patient H medication")?;
    equip_medication(ctx, patient_h, medication_id)?;
    crate::capability::refresh_character_capability(ctx, PHYSICIAN_ID)?;
    crate::capability::refresh_character_capability(ctx, AMBIGUOUS_PHYSICIAN_ID)?;
    crate::filth::seed_demo(ctx, SICK_CHARACTER_ID, 9_999_999_999_999_996);
    Ok(())
}

const EXAMINATION_MINUTES: u64 = 15;

#[reducer]
pub fn examine_by_herbalist(
    ctx: &ReducerContext,
    patient_id: u64,
    settlement_id: String,
) -> Result<(), String> {
    let patient = crate::require_living_character(ctx, patient_id)?;
    if patient.current_settlement_id.as_deref() != Some(&settlement_id) {
        return Err("Patient must be at this herbalist's settlement".into());
    }
    let herbalist = ensure_settlement_herbalist(ctx, &settlement_id);
    crate::strategic::consume_personal_gold(
        ctx,
        patient_id,
        u64::from(adventuresim_core::strategic_economy::NPC_HERBALIST_EXAM_FEE),
    )?;

    for prior in ctx
        .db
        .herbalist_examination()
        .patient_id()
        .filter(patient_id)
        .collect::<Vec<_>>()
    {
        ctx.db.herbalist_examination().id().delete(prior.id);
    }

    // Reuse medical advancement so the patient's disease clock and capability
    // update without applying travel hunger, thirst, fatigue, or observance.
    if !advance_medical_participants(ctx, patient_id, patient_id, EXAMINATION_MINUTES)? {
        ctx.db.herbalist_examination().insert(HerbalistExamination {
            id: 0,
            patient_id,
            settlement_id,
            disease_names: Vec::new(),
            medication_names: Vec::new(),
        });
        return Ok(());
    }

    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(patient_id)
        .ok_or("Patient time not found")?
        .minutes;
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(patient_id)
        .ok_or("Patient attributes not found")?
        .immunity;
    let mut disease_names = Vec::new();
    let mut medication_names = Vec::new();
    for infection in character_episodes(ctx, patient_id)? {
        let state = disease::evaluate(infection, now, immunity);
        if matches!(
            state.stage,
            disease::DiseaseStage::Incubating | disease::DiseaseStage::Resolved
        ) || f32::from(herbalist.medicine_skill) < state.diagnosis_dc
        {
            continue;
        }
        let (disease_name, medication_name) = disease::herbalist_diagnosis(infection.disease_id);
        if !disease_names.iter().any(|name| name == disease_name) {
            disease_names.push(disease_name.to_owned());
            medication_names.push(medication_name.to_owned());
        }
    }
    ctx.db.herbalist_examination().insert(HerbalistExamination {
        id: 0,
        patient_id,
        settlement_id,
        disease_names,
        medication_names,
    });
    Ok(())
}

#[reducer]
pub fn dismiss_herbalist_examination(
    ctx: &ReducerContext,
    patient_id: u64,
    examination_id: u64,
) -> Result<(), String> {
    let result = ctx
        .db
        .herbalist_examination()
        .id()
        .find(examination_id)
        .ok_or("Herbalist examination result is no longer available")?;
    if result.patient_id != patient_id {
        return Err("This herbalist examination belongs to another patient".into());
    }
    ctx.db.herbalist_examination().id().delete(examination_id);
    Ok(())
}

/// A typed herbalist-only purchasing path. Medication remains forbidden to
/// generic merchants, and every course is inserted as an individual personal
/// inventory row so it can be equipped independently.
#[reducer]
pub fn purchase_from_herbalist(
    ctx: &ReducerContext,
    patient_id: u64,
    settlement_id: String,
    item_ids: Vec<String>,
    quantities: Vec<u32>,
) -> Result<(), String> {
    let patient = crate::require_living_character(ctx, patient_id)?;
    if patient.current_settlement_id.as_deref() != Some(&settlement_id) {
        return Err("Patient must be at this herbalist's settlement".into());
    }
    if item_ids.len() != quantities.len() || item_ids.is_empty() {
        return Err("Herbalist purchase entries must be aligned".into());
    }
    ensure_settlement_herbalist(ctx, &settlement_id);

    let mut cost = 0u64;
    for (item_id, quantity) in item_ids.iter().zip(&quantities) {
        if *quantity == 0 {
            return Err("Herbalist purchase quantities must be positive".into());
        }
        let definition = ctx
            .db
            .item()
            .id()
            .find(item_id)
            .ok_or("Herbalist item not found")?;
        let unit_price = match definition.kind {
            crate::ItemKind::Ingredient => {
                adventuresim_core::strategic_economy::merchant_buy_price(
                    definition.base_value.unwrap_or(1),
                )
            }
            crate::ItemKind::Medication => {
                let recipe = disease::medication_recipe_for_item(item_id)
                    .ok_or("Unknown prepared medication")?;
                adventuresim_core::strategic_economy::herbalist_medication_price(recipe)
            }
            _ => return Err("The herbalist sells only ingredients and prepared medication".into()),
        };
        cost = cost.saturating_add(u64::from(unit_price) * u64::from(*quantity));
    }
    crate::strategic::consume_personal_gold(ctx, patient_id, cost)?;
    for (item_id, quantity) in item_ids.iter().zip(&quantities) {
        crate::add_inventory_item(ctx, patient_id, item_id, *quantity);
    }
    Ok(())
}

fn advance_medical_participants(
    ctx: &ReducerContext,
    doctor_id: u64,
    target_id: u64,
    requested_minutes: u64,
) -> Result<bool, String> {
    let participants = if doctor_id == target_id {
        vec![doctor_id]
    } else {
        vec![doctor_id, target_id]
    };
    let elapsed = participants
        .iter()
        .try_fold(requested_minutes, |limit, character_id| {
            let disease = preview_elapsed_for_disease(ctx, *character_id, limit)?;
            let injury =
                crate::surgery::preview_elapsed_for_injuries(ctx, *character_id, limit, true)?;
            Ok::<u64, String>(limit.min(disease).min(injury))
        })?;
    let mut completed = elapsed == requested_minutes;
    for character_id in participants {
        completed &= crate::time::advance_character_wait_time(ctx, character_id, elapsed)?;
    }
    Ok(completed)
}

#[reducer]
pub fn examine_patient(ctx: &ReducerContext, doctor_id: u64, target_id: u64) -> Result<(), String> {
    // Character selection is enforced by strategic-web's session POST
    // boundary, matching the rest of the strategic reducer surface.
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
        return Err("A doctor may examine only themselves or a member of their party".into());
    }
    let medicine = ctx
        .db
        .character_capability()
        .character_id()
        .find(doctor_id)
        .ok_or("Doctor capability not found")?
        .medicine;
    if medicine < disease::MEDICINE_VITALS_THRESHOLD {
        return Err("Medicine 2 is required to examine a patient".into());
    }
    if !advance_medical_participants(ctx, doctor_id, target_id, EXAMINATION_MINUTES)? {
        return Ok(());
    }
    let target_minute = ctx
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
    let blood = ctx
        .db
        .character_condition()
        .character_id()
        .find(target_id)
        .map_or(1.0, |condition| {
            if condition.maximum_blood_ml > 0.0 {
                condition.current_blood_ml / condition.maximum_blood_ml
            } else {
                0.0
            }
        });
    let episodes = character_episodes(ctx, target_id)?;
    let (_, vitals, _, _) = disease::combined_state(&episodes, target_minute, immunity);
    let findings = disease::observed_symptoms(&episodes, target_minute, immunity);
    let mut possible = Vec::new();
    let mut confirmed_ids = Vec::new();
    let mut confirmed_diseases = Vec::new();
    let mut confirmed_stages = Vec::new();
    for infection in &episodes {
        let state = disease::evaluate(*infection, target_minute, immunity);
        if matches!(
            state.stage,
            disease::DiseaseStage::Incubating | disease::DiseaseStage::Resolved
        ) {
            continue;
        }
        if medicine >= state.diagnosis_dc {
            confirmed_ids.push(infection.id);
            confirmed_diseases.push(format!("{:?}", infection.disease_id).to_ascii_lowercase());
            confirmed_stages.push(stage_key(state.stage).into());
        } else if medicine >= state.diagnosis_dc - 1.0 {
            for candidate in disease::differential_candidates(&findings, infection.disease_id) {
                let id = format!("{candidate:?}").to_ascii_lowercase();
                if !possible.contains(&id) {
                    possible.push(id);
                }
            }
        }
    }
    possible.retain(|candidate| !confirmed_diseases.contains(candidate));
    for pending in ctx
        .db
        .medical_examination()
        .doctor_id()
        .filter(doctor_id)
        .filter(|exam| exam.target_id == target_id)
        .collect::<Vec<_>>()
    {
        ctx.db.medical_examination().id().delete(pending.id);
    }
    ctx.db.medical_examination().insert(MedicalExamination {
        id: 0,
        doctor_id,
        target_id,
        examined_at: target_minute,
        findings: findings
            .into_iter()
            .map(|finding| finding.period_label().into())
            .collect(),
        reveals_vitals: medicine >= disease::MEDICINE_VITALS_THRESHOLD,
        sanguine: (blood.clamp(0.0, 1.0) - vitals.sanguine).clamp(0.0, 1.0),
        phlegmatic: (1.0 - vitals.phlegmatic).clamp(0.0, 1.0),
        choleric: (1.0 - vitals.choleric).clamp(0.0, 1.0),
        melancholic: (1.0 - vitals.melancholic).clamp(0.0, 1.0),
        possible_disease_ids: possible,
        confirmed_infection_ids: confirmed_ids,
        confirmed_disease_ids: confirmed_diseases,
        confirmed_stages,
    });
    Ok(())
}

#[reducer]
pub fn dismiss_medical_examination(
    ctx: &ReducerContext,
    doctor_id: u64,
    target_id: u64,
    examination_id: u64,
) -> Result<(), String> {
    let examination = ctx
        .db
        .medical_examination()
        .id()
        .find(examination_id)
        .ok_or("Examination result is no longer available")?;
    if examination.doctor_id != doctor_id || examination.target_id != target_id {
        return Err("This examination result belongs to another doctor or patient".into());
    }
    ctx.db.medical_examination().id().delete(examination_id);
    Ok(())
}
