fn event_is_repeatable(kind: &CoreLoopEventKind) -> bool {
    matches!(
        kind,
        CoreLoopEventKind::Camp
            | CoreLoopEventKind::Recover
            | CoreLoopEventKind::Travel
            | CoreLoopEventKind::AutoresolveDefeat
            | CoreLoopEventKind::QuestDecision
            | CoreLoopEventKind::GeneratedInvestigationWait
    )
}

fn contract_issuer_unavailable_failure(error: &str) -> bool {
    parse_reducer_error(error) == Some(ReducerErrorCode::ContractIssuerUnavailable)
}

fn merchant_provider_unavailable_failure(error: &str) -> bool {
    parse_reducer_error(error) == Some(ReducerErrorCode::MerchantProviderUnavailable)
}

#[derive(Clone)]
struct FailureRecorder {
    output: Option<PathBuf>,
    fixture_disease: String,
    draft: std::sync::Arc<std::sync::Mutex<FailureDraft>>,
}

impl FailureRecorder {
    fn new(output: Option<PathBuf>, fixture_disease: String) -> Self {
        Self {
            output,
            fixture_disease,
            draft: Default::default(),
        }
    }

    fn update(&self, draft: FailureDraft) {
        if let Ok(mut current) = self.draft.lock() {
            *current = draft;
        }
    }

