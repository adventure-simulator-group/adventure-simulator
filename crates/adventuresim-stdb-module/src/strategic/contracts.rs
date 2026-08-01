fn contract_interaction_receipt_id(
    contract_id: &str,
    party_id: &str,
    stage: ContractInteractionStage,
) -> String {
    format!("interaction:{contract_id}:{party_id}:{stage:?}").to_lowercase()
}

fn record_contract_issuer_interaction(
    ctx: &ReducerContext,
    character_id: u64,
    contract_id: String,
    stage: ContractInteractionStage,
    dialogue_session_id: String,
    dialogue_action_id: String,
    dialogue_revision: u64,
    location_id: String,
) -> Result<(), String> {
    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character.party_id.ok_or("Must be in a party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can interact for a contract".into());
    }
    let contract = ctx
        .db
        .contract_authority()
        .id()
        .find(&contract_id)
        .ok_or("Contract not found")?;
    contract.parsed_state()?;
    let expected = match stage {
        ContractInteractionStage::Accept => ContractStatus::Offered,
        ContractInteractionStage::Report => ContractStatus::ReadyToReport,
    };
    if contract.status != expected
        || (stage == ContractInteractionStage::Report
            && contract.accepted_by.as_deref() != Some(&party_id))
    {
        return Err("Contract is not at the requested interaction stage".into());
    }
    if character.current_settlement_id.as_deref() != Some(&contract.settlement_id) {
        return Err("Contract issuer interaction requires their settlement".into());
    }
    let issuer = ctx
        .db
        .settlement_resident_profile()
        .character_id()
        .find(contract.issuer_resident_character_id)
        .ok_or("Contract issuer is not persistent")?;
    let presence = ctx
        .db
        .settlement_resident_presence()
        .character_id()
        .find(issuer.character_id)
        .ok_or("Contract issuer has no presence")?;
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(720, |time| time.minutes);
    if issuer.home_settlement_id != contract.settlement_id
        || issuer.service_id != contract.service_id
        || presence.settlement_id != contract.settlement_id
        || presence.location_id != location_id
        || !crate::settlement_population::npc_is_present(&presence, minute)
    {
        return Err("Contract issuer is not available for interaction".into());
    }
    let id = contract_interaction_receipt_id(&contract_id, &party_id, stage);
    let row = ContractIssuerInteractionReceipt {
        id: id.clone(),
        contract_id,
        party_id,
        stage,
        issuer_resident_character_id: issuer.character_id,
        interacting_character_id: character_id,
        interacted_at_minute: crate::time::refresh_clock(ctx)?,
        dialogue_session_id,
        dialogue_action_id,
        dialogue_revision,
        location_id,
        consumed: false,
    };
    if ctx
        .db
        .contract_issuer_interaction_receipt()
        .id()
        .find(&id)
        .is_some()
    {
        ctx.db
            .contract_issuer_interaction_receipt()
            .id()
            .update(row);
    } else {
        ctx.db.contract_issuer_interaction_receipt().insert(row);
    }
    Ok(())
}

fn record_dialogue_contract_issuer_interaction(
    ctx: &ReducerContext,
    character_id: u64,
    contract_id: String,
    stage: ContractInteractionStage,
    session: &DialogueSession,
    action_id: &str,
    resulting_revision: u64,
) -> Result<(), String> {
    if action_id == "start" {
        return Err("Contract interaction requires an explicit dialogue action".into());
    }
    let issuer = require_live_dialogue_presence(ctx, session, character_id)?;
    let contract = ctx
        .db
        .contract_authority()
        .id()
        .find(&contract_id)
        .ok_or("Contract not found")?;
    if issuer.character_id != contract.issuer_resident_character_id
        || issuer.service_id != contract.service_id
        || session.settlement_id != contract.settlement_id
    {
        return Err("This dialogue is not with the contract issuer".into());
    }
    record_contract_issuer_interaction(
        ctx,
        character_id,
        contract_id,
        stage,
        session.id.clone(),
        action_id.to_string(),
        resulting_revision,
        session.location_id.clone(),
    )
}

/// Disposable simulations cannot use the gateway-owned dialogue reducers.
/// This is the sole alternate producer and is restricted to the identity that
/// owns the simulation character; ordinary players and the web gateway cannot
/// mint an interaction receipt through it.
#[reducer]
pub fn simulate_contract_issuer_interaction(
    ctx: &ReducerContext,
    character_id: u64,
    contract_id: String,
    stage: ContractInteractionStage,
) -> Result<(), String> {
    if !crate::simulation::sender_owns_simulation_character(ctx, character_id) {
        return Err("Only an owned disposable simulation may simulate NPC interaction".into());
    }
    let contract = ctx
        .db
        .contract_authority()
        .id()
        .find(&contract_id)
        .ok_or("Contract not found")?;
    let presence = ctx
        .db
        .settlement_resident_presence()
        .character_id()
        .find(contract.issuer_resident_character_id)
        .ok_or("Contract issuer has no presence")?;
    record_contract_issuer_interaction(
        ctx,
        character_id,
        contract_id,
        stage,
        format!("simulation:{character_id}"),
        format!("simulation:{stage:?}").to_lowercase(),
        0,
        presence.location_id,
    )
}

