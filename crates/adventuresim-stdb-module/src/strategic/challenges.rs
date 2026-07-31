pub const ERRANTRY_ISSUER_ORGANIZATION_ID: &str = "order_saint_george";
pub const ERRANTRY_FINALE_THREAT_ID: &str = "armed_retainer";
pub const FEY_COUNTERMEASURE_REDUCTION_BPS: u32 = 2_500;
pub const FEY_COUNTERMEASURE_SCALE_FLOOR_BPS: u32 = 5_000;
pub const FEY_COUNTERMEASURE_MULTIPLIER_BPS: u32 = 7_500;
pub const DISPATCH_COUNTERMEASURE_REDUCTION_BPS: u32 = 1_500;
pub const DISPATCH_COUNTERMEASURE_MULTIPLIER_BPS: u32 = 8_500;
pub const COURIER_REST_DELAY_MINUTES: u64 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ChallengePresenterCatalogId {
    LadyBeneathThornV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ErrantryFinaleDefenseKind {
    UnnaturalProwess,
    Reinforcements,
    PoisonedArms,
    ConcealedTrap,
    Glamour,
    SupernaturalArmor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ErrantryCountermeasureKind {
    FavorOfTheThornLady,
    CapturedDispatch,
    Antidote,
    TrapWarning,
    ColdIronCharm,
    BlessedWeapon,
    RescuedAlly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum RoadChallengeCatalogId {
    WoundedOrderCourierV1,
}

/// Private case-level authority for an Order-issued errantry.
#[derive(Clone, Debug)]
#[table(accessor = errantry_authority)]
pub struct ErrantryAuthority {
    #[primary_key]
    pub case_id: String,
    pub contract_id: String,
    pub issuer_organization_id: String,
    pub issuer_npc_id: String,
    pub issuer_settlement_id: String,
    pub issuer_location_id: String,
    pub finale_case_site_id: String,
    pub finale_hostile_group_id: String,
    pub preliminary_challenge_ids: Vec<String>,
    pub finale_defenses_json: String,
}

/// Durable, typed and source-idempotent boon awarded by the preliminary trial.
#[derive(Clone, Debug)]
#[table(accessor = errantry_countermeasure)]
pub struct ErrantryCountermeasure {
    #[primary_key]
    pub source_challenge_id: String,
    #[index(btree)]
    pub party_id: String,
    pub case_id: String,
    pub case_site_id: String,
    pub hostile_group_id: String,
    pub item_id: String,
    pub kind: ErrantryCountermeasureKind,
    pub counters_defense: ErrantryFinaleDefenseKind,
    pub combat_scale_reduction_bps: u32,
    pub combat_capability_multiplier_bps: u32,
    pub awarded_at_minute: u64,
}

/// A chat-native, non-puzzle interruption that becomes available only after
/// resting at its bound road camp. Ignoring it requires no mutation; choosing
/// either response closes it without affecting the finale objective.
#[derive(Clone, Debug)]
#[table(accessor = road_challenge_authority)]
pub struct RoadChallengeAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub party_id: String,
    pub case_id: String,
    pub finale_case_site_id: String,
    pub finale_hostile_group_id: String,
    pub journey_departure_minute: u64,
    pub camp_movement_minute: u64,
    pub available_at_elapsed_minute: u64,
    pub catalog_id: RoadChallengeCatalogId,
    pub revision: u32,
    pub open: bool,
    pub resolved_choice: Option<String>,
}

#[derive(Clone, Debug)]
#[table(accessor = road_challenge_resolution_receipt)]
pub struct RoadChallengeResolutionReceipt {
    #[primary_key]
    pub id: String,
    pub challenge_id: String,
    pub party_id: String,
    pub character_id: u64,
    pub action_id: String,
    pub choice: String,
    pub resolved_at_minute: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendRoadChallenge {
    pub id: String,
    pub case_id: String,
    pub owner_character_id: u64,
    pub catalog_id: RoadChallengeCatalogId,
    pub revision: u32,
    pub open: bool,
    pub active: bool,
    pub resolved_choice: Option<String>,
    pub boon_item_id: Option<String>,
    pub counters_defense: Option<ErrantryFinaleDefenseKind>,
}

#[derive(Clone, Debug)]
#[table(accessor = order_errantry_acceptance_receipt)]
pub struct OrderErrantryAcceptanceReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub dialogue_session_id: String,
    pub action_id: String,
    pub character_id: u64,
    pub case_id: String,
    pub contract_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ErrantryLaunch {
    NormalTravel,
    DirectDemoCamp,
}

struct MaterializedErrantry {
    case_id: String,
    contract_id: String,
}

/// Private deterministic challenge authority. `puzzle_json` contains the seed
/// and canonical ordering and must never appear in a public table or view.
#[derive(Clone, Debug)]
#[table(accessor = challenge_authority)]
pub struct ChallengeAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub case_id: String,
    #[index(btree)]
    pub party_id: String,
    pub finale_case_site_id: String,
    pub finale_hostile_group_id: String,
    pub journey_departure_minute: u64,
    pub camp_movement_minute: u64,
    pub camp_elapsed_minute: u64,
    pub errantry_frame_json: String,
    pub puzzle_json: String,
    pub presenter_catalog_id: ChallengePresenterCatalogId,
    pub revision: u32,
    pub open: bool,
    pub solved_at_minute: Option<u64>,
}

/// Durable source/revision receipt. Wrong attempts are retained and retryable;
/// a receipt is immutable and an exact reducer retry is idempotent.
#[derive(Clone, Debug)]
#[table(accessor = challenge_attempt_receipt)]
pub struct ChallengeAttemptReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub challenge_id: String,
    pub case_id: String,
    pub party_id: String,
    pub character_id: u64,
    pub submitted_revision: u32,
    pub ordering_json: String,
    pub correct: bool,
    pub resulting_revision: u32,
    pub attempted_at_minute: u64,
}

/// Trusted-gateway, observer-bound projection. Puzzle seed and canonical
/// assignment are absent by construction.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendChallenge {
    pub id: String,
    pub case_id: String,
    pub party_id: String,
    pub owner_character_id: u64,
    pub finale_case_site_id: String,
    pub puzzle_projection_json: String,
    pub presenter_catalog_id: ChallengePresenterCatalogId,
    pub revision: u32,
    pub open: bool,
    pub solved: bool,
    pub active: bool,
    pub last_attempt_correct: Option<bool>,
    pub boon_item_id: Option<String>,
    pub boon_combat_scale_reduction_bps: Option<u32>,
}

#[view(accessor = backend_challenges, public)]
pub fn backend_challenges(ctx: &ViewContext) -> Vec<BackendChallenge> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .challenge_authority()
        .gateway_bucket()
        .filter(0u8)
        .filter_map(|challenge| {
            let party = ctx.db.party_authority().id().find(&challenge.party_id)?;
            let puzzle: adventuresim_core::errantry::OrderedSigilPuzzle =
                serde_json::from_str(&challenge.puzzle_json).ok()?;
            puzzle.validate().ok()?;
            let projection = serde_json::to_string(&puzzle.projection()).ok()?;
            let accepted = party
                .active_contract_id
                .as_ref()
                .and_then(|id| ctx.db.contract_authority().id().find(id))
                .is_some_and(|contract| {
                    contract.case_id == challenge.case_id
                        && contract.status == ContractStatus::Accepted
                        && contract.accepted_by.as_deref() == Some(&challenge.party_id)
                });
            let active = accepted && party_at_bound_trial_camp_view(ctx, &party, &challenge);
            let last_attempt_correct = ctx
                .db
                .challenge_attempt_receipt()
                .challenge_id()
                .filter(&challenge.id)
                .max_by_key(|receipt| receipt.submitted_revision)
                .map(|receipt| receipt.correct);
            Some(BackendChallenge {
                id: challenge.id.clone(),
                case_id: challenge.case_id,
                party_id: challenge.party_id,
                owner_character_id: party.leader_id,
                finale_case_site_id: challenge.finale_case_site_id,
                puzzle_projection_json: projection,
                presenter_catalog_id: challenge.presenter_catalog_id,
                revision: challenge.revision,
                open: challenge.open,
                solved: challenge.solved_at_minute.is_some(),
                active,
                last_attempt_correct,
                boon_item_id: ctx
                    .db
                    .errantry_countermeasure()
                    .source_challenge_id()
                    .find(&challenge.id)
                    .map(|boon| boon.item_id),
                boon_combat_scale_reduction_bps: ctx
                    .db
                    .errantry_countermeasure()
                    .source_challenge_id()
                    .find(&challenge.id)
                    .map(|boon| boon.combat_scale_reduction_bps),
            })
        })
        .collect()
}

