//! Private generated-outbreak authority and patient materialization.
//!
//! Canonical disease, source and remediation facts never cross a public view.

use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, table, view};

use crate::{
    character::{character, character__view},
    corpse::strategic_corpse,
    local_problem::{local_problem_receipt, local_problem_receipt__view},
    settlement_population::{settlement_npc, settlement_npc__view},
    strategic::strategic_gateway_authority__view,
    time::{character_time, character_time__view},
};

const MAX_OUTBREAK_PATIENTS: usize = 8;

#[derive(Clone, Debug)]
#[table(accessor = outbreak_authority)]
pub struct OutbreakAuthority {
    #[primary_key]
    pub case_id: String,
    #[unique]
    pub problem_id: String,
    #[index(btree)]
    pub settlement_id: String,
    pub disease_id: String,
    pub transmission_route: String,
    pub source_kind: String,
    pub source_json: String,
    pub physical_source_site_id: String,
    pub patient_presentation_site_id: String,
    pub responsible_npc_id: Option<String>,
    pub culpability: Option<String>,
    pub carrier_threat_id: Option<String>,
    pub chronology_json: String,
    pub remediation_id: String,
    pub remediation_json: String,
    pub remediated_at: Option<u64>,
    pub remediated_by_party_id: Option<String>,
    pub remediation_source_id: Option<String>,
}

#[derive(Clone, Debug)]
#[table(accessor = outbreak_patient_authority)]
pub struct OutbreakPatientAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub case_id: String,
    #[index(btree)]
    pub presentation_npc_id: String,
    pub family_npc_id: Option<String>,
    pub patient_key: u64,
    #[unique]
    pub episode_id: u64,
    pub disease_id: String,
    pub immunity_milli: u16,
    pub ruleset_version: u16,
    pub phenotype_key_version: u16,
    pub exposed_at: u64,
    pub became_symptomatic_at: u64,
    pub died_at: Option<u64>,
    pub death_kind: Option<String>,
    pub terminal_failure: Option<String>,
    pub corpse_id: Option<String>,
    pub autopsy_evidence_id: Option<String>,
}

#[derive(Clone, Debug)]
#[table(accessor = outbreak_patient_examination)]
pub struct OutbreakPatientExamination {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub observer_character_id: u64,
    pub patient_ref: String,
    pub finding: String,
    pub examined_at: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = outbreak_source_presence_span)]
