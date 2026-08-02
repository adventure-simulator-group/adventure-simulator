use super::{
    ArgumentValue, Capability, ChoiceArguments, ChoiceKind, DeveloperCaseAnalysis, DiscoveryView,
    EVAL_FORMAT_VERSION, JournalView, LegalChoice, LocationResolution, PartyView, PlayerFrame,
    PolicyClassification, PublicClaim, PublicDialogueLine, PublicEvidence, PublicLocation,
    PublicQuestTrace, PublicTraceEvent, Termination, TerminationErrorCode, WitnessAvailability,
    WitnessReferral,
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
    settlement_name: String,
    current_location: String,
    frame: PlayerFrame,
    capabilities: BTreeMap<String, Capability>,
    tavern_entered: bool,
    visible_witnesses: BTreeSet<usize>,
    interviewed: BTreeSet<usize>,
    completed_actions: BTreeSet<usize>,
    completed_remediations: BTreeSet<String>,
    exact_sites: BTreeSet<String>,
    visited_sites: BTreeSet<String>,
    prepared: BTreeSet<String>,
    /// Ordinary schedules are player-visible state, not a pipeline error.
    witness_returns_at: BTreeMap<usize, u64>,
    trace: Vec<PublicTraceEvent>,
    completed_action_provenance: Vec<CompletedAction>,
    route: Option<RouteClass>,
    solved: bool,
}

#[derive(Clone, Debug)]
struct CompletedAction {
    action_index: usize,
    route: RouteClass,
    target_kind: String,
    target_id: String,
}

impl InvestigationEnvironment {
    pub fn generate(config: EvalCaseConfig) -> Result<Self, String> {
        let context = generation_context(config.seed, config.family);
        let settlement_name = context.settlement_name.clone();
        let generated = qg::generate(&context)
            .map_err(|error| format!("quest generation failed: {error:?}"))?;
        Self::from_generated_at(generated, config.party, settlement_name)
    }

    pub fn from_generated(generated: GeneratedCase, party: PartyView) -> Result<Self, String> {
        Self::from_generated_at(generated, party, "the settlement".into())
    }