#[view(accessor = backend_road_challenges, public)]
pub fn backend_road_challenges(ctx: &ViewContext) -> Vec<BackendRoadChallenge> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .road_challenge_authority()
        .gateway_bucket()
        .filter(0u8)
        .filter_map(|challenge| {
            let party = ctx.db.party_authority().id().find(&challenge.party_id)?;
            let accepted = party
                .active_contract_id
                .as_ref()
                .and_then(|id| ctx.db.contract_authority().id().find(id))
                .is_some_and(|contract| {
                    contract.case_id == challenge.case_id
                        && contract.status == ContractStatus::Accepted
                        && contract.accepted_by.as_deref() == Some(&challenge.party_id)
                });
            let active = accepted && party_at_bound_road_challenge_view(ctx, &party, &challenge);
            let boon = ctx
                .db
                .errantry_countermeasure()
                .source_challenge_id()
                .find(&challenge.id);
            Some(BackendRoadChallenge {
                id: challenge.id,
                case_id: challenge.case_id,
                owner_character_id: party.leader_id,
                catalog_id: challenge.catalog_id,
                revision: challenge.revision,
                open: challenge.open,
                active,
                resolved_choice: challenge.resolved_choice,
                boon_item_id: boon.as_ref().map(|item| item.item_id.clone()),
                counters_defense: boon.map(|item| item.counters_defense),
            })
        })
        .collect()
}

fn journey_destination_matches(endpoint: &JourneyEndpoint, case_site_id: &str) -> bool {
    matches!(
        endpoint,
        JourneyEndpoint::CaseSite(site) if site.id.value == case_site_id
    )
}

fn journey_at_bound_trial_camp(
    journey: &PartyJourney,
    challenge: &ChallengeAuthority,
) -> bool {
    adventuresim_core::errantry::journey_camp_identity_matches(
        journey.departure_minute,
        journey.completed_minutes,
        &journey.camp_stop_minutes,
        challenge.journey_departure_minute,
        challenge.camp_movement_minute,
    )
        && journey_destination_matches(&journey.destination, &challenge.finale_case_site_id)
}

fn journey_at_bound_road_challenge(
    journey: &PartyJourney,
    challenge: &RoadChallengeAuthority,
) -> bool {
    adventuresim_core::errantry::rested_road_trial_camp_matches(
        journey.departure_minute,
        journey.completed_minutes,
        journey.completed_elapsed_minutes,
        &journey.camp_stop_minutes,
        challenge.journey_departure_minute,
        challenge.camp_movement_minute,
        challenge.available_at_elapsed_minute,
    ) && journey_destination_matches(&journey.destination, &challenge.finale_case_site_id)
}

fn party_at_bound_trial_camp_view(
    ctx: &ViewContext,
    party: &Party,
    challenge: &ChallengeAuthority,
) -> bool {
    party.current_settlement_id.is_none()
        && party.current_case_site_id.is_none()
        && party.camp_destination.is_some()
        && ctx
            .db
            .party_journey_authority()
            .party_id()
            .find(&party.id)
            .is_some_and(|journey| journey_at_bound_trial_camp(&journey, challenge))
        && !ctx
            .db
            .strategic_encounter()
            .party_id()
            .find(&party.id)
            .is_some_and(|encounter| encounter.status == "awaiting_choice")
}

fn party_at_bound_trial_camp(
    ctx: &ReducerContext,
    party: &Party,
    challenge: &ChallengeAuthority,
) -> bool {
    party.current_settlement_id.is_none()
        && party.current_case_site_id.is_none()
        && party.camp_destination.is_some()
        && ctx
            .db
            .party_journey_authority()
            .party_id()
            .find(&party.id)
            .is_some_and(|journey| journey_at_bound_trial_camp(&journey, challenge))
        && !ctx
            .db
            .strategic_encounter()
            .party_id()
            .find(&party.id)
            .is_some_and(|encounter| encounter.status == "awaiting_choice")
}

fn party_at_bound_road_challenge_view(
    ctx: &ViewContext,
    party: &Party,
    challenge: &RoadChallengeAuthority,
) -> bool {
    party.current_settlement_id.is_none()
        && party.current_case_site_id.is_none()
        && party.camp_destination.is_some()
        && ctx
            .db
            .party_journey_authority()
            .party_id()
            .find(&party.id)
            .is_some_and(|journey| journey_at_bound_road_challenge(&journey, challenge))
        && !ctx
            .db
            .strategic_encounter()
            .party_id()
            .find(&party.id)
            .is_some_and(|encounter| encounter.status == "awaiting_choice")
}

fn party_at_bound_road_challenge(
    ctx: &ReducerContext,
    party: &Party,
    challenge: &RoadChallengeAuthority,
) -> bool {
    party.current_settlement_id.is_none()
        && party.current_case_site_id.is_none()
        && party.camp_destination.is_some()
        && ctx
            .db
            .party_journey_authority()
            .party_id()
            .find(&party.id)
            .is_some_and(|journey| journey_at_bound_road_challenge(&journey, challenge))
        && !ctx
            .db
            .strategic_encounter()
            .party_id()
            .find(&party.id)
            .is_some_and(|encounter| encounter.status == "awaiting_choice")
}