pub struct OutbreakSourcePresenceSpan {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    #[index(btree)]
    pub source_site_id: String,
    pub started_at: u64,
    pub ended_at: Option<u64>,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendOutbreakPatient {
    pub owner_character_id: u64,
    pub patient_ref: String,
    pub presentation_npc_id: String,
    pub display_name: String,
    pub case_id: String,
    pub source_site_id: String,
    pub alive: bool,
    pub symptomatic: bool,
    pub findings: Vec<String>,
}

fn patient_episode(
    patient: &OutbreakPatientAuthority,
) -> Result<adventuresim_core::disease::InfectionEpisode, String> {
    if patient.ruleset_version != adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION
        || patient.phenotype_key_version != adventuresim_core::physiology::PHENOTYPE_KEY_VERSION
    {
        return Err("Outbreak patient uses an unsupported physiology ruleset".into());
    }
    Ok(adventuresim_core::disease::InfectionEpisode {
        id: patient.episode_id,
        character_id: patient.patient_key,
        disease_id: crate::disease::parse_id(&patient.disease_id)?,
        contracted_at: patient.exposed_at,
        ruleset_version: patient.ruleset_version,
        phenotype_key_version: patient.phenotype_key_version,
    })
}

fn patient_alive_at(patient: &OutbreakPatientAuthority, minute: u64) -> Result<bool, String> {
    patient_episode(patient)?;
    Ok(patient.died_at.is_none_or(|death| minute < death))
}

/// Observer-scoped living-patient portraits. The gateway receives only safe
/// presentation state after the owning character has a normal rumor receipt.
#[view(accessor = backend_outbreak_patients, public)]
pub fn backend_outbreak_patients(ctx: &ViewContext) -> Vec<BackendOutbreakPatient> {
    let trusted = ctx
        .db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|authority| authority.identity == ctx.sender());
    if !trusted {
        return Vec::new();
    }
    let mut result = Vec::new();
    for patient in ctx
        .db
        .outbreak_patient_authority()
        .gateway_bucket()
        .filter(0u8)
    {
        let Some(authority) = ctx.db.outbreak_authority().case_id().find(&patient.case_id) else {
            continue;
        };
        let Some(npc) = ctx
            .db
            .settlement_npc()
            .id()
            .find(&patient.presentation_npc_id)
        else {
            continue;
        };
        for receipt in ctx
            .db
            .local_problem_receipt()
            .settlement_id()
            .filter(&authority.settlement_id)
            .filter(|receipt| receipt.problem_id == authority.problem_id)
        {
            let Some(actor) = ctx
                .db
                .character()
                .id()
                .find(receipt.character_id)
                .filter(|actor| actor.alive)
            else {
                continue;
            };
            let Some(actor_time) = ctx.db.character_time().character_id().find(actor.id) else {
                continue;
            };
            let at = patient
                .died_at
                .map_or(actor_time.minutes, |death| actor_time.minutes.min(death));
            let Ok(episode) = patient_episode(&patient) else {
                continue;
            };
            let state = adventuresim_core::disease::evaluate(
                episode,
                at,
                f32::from(patient.immunity_milli) / 1_000.0,
            );
            let alive = patient_alive_at(&patient, actor_time.minutes).unwrap_or(false);
            let findings = ctx
                .db
                .outbreak_patient_examination()
                .observer_character_id()
                .filter(actor.id)
                .filter(|finding| finding.patient_ref == patient.id)
                .map(|finding| finding.finding)
                .collect();
            result.push(BackendOutbreakPatient {
                owner_character_id: actor.id,
                patient_ref: patient.id.clone(),
                presentation_npc_id: patient.presentation_npc_id.clone(),
                display_name: npc.name.clone(),
                case_id: authority.case_id.clone(),
                source_site_id: authority.patient_presentation_site_id.clone(),
                alive,
                symptomatic: !state.symptoms.is_empty(),
                findings,
            });
        }
    }
    result
}

#[spacetimedb::reducer]
pub fn examine_outbreak_patient(
    ctx: &ReducerContext,
    actor_id: u64,
    patient_ref: String,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    crate::strategic::require_strategic_character_authority(ctx, actor_id)?;
    let patient = ctx
        .db
        .outbreak_patient_authority()
        .id()
        .find(&patient_ref)
        .ok_or("Outbreak patient not found")?;
    let authority = ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&patient.case_id)
        .ok_or("Outbreak authority not found")?;
    if !ctx
        .db
        .local_problem_receipt()
        .character_id()
        .filter(actor_id)
        .any(|receipt| receipt.problem_id == authority.problem_id)
    {
        return Err("This patient has not been legitimately discovered".into());
    }
    let actor = crate::character::require_living_character(ctx, actor_id)?;
    if actor.current_settlement_id.as_deref() != Some(authority.settlement_id.as_str()) {
        return Err("Physician and patient must be in the same settlement".into());
    }
    if crate::investigation::character_case_site_id(ctx, actor_id).as_deref()
        != Some(authority.patient_presentation_site_id.as_str())
    {
        return Err("Travel to the patient's case site before examining them".into());
    }
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(actor_id)
        .map(|row| row.minutes)
        .ok_or("Character time not found")?;
    if !patient_alive_at(&patient, minute)? {
        return Err("Use the corpse examination interface for a dead patient".into());
    }
    let receipt_id = format!("outbreak-patient-exam:{actor_id}:{patient_ref}");
    if ctx
        .db
        .outbreak_patient_examination()
        .id()
        .find(&receipt_id)
        .is_some()
    {
        return Ok(());
    }
    if !crate::time::advance_investigation_time(ctx, actor_id, 20)? {
        return Err("Actor could not complete the patient examination".into());
    }
    let check = crate::condition::mental_check(
        ctx,
        actor_id,
        adventuresim_core::prelude::Skill::Physiology,
    )?;
    let episode = patient_episode(&patient)?;
    let symptoms = adventuresim_core::disease::observed_symptoms(
        &[episode],
        minute,
        f32::from(patient.immunity_milli) / 1_000.0,
    );
    let finding = if check >= 2.0 && !symptoms.is_empty() {
        format!(
            "Physiology records a changing systemic pattern with {} observable sign(s); it does not by itself identify the disease or source.",
            symptoms.len()
        )
    } else {
        "The examination does not support a defensible physiological pattern yet.".into()
    };
    ctx.db
        .outbreak_patient_examination()
        .insert(OutbreakPatientExamination {
            id: receipt_id,
            observer_character_id: actor_id,
            patient_ref,
            finding,
            examined_at: minute.saturating_add(20),
        });
    Ok(())
}

