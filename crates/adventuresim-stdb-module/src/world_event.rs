//! Private typed occurrence authority and synchronous consequence adapters.
//!
//! This is deliberately not a public event log or subscription surface. It
//! records exact canonical provenance only after all consequences succeed in
//! the surrounding SpacetimeDB transaction.

use adventuresim_core::world_event::{
    WORLD_EVENT_SCHEMA_REVISION, WorldEventActor, WorldEventConsequence, WorldEventEnvelope,
    WorldEventOffenseKind as ExistingOffenseKind, WorldEventPayloadRef, WorldEventPlace,
    WorldEventReputationMeaning as ReputationMeaning, WorldEventSource, WorldEventSubject,
    plan_food_water_infection, plan_generated_case_resolution, plan_noticed_illegal_foraging,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spacetimedb::{ReducerContext, SpacetimeType, Table, table};
use std::collections::BTreeSet;

use crate::disease::infection_episode;
use crate::local_problem::local_problem_outcome_receipt;
use crate::reputation::{case_reputation_participant, discovered_offense, reputation_event};

const MAX_WORLD_EVENT_CONSEQUENCES: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, SpacetimeType)]
pub struct PersistedForagingEventSource {
    pub request_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, SpacetimeType)]
pub struct PersistedGeneratedFinaleEventSource {
    pub finale_id: String,
    pub source_id: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, SpacetimeType)]
pub struct PersistedFoodWaterEventSource {
    pub consumption_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, SpacetimeType)]
pub enum PersistedWorldEventSource {
    ForagingAction(PersistedForagingEventSource),
    GeneratedCaseFinale(PersistedGeneratedFinaleEventSource),
    FoodWaterExposure(PersistedFoodWaterEventSource),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, SpacetimeType)]
pub enum PersistedWorldEventActor {
    Character { character_id: u64 },
    Party { party_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, SpacetimeType)]
pub enum PersistedWorldEventSubject {
    Character { character_id: u64 },
    Case { canonical_case_id: String },
    LocalProblem { problem_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, SpacetimeType)]
pub enum PersistedWorldEventPlace {
    Settlement { settlement_id: String },
    Strategic { place_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, SpacetimeType)]
pub struct PersistedForagingPayloadRef {
    pub request_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, SpacetimeType)]
pub struct PersistedCaseResolutionPayloadRef {
    pub canonical_case_id: String,
    pub finale_id: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, SpacetimeType)]
pub struct PersistedFoodWaterPayloadRef {
    pub carrier_id: u64,
    pub contribution_digest: String,
    pub dose_microunits: u64,
    pub protected_dose_microunits: u64,
    pub immunity_milli: u32,
    pub prior_immunity_milli: u32,
    pub consumed_fraction_bps: u16,
    pub disease_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, SpacetimeType)]
pub enum PersistedWorldEventPayloadRef {
    NoticedIllegalForaging(PersistedForagingPayloadRef),
    GeneratedCaseResolution(PersistedCaseResolutionPayloadRef),
    FoodWaterInfection(PersistedFoodWaterPayloadRef),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, SpacetimeType)]
pub struct PersistedWorldEventEnvelope {
    pub schema_revision: u16,
    pub source: PersistedWorldEventSource,
    pub actor: PersistedWorldEventActor,
    pub subjects: Vec<PersistedWorldEventSubject>,
    pub place: PersistedWorldEventPlace,
    pub occurred_at_minute: u64,
    pub payload: PersistedWorldEventPayloadRef,
}