/// Bind optional preliminary trials to the first real camp reached on their
/// accepted finale journey. Issuance never predicts camp coordinates.
pub(crate) fn bind_errantry_trials_to_current_camp(
    ctx: &ReducerContext,
    party_id: &str,
) -> Result<(), String> {
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    if party.camp_destination.is_none() {
        return Ok(());
    }
    let journey = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id.to_string())
        .ok_or("Camp has no journey authority")?;
    let JourneyEndpoint::CaseSite(destination) = &journey.destination else {
        return Ok(());
    };
    let Some(contract_id) = party.active_contract_id.as_deref() else {
        return Ok(());
    };
    let Some(contract) = ctx
        .db
        .contract_authority()
        .id()
        .find(contract_id.to_string())
    else {
        return Ok(());
    };
    if contract.status != ContractStatus::Accepted
        || contract.accepted_by.as_deref() != Some(party_id)
    {
        return Ok(());
    }
    let mut candidates = ctx
        .db
        .challenge_authority()
        .party_id()
        .filter(&party_id.to_string())
        .filter(|challenge| {
            challenge.open
                && challenge.solved_at_minute.is_none()
                && challenge.case_id == contract.case_id
                && challenge.finale_case_site_id == destination.id.value
                && challenge.journey_departure_minute == 0
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    let site_id = format!(
        "journey-camp:{}:{}",
        journey.departure_minute, journey.completed_minutes
    );
    if let Some(mut challenge) = candidates.into_iter().next() {
        let mut frame: adventuresim_core::errantry::ErrantryFrame =
            serde_json::from_str(&challenge.errantry_frame_json)
                .map_err(|_| "Errantry frame authority is invalid")?;
        if !frame
            .trials
            .iter()
            .any(|trial| trial.challenge_id.as_deref() == Some(&challenge.id))
        {
            return Err("Challenge is not bound to its errantry frame".into());
        }
        for trial in &mut frame.trials {
            if trial.site_id == "journey-camp:unbound" {
                trial.site_id = site_id.clone();
            }
        }
        challenge.errantry_frame_json =
            serde_json::to_string(&frame).map_err(|_| "Could not encode errantry frame")?;
        challenge.journey_departure_minute = journey.departure_minute;
        challenge.camp_movement_minute = journey.completed_minutes;
        challenge.camp_elapsed_minute = journey.completed_elapsed_minutes;
        ctx.db.challenge_authority().id().update(challenge);
    }
    let mut road_candidates = ctx
        .db
        .road_challenge_authority()
        .party_id()
        .filter(&party_id.to_string())
        .filter(|challenge| {
            challenge.open
                && challenge.case_id == contract.case_id
                && challenge.finale_case_site_id == destination.id.value
                && challenge.journey_departure_minute == 0
        })
        .collect::<Vec<_>>();
    road_candidates.sort_by(|left, right| left.id.cmp(&right.id));
    for mut challenge in road_candidates {
        challenge.journey_departure_minute = journey.departure_minute;
        challenge.camp_movement_minute = journey.completed_minutes;
        challenge.available_at_elapsed_minute = journey
            .completed_elapsed_minutes
            .saturating_add(COURIER_REST_DELAY_MINUTES);
        ctx.db.road_challenge_authority().id().update(challenge);
    }
    Ok(())
}

fn award_errantry_countermeasure(
    ctx: &ReducerContext,
    source_challenge_id: &str,
    party_id: &str,
    case_id: &str,
    case_site_id: &str,
    hostile_group_id: &str,
    item_id: &str,
    kind: ErrantryCountermeasureKind,
    counters_defense: ErrantryFinaleDefenseKind,
    combat_scale_reduction_bps: u32,
    combat_capability_multiplier_bps: u32,
    now: u64,
) -> Result<(), String> {
    if ctx
        .db
        .errantry_countermeasure()
        .source_challenge_id()
        .find(&source_challenge_id.to_string())
        .is_some()
    {
        return Ok(());
    }
    ctx.db
        .errantry_countermeasure()
        .insert(ErrantryCountermeasure {
            source_challenge_id: source_challenge_id.into(),
            party_id: party_id.into(),
            case_id: case_id.into(),
            case_site_id: case_site_id.into(),
            hostile_group_id: hostile_group_id.into(),
            item_id: item_id.into(),
            kind,
            counters_defense,
            combat_scale_reduction_bps,
            combat_capability_multiplier_bps,
            awarded_at_minute: now,
        });
    ctx.db.party_inventory_item().insert(PartyInventoryItem {
        id: 0,
        party_id: party_id.into(),
        item_id: item_id.into(),
        quantity: 1,
    });
    Ok(())
}

pub(crate) fn errantry_mission_scale_snapshot(
    ctx: &ReducerContext,
    party_id: &str,
    case_id: &str,
    case_site_id: &str,
    hostile_group_id: &str,
    base_scale_bps: u32,
) -> (u32, u32, Option<String>, String) {
    let defenses = ctx
        .db
        .errantry_authority()
        .case_id()
        .find(&case_id.to_string())
        .and_then(|errantry| {
            serde_json::from_str::<Vec<adventuresim_core::errantry::FinaleDefenseKind>>(
                &errantry.finale_defenses_json,
            )
            .ok()
        })
        .unwrap_or_default();
    let countermeasures = ctx
        .db
        .errantry_countermeasure()
        .party_id()
        .filter(&party_id.to_string())
        .filter(|boon| {
            boon.case_id == case_id
                && boon.case_site_id == case_site_id
                && boon.hostile_group_id == hostile_group_id
        })
        .map(|boon| adventuresim_core::errantry::MaterialCountermeasure {
            kind: core_countermeasure_kind(boon.kind),
            source_challenge_id: boon.source_challenge_id,
            item_id: boon.item_id,
            counters: core_defense_kind(boon.counters_defense),
            enemy_scale_reduction_bps: boon.combat_scale_reduction_bps,
            enemy_capability_multiplier_bps: boon.combat_capability_multiplier_bps,
        })
        .collect::<Vec<_>>();
    let resolution = adventuresim_core::errantry::resolve_finale_approach(
        &defenses,
        &countermeasures,
    );
    let primary_source = resolution
        .applied
        .first()
        .map(|item| item.source_challenge_id.clone());
    let enemy_scale = base_scale_bps
        .saturating_sub(resolution.enemy_scale_reduction_bps)
        .max(FEY_COUNTERMEASURE_SCALE_FLOOR_BPS);
    let snapshot_json = serde_json::to_string(&resolution)
        .expect("finale approach resolution is serializable");
    (
        enemy_scale,
        resolution.enemy_capability_multiplier_bps,
        primary_source,
        snapshot_json,
    )
}

fn core_defense_kind(
    kind: ErrantryFinaleDefenseKind,
) -> adventuresim_core::errantry::FinaleDefenseKind {
    use adventuresim_core::errantry::FinaleDefenseKind as Core;
    match kind {
        ErrantryFinaleDefenseKind::UnnaturalProwess => Core::UnnaturalProwess,
        ErrantryFinaleDefenseKind::Reinforcements => Core::Reinforcements,
        ErrantryFinaleDefenseKind::PoisonedArms => Core::PoisonedArms,
        ErrantryFinaleDefenseKind::ConcealedTrap => Core::ConcealedTrap,
        ErrantryFinaleDefenseKind::Glamour => Core::Glamour,
        ErrantryFinaleDefenseKind::SupernaturalArmor => Core::SupernaturalArmor,
    }
}

fn core_countermeasure_kind(
    kind: ErrantryCountermeasureKind,
) -> adventuresim_core::errantry::CountermeasureKind {
    use adventuresim_core::errantry::CountermeasureKind as Core;
    match kind {
        ErrantryCountermeasureKind::FavorOfTheThornLady => Core::FavorOfTheThornLady,
        ErrantryCountermeasureKind::CapturedDispatch => Core::CapturedDispatch,
        ErrantryCountermeasureKind::Antidote => Core::Antidote,
        ErrantryCountermeasureKind::TrapWarning => Core::TrapWarning,
        ErrantryCountermeasureKind::ColdIronCharm => Core::ColdIronCharm,
        ErrantryCountermeasureKind::BlessedWeapon => Core::BlessedWeapon,
        ErrantryCountermeasureKind::RescuedAlly => Core::RescuedAlly,
    }
}

fn parse_ordered_sigils(
    ordering_json: &str,
) -> Result<[adventuresim_core::errantry::Sigil; adventuresim_core::errantry::ORDERED_SIGIL_COUNT], String>
{
    let ordering: Vec<adventuresim_core::errantry::Sigil> =
        serde_json::from_str(ordering_json).map_err(|_| "Malformed sigil ordering")?;
    ordering
        .try_into()
        .map_err(|_| "Submit exactly five sigils".into())
}

fn validate_challenge_retry(
    existing: &ChallengeAttemptReceipt,
    case_id: &str,
    challenge_id: &str,
    party_id: &str,
    character_id: u64,
    normalized_ordering: &str,
) -> Result<(), String> {
    if existing.case_id == case_id
        && existing.challenge_id == challenge_id
        && existing.party_id == party_id
        && existing.character_id == character_id
        && existing.ordering_json == normalized_ordering
    {
        Ok(())
    } else {
        Err("Conflicting retry for challenge revision".into())
    }
}

/// Submit one complete ordering. Every authority coordinate is derived again:
/// selected character, party leadership, active accepted contract, case,
/// challenge, exact journey camp coordinates, pending encounter state, open
/// state, and expected revision.
#[reducer]
pub fn submit_ordered_sigil_challenge(
    ctx: &ReducerContext,
    character_id: u64,
    case_id: String,
    challenge_id: String,
    expected_revision: u32,
    ordering_json: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character.party_id.ok_or("Must be in a party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can answer this challenge".into());
    }
    let mut challenge = ctx
        .db
        .challenge_authority()
        .id()
        .find(&challenge_id)
        .ok_or("Challenge not found")?;
    if challenge.case_id != case_id || challenge.party_id != party_id {
        return Err("Challenge authority does not match this party and case".into());
    }
    let ordering = parse_ordered_sigils(&ordering_json)?;
    let normalized_ordering =
        serde_json::to_string(&ordering).map_err(|_| "Could not encode sigil ordering")?;
    let receipt_id = format!(
        "challenge-attempt:{}:{}:{}",
        challenge.id, party_id, expected_revision
    );
    // Lost-response retries remain idempotent after a successful attempt has
    // closed the challenge, resolved the case, paid the demo contract, and
    // cleared the party's active contract. New attempts continue below and
    // must satisfy every live authority check.
    if let Some(existing) = ctx.db.challenge_attempt_receipt().id().find(&receipt_id) {
        return validate_challenge_retry(
            &existing,
            &case_id,
            &challenge_id,
            &party_id,
            character_id,
            &normalized_ordering,
        );
    }
    let active_contract_id = party
        .active_contract_id
        .as_ref()
        .ok_or("Party has no active quest")?;
    let contract = ctx
        .db
        .contract_authority()
        .id()
        .find(active_contract_id)
        .ok_or("Active quest not found")?;
    if contract.case_id != case_id
        || contract.accepted_by.as_deref() != Some(&party_id)
        || contract.status != ContractStatus::Accepted
    {
        return Err("Challenge does not belong to the active accepted quest".into());
    }
    let errantry = ctx
        .db
        .errantry_authority()
        .case_id()
        .find(&case_id)
        .ok_or("Case has no errantry issuance authority")?;
    if errantry.contract_id != contract.id
        || !errantry.preliminary_challenge_ids.contains(&challenge.id)
        || errantry.finale_case_site_id != challenge.finale_case_site_id
        || errantry.finale_hostile_group_id != challenge.finale_hostile_group_id
        || errantry.issuer_organization_id != ERRANTRY_ISSUER_ORGANIZATION_ID
    {
        return Err("Errantry issuance does not match the accepted quest".into());
    }
    let issuer = ctx
        .db
        .settlement_npc()
        .id()
        .find(&errantry.issuer_npc_id)
        .ok_or("Errantry issuer is no longer a live representative")?;
    let issuer_organization = adventuresim_core::organization::organization(
        &errantry.issuer_organization_id,
    )
    .filter(|organization| organization.errantry_issuance)
    .ok_or("Issuer organization lacks errantry authority")?;
    if issuer_organization.id != ERRANTRY_ISSUER_ORGANIZATION_ID
        || exact_organization_representative(
            ctx,
            &issuer,
            &errantry.issuer_settlement_id,
            &errantry.issuer_location_id,
        )
        .as_deref()
            != Some(ERRANTRY_ISSUER_ORGANIZATION_ID)
    {
        return Err("Errantry issuer is not the exact live Order chapter representative".into());
    }
    let case = ctx
        .db
        .case_authority()
        .id()
        .find(&case_id)
        .ok_or("Case not found")?;
    if case.resolution_status != CaseResolutionStatus::Open {
        return Err("Case is no longer open".into());
    }
    if !party_at_bound_trial_camp(ctx, &party, &challenge) {
        return Err(
            "The fey trial is available only at its bound camp, after pending encounters".into(),
        );
    }
    if !challenge.open || challenge.solved_at_minute.is_some() {
        return Err("Challenge is closed".into());
    }
    if challenge.revision != expected_revision {
        return Err("Challenge revision is stale".into());
    }
    let puzzle: adventuresim_core::errantry::OrderedSigilPuzzle =
        serde_json::from_str(&challenge.puzzle_json)
            .map_err(|_| "Challenge authority is invalid")?;
    let frame: adventuresim_core::errantry::ErrantryFrame =
        serde_json::from_str(&challenge.errantry_frame_json)
            .map_err(|_| "Errantry frame authority is invalid")?;
    let bound_trial = frame
        .trials
        .iter()
        .find(|trial| trial.challenge_id.as_deref() == Some(&challenge.id))
        .ok_or("Challenge is not bound to its errantry frame")?;
    if bound_trial.site_id
        != format!(
            "journey-camp:{}:{}",
            challenge.journey_departure_minute, challenge.camp_movement_minute
        )
        || bound_trial.kind != adventuresim_core::errantry::TrialKind::Puzzle
    {
        return Err("Errantry trial binding does not match challenge authority".into());
    }
    puzzle
        .validate()
        .map_err(|_| "Challenge authority failed deterministic replay")?;
    let replay = adventuresim_core::errantry::OrderedSigilPuzzle::generate_versioned(
        puzzle.rules_version,
        puzzle.seed,
    )
    .map_err(str::to_string)?;
    if replay != puzzle {
        return Err("Challenge deterministic replay does not match authority".into());
    }
    let submission = adventuresim_core::errantry::OrderedSigilSubmission {
        expected_revision,
        ordering,
    };
    let correct = puzzle
        .check(&submission)
        .map_err(|_| "Sigil ordering must contain each sigil exactly once")?;
    let now = crate::time::refresh_clock(ctx)?;
    let resulting_revision = expected_revision.saturating_add(1);
    ctx.db
        .challenge_attempt_receipt()
        .insert(ChallengeAttemptReceipt {
            id: receipt_id,
            challenge_id: challenge.id.clone(),
            case_id: case_id.clone(),
            party_id: party_id.clone(),
            character_id,
            submitted_revision: expected_revision,
            ordering_json: normalized_ordering,
            correct,
            resulting_revision,
            attempted_at_minute: now,
        });
    challenge.revision = resulting_revision;
    if correct {
        challenge.open = false;
        challenge.solved_at_minute = Some(now);
    }
    ctx.db.challenge_authority().id().update(challenge.clone());
    if correct {
        award_errantry_countermeasure(
            ctx,
            &challenge.id,
            &challenge.party_id,
            &challenge.case_id,
            &challenge.finale_case_site_id,
            &challenge.finale_hostile_group_id,
            adventuresim_core::item_references::FEY_COUNTERMEASURE_ITEM_ID,
            ErrantryCountermeasureKind::FavorOfTheThornLady,
            ErrantryFinaleDefenseKind::UnnaturalProwess,
            FEY_COUNTERMEASURE_REDUCTION_BPS,
            FEY_COUNTERMEASURE_MULTIPLIER_BPS,
            now,
        )?;
        let solved_challenge_id = challenge.id.clone();
        ingest_case_outcome_fact(
            ctx,
            &format!("challenge-solved:{solved_challenge_id}"),
            &case_id,
            &party_id,
            adventuresim_core::case::OutcomeFactKind::ChallengeSolved {
                challenge_id: solved_challenge_id,
            },
        )?;
    }
    Ok(())
}

