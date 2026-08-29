#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReducerOperation {
    AbandonDefeatedQuest,
    AbandonUnsafeActiveContract,
    AbandonUnsafeQuest,
    AcceptPartyJoinRequest,
    AcceptQuest,
    AdministerPreparation,
    AdvanceSimulationWorldTime,
    AutoresolveGeneratedMission,
    AutoresolveMission,
    ChooseDialogueTopic,
    ClaimSimulationRun,
    ConfigureSafeDepartureWindow,
    ConfigureSimulationCharacter,
    ContinueCampTravel,
    ContributeJourneyCurrency,
    CreateNamedCharacterWithId,
    DefeatRetreatToSettlement,
    EnsureSettlementActivity,
    EnsureSettlementActivityAfterEvacuation,
    EnsureSettlementActivityAfterIdleSiteReturn,
    EvacuateGeneratedCaseSite,
    ExpeditionHealthEvacuation,
    ExpeditionRecoveryRest,
    GeneratedCaseSitePlannedRecovery,
    GeneratedDefeatRetreatToSettlement,
    GeneratedUnchangedDefeatRetreat,
    GeneratedUnsafeCombatRetreat,
    IdleCaseSiteReturn,
    IllnessRetreatToSettlement,
    InstallActivitySchedule,
    InteractAcceptContract,
    InteractReportContract,
    LiquidatePartyInventory,
    MedicalRecoveryRest,
    NaturalIllnessRecoveryRest,
    NegotiateHostileWithdrawal,
    PassiveNoActionableRest,
    PauseScheduleForTreatment,
    PerformInvestigationAction,
    PurchaseAmmunition,
    PurchaseFirstAidMaterial,
    PurchaseFromHerbalist,
    PurchaseJourneyProvisions,
    PurchasePartyTent,
    PurchasePersonalStorefrontWithPartyStake,
    RegisterStrategicGateway,
    ReplaceItemAtPlacement,
    ReplenishQuestsAfterAbandon,
    RequestGeneralPartyJoin,
    ResolveErrantryRoadChallenge,
    ResolveStrategicEncounter,
    RestoreScheduleAfterTreatment,
    ResynchronizePartyAfterGeneratedPreflight,
    RestAtCamp,
    RetrieveRepairedItem,
    ReturnCompletedGeneratedCase,
    ReturnFromGeneratedCaseSite,
    ReturnToSettlement,
    SeedSimulationDisease,
    SeedSimulationEquipmentDamage,
    SeedSimulationQuestFixture,
    SeedSimulationWorld,
    SettlementActivityRest,
    SponsorPartyMemberInnRest,
    StartDialogue,
    StartDiscoveryDialogue,
    StoreBattleLoot,
    SubmitItemForRepair,
    SurrenderToAuthority,
    SynchronizePartyForActivity,
    TravelCamps,
    TravelToCaseSite,
    TravelToGeneratedCaseSite,
    TurnInQuest,
    UnsafeContractRetreatToSettlement,
    VisibleFirstAid,
    WaitForInvestigationWindowCamp,
    WaitForInvestigationWindowSettlement,
    WaitForRepairs,
    WithdrawPurchaseCoin,
}