/// Private exact-match replay authority. Consequence details and provenance
/// never enter a public table or view.
#[derive(Clone, Debug)]
#[table(accessor = world_event_receipt)]
pub struct WorldEventReceipt {
    #[primary_key]
    pub id: String,
    pub envelope: PersistedWorldEventEnvelope,
    /// Stable reducer-input identity, computed before any mutable domain read.
    pub request_digest: String,
    /// SHA-256 over the validated closed envelope, including its ID.
    pub envelope_digest: String,
    /// SHA-256 over the validated, closed, canonically ordered consequence set.
    pub consequence_digest: String,
    pub consequence_count: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConsequenceOrder {
    NoticedIllegalForaging,
    GeneratedCaseResolution,
    FoodWaterInfection,
}

#[derive(Clone, Debug, Serialize)]
enum WorldEventRequest {
    NoticedIllegalForaging {
        envelope: WorldEventEnvelope,
        infamy_centipoints: i32,
    },
    GeneratedCaseResolution {
        envelope: WorldEventEnvelope,
        public_case_id: String,
        fame: i32,
    },
    FoodWaterInfection {
        envelope: WorldEventEnvelope,
        episode_id: u64,
    },
}

impl WorldEventRequest {
    fn envelope(&self) -> &WorldEventEnvelope {
        match self {
            Self::NoticedIllegalForaging { envelope, .. }
            | Self::GeneratedCaseResolution { envelope, .. }
            | Self::FoodWaterInfection { envelope, .. } => envelope,
        }
    }
}

fn consequence_identity(consequence: &WorldEventConsequence) -> (&'static str, String) {
    match consequence {
        WorldEventConsequence::Reputation { event_id, .. } => ("reputation", event_id.clone()),
        WorldEventConsequence::DiscoveredOffense { offense_id, .. } => {
            ("offense", offense_id.clone())
        }
        WorldEventConsequence::LocalProblemOutcome {
            source_outcome_id, ..
        } => ("local_problem", source_outcome_id.clone()),
        WorldEventConsequence::CaseParticipantSnapshot { snapshot_id, .. } => {
            ("case_participant", snapshot_id.clone())
        }
        WorldEventConsequence::InfectionEpisode { episode_id, .. } => {
            ("infection_episode", episode_id.to_string())
        }
    }
}

fn persist(envelope: &WorldEventEnvelope) -> PersistedWorldEventEnvelope {
    PersistedWorldEventEnvelope {
        schema_revision: envelope.schema_revision,
        source: match &envelope.source {
            WorldEventSource::ForagingAction { request_id } => {
                PersistedWorldEventSource::ForagingAction(PersistedForagingEventSource {
                    request_id: request_id.clone(),
                })
            }
            WorldEventSource::GeneratedCaseFinale {
                finale_id,
                source_id,
            } => PersistedWorldEventSource::GeneratedCaseFinale(
                PersistedGeneratedFinaleEventSource {
                    finale_id: finale_id.clone(),
                    source_id: source_id.clone(),
                },
            ),
            WorldEventSource::FoodWaterExposure { consumption_id } => {
                PersistedWorldEventSource::FoodWaterExposure(PersistedFoodWaterEventSource {
                    consumption_id: consumption_id.clone(),
                })
            }
        },
        actor: match &envelope.actor {
            WorldEventActor::Character { character_id } => PersistedWorldEventActor::Character {
                character_id: *character_id,
            },
            WorldEventActor::Party { party_id } => PersistedWorldEventActor::Party {
                party_id: party_id.clone(),
            },
        },
        subjects: envelope
            .subjects
            .iter()
            .map(|subject| match subject {
                WorldEventSubject::Character { character_id } => {
                    PersistedWorldEventSubject::Character {
                        character_id: *character_id,
                    }
                }
                WorldEventSubject::Case { canonical_case_id } => PersistedWorldEventSubject::Case {
                    canonical_case_id: canonical_case_id.clone(),
                },
                WorldEventSubject::LocalProblem { problem_id } => {
                    PersistedWorldEventSubject::LocalProblem {
                        problem_id: problem_id.clone(),
                    }
                }
            })
            .collect(),
        place: match &envelope.place {
            WorldEventPlace::Settlement { settlement_id } => PersistedWorldEventPlace::Settlement {
                settlement_id: settlement_id.clone(),
            },
            WorldEventPlace::Strategic { place_id } => PersistedWorldEventPlace::Strategic {
                place_id: place_id.clone(),
            },
        },
        occurred_at_minute: envelope.occurred_at_minute,
        payload: match &envelope.payload {
            WorldEventPayloadRef::NoticedIllegalForaging { request_id } => {
                PersistedWorldEventPayloadRef::NoticedIllegalForaging(PersistedForagingPayloadRef {
                    request_id: request_id.clone(),
                })
            }
            WorldEventPayloadRef::GeneratedCaseResolution {
                canonical_case_id,
                finale_id,
            } => PersistedWorldEventPayloadRef::GeneratedCaseResolution(
                PersistedCaseResolutionPayloadRef {
                    canonical_case_id: canonical_case_id.clone(),
                    finale_id: finale_id.clone(),
                },
            ),
            WorldEventPayloadRef::FoodWaterInfection {
                carrier_id,
                contribution_digest,
                dose_microunits,
                protected_dose_microunits,
                immunity_milli,
                prior_immunity_milli,
                consumed_fraction_bps,
                disease_id,
            } => PersistedWorldEventPayloadRef::FoodWaterInfection(PersistedFoodWaterPayloadRef {
                carrier_id: *carrier_id,
                contribution_digest: contribution_digest.clone(),
                dose_microunits: *dose_microunits,
                protected_dose_microunits: *protected_dose_microunits,
                immunity_milli: *immunity_milli,
                prior_immunity_milli: *prior_immunity_milli,
                consumed_fraction_bps: *consumed_fraction_bps,
                disease_id: disease_id.clone(),
            }),
        },
    }
}

fn fingerprint<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "Could not fingerprint typed world event")?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn commit_world_event(
    ctx: &ReducerContext,
    request: WorldEventRequest,
    build: impl FnOnce() -> (
        WorldEventEnvelope,
        ConsequenceOrder,
        Vec<WorldEventConsequence>,
    ),
) -> Result<bool, String> {
    request
        .envelope()
        .validate()
        .map_err(|_| "Typed world event is invalid")?;
    let request_digest = fingerprint(&request)?;
    if let Some(existing) = ctx
        .db
        .world_event_receipt()
        .id()
        .find(&request.envelope().id)
    {
        return if existing.request_digest == request_digest {
            Ok(false)
        } else {
            Err("World event ID conflicts with different canonical request".into())
        };
    }
    let (envelope, order, consequences) = build();
    envelope
        .validate()
        .map_err(|_| "Typed world event is invalid")?;
    validate_consequences(order, &consequences)?;
    validate_semantic_binding(&request, &envelope, &consequences)?;
    let digest = fingerprint(&envelope)?;
    let consequence_digest = fingerprint(&consequences)?;
    let consequence_count = u16::try_from(consequences.len())
        .map_err(|_| "World event consequence count is out of bounds")?;
    let persisted = persist(&envelope);
    preflight_consequences(ctx, &consequences)?;
    apply_consequences(ctx, consequences)?;
    ctx.db.world_event_receipt().insert(WorldEventReceipt {
        id: envelope.id,
        envelope: persisted,
        request_digest,
        envelope_digest: digest,
        consequence_digest,
        consequence_count,
    });
    Ok(true)
}