/// Resolve the mortal roadside courtesy trial. It becomes available only
/// after resting at its exact bound camp. Simply continuing travel ignores it
/// without blocking the finale objective.
#[reducer]
pub fn resolve_errantry_road_challenge(
    ctx: &ReducerContext,
    character_id: u64,
    challenge_id: String,
    expected_revision: u32,
    choice: String,
    action_id: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    if action_id.is_empty() || action_id.len() > 160 {
        return Err("Road challenge action ID is invalid".into());
    }
    if choice != "aid" && choice != "leave" {
        return Err("Road challenge choice is invalid".into());
    }
    let receipt_id = format!("road-challenge:{challenge_id}:{action_id}");
    if let Some(existing) = ctx
        .db
        .road_challenge_resolution_receipt()
        .id()
        .find(&receipt_id)
    {
        return if existing.challenge_id == challenge_id
            && existing.character_id == character_id
            && existing.choice == choice
        {
            Ok(())
        } else {
            Err("Conflicting road challenge retry".into())
        };
    }
    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character.party_id.ok_or("Must be in a party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can answer this road challenge".into());
    }
    let mut challenge = ctx
        .db
        .road_challenge_authority()
        .id()
        .find(&challenge_id)
        .ok_or("Road challenge not found")?;
    if challenge.party_id != party_id {
        return Err("Road challenge does not belong to this party".into());
    }
    let contract_id = party
        .active_contract_id
        .as_deref()
        .ok_or("No active errantry contract")?;
    let contract = ctx
        .db
        .contract_authority()
        .id()
        .find(contract_id.to_string())
        .ok_or("Active errantry contract not found")?;
    if contract.status != ContractStatus::Accepted
        || contract.accepted_by.as_deref() != Some(&party_id)
        || contract.case_id != challenge.case_id
    {
        return Err("Road challenge is not bound to the accepted errantry".into());
    }
    let errantry = ctx
        .db
        .errantry_authority()
        .case_id()
        .find(&challenge.case_id)
        .ok_or("Errantry authority not found")?;
    if !errantry.preliminary_challenge_ids.contains(&challenge.id)
        || errantry.finale_case_site_id != challenge.finale_case_site_id
        || errantry.finale_hostile_group_id != challenge.finale_hostile_group_id
    {
        return Err("Road challenge authority does not match errantry".into());
    }
    if !party_at_bound_road_challenge(ctx, &party, &challenge) {
        return Err("Road challenge is not active at this camp".into());
    }
    if !challenge.open || challenge.resolved_choice.is_some() {
        return Err("Road challenge is already closed".into());
    }
    if challenge.revision != expected_revision {
        return Err("Road challenge is stale".into());
    }
    let now = crate::time::refresh_clock(ctx)?;
    if choice == "aid" {
        award_errantry_countermeasure(
            ctx,
            &challenge.id,
            &challenge.party_id,
            &challenge.case_id,
            &challenge.finale_case_site_id,
            &challenge.finale_hostile_group_id,
            adventuresim_core::item_references::CAPTURED_DISPATCH_ITEM_ID,
            ErrantryCountermeasureKind::CapturedDispatch,
            ErrantryFinaleDefenseKind::Reinforcements,
            DISPATCH_COUNTERMEASURE_REDUCTION_BPS,
            DISPATCH_COUNTERMEASURE_MULTIPLIER_BPS,
            now,
        )?;
    }
    challenge.open = false;
    challenge.resolved_choice = Some(choice.clone());
    challenge.revision = challenge.revision.saturating_add(1);
    ctx.db.road_challenge_authority().id().update(challenge);
    ctx.db
        .road_challenge_resolution_receipt()
        .insert(RoadChallengeResolutionReceipt {
            id: receipt_id,
            challenge_id,
            party_id,
            character_id,
            action_id,
            choice,
            resolved_at_minute: now,
        });
    Ok(())
}

