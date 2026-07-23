//! Typed, deterministic generation for investigation-led quests.
//!
//! The catalog is deliberately static Rust data rather than an interpreted
//! rules language.  Relations are defined once, in the direction in which the
//! solver consumes them.  Diagnostic traces contain canonical truth and must
//! remain private to strategic authority and developer tools.

use crate::{
    bestiary::{ALL_REPORTS, ReportDescription, ThreatId, description_likelihood},
    case::{
        AssetId, Objective, ObjectiveExpression, ObjectiveId, ObjectivePath, ObjectiveRequirement,
        SubjectId,
    },
    investigation_action::{InvestigationActionKind, Terrain},
    local_problem::{Effects, EncounterArchetype, Scope, Symptom},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CATALOG_REVISION: &str = "questgen-2026-07-23.1";
pub const MAX_SOLVER_CANDIDATES: usize = 4_096;
pub const MAX_SOLVER_VISITED_NODES: usize = 16_384;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub String);
        impl $name {
            pub fn try_new(value: impl Into<String>) -> Result<Self, &'static str> {
                let value = value.into();
                if value.is_empty()
                    || value.len() > 256
                    || !value.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-' | b'.')
                    })
                {
                    return Err("invalid bounded quest-generation ID");
                }
                Ok(Self(value))
            }
            fn new(value: impl Into<String>) -> Self {
                Self::try_new(value).expect("static/generated quest ID")
            }
        }
    };
}
id_type!(ModuleId);
id_type!(RelationId);
id_type!(FactorId);
id_type!(BridgeId);
id_type!(SiteId);
id_type!(WitnessId);
id_type!(EvidenceId);
id_type!(ActionId);
id_type!(FinaleId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateFamily {
    RecurringDepredation,
    DisappearanceOrLoss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalCause {
    Hostile(ThreatId),
    VoluntaryDisappearance,
    ConcealmentByWitness,
    IncidentalLoss,
    FabricatedClaim,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteKind {
    Cave,
    Crypt,
    ForestCamp,
    OccupiedHouse,
    Riverside,
    Graveyard,
    Roadside,
    AbandonedFarm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteRole {
    Finale,
    Evidence,
    Decoy,
    LastKnown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WitnessDemographic {
    Child,
    Laborer,
    Merchant,
    Cleric,
    Guard,
    Noble,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Circumstance {
    NightWindow,
    SecretRiversideMeeting,
    AdultVenue,
    RoadJourney,
    GraveDuty,
    LivestockWatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reliability {
    Truthful,
    Mistaken,
    Evasive,
    Deceptive,
    PartlyTruthful,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Footprints,
    ClothScrap,
    BoneDust,
    BloodlessCorpse,
    DroppedToken,
    DragMarks,
    LedgerEntry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteClass {
    PhysicalTrail,
    PatternSurveillance,
    SocialInquiry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinaleKind {
    Defeat,
    DriveOff,
    Capture,
    Rescue,
    RetrieveReturn,
    Expose,
    Negotiate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedDialogueAction {
    Expose,
    ReturnAsset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Weight {
    pub plausibility: u32,
    pub curation: u32,
}
impl Weight {
    pub const fn new(plausibility: u32, curation: u32) -> Self {
        Self {
            plausibility,
            curation,
        }
    }
    pub fn combined(self) -> u64 {
        u64::from(self.plausibility) * u64::from(self.curation)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationContext {
    pub seed: u64,
    pub settlement_id: String,
    pub settlement_name: String,
    pub scope: Scope,
    pub ordinal: u16,
    pub now_minute: u64,
    pub requested_family: Option<TemplateFamily>,
    pub witness_candidates: Vec<WitnessCandidate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessCandidate {
    pub npc_id: String,
    pub demographic: WitnessDemographic,
    pub profession: String,
    pub visible_description: String,
    pub expected_location: String,
    pub allowed_circumstances: BTreeSet<Circumstance>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactorTrace {
    pub module_id: ModuleId,
    pub relation_id: RelationId,
    pub factor_ids: Vec<FactorId>,
    pub candidate_id: String,
    pub plausibility: u32,
    pub curation: u32,
    pub accepted: bool,
    pub hard_zero_reason: Option<String>,
    pub required_bridge: Option<BridgeId>,
    pub decision: TraceDecision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceDecision {
    Candidate,
    Bound,
    ForwardRejected,
    Backtracked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CausalBridge {
    pub id: BridgeId,
    pub explanation: String,
    pub event_id: String,
    pub evidence_id: EvidenceId,
    pub lead_summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalEvent {
    pub id: String,
    pub proposition_id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub occurred_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsequenceProfile {
    pub symptom: Symptom,
    pub effects: Effects,
    pub public_summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedSite {
    pub id: SiteId,
    pub kind: SiteKind,
    pub role: SiteRole,
    pub terrain: Terrain,
    pub safe_label: String,
    pub exact_location_initially_known: bool,
    pub is_true_location: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedArea {
    pub id: String,
    pub safe_label: String,
    pub terrain: Terrain,
    pub contains_site_ids: Vec<SiteId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestimonyDraft {
    pub proposition_id: String,
    pub reliability: Reliability,
    pub truthful_text: String,
    pub spoken_text: String,
    pub destination_stage: String,
    pub site_id: Option<SiteId>,
    /// Proposition superseded by this claim. Set only on the later correction.
    pub corrects_proposition_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessBinding {
    pub id: WitnessId,
    pub npc_id: String,
    pub demographic: WitnessDemographic,
    pub circumstance: Circumstance,
    pub description: ReportDescription,
    pub expected_location: String,
    pub visible_description: String,
    pub testimony: Vec<TestimonyDraft>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedEvidence {
    pub id: EvidenceId,
    pub kind: EvidenceKind,
    pub proposition_id: String,
    pub site_id: SiteId,
    pub safe_description: String,
    pub corrects_proposition_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedAction {
    pub id: ActionId,
    pub kind: InvestigationActionKind,
    pub route: RouteClass,
    pub target_kind: String,
    pub target_id: String,
    pub prerequisite: Option<ActionId>,
    pub alternate: ActionId,
    pub active_initially: bool,
    pub safe_summary: String,
    pub outputs: Vec<GeneratedActionOutput>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeneratedActionOutput {
    Destination {
        stage: GeneratedDestinationStage,
        site_id: Option<SiteId>,
    },
    Evidence {
        evidence_id: EvidenceId,
    },
    AmbushReady,
    Consequence {
        consequence: GeneratedActionConsequence,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedDestinationStage {
    Unknown,
    Textual,
    Landmark,
    ApproximateArea,
    RouteSegment,
    Exact,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeneratedActionConsequence {
    RetrieveAsset {
        asset_id: String,
        next_version: u32,
    },
    RescueSubject {
        subject_id: String,
        next_version: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedFinale {
    pub id: FinaleId,
    pub kind: FinaleKind,
    pub site_id: SiteId,
    pub hostile_group_id: Option<String>,
    pub subject_id: Option<String>,
    pub asset_id: Option<String>,
    pub strategic_outcome_compatible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedDialogueProducer {
    pub action: GeneratedDialogueAction,
    pub objective_id: ObjectiveId,
    pub recipient_npc_id: String,
    pub subject_ref: Option<String>,
    pub asset_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractDraft {
    pub issuer_npc_id: String,
    pub issuer_belief_title: String,
    pub issuer_belief_description: String,
    pub opposition_wording: String,
    pub opposition_count_wording: String,
    pub reward: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedCase {
    pub catalog_revision: String,
    pub generation_seed: u64,
    pub family: TemplateFamily,
    pub canonical_case_id: String,
    pub public_case_id: String,
    pub problem_id: String,
    pub cause: CanonicalCause,
    pub canonical_events: Vec<CanonicalEvent>,
    pub consequence: ConsequenceProfile,
    pub sites: Vec<GeneratedSite>,
    pub areas: Vec<GeneratedArea>,
    pub witnesses: Vec<WitnessBinding>,
    pub evidence: Vec<GeneratedEvidence>,
    pub actions: Vec<GeneratedAction>,
    pub objectives: ObjectiveExpression,
    pub custody: Vec<(String, SiteId)>,
    pub hostile_groups: Vec<(String, SiteId, ThreatId, u32)>,
    pub finales: Vec<GeneratedFinale>,
    pub dialogue_producers: Vec<GeneratedDialogueProducer>,
    pub contract: Option<ContractDraft>,
    pub bridges: Vec<CausalBridge>,
    /// Private diagnostic authority. Never place this in a public table/view.
    pub factor_trace: Vec<FactorTrace>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenerationError {
    NoCandidates {
        module: ModuleId,
        diagnostics: Vec<FactorTrace>,
    },
    CandidateLimit,
    InvalidManifest(Vec<String>),
}

#[derive(Clone)]
struct Candidate<T> {
    id: &'static str,
    value: T,
    weight: Weight,
    bridge: Option<&'static str>,
    impossible: Option<&'static str>,
    factors: Vec<&'static str>,
}

fn hash(seed: u64, domain: &str) -> u64 {
    domain.bytes().fold(seed ^ 0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
fn choose<T: Copy>(
    seed: u64,
    module: &str,
    relation: &str,
    candidates: &[Candidate<T>],
    trace: &mut Vec<FactorTrace>,
) -> Result<(T, Option<&'static str>), GenerationError> {
    if candidates.len() > MAX_SOLVER_CANDIDATES {
        return Err(GenerationError::CandidateLimit);
    }
    let mut total = 0u64;
    for c in candidates {
        let accepted = c.impossible.is_none() && c.weight.combined() > 0;
        trace.push(FactorTrace {
            module_id: ModuleId::new(module),
            relation_id: RelationId::new(relation),
            factor_ids: c.factors.iter().map(|f| FactorId::new(*f)).collect(),
            candidate_id: c.id.into(),
            plausibility: c.weight.plausibility,
            curation: c.weight.curation,
            accepted,
            hard_zero_reason: c.impossible.map(str::to_owned),
            required_bridge: c.bridge.map(BridgeId::new),
            decision: TraceDecision::Candidate,
        });
        if accepted {
            total = total.saturating_add(c.weight.combined());
        }
    }
    if total == 0 {
        return Err(GenerationError::NoCandidates {
            module: ModuleId::new(module),
            diagnostics: trace.clone(),
        });
    }
    let mut draw = hash(seed, module) % total;
    for c in candidates {
        let weight = if c.impossible.is_none() {
            c.weight.combined()
        } else {
            0
        };
        if draw < weight {
            return Ok((c.value, c.bridge));
        }
        draw -= weight;
    }
    unreachable!("bounded weighted draw must select")
}

fn weighted_order<T: Copy>(seed: u64, domain: &str, candidates: &[Candidate<T>]) -> Vec<usize> {
    let mut indices = (0..candidates.len())
        .filter(|index| {
            let c = &candidates[*index];
            c.impossible.is_none() && c.weight.combined() > 0
        })
        .collect::<Vec<_>>();
    // Integer-only deterministic weighted permutation. Larger weights yield a
    // smaller key on average, without duplicating inverse probability tables.
    indices.sort_by_key(|index| {
        let c = &candidates[*index];
        (
            hash(seed, &format!("{domain}:{}", c.id)) / c.weight.combined().max(1),
            c.id,
        )
    });
    indices
}

#[derive(Clone, Copy)]
struct SolvedVariables {
    family: TemplateFamily,
    cause: CanonicalCause,
    site: SiteKind,
    demographic: WitnessDemographic,
    circumstance: Circumstance,
    description: ReportDescription,
    site_bridge: Option<&'static str>,
    circumstance_bridge: Option<&'static str>,
    primary_witness: usize,
    secondary_witness: usize,
}

fn solve_variables(
    context: &GenerationContext,
    trace: &mut Vec<FactorTrace>,
) -> Result<SolvedVariables, GenerationError> {
    if context.witness_candidates.len() < 2 {
        return Err(GenerationError::InvalidManifest(vec![
            "generation requires two real persistent witness candidates".into(),
        ]));
    }
    let families = family_candidates();
    let family_indices = if let Some(requested) = context.requested_family {
        families
            .iter()
            .position(|c| c.value == requested)
            .into_iter()
            .collect()
    } else {
        weighted_order(context.seed, "family", &families)
    };
    let witnesses = deterministic_witness_order(context);
    let mut visited = 0usize;
    for family_index in family_indices {
        let family = families[family_index].value;
        let causes = cause_candidates(family);
        for cause_index in weighted_order(context.seed.rotate_left(3), "cause", &causes) {
            let cause = causes[cause_index].value;
            let sites = site_candidates(cause);
            for site_index in weighted_order(context.seed.rotate_left(7), "site", &sites) {
                let site = sites[site_index].value;
                for &primary_index in &witnesses {
                    let witness = &context.witness_candidates[primary_index];
                    let circumstances = circumstance_candidates(witness.demographic);
                    for circumstance_index in
                        weighted_order(context.seed.rotate_left(19), "circumstance", &circumstances)
                    {
                        visited += 1;
                        if visited > MAX_SOLVER_VISITED_NODES {
                            return Err(GenerationError::CandidateLimit);
                        }
                        let circumstance = circumstances[circumstance_index].value;
                        if !witness.allowed_circumstances.contains(&circumstance) {
                            trace.push(FactorTrace {
                                module_id: ModuleId::new("module.circumstance"),
                                relation_id: RelationId::new("relation.circumstance.npc_fact"),
                                factor_ids: vec![FactorId::new("factor.witness.actual_schedule")],
                                candidate_id: format!("{}:{circumstance:?}", witness.npc_id),
                                plausibility: 0,
                                curation: 0,
                                accepted: false,
                                hard_zero_reason: Some(
                                    "persistent NPC facts do not permit this circumstance".into(),
                                ),
                                required_bridge: None,
                                decision: TraceDecision::ForwardRejected,
                            });
                            continue;
                        }
                        let descriptions = description_candidates(cause);
                        let Some(description_index) = weighted_order(
                            context.seed.rotate_left(29),
                            "description",
                            &descriptions,
                        )
                        .first()
                        .copied() else {
                            trace.push(FactorTrace {
                                module_id: ModuleId::new("module.description"),
                                relation_id: RelationId::new("relation.description.cause"),
                                factor_ids: vec![FactorId::new("factor.description.forward_check")],
                                candidate_id: format!("{cause:?}"),
                                plausibility: 0,
                                curation: 0,
                                accepted: false,
                                hard_zero_reason: Some(
                                    "cause has no possible bestiary report".into(),
                                ),
                                required_bridge: None,
                                decision: TraceDecision::Backtracked,
                            });
                            continue;
                        };
                        let secondary_index = witnesses
                            .iter()
                            .copied()
                            .find(|index| *index != primary_index)
                            .expect("two witnesses checked");
                        for (module, id, bridge_id, factors) in [
                            (
                                "module.template",
                                format!("{family:?}"),
                                None,
                                families[family_index].factors.clone(),
                            ),
                            (
                                "module.cause",
                                format!("{cause:?}"),
                                None,
                                causes[cause_index].factors.clone(),
                            ),
                            (
                                "module.site",
                                format!("{site:?}"),
                                sites[site_index].bridge,
                                sites[site_index].factors.clone(),
                            ),
                            (
                                "module.witness",
                                witness.npc_id.clone(),
                                None,
                                vec!["factor.witness.actual_population"],
                            ),
                            (
                                "module.circumstance",
                                format!("{circumstance:?}"),
                                circumstances[circumstance_index].bridge,
                                circumstances[circumstance_index].factors.clone(),
                            ),
                            (
                                "module.description",
                                format!("{:?}", descriptions[description_index].value),
                                None,
                                descriptions[description_index].factors.clone(),
                            ),
                        ] {
                            trace.push(FactorTrace {
                                module_id: ModuleId::new(module),
                                relation_id: RelationId::new("relation.solver.binding"),
                                factor_ids: factors.into_iter().map(FactorId::new).collect(),
                                candidate_id: id,
                                plausibility: 100,
                                curation: 100,
                                accepted: true,
                                hard_zero_reason: None,
                                required_bridge: bridge_id.map(BridgeId::new),
                                decision: TraceDecision::Bound,
                            });
                        }
                        return Ok(SolvedVariables {
                            family,
                            cause,
                            site,
                            demographic: witness.demographic,
                            circumstance,
                            description: descriptions[description_index].value,
                            site_bridge: sites[site_index].bridge,
                            circumstance_bridge: circumstances[circumstance_index].bridge,
                            primary_witness: primary_index,
                            secondary_witness: secondary_index,
                        });
                    }
                    trace.push(FactorTrace {
                        module_id: ModuleId::new("module.witness"),
                        relation_id: RelationId::new("relation.solver.backtrack"),
                        factor_ids: vec![FactorId::new("factor.no_valid_circumstance")],
                        candidate_id: witness.npc_id.clone(),
                        plausibility: 0,
                        curation: 0,
                        accepted: false,
                        hard_zero_reason: Some("witness has no compatible circumstance".into()),
                        required_bridge: None,
                        decision: TraceDecision::Backtracked,
                    });
                }
            }
        }
    }
    Err(GenerationError::NoCandidates {
        module: ModuleId::new("module.quest"),
        diagnostics: trace.clone(),
    })
}

fn family_candidates() -> [Candidate<TemplateFamily>; 2] {
    [
        Candidate {
            id: "family.recurring_depredation",
            value: TemplateFamily::RecurringDepredation,
            weight: Weight::new(100, 100),
            bridge: None,
            impossible: None,
            factors: vec!["factor.family.rotation"],
        },
        Candidate {
            id: "family.disappearance_or_loss",
            value: TemplateFamily::DisappearanceOrLoss,
            weight: Weight::new(100, 100),
            bridge: None,
            impossible: None,
            factors: vec!["factor.family.rotation"],
        },
    ]
}

fn cause_candidates(family: TemplateFamily) -> Vec<Candidate<CanonicalCause>> {
    let loss = family == TemplateFamily::DisappearanceOrLoss;
    let mut values = vec![
        (ThreatId::Bandit, 75, 80),
        (ThreatId::Goblin, if loss { 30 } else { 70 }, 75),
        (ThreatId::Ghoul, 40, 70),
        (ThreatId::Skeleton, 35, 70),
        (ThreatId::Werewolf, if loss { 25 } else { 45 }, 60),
        (ThreatId::Smuggler, if loss { 60 } else { 25 }, 65),
        (ThreatId::Wolf, if loss { 20 } else { 65 }, 65),
    ]
    .into_iter()
    .map(|(threat, p, c)| Candidate {
        id: threat.as_str(),
        value: CanonicalCause::Hostile(threat),
        weight: Weight::new(p, c),
        bridge: None,
        impossible: None,
        factors: vec!["factor.cause.bestiary"],
    })
    .collect::<Vec<_>>();
    if loss {
        values.extend([
            Candidate {
                id: "cause.concealment",
                value: CanonicalCause::ConcealmentByWitness,
                weight: Weight::new(35, 75),
                bridge: None,
                impossible: None,
                factors: vec!["factor.cause.nonhostile"],
            },
            Candidate {
                id: "cause.incidental_loss",
                value: CanonicalCause::IncidentalLoss,
                weight: Weight::new(40, 65),
                bridge: None,
                impossible: None,
                factors: vec!["factor.cause.nonhostile"],
            },
            Candidate {
                id: "cause.fabricated",
                value: CanonicalCause::FabricatedClaim,
                weight: Weight::new(20, 55),
                bridge: None,
                impossible: None,
                factors: vec!["factor.cause.nonhostile"],
            },
        ]);
    }
    values
}

fn site_candidates(cause: CanonicalCause) -> Vec<Candidate<SiteKind>> {
    use SiteKind as S;
    [
        S::Cave,
        S::Crypt,
        S::ForestCamp,
        S::OccupiedHouse,
        S::Riverside,
        S::Graveyard,
        S::Roadside,
        S::AbandonedFarm,
    ]
    .into_iter()
    .map(|site| {
        let (p, bridge, impossible, factor) = match (cause, site) {
            (CanonicalCause::Hostile(ThreatId::Skeleton), S::Crypt)
            | (CanonicalCause::Hostile(ThreatId::Ghoul), S::Graveyard) => {
                (95, None, None, "factor.site.natural_habitat")
            }
            (CanonicalCause::Hostile(ThreatId::Bandit), S::ForestCamp)
            | (CanonicalCause::Hostile(ThreatId::Goblin), S::Cave)
            | (CanonicalCause::Hostile(ThreatId::Wolf), S::AbandonedFarm) => {
                (80, None, None, "factor.site.common")
            }
            (CanonicalCause::Hostile(ThreatId::Werewolf), S::OccupiedHouse)
            | (CanonicalCause::Hostile(ThreatId::Smuggler), S::Riverside) => {
                (90, None, None, "factor.site.concealment")
            }
            (CanonicalCause::Hostile(ThreatId::Skeleton), S::OccupiedHouse) => (
                3,
                Some("bridge.skeletons_occupied_house"),
                None,
                "factor.site.rare_bridge",
            ),
            (CanonicalCause::Hostile(ThreatId::Wolf), S::Crypt) => (
                0,
                None,
                Some("quadruped pack cannot maintain a sealed crypt"),
                "factor.site.impossible",
            ),
            (
                CanonicalCause::VoluntaryDisappearance | CanonicalCause::ConcealmentByWitness,
                S::OccupiedHouse | S::Riverside,
            ) => (85, None, None, "factor.site.social"),
            (CanonicalCause::IncidentalLoss, S::Roadside | S::Riverside) => {
                (80, None, None, "factor.site.accident")
            }
            (CanonicalCause::FabricatedClaim, S::OccupiedHouse) => {
                (80, None, None, "factor.site.fabrication")
            }
            (_, S::OccupiedHouse) => (12, None, None, "factor.site.unusual"),
            (_, S::Roadside) => (25, None, None, "factor.site.transit"),
            _ => (20, None, None, "factor.site.possible"),
        };
        Candidate {
            id: site_id(site),
            value: site,
            weight: Weight::new(p, 70),
            bridge,
            impossible,
            factors: vec![factor],
        }
    })
    .collect()
}

fn circumstance_candidates(demo: WitnessDemographic) -> Vec<Candidate<Circumstance>> {
    use Circumstance as C;
    [
        C::NightWindow,
        C::SecretRiversideMeeting,
        C::AdultVenue,
        C::RoadJourney,
        C::GraveDuty,
        C::LivestockWatch,
    ]
    .into_iter()
    .map(|circ| {
        let (p, bridge, impossible, factor) = match (demo, circ) {
            (WitnessDemographic::Child, C::AdultVenue) => (
                2,
                Some("bridge.child_at_adult_venue"),
                None,
                "factor.witness.rare_venue",
            ),
            (WitnessDemographic::Cleric, C::AdultVenue) => (
                0,
                None,
                Some("assigned cleric witness is not present in the adult venue"),
                "factor.witness.impossible_venue",
            ),
            (WitnessDemographic::Child, C::NightWindow) => {
                (90, None, None, "factor.witness.household")
            }
            (_, C::RoadJourney) => (55, None, None, "factor.witness.travel"),
            (_, C::SecretRiversideMeeting) => (25, None, None, "factor.witness.private"),
            _ => (35, None, None, "factor.witness.general"),
        };
        Candidate {
            id: circumstance_id(circ),
            value: circ,
            weight: Weight::new(p, 70),
            bridge,
            impossible,
            factors: vec![factor],
        }
    })
    .collect()
}

fn description_candidates(cause: CanonicalCause) -> Vec<Candidate<ReportDescription>> {
    ALL_REPORTS
        .iter()
        .copied()
        .map(|report| {
            let p = match cause {
                CanonicalCause::Hostile(threat) => description_likelihood(threat, report),
                CanonicalCause::VoluntaryDisappearance
                | CanonicalCause::ConcealmentByWitness
                | CanonicalCause::FabricatedClaim => {
                    if matches!(
                        report,
                        ReportDescription::ArmedPeople | ReportDescription::UnseenNightVisitor
                    ) {
                        55
                    } else {
                        5
                    }
                }
                CanonicalCause::IncidentalLoss => {
                    if report == ReportDescription::UnseenNightVisitor {
                        60
                    } else {
                        3
                    }
                }
            };
            Candidate {
                id: report_id(report),
                value: report,
                weight: Weight::new(p, 80),
                bridge: None,
                impossible: (p == 0).then_some("bestiary forward description likelihood is zero"),
                factors: vec!["factor.description.bestiary_forward_likelihood"],
            }
        })
        .collect()
}

fn site_id(site: SiteKind) -> &'static str {
    match site {
        SiteKind::Cave => "site.cave",
        SiteKind::Crypt => "site.crypt",
        SiteKind::ForestCamp => "site.forest_camp",
        SiteKind::OccupiedHouse => "site.occupied_house",
        SiteKind::Riverside => "site.riverside",
        SiteKind::Graveyard => "site.graveyard",
        SiteKind::Roadside => "site.roadside",
        SiteKind::AbandonedFarm => "site.abandoned_farm",
    }
}
fn circumstance_id(v: Circumstance) -> &'static str {
    match v {
        Circumstance::NightWindow => "circumstance.night_window",
        Circumstance::SecretRiversideMeeting => "circumstance.secret_riverside",
        Circumstance::AdultVenue => "circumstance.adult_venue",
        Circumstance::RoadJourney => "circumstance.road",
        Circumstance::GraveDuty => "circumstance.grave_duty",
        Circumstance::LivestockWatch => "circumstance.livestock_watch",
    }
}
fn report_id(v: ReportDescription) -> &'static str {
    match v {
        ReportDescription::ArmedPeople => "description.armed_people",
        ReportDescription::SmallUprightFigures => "description.small_upright",
        ReportDescription::LargeUprightBeast => "description.large_upright",
        ReportDescription::GauntHuman => "description.gaunt_human",
        ReportDescription::WalkingDead => "description.walking_dead",
        ReportDescription::LargeAnimal => "description.large_animal",
        ReportDescription::DoglikeBeast => "description.doglike",
        ReportDescription::UnseenNightVisitor => "description.unseen",
    }
}

fn terrain(site: SiteKind) -> Terrain {
    match site {
        SiteKind::Cave | SiteKind::Crypt => Terrain::Underground,
        SiteKind::ForestCamp | SiteKind::AbandonedFarm => Terrain::Forest,
        SiteKind::OccupiedHouse | SiteKind::Graveyard => Terrain::Settlement,
        SiteKind::Riverside | SiteKind::Roadside => Terrain::Road,
    }
}
fn label(site: SiteKind) -> &'static str {
    match site {
        SiteKind::Cave => "a cave beyond the fields",
        SiteKind::Crypt => "the old crypt",
        SiteKind::ForestCamp => "a camp in the woods",
        SiteKind::OccupiedHouse => "an occupied house",
        SiteKind::Riverside => "a secluded bend in the river",
        SiteKind::Graveyard => "the old graveyard",
        SiteKind::Roadside => "a lonely stretch of road",
        SiteKind::AbandonedFarm => "an abandoned farm",
    }
}

fn bridge(id: &str, prefix: &str, _now: u64) -> CausalBridge {
    match id {
        "bridge.skeletons_occupied_house" => CausalBridge {
            id: BridgeId::new(id),
            explanation: "A graverobber moved animated remains into a shuttered house.".into(),
            event_id: format!("{prefix}:event:bridge:skeleton_house"),
            evidence_id: EvidenceId::new(format!("{prefix}:evidence:grave_clay")),
            lead_summary: "Grave clay and cart ruts connect the house to the crypt.".into(),
        },
        "bridge.child_at_adult_venue" => CausalBridge {
            id: BridgeId::new(id),
            explanation: "The child was fetching an adult relative from outside the venue.".into(),
            event_id: format!("{prefix}:event:bridge:child_venue"),
            evidence_id: EvidenceId::new(format!("{prefix}:evidence:errand_token")),
            lead_summary: "An errand token corroborates why the child waited outside.".into(),
        },
        _ => CausalBridge {
            id: BridgeId::new(id),
            explanation: "A rare causal link makes the combination possible.".into(),
            event_id: format!("{prefix}:event:bridge"),
            evidence_id: EvidenceId::new(format!("{prefix}:evidence:bridge")),
            lead_summary: "A corroborating clue explains the unusual combination.".into(),
        },
    }
}

fn consequence(cause: CanonicalCause, family: TemplateFamily) -> ConsequenceProfile {
    let (symptom, effects, summary) = match (family, cause) {
        (
            TemplateFamily::RecurringDepredation,
            CanonicalCause::Hostile(ThreatId::Ghoul | ThreatId::Werewolf),
        ) => (
            Symptom::NightScreams,
            Effects {
                buy_bps: 400,
                sell_penalty_bps: 200,
                encounter_frequency_bps: 700,
                encounter_archetype: Some(EncounterArchetype::Undead),
                disease_intensity: 180,
            },
            "Locals report troubling sounds and disappearances after dark.",
        ),
        (
            TemplateFamily::RecurringDepredation,
            CanonicalCause::Hostile(ThreatId::Wolf | ThreatId::Goblin),
        ) => (
            Symptom::VanishedLivestock,
            Effects {
                buy_bps: 700,
                sell_penalty_bps: 300,
                encounter_frequency_bps: 1000,
                encounter_archetype: Some(EncounterArchetype::Goblins),
                disease_intensity: 0,
            },
            "Livestock have been disappearing from nearby holdings.",
        ),
        (TemplateFamily::RecurringDepredation, _) => (
            Symptom::MissingCaravans,
            Effects {
                buy_bps: 1200,
                sell_penalty_bps: 500,
                encounter_frequency_bps: 1500,
                encounter_archetype: Some(EncounterArchetype::Bandits),
                disease_intensity: 0,
            },
            "Several expected caravans have not arrived.",
        ),
        (TemplateFamily::DisappearanceOrLoss, _) => (
            Symptom::EmptyStalls,
            Effects {
                buy_bps: 900,
                sell_penalty_bps: 400,
                encounter_frequency_bps: 500,
                encounter_archetype: None,
                disease_intensity: 0,
            },
            "A disappearance has disrupted work and trade, but nobody agrees on the cause.",
        ),
    };
    ConsequenceProfile {
        symptom,
        effects,
        public_summary: summary.into(),
    }
}

fn deterministic_witness_order(context: &GenerationContext) -> Vec<usize> {
    let mut indices = (0..context.witness_candidates.len()).collect::<Vec<_>>();
    indices.sort_by_key(|index| {
        hash(
            context.seed,
            &format!("witness:{}", context.witness_candidates[*index].npc_id),
        )
    });
    indices
}

fn build_actions(
    prefix: &str,
    family: TemplateFamily,
    finale: &SiteId,
    evidence_site: &SiteId,
    area_id: &str,
    witness_npc_id: &str,
) -> Vec<GeneratedAction> {
    let make = |name: &str,
                kind,
                route,
                target_kind: &str,
                target: String,
                prerequisite: Option<&str>,
                alternate: &str,
                active,
                summary: &str,
                outputs: Vec<GeneratedActionOutput>| GeneratedAction {
        id: ActionId::new(format!("{prefix}:action:{name}")),
        kind,
        route,
        target_kind: target_kind.into(),
        target_id: target,
        prerequisite: prerequisite.map(|p| ActionId::new(format!("{prefix}:action:{p}"))),
        alternate: ActionId::new(format!("{prefix}:action:{alternate}")),
        active_initially: active,
        safe_summary: summary.into(),
        outputs,
    };
    match family {
        TemplateFamily::RecurringDepredation => vec![
            make(
                "approach",
                InvestigationActionKind::ApproachLead,
                RouteClass::PhysicalTrail,
                "area",
                area_id.into(),
                None,
                "watch",
                true,
                "Approach the last reported incident.",
                vec![GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::ApproximateArea,
                    site_id: None,
                }],
            ),
            make(
                "search",
                InvestigationActionKind::SearchArea,
                RouteClass::PhysicalTrail,
                "area",
                area_id.into(),
                Some("approach"),
                "patrol",
                false,
                "Search for physical traces.",
                vec![GeneratedActionOutput::Evidence {
                    evidence_id: EvidenceId::new(format!("{prefix}:evidence:tracks")),
                }],
            ),
            make(
                "follow",
                InvestigationActionKind::FollowTracks,
                RouteClass::PhysicalTrail,
                "site",
                finale.0.clone(),
                Some("search"),
                "ambush",
                false,
                "Follow the physical trail.",
                vec![GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::Exact,
                    site_id: Some(finale.clone()),
                }],
            ),
            make(
                "watch",
                InvestigationActionKind::Watch,
                RouteClass::PatternSurveillance,
                "contact",
                witness_npc_id.into(),
                None,
                "approach",
                true,
                "Watch where incidents recur.",
                vec![GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::Textual,
                    site_id: None,
                }],
            ),
            make(
                "patrol",
                InvestigationActionKind::Patrol,
                RouteClass::PatternSurveillance,
                "area",
                area_id.into(),
                Some("watch"),
                "search",
                false,
                "Patrol at the reported time.",
                vec![GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::RouteSegment,
                    site_id: Some(finale.clone()),
                }],
            ),
            make(
                "ambush",
                InvestigationActionKind::LayAmbush,
                RouteClass::PatternSurveillance,
                "site",
                finale.0.clone(),
                Some("patrol"),
                "follow",
                false,
                "Lay an ambush along the established route.",
                vec![
                    GeneratedActionOutput::AmbushReady,
                    GeneratedActionOutput::Destination {
                        stage: GeneratedDestinationStage::Exact,
                        site_id: Some(finale.clone()),
                    },
                ],
            ),
        ],
        TemplateFamily::DisappearanceOrLoss => vec![
            make(
                "inspect_last_known",
                InvestigationActionKind::InspectSite,
                RouteClass::PhysicalTrail,
                "site",
                evidence_site.0.clone(),
                None,
                "locate_contact",
                true,
                "Inspect the last-known place.",
                vec![GeneratedActionOutput::Evidence {
                    evidence_id: EvidenceId::new(format!("{prefix}:evidence:tracks")),
                }],
            ),
            make(
                "follow",
                InvestigationActionKind::FollowTracks,
                RouteClass::PhysicalTrail,
                "site",
                finale.0.clone(),
                Some("inspect_last_known"),
                "approach_social",
                false,
                "Follow traces away from the last-known place.",
                vec![GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::Exact,
                    site_id: Some(finale.clone()),
                }],
            ),
            make(
                "locate_contact",
                InvestigationActionKind::LocateContact,
                RouteClass::SocialInquiry,
                "contact",
                witness_npc_id.into(),
                None,
                "inspect_last_known",
                true,
                "Find the referred witness.",
                vec![GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::Textual,
                    site_id: None,
                }],
            ),
            make(
                "approach_social",
                InvestigationActionKind::ApproachLead,
                RouteClass::SocialInquiry,
                "site",
                finale.0.clone(),
                Some("locate_contact"),
                "follow",
                false,
                "Approach the social lead or fence.",
                vec![GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::Exact,
                    site_id: Some(finale.clone()),
                }],
            ),
        ],
    }
}

pub fn generate(context: &GenerationContext) -> Result<GeneratedCase, GenerationError> {
    let mut trace = Vec::new();
    let solved = solve_variables(context, &mut trace)?;
    let SolvedVariables {
        family,
        cause,
        site,
        demographic,
        circumstance,
        description,
        site_bridge,
        circumstance_bridge: circ_bridge,
        primary_witness,
        secondary_witness,
    } = solved;
    let primary = &context.witness_candidates[primary_witness];
    let secondary = &context.witness_candidates[secondary_witness];
    let prefix = format!(
        "case:{:016x}",
        hash(
            context.seed,
            &format!("{}:{}", context.settlement_id, context.ordinal)
        )
    );
    let problem_id = format!(
        "problem:{:016x}",
        hash(context.seed, &format!("problem:{}", context.ordinal))
    );
    let finale_site = SiteId::new(format!("{prefix}:site:finale"));
    let evidence_site = SiteId::new(format!("{prefix}:site:evidence"));
    let decoy_site = SiteId::new(format!("{prefix}:site:decoy"));
    let witness1 = WitnessId::new(format!("{prefix}:witness:primary"));
    let witness2 = WitnessId::new(format!("{prefix}:witness:corroborating"));
    let npc1 = primary.npc_id.clone();
    let npc2 = secondary.npc_id.clone();
    let false_statement = format!("It looked like {:?}, near {}.", description, label(site));
    let true_statement = format!(
        "I saw signs pointing toward {}, but I could not identify the culprit.",
        label(site)
    );
    let description_prop = format!("{prefix}:proposition:description");
    let correction_prop = format!("{prefix}:proposition:location:corrected");
    let reliability = match hash(context.seed, "reliability") % 5 {
        0 => Reliability::Truthful,
        1 => Reliability::Mistaken,
        2 => Reliability::Evasive,
        3 => Reliability::Deceptive,
        _ => Reliability::PartlyTruthful,
    };
    let sites = vec![
        GeneratedSite {
            id: finale_site.clone(),
            kind: site,
            role: SiteRole::Finale,
            terrain: terrain(site),
            safe_label: label(site).into(),
            exact_location_initially_known: false,
            is_true_location: true,
        },
        GeneratedSite {
            id: evidence_site.clone(),
            kind: if family == TemplateFamily::RecurringDepredation {
                SiteKind::Roadside
            } else {
                SiteKind::OccupiedHouse
            },
            role: if family == TemplateFamily::RecurringDepredation {
                SiteRole::Evidence
            } else {
                SiteRole::LastKnown
            },
            terrain: Terrain::Settlement,
            safe_label: if family == TemplateFamily::RecurringDepredation {
                "the latest incident site".into()
            } else {
                "the last-known place".into()
            },
            exact_location_initially_known: true,
            is_true_location: false,
        },
        GeneratedSite {
            id: decoy_site.clone(),
            kind: SiteKind::Riverside,
            role: SiteRole::Decoy,
            terrain: Terrain::Road,
            safe_label: "a plausible but unconfirmed riverside place".into(),
            exact_location_initially_known: false,
            is_true_location: false,
        },
    ];
    let witnesses = vec![
        WitnessBinding {
            id: witness1.clone(),
            npc_id: npc1,
            demographic,
            circumstance,
            description,
            expected_location: primary.expected_location.clone(),
            visible_description: primary.visible_description.clone(),
            testimony: vec![TestimonyDraft {
                proposition_id: description_prop.clone(),
                reliability,
                truthful_text: true_statement,
                spoken_text: false_statement,
                destination_stage: if reliability == Reliability::Truthful {
                    "approximate_area"
                } else {
                    "exact_believed"
                }
                .into(),
                site_id: Some(if reliability == Reliability::Truthful {
                    finale_site.clone()
                } else {
                    decoy_site.clone()
                }),
                corrects_proposition_id: None,
            }],
        },
        WitnessBinding {
            id: witness2.clone(),
            npc_id: npc2,
            demographic: secondary.demographic,
            circumstance: Circumstance::RoadJourney,
            description,
            expected_location: secondary.expected_location.clone(),
            visible_description: secondary.visible_description.clone(),
            testimony: vec![TestimonyDraft {
                proposition_id: description_prop.clone(),
                reliability: Reliability::Truthful,
                truthful_text: "The earlier location does not fit the tracks; they lead elsewhere."
                    .into(),
                spoken_text:
                    "Those tracks turn away from the river and continue toward the true site."
                        .into(),
                destination_stage: "route_segment".into(),
                site_id: Some(finale_site.clone()),
                corrects_proposition_id: Some(description_prop.clone()),
            }],
        },
    ];
    let mut evidence = vec![
        GeneratedEvidence {
            id: EvidenceId::new(format!("{prefix}:evidence:tracks")),
            kind: EvidenceKind::Footprints,
            proposition_id: correction_prop.clone(),
            site_id: evidence_site.clone(),
            safe_description:
                "Tracks preserve direction and gait without identifying the creature outright."
                    .into(),
            corrects_proposition_id: Some(format!("{prefix}:proposition:description")),
        },
        GeneratedEvidence {
            id: EvidenceId::new(format!("{prefix}:evidence:token")),
            kind: EvidenceKind::DroppedToken,
            proposition_id: format!("{prefix}:proposition:association"),
            site_id: decoy_site.clone(),
            safe_description:
                "A dropped token links the report to another person, not necessarily the culprit."
                    .into(),
            corrects_proposition_id: None,
        },
    ];
    let area_id = format!("{prefix}:area:incident");
    let hostile_id = format!("hostile-group:{}", finale_site.0);
    let id_suffix = prefix.trim_start_matches("case:");
    let subject = SubjectId::new(format!("subject:{id_suffix}")).expect("generated subject id");
    let asset = AssetId::new(format!("asset:{id_suffix}")).expect("generated asset id");
    let mut actions = build_actions(
        &prefix,
        family,
        &finale_site,
        &evidence_site,
        &area_id,
        &primary.npc_id,
    );
    let issuer = context
        .witness_candidates
        .get(2)
        .unwrap_or(secondary)
        .npc_id
        .clone();
    let (objectives, finales, custody, dialogue_producers) = match family {
        TemplateFamily::RecurringDepredation => (
            ObjectiveExpression::new(vec![
                ObjectivePath {
                    objectives: vec![Objective {
                        id: ObjectiveId::new(format!("objective:{id_suffix}:defeat")).unwrap(),
                        requirement: ObjectiveRequirement::Defeat {
                            hostile_group_id: hostile_id.clone(),
                            count: 1,
                        },
                    }],
                },
                ObjectivePath {
                    objectives: vec![Objective {
                        id: ObjectiveId::new(format!("objective:{id_suffix}:driveoff")).unwrap(),
                        requirement: ObjectiveRequirement::DriveOff {
                            hostile_group_id: hostile_id.clone(),
                        },
                    }],
                },
            ])
            .expect("generated objective"),
            vec![
                GeneratedFinale {
                    id: FinaleId::new(format!("{prefix}:finale:defeat")),
                    kind: FinaleKind::Defeat,
                    site_id: finale_site.clone(),
                    hostile_group_id: Some(hostile_id.clone()),
                    subject_id: None,
                    asset_id: None,
                    strategic_outcome_compatible: true,
                },
                GeneratedFinale {
                    id: FinaleId::new(format!("{prefix}:finale:driveoff")),
                    kind: FinaleKind::DriveOff,
                    site_id: finale_site.clone(),
                    hostile_group_id: Some(hostile_id.clone()),
                    subject_id: None,
                    asset_id: None,
                    strategic_outcome_compatible: true,
                },
            ],
            vec![],
            vec![],
        ),
        TemplateFamily::DisappearanceOrLoss => match cause {
            CanonicalCause::Hostile(_) | CanonicalCause::ConcealmentByWitness => {
                let objective_id =
                    ObjectiveId::new(format!("objective:{id_suffix}:rescue")).unwrap();
                for action in actions.iter_mut().filter(|action| {
                    action.id.0.ends_with(":action:follow")
                        || action.id.0.ends_with(":action:approach_social")
                }) {
                    action.outputs.push(GeneratedActionOutput::Consequence {
                        consequence: GeneratedActionConsequence::RescueSubject {
                            subject_id: subject.as_str().into(),
                            next_version: 1,
                        },
                    });
                }
                (
                    ObjectiveExpression::new(vec![ObjectivePath {
                        objectives: vec![Objective {
                            id: objective_id,
                            requirement: ObjectiveRequirement::Rescue {
                                subject_id: subject.clone(),
                            },
                        }],
                    }])
                    .expect("generated rescue objective"),
                    vec![GeneratedFinale {
                        id: FinaleId::new(format!("{prefix}:finale:rescue")),
                        kind: FinaleKind::Rescue,
                        site_id: finale_site.clone(),
                        hostile_group_id: matches!(cause, CanonicalCause::Hostile(_))
                            .then_some(hostile_id.clone()),
                        subject_id: Some(subject.as_str().into()),
                        asset_id: None,
                        strategic_outcome_compatible: true,
                    }],
                    vec![(subject.as_str().into(), finale_site.clone())],
                    vec![],
                )
            }
            CanonicalCause::IncidentalLoss => {
                for action in actions.iter_mut().filter(|action| {
                    action.id.0.ends_with(":action:follow")
                        || action.id.0.ends_with(":action:approach_social")
                }) {
                    action.outputs.push(GeneratedActionOutput::Consequence {
                        consequence: GeneratedActionConsequence::RetrieveAsset {
                            asset_id: asset.as_str().into(),
                            next_version: 1,
                        },
                    });
                }
                let retrieve_id =
                    ObjectiveId::new(format!("objective:{id_suffix}:retrieve")).unwrap();
                let return_id = ObjectiveId::new(format!("objective:{id_suffix}:return")).unwrap();
                (
                    ObjectiveExpression::new(vec![ObjectivePath {
                        objectives: vec![
                            Objective {
                                id: retrieve_id,
                                requirement: ObjectiveRequirement::Retrieve {
                                    asset_id: asset.clone(),
                                },
                            },
                            Objective {
                                id: return_id.clone(),
                                requirement: ObjectiveRequirement::Return {
                                    asset_id: asset.clone(),
                                    custodian_id: issuer.clone(),
                                },
                            },
                        ],
                    }])
                    .expect("generated recovery objective"),
                    vec![GeneratedFinale {
                        id: FinaleId::new(format!("{prefix}:finale:return")),
                        kind: FinaleKind::RetrieveReturn,
                        site_id: finale_site.clone(),
                        hostile_group_id: None,
                        subject_id: None,
                        asset_id: Some(asset.as_str().into()),
                        strategic_outcome_compatible: false,
                    }],
                    vec![(asset.as_str().into(), finale_site.clone())],
                    vec![GeneratedDialogueProducer {
                        action: GeneratedDialogueAction::ReturnAsset,
                        objective_id: return_id,
                        recipient_npc_id: issuer.clone(),
                        subject_ref: None,
                        asset_id: Some(asset.as_str().into()),
                    }],
                )
            }
            CanonicalCause::FabricatedClaim => {
                let objective_id =
                    ObjectiveId::new(format!("objective:{id_suffix}:expose")).unwrap();
                (
                    ObjectiveExpression::new(vec![ObjectivePath {
                        objectives: vec![Objective {
                            id: objective_id.clone(),
                            requirement: ObjectiveRequirement::Expose {
                                subject_ref: description_prop.clone(),
                            },
                        }],
                    }])
                    .expect("generated exposure objective"),
                    vec![GeneratedFinale {
                        id: FinaleId::new(format!("{prefix}:finale:expose")),
                        kind: FinaleKind::Expose,
                        site_id: finale_site.clone(),
                        hostile_group_id: None,
                        subject_id: Some(description_prop.clone()),
                        asset_id: None,
                        strategic_outcome_compatible: false,
                    }],
                    vec![],
                    vec![GeneratedDialogueProducer {
                        action: GeneratedDialogueAction::Expose,
                        objective_id,
                        recipient_npc_id: issuer.clone(),
                        subject_ref: Some(description_prop.clone()),
                        asset_id: None,
                    }],
                )
            }
            CanonicalCause::VoluntaryDisappearance => unreachable!(
                "voluntary disappearance is excluded until locate/report producers exist"
            ),
        },
    };
    let mut bridges = Vec::new();
    for key in [site_bridge, circ_bridge].into_iter().flatten() {
        if !bridges.iter().any(|b: &CausalBridge| b.id.0 == key) {
            bridges.push(bridge(key, &prefix, context.now_minute));
        }
    }
    for item in &bridges {
        if !evidence
            .iter()
            .any(|candidate| candidate.id == item.evidence_id)
        {
            evidence.push(GeneratedEvidence {
                id: item.evidence_id.clone(),
                kind: EvidenceKind::DroppedToken,
                proposition_id: format!("{}:proposition", item.event_id),
                site_id: evidence_site.clone(),
                safe_description: item.lead_summary.clone(),
                corrects_proposition_id: None,
            });
        }
    }
    let canonical_events = vec![CanonicalEvent {
        id: format!("{prefix}:event:incident"),
        proposition_id: format!("{prefix}:proposition:truth"),
        subject: format!("{cause:?}"),
        predicate: "caused".into(),
        object: format!("{:?}", consequence(cause, family).symptom),
        occurred_at: context.now_minute.saturating_sub(180),
    }]
    .into_iter()
    .chain(bridges.iter().map(|b| CanonicalEvent {
        id: b.event_id.clone(),
        proposition_id: format!("{}:proposition", b.event_id),
        subject: "causal bridge".into(),
        predicate: "explains".into(),
        object: b.explanation.clone(),
        occurred_at: context.now_minute.saturating_sub(120),
    }))
    .collect();
    let manifest = GeneratedCase {
        catalog_revision: CATALOG_REVISION.into(),
        generation_seed: context.seed,
        family,
        canonical_case_id: prefix.clone(),
        public_case_id: format!(
            "journal:{:016x}",
            hash(context.seed, &format!("public:{}", context.ordinal))
        ),
        problem_id,
        cause,
        canonical_events,
        consequence: consequence(cause, family),
        sites,
        areas: vec![GeneratedArea {
            id: area_id,
            safe_label: "the area described by local accounts".into(),
            terrain: Terrain::Settlement,
            contains_site_ids: vec![evidence_site.clone(), decoy_site],
        }],
        witnesses,
        evidence,
        actions,
        objectives,
        custody,
        hostile_groups: match cause {
            CanonicalCause::Hostile(threat) => vec![(hostile_id, finale_site, threat, 1)],
            _ => vec![],
        },
        finales,
        dialogue_producers,
        contract: Some(ContractDraft {
            issuer_npc_id: issuer,
            issuer_belief_title: "A troubling local matter".into(),
            issuer_belief_description:
                "The issuer describes the symptoms and asks for a verified resolution.".into(),
            opposition_wording: "unknown opposition".into(),
            opposition_count_wording: "unknown number".into(),
            reward: 150,
        }),
        bridges,
        factor_trace: trace,
    };
    validate(&manifest).map_err(GenerationError::InvalidManifest)?;
    Ok(manifest)
}

pub fn validate(case: &GeneratedCase) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if case.catalog_revision != CATALOG_REVISION {
        errors.push("catalog revision mismatch".into());
    }
    let true_sites: Vec<_> = case.sites.iter().filter(|s| s.is_true_location).collect();
    if true_sites.len() != 1 {
        errors.push("case must have exactly one canonical finale location".into());
    }
    let route_classes: BTreeSet<_> = case.actions.iter().map(|a| a.route).collect();
    if route_classes.len() < 2 {
        errors.push("case requires two materially different route classes".into());
    }
    if case.actions.iter().filter(|a| a.active_initially).count() < 2 {
        errors.push("two routes must be initially playable".into());
    }
    let action_ids: BTreeSet<_> = case.actions.iter().map(|a| a.id.clone()).collect();
    for action in &case.actions {
        if !action_ids.contains(&action.alternate) {
            errors.push(format!("{} has no recovery route", action.id.0));
        }
        if action.prerequisite.as_ref() == Some(&action.id) {
            errors.push(format!("{} dominates itself", action.id.0));
        }
        let target_exists = match action.target_kind.as_str() {
            "site" => case.sites.iter().any(|site| site.id.0 == action.target_id),
            "area" => case.areas.iter().any(|area| area.id == action.target_id),
            "contact" => case
                .witnesses
                .iter()
                .any(|witness| witness.npc_id == action.target_id),
            _ => false,
        };
        if !target_exists {
            errors.push(format!(
                "{} references missing {} authority {}",
                action.id.0, action.target_kind, action.target_id
            ));
        }
    }
    for witness in &case.witnesses {
        if witness.npc_id.is_empty()
            || witness.expected_location.is_empty()
            || witness.visible_description.is_empty()
        {
            errors.push(format!("{} lacks persistent referral data", witness.id.0));
        }
    }
    for t in &case.factor_trace {
        if t.accepted && t.plausibility > 0 && t.plausibility < 5 && t.required_bridge.is_none() {
            errors.push(format!(
                "rare candidate {} lacks causal bridge",
                t.candidate_id
            ));
        }
        if !t.accepted && t.hard_zero_reason.is_none() {
            errors.push(format!(
                "rejected candidate {} lacks diagnostic",
                t.candidate_id
            ));
        }
    }
    for bridge in &case.bridges {
        if !case
            .canonical_events
            .iter()
            .any(|e| e.id == bridge.event_id)
        {
            errors.push(format!("bridge {} has no event", bridge.id.0));
        }
        if !case.evidence.iter().any(|e| e.id == bridge.evidence_id) {
            errors.push(format!("bridge {} has no evidence authority", bridge.id.0));
        }
        if !case.actions.iter().any(|a| {
            a.outputs
                .iter()
                .any(|output| matches!(output, GeneratedActionOutput::Evidence { .. }))
        }) {
            errors.push(format!("bridge {} has no playable lead path", bridge.id.0));
        }
        if bridge.lead_summary.is_empty() {
            errors.push(format!("bridge {} has no lead", bridge.id.0));
        }
    }
    let finale_sites: BTreeSet<_> = case.finales.iter().map(|f| f.site_id.clone()).collect();
    if finale_sites
        .iter()
        .any(|id| !case.sites.iter().any(|s| &s.id == id))
    {
        errors.push("finale references missing site".into());
    }
    let true_site = true_sites.first().map(|site| &site.id);
    for route in &route_classes {
        if !case
            .actions
            .iter()
            .filter(|action| &action.route == route)
            .any(|action| {
                action.outputs.iter().any(|output| {
                    matches!(
                        output,
                        GeneratedActionOutput::Destination {
                            stage: GeneratedDestinationStage::Exact,
                            site_id: Some(site_id),
                        } if Some(site_id) == true_site
                    )
                })
            })
        {
            errors.push(format!("{route:?} has no exact finale-site output"));
        }
    }
    for finale in &case.finales {
        let produced = match finale.kind {
            FinaleKind::Defeat | FinaleKind::DriveOff => case
                .hostile_groups
                .iter()
                .any(|(id, site, _, _)| {
                    finale.hostile_group_id.as_deref() == Some(id) && site == &finale.site_id
                }),
            FinaleKind::Rescue => case.actions.iter().any(|action| {
                action.outputs.iter().any(|output| {
                    matches!(
                        output,
                        GeneratedActionOutput::Consequence {
                            consequence: GeneratedActionConsequence::RescueSubject { subject_id, .. }
                        } if finale.subject_id.as_deref() == Some(subject_id)
                    )
                })
            }),
            FinaleKind::RetrieveReturn => case.actions.iter().any(|action| {
                action.outputs.iter().any(|output| matches!(
                    output,
                    GeneratedActionOutput::Consequence {
                        consequence: GeneratedActionConsequence::RetrieveAsset { asset_id, .. }
                    } if finale.asset_id.as_deref() == Some(asset_id)
                ))
                    && case.dialogue_producers.iter().any(|producer| {
                        producer.action == GeneratedDialogueAction::ReturnAsset
                            && producer.asset_id.as_deref() == finale.asset_id.as_deref()
                    })
            }),
            FinaleKind::Expose => case.dialogue_producers.iter().any(|producer| {
                producer.action == GeneratedDialogueAction::Expose
                    && producer.subject_ref.as_deref() == finale.subject_id.as_deref()
            }),
            FinaleKind::Negotiate | FinaleKind::Capture => false,
        };
        if !produced {
            errors.push(format!("{:?} has no concrete owning producer", finale.kind));
        }
    }
    let objective_ids: BTreeSet<_> = case
        .objectives
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
        .map(|objective| objective.id.clone())
        .collect();
    for producer in &case.dialogue_producers {
        if !objective_ids.contains(&producer.objective_id) {
            errors.push(format!(
                "dialogue producer references missing objective {}",
                producer.objective_id.as_str()
            ));
        }
        if producer.recipient_npc_id.is_empty() {
            errors.push("dialogue producer has no recipient".into());
        }
    }
    for objective in case
        .objectives
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
    {
        let produced = match &objective.requirement {
            ObjectiveRequirement::Defeat {
                hostile_group_id, ..
            }
            | ObjectiveRequirement::DriveOff { hostile_group_id } => case
                .hostile_groups
                .iter()
                .any(|(id, _, _, _)| id == hostile_group_id),
            ObjectiveRequirement::Rescue { subject_id } => case.actions.iter().any(|action| {
                action.outputs.iter().any(|output| matches!(
                    output,
                    GeneratedActionOutput::Consequence {
                        consequence: GeneratedActionConsequence::RescueSubject { subject_id: produced, .. }
                    } if produced == subject_id.as_str()
                ))
            }),
            ObjectiveRequirement::Retrieve { asset_id } => case.actions.iter().any(|action| {
                action.outputs.iter().any(|output| matches!(
                    output,
                    GeneratedActionOutput::Consequence {
                        consequence: GeneratedActionConsequence::RetrieveAsset { asset_id: produced, .. }
                    } if produced == asset_id.as_str()
                ))
            }),
            ObjectiveRequirement::Return {
                asset_id,
                custodian_id,
            } => case.dialogue_producers.iter().any(|producer| {
                producer.objective_id == objective.id
                    && producer.action == GeneratedDialogueAction::ReturnAsset
                    && producer.asset_id.as_deref() == Some(asset_id.as_str())
                    && producer.recipient_npc_id == *custodian_id
            }),
            ObjectiveRequirement::Expose { subject_ref } => {
                case.dialogue_producers.iter().any(|producer| {
                    producer.objective_id == objective.id
                        && producer.action == GeneratedDialogueAction::Expose
                        && producer.subject_ref.as_deref() == Some(subject_ref)
                })
            }
            _ => false,
        };
        if !produced {
            errors.push(format!(
                "objective {} has no concrete owning producer",
                objective.id.as_str()
            ));
        }
    }
    let expected_finale = match (case.family, case.cause) {
        (TemplateFamily::RecurringDepredation, CanonicalCause::Hostile(_)) => {
            case.finales.iter().all(|finale| {
                matches!(finale.kind, FinaleKind::Defeat | FinaleKind::DriveOff)
                    && finale.hostile_group_id.is_some()
            })
        }
        (
            TemplateFamily::DisappearanceOrLoss,
            CanonicalCause::Hostile(_) | CanonicalCause::ConcealmentByWitness,
        ) => {
            case.finales.len() == 1
                && case.finales[0].kind == FinaleKind::Rescue
                && case.finales[0].subject_id.is_some()
        }
        (TemplateFamily::DisappearanceOrLoss, CanonicalCause::IncidentalLoss) => {
            case.finales.len() == 1
                && case.finales[0].kind == FinaleKind::RetrieveReturn
                && case.finales[0].asset_id.is_some()
        }
        (TemplateFamily::DisappearanceOrLoss, CanonicalCause::FabricatedClaim) => {
            case.finales.len() == 1 && case.finales[0].kind == FinaleKind::Expose
        }
        _ => false,
    };
    if !expected_finale {
        errors.push("canonical cause is incompatible with generated objective/finale".into());
    }
    if case.contract.as_ref().is_some_and(|c| {
        c.opposition_wording
            .to_lowercase()
            .contains(&format!("{:?}", case.cause).to_lowercase())
    }) {
        errors.push("contract leaks canonical cause".into());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn audit(seeds: u64) -> BTreeMap<TemplateFamily, u64> {
    let mut out = BTreeMap::new();
    for seed in 0..seeds {
        let context = GenerationContext {
            seed,
            settlement_id: "audit".into(),
            settlement_name: "Audit".into(),
            scope: Scope::Settlement {
                settlement_id: "audit".into(),
            },
            ordinal: 0,
            now_minute: 1_000,
            requested_family: None,
            witness_candidates: test_witnesses(),
        };
        if let Ok(case) = generate(&context) {
            *out.entry(case.family).or_default() += 1;
        }
    }
    out
}

pub fn test_witnesses() -> Vec<WitnessCandidate> {
    vec![
        WitnessCandidate {
            npc_id: "npc:a".into(),
            demographic: WitnessDemographic::Child,
            profession: "apprentice".into(),
            visible_description: "a short, fair-haired apprentice".into(),
            expected_location: "residential".into(),
            allowed_circumstances: BTreeSet::from([
                Circumstance::NightWindow,
                Circumstance::AdultVenue,
            ]),
        },
        WitnessCandidate {
            npc_id: "npc:b".into(),
            demographic: WitnessDemographic::Guard,
            profession: "guard".into(),
            visible_description: "a tall guard with dark hair".into(),
            expected_location: "keep".into(),
            allowed_circumstances: BTreeSet::from([
                Circumstance::RoadJourney,
                Circumstance::GraveDuty,
            ]),
        },
        WitnessCandidate {
            npc_id: "npc:c".into(),
            demographic: WitnessDemographic::Merchant,
            profession: "merchant".into(),
            visible_description: "a broad merchant with grey hair".into(),
            expected_location: "market".into(),
            allowed_circumstances: BTreeSet::from([
                Circumstance::RoadJourney,
                Circumstance::SecretRiversideMeeting,
            ]),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context(seed: u64, family: TemplateFamily) -> GenerationContext {
        GenerationContext {
            seed,
            settlement_id: "lubeck".into(),
            settlement_name: "Lubeck".into(),
            scope: Scope::Settlement {
                settlement_id: "lubeck".into(),
            },
            ordinal: 0,
            now_minute: 50_000,
            requested_family: Some(family),
            witness_candidates: test_witnesses(),
        }
    }
    #[test]
    fn golden_seeds_cover_both_families() {
        assert_eq!(
            generate(&context(7, TemplateFamily::RecurringDepredation))
                .unwrap()
                .family,
            TemplateFamily::RecurringDepredation
        );
        assert_eq!(
            generate(&context(7, TemplateFamily::DisappearanceOrLoss))
                .unwrap()
                .family,
            TemplateFamily::DisappearanceOrLoss
        );
    }
    #[test]
    fn deterministic_and_counterfactual() {
        let a = generate(&context(41, TemplateFamily::DisappearanceOrLoss)).unwrap();
        assert_eq!(
            a,
            generate(&context(41, TemplateFamily::DisappearanceOrLoss)).unwrap()
        );
        let b = generate(&context(42, TemplateFamily::DisappearanceOrLoss)).unwrap();
        assert_eq!(a.consequence.symptom, b.consequence.symptom);
        assert_ne!((a.cause, a.sites[0].kind), (b.cause, b.sites[0].kind));
    }
    #[test]
    fn descriptions_are_ambiguous() {
        for seed in 0..256 {
            let generated = generate(&context(seed, TemplateFamily::RecurringDepredation)).unwrap();
            assert!(
                crate::bestiary::ambiguous_description_cardinality(
                    generated.witnesses[0].description
                ) >= 2
            );
        }
    }
    #[test]
    fn hard_zero_and_rare_rules_are_auditable() {
        let mut trace = Vec::new();
        let _ = choose(
            1,
            "module.site",
            "relation.site.cause",
            &site_candidates(CanonicalCause::Hostile(ThreatId::Skeleton)),
            &mut trace,
        )
        .unwrap();
        let house = trace
            .iter()
            .find(|t| t.candidate_id == "site.occupied_house")
            .unwrap();
        assert_eq!(house.plausibility, 3);
        assert_eq!(
            house.required_bridge.as_ref().unwrap().0,
            "bridge.skeletons_occupied_house"
        );
        let wolf_crypt = site_candidates(CanonicalCause::Hostile(ThreatId::Wolf))
            .into_iter()
            .find(|c| c.id == "site.crypt")
            .unwrap();
        assert_eq!(wolf_crypt.weight.plausibility, 0);
        assert!(wolf_crypt.impossible.is_some());
    }
    #[test]
    fn child_adult_venue_is_rare_but_bridged() {
        let adult = circumstance_candidates(WitnessDemographic::Child)
            .into_iter()
            .find(|c| c.value == Circumstance::AdultVenue)
            .unwrap();
        assert_eq!(adult.weight.plausibility, 2);
        assert_eq!(adult.bridge, Some("bridge.child_at_adult_venue"));
    }
    #[test]
    fn graph_survives_removing_either_witness_route() {
        for family in [
            TemplateFamily::RecurringDepredation,
            TemplateFamily::DisappearanceOrLoss,
        ] {
            let case = generate(&context(7, family)).unwrap();
            validate(&case).unwrap();
            let routes = case
                .actions
                .iter()
                .map(|a| a.route)
                .collect::<BTreeSet<_>>();
            assert_eq!(routes.len(), 2);
            for route in routes {
                assert!(
                    case.actions
                        .iter()
                        .any(|a| a.route != route && a.active_initially)
                );
            }
        }
    }
    #[test]
    fn disappearance_truth_selects_only_compatible_targets_and_producers() {
        for seed in 0..256 {
            let generated = generate(&context(seed, TemplateFamily::DisappearanceOrLoss)).unwrap();
            assert_ne!(generated.cause, CanonicalCause::VoluntaryDisappearance);
            validate(&generated).unwrap();
            match generated.cause {
                CanonicalCause::Hostile(_) | CanonicalCause::ConcealmentByWitness => {
                    assert_eq!(generated.finales.len(), 1);
                    assert_eq!(generated.finales[0].kind, FinaleKind::Rescue);
                    let subjects = generated
                        .actions
                        .iter()
                        .filter(|action| {
                            action.id.0.ends_with(":action:follow")
                                || action.id.0.ends_with(":action:approach_social")
                        })
                        .flat_map(|action| &action.outputs)
                        .filter_map(|output| match output {
                            GeneratedActionOutput::Consequence {
                                consequence:
                                    GeneratedActionConsequence::RescueSubject { subject_id, .. },
                            } => Some(subject_id),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(subjects.len(), 2);
                    assert_eq!(subjects[0], subjects[1]);
                }
                CanonicalCause::IncidentalLoss => {
                    assert_eq!(generated.finales[0].kind, FinaleKind::RetrieveReturn);
                    assert!(generated.dialogue_producers.iter().any(|producer| {
                        producer.action == GeneratedDialogueAction::ReturnAsset
                    }));
                }
                CanonicalCause::FabricatedClaim => {
                    assert_eq!(generated.finales[0].kind, FinaleKind::Expose);
                    assert!(
                        generated
                            .dialogue_producers
                            .iter()
                            .any(|producer| { producer.action == GeneratedDialogueAction::Expose })
                    );
                }
                CanonicalCause::VoluntaryDisappearance => unreachable!(),
            }
        }
    }
    #[test]
    fn correction_reuses_the_proposition_it_corrects() {
        let generated = generate(&context(19, TemplateFamily::DisappearanceOrLoss)).unwrap();
        let initial = &generated.witnesses[0].testimony[0];
        let correction = &generated.witnesses[1].testimony[0];
        assert_eq!(initial.proposition_id, correction.proposition_id);
        assert_eq!(
            correction.corrects_proposition_id.as_deref(),
            Some(initial.proposition_id.as_str())
        );
    }
    #[test]
    fn marginal_sweep_is_bounded_and_has_both_templates() {
        let result = audit(256);
        assert!(result[&TemplateFamily::RecurringDepredation] > 80);
        assert!(result[&TemplateFamily::DisappearanceOrLoss] > 80);
    }
    #[test]
    fn public_identity_and_contract_do_not_reveal_truth() {
        let case = generate(&context(88, TemplateFamily::RecurringDepredation)).unwrap();
        assert_ne!(case.canonical_case_id, case.public_case_id);
        let json = serde_json::to_string(case.contract.as_ref().unwrap())
            .unwrap()
            .to_lowercase();
        assert!(!json.contains(&format!("{:?}", case.cause).to_lowercase()));
    }
}