impl ReducerOperation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::AbandonDefeatedQuest => "abandon_defeated_quest",
            Self::AbandonUnsafeActiveContract => "abandon_unsafe_active_contract",
            Self::AbandonUnsafeQuest => "abandon_unsafe_quest",
            Self::AcceptPartyJoinRequest => "accept_party_join_request",
            Self::AcceptQuest => "accept_quest",
            Self::AdministerPreparation => "administer_preparation",
            Self::AdvanceSimulationWorldTime => "advance_simulation_world_time",
            Self::AutoresolveGeneratedMission => "autoresolve_generated_mission",
            Self::AutoresolveMission => "autoresolve_mission",
            Self::ChooseDialogueTopic => "choose_dialogue_topic",
            Self::ClaimSimulationRun => "claim_simulation_run",
            Self::ConfigureSafeDepartureWindow => "configure_safe_departure_window",
            Self::ConfigureSimulationCharacter => "configure_simulation_character",
            Self::ContinueCampTravel => "continue_camp_travel",
            Self::ContributeJourneyCurrency => "contribute_journey_currency",
            Self::CreateNamedCharacterWithId => "create_named_character_with_id",
            Self::DefeatRetreatToSettlement => "defeat_retreat_to_settlement",
            Self::EnsureSettlementActivity => "ensure_settlement_activity",
            Self::EnsureSettlementActivityAfterEvacuation => {
                "ensure_settlement_activity_after_evacuation"
            }
            Self::EnsureSettlementActivityAfterIdleSiteReturn => {
                "ensure_settlement_activity_after_idle_site_return"
            }
            Self::EvacuateGeneratedCaseSite => "evacuate_generated_case_site",
            Self::ExpeditionHealthEvacuation => "expedition_health_evacuation",
            Self::ExpeditionRecoveryRest => "expedition_recovery_rest",
            Self::GeneratedCaseSitePlannedRecovery => "generated_case_site_planned_recovery",
            Self::GeneratedDefeatRetreatToSettlement => "generated_defeat_retreat_to_settlement",
            Self::GeneratedUnchangedDefeatRetreat => "generated_unchanged_defeat_retreat",
            Self::GeneratedUnsafeCombatRetreat => "generated_unsafe_combat_retreat",
            Self::IdleCaseSiteReturn => "idle_case_site_return",
            Self::IllnessRetreatToSettlement => "illness_retreat_to_settlement",
            Self::InstallActivitySchedule => "install_activity_schedule",
            Self::InteractAcceptContract => "interact_accept_contract",
            Self::InteractReportContract => "interact_report_contract",
            Self::LiquidatePartyInventory => "liquidate_party_inventory",
            Self::MedicalRecoveryRest => "medical_recovery_rest",
            Self::NaturalIllnessRecoveryRest => "natural_illness_recovery_rest",
            Self::NegotiateHostileWithdrawal => "negotiate_hostile_withdrawal",
            Self::PassiveNoActionableRest => "passive_no_actionable_rest",
            Self::PauseScheduleForTreatment => "pause_schedule_for_treatment",
            Self::PerformInvestigationAction => "perform_investigation_action",
            Self::PurchaseAmmunition => "purchase_ammunition",
            Self::PurchaseFirstAidMaterial => "purchase_first_aid_material",
            Self::PurchaseFromHerbalist => "purchase_from_herbalist",
            Self::PurchaseJourneyProvisions => "purchase_journey_provisions",
            Self::PurchasePartyTent => "purchase_party_tent",
            Self::PurchasePersonalStorefrontWithPartyStake => {
                "purchase_personal_storefront_with_party_stake"
            }
            Self::RegisterStrategicGateway => "register_strategic_gateway",
            Self::ReplaceItemAtPlacement => "replace_item_at_placement",
            Self::ReplenishQuestsAfterAbandon => "replenish_quests_after_abandon",
            Self::RequestGeneralPartyJoin => "request_general_party_join",
            Self::ResolveErrantryRoadChallenge => "resolve_errantry_road_challenge",
            Self::ResolveStrategicEncounter => "resolve_strategic_encounter",
            Self::RestoreScheduleAfterTreatment => "restore_schedule_after_treatment",
            Self::ResynchronizePartyAfterGeneratedPreflight => {
                "resynchronize_party_after_generated_preflight"
            }
            Self::RestAtCamp => "rest_at_camp",
            Self::RetrieveRepairedItem => "retrieve_repaired_item",
            Self::ReturnCompletedGeneratedCase => "return_completed_generated_case",
            Self::ReturnFromGeneratedCaseSite => "return_from_generated_case_site",
            Self::ReturnToSettlement => "return_to_settlement",
            Self::SeedSimulationDisease => "seed_simulation_disease",
            Self::SeedSimulationEquipmentDamage => "seed_simulation_equipment_damage",
            Self::SeedSimulationQuestFixture => "seed_simulation_quest_fixture",
            Self::SeedSimulationWorld => "seed_simulation_world",
            Self::SettlementActivityRest => "settlement_activity_rest",
            Self::SponsorPartyMemberInnRest => "sponsor_party_member_inn_rest",
            Self::StartDialogue => "start_dialogue",
            Self::StartDiscoveryDialogue => "start_discovery_dialogue",
            Self::StoreBattleLoot => "store_battle_loot",
            Self::SubmitItemForRepair => "submit_item_for_repair",
            Self::SurrenderToAuthority => "surrender_to_authority",
            Self::SynchronizePartyForActivity => "synchronize_party_for_activity",
            Self::TravelCamps => "travel_camps",
            Self::TravelToCaseSite => "travel_to_case_site",
            Self::TravelToGeneratedCaseSite => "travel_to_generated_case_site",
            Self::TurnInQuest => "turn_in_quest",
            Self::UnsafeContractRetreatToSettlement => "unsafe_contract_retreat_to_settlement",
            Self::VisibleFirstAid => "visible_first_aid",
            Self::WaitForInvestigationWindowCamp => "wait_for_investigation_window_camp",
            Self::WaitForInvestigationWindowSettlement => {
                "wait_for_investigation_window_settlement"
            }
            Self::WaitForRepairs => "wait_for_repairs",
            Self::WithdrawPurchaseCoin => "withdraw_purchase_coin",
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
                | Self::IdleCaseSiteReturn
                | Self::EvacuateGeneratedCaseSite
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ReducerCallFailureKind {
    Dispatch(String),
    Rejected {
        code: Option<ReducerErrorCode>,
        detail: String,
    },
    TimedOut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReducerCallFailure {
    operation: ReducerOperation,
    kind: ReducerCallFailureKind,
}

impl ReducerCallFailure {
    fn dispatch(operation: ReducerOperation, detail: impl Into<String>) -> Self {
        Self {
            operation,
            kind: ReducerCallFailureKind::Dispatch(detail.into()),
        }
    }

    fn rejected(operation: ReducerOperation, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        Self {
            operation,
            kind: ReducerCallFailureKind::Rejected {
                code: parse_reducer_error(&detail),
                detail,
            },
        }
    }

    const fn timed_out(operation: ReducerOperation) -> Self {
        Self {
            operation,
            kind: ReducerCallFailureKind::TimedOut,
        }
    }

    const fn code(&self) -> Option<ReducerErrorCode> {
        match &self.kind {
            ReducerCallFailureKind::Rejected { code, .. } => *code,
            ReducerCallFailureKind::Dispatch(_) | ReducerCallFailureKind::TimedOut => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CoreLoopError {
    Reducer(ReducerCallFailure),
    Operation {
        operation: ReducerOperation,
        detail: String,
    },
    Other(String),
}

impl CoreLoopError {
    fn reducer_dispatch(operation: ReducerOperation, detail: impl Into<String>) -> Self {
        Self::Reducer(ReducerCallFailure::dispatch(operation, detail))
    }

    fn reducer_rejected(operation: ReducerOperation, detail: impl Into<String>) -> Self {
        Self::Reducer(ReducerCallFailure::rejected(operation, detail))
    }

    const fn reducer_timed_out(operation: ReducerOperation) -> Self {
        Self::Reducer(ReducerCallFailure::timed_out(operation))
    }

    const fn reducer_failure(&self) -> Option<&ReducerCallFailure> {
        match self {
            Self::Reducer(failure) => Some(failure),
            Self::Operation { .. } | Self::Other(_) => None,
        }
    }

    fn operation(operation: ReducerOperation, detail: impl Into<String>) -> Self {
        Self::Operation {
            operation,
            detail: detail.into(),
        }
    }

    const fn operation_kind(&self) -> Option<ReducerOperation> {
        match self {
            Self::Reducer(failure) => Some(failure.operation),
            Self::Operation { operation, .. } => Some(*operation),
            Self::Other(_) => None,
        }
    }

    const fn reducer_code(&self) -> Option<ReducerErrorCode> {
        match self.reducer_failure() {
            Some(failure) => failure.code(),
            None => None,
        }
    }
}

impl std::fmt::Display for CoreLoopError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reducer(failure) => match &failure.kind {
                ReducerCallFailureKind::Dispatch(detail) => write!(
                    formatter,
                    "could not send {}: {detail}",
                    failure.operation.as_str()
                ),
                ReducerCallFailureKind::Rejected { detail, .. } => {
                    write!(formatter, "{} failed: {detail}", failure.operation.as_str())
                }
                ReducerCallFailureKind::TimedOut => write!(
                    formatter,
                    "{} timed out after {ACTION_TIMEOUT:?}",
                    failure.operation.as_str()
                ),
            },
            Self::Operation { operation, detail } => {
                write!(formatter, "{} failed: {detail}", operation.as_str())
            }
            Self::Other(detail) => formatter.write_str(detail),
        }
    }
}

impl std::error::Error for CoreLoopError {}