fn puzzle_demo_enabled() -> bool {
    COMPILED_DEV_BOOTSTRAP_TOKEN.is_some_and(|token| {
        adventuresim_core::simulation_security::simulation_bootstrap_authorized(
            COMPILED_DEV_BOOTSTRAP_TOKEN,
            token,
        )
    })
}

fn active_puzzle_demo(
    ctx: &ReducerContext,
    party_id: &str,
    character_id: u64,
) -> Option<(ChallengeAuthority, Contract)> {
    let party_key = party_id.to_string();
    let demo_prefix = format!("challenge:ordered-sigils:demo:{character_id}:");
    let mut challenges = ctx
        .db
        .challenge_authority()
        .party_id()
        .filter(&party_key)
        .filter(|challenge| {
            challenge.open
                && challenge.solved_at_minute.is_none()
                && challenge.id.starts_with(&demo_prefix)
        })
        .collect::<Vec<_>>();
    challenges.sort_by(|left, right| left.id.cmp(&right.id));
    challenges.into_iter().find_map(|challenge| {
        let contract = ctx
            .db
            .contract_authority()
            .case_id()
            .filter(&challenge.case_id)
            .find(|contract| {
                contract.service_id == "errantry:order_saint_george"
                    && contract.status == ContractStatus::Accepted
                    && contract.accepted_by.as_deref() == Some(party_id)
            })?;
        Some((challenge, contract))
    })
}

fn puzzle_demo_suffix(character_id: u64, ordinal: u64) -> String {
    format!("demo:{character_id}:{ordinal}")
}

fn errantry_suffix(character_id: u64, ordinal: u64, launch: ErrantryLaunch) -> String {
    match launch {
        ErrantryLaunch::DirectDemoCamp => puzzle_demo_suffix(character_id, ordinal),
        ErrantryLaunch::NormalTravel => format!("order:{character_id}:{ordinal}"),
    }
}

fn order_errantry_issuer(
    ctx: &ReducerContext,
) -> Option<(
    crate::settlement_population::SettlementNpc,
    String,
    String,
)> {
    let order = adventuresim_core::organization::organization(ERRANTRY_ISSUER_ORGANIZATION_ID)
        .filter(|organization| organization.errantry_issuance)?;
    order.chapters.iter().find_map(|chapter| {
        let settlement = ctx.db.settlement().id().find(&chapter.settlement_id)?;
        let location_id = adventuresim_core::organization::chapter_effective_location_id(
            order,
            chapter,
            &settlement.economy,
        )
        .to_string();
        let expected_id = adventuresim_core::organization::organization_representative_id(
            &chapter.settlement_id,
            ERRANTRY_ISSUER_ORGANIZATION_ID,
        );
        let npc = ctx.db.settlement_npc().id().find(&expected_id)?;
        (exact_organization_representative(
            ctx,
            &npc,
            &chapter.settlement_id,
            &location_id,
        )
        .as_deref()
            == Some(ERRANTRY_ISSUER_ORGANIZATION_ID))
        .then(|| (npc, chapter.settlement_id.clone(), location_id))
    })
}

/// Creates or reuses an accepted, immediately playable errantry quest.
#[reducer]
pub fn load_puzzle_demo(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    if !puzzle_demo_enabled() {
        return Err("Puzzle demo loading is disabled in this module build".into());
    }
    materialize_order_errantry(ctx, character_id, None, ErrantryLaunch::DirectDemoCamp).map(|_| ())
}