fn materialize_patient_corpse(
    ctx: &ReducerContext,
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    exposure: &adventuresim_core::quest_generation::OutbreakExposure,
    display_name: &str,
    settlement_id: &str,
    death_minute: u64,
) -> Result<String, String> {
    use adventuresim_core::{
        autopsy::{PostCombatBody, SystemicPathologySnapshot, post_combat_body},
        disease::{InfectionEpisode, Symptom, combined_state},
        physiology::Meter,
        quest_generation::OutbreakPatientDeathKind,
    };
    let outbreak = generated
        .outbreak
        .as_ref()
        .ok_or("Outbreak truth missing")?;
    let corpse_id = format!(
        "corpse:outbreak:{}:{}",
        generated.canonical_case_id, exposure.patient_ref
    );
    if ctx.db.strategic_corpse().id().find(&corpse_id).is_some() {
        return Ok(corpse_id);
    }
    let body = match exposure.death_kind {
        Some(OutbreakPatientDeathKind::CarrierAttack) => {
            let threat = outbreak
                .carrier_threat
                .ok_or("Carrier attack death has no modeled threat")?;
            let attacker = crate::strategic::autoresolve_enemy(
                exposure.patient_key.wrapping_sub(1),
                threat.as_str(),
                12,
                10_000,
            )?;
            let victim =
                crate::strategic::autoresolve_enemy(exposure.patient_key, "poacher", 1, 10_000)?;
            let outcome = adventuresim_core::autopsy::resolve_death_required_incident(
                &[attacker],
                &[victim],
                exposure.patient_key,
                generated.generation_seed ^ exposure.patient_key,
                128,
            )
            .ok_or("Carrier autoresolve could not produce its required attack death")?;
            let victim = outcome
                .enemies
                .iter()
                .find(|enemy| enemy.id == exposure.patient_key)
                .ok_or("Carrier autoresolve omitted its victim")?;
            post_combat_body(victim, &outcome.log)
        }
        Some(OutbreakPatientDeathKind::Disease) => PostCombatBody {
            combatant_id: exposure.patient_key,
            health: [1.0; 7],
            blood_loss_fraction: 0.0,
            injuries: Vec::new(),
        },
        None => return Err("Living outbreak patient cannot materialize a corpse".into()),
    };
    crate::corpse::persist_body(
        ctx,
        crate::corpse::StrategicCorpse {
            id: corpse_id.clone(),
            source_id: format!("outbreak-victim:{}", generated.canonical_case_id),
            discovering_party_id: String::new(),
            subject_character_id: None,
            display_name: display_name.into(),
            creature_kind: "human".into(),
            settlement_id: settlement_id.into(),
            case_site_id: outbreak.physical_source_site.0.clone(),
            death_minute,
            discovered_minute: death_minute,
            buried: false,
            exhumed: false,
            burned: false,
            party_killed_enemy: false,
            handling_damage_bps: 0,
            opened: false,
            opening_quality_bps: 0,
            opening_obscuration_bps: 0,
            revision: 0,
        },
        body,
    )?;
    let episode = InfectionEpisode {
        id: exposure.episode_id,
        character_id: exposure.patient_key,
        disease_id: outbreak.disease,
        contracted_at: exposure.exposed_at,
        ruleset_version: adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION,
        phenotype_key_version: exposure.phenotype_key_version,
    };
    let (_, vitals, symptoms, _) = combined_state(
        &[episode],
        death_minute,
        f32::from(exposure.immunity_milli) / 1_000.0,
    );
    let physiology_key = crate::disease::physiology_key(ctx)?;
    if physiology_key.version != exposure.phenotype_key_version {
        return Err("Patient phenotype version does not match private key material".into());
    }
    let meters = adventuresim_core::disease::private_meter_state(
        episode,
        death_minute,
        f32::from(exposure.immunity_milli) / 1_000.0,
        &physiology_key.key,
    );
    let bps = |value: f32| (value.clamp(0.0, 1.0) * 10_000.0).round() as u16;
    crate::corpse::persist_pathology_snapshot(
        ctx,
        &corpse_id,
        &SystemicPathologySnapshot {
            respiratory_bps: bps(vitals.phlegmatic.max(meters.get(Meter::Oxygenation))),
            circulatory_bps: bps(vitals.sanguine.max(meters.get(Meter::Perfusion))),
            homeostatic_bps: bps(vitals
                .choleric
                .max(meters.get(Meter::Hydration))
                .max(meters.get(Meter::Temperature))
                .max(meters.get(Meter::Inflammation))),
            neurologic_bps: bps(vitals.melancholic.max(meters.get(Meter::Neurologic))),
            feverish: symptoms.contains(&Symptom::Feverish),
            air_hunger: symptoms.contains(&Symptom::AirHunger),
            wasting: symptoms.contains(&Symptom::Wasting),
        },
    )?;
    if let Some(family_npc_id) = &exposure.family_npc_id {
        crate::corpse::materialize_corpse_family_bindings(
            ctx,
            &corpse_id,
            settlement_id,
            std::slice::from_ref(family_npc_id),
        )?;
    }
    Ok(corpse_id)
}