fn apply_consequences(
    ctx: &ReducerContext,
    consequences: Vec<WorldEventConsequence>,
) -> Result<(), String> {
    for consequence in consequences {
        match consequence {
            WorldEventConsequence::Reputation {
                event_id,
                character_id,
                settlement_id,
                meaning,
                source_id,
                raw_fame,
                raw_infamy,
                minute,
            } => {
                let source_kind = match meaning {
                    ReputationMeaning::IllegalForaging => "illegal_foraging",
                    ReputationMeaning::CaseResolution => "case_resolution",
                };
                crate::reputation::record_event(
                    ctx,
                    event_id,
                    character_id,
                    &settlement_id,
                    source_kind,
                    &source_id,
                    raw_fame,
                    raw_infamy,
                    minute,
                )?;
            }
            WorldEventConsequence::DiscoveredOffense {
                offense_id,
                character_id,
                settlement_id,
                kind,
                severity,
                minute,
            } => {
                let kind = match kind {
                    ExistingOffenseKind::IllegalForaging => "illegal_foraging",
                };
                crate::reputation::record_discovered_offense(
                    ctx,
                    offense_id,
                    character_id,
                    &settlement_id,
                    kind,
                    severity,
                    minute,
                );
            }
            WorldEventConsequence::LocalProblemOutcome {
                problem_id,
                source_outcome_id,
                minute,
                mitigation_bps,
                resolve,
            } => {
                crate::local_problem::apply_outcome(
                    ctx,
                    &problem_id,
                    &crate::local_problem::LocalProblemOutcomeInput {
                        source_outcome_id,
                        at_minute: minute,
                        mitigation_bps,
                        resolve,
                    },
                )?;
            }
            WorldEventConsequence::CaseParticipantSnapshot {
                case_id,
                character_id,
                party_id,
                minute,
                ..
            } => crate::reputation::snapshot_case_resolution_participant(
                ctx,
                &case_id,
                character_id,
                &party_id,
                minute,
            ),
            WorldEventConsequence::InfectionEpisode {
                episode_id,
                character_id,
                disease_id,
                contracted_at,
                ..
            } => {
                if ctx.db.infection_episode().id().find(episode_id).is_none() {
                    ctx.db
                        .infection_episode()
                        .insert(crate::disease::InfectionEpisodeRow {
                            id: episode_id,
                            character_id,
                            disease_id,
                            contracted_at,
                            ruleset_version:
                                adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION,
                            phenotype_key_version:
                                adventuresim_core::physiology::PHENOTYPE_KEY_VERSION,
                        });
                }
            }
        }
    }
    Ok(())
}

