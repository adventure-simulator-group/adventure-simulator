//! Private generated-outbreak authority and patient materialization.
//!
//! Canonical disease, source and remediation facts never cross a public view.

use spacetimedb::{ReducerContext, Table, ViewContext, table};

use crate::{
    character::{character, character_attributes, character_death},
    corpse::strategic_corpse,
    disease::infection_episode,
    local_problem::{local_problem_receipt, local_problem_receipt__view},
    relationship::character_kinship,
    settlement_population::{settlement_resident_presence, settlement_resident_profile},
    strategic::party_authority,
    time::character_time,
    world_actor::character_context_membership,
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
    pub responsible_resident_character_id: Option<u64>,
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
    pub case_id: String,
    #[index(btree)]
    pub patient_character_id: u64,
    #[unique]
    pub episode_id: u64,
    pub context_active: bool,
    pub health_active: bool,
    pub corpse_id: Option<String>,
    pub autopsy_evidence_id: Option<String>,
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

/// Case-site Patient rows are visible only to a party whose leader has learned
/// the underlying local problem. Exact physical co-presence is checked by the
/// shared context projection before this predicate is called.
pub(crate) fn case_patient_visible_to_character_view(
    ctx: &ViewContext,
    character_id: u64,
    case_id: &str,
) -> bool {
    let Some(authority) = ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&case_id.to_owned())
    else {
        return false;
    };
    ctx.db
        .local_problem_receipt()
        .character_id()
        .filter(character_id)
        .any(|receipt| {
            receipt.problem_id == authority.problem_id
                && receipt.settlement_id == authority.settlement_id
        })
}

pub(crate) fn case_patient_visible_to_party(
    ctx: &ReducerContext,
    party_id: &str,
    case_id: &str,
) -> bool {
    let Some(authority) = ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&case_id.to_owned())
    else {
        return false;
    };
    let Some(party) = ctx.db.party_authority().id().find(&party_id.to_owned()) else {
        return false;
    };
    ctx.db
        .local_problem_receipt()
        .character_id()
        .filter(party.leader_id)
        .any(|receipt| {
            receipt.problem_id == authority.problem_id
                && receipt.settlement_id == authority.settlement_id
        })
}