fn consume_contract_interaction(
    ctx: &ReducerContext,
    contract_id: &str,
    party_id: &str,
    stage: ContractInteractionStage,
) -> Result<(), String> {
    let id = contract_interaction_receipt_id(contract_id, party_id, stage);
    let mut receipt = ctx
        .db
        .contract_issuer_interaction_receipt()
        .id()
        .find(&id)
        .ok_or("Interact with the contract issuer first")?;
    if receipt.consumed || receipt.stage != stage {
        return Err("Contract issuer interaction receipt is unavailable".into());
    }
    receipt.consumed = true;
    ctx.db
        .contract_issuer_interaction_receipt()
        .id()
        .update(receipt);
    Ok(())
}

#[reducer]
pub fn accept_contract(
    ctx: &ReducerContext,
    character_id: u64,
    contract_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };

    let Some(party_id) = character.party_id.clone() else {
        return Err("Must be in a party to accept quests".into());
    };

    let Some(mut party) = ctx.db.party_authority().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    if party.leader_id != character_id {
        return Err("Only the party leader can accept quests".into());
    }

    if party.active_contract_id.is_some() {
        return Err("Party already has an active quest".into());
    }

    let Some(mut quest) = ctx.db.contract_authority().id().find(&contract_id) else {
        return Err("Quest not found".into());
    };
    quest.parsed_state()?;

    if quest.status != ContractStatus::Offered {
        return Err("Quest is not available".into());
    }
    let case = ctx
        .db
        .case_authority()
        .id()
        .find(&quest.case_id)
        .ok_or("Contract case not found")?;
    if case.resolution_status != CaseResolutionStatus::Open {
        return Err("This case is no longer open".into());
    }

    if character.current_settlement_id.as_ref() != Some(&quest.settlement_id) {
        return Err("Must be at the quest's settlement to accept it".into());
    }
    consume_contract_interaction(
        ctx,
        &contract_id,
        &party_id,
        ContractInteractionStage::Accept,
    )?;

    quest.status = ContractStatus::Accepted;
    quest.accepted_by = Some(party_id.clone());
    quest.accepted_at_minute = Some(crate::time::refresh_clock(ctx)?);
    let case_id = quest.case_id.clone();
    let contract_id = quest.id.clone();
    ctx.db.contract_authority().id().update(quest);

    let site = ctx
        .db
        .case_site_authority()
        .case_id()
        .filter(&case_id)
        .next()
        .ok_or("Quest destination is not configured")?;
    disclose_exact_case_site(ctx, character_id, &case_id, &site, "the contract issuer")?;

    party.active_contract_id = Some(contract_id);
    ctx.db.party_authority().id().update(party);
    Ok(())
}

/// Selects an already-known exact site for presentation. This reducer has no
/// quest, contract, objective, reward, movement, or knowledge side effects.
#[reducer]
pub fn track_case_site(
    ctx: &ReducerContext,
    character_id: u64,
    case_site_id: CaseSiteId,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character
        .party_id
        .ok_or("Must be in a party to track a case site")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can change the tracked site".into());
    }
    exact_case_site_for_observer(ctx, character_id, case_site_id.as_str())
        .ok_or("That exact site has not been disclosed to this observer")?;
    let row = PartyCaseSiteTracking {
        party_id: party_id.clone(),
        observer_character_id: character_id,
        case_site_id,
        tracked_at: crate::time::refresh_clock(ctx)?,
    };
    if ctx
        .db
        .party_case_site_tracking()
        .party_id()
        .find(&party_id)
        .is_some()
    {
        ctx.db.party_case_site_tracking().party_id().update(row);
    } else {
        ctx.db.party_case_site_tracking().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn abandon_contract(
    ctx: &ReducerContext,
    character_id: u64,
    contract_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    require_character_no_unresolved_encounter(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };

    let Some(party_id) = character.party_id.clone() else {
        return Err("Not in a party".into());
    };

    let Some(mut party) = ctx.db.party_authority().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    if party.leader_id != character_id {
        return Err("Only the party leader can abandon quests".into());
    }
    if crate::investigation::character_case_site_id(ctx, character_id).is_some() {
        return Err("Travel to a settlement before abandoning the quest".into());
    }

    let Some(mut quest) = ctx.db.contract_authority().id().find(&contract_id) else {
        return Err("Quest not found".into());
    };
    quest.parsed_state()?;

    if quest.accepted_by.as_ref() != Some(&party_id) {
        return Err("This quest is not accepted by your party".into());
    }
    if quest.status == ContractStatus::ReadyToReport {
        return Err("A completed quest must be returned to its questgiver".into());
    }

    quest.status = ContractStatus::Withdrawn;
    ctx.db.contract_authority().id().update(quest);

    party.active_contract_id = None;
    ctx.db.party_authority().id().update(party);
    Ok(())
}