fn validate_consequences(
    order: ConsequenceOrder,
    consequences: &[WorldEventConsequence],
) -> Result<(), String> {
    if consequences.len() > MAX_WORLD_EVENT_CONSEQUENCES {
        return Err("World event consequence count is out of bounds".into());
    }
    let mut identities = BTreeSet::new();
    validate_consequence_order(order, consequences)?;
    for consequence in consequences {
        let identity = consequence_identity(consequence);
        if !identities.insert(identity) {
            return Err("World event repeats a consequence identity".into());
        }
    }
    Ok(())
}

fn validate_semantic_binding(
    request: &WorldEventRequest,
    envelope: &WorldEventEnvelope,
    consequences: &[WorldEventConsequence],
) -> Result<(), String> {
    let expected = match request {
        WorldEventRequest::NoticedIllegalForaging {
            envelope: requested,
            infamy_centipoints,
        } => {
            if envelope != requested {
                return Err("Foraging event envelope differs from its request".into());
            }
            let (
                WorldEventSource::ForagingAction { request_id },
                WorldEventActor::Character { character_id },
                WorldEventPlace::Settlement { settlement_id },
            ) = (&envelope.source, &envelope.actor, &envelope.place)
            else {
                return Err("Foraging event request is not canonical".into());
            };
            plan_noticed_illegal_foraging(
                *character_id,
                settlement_id,
                request_id,
                *infamy_centipoints,
                envelope.occurred_at_minute,
            )
        }
        WorldEventRequest::GeneratedCaseResolution {
            envelope: requested,
            fame,
            ..
        } => {
            let mut stable = envelope.clone();
            stable
                .subjects
                .retain(|subject| !matches!(subject, WorldEventSubject::Character { .. }));
            if stable != *requested {
                return Err("Case event envelope differs from its stable request".into());
            }
            let (
                WorldEventSource::GeneratedCaseFinale {
                    finale_id,
                    source_id,
                },
                WorldEventActor::Party { party_id },
                WorldEventPlace::Settlement { settlement_id },
                WorldEventPayloadRef::GeneratedCaseResolution {
                    canonical_case_id,
                    finale_id: payload_finale_id,
                },
            ) = (
                &envelope.source,
                &envelope.actor,
                &envelope.place,
                &envelope.payload,
            )
            else {
                return Err("Case event request is not canonical".into());
            };
            if finale_id != payload_finale_id || envelope.id != format!("case-finale:{finale_id}") {
                return Err("Case event identity is not canonical".into());
            }
            let problem_id = envelope.subjects.iter().find_map(|subject| match subject {
                WorldEventSubject::LocalProblem { problem_id } => Some(problem_id),
                _ => None,
            });
            let character_ids = envelope
                .subjects
                .iter()
                .filter_map(|subject| match subject {
                    WorldEventSubject::Character { character_id } => Some(*character_id),
                    _ => None,
                })
                .collect::<Vec<_>>();
            plan_generated_case_resolution(
                canonical_case_id,
                party_id,
                settlement_id,
                source_id,
                problem_id.map(String::as_str),
                &character_ids,
                *fame,
                envelope.occurred_at_minute,
            )
        }
        WorldEventRequest::FoodWaterInfection {
            envelope: requested,
            episode_id,
        } => {
            if envelope != requested {
                return Err("Food-water infection envelope differs from its request".into());
            }
            let (
                WorldEventSource::FoodWaterExposure { consumption_id },
                WorldEventActor::Character { character_id },
                WorldEventPayloadRef::FoodWaterInfection {
                    contribution_digest,
                    disease_id,
                    ..
                },
            ) = (&envelope.source, &envelope.actor, &envelope.payload)
            else {
                return Err("Food-water infection request is not canonical".into());
            };
            if envelope.id != format!("food-water-infection:{character_id}:{consumption_id}") {
                return Err("Food-water infection identity is not canonical".into());
            }
            adventuresim_core::world_event::plan_food_water_infection(
                *episode_id,
                *character_id,
                disease_id,
                envelope.occurred_at_minute,
                contribution_digest,
            )
        }
    };
    if consequences == expected {
        Ok(())
    } else {
        Err("World event consequences differ from canonical request semantics".into())
    }
}