fn materialize_patient_corpse(
    ctx: &ReducerContext,
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    exposure: &adventuresim_core::quest_generation::OutbreakExposure,
    settlement_id: &str,
    death_minute: u64,
) -> Result<String, String> {
    use adventuresim_core::{
        autopsy::SystemicPathologySnapshot,
        disease::{InfectionEpisode, Symptom, combined_state},
        physiology::Meter,
        quest_generation::OutbreakPatientDeathKind,
    };
    let outbreak = generated
        .outbreak
        .as_ref()
        .ok_or("Outbreak truth missing")?;
    let cause = match exposure.death_kind {
        Some(OutbreakPatientDeathKind::CarrierAttack) => crate::character::DeathCause::Combat,
        Some(OutbreakPatientDeathKind::Disease) => crate::character::DeathCause::Disease,
        None => return Err("Living outbreak patient cannot materialize a corpse".into()),
    };
    let source = match exposure.death_kind {
        Some(OutbreakPatientDeathKind::CarrierAttack) => crate::character::DeathSource::Autoresolve,
        _ => crate::character::DeathSource::Disease,
    };
    crate::investigation::set_character_case_site(
        ctx,
        exposure.patient_character_id,
        Some(outbreak.patient_presentation_site.0.clone()),
    );
    let death_source_id = format!("outbreak-victim:{}", generated.canonical_case_id);
    if let Some(existing) = ctx
        .db
        .character_death()
        .character_id()
        .find(exposure.patient_character_id)
    {
        if existing.strategic_minute != death_minute
            || existing.source_id.as_deref() != Some(death_source_id.as_str())
        {
            return Err("Outbreak patient death provenance collision".into());
        }
    }
    crate::character::transition_character_to_dead_at(
        ctx,
        exposure.patient_character_id,
        cause,
        source,
        Some(death_source_id),
        death_minute,
    )?;
    let corpse_id = format!("corpse:character:{}", exposure.patient_character_id);
    let episode = InfectionEpisode {
        id: exposure.episode_id,
        character_id: exposure.patient_character_id,
        disease_id: outbreak.disease,
        contracted_at: exposure.exposed_at,
        ruleset_version: adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION,
        phenotype_key_version: adventuresim_core::physiology::PHENOTYPE_KEY_VERSION,
    };
    let (_, vitals, symptoms, _) = combined_state(
        &[episode],
        death_minute,
        ctx.db
            .character_attributes()
            .character_id()
            .find(exposure.patient_character_id)
            .map_or(3.0, |attributes| attributes.immunity),
    );
    let physiology_key = crate::disease::physiology_key(ctx)?;
    if physiology_key.version != adventuresim_core::physiology::PHENOTYPE_KEY_VERSION {
        return Err("Patient phenotype version does not match private key material".into());
    }
    let meters = adventuresim_core::disease::private_meter_state(
        episode,
        death_minute,
        ctx.db
            .character_attributes()
            .character_id()
            .find(exposure.patient_character_id)
            .map_or(3.0, |attributes| attributes.immunity),
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
    let canonical_family = ctx
        .db
        .character_kinship()
        .subject_id()
        .filter(exposure.patient_character_id)
        .filter_map(|edge| {
            ctx.db
                .settlement_resident_profile()
                .character_id()
                .find(edge.related_id)
                .filter(|profile| profile.home_settlement_id == settlement_id)
                .map(|_| edge.related_id)
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    crate::corpse::materialize_corpse_family_bindings(
        ctx,
        &corpse_id,
        settlement_id,
        &canonical_family,
    )?;
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
    if let Some(existing) = ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&generated.canonical_case_id)
    {
        let chronology_json = serde_json::to_string(&outbreak.exposure_chronology)
            .map_err(|_| "Could not encode outbreak chronology")?;
        let exact_patients = outbreak.exposure_chronology.iter().all(|exposure| {
            ctx.db
                .outbreak_patient_authority()
                .id()
                .find(&exposure.patient_ref)
                .is_some_and(|patient| {
                    patient.case_id == generated.canonical_case_id
                        && patient.patient_character_id == exposure.patient_character_id
                        && patient.episode_id == exposure.episode_id
                })
                && ctx
                    .db
                    .infection_episode()
                    .id()
                    .find(exposure.episode_id)
                    .is_some_and(|episode| {
                        episode.character_id == exposure.patient_character_id
                            && episode.contracted_at == exposure.exposed_at
                    })
        });
        return if existing.problem_id == generated.problem_id
            && existing.settlement_id == settlement_id
            && existing.disease_id == crate::disease::disease_key(outbreak.disease)
            && existing.chronology_json == chronology_json
            && exact_patients
        {
            Ok(())
        } else {
            Err("Generated outbreak provenance collision".into())
        };
    }
    if ctx
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
        responsible_resident_character_id: responsible
            .map(|value| value.resident_character_id.clone()),
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
        let npc = crate::settlement_population::resolve_settlement_resident(
            ctx,
            exposure.patient_character_id,
        )
        .ok_or("Outbreak patient NPC no longer exists")?;
        if npc.home_settlement_id != settlement_id {
            return Err("Outbreak presentation NPC is not local to its patient".into());
        }
        let mut patient_time = ctx
            .db
            .character_time()
            .character_id()
            .find(exposure.patient_character_id)
            .ok_or("Outbreak patient Character has no ordinary clock")?;
        if patient_time.minutes < now_minute {
            patient_time.minutes = now_minute;
            ctx.db.character_time().character_id().update(patient_time);
            crate::time::settle_lifecycle_after_character_time_write(
                ctx,
                exposure.patient_character_id,
                now_minute,
            )?;
        }
        let episode = adventuresim_core::disease::InfectionEpisode {
            id: exposure.episode_id,
            character_id: exposure.patient_character_id,
            disease_id: outbreak.disease,
            contracted_at: exposure.exposed_at,
            ruleset_version: adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION,
            phenotype_key_version: adventuresim_core::physiology::PHENOTYPE_KEY_VERSION,
        };
        let immunity = ctx
            .db
            .character_attributes()
            .character_id()
            .find(exposure.patient_character_id)
            .ok_or("Outbreak patient Character has no ordinary attributes")?
            .immunity;
        if let Some(existing) = ctx.db.infection_episode().id().find(exposure.episode_id) {
            if existing.character_id != exposure.patient_character_id
                || existing.disease_id != crate::disease::disease_key(outbreak.disease)
                || existing.contracted_at != exposure.exposed_at
                || existing.ruleset_version
                    != adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION
                || existing.phenotype_key_version
                    != adventuresim_core::physiology::PHENOTYPE_KEY_VERSION
            {
                return Err("Outbreak infection provenance collision".into());
            }
        } else {
            ctx.db
                .infection_episode()
                .insert(crate::disease::InfectionEpisodeRow {
                    id: exposure.episode_id,
                    character_id: exposure.patient_character_id,
                    disease_id: crate::disease::disease_key(outbreak.disease).into(),
                    contracted_at: exposure.exposed_at,
                    ruleset_version: adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION,
                    phenotype_key_version: adventuresim_core::physiology::PHENOTYPE_KEY_VERSION,
                });
        }
        let definition = adventuresim_core::disease::definition(outbreak.disease);
        let course_end = exposure
            .exposed_at
            .saturating_add(definition.incubation_minutes)
            .saturating_add(definition.rise_minutes)
            .saturating_add(definition.peak_minutes)
            .saturating_add(definition.recovery_minutes);
        let private_terminal = crate::disease::first_private_terminal(
            ctx,
            exposure.patient_character_id,
            &[episode],
            exposure.exposed_at,
            course_end,
            immunity,
        )?;
        let mut resolved_exposure = exposure.clone();
        match exposure.death_kind {
            Some(adventuresim_core::quest_generation::OutbreakPatientDeathKind::Disease) => {
                resolved_exposure.died_at = private_terminal.map(|value| value.0);
                resolved_exposure.death_kind = private_terminal.map(|_| {
                    adventuresim_core::quest_generation::OutbreakPatientDeathKind::Disease
                });
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
                } else {
                    resolved_exposure.died_at = None;
                    resolved_exposure.death_kind = None;
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
        let membership_id = format!(
            "context:{}:patient:{}",
            generated.canonical_case_id, exposure.patient_character_id
        );
        let patient_active = resolved_exposure.died_at.is_none() && now_minute < course_end;
        let membership = crate::world_actor::CharacterContextMembership {
            id: membership_id.clone(),
            context_id: generated.canonical_case_id.clone(),
            location_id: outbreak.patient_presentation_site.0.clone(),
            character_id: exposure.patient_character_id,
            context_kind: crate::world_actor::CharacterContextKind::CaseSite,
            role: crate::world_actor::CharacterContextRole::Patient,
            ordinal: u16::try_from(
                outbreak
                    .exposure_chronology
                    .iter()
                    .position(|candidate| candidate.patient_ref == exposure.patient_ref)
                    .ok_or("Outbreak patient lost its authored ordinal")?,
            )
            .map_err(|_| "Outbreak patient ordinal exceeds its bounded roster")?,
            active: patient_active,
            revision: 1,
            treatment_consent: true,
        };
        if ctx
            .db
            .character_context_membership()
            .character_id()
            .filter(exposure.patient_character_id)
            .any(|existing| {
                existing.active
                    && existing.role == crate::world_actor::CharacterContextRole::Patient
                    && existing.context_id != generated.canonical_case_id
            })
        {
            return Err(
                "Canonical Character is already an active Patient in another context".into(),
            );
        }
        if let Some(existing) = ctx
            .db
            .character_context_membership()
            .id()
            .find(&membership_id)
        {
            if existing.context_id != membership.context_id
                || existing.location_id != membership.location_id
                || existing.character_id != membership.character_id
                || existing.context_kind != membership.context_kind
                || existing.role != membership.role
            {
                return Err("Outbreak Patient context provenance collision".into());
            }
        } else {
            ctx.db.character_context_membership().insert(membership);
        }
        if let Some(mut presence) = ctx
            .db
            .settlement_resident_presence()
            .character_id()
            .find(exposure.patient_character_id)
        {
            presence.context_suppressed = patient_active;
            presence.health_suppressed = patient_active
                || ctx
                    .db
                    .character()
                    .id()
                    .find(exposure.patient_character_id)
                    .is_none_or(|character| !character.alive);
            ctx.db
                .settlement_resident_presence()
                .character_id()
                .update(presence);
        }
        ctx.db
            .outbreak_patient_authority()
            .insert(OutbreakPatientAuthority {
                id: row_id,
                case_id: generated.canonical_case_id.clone(),
                patient_character_id: exposure.patient_character_id,
                episode_id: exposure.episode_id,
                context_active: patient_active,
                health_active: patient_active,
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
    deactivate_outbreak_patient_contexts(ctx, case_id);
    Ok(())
}

fn deactivate_outbreak_patient_contexts(ctx: &ReducerContext, case_id: &str) {
    crate::world_actor::deactivate_context_roster(ctx, case_id);
    for mut patient in ctx
        .db
        .outbreak_patient_authority()
        .case_id()
        .filter(&case_id.to_string())
        .collect::<Vec<_>>()
    {
        patient.context_active = false;
        if let Some(mut presence) = ctx
            .db
            .settlement_resident_presence()
            .character_id()
            .find(patient.patient_character_id)
        {
            presence.context_suppressed = false;
            ctx.db
                .settlement_resident_presence()
                .character_id()
                .update(presence);
        }
        ctx.db.outbreak_patient_authority().id().update(patient);
    }
}

/// Release a recovered or dead patient from case-site presentation whenever
/// their ordinary Character clock advances past the standard episode course.
pub(crate) fn refresh_patient_context_after_time_write(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
) {
    let alive = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .is_some_and(|character| character.alive);
    let mut released_any = false;
    for mut patient in ctx
        .db
        .outbreak_patient_authority()
        .patient_character_id()
        .filter(character_id)
        .filter(|patient| patient.health_active)
        .collect::<Vec<_>>()
    {
        let recovered = ctx
            .db
            .infection_episode()
            .id()
            .find(patient.episode_id)
            .and_then(|episode| {
                crate::disease::parse_id(&episode.disease_id)
                    .ok()
                    .map(|id| (episode, id))
            })
            .is_some_and(|(episode, disease_id)| {
                let definition = adventuresim_core::disease::definition(disease_id);
                minute
                    >= episode
                        .contracted_at
                        .saturating_add(definition.incubation_minutes)
                        .saturating_add(definition.rise_minutes)
                        .saturating_add(definition.peak_minutes)
                        .saturating_add(definition.recovery_minutes)
            });
        if alive && !recovered {
            continue;
        }
        patient.context_active = false;
        patient.health_active = false;
        let membership_id = format!(
            "context:{}:patient:{}",
            patient.case_id, patient.patient_character_id
        );
        if let Some(mut membership) = ctx
            .db
            .character_context_membership()
            .id()
            .find(&membership_id)
        {
            membership.active = false;
            ctx.db
                .character_context_membership()
                .id()
                .update(membership);
        }
        ctx.db.outbreak_patient_authority().id().update(patient);
        released_any = true;
    }
    if released_any
        && let Some(mut presence) = ctx
            .db
            .settlement_resident_presence()
            .character_id()
            .find(character_id)
    {
        presence.context_suppressed = false;
        presence.health_suppressed = !alive;
        ctx.db
            .settlement_resident_presence()
            .character_id()
            .update(presence);
    }
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
    deactivate_outbreak_patient_contexts(ctx, case_id);
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
        assert!(production.contains("infection_episode()"));
        assert!(production.contains("CharacterContextRole::Patient"));
        assert!(!production.contains("insert_character_with_origin"));
        assert!(production.contains("patient_character_id"));
        assert!(!production.contains("OutbreakPatientExamination"));
        assert!(!production.contains("examine_outbreak_patient"));
        assert!(!production.contains(&["settlement_outbreak()", ".insert"].concat()));
    }

    #[test]
    fn outbreak_corpses_use_ordinary_character_death_and_generic_pathology() {
        let source = include_str!("outbreak.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("transition_character_to_dead_at"));
        assert!(production.contains("corpse:character:"));
        assert!(production.contains("persist_pathology_snapshot"));
        assert!(production.contains("CharacterContextRole::Patient"));
        assert!(production.contains("context_suppressed"));
    }

    #[test]
    fn patient_context_visibility_requires_problem_knowledge() {
        let source = include_str!("outbreak.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("case_patient_visible_to_character_view"));
        assert!(production.contains("local_problem_receipt()"));
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

    #[test]
    fn remediation_releases_context_without_curing_and_family_is_canonical() {
        let source = include_str!("outbreak.rs");
        let deactivate = source
            .split("fn deactivate_outbreak_patient_contexts")
            .nth(1)
            .and_then(|tail| {
                tail.split("pub(crate) fn refresh_patient_context_after_time_write")
                    .next()
            })
            .expect("context deactivation");
        assert!(deactivate.contains("patient.context_active = false"));
        assert!(!deactivate.contains("patient.health_active = false"));
        assert!(!deactivate.contains("health_suppressed = false"));
        assert!(source.contains("character_kinship()"));
        assert!(!source.contains("family_resident_character_id: Option"));
    }
}