pub(crate) fn remediation_id(
    generated: &adventuresim_core::quest_generation::GeneratedCase,
) -> Result<String, String> {
    generated
        .objectives
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
        .find_map(|objective| match &objective.requirement {
            adventuresim_core::case::ObjectiveRequirement::RemediateSource { remediation_id } => {
                Some(remediation_id.clone())
            }
            _ => None,
        })
        .ok_or("Outbreak has no exact remediation objective".into())
}

pub(crate) fn materialize_generated_outbreak(
    ctx: &ReducerContext,
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    settlement_id: &str,
    now_minute: u64,
) -> Result<(), String> {
    use adventuresim_core::quest_generation::OutbreakSource;

    let Some(outbreak) = &generated.outbreak else {
        return Ok(());
    };
    if outbreak.exposure_chronology.len() > MAX_OUTBREAK_PATIENTS {
        return Err("Generated outbreak exceeds bounded patient materialization".into());
    }
    let exact_remediation_id = remediation_id(generated)?;
    let (source_kind, carrier) = match &outbreak.source {
        OutbreakSource::Sanitation { .. } => ("sanitation", None),
        OutbreakSource::Behavior { .. } => ("behavior", None),
        OutbreakSource::ThreatVector { threat } => ("threat_vector", Some(threat.as_str())),
        OutbreakSource::Environmental { .. } => ("environmental", None),
    };
    let responsible = outbreak.responsible_npc.as_ref();
    if ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&generated.canonical_case_id)
        .is_some()
        || ctx
            .db
            .outbreak_authority()
            .problem_id()
            .find(&generated.problem_id)
            .is_some()
    {
        return Err("Generated outbreak authority ID collision".into());
    }
    ctx.db.outbreak_authority().insert(OutbreakAuthority {
        case_id: generated.canonical_case_id.clone(),
        problem_id: generated.problem_id.clone(),
        settlement_id: settlement_id.into(),
        disease_id: crate::disease::disease_key(outbreak.disease).into(),
        transmission_route: format!("{:?}", outbreak.transmission_route).to_ascii_lowercase(),
        source_kind: source_kind.into(),
        source_json: serde_json::to_string(&outbreak.source)
            .map_err(|_| "Could not encode outbreak source")?,
        physical_source_site_id: outbreak.physical_source_site.0.clone(),
        patient_presentation_site_id: outbreak.patient_presentation_site.0.clone(),
        responsible_npc_id: responsible.map(|value| value.npc_id.clone()),
        culpability: responsible
            .map(|value| format!("{:?}", value.culpability).to_ascii_lowercase()),
        carrier_threat_id: carrier.map(str::to_owned),
        chronology_json: serde_json::to_string(&outbreak.exposure_chronology)
            .map_err(|_| "Could not encode outbreak chronology")?,
        remediation_id: exact_remediation_id,
        remediation_json: serde_json::to_string(&outbreak.remediation)
            .map_err(|_| "Could not encode outbreak remediation")?,
        remediated_at: None,
        remediated_by_party_id: None,
        remediation_source_id: None,
    });

    for exposure in &outbreak.exposure_chronology {
        if ctx
            .db
            .outbreak_patient_authority()
            .id()
            .find(&exposure.patient_ref)
            .is_some()
            || ctx
                .db
                .outbreak_patient_authority()
                .episode_id()
                .find(exposure.episode_id)
                .is_some()
        {
            return Err(format!(
                "Generated outbreak patient ID collision: {}",
                exposure.patient_ref
            ));
        }
        let npc = ctx
            .db
            .settlement_npc()
            .id()
            .find(&exposure.presentation_npc_id)
            .ok_or("Outbreak patient NPC no longer exists")?;
        if npc.home_settlement_id != settlement_id {
            return Err("Outbreak presentation NPC is not local to its patient".into());
        }
        if let Some(family_npc_id) = &exposure.family_npc_id {
            let family = ctx
                .db
                .settlement_npc()
                .id()
                .find(family_npc_id)
                .ok_or("Explicit outbreak family NPC no longer exists")?;
            if family.home_settlement_id != settlement_id {
                return Err("Explicit outbreak family NPC is not local".into());
            }
        }
        let episode = patient_episode(&OutbreakPatientAuthority {
            id: exposure.patient_ref.clone(),
            gateway_bucket: 0,
            case_id: generated.canonical_case_id.clone(),
            presentation_npc_id: exposure.presentation_npc_id.clone(),
            family_npc_id: exposure.family_npc_id.clone(),
            patient_key: exposure.patient_key,
            episode_id: exposure.episode_id,
            disease_id: crate::disease::disease_key(outbreak.disease).into(),
            immunity_milli: exposure.immunity_milli,
            ruleset_version: adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION,
            phenotype_key_version: exposure.phenotype_key_version,
            exposed_at: exposure.exposed_at,
            became_symptomatic_at: exposure.became_symptomatic_at,
            died_at: None,
            death_kind: None,
            terminal_failure: None,
            corpse_id: None,
            autopsy_evidence_id: None,
        })?;
        let definition = adventuresim_core::disease::definition(outbreak.disease);
        let course_end = exposure
            .exposed_at
            .saturating_add(definition.incubation_minutes)
            .saturating_add(definition.rise_minutes)
            .saturating_add(definition.peak_minutes)
            .saturating_add(definition.recovery_minutes);
        let private_terminal = crate::disease::first_private_terminal(
            ctx,
            exposure.patient_key,
            &[episode],
            exposure.exposed_at,
            course_end,
            f32::from(exposure.immunity_milli) / 1_000.0,
        )?;
        let mut resolved_exposure = exposure.clone();
        match exposure.death_kind {
            Some(adventuresim_core::quest_generation::OutbreakPatientDeathKind::Disease) => {
                resolved_exposure.died_at = private_terminal.map(|value| value.0);
                resolved_exposure.death_kind = private_terminal.map(|_| {
                    adventuresim_core::quest_generation::OutbreakPatientDeathKind::Disease
                });
                resolved_exposure.terminal_failure = private_terminal.map(|value| value.1);
            }
            Some(adventuresim_core::quest_generation::OutbreakPatientDeathKind::CarrierAttack) => {
                let latest_attack = private_terminal
                    .map(|(terminal_at, _)| terminal_at.saturating_sub(1))
                    .unwrap_or(now_minute)
                    .min(now_minute);
                let attack_at = exposure
                    .died_at
                    .unwrap_or(latest_attack)
                    .min(latest_attack)
                    .max(exposure.became_symptomatic_at);
                if attack_at <= latest_attack {
                    resolved_exposure.died_at = Some(attack_at);
                    resolved_exposure.terminal_failure = None;
                } else {
                    resolved_exposure.died_at = None;
                    resolved_exposure.death_kind = None;
                    resolved_exposure.terminal_failure = None;
                }
            }
            None => {}
        }
        let row_id = resolved_exposure.patient_ref.clone();
        let corpse_id = resolved_exposure
            .died_at
            .filter(|death_minute| *death_minute <= now_minute)
            .map(|death_minute| {
                materialize_patient_corpse(
                    ctx,
                    generated,
                    &resolved_exposure,
                    &npc.name,
                    settlement_id,
                    death_minute,
                )
            })
            .transpose()?;
        let autopsy_evidence_id = corpse_id.as_ref().and_then(|_| {
            generated
                .evidence
                .iter()
                .find(|evidence| {
                    evidence.kind
                        == adventuresim_core::quest_generation::EvidenceKind::BloodlessCorpse
                })
                .map(|evidence| evidence.id.0.clone())
        });
        ctx.db
            .outbreak_patient_authority()
            .insert(OutbreakPatientAuthority {
                id: row_id,
                gateway_bucket: 0,
                case_id: generated.canonical_case_id.clone(),
                presentation_npc_id: exposure.presentation_npc_id.clone(),
                family_npc_id: exposure.family_npc_id.clone(),
                patient_key: exposure.patient_key,
                episode_id: exposure.episode_id,
                disease_id: crate::disease::disease_key(outbreak.disease).into(),
                immunity_milli: exposure.immunity_milli,
                ruleset_version: adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION,
                phenotype_key_version: exposure.phenotype_key_version,
                exposed_at: exposure.exposed_at,
                became_symptomatic_at: exposure.became_symptomatic_at,
                died_at: resolved_exposure.died_at,
                death_kind: resolved_exposure
                    .death_kind
                    .map(|value| format!("{value:?}").to_ascii_lowercase()),
                terminal_failure: resolved_exposure
                    .terminal_failure
                    .map(|value| format!("{value:?}").to_ascii_lowercase()),
                corpse_id,
                autopsy_evidence_id,
            });
    }
    Ok(())
}