    fn from_generated_at(
        generated: GeneratedCase,
        party: PartyView,
        settlement_name: String,
    ) -> Result<Self, String> {
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
            current_location: settlement_name.clone(),
            settlement_name,
            frame,
            capabilities: BTreeMap::new(),
            tavern_entered: false,
            visible_witnesses: BTreeSet::new(),
            interviewed: BTreeSet::new(),
            completed_actions: BTreeSet::new(),
            completed_remediations: BTreeSet::new(),
            exact_sites: BTreeSet::new(),
            visited_sites: BTreeSet::new(),
            prepared: BTreeSet::new(),
            witness_returns_at,
            trace: Vec::new(),
            completed_action_provenance: Vec::new(),
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
        let action_label = legal.label.clone();
        let game_minute = self.frame.game_minute;
        let action_step = self.frame.step;
        let pre_observation_digest = semantic_digest(&self.frame)?;
        let waiting_for_witness = matches!(&capability, Capability::WaitForWitness(_));
        let mut learned = Vec::new();
        let mut learned_claim_ids = Vec::new();
        let mut dialogue = Vec::new();
        let mut corrected_proposition_ids = Vec::new();
        let (result, minutes, cost) = match capability {
            Capability::EnterTavern => {
                self.current_location = format!("{} tavern", self.settlement_name);
                self.tavern_entered = true;
                self.frame.discovery.problem_summary =
                    self.generated.consequence.public_summary.clone();
                self.frame.discovery.consequence_summary =
                    format!("{:?}", self.generated.consequence.symptom);
                self.frame.discovery.learned_at = "settlement tavern rumor".into();
                if let Some(witness) = self.generated.witnesses.first() {
                    self.visible_witnesses.insert(0);
                    self.frame.discovery.referrals = vec![WitnessReferral {
                        witness_id: opaque_handle("witness", 0),
                        display_name: witness.display_name.clone(),
                        physical_description: witness.visible_description.clone(),
                        expected_location: witness.expected_location_label.clone(),
                        interviewed: false,
                        availability: WitnessAvailability::Available,
                    }];
                }
                self.refresh_witness_availability();
                learned.push(self.generated.consequence.public_summary.clone());
                dialogue.push(PublicDialogueLine {
                    speaker: "Tavern keeper".into(),
                    text: format!(
                        "Locals have been saying: {}",
                        self.generated.consequence.public_summary
                    ),
                });
                dialogue.extend(self.frame.discovery.referrals.iter().map(|referral| {
                    PublicDialogueLine {
                        speaker: "Tavern keeper".into(),
                        text: format!(
                            "Ask {}—{}, usually found at {}.",
                            referral.display_name,
                            referral.physical_description,
                            referral.expected_location
                        ),
                    }
                }));
                (
                    "The tavern's talk reveals a local problem and witness referrals.".into(),
                    15,
                    0,
                )
            }
            Capability::Interview(index) => {
                if !self.visible_witnesses.contains(&index) {
                    return Err("witness has not been referred to the player".into());
                }
                let witness = self
                    .generated
                    .witnesses
                    .get(index)
                    .ok_or("stale witness")?
                    .clone();
                self.current_location = witness.expected_location_label.clone();
                self.interviewed.insert(index);
                if let Some(referral) = self
                    .frame
                    .discovery
                    .referrals
                    .iter_mut()
                    .find(|referral| referral.witness_id == opaque_handle("witness", index))
                {
                    referral.interviewed = true;
                }
                for (_, statement) in qg::initial_testimony_projection(&witness) {
                    let claim_id = claim_handle(&self.generated, &statement.proposition_id);
                    self.frame.journal.claims.push(PublicClaim {
                        proposition_id: claim_id.clone(),
                        source: witness.visible_description.clone(),
                        text: statement.spoken_text.clone(),
                    });
                    learned_claim_ids.push(claim_id);
                    if let Some(corrected) = &statement.corrects_proposition_id {
                        let handle = claim_handle(&self.generated, corrected);
                        self.frame.journal.corrections.push(handle.clone());
                        corrected_proposition_ids.push(handle);
                    }
                    learned.push(statement.spoken_text.clone());
                    dialogue.push(PublicDialogueLine {
                        speaker: witness.display_name.clone(),
                        text: statement.spoken_text.clone(),
                    });
                    for referred in &statement.referred_witness_ids {
                        if let Some((referred_index, referred_witness)) = self
                            .generated
                            .witnesses
                            .iter()
                            .enumerate()
                            .find(|(_, candidate)| candidate.id == *referred)
                        {
                            if self.visible_witnesses.insert(referred_index) {
                                self.frame.discovery.referrals.push(WitnessReferral {
                                    witness_id: opaque_handle("witness", referred_index),
                                    display_name: referred_witness.display_name.clone(),
                                    physical_description: referred_witness
                                        .visible_description
                                        .clone(),
                                    expected_location: referred_witness
                                        .expected_location_label
                                        .clone(),
                                    interviewed: false,
                                    availability: WitnessAvailability::Available,
                                });
                            }
                        }
                    }
                }
                (
                    "The witness's account is recorded with its source.".into(),
                    20,
                    0,
                )
            }
            Capability::WaitForWitness(index) => {
                if let Some(witness) = self.generated.witnesses.get(index) {
                    self.current_location = witness.expected_location_label.clone();
                }
                let return_at = *self
                    .witness_returns_at
                    .get(&index)
                    .ok_or("witness has no scheduled return")?;
                let wait = return_at.saturating_sub(self.frame.game_minute).max(15);
                self.frame.game_minute = return_at;
                self.refresh_witness_availability();
                learned.push("The referred witness returns to their expected location.".into());
                (
                    "The party waits rather than treating an ordinary absence as a failure.".into(),
                    wait as u32,
                    1,
                )
            }
            Capability::Action(index, _action_kind, route) => {
                let action = self.generated.actions.get(index).ok_or("stale action")?;
                self.current_location = self.action_location(action);
                if action.target_kind == "site" && !self.visited_sites.contains(&action.target_id) {
                    return Err("site action requires authoritative occupancy".into());
                }
                self.completed_actions.insert(index);
                self.completed_action_provenance.push(CompletedAction {
                    action_index: index,
                    route,
                    target_kind: action.target_kind.clone(),
                    target_id: action.target_id.clone(),
                });
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
                            if let Some((evidence_index, evidence)) = self
                                .generated
                                .evidence
                                .iter()
                                .enumerate()
                                .find(|(_, item)| &item.id == evidence_id)
                            {
                                self.frame.journal.evidence.push(PublicEvidence {
                                    evidence_id: opaque_handle("evidence", evidence_index),
                                    description: evidence.safe_description.clone(),
                                    discovery_source: action.safe_summary.clone(),
                                });
                                if let Some(corrected) = &evidence.corrects_proposition_id {
                                    let handle = claim_handle(&self.generated, corrected);
                                    self.frame.journal.corrections.push(handle.clone());
                                    corrected_proposition_ids.push(handle);
                                }
                                learned.push(evidence.safe_description.clone());
                            }
                        }
                        GeneratedActionOutput::TrackFinding { finding, .. } => {
                            learned.push(finding.clone());
                        }
                        GeneratedActionOutput::AmbushReady => {
                            learned.push("The party has established an ambush position.".into());
                        }
                        GeneratedActionOutput::Consequence { .. } => {
                            learned.push(
                                "The site investigation produced a recoverable result.".into(),
                            );
                        }
                        GeneratedActionOutput::Remediation { remediation_id } => {
                            let exact_objective = self
                                .generated
                                .objectives
                                .alternatives
                                .iter()
                                .flat_map(|path| &path.objectives)
                                .any(|objective| {
                                    matches!(
                                        &objective.requirement,
                                        adventuresim_core::case::ObjectiveRequirement::RemediateSource {
                                            remediation_id: required
                                        } if required == remediation_id
                                    )
                                });
                            if !exact_objective {
                                return Err(
                                    "action remediation does not match the generated objective"
                                        .into(),
                                );
                            }
                            self.completed_remediations.insert(remediation_id.clone());
                            self.route = Some(route);
                            self.solved = true;
                            learned.push(
                                "The supported intervention removes the outbreak source.".into(),
                            );
                        }
                        GeneratedActionOutput::SystemicOutcome { outcome } => match outcome {
                            // These values declare the typed systemic producer that can
                            // satisfy an objective. Completing an investigation action is
                            // not itself authority to fabricate that surrender, custody,
                            // recruitment, property, or theft fact in the evaluator.
                            qg::GeneratedSystemicOutcome::Surrender { .. }
                            | qg::GeneratedSystemicOutcome::RecruitOrDefect { .. }
                            | qg::GeneratedSystemicOutcome::Ransom { .. }
                            | qg::GeneratedSystemicOutcome::CustodyHandoff { .. }
                            | qg::GeneratedSystemicOutcome::EscapeCustody { .. }
                            | qg::GeneratedSystemicOutcome::TransferOwnership { .. }
                            | qg::GeneratedSystemicOutcome::Theft { .. } => {}
                        },
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
                    self.current_location = site.safe_label.clone();
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
            Capability::ResolveCarrier(route) => {
                let remediation_id = self
                    .generated
                    .objectives
                    .alternatives
                    .iter()
                    .flat_map(|path| &path.objectives)
                    .find_map(|objective| match &objective.requirement {
                        adventuresim_core::case::ObjectiveRequirement::RemediateSource {
                            remediation_id,
                        } => Some(remediation_id.clone()),
                        _ => None,
                    })
                    .ok_or("carrier outbreak has no exact remediation objective")?;
                let accepted = self
                    .generated
                    .outbreak
                    .as_ref()
                    .and_then(|outbreak| match &outbreak.remediation {
                        qg::OutbreakRemediation::ResolveCarrierThreat {
                            accepted_outcomes, ..
                        } => Some(accepted_outcomes),
                        _ => None,
                    })
                    .is_some_and(|outcomes| {
                        outcomes.contains(&qg::OutbreakCarrierOutcome::Defeated)
                    });
                if !accepted {
                    return Err(
                        "carrier outbreak does not accept the modeled hostile outcome".into(),
                    );
                }
                self.completed_remediations.insert(remediation_id);
                self.route = Some(route);
                self.solved = true;
                (
                    "The identified carrier group is defeated through the normal hostile outcome."
                        .into(),
                    120,
                    2,
                )
            }
        };
        self.frame.party.supplies = self.frame.party.supplies.saturating_sub(cost);
        let preparation_tags = if kind == ChoiceKind::Prepare {
            learned
                .iter()
                .filter_map(|item| item.strip_prefix("Prepared "))
                .map(|item| item.trim_end_matches('.').to_owned())
                .collect()
        } else {
            Vec::new()
        };
        self.frame.step += 1;
        if !waiting_for_witness {
            self.frame.game_minute += u64::from(minutes);
        }
        self.refresh_choices();
        let post_observation_digest = semantic_digest(&self.frame)?;
        self.trace.push(PublicTraceEvent {
            step: action_step,
            game_minute,
            location: self.current_location.clone(),
            observation_provenance: "offline_projection/player_frame".into(),
            pre_observation_digest,
            post_observation_digest,
            choice_id: decision.choice_id.clone(),
            choice_kind: kind,
            action_label,
            dialogue,
            result,
            learned,
            learned_claim_ids,
            corrected_proposition_ids,
            preparation_tags,
            game_minutes: minutes,
            resource_cost: cost,
        });
        Ok(())
    }

