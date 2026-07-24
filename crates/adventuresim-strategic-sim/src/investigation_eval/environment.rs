use super::{
    ArgumentValue, Capability, ChoiceArguments, ChoiceKind, DeveloperCaseAnalysis, DiscoveryView,
    EVAL_FORMAT_VERSION, JournalView, LegalChoice, LocationResolution, PartyView, PlayerFrame,
    PublicClaim, PublicEvidence, PublicLocation, PublicQuestTrace, PublicTraceEvent, Termination,
    WitnessAvailability, WitnessReferral,
};
use adventuresim_core::quest_generation::{
    self as qg, Circumstance, GeneratedActionOutput, GeneratedCase, GeneratedDestinationStage,
    RouteClass, TemplateFamily, WitnessCandidate, WitnessDemographic,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct EvalCaseConfig {
    pub seed: u64,
    pub family: TemplateFamily,
    pub party: PartyView,
}

impl EvalCaseConfig {
    pub fn fixture(seed: u64, family: TemplateFamily) -> Self {
        Self {
            seed,
            family,
            party: PartyView {
                members: 3,
                terrain_skill: 55,
                insight: 50,
                perception: 50,
                combat_readiness: 65,
                supplies: 12,
                equipment_tags: vec!["rope".into(), "lantern".into(), "mixed_weapons".into()],
            },
        }
    }
}

#[derive(Debug)]
pub struct InvestigationEnvironment {
    generated: GeneratedCase,
    analysis: DeveloperCaseAnalysis,
    frame: PlayerFrame,
    capabilities: BTreeMap<String, Capability>,
    tavern_entered: bool,
    interviewed: BTreeSet<usize>,
    completed_actions: BTreeSet<usize>,
    exact_sites: BTreeSet<String>,
    visited_sites: BTreeSet<String>,
    prepared: BTreeSet<String>,
    /// Ordinary schedules are player-visible state, not a pipeline error.
    witness_returns_at: BTreeMap<usize, u64>,
    trace: Vec<PublicTraceEvent>,
    route: Option<RouteClass>,
    solved: bool,
}

impl InvestigationEnvironment {
    pub fn generate(config: EvalCaseConfig) -> Result<Self, String> {
        let context = generation_context(config.seed, config.family);
        let generated = qg::generate(&context)
            .map_err(|error| format!("quest generation failed: {error:?}"))?;
        Self::from_generated(generated, config.party)
    }

    pub fn from_generated(generated: GeneratedCase, party: PartyView) -> Result<Self, String> {
        let analysis = developer_analysis(&generated)?;
        let frame = PlayerFrame {
            version: EVAL_FORMAT_VERSION,
            case_id: generated.public_case_id.clone(),
            step: 0,
            game_minute: 0,
            discovery: DiscoveryView {
                problem_summary: "No local problem has been learned yet.".into(),
                consequence_summary: String::new(),
                learned_at: String::new(),
                referrals: Vec::new(),
            },
            journal: JournalView::default(),
            party,
            legal_choices: Vec::new(),
        };
        let mut witness_returns_at = BTreeMap::new();
        if generated.generation_seed.is_multiple_of(2) {
            witness_returns_at.insert(1, 90);
        }
        let mut value = Self {
            generated,
            analysis,
            frame,
            capabilities: BTreeMap::new(),
            tavern_entered: false,
            interviewed: BTreeSet::new(),
            completed_actions: BTreeSet::new(),
            exact_sites: BTreeSet::new(),
            visited_sites: BTreeSet::new(),
            prepared: BTreeSet::new(),
            witness_returns_at,
            trace: Vec::new(),
            route: None,
            solved: false,
        };
        value.refresh_choices();
        Ok(value)
    }

    pub fn frame(&self) -> &PlayerFrame {
        &self.frame
    }

    pub fn developer_analysis(&self) -> &DeveloperCaseAnalysis {
        &self.analysis
    }

    pub fn apply(&mut self, decision: &super::PolicyDecision) -> Result<(), String> {
        if decision.version != EVAL_FORMAT_VERSION {
            return Err("unsupported policy decision version".into());
        }
        let capability = self
            .capabilities
            .get(&decision.choice_id)
            .cloned()
            .ok_or_else(|| "choice ID is forged, stale, or not currently legal".to_string())?;
        let legal = self
            .frame
            .legal_choices
            .iter()
            .find(|choice| choice.choice_id == decision.choice_id)
            .ok_or("choice capability lacks public presentation")?;
        validate_arguments(&legal.typed_arguments, &decision.arguments)?;
        let kind = legal.kind;
        let waiting_for_witness = matches!(&capability, Capability::WaitForWitness(_));
        let mut learned = Vec::new();
        let (result, minutes, cost) = match capability {
            Capability::EnterTavern => {
                self.tavern_entered = true;
                self.frame.discovery.problem_summary =
                    self.generated.consequence.public_summary.clone();
                self.frame.discovery.consequence_summary =
                    format!("{:?}", self.generated.consequence.symptom);
                self.frame.discovery.learned_at = "settlement tavern rumor".into();
                self.frame.discovery.referrals = self
                    .generated
                    .witnesses
                    .iter()
                    .map(|witness| WitnessReferral {
                        witness_id: witness.id.0.clone(),
                        display_name: witness_name(&witness.npc_id),
                        physical_description: witness.visible_description.clone(),
                        expected_location: witness.expected_location.clone(),
                        interviewed: false,
                        availability: WitnessAvailability::Available,
                    })
                    .collect();
                self.refresh_witness_availability();
                learned.push(self.generated.consequence.public_summary.clone());
                (
                    "The tavern's talk reveals a local problem and witness referrals.".into(),
                    15,
                    0,
                )
            }
            Capability::Interview(index) => {
                let witness = self.generated.witnesses.get(index).ok_or("stale witness")?;
                self.interviewed.insert(index);
                if let Some(referral) = self.frame.discovery.referrals.get_mut(index) {
                    referral.interviewed = true;
                }
                for statement in &witness.testimony {
                    self.frame.journal.claims.push(PublicClaim {
                        proposition_id: statement.proposition_id.clone(),
                        source: witness.visible_description.clone(),
                        text: statement.spoken_text.clone(),
                    });
                    if let Some(corrected) = &statement.corrects_proposition_id {
                        self.frame.journal.corrections.push(corrected.clone());
                    }
                    learned.push(statement.spoken_text.clone());
                }
                (
                    "The witness's account is recorded with its source.".into(),
                    20,
                    0,
                )
            }
            Capability::WaitForWitness(index) => {
                let return_at = *self
                    .witness_returns_at
                    .get(&index)
                    .ok_or("witness has no scheduled return")?;
                let wait = return_at.saturating_sub(self.frame.game_minute).max(15);
                self.frame.game_minute = return_at;
                self.refresh_witness_availability();
                learned.push("The referred witness returns to their expected location.".into());
                (
                    "The party waits rather than treating an ordinary absence as a failure."
                        .into(),
                    wait as u32,
                    1,
                )
            }
            Capability::Action(index, _action_kind, route) => {
                let action = self.generated.actions.get(index).ok_or("stale action")?;
                if action.target_kind == "site" && !self.visited_sites.contains(&action.target_id) {
                    return Err("site action requires authoritative occupancy".into());
                }
                self.completed_actions.insert(index);
                self.route.get_or_insert(route);
                for output in &action.outputs {
                    match output {
                        GeneratedActionOutput::Destination { stage, site_id } => {
                            let label = site_id
                                .as_ref()
                                .and_then(|id| {
                                    self.generated.sites.iter().find(|site| &site.id == id)
                                })
                                .map(|site| site.safe_label.clone())
                                .unwrap_or_else(|| action.safe_summary.clone());
                            let resolution = if *stage == GeneratedDestinationStage::Exact {
                                if let Some(id) = site_id {
                                    self.exact_sites.insert(id.0.clone());
                                }
                                LocationResolution::Exact
                            } else {
                                LocationResolution::Approximate
                            };
                            upsert_location(
                                &mut self.frame.journal.locations,
                                label.clone(),
                                resolution,
                            );
                            learned.push(label);
                        }
                        GeneratedActionOutput::Evidence { evidence_id }
                        | GeneratedActionOutput::PatternCondition { evidence_id, .. } => {
                            if let Some(evidence) = self
                                .generated
                                .evidence
                                .iter()
                                .find(|item| &item.id == evidence_id)
                            {
                                self.frame.journal.evidence.push(PublicEvidence {
                                    evidence_id: evidence.id.0.clone(),
                                    description: evidence.safe_description.clone(),
                                    discovery_source: action.safe_summary.clone(),
                                });
                                if let Some(corrected) = &evidence.corrects_proposition_id {
                                    self.frame.journal.corrections.push(corrected.clone());
                                }
                                learned.push(evidence.safe_description.clone());
                            }
                        }
                        GeneratedActionOutput::AmbushReady => {
                            learned.push("The party has established an ambush position.".into());
                        }
                        GeneratedActionOutput::Consequence { .. } => {
                            learned.push(
                                "The site investigation produced a recoverable result.".into(),
                            );
                        }
                    }
                }
                (format!("Completed: {}", action.safe_summary), 60, 1)
            }
            Capability::Travel(site_id) => {
                self.visited_sites.insert(site_id.clone());
                if let Some(site) = self
                    .generated
                    .sites
                    .iter()
                    .find(|site| site.id.0 == site_id)
                {
                    upsert_location(
                        &mut self.frame.journal.locations,
                        site.safe_label.clone(),
                        LocationResolution::Visited,
                    );
                }
                (
                    "The party travels to the learned exact location.".into(),
                    180,
                    2,
                )
            }
            Capability::Prepare(tag) => {
                self.prepared.insert(tag.clone());
                self.frame.party.equipment_tags.push(tag.clone());
                learned.push(format!("Prepared {tag}."));
                (
                    "The party adjusts its equipment and supplies.".into(),
                    30,
                    1,
                )
            }
            Capability::Conclude(route) => {
                self.route = Some(route);
                self.solved = true;
                (
                    "The generated case's earned finale is resolved.".into(),
                    120,
                    2,
                )
            }
        };
        self.frame.party.supplies = self.frame.party.supplies.saturating_sub(cost);
        let digest = semantic_digest(&self.frame)?;
        self.trace.push(PublicTraceEvent {
            step: self.frame.step,
            frame_digest: digest,
            choice_id: decision.choice_id.clone(),
            choice_kind: kind,
            result,
            learned,
            game_minutes: minutes,
            resource_cost: cost,
        });
        self.frame.step += 1;
        if !waiting_for_witness {
            self.frame.game_minute += u64::from(minutes);
        }
        self.refresh_choices();
        Ok(())
    }

    pub fn is_solved(&self) -> bool {
        self.solved
    }

    pub fn public_trace(
        &self,
        policy: String,
        termination: Termination,
    ) -> Result<PublicQuestTrace, String> {
        let mut trace = PublicQuestTrace {
            version: EVAL_FORMAT_VERSION,
            case_id: self.frame.case_id.clone(),
            policy,
            events: self.trace.clone(),
            solved: self.solved,
            exhausted: self.frame.legal_choices.is_empty(),
            termination,
            route: self.route,
            semantic_digest: String::new(),
        };
        trace.semantic_digest = semantic_digest(&trace)?;
        Ok(trace)
    }

    fn refresh_choices(&mut self) {
        self.capabilities.clear();
        let mut choices = Vec::new();
        if !self.tavern_entered {
            self.push_choice(
                &mut choices,
                ChoiceKind::EnterTavern,
                "Enter the tavern and listen.",
                Capability::EnterTavern,
            );
        } else {
            let witness_choices = self
                .generated
                .witnesses
                .iter()
                .enumerate()
                .map(|(index, witness)| {
                    (
                        index,
                        format!(
                            "Speak with {} at {}.",
                            witness.visible_description, witness.expected_location
                        ),
                    )
                })
                .collect::<Vec<_>>();
            for (index, label) in witness_choices {
                if !self.interviewed.contains(&index) && self.witness_available(index) {
                    self.push_choice(
                        &mut choices,
                        ChoiceKind::InterviewWitness,
                        &label,
                        Capability::Interview(index),
                    );
                }
                if !self.interviewed.contains(&index) && !self.witness_available(index) {
                    self.push_choice(
                        &mut choices,
                        ChoiceKind::Wait,
                        &format!("Wait for {label} to return."),
                        Capability::WaitForWitness(index),
                    );
                }
            }
            let action_choices = self
                .generated
                .actions
                .iter()
                .enumerate()
                .map(|(index, action)| {
                    (
                        index,
                        action.safe_summary.clone(),
                        action.kind,
                        action.route,
                    )
                })
                .collect::<Vec<_>>();
            for (index, label, kind, route) in action_choices {
                if self.completed_actions.contains(&index) || !self.action_available(index) {
                    continue;
                }
                self.push_choice(
                    &mut choices,
                    ChoiceKind::Investigate,
                    &label,
                    Capability::Action(index, kind, route),
                );
            }
            for site_id in self.exact_sites.clone() {
                if !self.visited_sites.contains(&site_id) {
                    let label = self
                        .generated
                        .sites
                        .iter()
                        .find(|site| site.id.0 == site_id)
                        .map(|site| site.safe_label.as_str())
                        .unwrap_or("learned destination");
                    self.push_choice(
                        &mut choices,
                        ChoiceKind::Travel,
                        &format!("Travel to {label}."),
                        Capability::Travel(site_id),
                    );
                }
            }
            if self.prepared.is_empty() {
                self.push_choice(
                    &mut choices,
                    ChoiceKind::Prepare,
                    "Prepare rope, light, and suitable weapons.",
                    Capability::Prepare("investigation_kit".into()),
                );
            }
            for finale in &self.generated.finales {
                if self.visited_sites.contains(&finale.site_id.0)
                    && self
                        .generated
                        .actions
                        .iter()
                        .enumerate()
                        .any(|(index, action)| {
                            self.completed_actions.contains(&index)
                                && action.route == self.route.unwrap_or(action.route)
                                && action.target_kind == "site"
                        })
                {
                    let route = self.route.unwrap_or(RouteClass::PhysicalTrail);
                    self.push_choice(
                        &mut choices,
                        ChoiceKind::Conclude,
                        &format!("Attempt the {:?} finale.", finale.kind),
                        Capability::Conclude(route),
                    );
                    break;
                }
            }
        }
        self.frame.legal_choices = choices;
    }

    fn action_available(&self, index: usize) -> bool {
        let action = &self.generated.actions[index];
        if !action.active_initially
            && action.prerequisite.as_ref().is_some_and(|required| {
                !self
                    .generated
                    .actions
                    .iter()
                    .enumerate()
                    .any(|(prior, candidate)| {
                        &candidate.id == required && self.completed_actions.contains(&prior)
                    })
            })
        {
            return false;
        }
        action.target_kind != "site" || self.visited_sites.contains(&action.target_id)
    }

    fn witness_available(&self, index: usize) -> bool {
        self.witness_returns_at
            .get(&index)
            .is_none_or(|return_at| self.frame.game_minute >= *return_at)
    }

    fn refresh_witness_availability(&mut self) {
        let game_minute = self.frame.game_minute;
        let returns = &self.witness_returns_at;
        let interviewed = &self.interviewed;
        for (index, referral) in self.frame.discovery.referrals.iter_mut().enumerate() {
            referral.availability = if interviewed.contains(&index)
                || returns.get(&index).is_none_or(|return_at| game_minute >= *return_at)
            {
                WitnessAvailability::Available
            } else if game_minute == 0 {
                WitnessAvailability::ScheduledElsewhere
            } else {
                WitnessAvailability::AwaitingReturn
            };
        }
    }

    fn push_choice(
        &mut self,
        choices: &mut Vec<LegalChoice>,
        kind: ChoiceKind,
        label: &str,
        capability: Capability,
    ) {
        let id = choice_id(
            &self.frame.case_id,
            self.frame.step,
            choices.len(),
            &capability,
        );
        self.capabilities.insert(id.clone(), capability);
        choices.push(LegalChoice {
            choice_id: id,
            kind,
            label: label.to_owned(),
            typed_arguments: ChoiceArguments {
                allowed: Vec::<ArgumentValue>::new(),
            },
        });
    }
}

fn witness_name(npc_id: &str) -> String {
    // Generated population names are not yet part of the core generator's
    // portable fixture input. Keep the evaluator presentation realistic without
    // leaking raw NPC IDs to the policy.
    match npc_id.rsplit(':').next().unwrap_or("local") {
        "watchman" => "Konrad, the watchman".into(),
        "cooper" => "Marta, the cooper".into(),
        "merchant" => "Elsbeth, the merchant".into(),
        role => format!("a local {role}"),
    }
}

fn validate_arguments(
    allowed: &ChoiceArguments,
    selected: &super::DecisionArguments,
) -> Result<(), String> {
    match &selected.selection {
        None if allowed.allowed.is_empty() => Ok(()),
        Some(value)
            if allowed
                .allowed
                .iter()
                .any(|argument| argument.values.contains(value)) =>
        {
            Ok(())
        }
        _ => Err("typed choice arguments are not legal for this capability".into()),
    }
}

fn choice_id(case_id: &str, step: u32, ordinal: usize, capability: &Capability) -> String {
    let digest = blake3::hash(format!("{case_id}:{step}:{ordinal}:{capability:?}").as_bytes());
    format!("choice:{}", &digest.to_hex()[..24])
}

fn upsert_location(
    locations: &mut Vec<PublicLocation>,
    label: String,
    resolution: LocationResolution,
) {
    if let Some(existing) = locations.iter_mut().find(|entry| entry.label == label) {
        existing.resolution = resolution;
    } else {
        locations.push(PublicLocation { label, resolution });
    }
}

pub fn semantic_digest<T: serde::Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn developer_analysis(case: &GeneratedCase) -> Result<DeveloperCaseAnalysis, String> {
    let true_site = case
        .sites
        .iter()
        .find(|site| site.is_true_location)
        .map(|site| site.id.0.clone())
        .ok_or("generated case lacks true site")?;
    let private_digest = semantic_digest(case)?;
    Ok(DeveloperCaseAnalysis {
        family: case.family,
        canonical_case_id: case.canonical_case_id.clone(),
        canonical_cause: format!("{:?}", case.cause),
        generation_seed: case.generation_seed,
        catalog_revision: case.catalog_revision.clone(),
        true_site,
        factor_ids: case
            .factor_trace
            .iter()
            .flat_map(|trace| trace.factor_ids.iter().map(|id| id.0.clone()))
            .collect(),
        plausibility_factors: case
            .factor_trace
            .iter()
            .map(|trace| trace.plausibility)
            .collect(),
        curation_factors: case
            .factor_trace
            .iter()
            .map(|trace| trace.curation)
            .collect(),
        bridge_ids: case
            .bridges
            .iter()
            .map(|bridge| bridge.id.0.clone())
            .collect(),
        generator_manifest_digest: private_digest,
    })
}

fn generation_context(seed: u64, family: TemplateFamily) -> qg::GenerationContext {
    let circumstances = BTreeSet::from([
        Circumstance::NightWindow,
        Circumstance::SecretRiversideMeeting,
        Circumstance::AdultVenue,
        Circumstance::RoadJourney,
        Circumstance::GraveDuty,
        Circumstance::LivestockWatch,
    ]);
    let witness = |id: &str, demographic, description: &str, location: &str| WitnessCandidate {
        npc_id: format!("npc:{id}"),
        demographic,
        age_band: "adult".into(),
        sex: "unspecified".into(),
        profession: id.into(),
        visible_description: description.into(),
        expected_location: location.into(),
        presence_version: 1,
        allowed_circumstances: circumstances.clone(),
    };
    qg::GenerationContext {
        seed,
        observer_entropy_hi: seed.rotate_left(17) ^ 0x188,
        observer_entropy_lo: seed.rotate_right(9) ^ 0x5151,
        settlement_id: "settlement:evaluator".into(),
        settlement_name: "Greifenhagen".into(),
        scope: adventuresim_core::local_problem::Scope::Settlement {
            settlement_id: "settlement:evaluator".into(),
        },
        ordinal: (seed & u64::from(u16::MAX)) as u16,
        now_minute: 100_000,
        requested_family: Some(family),
        witness_candidates: vec![
            witness(
                "watchman",
                WitnessDemographic::Guard,
                "a tall watchman with cropped fair hair and a scarred chin",
                "the gatehouse",
            ),
            witness(
                "cooper",
                WitnessDemographic::Laborer,
                "a short cooper with dark curls and a blue apron",
                "the riverside workshop",
            ),
            witness(
                "merchant",
                WitnessDemographic::Merchant,
                "an elderly merchant in a red wool cap",
                "the market arcade",
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::investigation_eval::{DecisionArguments, PolicyDecision};

    #[test]
    fn forged_choices_and_arguments_fail_closed() {
        let mut env = InvestigationEnvironment::generate(EvalCaseConfig::fixture(
            7,
            TemplateFamily::RecurringDepredation,
        ))
        .unwrap();
        assert!(
            env.apply(&PolicyDecision {
                version: EVAL_FORMAT_VERSION,
                choice_id: "choice:forged".into(),
                arguments: DecisionArguments::default(),
            })
            .is_err()
        );
        let id = env.frame().legal_choices[0].choice_id.clone();
        assert!(
            env.apply(&PolicyDecision {
                version: EVAL_FORMAT_VERSION,
                choice_id: id,
                arguments: DecisionArguments {
                    selection: Some("raw-reducer:drop-table".into())
                },
            })
            .is_err()
        );
    }

    #[test]
    fn private_truth_is_absent_from_player_serialization() {
        let env = InvestigationEnvironment::generate(EvalCaseConfig::fixture(
            11,
            TemplateFamily::DisappearanceOrLoss,
        ))
        .unwrap();
        let public = serde_json::to_string(env.frame()).unwrap();
        let private = env.developer_analysis();
        assert!(!public.contains(&private.canonical_case_id));
        assert!(!public.contains(&private.canonical_cause));
        assert!(!public.contains(&private.true_site));
        assert!(!public.contains("factor_ids"));
        assert!(!public.contains("plausibility"));
    }
}