pub(crate) fn commit_source_remediation(
    ctx: &ReducerContext,
    case_id: &str,
    party_id: &str,
    source_id: &str,
    remediation_id: &str,
    source_site_id: &str,
    at_minute: u64,
) -> Result<(), String> {
    let mut authority = ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&case_id.to_owned())
        .ok_or("Outbreak authority not found")?;
    if authority.remediation_id != remediation_id
        || authority.physical_source_site_id != source_site_id
    {
        return Err("Intervention does not match the authoritative outbreak source".into());
    }
    if authority.source_kind == "threat_vector" {
        return Err("A carrier outbreak must be remediated through its hostile outcome".into());
    }
    if authority.remediated_at.is_some() {
        return if authority.remediation_source_id.as_deref() == Some(source_id)
            && authority.remediated_by_party_id.as_deref() == Some(party_id)
        {
            Ok(())
        } else {
            Err("Outbreak source was already remediated by different authority".into())
        };
    }
    authority.remediated_at = Some(at_minute);
    authority.remediated_by_party_id = Some(party_id.into());
    authority.remediation_source_id = Some(source_id.into());
    ctx.db.outbreak_authority().case_id().update(authority);
    Ok(())
}

pub(crate) fn commit_carrier_remediation(
    ctx: &ReducerContext,
    case_id: &str,
    party_id: &str,
    source_id: &str,
    remediation_id: &str,
    at_minute: u64,
) -> Result<(), String> {
    let mut authority = ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&case_id.to_owned())
        .ok_or("Outbreak authority not found")?;
    if authority.source_kind != "threat_vector" || authority.remediation_id != remediation_id {
        return Err("Hostile outcome does not match the authoritative carrier source".into());
    }
    if authority.remediated_at.is_some() {
        return if authority.remediation_source_id.as_deref() == Some(source_id)
            && authority.remediated_by_party_id.as_deref() == Some(party_id)
        {
            Ok(())
        } else {
            Err("Carrier source was already remediated by different authority".into())
        };
    }
    authority.remediated_at = Some(at_minute);
    authority.remediated_by_party_id = Some(party_id.into());
    authority.remediation_source_id = Some(source_id.into());
    ctx.db.outbreak_authority().case_id().update(authority);
    Ok(())
}