    fn write(&self, error: &str) -> Result<(), String> {
        let Some(path) = &self.output else {
            return Ok(());
        };
        let (category, message) = safe_core_loop_failure(error);
        let draft = self
            .draft
            .lock()
            .map_err(|_| "failure diagnostic state was unavailable".to_string())?
            .clone();
        let artifact = CoreLoopFailureArtifact {
            schema_version: CORE_LOOP_FAILURE_SCHEMA_VERSION,
            category: category.into(),
            message: message.into(),
            operation: safe_failure_operation(error).map(|operation| operation.as_str().to_owned()),
            reason_code: safe_failure_reason_code(error, category).into(),
            fixture_disease: self.fixture_disease.clone(),
            metrics: draft.metrics,
            quest_coverage: None,
            total_event_count: draft.total_event_count,
            trace_truncated: draft.trace_truncated,
            trace: draft.trace,
            final_agents: draft.final_agents,
        };
        let bytes = serde_json::to_vec_pretty(&artifact).map_err(|error| error.to_string())?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        use std::io::Write as _;
        options
            .open(path)
            .and_then(|mut file| {
                file.write_all(&bytes)?;
                file.write_all(b"\n")
            })
            .map_err(|error| format!("could not write failure diagnostic: {error}"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FailureOperation {
    PerformInvestigationAction,
    WaitForInvestigationWindowSettlement,
    WaitForInvestigationWindowCamp,
    StartDiscoveryDialogue,
    ChooseDialogueTopic,
    RestAtCamp,
    ContinueCampTravel,
    TravelCamps,
    PassiveNoActionableRest,
    SponsorPartyMemberInnRest,
    SettlementActivityRest,
    PurchaseJourneyProvisions,
    PurchasePartyTent,
    PurchaseAmmunition,
    WithdrawPurchaseCoin,
    PurchaseFromHerbalist,
    FinalizeStorefrontTrade,
    PurchasePersonalStorefrontWithPartyStake,
    AdministerPreparation,
    TravelToCaseSite,
    TravelToGeneratedCaseSite,
    UnsafeContractRetreatToSettlement,
    IllnessRetreatToSettlement,
    DefeatRetreatToSettlement,
    ReturnToSettlement,
    ReturnCompletedGeneratedCase,
    GeneratedUnchangedDefeatRetreat,
    GeneratedDefeatRetreatToSettlement,
    ReturnFromGeneratedCaseSite,
    ExpeditionHealthEvacuation,
}

impl FailureOperation {
    const ALL: [Self; 30] = [
        Self::PerformInvestigationAction,
        Self::WaitForInvestigationWindowSettlement,
        Self::WaitForInvestigationWindowCamp,
        Self::StartDiscoveryDialogue,
        Self::ChooseDialogueTopic,
        Self::RestAtCamp,
        Self::ContinueCampTravel,
        Self::TravelCamps,
        Self::PassiveNoActionableRest,
        Self::SponsorPartyMemberInnRest,
        Self::SettlementActivityRest,
        Self::PurchaseJourneyProvisions,
        Self::PurchasePartyTent,
        Self::PurchaseAmmunition,
        Self::WithdrawPurchaseCoin,
        Self::PurchaseFromHerbalist,
        Self::FinalizeStorefrontTrade,
        Self::PurchasePersonalStorefrontWithPartyStake,
        Self::AdministerPreparation,
        Self::TravelToCaseSite,
        Self::TravelToGeneratedCaseSite,
        Self::UnsafeContractRetreatToSettlement,
        Self::IllnessRetreatToSettlement,
        Self::DefeatRetreatToSettlement,
        Self::ReturnToSettlement,
        Self::ReturnCompletedGeneratedCase,
        Self::GeneratedUnchangedDefeatRetreat,
        Self::GeneratedDefeatRetreatToSettlement,
        Self::ReturnFromGeneratedCaseSite,
        Self::ExpeditionHealthEvacuation,
    ];

    const fn as_str(self) -> &'static str {
        match self {
            Self::PerformInvestigationAction => "perform_investigation_action",
            Self::WaitForInvestigationWindowSettlement => {
                "wait_for_investigation_window_settlement"
            }
            Self::WaitForInvestigationWindowCamp => "wait_for_investigation_window_camp",
            Self::StartDiscoveryDialogue => "start_discovery_dialogue",
            Self::ChooseDialogueTopic => "choose_dialogue_topic",
            Self::RestAtCamp => "rest_at_camp",
            Self::ContinueCampTravel => "continue_camp_travel",
            Self::TravelCamps => "travel_camps",
            Self::PassiveNoActionableRest => "passive_no_actionable_rest",
            Self::SponsorPartyMemberInnRest => "sponsor_party_member_inn_rest",
            Self::SettlementActivityRest => "settlement_activity_rest",
            Self::PurchaseJourneyProvisions => "purchase_journey_provisions",
            Self::PurchasePartyTent => "purchase_party_tent",
            Self::PurchaseAmmunition => "purchase_ammunition",
            Self::WithdrawPurchaseCoin => "withdraw_purchase_coin",
            Self::PurchaseFromHerbalist => "purchase_from_herbalist",
            Self::FinalizeStorefrontTrade => "finalize_storefront_trade",
            Self::PurchasePersonalStorefrontWithPartyStake => {
                "purchase_personal_storefront_with_party_stake"
            }
            Self::AdministerPreparation => "administer_preparation",
            Self::TravelToCaseSite => "travel_to_case_site",
            Self::TravelToGeneratedCaseSite => "travel_to_generated_case_site",
            Self::UnsafeContractRetreatToSettlement => "unsafe_contract_retreat_to_settlement",
            Self::IllnessRetreatToSettlement => "illness_retreat_to_settlement",
            Self::DefeatRetreatToSettlement => "defeat_retreat_to_settlement",
            Self::ReturnToSettlement => "return_to_settlement",
            Self::ReturnCompletedGeneratedCase => "return_completed_generated_case",
            Self::GeneratedUnchangedDefeatRetreat => "generated_unchanged_defeat_retreat",
            Self::GeneratedDefeatRetreatToSettlement => "generated_defeat_retreat_to_settlement",
            Self::ReturnFromGeneratedCaseSite => "return_from_generated_case_site",
            Self::ExpeditionHealthEvacuation => "expedition_health_evacuation",
        }
    }

    const fn is_travel(self) -> bool {
        matches!(
            self,
            Self::RestAtCamp
                | Self::ContinueCampTravel
                | Self::TravelCamps
                | Self::TravelToCaseSite
                | Self::TravelToGeneratedCaseSite
                | Self::UnsafeContractRetreatToSettlement
                | Self::IllnessRetreatToSettlement
                | Self::DefeatRetreatToSettlement
                | Self::ReturnToSettlement
                | Self::ReturnCompletedGeneratedCase
                | Self::GeneratedUnchangedDefeatRetreat
                | Self::GeneratedDefeatRetreatToSettlement
                | Self::ReturnFromGeneratedCaseSite
                | Self::ExpeditionHealthEvacuation
        )
    }
}

fn safe_failure_operation(error: &str) -> Option<FailureOperation> {
    FailureOperation::ALL.into_iter().find(|operation| {
        let name = operation.as_str();
        error.starts_with(&format!("{name} failed:"))
            || error.starts_with(&format!("{name} timed out"))
            || error.starts_with(&format!("could not send {name}:"))
    })
}

fn safe_failure_reason_code(error: &str, category: &str) -> &'static str {
    match parse_reducer_error(error) {
        Some(ReducerErrorCode::JourneyDaylightWindowRequired) => {
            return "journey_daylight_window_rest_required";
        }
        Some(ReducerErrorCode::InvestigationNightWindow) => return "investigation_night_window",
        Some(ReducerErrorCode::InvestigationRouteInvalid) => {
            return "invalid_investigation_route";
        }
        Some(ReducerErrorCode::InvestigationActionStale) => {
            return "investigation_action_stale";
        }
        Some(ReducerErrorCode::InvestigationActionUnavailable) => {
            return "investigation_action_unavailable";
        }
        Some(ReducerErrorCode::VictimCohortStateChanged) => {
            return "investigation_victim_cohort_state_changed";
        }
        Some(
            ReducerErrorCode::ContractIssuerUnavailable
            | ReducerErrorCode::MerchantProviderUnavailable,
        )
        | None => {}
    }
    match safe_failure_operation(error) {
        Some(operation) if operation.is_travel() => "journey_travel_reducer_failed",
        Some(FailureOperation::StartDiscoveryDialogue) => "discovery_contact_failed",
        Some(FailureOperation::PurchaseJourneyProvisions) => "journey_provision_purchase_failed",
        Some(FailureOperation::PurchasePartyTent) => "party_tent_purchase_failed",
        Some(FailureOperation::PurchaseAmmunition | FailureOperation::WithdrawPurchaseCoin) => {
            "ammunition_purchase_failed"
        }
        Some(FailureOperation::PurchaseFromHerbalist) => "medical_purchase_failed",
        Some(
            FailureOperation::FinalizeStorefrontTrade
            | FailureOperation::PurchasePersonalStorefrontWithPartyStake,
        ) => "equipment_storefront_trade_failed",
        Some(FailureOperation::AdministerPreparation) => "medical_intervention_failed",
        Some(FailureOperation::PerformInvestigationAction) => "investigation_action_failed",
        Some(
            FailureOperation::WaitForInvestigationWindowSettlement
            | FailureOperation::WaitForInvestigationWindowCamp,
        ) => "investigation_wait_failed",
        Some(
            FailureOperation::PassiveNoActionableRest
            | FailureOperation::SponsorPartyMemberInnRest
            | FailureOperation::SettlementActivityRest,
        ) => "rest_action_failed",
        Some(FailureOperation::ChooseDialogueTopic) => "dialogue_action_failed",
        Some(_) | None => match category {
            "rest_service_unavailable" => "rest_service_unavailable",
            "insufficient_visible_resources" => "insufficient_visible_resources",
            "bounded_progress_exhausted" => "bounded_progress_exhausted",
            "authoritative_backend_unavailable" => "authoritative_backend_unavailable",
            "invalid_run_environment" => "invalid_run_environment",
            _ => "unclassified_core_loop_error",
        },
    }
}

fn safe_core_loop_failure(error: &str) -> (&'static str, &'static str) {
    match parse_reducer_error(error) {
        Some(ReducerErrorCode::JourneyDaylightWindowRequired) => {
            return (
                "journey_temporally_unavailable",
                "Camp travel was continued outside its public projected walking window.",
            );
        }
        Some(ReducerErrorCode::InvestigationNightWindow) => {
            return (
                "investigation_temporally_unavailable",
                "A projected investigation action was attempted outside its learned time window.",
            );
        }
        Some(ReducerErrorCode::InvestigationRouteInvalid) => {
            return (
                "invalid_investigation_route",
                "The projected investigation route no longer has a coherent completed origin.",
            );
        }
        Some(
            ReducerErrorCode::InvestigationActionStale
            | ReducerErrorCode::InvestigationActionUnavailable
            | ReducerErrorCode::VictimCohortStateChanged,
        ) => {
            return (
                "investigation_state_changed",
                "A publicly projected investigation target changed before the action completed.",
            );
        }
        Some(
            ReducerErrorCode::ContractIssuerUnavailable
            | ReducerErrorCode::MerchantProviderUnavailable,
        )
        | None => {}
    }
    match safe_failure_operation(error) {
        Some(operation) if operation.is_travel() => (
            "journey_travel_failed",
            "The authoritative journey transition could not be completed.",
        ),
        Some(FailureOperation::StartDiscoveryDialogue) => (
            "discovery_contact_failed",
            "A public discovery contact could not be completed.",
        ),
        Some(FailureOperation::PurchaseJourneyProvisions) => (
            "journey_provision_purchase_failed",
            "The public journey-provision purchase could not be completed.",
        ),
        Some(FailureOperation::PurchasePartyTent) => (
            "survival_purchase_failed",
            "The public party-shelter purchase could not be completed.",
        ),
        Some(FailureOperation::PurchaseAmmunition | FailureOperation::WithdrawPurchaseCoin) => (
            "survival_purchase_failed",
            "The public ammunition preparation could not be completed.",
        ),
        Some(FailureOperation::PurchaseFromHerbalist) => (
            "medical_purchase_failed",
            "The selected public herbalist preparation could not be purchased.",
        ),
        Some(
            FailureOperation::FinalizeStorefrontTrade
            | FailureOperation::PurchasePersonalStorefrontWithPartyStake,
        ) => (
            "equipment_purchase_failed",
            "The revalidated public equipment purchase was rejected by authoritative storefront rules.",
        ),
        Some(FailureOperation::AdministerPreparation) => (
            "medical_intervention_failed",
            "The selected public preparation was rejected by authoritative intervention rules.",
        ),
        Some(
            FailureOperation::PerformInvestigationAction
            | FailureOperation::WaitForInvestigationWindowSettlement
            | FailureOperation::WaitForInvestigationWindowCamp,
        ) => (
            "investigation_action_failed",
            "The authoritative investigation action could not be completed.",
        ),
        Some(FailureOperation::ChooseDialogueTopic) => (
            "dialogue_action_failed",
            "The authoritative dialogue action could not be completed.",
        ),
        Some(
            FailureOperation::PassiveNoActionableRest
            | FailureOperation::SponsorPartyMemberInnRest
            | FailureOperation::SettlementActivityRest,
        ) => (
            "rest_action_failed",
            "The authoritative rest action could not be completed.",
        ),
        Some(_) | None => (
            "core_loop_error",
            "The authoritative core loop stopped before completion.",
        ),
    }
}

fn bounded_failure_trace(
    trace: &[CoreLoopEvent],
    total_event_count: u64,
) -> (Vec<CoreLoopEvent>, bool) {
    let start = trace.len().saturating_sub(MAX_FAILURE_TRACE_EVENTS);
    (
        trace[start..].to_vec(),
        total_event_count > MAX_FAILURE_TRACE_EVENTS as u64,
    )
}
