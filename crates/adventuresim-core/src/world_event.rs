//! Closed provenance for authoritative strategic occurrences.
//!
//! An event is private canonical authority, not an observation, disclosure,
//! permission, subscription, or command bus. Domain reducers remain
//! responsible for deciding rights and knowledge before constructing one.

use adventuresim_world_schema::BASIS_POINTS_PER_WHOLE;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const WORLD_EVENT_SCHEMA_REVISION: u16 = 1;
pub const MAX_WORLD_EVENT_ID_BYTES: usize = 192;
pub const MAX_WORLD_EVENT_SUBJECTS: usize = 64;
const INFECTION_DOSE_MICROUNITS_PER_UNIT: f64 = 1_000_000.0;

/// Quantizes a non-negative infection dose at the world-event boundary.
pub fn infection_dose_microunits(dose: f32) -> Option<u64> {
    if !dose.is_finite() || dose < 0.0 {
        return None;
    }
    let scaled = f64::from(dose) * INFECTION_DOSE_MICROUNITS_PER_UNIT;
    (scaled <= u64::MAX as f64).then(|| scaled.round() as u64)
}

pub fn infection_dose_from_microunits(microunits: u64) -> f64 {
    microunits as f64 / INFECTION_DOSE_MICROUNITS_PER_UNIT
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WorldEventSource {
    ForagingAction {
        request_id: String,
    },
    GeneratedCaseFinale {
        finale_id: String,
        source_id: String,
    },
    FoodWaterExposure {
        consumption_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WorldEventActor {
    Character { character_id: u64 },
    Party { party_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum WorldEventSubject {
    Character { character_id: u64 },
    Case { canonical_case_id: String },
    LocalProblem { problem_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WorldEventPlace {
    Settlement { settlement_id: String },
    Strategic { place_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WorldEventPayloadRef {
    NoticedIllegalForaging {
        request_id: String,
    },
    GeneratedCaseResolution {
        canonical_case_id: String,
        finale_id: String,
    },
    FoodWaterInfection {
        carrier_id: u64,
        contribution_digest: String,
        dose_microunits: u64,
        protected_dose_microunits: u64,
        immunity_milli: u32,
        prior_immunity_milli: u32,
        consumed_fraction_bps: u16,
        disease_id: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WorldEventReputationMeaning {
    IllegalForaging,
    CaseResolution,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WorldEventOffenseKind {
    IllegalForaging,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WorldEventConsequence {
    Reputation {
        event_id: String,
        character_id: u64,
        settlement_id: String,
        meaning: WorldEventReputationMeaning,
        source_id: String,
        raw_fame: i32,
        raw_infamy: i32,
        minute: u64,
    },
    DiscoveredOffense {
        offense_id: String,
        character_id: u64,
        settlement_id: String,
        kind: WorldEventOffenseKind,
        severity: u8,
        minute: u64,
    },
    LocalProblemOutcome {
        problem_id: String,
        source_outcome_id: String,
        minute: u64,
        mitigation_bps: u16,
        resolve: bool,
    },
    CaseParticipantSnapshot {
        snapshot_id: String,
        case_id: String,
        character_id: u64,
        party_id: String,
        minute: u64,
    },
    InfectionEpisode {
        episode_id: u64,
        character_id: u64,
        disease_id: String,
        contracted_at: u64,
        contribution_digest: String,
    },
}

pub fn plan_food_water_infection(
    episode_id: u64,
    character_id: u64,
    disease_id: &str,
    contracted_at: u64,
    contribution_digest: &str,
) -> Vec<WorldEventConsequence> {
    vec![WorldEventConsequence::InfectionEpisode {
        episode_id,
        character_id,
        disease_id: disease_id.into(),
        contracted_at,
        contribution_digest: contribution_digest.into(),
    }]
}

pub fn plan_noticed_illegal_foraging(
    character_id: u64,
    settlement_id: &str,
    request_id: &str,
    infamy_centipoints: i32,
    minute: u64,
) -> Vec<WorldEventConsequence> {
    let event_id = format!("forage:{character_id}:{request_id}");
    vec![
        WorldEventConsequence::Reputation {
            event_id: event_id.clone(),
            character_id,
            settlement_id: settlement_id.into(),
            meaning: WorldEventReputationMeaning::IllegalForaging,
            source_id: request_id.into(),
            raw_fame: 0,
            raw_infamy: infamy_centipoints,
            minute,
        },
        WorldEventConsequence::DiscoveredOffense {
            offense_id: format!("offense:{event_id}"),
            character_id,
            settlement_id: settlement_id.into(),
            kind: WorldEventOffenseKind::IllegalForaging,
            severity: 1,
            minute,
        },
    ]
}

pub fn canonical_case_resolution_participants(
    battle_participant_ids: impl IntoIterator<Item = u64>,
    living_fallback_ids: impl IntoIterator<Item = u64>,
) -> Vec<u64> {
    let mut battle = battle_participant_ids.into_iter().collect::<Vec<_>>();
    battle.sort_unstable();
    battle.dedup();
    if !battle.is_empty() {
        return battle;
    }
    let mut fallback = living_fallback_ids.into_iter().collect::<Vec<_>>();
    fallback.sort_unstable();
    fallback.dedup();
    fallback
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
pub fn plan_generated_case_resolution(
    canonical_case_id: &str,
    party_id: &str,
    settlement_id: &str,
    source_id: &str,
    local_problem_id: Option<&str>,
    participant_ids: &[u64],
    fame: i32,
    minute: u64,
) -> Vec<WorldEventConsequence> {
    let mut consequences = Vec::new();
    if let Some(problem_id) = local_problem_id {
        consequences.push(WorldEventConsequence::LocalProblemOutcome {
            problem_id: problem_id.into(),
            source_outcome_id: source_id.into(),
            minute,
            mitigation_bps: BASIS_POINTS_PER_WHOLE,
            resolve: true,
        });
    }
    for &character_id in participant_ids {
        consequences.push(WorldEventConsequence::CaseParticipantSnapshot {
            snapshot_id: format!("{canonical_case_id}:{character_id}"),
            case_id: canonical_case_id.into(),
            character_id,
            party_id: party_id.into(),
            minute,
        });
        consequences.push(WorldEventConsequence::Reputation {
            event_id: format!("case-resolution:{canonical_case_id}:{character_id}"),
            character_id,
            settlement_id: settlement_id.into(),
            meaning: WorldEventReputationMeaning::CaseResolution,
            source_id: canonical_case_id.into(),
            raw_fame: fame,
            raw_infamy: 0,
            minute,
        });
    }
    consequences
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorldEventEnvelope {
    pub schema_revision: u16,
    pub id: String,
    pub source: WorldEventSource,
    pub actor: WorldEventActor,
    /// Directed affected entities, in canonical enum/value order.
    pub subjects: Vec<WorldEventSubject>,
    /// Canonical occurrence place and legal jurisdiction. This is not proof
    /// that any character observed or learned about the event.
    pub place: WorldEventPlace,
    pub occurred_at_minute: u64,
    pub payload: WorldEventPayloadRef,
}

impl WorldEventEnvelope {
    pub fn validate(&self) -> Result<(), WorldEventError> {
        if self.schema_revision != WORLD_EVENT_SCHEMA_REVISION {
            return Err(WorldEventError::UnsupportedSchemaRevision);
        }
        validate_id(&self.id)?;
        match &self.source {
            WorldEventSource::ForagingAction { request_id } => validate_id(request_id)?,
            WorldEventSource::GeneratedCaseFinale {
                finale_id,
                source_id,
            } => {
                validate_id(finale_id)?;
                validate_id(source_id)?;
            }
            WorldEventSource::FoodWaterExposure { consumption_id } => validate_id(consumption_id)?,
        }
        match &self.actor {
            WorldEventActor::Character { character_id } if *character_id == 0 => {
                return Err(WorldEventError::ZeroCharacterId);
            }
            WorldEventActor::Party { party_id } => validate_id(party_id)?,
            WorldEventActor::Character { .. } => {}
        }
        if self.subjects.is_empty() || self.subjects.len() > MAX_WORLD_EVENT_SUBJECTS {
            return Err(WorldEventError::SubjectCountOutOfBounds);
        }
        let mut prior = None;
        let mut unique = BTreeSet::new();
        for subject in &self.subjects {
            match subject {
                WorldEventSubject::Character { character_id } if *character_id == 0 => {
                    return Err(WorldEventError::ZeroCharacterId);
                }
                WorldEventSubject::Case { canonical_case_id } => validate_id(canonical_case_id)?,
                WorldEventSubject::LocalProblem { problem_id } => validate_id(problem_id)?,
                WorldEventSubject::Character { .. } => {}
            }
            if prior.is_some_and(|value| value >= subject) {
                return Err(WorldEventError::SubjectsNotCanonical);
            }
            if !unique.insert(subject) {
                return Err(WorldEventError::DuplicateSubject);
            }
            prior = Some(subject);
        }
        match &self.place {
            WorldEventPlace::Settlement { settlement_id } => validate_id(settlement_id)?,
            WorldEventPlace::Strategic { place_id } => validate_id(place_id)?,
        }
        match (&self.source, &self.actor, &self.payload) {
            (
                WorldEventSource::ForagingAction { request_id: source },
                WorldEventActor::Character { character_id },
                WorldEventPayloadRef::NoticedIllegalForaging { request_id: payload },
            ) if source == payload
                && self.subjects
                    == [WorldEventSubject::Character {
                        character_id: *character_id,
                    }] => {}
            (
                WorldEventSource::GeneratedCaseFinale { finale_id: source, .. },
                WorldEventActor::Party { .. },
                WorldEventPayloadRef::GeneratedCaseResolution {
                    canonical_case_id,
                    finale_id: payload,
                },
            ) if source == payload
                && self.subjects.iter().any(|subject| matches!(subject,
                    WorldEventSubject::Case { canonical_case_id: subject_id } if subject_id == canonical_case_id
                )) => {}
            (
                WorldEventSource::FoodWaterExposure { .. },
                WorldEventActor::Character { character_id },
                WorldEventPayloadRef::FoodWaterInfection {
                    carrier_id,
                    contribution_digest,
                    dose_microunits,
                    protected_dose_microunits,
                    immunity_milli,
                    prior_immunity_milli,
                    consumed_fraction_bps,
                    disease_id,
                },
            ) if *carrier_id != 0
                && *dose_microunits != 0
                && *protected_dose_microunits <= *dose_microunits
                && *immunity_milli <= 100_000
                && *prior_immunity_milli <= 100_000
                && *consumed_fraction_bps <= BASIS_POINTS_PER_WHOLE
                && validate_id(contribution_digest).is_ok()
                && validate_id(disease_id).is_ok()
                && self.subjects == [WorldEventSubject::Character { character_id: *character_id }] => {}
            _ => return Err(WorldEventError::InconsistentDomainReference),
        }
        Ok(())
    }
}

fn validate_id(value: &str) -> Result<(), WorldEventError> {
    if value.is_empty()
        || value.len() > MAX_WORLD_EVENT_ID_BYTES
        || value.chars().any(char::is_control)
    {
        Err(WorldEventError::InvalidIdentity)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldEventError {
    UnsupportedSchemaRevision,
    InvalidIdentity,
    ZeroCharacterId,
    SubjectCountOutOfBounds,
    SubjectsNotCanonical,
    DuplicateSubject,
    InconsistentDomainReference,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldEventReplayDecision {
    Apply,
    Replay,
    Collision,
}

/// Classify retry identity before consulting mutable domain state.
pub fn classify_world_event_retry(
    existing: Option<&WorldEventEnvelope>,
    proposed: &WorldEventEnvelope,
) -> WorldEventReplayDecision {
    match existing {
        None => WorldEventReplayDecision::Apply,
        Some(existing) if existing == proposed => WorldEventReplayDecision::Replay,
        Some(_) => WorldEventReplayDecision::Collision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infection_dose_quantization_rejects_invalid_values_and_round_trips() {
        let microunits = infection_dose_microunits(1.25).unwrap();

        assert_eq!(microunits, 1_250_000);
        assert_eq!(infection_dose_from_microunits(microunits), 1.25);
        assert_eq!(infection_dose_microunits(-0.01), None);
        assert_eq!(infection_dose_microunits(f32::NAN), None);
    }

    fn forage() -> WorldEventEnvelope {
        WorldEventEnvelope {
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
        }
    }

    #[test]
    fn exact_closed_foraging_envelope_is_valid() {
        assert_eq!(forage().validate(), Ok(()));
    }

    #[test]
    fn mismatched_payload_and_noncanonical_subjects_fail_closed() {
        let mut event = forage();
        event.payload = WorldEventPayloadRef::NoticedIllegalForaging {
            request_id: "other".into(),
        };
        assert_eq!(
            event.validate(),
            Err(WorldEventError::InconsistentDomainReference)
        );

        let mut event = forage();
        event
            .subjects
            .push(WorldEventSubject::Character { character_id: 7 });
        assert!(matches!(
            event.validate(),
            Err(WorldEventError::SubjectsNotCanonical | WorldEventError::DuplicateSubject)
        ));
    }

    #[test]
    fn retry_is_exact_and_collision_safe() {
        let event = forage();
        assert_eq!(
            classify_world_event_retry(None, &event),
            WorldEventReplayDecision::Apply
        );
        assert_eq!(
            classify_world_event_retry(Some(&event), &event.clone()),
            WorldEventReplayDecision::Replay
        );
        let mut collision = event.clone();
        collision.occurred_at_minute += 1;
        assert_eq!(
            classify_world_event_retry(Some(&event), &collision),
            WorldEventReplayDecision::Collision
        );
    }

    #[test]
    fn illegal_foraging_plan_matches_legacy_ids_amounts_and_order() {
        let plan = plan_noticed_illegal_foraging(7, "lubeck", "req", 100, 60);
        assert_eq!(plan.len(), 2);
        assert!(matches!(
            &plan[0],
            WorldEventConsequence::Reputation {
                event_id,
                character_id: 7,
                settlement_id,
                meaning: WorldEventReputationMeaning::IllegalForaging,
                source_id,
                raw_fame: 0,
                raw_infamy: 100,
                minute: 60,
            } if event_id == "forage:7:req" && settlement_id == "lubeck" && source_id == "req"
        ));
        assert!(matches!(
            &plan[1],
            WorldEventConsequence::DiscoveredOffense {
                offense_id,
                character_id: 7,
                settlement_id,
                kind: WorldEventOffenseKind::IllegalForaging,
                severity: 1,
                minute: 60,
            } if offense_id == "offense:forage:7:req" && settlement_id == "lubeck"
        ));
    }

    #[test]
    fn case_plan_preserves_problem_then_sorted_snapshots_and_fame() {
        let participants = canonical_case_resolution_participants([9, 4, 9], [2, 3]);
        assert_eq!(participants, [4, 9]);
        let plan = plan_generated_case_resolution(
            "canonical-case",
            "party",
            "lubeck",
            "outcome",
            Some("problem"),
            &participants,
            500,
            60,
        );
        assert!(matches!(
            &plan[0],
            WorldEventConsequence::LocalProblemOutcome {
                problem_id,
                source_outcome_id,
                minute: 60,
                mitigation_bps: 10_000,
                resolve: true,
            } if problem_id == "problem" && source_outcome_id == "outcome"
        ));
        for (pair, character_id) in plan[1..].as_chunks::<2>().0.iter().zip([4, 9]) {
            assert!(matches!(
                &pair[0],
                WorldEventConsequence::CaseParticipantSnapshot {
                    snapshot_id,
                    case_id,
                    character_id: actual,
                    party_id,
                    minute: 60,
                } if snapshot_id == &format!("canonical-case:{character_id}")
                    && case_id == "canonical-case" && *actual == character_id && party_id == "party"
            ));
            assert!(matches!(
                &pair[1],
                WorldEventConsequence::Reputation {
                    character_id: actual,
                    raw_fame: 500,
                    raw_infamy: 0,
                    ..
                } if *actual == character_id
            ));
        }
    }

    #[test]
    fn case_participants_use_living_fallback_and_allow_true_zero() {
        assert_eq!(
            canonical_case_resolution_participants([], [8, 3, 8]),
            [3, 8]
        );
        assert!(canonical_case_resolution_participants([], []).is_empty());
        assert!(
            plan_generated_case_resolution(
                "case",
                "party",
                "lubeck",
                "outcome",
                None,
                &[],
                500,
                60
            )
            .is_empty()
        );
    }

    #[test]
    fn planned_reputation_uses_the_unchanged_spillover_formula() {
        use crate::reputation::{ReputationEdge, ReputationSettlement, contributions};

        let plan = plan_noticed_illegal_foraging(7, "lubeck", "req", 100, 60);
        let WorldEventConsequence::Reputation {
            settlement_id,
            raw_fame,
            raw_infamy,
            ..
        } = &plan[0]
        else {
            unreachable!();
        };
        let settlements = vec![
            ReputationSettlement {
                id: "lubeck".into(),
                node_id: Some(1),
                population_level: 2,
                population_estimate: 3_000,
            },
            ReputationSettlement {
                id: "hamburg".into(),
                node_id: Some(2),
                population_level: 3,
                population_estimate: 6_000,
            },
        ];
        let edges = vec![ReputationEdge {
            from: 1,
            to: 2,
            length_m: 20_000,
        }];
        assert_eq!(
            contributions(settlement_id, *raw_fame, *raw_infamy, &settlements, &edges),
            contributions("lubeck", 0, 100, &settlements, &edges)
        );
    }
}