pub(crate) fn accepted_hostile_remediation(
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    fact: &adventuresim_core::case::OutcomeFactKind,
) -> Option<String> {
    use adventuresim_core::{
        case::OutcomeFactKind,
        quest_generation::{OutbreakCarrierOutcome, OutbreakRemediation},
    };
    let outbreak = generated.outbreak.as_ref()?;
    let OutbreakRemediation::ResolveCarrierThreat {
        hostile_group_id,
        accepted_outcomes,
    } = &outbreak.remediation
    else {
        return None;
    };
    let accepted = match fact {
        OutcomeFactKind::HostilesDefeated {
            hostile_group_id: actual,
            ..
        } if actual == hostile_group_id => {
            accepted_outcomes.contains(&OutbreakCarrierOutcome::Defeated)
        }
        OutcomeFactKind::HostilesDrivenOff {
            hostile_group_id: actual,
        } if actual == hostile_group_id => {
            accepted_outcomes.contains(&OutbreakCarrierOutcome::DrivenOff)
        }
        _ => false,
    };
    accepted.then(|| remediation_id(generated).ok()).flatten()
}

/// Scope generated disease pressure to the authority that physically produces
/// it. Community bedding/behavior affects settlement presence; reservoirs and
/// carriers require occupancy at the exact source site.
pub(crate) fn exposure_windows(
    ctx: &ReducerContext,
    problem_id: &str,
    character_id: u64,
    from: u64,
    to: u64,
) -> Vec<(String, u64, u64)> {
    let Some(authority) = ctx
        .db
        .outbreak_authority()
        .problem_id()
        .find(&problem_id.to_owned())
    else {
        return vec![(problem_id.to_owned(), from, to)];
    };
    let exposure_to = to.min(authority.remediated_at.unwrap_or(to));
    if exposure_to <= from {
        return Vec::new();
    }
    match authority.source_kind.as_str() {
        "sanitation" | "behavior" => {
            ctx.db
                .character()
                .id()
                .find(character_id)
                .map_or_else(Vec::new, |character| {
                    (character.current_settlement_id.as_deref()
                        == Some(authority.settlement_id.as_str()))
                    .then_some((problem_id.to_owned(), from, exposure_to))
                    .into_iter()
                    .collect()
                })
        }
        "environmental" | "threat_vector" => ctx
            .db
            .outbreak_source_presence_span()
            .character_id()
            .filter(character_id)
            .filter(|span| span.source_site_id == authority.physical_source_site_id)
            .filter_map(|span| {
                let low = from.max(span.started_at);
                let high = exposure_to.min(span.ended_at.unwrap_or(exposure_to));
                (low < high).then_some((span.id, low, high))
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn record_case_site_presence_transition(
    ctx: &ReducerContext,
    character_id: u64,
    destination_site_id: Option<&str>,
) {
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |row| row.minutes);
    for mut span in ctx
        .db
        .outbreak_source_presence_span()
        .character_id()
        .filter(character_id)
        .filter(|span| span.ended_at.is_none())
        .collect::<Vec<_>>()
    {
        span.ended_at = Some(minute);
        ctx.db.outbreak_source_presence_span().id().update(span);
    }
    let Some(site_id) = destination_site_id else {
        return;
    };
    if !ctx.db.outbreak_authority().iter().any(|authority| {
        authority.physical_source_site_id == site_id && authority.remediated_at.is_none()
    }) {
        return;
    }
    let id = format!("outbreak-presence:{character_id}:{site_id}:{minute}");
    if ctx
        .db
        .outbreak_source_presence_span()
        .id()
        .find(&id)
        .is_none()
    {
        ctx.db
            .outbreak_source_presence_span()
            .insert(OutbreakSourcePresenceSpan {
                id,
                character_id,
                source_site_id: site_id.into(),
                started_at: minute,
                ended_at: None,
            });
    }
}

pub(crate) fn discover_case_corpses(
    ctx: &ReducerContext,
    case_id: &str,
    character_id: u64,
    discovered_at: u64,
) -> Result<(), String> {
    if ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&case_id.to_owned())
        .is_none()
    {
        return Ok(());
    }
    let party_id = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .and_then(|character| character.party_id)
        .ok_or("Outbreak corpse discovery requires a party")?;
    for patient in ctx
        .db
        .outbreak_patient_authority()
        .case_id()
        .filter(&case_id.to_owned())
    {
        let Some(corpse_id) = patient.corpse_id else {
            continue;
        };
        let Some(mut corpse) = ctx.db.strategic_corpse().id().find(&corpse_id) else {
            return Err("Outbreak patient corpse authority is missing".into());
        };
        if corpse.discovering_party_id.is_empty() {
            corpse.discovering_party_id = party_id.clone();
            corpse.discovered_minute = discovered_at;
            ctx.db.strategic_corpse().id().update(corpse);
        } else if corpse.discovering_party_id != party_id {
            // Knowledge remains party-scoped; another party's discovery is not
            // silently transferred.
            continue;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn outbreak_authority_and_patients_are_private_and_real() {
        let source = include_str!("outbreak.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("#[table(accessor = outbreak_authority)]"));
        assert!(production.contains("#[table(accessor = outbreak_patient_authority)]"));
        assert!(!production.contains("accessor = outbreak_authority, public"));
        assert!(!production.contains("accessor = outbreak_patient_authority, public"));
        assert!(production.contains("patient_episode("));
        assert!(production.contains("backend_outbreak_patients"));
        assert!(!production.contains("insert_character_with_origin"));
        assert!(!production.contains("patient_character_id"));
        assert!(!production.contains(&["settlement_outbreak()", ".insert"].concat()));
    }

    #[test]
    fn outbreak_corpses_dogfood_autoresolve_and_generic_pathology() {
        let source = include_str!("outbreak.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("resolve_death_required_incident"));
        assert!(production.contains("post_combat_body"));
        assert!(
            production.contains("PostCombatBody {\n            combatant_id: exposure.patient_key")
        );
        assert!(production.contains("threat.as_str()"));
        assert!(production.contains("persist_body"));
        assert!(production.contains("persist_pathology_snapshot"));
        assert!(!production.contains(&["Corpse", "Injury {"].concat()));
    }

    #[test]
    fn remediation_is_exact_idempotent_and_uses_normal_outcome_authority() {
        let source = include_str!("outbreak.rs");
        assert!(source.contains("authority.remediation_id != remediation_id"));
        assert!(source.contains("authority.physical_source_site_id != source_site_id"));
        assert!(source.contains("remediation_source_id.as_deref() == Some(source_id)"));
        let actions = include_str!("investigation/actions.rs");
        assert!(actions.contains("OutcomeFactKind::SourceRemediated"));
        let objectives = include_str!("strategic/custody_objectives.rs");
        assert!(objectives.contains("accepted_hostile_remediation"));
    }
}
