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

fn is_duplicate_semantic_event(
    previous: Option<&CoreLoopEventSemanticKey>,
    current: &CoreLoopEventSemanticKey,
) -> bool {
    !event_is_repeatable(&current.kind) && previous == Some(current)
}

fn contract_issuer_unavailable_failure(error: &CoreLoopError) -> bool {
    error.reducer_code() == Some(ReducerErrorCode::ContractIssuerUnavailable)
}

fn merchant_provider_unavailable_failure(error: &CoreLoopError) -> bool {
    error.reducer_code() == Some(ReducerErrorCode::MerchantProviderUnavailable)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CoreLoopFailureCategory {
    CoreLoopError,
    DialogueActionFailed,
    DiscoveryContactFailed,
    EquipmentPurchaseFailed,
    InvestigationActionFailed,
    InvestigationStateChanged,
    InvestigationTemporallyUnavailable,
    InvalidInvestigationRoute,
    JourneyProvisionPurchaseFailed,
    JourneyTemporallyUnavailable,
    JourneyTravelFailed,
    MedicalInterventionFailed,
    MedicalPurchaseFailed,
    RestActionFailed,
    SurvivalPurchaseFailed,
}

impl CoreLoopFailureCategory {
    const fn as_str(self) -> &'static str {
        match self {
            Self::CoreLoopError => "core_loop_error",
            Self::DialogueActionFailed => "dialogue_action_failed",
            Self::DiscoveryContactFailed => "discovery_contact_failed",
            Self::EquipmentPurchaseFailed => "equipment_purchase_failed",
            Self::InvestigationActionFailed => "investigation_action_failed",
            Self::InvestigationStateChanged => "investigation_state_changed",
            Self::InvestigationTemporallyUnavailable => "investigation_temporally_unavailable",
            Self::InvalidInvestigationRoute => "invalid_investigation_route",
            Self::JourneyProvisionPurchaseFailed => "journey_provision_purchase_failed",
            Self::JourneyTemporallyUnavailable => "journey_temporally_unavailable",
            Self::JourneyTravelFailed => "journey_travel_failed",
            Self::MedicalInterventionFailed => "medical_intervention_failed",
            Self::MedicalPurchaseFailed => "medical_purchase_failed",
            Self::RestActionFailed => "rest_action_failed",
            Self::SurvivalPurchaseFailed => "survival_purchase_failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CoreLoopFailureReason {
    AmmunitionPurchaseFailed,
    DialogueActionFailed,
    DiscoveryContactFailed,
    EquipmentStorefrontTradeFailed,
    InvestigationActionFailed,
    InvestigationActionStale,
    InvestigationActionUnavailable,
    InvestigationNightWindow,
    InvestigationVictimCohortStateChanged,
    InvestigationWaitFailed,
    InvalidInvestigationRoute,
    JourneyDaylightWindowRestRequired,
    JourneyProvisionPurchaseFailed,
    JourneyTravelReducerFailed,
    MedicalInterventionFailed,
    MedicalPurchaseFailed,
    PartyTentPurchaseFailed,
    RestActionFailed,
    UnclassifiedCoreLoopError,
}

impl CoreLoopFailureReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::AmmunitionPurchaseFailed => "ammunition_purchase_failed",
            Self::DialogueActionFailed => "dialogue_action_failed",
            Self::DiscoveryContactFailed => "discovery_contact_failed",
            Self::EquipmentStorefrontTradeFailed => "equipment_storefront_trade_failed",
            Self::InvestigationActionFailed => "investigation_action_failed",
            Self::InvestigationActionStale => "investigation_action_stale",
            Self::InvestigationActionUnavailable => "investigation_action_unavailable",
            Self::InvestigationNightWindow => "investigation_night_window",
            Self::InvestigationVictimCohortStateChanged => {
                "investigation_victim_cohort_state_changed"
            }
            Self::InvestigationWaitFailed => "investigation_wait_failed",
            Self::InvalidInvestigationRoute => "invalid_investigation_route",
            Self::JourneyDaylightWindowRestRequired => "journey_daylight_window_rest_required",
            Self::JourneyProvisionPurchaseFailed => "journey_provision_purchase_failed",
            Self::JourneyTravelReducerFailed => "journey_travel_reducer_failed",
            Self::MedicalInterventionFailed => "medical_intervention_failed",
            Self::MedicalPurchaseFailed => "medical_purchase_failed",
            Self::PartyTentPurchaseFailed => "party_tent_purchase_failed",
            Self::RestActionFailed => "rest_action_failed",
            Self::UnclassifiedCoreLoopError => "unclassified_core_loop_error",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CoreLoopFailureProjection {
    category: CoreLoopFailureCategory,
    reason: CoreLoopFailureReason,
    operation: Option<ReducerOperation>,
    message: &'static str,
}

fn project_core_loop_failure(error: &CoreLoopError) -> CoreLoopFailureProjection {
    let operation = error.operation_kind();
    let coded = match error.reducer_code() {
        Some(ReducerErrorCode::JourneyDaylightWindowRequired) => Some((
            CoreLoopFailureCategory::JourneyTemporallyUnavailable,
            CoreLoopFailureReason::JourneyDaylightWindowRestRequired,
            "Camp travel was continued outside its public projected walking window.",
        )),
        Some(ReducerErrorCode::InvestigationNightWindow) => Some((
            CoreLoopFailureCategory::InvestigationTemporallyUnavailable,
            CoreLoopFailureReason::InvestigationNightWindow,
            "A projected investigation action was attempted outside its learned time window.",
        )),
        Some(ReducerErrorCode::InvestigationRouteInvalid) => Some((
            CoreLoopFailureCategory::InvalidInvestigationRoute,
            CoreLoopFailureReason::InvalidInvestigationRoute,
            "The projected investigation route no longer has a coherent completed origin.",
        )),
        Some(ReducerErrorCode::InvestigationActionStale) => Some((
            CoreLoopFailureCategory::InvestigationStateChanged,
            CoreLoopFailureReason::InvestigationActionStale,
            "A publicly projected investigation target changed before the action completed.",
        )),
        Some(ReducerErrorCode::InvestigationActionUnavailable) => Some((
            CoreLoopFailureCategory::InvestigationStateChanged,
            CoreLoopFailureReason::InvestigationActionUnavailable,
            "A publicly projected investigation target changed before the action completed.",
        )),
        Some(ReducerErrorCode::VictimCohortStateChanged) => Some((
            CoreLoopFailureCategory::InvestigationStateChanged,
            CoreLoopFailureReason::InvestigationVictimCohortStateChanged,
            "A publicly projected investigation target changed before the action completed.",
        )),
        Some(ReducerErrorCode::DialogueContactUnavailable) => Some((
            CoreLoopFailureCategory::DiscoveryContactFailed,
            CoreLoopFailureReason::DiscoveryContactFailed,
            "A public discovery contact could not be completed.",
        )),
        Some(
            ReducerErrorCode::ContractIssuerUnavailable
            | ReducerErrorCode::MerchantProviderUnavailable,
        )
        | None => None,
    };
    if let Some((category, reason, message)) = coded {
        return CoreLoopFailureProjection {
            category,
            reason,
            operation,
            message,
        };
    }

    let (category, reason, message) = match operation {
        Some(operation) if operation.is_travel() => (
            CoreLoopFailureCategory::JourneyTravelFailed,
            CoreLoopFailureReason::JourneyTravelReducerFailed,
            "The authoritative journey transition could not be completed.",
        ),
        Some(ReducerOperation::StartDiscoveryDialogue) => (
            CoreLoopFailureCategory::DiscoveryContactFailed,
            CoreLoopFailureReason::DiscoveryContactFailed,
            "A public discovery contact could not be completed.",
        ),
        Some(ReducerOperation::PurchaseJourneyProvisions) => (
            CoreLoopFailureCategory::JourneyProvisionPurchaseFailed,
            CoreLoopFailureReason::JourneyProvisionPurchaseFailed,
            "The public journey-provision purchase could not be completed.",
        ),
        Some(ReducerOperation::PurchasePartyTent) => (
            CoreLoopFailureCategory::SurvivalPurchaseFailed,
            CoreLoopFailureReason::PartyTentPurchaseFailed,
            "The public party-shelter purchase could not be completed.",
        ),
        Some(ReducerOperation::PurchaseAmmunition | ReducerOperation::WithdrawPurchaseCoin) => (
            CoreLoopFailureCategory::SurvivalPurchaseFailed,
            CoreLoopFailureReason::AmmunitionPurchaseFailed,
            "The public ammunition preparation could not be completed.",
        ),
        Some(ReducerOperation::PurchaseFromHerbalist) => (
            CoreLoopFailureCategory::MedicalPurchaseFailed,
            CoreLoopFailureReason::MedicalPurchaseFailed,
            "The selected public herbalist preparation could not be purchased.",
        ),
        Some(ReducerOperation::PurchasePersonalStorefrontWithPartyStake) => (
            CoreLoopFailureCategory::EquipmentPurchaseFailed,
            CoreLoopFailureReason::EquipmentStorefrontTradeFailed,
            "The revalidated public equipment purchase was rejected by authoritative storefront rules.",
        ),
        Some(ReducerOperation::AdministerPreparation) => (
            CoreLoopFailureCategory::MedicalInterventionFailed,
            CoreLoopFailureReason::MedicalInterventionFailed,
            "The selected public preparation was rejected by authoritative intervention rules.",
        ),
        Some(ReducerOperation::PerformInvestigationAction) => (
            CoreLoopFailureCategory::InvestigationActionFailed,
            CoreLoopFailureReason::InvestigationActionFailed,
            "The authoritative investigation action could not be completed.",
        ),
        Some(
            ReducerOperation::WaitForInvestigationWindowSettlement
            | ReducerOperation::WaitForInvestigationWindowCamp,
        ) => (
            CoreLoopFailureCategory::InvestigationActionFailed,
            CoreLoopFailureReason::InvestigationWaitFailed,
            "The authoritative investigation action could not be completed.",
        ),
        Some(
            ReducerOperation::PassiveNoActionableRest
            | ReducerOperation::SponsorPartyMemberInnRest
            | ReducerOperation::SettlementActivityRest,
        ) => (
            CoreLoopFailureCategory::RestActionFailed,
            CoreLoopFailureReason::RestActionFailed,
            "The authoritative rest action could not be completed.",
        ),
        Some(ReducerOperation::ChooseDialogueTopic) => (
            CoreLoopFailureCategory::DialogueActionFailed,
            CoreLoopFailureReason::DialogueActionFailed,
            "The authoritative dialogue action could not be completed.",
        ),
        Some(_) | None => (
            CoreLoopFailureCategory::CoreLoopError,
            CoreLoopFailureReason::UnclassifiedCoreLoopError,
            "The authoritative core loop stopped before completion.",
        ),
    };
    CoreLoopFailureProjection {
        category,
        reason,
        operation,
        message,
    }
}

#[derive(Clone)]
struct FailureRecorder {
    output: Option<PathBuf>,
    fixture_disease: String,
    draft: std::sync::Arc<std::sync::Mutex<FailureDraft>>,
    error: std::sync::Arc<std::sync::Mutex<Option<CoreLoopError>>>,
}

impl FailureRecorder {
    fn new(output: Option<PathBuf>, fixture_disease: String) -> Self {
        Self {
            output,
            fixture_disease,
            draft: Default::default(),
            error: Default::default(),
        }
    }

    fn update(&self, draft: FailureDraft) {
        if let Ok(mut current) = self.draft.lock() {
            *current = draft;
        }
    }

    fn record(&self, error: CoreLoopError) {
        if let Ok(mut current) = self.error.lock() {
            *current = Some(error);
        }
    }

    fn write(&self, rendered_error: &str) -> Result<(), String> {
        let Some(path) = &self.output else {
            return Ok(());
        };
        let error = self
            .error
            .lock()
            .map_err(|_| "failure error state was unavailable".to_string())?
            .clone()
            .unwrap_or_else(|| CoreLoopError::Other(rendered_error.to_owned()));
        let projection = project_core_loop_failure(&error);
        let draft = self
            .draft
            .lock()
            .map_err(|_| "failure diagnostic state was unavailable".to_string())?
            .clone();
        let artifact = CoreLoopFailureArtifact {
            schema_version: CORE_LOOP_FAILURE_SCHEMA_VERSION,
            category: projection.category.as_str().into(),
            message: projection.message.into(),
            operation: projection
                .operation
                .map(|operation| operation.as_str().to_owned()),
            reason_code: projection.reason.as_str().into(),
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

const MAX_FAILURE_TRACE_EVENTS: usize = 64;
const CORE_LOOP_FAILURE_SCHEMA_VERSION: u32 = 9;
const MAX_PROJECTED_INVESTIGATION_WAIT_MINUTES: u32 = MINUTES_PER_DAY as u32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreLoopFailureAgent {
    pub agent_id: u32,
    pub character_id: u64,
    pub alive: bool,
    pub condition_status: DomainIncapacitationStatus,
    pub thermal: f32,
    pub wetness_bps: u16,
    pub thermal_strain: i32,
    pub ammunition: u32,
    pub carried_load_kg: f32,
    pub carry_capacity_kg: f32,
    pub encumbrance_remaining_bps: u32,
    pub equipment_ready: bool,
    pub party_tent_quantity: u32,
    pub hunger: f32,
    pub thirst: f32,
    pub food_days: f32,
    pub water_days: f32,
    pub visible_food_kcal: f32,
    pub visible_water_ml: f32,
    pub personal_gold_coin: u64,
    pub settlement_id: Option<String>,
    pub current_case_site_id: Option<String>,
    pub journey_destination: Option<String>,
    pub symptomatic: bool,
    pub critical: bool,
    pub settlement_services: Vec<String>,
    pub visible_herbalist_quote: Option<u64>,
    pub visible_inn_full_board_cost: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreLoopFailureArtifact {
    pub schema_version: u32,
    pub category: String,
    pub message: String,
    pub operation: Option<String>,
    pub reason_code: String,
    pub fixture_disease: String,
    pub metrics: CoreLoopMetrics,
    pub quest_coverage: Option<QuestCoverageEvidence>,
    pub total_event_count: u64,
    pub trace_truncated: bool,
    pub trace: Vec<CoreLoopEvent>,
    pub final_agents: Vec<CoreLoopFailureAgent>,
}

#[derive(Clone, Debug, Default)]
struct FailureDraft {
    metrics: CoreLoopMetrics,
    total_event_count: u64,
    trace_truncated: bool,
    trace: Vec<CoreLoopEvent>,
    final_agents: Vec<CoreLoopFailureAgent>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct PublicSurvivalObservation {
    thermal: f32,
    wetness_bps: u16,
    thermal_strain: i32,
    ammunition: u32,
    carried_load_kg: f32,
    carry_capacity_kg: f32,
    encumbrance_remaining_bps: u32,
    equipment_ready: bool,
    party_tent_quantity: u32,
}