fn preflight_consequences(
    ctx: &ReducerContext,
    consequences: &[WorldEventConsequence],
) -> Result<(), String> {
    for consequence in consequences {
        let exact = match consequence {
            WorldEventConsequence::Reputation {
                event_id,
                character_id,
                settlement_id,
                meaning,
                source_id,
                raw_fame,
                raw_infamy,
                minute,
            } => ctx
                .db
                .reputation_event()
                .id()
                .find(event_id)
                .map(|existing| {
                    let source_kind = match meaning {
                        ReputationMeaning::IllegalForaging => "illegal_foraging",
                        ReputationMeaning::CaseResolution => "case_resolution",
                    };
                    existing.character_id == *character_id
                        && existing.origin_settlement_id == *settlement_id
                        && existing.source_kind == source_kind
                        && existing.source_id == *source_id
                        && existing.raw_fame == *raw_fame
                        && existing.raw_infamy == *raw_infamy
                        && existing.occurred_at_minute == *minute
                }),
            WorldEventConsequence::InfectionEpisode {
                episode_id,
                character_id,
                disease_id,
                contracted_at,
                ..
            } => ctx
                .db
                .infection_episode()
                .id()
                .find(*episode_id)
                .map(|existing| {
                    existing.character_id == *character_id
                        && existing.disease_id == *disease_id
                        && existing.contracted_at == *contracted_at
                        && existing.ruleset_version
                            == adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION
                        && existing.phenotype_key_version
                            == adventuresim_core::physiology::PHENOTYPE_KEY_VERSION
                }),
            WorldEventConsequence::DiscoveredOffense {
                offense_id,
                character_id,
                settlement_id,
                kind,
                severity,
                minute,
            } => ctx
                .db
                .discovered_offense()
                .id()
                .find(offense_id)
                .map(|existing| {
                    let kind = match kind {
                        ExistingOffenseKind::IllegalForaging => "illegal_foraging",
                    };
                    existing.character_id == *character_id
                        && existing.settlement_id == *settlement_id
                        && existing.kind == kind
                        && existing.severity == (*severity).clamp(1, 5)
                        && !existing.execution_eligible
                        && existing.occurred_at_minute == *minute
                    // `settled` is mutable downstream legal state, not authored identity.
                }),
            WorldEventConsequence::LocalProblemOutcome {
                problem_id,
                source_outcome_id,
                minute,
                mitigation_bps,
                resolve,
            } => {
                let input = crate::local_problem::LocalProblemOutcomeInput {
                    source_outcome_id: source_outcome_id.clone(),
                    at_minute: *minute,
                    mitigation_bps: *mitigation_bps,
                    resolve: *resolve,
                };
                let expected = serde_json::to_string(&input)
                    .map_err(|_| "Could not encode outcome payload")?;
                let receipt_id = format!("{problem_id}:{source_outcome_id}");
                ctx.db
                    .local_problem_outcome_receipt()
                    .id()
                    .find(&receipt_id)
                    .map(|existing| {
                        existing.problem_id == *problem_id
                            && existing.source_outcome_id == *source_outcome_id
                            && existing.applied_at == *minute
                            && existing.mitigation_bps == *mitigation_bps
                            && existing.resolved == *resolve
                            && existing.payload_fingerprint == expected
                    })
            }
            WorldEventConsequence::CaseParticipantSnapshot {
                snapshot_id,
                case_id,
                character_id,
                party_id,
                minute,
            } => ctx
                .db
                .case_reputation_participant()
                .id()
                .find(snapshot_id)
                .map(|existing| {
                    existing.case_id == *case_id
                        && existing.character_id == *character_id
                        && existing.party_id == *party_id
                        && existing.captured_at_minute == *minute
                }),
        };
        if exact == Some(false) {
            let (kind, id) = consequence_identity(consequence);
            return Err(format!(
                "World event {kind} consequence ID conflicts with existing row: {id}"
            ));
        }
    }
    Ok(())
}

