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
    pub site_id: String,
    pub errantry_frame_json: String,
    pub puzzle_json: String,
    pub presenter_json: String,
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
    pub site_id: String,
    pub puzzle_projection_json: String,
    pub presenter_json: String,
    pub revision: u32,
    pub open: bool,
    pub solved: bool,
    pub last_attempt_correct: Option<bool>,
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
            let last_attempt_correct = ctx
                .db
                .challenge_attempt_receipt()
                .challenge_id()
                .filter(&challenge.id)
                .max_by_key(|receipt| receipt.submitted_revision)
                .map(|receipt| receipt.correct);
            Some(BackendChallenge {
                id: challenge.id,
                case_id: challenge.case_id,
                party_id: challenge.party_id,
                owner_character_id: party.leader_id,
                site_id: challenge.site_id,
                puzzle_projection_json: projection,
                presenter_json: challenge.presenter_json,
                revision: challenge.revision,
                open: challenge.open,
                solved: challenge.solved_at_minute.is_some(),
                last_attempt_correct,
            })
        })
        .collect()
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

/// Submit one complete ordering. Every authority coordinate is derived again:
/// selected character, party leadership, active accepted contract, case,
/// challenge, current site, open state, and expected revision.
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
    let case = ctx
        .db
        .case_authority()
        .id()
        .find(&case_id)
        .ok_or("Case not found")?;
    if case.resolution_status != CaseResolutionStatus::Open {
        return Err("Case is no longer open".into());
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
    let at_site = character.current_settlement_id.as_deref() == Some(&challenge.site_id)
        && party.current_settlement_id.as_deref() == Some(&challenge.site_id);
    if !at_site {
        return Err("Party is not at the challenge site".into());
    }
    let ordering = parse_ordered_sigils(&ordering_json)?;
    let normalized_ordering =
        serde_json::to_string(&ordering).map_err(|_| "Could not encode sigil ordering")?;
    let receipt_id = format!(
        "challenge-attempt:{}:{}:{}",
        challenge.id, party_id, expected_revision
    );
    if let Some(existing) = ctx.db.challenge_attempt_receipt().id().find(&receipt_id) {
        return if existing.case_id == case_id
            && existing.character_id == character_id
            && existing.ordering_json == normalized_ordering
        {
            Ok(())
        } else {
            Err("Conflicting retry for challenge revision".into())
        };
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
    if bound_trial.site_id != challenge.site_id
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
        ingest_case_outcome_fact(
            ctx,
            &format!("challenge-solved:{}", challenge.id),
            &case_id,
            &party_id,
            adventuresim_core::case::OutcomeFactKind::ChallengeSolved {
                challenge_id: challenge.id,
            },
        )?;
    }
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

/// Creates or reuses an accepted, immediately playable errantry quest.
#[reducer]
pub fn load_puzzle_demo(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    if !puzzle_demo_enabled() {
        return Err("Puzzle demo loading is disabled in this module build".into());
    }
    let character = crate::character::require_living_character(ctx, character_id)?;
    let settlement_id = character
        .current_settlement_id
        .clone()
        .ok_or("Load the puzzle demo while in a settlement")?;
    let party_id = character.party_id.clone().ok_or("Must be in a party")?;
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can load the puzzle demo".into());
    }

    let suffix = format!("{character_id}-{settlement_id}");
    let case_id = format!("case:errantry-puzzle:{suffix}");
    let contract_id = format!("contract:errantry-puzzle:{suffix}");
    let challenge_id = format!("challenge:ordered-sigils:{suffix}");
    if let Some(active) = &party.active_contract_id
        && active != &contract_id
    {
        return Err("Finish or abandon the active quest before loading the puzzle demo".into());
    }
    let objective = adventuresim_core::case::ObjectiveExpression::new(vec![
        adventuresim_core::case::ObjectivePath {
            objectives: vec![adventuresim_core::case::Objective {
                id: adventuresim_core::case::ObjectiveId::new(format!(
                    "objective:solve-puzzle:{suffix}"
                ))
                .map_err(|_| "Puzzle objective ID is invalid")?,
                requirement: adventuresim_core::case::ObjectiveRequirement::SolveChallenge {
                    challenge_id: challenge_id.clone(),
                },
            }],
        },
    ])
    .map_err(|_| "Puzzle objective is invalid")?;
    let objective_expression_json =
        serde_json::to_string(&objective).map_err(|_| "Could not encode puzzle objective")?;
    if ctx.db.case_authority().id().find(&case_id).is_none() {
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
    }
    if ctx
        .db
        .contract_authority()
        .id()
        .find(&contract_id)
        .is_none()
    {
        ctx.db.contract_authority().insert(Contract {
            id: contract_id.clone(),
            gateway_bucket: 0,
            case_id: case_id.clone(),
            title: "The Trial of Five Signs".into(),
            description: "A knightly errand leads directly to a trial of discernment.".into(),
            difficulty: 1,
            gold_reward: 0,
            xp_reward: 0,
            settlement_id: settlement_id.clone(),
            service_id: "developer:puzzle-demo".into(),
            issuer_npc_id: "developer:puzzle-demo".into(),
            status: ContractStatus::Accepted,
            accepted_by: Some(party_id.clone()),
            opposition_wording: "an enchanted gate".into(),
            opposition_count_wording: "one".into(),
            accepted_at_minute: Some(crate::time::refresh_clock(ctx)?),
            paid_at_minute: None,
        });
    }
    if ctx
        .db
        .challenge_authority()
        .id()
        .find(&challenge_id)
        .is_none()
    {
        let seed = 0x4b4e_4947_4854_4c59 ^ character_id;
        let puzzle = adventuresim_core::errantry::OrderedSigilPuzzle::generate(seed);
        let presenter = adventuresim_core::errantry::presenter(
            adventuresim_core::errantry::PresenterKind::FeySpoken,
        );
        let frame = adventuresim_core::errantry::ErrantryFrame {
            id: format!("errantry:five-signs:{suffix}"),
            purpose: adventuresim_core::errantry::ErrantryPurpose::ProveWorth,
            charge: "Keep faith upon the road and answer the trial with discernment.".into(),
            trials: vec![adventuresim_core::errantry::TrialBinding {
                order: 0,
                trial_id: format!("trial:five-signs:{suffix}"),
                challenge_id: Some(challenge_id.clone()),
                site_id: settlement_id.clone(),
                kind: adventuresim_core::errantry::TrialKind::Puzzle,
            }],
        };
        ctx.db.challenge_authority().insert(ChallengeAuthority {
            id: challenge_id,
            gateway_bucket: 0,
            case_id,
            party_id: party_id.clone(),
            site_id: settlement_id.clone(),
            errantry_frame_json: serde_json::to_string(&frame)
                .map_err(|_| "Could not encode errantry frame authority")?,
            puzzle_json: serde_json::to_string(&puzzle)
                .map_err(|_| "Could not encode puzzle authority")?,
            presenter_json: serde_json::to_string(&presenter)
                .map_err(|_| "Could not encode puzzle presenter")?,
            revision: 0,
            open: true,
            solved_at_minute: None,
        });
    }
    party.active_contract_id = Some(contract_id);
    party.current_settlement_id = Some(settlement_id);
    ctx.db.party_authority().id().update(party);
    Ok(())
}

#[cfg(test)]
mod challenge_source_boundary_tests {
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
}