/// Narrow production issuance seam: the client identifies only its live
/// dialogue session. The reducer derives and verifies the NPC, chapter,
/// organization capability, settlement, and location before materializing.
#[reducer]
pub fn accept_order_errantry(
    ctx: &ReducerContext,
    character_id: u64,
    dialogue_session_id: String,
    action_id: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    if action_id.is_empty() || action_id.len() > 160 {
        return Err("Errantry acceptance action ID is invalid".into());
    }
    let receipt_id = format!("order-errantry-accept:{dialogue_session_id}:{action_id}");
    // Exact lost-response retries return before physical dialogue presence is
    // revalidated. The immutable receipt is accepted only while its sourced
    // case and contract still agree with current errantry authority.
    if let Some(receipt) = ctx
        .db
        .order_errantry_acceptance_receipt()
        .id()
        .find(&receipt_id)
    {
        let current = ctx
            .db
            .errantry_authority()
            .case_id()
            .find(&receipt.case_id);
        return if receipt.dialogue_session_id == dialogue_session_id
            && receipt.action_id == action_id
            && receipt.character_id == character_id
            && current.is_some_and(|errantry| errantry.contract_id == receipt.contract_id)
        {
            Ok(())
        } else {
            Err("Conflicting Order errantry acceptance retry".into())
        };
    }
    let session = ctx
        .db
        .dialogue_session()
        .id()
        .find(&dialogue_session_id)
        .ok_or("Dialogue session not found")?;
    if session.owner_character_id != character_id
        || session.conversation_id != "organization-representative"
    {
        return Err("Errantry must be accepted in the owning organization dialogue".into());
    }
    let npc = require_live_dialogue_presence(ctx, &session, character_id)?;
    let organization_id =
        exact_organization_representative(ctx, &npc, &session.settlement_id, &session.location_id)
            .ok_or("Dialogue NPC is not an exact chapter representative")?;
    let organization = adventuresim_core::organization::organization(&organization_id)
        .filter(|organization| organization.errantry_issuance)
        .ok_or("This organization cannot issue errantry")?;
    if organization.id != ERRANTRY_ISSUER_ORGANIZATION_ID {
        return Err("Only the Order of St. George can issue this errantry".into());
    }
    let materialized = materialize_order_errantry(
        ctx,
        character_id,
        Some((npc, session.settlement_id, session.location_id)),
        ErrantryLaunch::NormalTravel,
    )?;
    ctx.db
        .order_errantry_acceptance_receipt()
        .insert(OrderErrantryAcceptanceReceipt {
            id: receipt_id,
            dialogue_session_id,
            action_id,
            character_id,
            case_id: materialized.case_id,
            contract_id: materialized.contract_id,
        });
    Ok(())
}