    pub fn is_solved(&self) -> bool {
        self.solved
    }

    pub fn public_trace(
        &self,
        policy: String,
        initial_observation_digest: String,
        initial_classification: PolicyClassification,
        termination: Termination,
        termination_error: Option<TerminationErrorCode>,
    ) -> Result<PublicQuestTrace, String> {
        let mut trace = PublicQuestTrace {
            version: EVAL_FORMAT_VERSION,
            case_id: self.frame.case_id.clone(),
            policy,
            title: format!("Investigation in {}", self.settlement_name),
            problem_summary: self.generated.consequence.public_summary.clone(),
            initial_observation_digest,
            initial_classification,
            events: self.trace.clone(),
            solved: self.solved,
            exhausted: self.frame.legal_choices.is_empty(),
            termination,
            termination_error,
            route: self.route,
            semantic_digest: String::new(),
        };
        trace.semantic_digest = semantic_digest(&trace)?;
        Ok(trace)
    }

    fn action_location(&self, action: &qg::GeneratedAction) -> String {
        match action.target_kind.as_str() {
            "site" => self
                .generated
                .sites
                .iter()
                .find(|site| site.id.0 == action.target_id)
                .map(|site| site.safe_label.clone()),
            "area" => self
                .generated
                .areas
                .iter()
                .find(|area| area.id == action.target_id)
                .map(|area| area.safe_label.clone()),
            "witness" => self
                .generated
                .witnesses
                .iter()
                .find(|witness| witness.id.0 == action.target_id)
                .map(|witness| witness.expected_location_label.clone()),
            _ => None,
        }
        .unwrap_or_else(|| self.current_location.clone())
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
                .filter(|(index, _)| self.visible_witnesses.contains(index))
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
                if let Some(route) = self.admissible_finale_route(&finale.site_id.0) {
                    self.push_choice(
                        &mut choices,
                        ChoiceKind::Conclude,
                        &format!("Attempt the {:?} finale.", finale.kind),
                        Capability::Conclude(route),
                    );
                    break;
                }
            }
            if let Some(route) = self.admissible_carrier_route() {
                self.push_choice(
                    &mut choices,
                    ChoiceKind::Conclude,
                    "Confront the identified carrier group.",
                    Capability::ResolveCarrier(route),
                );
            }
        }
        self.frame.legal_choices = choices;
    }

    fn action_available(&self, index: usize) -> bool {
        let action = &self.generated.actions[index];
        if action.target_kind == "contact"
            && !self.visible_witnesses.iter().any(|visible| {
                self.generated
                    .witnesses
                    .get(*visible)
                    .is_some_and(|witness| {
                        witness.resident_character_id.to_string() == action.target_id
                    })
            })
        {
            return false;
        }
        // A tavern referral alone is not enough to materialize an investigation
        // action. The player must first hear a source-attributed account.
        if action.active_initially && self.interviewed.is_empty() {
            return false;
        }
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

    fn admissible_finale_route(&self, finale_site_id: &str) -> Option<RouteClass> {
        self.completed_action_provenance
            .iter()
            .rev()
            .find_map(|completed| {
                (completed.target_kind == "site"
                    && completed.target_id == finale_site_id
                    && self.visited_sites.contains(finale_site_id)
                    && self.action_chain_complete(completed.action_index))
                .then_some(completed.route)
            })
    }

    fn admissible_carrier_route(&self) -> Option<RouteClass> {
        let outbreak = self.generated.outbreak.as_ref()?;
        if !matches!(
            &outbreak.remediation,
            qg::OutbreakRemediation::ResolveCarrierThreat { .. }
        ) || !self
            .visited_sites
            .contains(&outbreak.physical_source_site.0)
        {
            return None;
        }
        self.completed_actions.iter().rev().find_map(|index| {
            let action = &self.generated.actions[*index];
            (self.action_chain_complete(*index)
                && action.outputs.iter().any(|output| {
                    matches!(
                        output,
                        GeneratedActionOutput::Destination {
                            stage: GeneratedDestinationStage::Exact,
                            site_id: Some(site_id),
                        } if site_id == &outbreak.physical_source_site
                    )
                }))
            .then_some(action.route)
        })
    }

    fn action_chain_complete(&self, index: usize) -> bool {
        let Some(action) = self.generated.actions.get(index) else {
            return false;
        };
        match &action.prerequisite {
            None => self.completed_actions.contains(&index),
            Some(required) => {
                self.completed_actions.contains(&index)
                    && self
                        .generated
                        .actions
                        .iter()
                        .enumerate()
                        .find_map(|(prior, candidate)| (&candidate.id == required).then_some(prior))
                        .is_some_and(|prior| self.action_chain_complete(prior))
            }
        }
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
                || returns
                    .get(&index)
                    .is_none_or(|return_at| game_minute >= *return_at)
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
        factor_trace: case.factor_trace.clone(),
        bridges: case.bridges.clone(),
        generator_manifest_digest: private_digest,
    })
}