fn validate_consequence_order(
    order: ConsequenceOrder,
    consequences: &[WorldEventConsequence],
) -> Result<(), String> {
    match order {
        ConsequenceOrder::NoticedIllegalForaging => match consequences {
            [
                WorldEventConsequence::Reputation {
                    meaning: ReputationMeaning::IllegalForaging,
                    character_id: reputation_character,
                    settlement_id: reputation_settlement,
                    minute: reputation_minute,
                    ..
                },
                WorldEventConsequence::DiscoveredOffense {
                    kind: ExistingOffenseKind::IllegalForaging,
                    character_id: offense_character,
                    settlement_id: offense_settlement,
                    minute: offense_minute,
                    ..
                },
            ] if reputation_character == offense_character
                && reputation_settlement == offense_settlement
                && reputation_minute == offense_minute =>
            {
                Ok(())
            }
            _ => Err("Illegal-foraging consequences are not canonical".into()),
        },
        ConsequenceOrder::GeneratedCaseResolution => {
            let consequence_start = usize::from(matches!(
                consequences.first(),
                Some(WorldEventConsequence::LocalProblemOutcome { .. })
            ));
            let participant_consequences = &consequences[consequence_start..];
            if !participant_consequences.len().is_multiple_of(2) {
                return Err("Case-resolution consequences are not canonical".into());
            }
            let mut prior_character_id = None;
            for pair in participant_consequences.as_chunks::<2>().0 {
                let [
                    WorldEventConsequence::CaseParticipantSnapshot {
                        character_id: snapshot_character,
                        case_id,
                        ..
                    },
                    WorldEventConsequence::Reputation {
                        meaning: ReputationMeaning::CaseResolution,
                        character_id: reputation_character,
                        source_id,
                        ..
                    },
                ] = pair
                else {
                    return Err("Case-resolution consequences are not canonical".into());
                };
                if snapshot_character != reputation_character || case_id != source_id {
                    return Err("Case-resolution consequence provenance differs".into());
                }
                if prior_character_id.is_some_and(|prior| prior >= *snapshot_character) {
                    return Err("Case reputation participants are not canonical".into());
                }
                prior_character_id = Some(*snapshot_character);
            }
            Ok(())
        }
        ConsequenceOrder::FoodWaterInfection => match consequences {
            [WorldEventConsequence::InfectionEpisode { .. }] => Ok(()),
            _ => Err("Food-water infection consequences are not canonical".into()),
        },
    }
}