fn materialize_order_errantry(
    ctx: &ReducerContext,
    character_id: u64,
    issuer_override: Option<(
        crate::settlement_population::SettlementNpc,
        String,
        String,
    )>,
    launch: ErrantryLaunch,
) -> Result<MaterializedErrantry, String> {
    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character.party_id.clone().ok_or("Must be in a party")?;
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can accept or load this errantry".into());
    }
    if launch == ErrantryLaunch::DirectDemoCamp
        && let Some((challenge, contract)) = active_puzzle_demo(ctx, &party_id, character_id)
    {
        if let Some(active) = party.active_contract_id.as_deref()
            && active != contract.id
        {
            return Err("Finish or abandon the active quest before loading the puzzle demo".into());
        }
        party.active_contract_id = Some(contract.id.clone());
        ctx.db.party_authority().id().update(party);
        return Ok(MaterializedErrantry {
            case_id: challenge.case_id,
            contract_id: contract.id,
        });
    }
    let origin_settlement_id = character
        .current_settlement_id
        .clone()
        .ok_or("Load a fresh puzzle demo while in a settlement")?;
    let origin_settlement = ctx
        .db
        .settlement()
        .id()
        .find(&origin_settlement_id)
        .ok_or("Current settlement not found")?;
    let (issuer, issuer_settlement_id, issuer_location_id) = issuer_override
        .or_else(|| order_errantry_issuer(ctx))
        .ok_or("No live Order of St. George chapter representative")?;
    if let Some(active) = party.active_contract_id.clone() {
        let active_contract = ctx
            .db
            .contract_authority()
            .id()
            .find(&active)
            .ok_or("Active quest not found")?;
        return Err(format!(
            "Finish or abandon the active quest '{}' before accepting errantry",
            active_contract.title
        ));
    }
    let namespace = match launch {
        ErrantryLaunch::DirectDemoCamp => "demo",
        ErrantryLaunch::NormalTravel => "order",
    };
    let demo_prefix = format!("challenge:ordered-sigils:{namespace}:{character_id}:");
    let ordinal = ctx
        .db
        .challenge_authority()
        .party_id()
        .filter(&party_id)
        .filter(|challenge| challenge.id.starts_with(&demo_prefix))
        .count() as u64;
    let suffix = errantry_suffix(character_id, ordinal, launch);
    let case_id = format!("case:errantry-puzzle:{suffix}");
    let contract_id = format!("contract:errantry-puzzle:{suffix}");
    let challenge_id = format!("challenge:ordered-sigils:{suffix}");
    let courier_challenge_id = format!("challenge:wounded-order-courier:{suffix}");
    let case_site_id = format!("case-site:errantry-finale:{suffix}");
    let hostile_group_id = format!("hostile-group:errantry-finale:{suffix}");
    // The preliminary trial is a true optional boon: defeating the finale
    // resolves the case whether or not ChallengeSolved was emitted.
    let objective = adventuresim_core::case::ObjectiveExpression::new(vec![
        adventuresim_core::case::ObjectivePath {
            objectives: vec![adventuresim_core::case::Objective {
                id: adventuresim_core::case::ObjectiveId::new(format!(
                    "objective:defeat-finale:{suffix}"
                ))
                .map_err(|_| "Finale objective ID is invalid")?,
                requirement: adventuresim_core::case::ObjectiveRequirement::Defeat {
                    hostile_group_id: hostile_group_id.clone(),
                    count: 4,
                },
            }],
        },
    ])
    .map_err(|_| "Puzzle objective is invalid")?;
    let objective_expression_json =
        serde_json::to_string(&objective).map_err(|_| "Could not encode puzzle objective")?;
    ctx.db.case_authority().insert(CaseAuthority {
        id: case_id.clone(),
        investigation_case_id: format!("errantry:{suffix}"),
        provenance_kind: "manual".into(),
        generated_case_id: String::new(),
        local_problem_id: None,
        objective_expression_json,
        resolution_status: CaseResolutionStatus::Open,
        resolved_by_party_id: None,
    });
    ctx.db.contract_authority().insert(Contract {
        id: contract_id.clone(),
        gateway_bucket: 0,
        case_id: case_id.clone(),
        title: "The Trial of Five Signs".into(),
        description: "A knightly errand leads directly to a trial of discernment.".into(),
        difficulty: 1,
        gold_reward: 0,
        xp_reward: 0,
        settlement_id: issuer_settlement_id.clone(),
        service_id: "errantry:order_saint_george".into(),
        issuer_npc_id: issuer.id.clone(),
        status: ContractStatus::Accepted,
        accepted_by: Some(party_id.clone()),
        opposition_wording: "an enchanted gate".into(),
        opposition_count_wording: "a household guard".into(),
        accepted_at_minute: Some(crate::time::refresh_clock(ctx)?),
        paid_at_minute: None,
    });
    let seed = 0x4b4e_4947_4854_4c59 ^ character_id ^ ordinal.rotate_left(23);
    let puzzle = adventuresim_core::errantry::OrderedSigilPuzzle::generate(seed);
    let now = crate::time::refresh_clock(ctx)?;
    let site = CaseSiteAuthority {
        id_key: case_site_id.clone(),
        id: CaseSiteId::from(case_site_id.clone()),
        case_id: case_id.clone(),
        origin_settlement_id: origin_settlement_id.clone(),
        name: "The Black Knight's Ford".into(),
        description: "A ford held by a knight who preys upon travelers.".into(),
        scene_key: "forest-clearing".into(),
        longitude_e7: ((origin_settlement.coord_x + 0.02) * 10_000_000.0) as i32,
        latitude_e7: ((origin_settlement.coord_y + 0.02) * 10_000_000.0) as i32,
        coordinates_are_geographic: true,
        distance_m: 2_000,
    };
    ctx.db.case_site_authority().insert(site.clone());
    materialize_hostile_group(
        ctx,
        &hostile_group_id,
        &site,
        ERRANTRY_FINALE_THREAT_ID.into(),
        4,
        6,
    )?;
    let frame = adventuresim_core::errantry::ErrantryFrame {
        id: format!("errantry:five-signs:{suffix}"),
        purpose: adventuresim_core::errantry::ErrantryPurpose::ProveWorth,
        charge: "Keep faith upon the road and answer the trial with discernment.".into(),
        trials: vec![
            adventuresim_core::errantry::TrialBinding {
                order: 0,
                trial_id: format!("trial:five-signs:{suffix}"),
                challenge_id: Some(challenge_id.clone()),
                site_id: "journey-camp:unbound".into(),
                kind: adventuresim_core::errantry::TrialKind::Puzzle,
            },
            adventuresim_core::errantry::TrialBinding {
                order: 1,
                trial_id: format!("trial:wounded-courier:{suffix}"),
                challenge_id: Some(courier_challenge_id.clone()),
                site_id: "journey-camp:unbound".into(),
                kind: adventuresim_core::errantry::TrialKind::Social,
            },
        ],
    };
    ctx.db.challenge_authority().insert(ChallengeAuthority {
        id: challenge_id.clone(),
        gateway_bucket: 0,
        case_id: case_id.clone(),
        party_id: party_id.clone(),
        finale_case_site_id: case_site_id.clone(),
        finale_hostile_group_id: hostile_group_id.clone(),
        journey_departure_minute: 0,
        camp_movement_minute: 0,
        camp_elapsed_minute: 0,
        errantry_frame_json: serde_json::to_string(&frame)
            .map_err(|_| "Could not encode errantry frame authority")?,
        puzzle_json: serde_json::to_string(&puzzle)
            .map_err(|_| "Could not encode puzzle authority")?,
        presenter_catalog_id: ChallengePresenterCatalogId::LadyBeneathThornV1,
        revision: 0,
        open: true,
        solved_at_minute: None,
    });
    ctx.db
        .road_challenge_authority()
        .insert(RoadChallengeAuthority {
            id: courier_challenge_id.clone(),
            gateway_bucket: 0,
            party_id: party_id.clone(),
            case_id: case_id.clone(),
            finale_case_site_id: case_site_id.clone(),
            finale_hostile_group_id: hostile_group_id.clone(),
            journey_departure_minute: 0,
            camp_movement_minute: 0,
            available_at_elapsed_minute: 0,
            catalog_id: RoadChallengeCatalogId::WoundedOrderCourierV1,
            revision: 0,
            open: true,
            resolved_choice: None,
        });
    let finale_defenses = vec![
        adventuresim_core::errantry::FinaleDefenseKind::UnnaturalProwess,
        adventuresim_core::errantry::FinaleDefenseKind::Reinforcements,
    ];
    ctx.db.errantry_authority().insert(ErrantryAuthority {
        case_id: case_id.clone(),
        contract_id: contract_id.clone(),
        issuer_organization_id: ERRANTRY_ISSUER_ORGANIZATION_ID.into(),
        issuer_npc_id: issuer.id,
        issuer_settlement_id,
        issuer_location_id,
        finale_case_site_id: case_site_id.clone(),
        finale_hostile_group_id: hostile_group_id,
        preliminary_challenge_ids: vec![challenge_id.clone(), courier_challenge_id],
        finale_defenses_json: serde_json::to_string(&finale_defenses)
            .map_err(|_| "Could not encode finale defenses")?,
    });
    crate::investigation::disclose_exact_case_site(
        ctx,
        character_id,
        &case_id,
        &site,
        "the Order of St. George",
    )?;
    let destination = JourneyEndpoint::CaseSite(JourneyCaseSiteEndpoint {
        id: CaseSiteId::from(case_site_id.clone()),
        name: site.name.clone(),
    });
    party.active_contract_id = Some(contract_id.clone());
    if launch == ErrantryLaunch::DirectDemoCamp {
        let camp_movement_minute = 60;
        let camp_elapsed_minute = 60;
        ctx.db.party_journey_authority().insert(PartyJourney {
            party_id: party_id.clone(),
            gateway_bucket: 0,
            origin: JourneyEndpoint::Settlement(JourneySettlementEndpoint {
                id: origin_settlement_id,
                name: origin_settlement.name,
            }),
            destination: destination.clone(),
            total_minutes: 120,
            completed_minutes: camp_movement_minute,
            camp_stop_minutes: vec![camp_movement_minute],
            forecast_camp_stop_minutes: Vec::new(),
            fatigue_percent: party.camp_fatigue_percent,
            plan_version: 2,
            departure_minute: now,
            total_elapsed_minutes: 120,
            completed_elapsed_minutes: camp_elapsed_minute,
            walking_minutes_per_day: party.walking_minutes_per_day,
            travel_at_night: party.travel_at_night,
            camp_duration_mode: party.camp_duration_mode,
            fixed_camp_minutes: party.fixed_camp_minutes,
        });
        ctx.db
            .party_journey_encounter_authority()
            .insert(PartyJourneyEncounterAuthority {
                party_id: party_id.clone(),
                seed: ctx.random(),
                next_roll: 1,
            });
        party.current_settlement_id = None;
        party.current_case_site_id = None;
        party.camp_destination = Some(destination);
        party.camp_remaining_minutes = 60;
        ctx.db.party_authority().id().update(party);
        for member_id in living_party_member_ids(ctx, &party_id) {
            let mut member = ctx
                .db
                .character()
                .id()
                .find(member_id)
                .ok_or("Party member not found")?;
            member.current_settlement_id = None;
            crate::investigation::set_character_case_site(ctx, member_id, None);
            ctx.db.character().id().update(member);
        }
        bind_errantry_trials_to_current_camp(ctx, &party_id)?;
    } else {
        ctx.db.party_authority().id().update(party);
    }
    Ok(MaterializedErrantry {
        case_id,
        contract_id,
    })
}

#[cfg(test)]
mod challenge_source_boundary_tests {
    use super::{
        ChallengeAttemptReceipt, ERRANTRY_FINALE_THREAT_ID,
        FEY_COUNTERMEASURE_MULTIPLIER_BPS, autoresolve_enemy_with_countermeasure,
        puzzle_demo_suffix, validate_challenge_retry,
    };

    #[test]
    fn public_projection_omits_private_truth_fields() {
        let source = include_str!("challenges.rs");
        let projection = source
            .split("pub struct BackendChallenge")
            .nth(1)
            .unwrap()
            .split("#[view")
            .next()
            .unwrap();
        assert!(!projection.contains("seed:"));
        assert!(!projection.contains("solution"));
        assert!(!projection.contains("puzzle_json:"));
        assert!(source.contains("ChallengeSolved"));
        assert!(source.contains("challenge.revision != expected_revision"));
        assert!(source.contains("contract.accepted_by.as_deref() != Some(&party_id)"));
    }