fn opaque_handle(kind: &str, index: usize) -> String {
    format!("{kind}:observed-{}", index + 1)
}

fn claim_handle(case: &GeneratedCase, proposition_id: &str) -> String {
    let index = qg::player_visible_testimony_sequence(case)
        .into_iter()
        .map(|(_, statement)| statement)
        .position(|statement| statement.proposition_id == proposition_id)
        .unwrap_or(usize::MAX);
    if index == usize::MAX {
        "claim:observed-unknown".into()
    } else {
        opaque_handle("claim", index)
    }
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
    let witness = |id: &str, display_name: &str, demographic, description: &str, location: &str| {
        WitnessCandidate {
            resident_character_id: adventuresim_core::settlement_population::stable_hash(&format!(
                "investigation-eval-witness:{id}"
            )) | (1u64 << 63),
            display_name: display_name.into(),
            demographic,
            age_band: "adult".into(),
            sex: "unspecified".into(),
            profession: id.into(),
            visible_description: description.into(),
            expected_location: location.into(),
            expected_location_label: location.into(),
            presence_version: 1,
            allowed_circumstances: circumstances.clone(),
        }
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
        incident_weather: adventuresim_core::weather::Precipitation::Clear,
        requested_family: Some(family),
        witness_candidates: vec![
            witness(
                "watchman",
                "Konrad",
                WitnessDemographic::Guard,
                "a tall watchman with cropped fair hair and a scarred chin",
                "the gatehouse",
            ),
            witness(
                "cooper",
                "Marta",
                WitnessDemographic::Laborer,
                "a short cooper with dark curls and a blue apron",
                "the riverside workshop",
            ),
            witness(
                "merchant",
                "Elsbeth",
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

    #[test]
    fn tavern_referral_requires_visible_testimony_before_roots() {
        let mut env = InvestigationEnvironment::generate(EvalCaseConfig::fixture(
            9,
            TemplateFamily::RecurringDepredation,
        ))
        .unwrap();
        let tavern = env.frame().legal_choices[0].choice_id.clone();
        env.apply(&PolicyDecision {
            version: EVAL_FORMAT_VERSION,
            choice_id: tavern,
            arguments: DecisionArguments::default(),
        })
        .unwrap();
        assert!(
            env.frame()
                .legal_choices
                .iter()
                .all(|choice| choice.kind != ChoiceKind::Investigate)
        );
        let witness = env
            .frame()
            .legal_choices
            .iter()
            .find(|choice| choice.kind == ChoiceKind::InterviewWitness)
            .unwrap()
            .choice_id
            .clone();
        env.apply(&PolicyDecision {
            version: EVAL_FORMAT_VERSION,
            choice_id: witness,
            arguments: DecisionArguments::default(),
        })
        .unwrap();
        assert!(
            env.frame()
                .legal_choices
                .iter()
                .any(|choice| choice.kind == ChoiceKind::Investigate)
        );
    }

    #[test]
    fn offline_projection_excludes_withheld_and_unreferred_testimony() {
        let context = generation_context(19, TemplateFamily::RecurringDepredation);
        let mut generated = qg::generate(&context).unwrap();
        generated.witnesses[0].testimony.push(qg::TestimonyDraft {
            proposition_id: "withheld-canary-proposition".into(),
            reliability: qg::Reliability::Truthful,
            delivery: qg::TestimonyDelivery::Withheld,
            truthful_text: "WITHHELD_CANARY".into(),
            spoken_text: "WITHHELD_CANARY".into(),
            challenge_text: "WITHHELD_CANARY".into(),
            challenge_responses: qg::TestimonyChallengeResponses {
                charm: Some("CANARY_CHARM".into()),
                command: None,
                bluff: None,
            },
            destination_stage: "textual".into(),
            site_id: None,
            corrects_proposition_id: None,
            referred_witness_ids: vec![],
        });
        for statement in &mut generated.witnesses[0].testimony {
            statement.referred_witness_ids.clear();
        }
        generated.witnesses[1].display_name = "UNREFERRED_CANARY".into();
        generated.witnesses[1].testimony[0].spoken_text = "UNREFERRED_CANARY".into();
        generated
            .actions
            .iter_mut()
            .find(|action| action.target_kind == "contact")
            .unwrap()
            .target_id = generated.witnesses[1].resident_character_id.to_string();
        let mut env = InvestigationEnvironment::from_generated(
            generated,
            EvalCaseConfig::fixture(19, TemplateFamily::RecurringDepredation).party,
        )
        .unwrap();
        let tavern = env.frame().legal_choices[0].choice_id.clone();
        env.apply(&PolicyDecision {
            version: EVAL_FORMAT_VERSION,
            choice_id: tavern,
            arguments: DecisionArguments::default(),
        })
        .unwrap();
        let primary = env
            .frame()
            .legal_choices
            .iter()
            .find(|choice| choice.kind == ChoiceKind::InterviewWitness)
            .unwrap()
            .choice_id
            .clone();
        env.apply(&PolicyDecision {
            version: EVAL_FORMAT_VERSION,
            choice_id: primary,
            arguments: DecisionArguments::default(),
        })
        .unwrap();
        let public = format!("{:?}", env.frame());
        assert!(!public.contains("WITHHELD_CANARY"));
        assert!(!public.contains("UNREFERRED_CANARY"));
        assert!(
            env.frame()
                .legal_choices
                .iter()
                .all(|choice| choice.label != "Find the referred witness.")
        );
    }

    #[test]
    fn mixed_route_progress_cannot_offer_an_unrelated_finale() {
        let mut env = InvestigationEnvironment::generate(EvalCaseConfig::fixture(
            17,
            TemplateFamily::DisappearanceOrLoss,
        ))
        .unwrap();
        let finale_site = env.generated.finales[0].site_id.0.clone();
        env.visited_sites.insert(finale_site.clone());
        let (action_index, route, target_kind, target_id) = env
            .generated
            .actions
            .iter()
            .enumerate()
            .find(|(_, action)| action.target_id != finale_site)
            .map(|(index, action)| {
                (
                    index,
                    action.route,
                    action.target_kind.clone(),
                    action.target_id.clone(),
                )
            })
            .unwrap();
        env.completed_actions.insert(action_index);
        env.completed_action_provenance.push(CompletedAction {
            action_index,
            route,
            target_kind,
            target_id,
        });
        assert_eq!(env.admissible_finale_route(&finale_site), None);
    }

    #[test]
    fn exact_outbreak_remediation_output_solves_the_generated_objective() {
        let mut env = InvestigationEnvironment::generate(EvalCaseConfig::fixture(
            0,
            TemplateFamily::Outbreak,
        ))
        .unwrap();
        env.tavern_entered = true;
        env.interviewed.insert(0);
        let action_index = env
            .generated
            .actions
            .iter()
            .position(|action| {
                action
                    .outputs
                    .iter()
                    .any(|output| matches!(output, GeneratedActionOutput::Remediation { .. }))
            })
            .unwrap();
        let action = env.generated.actions[action_index].clone();
        if let Some(required) = &action.prerequisite {
            let prior = env
                .generated
                .actions
                .iter()
                .position(|candidate| &candidate.id == required)
                .unwrap();
            env.completed_actions.insert(prior);
        }
        env.visited_sites.insert(action.target_id.clone());
        env.refresh_choices();
        let choice_id = env
            .capabilities
            .iter()
            .find_map(|(choice_id, capability)| {
                matches!(capability, Capability::Action(index, _, _) if *index == action_index)
                    .then(|| choice_id.clone())
            })
            .unwrap();
        env.apply(&PolicyDecision {
            version: EVAL_FORMAT_VERSION,
            choice_id,
            arguments: DecisionArguments::default(),
        })
        .unwrap();
        let remediation_id = action
            .outputs
            .iter()
            .find_map(|output| match output {
                GeneratedActionOutput::Remediation { remediation_id } => {
                    Some(remediation_id.as_str())
                }
                _ => None,
            })
            .unwrap();
        assert!(env.is_solved());
        assert_eq!(env.route, Some(action.route));
        assert!(env.completed_remediations.contains(remediation_id));
    }
}