pub(crate) fn commit_noticed_illegal_foraging(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
    request_id: &str,
    infamy_centipoints: i32,
    minute: u64,
) -> Result<(), String> {
    let envelope = WorldEventEnvelope {
        schema_revision: WORLD_EVENT_SCHEMA_REVISION,
        id: format!("forage:{character_id}:{request_id}"),
        source: WorldEventSource::ForagingAction {
            request_id: request_id.into(),
        },
        actor: WorldEventActor::Character { character_id },
        subjects: vec![WorldEventSubject::Character { character_id }],
        place: WorldEventPlace::Settlement {
            settlement_id: settlement_id.into(),
        },
        occurred_at_minute: minute,
        payload: WorldEventPayloadRef::NoticedIllegalForaging {
            request_id: request_id.into(),
        },
    };
    let consequences = plan_noticed_illegal_foraging(
        character_id,
        settlement_id,
        request_id,
        infamy_centipoints,
        minute,
    );
    let request = WorldEventRequest::NoticedIllegalForaging {
        envelope: envelope.clone(),
        infamy_centipoints,
    };
    commit_world_event(ctx, request, || {
        (
            envelope,
            ConsequenceOrder::NoticedIllegalForaging,
            consequences,
        )
    })
    .map(|_| ())
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
pub(crate) fn commit_generated_case_resolution(
    ctx: &ReducerContext,
    finale_id: &str,
    source_id: &str,
    canonical_case_id: &str,
    public_case_id: &str,
    party_id: &str,
    settlement_id: &str,
    local_problem_id: Option<&str>,
    fame: i32,
    minute: u64,
) -> Result<(), String> {
    let mut subjects = vec![WorldEventSubject::Case {
        canonical_case_id: canonical_case_id.into(),
    }];
    if let Some(problem_id) = local_problem_id {
        subjects.push(WorldEventSubject::LocalProblem {
            problem_id: problem_id.into(),
        });
    }
    subjects.sort();
    let request_envelope = WorldEventEnvelope {
        schema_revision: WORLD_EVENT_SCHEMA_REVISION,
        id: format!("case-finale:{finale_id}"),
        source: WorldEventSource::GeneratedCaseFinale {
            finale_id: finale_id.into(),
            source_id: source_id.into(),
        },
        actor: WorldEventActor::Party {
            party_id: party_id.into(),
        },
        subjects,
        place: WorldEventPlace::Settlement {
            settlement_id: settlement_id.into(),
        },
        occurred_at_minute: minute,
        payload: WorldEventPayloadRef::GeneratedCaseResolution {
            canonical_case_id: canonical_case_id.into(),
            finale_id: finale_id.into(),
        },
    };
    let request = WorldEventRequest::GeneratedCaseResolution {
        envelope: request_envelope.clone(),
        public_case_id: public_case_id.into(),
        fame,
    };
    commit_world_event(ctx, request, || {
        let character_ids =
            crate::reputation::case_resolution_participant_ids(ctx, public_case_id, party_id);
        let mut envelope = request_envelope;
        envelope.subjects.extend(
            character_ids
                .iter()
                .copied()
                .map(|character_id| WorldEventSubject::Character { character_id }),
        );
        envelope.subjects.sort();
        let consequences = plan_generated_case_resolution(
            canonical_case_id,
            party_id,
            settlement_id,
            source_id,
            local_problem_id,
            &character_ids,
            fame,
            minute,
        );
        (
            envelope,
            ConsequenceOrder::GeneratedCaseResolution,
            consequences,
        )
    })
    .map(|_| ())
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
pub(crate) fn commit_food_water_infection(
    ctx: &ReducerContext,
    consumption_id: &str,
    character_id: u64,
    strategic_place_id: &str,
    carrier_id: u64,
    contribution_digest: &str,
    dose: f32,
    protected_dose: f32,
    immunity: f32,
    prior_immunity: f32,
    consumed_fraction_bps: u16,
    disease_id: &str,
    episode_id: u64,
    minute: u64,
) -> Result<(), String> {
    let envelope = WorldEventEnvelope {
        schema_revision: WORLD_EVENT_SCHEMA_REVISION,
        id: format!("food-water-infection:{character_id}:{consumption_id}"),
        source: WorldEventSource::FoodWaterExposure {
            consumption_id: consumption_id.into(),
        },
        actor: WorldEventActor::Character { character_id },
        subjects: vec![WorldEventSubject::Character { character_id }],
        place: WorldEventPlace::Strategic {
            place_id: strategic_place_id.into(),
        },
        occurred_at_minute: minute,
        payload: WorldEventPayloadRef::FoodWaterInfection {
            carrier_id,
            contribution_digest: contribution_digest.into(),
            dose_microunits: adventuresim_core::world_event::infection_dose_microunits(dose)
                .ok_or("Food-water infection dose is invalid")?,
            protected_dose_microunits: adventuresim_core::world_event::infection_dose_microunits(
                protected_dose,
            )
            .ok_or("Protected food-water infection dose is invalid")?,
            immunity_milli: (immunity.clamp(0.0, 100.0) * 1_000.0).round() as u32,
            prior_immunity_milli: (prior_immunity.clamp(0.0, 100.0) * 1_000.0).round() as u32,
            consumed_fraction_bps,
            disease_id: disease_id.into(),
        },
    };
    let consequences = plan_food_water_infection(
        episode_id,
        character_id,
        disease_id,
        minute,
        contribution_digest,
    );
    let request = WorldEventRequest::FoodWaterInfection {
        envelope: envelope.clone(),
        episode_id,
    };
    commit_world_event(ctx, request, || {
        (envelope, ConsequenceOrder::FoodWaterInfection, consequences)
    })
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_envelope_keeps_every_private_typed_field() {
        let event = WorldEventEnvelope {
            schema_revision: WORLD_EVENT_SCHEMA_REVISION,
            id: "forage:7:req".into(),
            source: WorldEventSource::ForagingAction {
                request_id: "req".into(),
            },
            actor: WorldEventActor::Character { character_id: 7 },
            subjects: vec![WorldEventSubject::Character { character_id: 7 }],
            place: WorldEventPlace::Settlement {
                settlement_id: "lubeck".into(),
            },
            occurred_at_minute: 60,
            payload: WorldEventPayloadRef::NoticedIllegalForaging {
                request_id: "req".into(),
            },
        };
        assert_eq!(persist(&event).occurred_at_minute, 60);
        assert_eq!(fingerprint(&event), fingerprint(&event.clone()));
    }

    #[test]
    fn consequence_identity_is_closed_and_exact() {
        let consequence = WorldEventConsequence::DiscoveredOffense {
            offense_id: "offense:1".into(),
            character_id: 7,
            settlement_id: "lubeck".into(),
            kind: ExistingOffenseKind::IllegalForaging,
            severity: 1,
            minute: 60,
        };
        assert_eq!(
            consequence_identity(&consequence),
            ("offense", "offense:1".into())
        );
    }

    #[test]
    fn canonical_orders_reject_duplicates_and_reordering() {
        let reputation = WorldEventConsequence::Reputation {
            event_id: "forage:7:req".into(),
            character_id: 7,
            settlement_id: "lubeck".into(),
            meaning: ReputationMeaning::IllegalForaging,
            source_id: "req".into(),
            raw_fame: 0,
            raw_infamy: 100,
            minute: 60,
        };
        let offense = WorldEventConsequence::DiscoveredOffense {
            offense_id: "offense:forage:7:req".into(),
            character_id: 7,
            settlement_id: "lubeck".into(),
            kind: ExistingOffenseKind::IllegalForaging,
            severity: 1,
            minute: 60,
        };
        assert!(
            validate_consequence_order(
                ConsequenceOrder::NoticedIllegalForaging,
                &[reputation.clone(), offense.clone()]
            )
            .is_ok()
        );
        assert!(
            validate_consequence_order(
                ConsequenceOrder::NoticedIllegalForaging,
                &[offense, reputation]
            )
            .is_err()
        );
    }

    #[test]
    fn request_identity_binds_deltas_and_semantic_plan() {
        let envelope = WorldEventEnvelope {
            schema_revision: WORLD_EVENT_SCHEMA_REVISION,
            id: "forage:7:req".into(),
            source: WorldEventSource::ForagingAction {
                request_id: "req".into(),
            },
            actor: WorldEventActor::Character { character_id: 7 },
            subjects: vec![WorldEventSubject::Character { character_id: 7 }],
            place: WorldEventPlace::Settlement {
                settlement_id: "lubeck".into(),
            },
            occurred_at_minute: 60,
            payload: WorldEventPayloadRef::NoticedIllegalForaging {
                request_id: "req".into(),
            },
        };
        let request = WorldEventRequest::NoticedIllegalForaging {
            envelope: envelope.clone(),
            infamy_centipoints: 100,
        };
        let changed = WorldEventRequest::NoticedIllegalForaging {
            envelope: envelope.clone(),
            infamy_centipoints: 101,
        };
        assert_ne!(
            fingerprint(&request).unwrap(),
            fingerprint(&changed).unwrap()
        );

        let consequences = vec![
            WorldEventConsequence::Reputation {
                event_id: envelope.id.clone(),
                character_id: 7,
                settlement_id: "lubeck".into(),
                meaning: ReputationMeaning::IllegalForaging,
                source_id: "req".into(),
                raw_fame: 0,
                raw_infamy: 100,
                minute: 60,
            },
            WorldEventConsequence::DiscoveredOffense {
                offense_id: "offense:forage:7:req".into(),
                character_id: 7,
                settlement_id: "lubeck".into(),
                kind: ExistingOffenseKind::IllegalForaging,
                severity: 1,
                minute: 60,
            },
        ];
        assert!(validate_semantic_binding(&request, &envelope, &consequences).is_ok());
        let mut wrong = consequences;
        let WorldEventConsequence::Reputation { raw_infamy, .. } = &mut wrong[0] else {
            unreachable!();
        };
        *raw_infamy += 1;
        assert!(validate_semantic_binding(&request, &envelope, &wrong).is_err());
    }

    #[test]
    fn receipt_lookup_precedes_mutable_plan_reads_and_commit() {
        let source = crate::production_source(include_str!("world_event.rs"));
        let commit = source
            .split("fn commit_world_event")
            .nth(1)
            .and_then(|tail| tail.split("fn apply_consequences").next())
            .expect("world event commit boundary");
        let lookup = commit
            .find("world_event_receipt()")
            .expect("receipt lookup");
        let build = commit
            .find("let (envelope, order, consequences) = build()")
            .expect("first-application plan build");
        let preflight = commit
            .find("preflight_consequences")
            .expect("subordinate exact preflight");
        let insert = commit
            .rfind(".insert(WorldEventReceipt")
            .expect("receipt inserted last");
        assert!(lookup < build && build < preflight && preflight < insert);
    }
}