    #[test]
    fn lost_success_response_retries_exactly_but_conflicts_fail() {
        let receipt = ChallengeAttemptReceipt {
            id: "challenge-attempt:challenge:test:party:test:0".into(),
            challenge_id: "challenge:test".into(),
            case_id: "case:test".into(),
            party_id: "party:test".into(),
            character_id: 7,
            submitted_revision: 0,
            ordering_json: "[\"Crown\",\"Hart\",\"Moon\",\"Rose\",\"Sword\"]".into(),
            correct: true,
            resulting_revision: 1,
            attempted_at_minute: 42,
        };
        validate_challenge_retry(
            &receipt,
            "case:test",
            "challenge:test",
            "party:test",
            7,
            "[\"Crown\",\"Hart\",\"Moon\",\"Rose\",\"Sword\"]",
        )
        .unwrap();
        assert!(
            validate_challenge_retry(
                &receipt,
                "case:test",
                "challenge:test",
                "party:test",
                7,
                "[\"Sword\",\"Hart\",\"Moon\",\"Rose\",\"Crown\"]",
            )
            .unwrap_err()
            .contains("Conflicting retry")
        );

        let source = include_str!("challenges.rs");
        let receipt_check = source
            .find("if let Some(existing) = ctx.db.challenge_attempt_receipt()")
            .unwrap();
        let active_contract = source.find("let active_contract_id = party").unwrap();
        let case_open = source.find("case.resolution_status !=").unwrap();
        assert!(receipt_check < active_contract && receipt_check < case_open);
    }

    #[test]
    fn demo_is_reused_and_materializes_a_real_bound_camp() {
        assert_ne!(
            puzzle_demo_suffix(7, 0),
            puzzle_demo_suffix(7, 1)
        );
        let source = include_str!("challenges.rs");
        let loader = source.split("fn materialize_order_errantry").nth(1).unwrap();
        let reuse = loader.find("active_puzzle_demo").unwrap();
        let fresh_ordinal = loader.find("let ordinal =").unwrap();
        assert!(reuse < fresh_ordinal);
        assert!(loader.contains("return Ok(())"));
        assert!(loader.contains("ordinal.rotate_left(23)"));
        let reuse_lookup = source
            .split("fn active_puzzle_demo")
            .nth(1)
            .unwrap()
            .split("fn puzzle_demo_suffix")
            .next()
            .unwrap();
        assert!(
            reuse_lookup
                .contains("challenge:ordered-sigils:demo:{character_id}:")
        );
        assert!(loader.contains("PartyJourney"));
        assert!(loader.contains("camp_destination = Some(destination)"));
        assert!(loader.contains("ErrantryAuthority"));
        assert!(source.contains("accept_order_errantry"));
        assert!(source.contains("organization.errantry_issuance"));
    }

    #[test]
    fn trial_is_optional_camp_authority_and_boon_is_snapshot_only() {
        let source = include_str!("challenges.rs");
        let camp_match = source
            .split("fn journey_at_bound_trial_camp")
            .nth(1)
            .unwrap()
            .split("fn party_at_bound_trial_camp_view")
            .next()
            .unwrap();
        assert!(camp_match.contains("journey_camp_identity_matches"));
        assert!(!camp_match.contains("completed_elapsed_minutes"));
        assert!(source.contains("encounter.status == \"awaiting_choice\""));
        assert!(!source.contains("insert(StrategicEncounter"));
        assert!(source.contains("ErrantryCountermeasure"));
        assert!(source.contains("FEY_COUNTERMEASURE_SCALE_FLOOR_BPS"));

        let mission = include_str!("custody_objectives.rs");
        assert!(mission.contains("errantry_mission_scale_snapshot"));
        assert!(mission.contains("base_enemy_combat_scale_bps"));
        assert!(mission.contains("countermeasure_source_challenge_id"));
        let tactical = include_str!("../tactical.rs");
        assert!(tactical.contains("mission.enemy_combat_scale_bps"));
        let autoresolve = include_str!("autoresolve.rs");
        assert!(autoresolve.contains("mission.enemy_combat_scale_bps"));
    }

    #[test]
    fn rested_courier_trial_is_optional_authorized_and_material() {
        let source = include_str!("challenges.rs");
        let reducer = source
            .split("pub fn resolve_errantry_road_challenge")
            .nth(1)
            .and_then(|tail| tail.split("fn puzzle_demo_enabled").next())
            .unwrap();
        assert!(reducer.contains("require_strategic_gateway"));
        assert!(reducer.contains("party.leader_id != character_id"));
        assert!(reducer.contains("party_at_bound_road_challenge"));
        assert!(source.contains("encounter.status == \"awaiting_choice\""));
        assert!(reducer.contains("CAPTURED_DISPATCH_ITEM_ID"));
        assert!(reducer.contains("ErrantryFinaleDefenseKind::Reinforcements"));
        assert!(!reducer.contains("ingest_case_outcome_fact"));
        assert!(source.contains("COURIER_REST_DELAY_MINUTES"));
        assert!(source.contains("preliminary_challenge_ids"));
        assert!(source.contains("finale_defenses_json"));
    }

    #[test]
    fn mission_snapshot_records_all_typed_approach_effects() {
        let source = include_str!("challenges.rs");
        let snapshot = source
            .split("pub(crate) fn errantry_mission_scale_snapshot")
            .nth(1)
            .and_then(|tail| tail.split("fn core_defense_kind").next())
            .unwrap();
        assert!(snapshot.contains("resolve_finale_approach"));
        assert!(snapshot.contains("finale_defenses_json"));
        assert!(snapshot.contains("combat_capability_multiplier_bps"));
        assert!(snapshot.contains("serde_json::to_string(&resolution)"));
        let mission = include_str!("authority_model.rs");
        assert!(mission.contains("errantry_approach_snapshot_json"));
    }

    #[test]
    fn authored_finale_threat_and_boon_reduce_consumed_strength() {
        assert_eq!(
            ERRANTRY_FINALE_THREAT_ID
                .parse::<adventuresim_core::bestiary::ThreatId>()
                .unwrap(),
            adventuresim_core::bestiary::ThreatId::ArmedRetainer
        );
        let base = autoresolve_enemy_with_countermeasure(
            1,
            ERRANTRY_FINALE_THREAT_ID,
            6,
            10_000,
            10_000,
        )
        .unwrap();
        let aided = autoresolve_enemy_with_countermeasure(
            2,
            ERRANTRY_FINALE_THREAT_ID,
            6,
            7_500,
            FEY_COUNTERMEASURE_MULTIPLIER_BPS,
        )
        .unwrap();
        assert!(aided.attributes.endurance < base.attributes.endurance);
        assert!(aided.skills.sword_hours < base.skills.sword_hours);
        let (expected_physical, expected_training) =
            adventuresim_core::threat_escalation::combat_scaling_multipliers(
                7_500,
                FEY_COUNTERMEASURE_MULTIPLIER_BPS,
            );
        assert!(
            (aided.attributes.endurance / base.attributes.endurance - expected_physical).abs()
                < 0.000_01
        );
        assert!(
            (aided.skills.sword_hours / base.skills.sword_hours - expected_training).abs()
                < 0.000_01
        );

        let source = include_str!("challenges.rs");
        assert!(source.contains("ERRANTRY_FINALE_THREAT_ID.into()"));
        assert!(source.contains("4,\n        6,"));
        assert!(source.contains("ErrantryLaunch::NormalTravel"));
        assert!(source.contains("ErrantryLaunch::DirectDemoCamp"));
    }

    #[test]
    fn committed_acceptance_retry_precedes_live_presence_validation() {
        let source = include_str!("challenges.rs");
        let reducer = source
            .split("pub fn accept_order_errantry")
            .nth(1)
            .and_then(|tail| tail.split("fn materialize_order_errantry").next())
            .unwrap();
        assert!(
            reducer
                .find("order_errantry_acceptance_receipt()")
                .unwrap()
                < reducer.find("require_live_dialogue_presence").unwrap()
        );
        assert!(reducer.contains("errantry.contract_id == receipt.contract_id"));
        assert!(reducer.contains("Conflicting Order errantry acceptance retry"));
    }
}
