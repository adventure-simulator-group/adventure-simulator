pub(super) fn leader_is_actionable(
    party_id: &str,
    authoritative_leader_id: u64,
    character_id: u64,
    alive: bool,
    character_party_id: Option<&str>,
) -> bool {
    alive && character_id == authoritative_leader_id && character_party_id == Some(party_id)
}

pub(super) fn equipment_utility(profile: &AgentProfile, item: &Item) -> Option<f32> {
    let preference = &profile.equipment;
    let armor = matches!(item.kind, ItemKind::Armor | ItemKind::Clothing);
    let compatible = match preference.style {
        EquipmentStyle::Unarmored => !armor && item.melee && item.weight <= 2.5,
        EquipmentStyle::Ranged => !armor && item.ranged,
        EquipmentStyle::Light => (armor && item.weight <= 8.0) || (!armor && item.melee),
        EquipmentStyle::Heavy => armor || item.melee,
    };
    if !compatible || item.base_value.is_none() {
        return None;
    }
    let protection = item.coverage + item.resistance + item.padding;
    let mobility = item.flexibility + item.range_of_motion - item.weight * 0.1;
    let price = 1.0 / (1.0 + item.base_value.unwrap_or(1) as f32 / 100.0);
    Some(
        preference.protection_weight * protection
            + preference.mobility_weight * mobility
            + preference.price_weight * price
            + preference.reach_weight * item.reach,
    )
}

pub(super) fn root_requirement_matches_slot(
    requirement: &EquipmentOccupancyRequirement,
    slot: ItemSlot,
) -> bool {
    match slot {
        ItemSlot::LeftHolding => {
            requirement.location == EquipmentLocation::LeftHand
                && requirement.channel == EquipmentChannel::Held
        }
        ItemSlot::RightHolding => {
            requirement.location == EquipmentLocation::RightHand
                && requirement.channel == EquipmentChannel::Held
        }
        ItemSlot::AnyHolding => requirement.channel == EquipmentChannel::Held,
        ItemSlot::LeftArm => requirement.location == EquipmentLocation::LeftArm,
        ItemSlot::RightArm => requirement.location == EquipmentLocation::RightArm,
        ItemSlot::AnyArm => matches!(
            requirement.location,
            EquipmentLocation::LeftArm | EquipmentLocation::RightArm
        ),
        ItemSlot::LeftLeg => requirement.location == EquipmentLocation::LeftLeg,
        ItemSlot::RightLeg => requirement.location == EquipmentLocation::RightLeg,
        ItemSlot::AnyLeg => matches!(
            requirement.location,
            EquipmentLocation::LeftLeg | EquipmentLocation::RightLeg
        ),
        ItemSlot::Head => requirement.location == EquipmentLocation::Head,
        ItemSlot::Chest => requirement.location == EquipmentLocation::Chest,
        ItemSlot::Stomach => requirement.location == EquipmentLocation::Stomach,
        ItemSlot::None => true,
    }
}

pub fn run_core_loop(config: CoreLoopConfig) -> Result<CoreLoopReport, String> {
    let failure_recorder = FailureRecorder::new(config.failure_output.clone());
    let result = run_core_loop_inner(config, failure_recorder.clone());
    if let Err(error) = &result
        && let Err(diagnostic_error) = failure_recorder.write(error)
    {
        return Err(format!("{error}; {diagnostic_error}"));
    }
    result
}

fn run_core_loop_inner(
    config: CoreLoopConfig,
    failure_recorder: FailureRecorder,
) -> Result<CoreLoopReport, String> {
    config.validate()?;
    let bootstrap_token =
        bootstrap_token_from_environment(std::env::var(BOOTSTRAP_TOKEN_ENV).ok())?;
    let (connected_tx, connected_rx) = mpsc::sync_channel(1);
    let connect_error_tx = connected_tx.clone();
    let connection = DbConnection::builder()
        .with_uri(&config.host)
        .with_database_name(&config.database)
        .on_connect(move |_, _, _| {
            let _ = connected_tx.send(Ok(()));
        })
        .on_connect_error(move |_, error| {
            let _ = connect_error_tx.send(Err(error.to_string()));
        })
        .build()
        .map_err(|error| error.to_string())?;
    let (subscription_tx, subscription_rx) = mpsc::sync_channel(1);
    let subscription_error_tx = subscription_tx.clone();
    connection
        .subscription_builder()
        .on_applied(move |_| {
            let _ = subscription_tx.send(Ok(()));
        })
        .on_error(move |_, error| {
            let _ = subscription_error_tx.send(Err(error.to_string()));
        })
        // Deliberately enumerate the policy observation surface. In
        // particular, never transport backend infection episodes, committed
        // cuts, or full medical examinations into the simulator process.
        .add_query(|query| query.from.autoresolve_report())
        .add_query(|query| query.from.backend_case_battles())
        .add_query(|query| query.from.backend_case_site_pins())
        .add_query(|query| query.from.backend_dialogue_sessions())
        .add_query(|query| query.from.backend_dialogue_topic_options())
        .add_query(|query| query.from.backend_investigation_action_outcomes())
        .add_query(|query| query.from.backend_investigation_actions())
        .add_query(|query| query.from.backend_investigation_cases())
        .add_query(|query| query.from.backend_investigation_journal())
        .add_query(|query| query.from.backend_investigation_leads())
        .add_query(|query| query.from.backend_local_problem_trade_effects())
        .add_query(|query| query.from.battle_loot_item())
        .add_query(|query| query.from.battle_result())
        .add_query(|query| query.from.character())
        .add_query(|query| query.from.character_capability())
        .add_query(|query| query.from.character_death())
        .add_query(|query| query.from.character_equipped_item())
        .add_query(|query| query.from.equipment_occupancy())
        .add_query(|query| query.from.character_illness_status())
        .add_query(|query| query.from.character_needs())
        .add_query(|query| query.from.character_strategic_condition())
        .add_query(|query| query.from.character_time())
        .add_query(|query| query.from.character_training_schedule())
        .add_query(|query| query.from.inventory_item())
        .add_query(|query| query.from.food_lot())
        .add_query(|query| query.from.item())
        .add_query(|query| query.from.item_condition())
        .add_query(|query| query.from.local_problem_symptom())
        .add_query(|query| query.from.party())
        .add_query(|query| query.from.party_inventory_item())
        .add_query(|query| query.from.party_journey())
        .add_query(|query| query.from.party_journey_itinerary())
        .add_query(|query| query.from.party_join_request())
        .add_query(|query| query.from.party_member())
        .add_query(|query| query.from.party_stake())
        .add_query(|query| query.from.backend_contracts())
        .add_query(|query| query.from.backend_settlement_npcs())
        .add_query(|query| query.from.strategic_encounter())
        .add_query(|query| query.from.repair_order())
        .add_query(|query| query.from.settlement())
        .add_query(|query| query.from.settlement_npc_presence())
        .add_query(|query| query.from.settlement_smith())
        .add_query(|query| query.from.simulation_run())
        .add_query(|query| query.from.world_clock())
        .add_query(|query| query.from.world_data_import())
        .subscribe();
    connection.run_threaded();
    connected_rx
        .recv_timeout(ACTION_TIMEOUT)
        .map_err(|_| "connection timed out".to_string())??;
    subscription_rx
        .recv_timeout(ACTION_TIMEOUT)
        .map_err(|_| "subscription timed out".to_string())??;

    let profiles = (0..config.population)
        .map(|id| generate_profile(config.seed, id))
        .collect::<Vec<_>>();
    let base_id = 0x5349_4d00_0000_0000_u64 ^ config.seed.rotate_left(17);
    let character_ids = (0..config.population)
        .map(|id| base_id ^ u64::from(id + 1))
        .collect::<Vec<_>>();
    let mut runner = LiveRunner {
        connection,
        profiles,
        character_ids,
        metrics: CoreLoopMetrics::default(),
        trace: Vec::new(),
        sequence: 0,
        dialogue_nonce: 0,
        last_semantic_event: None,
        recorded_deaths: HashSet::new(),
        medically_paused_schedules: HashSet::new(),
        generated_seen_cases: HashSet::new(),
        generated_terminal_cases: HashSet::new(),
        generated_exact_site_cases: HashSet::new(),
        generated_traveled_cases: HashSet::new(),
        generated_finance_blocks: HashMap::new(),
        generated_discovery_backoff: HashMap::new(),
        failure_recorder,
    };
    if runner
        .connection
        .db
        .simulation_run()
        .iter()
        .next()
        .is_some()
        || runner.connection.db.character().iter().next().is_some()
    {
        return Err("refusing reused or populated simulation database".into());
    }
    let world_import = runner.connection.db.world_data_import().iter().next();
    if config.use_imported_world {
        let imported = world_import
            .as_ref()
            .filter(|import| import.completed)
            .ok_or("full-world mode requires a completed world_data_import")?;
        if Some(imported.manifest_digest.as_str())
            != config.expected_world_manifest_digest.as_deref()
        {
            return Err(
                "imported world manifest does not match the pinned expected manifest".into(),
            );
        }
        if imported.artifact_id.trim().is_empty()
            || imported.manifest_digest.len() != 64
            || runner.connection.db.settlement().iter().next().is_none()
        {
            return Err(
                "completed world_data_import has invalid provenance or no settlements".into(),
            );
        }
    } else if world_import.is_some() || runner.connection.db.settlement().iter().next().is_some() {
        return Err("fixture mode refuses imported or pre-existing settlement state".into());
    }
    let result = reducer_call!(runner, "claim_simulation_run", |cb| runner
        .connection
        .reducers
        .claim_simulation_run_then(
            bootstrap_token.clone(),
            config.run_nonce.clone(),
            config.seed,
            cb,
        ));
    runner.call(result)?;
    // The disposable simulation owns this otherwise-empty database, so its
    // authenticated connection is also the trusted strategic gateway.
    let result = reducer_call!(runner, "register_strategic_gateway", |cb| runner
        .connection
        .reducers
        .register_strategic_gateway_then(None, 0, cb));
    runner.call(result)?;
    // Re-subscribe the gateway-only observation surface after registration.
    // This does not rely on an already-applied subscription recomputing views
    // when gateway authority changes.
    let (gateway_subscription_tx, gateway_subscription_rx) = mpsc::sync_channel(1);
    let gateway_subscription_error_tx = gateway_subscription_tx.clone();
    runner
        .connection
        .subscription_builder()
        .on_applied(move |_| {
            let _ = gateway_subscription_tx.send(Ok(()));
        })
        .on_error(move |_, error| {
            let _ = gateway_subscription_error_tx.send(Err(error.to_string()));
        })
        .add_query(|query| query.from.backend_case_battles())
        .add_query(|query| query.from.backend_case_site_pins())
        .add_query(|query| query.from.backend_contracts())
        .add_query(|query| query.from.backend_dialogue_sessions())
        .add_query(|query| query.from.backend_dialogue_topic_options())
        .add_query(|query| query.from.backend_investigation_action_outcomes())
        .add_query(|query| query.from.backend_investigation_actions())
        .add_query(|query| query.from.backend_investigation_cases())
        .add_query(|query| query.from.backend_investigation_journal())
        .add_query(|query| query.from.backend_investigation_leads())
        .add_query(|query| query.from.backend_local_problem_trade_effects())
        .add_query(|query| query.from.backend_settlement_npcs())
        .add_query(|query| query.from.party())
        .add_query(|query| query.from.settlement_npc_presence())
        .subscribe();
    gateway_subscription_rx
        .recv_timeout(ACTION_TIMEOUT)
        .map_err(|_| "gateway subscription timed out".to_string())??;
    if !config.use_imported_world {
        let result = reducer_call!(runner, "seed_simulation_world", |cb| runner
            .connection
            .reducers
            .seed_simulation_world_then(config.run_nonce.clone(), cb));
        runner.call(result)?;
    }
    let starting_settlement_id = runner
        .connection
        .db
        .settlement()
        .iter()
        .map(|settlement| settlement.id)
        .min()
        .ok_or("simulation world has no starting settlement")?;
    for (agent, character_id) in runner.character_ids.clone().into_iter().enumerate() {
        let name = format!("sim-{}-{agent}", config.seed);
        let result = reducer_call!(runner, "create_named_character_with_id", |cb| runner
            .connection
            .reducers
            .create_named_character_with_id_then(character_id, name.clone(), cb));
        runner.call(result)?;
        let settlement = starting_settlement_id.clone();
        let profile = runner.profiles[agent].clone();
        let attributes = live_attributes(character_id, &profile);
        let skills = live_skills(character_id, &profile);
        let downtime = live_schedule(&profile);
        let personality = live_personality(character_id, &profile.personality);
        let result = reducer_call!(runner, "configure_simulation_character", |cb| runner
            .connection
            .reducers
            .configure_simulation_character_then(
                config.run_nonce.clone(),
                character_id,
                agent as u32,
                settlement.clone(),
                attributes.clone(),
                skills.clone(),
                downtime.clone(),
                personality.clone(),
                cb,
            ));
        runner.call(result)?;
        let fixture_item = runner
            .connection
            .db
            .inventory_item()
            .iter()
            .find(|row| {
                row.character_id == character_id
                    && runner
                        .connection
                        .db
                        .item()
                        .iter()
                        .find(|item| item.id == row.item_id)
                        .is_some_and(|item| {
                            matches!(
                                item.kind,
                                ItemKind::Weapon | ItemKind::Armor | ItemKind::Shield
                            )
                        })
            })
            .ok_or("simulation character has no durable fixture item")?;
        let result = reducer_call!(runner, "seed_simulation_equipment_damage", |cb| runner
            .connection
            .reducers
            .seed_simulation_equipment_damage_then(
                config.run_nonce.clone(),
                character_id,
                fixture_item.id,
                cb,
            ));
        runner.call(result)?;
        if agent == 0 {
            let result = reducer_call!(runner, "seed_simulation_disease", |cb| runner
                .connection
                .reducers
                .seed_simulation_disease_then(config.run_nonce.clone(), character_id, cb));
            runner.call(result)?;
        }
        runner.metrics.parties_formed += 1;
        runner.event(agent as u32, CoreLoopEventKind::FormParty, name);
    }

    // Joining is demonstrated with the same ordinary request/accept reducers as players.
    // The bounded bootstrap co-locates fresh sim-* solo parties before they use
    // the ordinary request/accept reducers to merge.
    let settlement = runner
        .party_for(runner.character_ids[0])?
        .current_settlement_id
        .clone()
        .ok_or("leader not at settlement")?;
    let mut party_ids = Vec::new();
    for first in (0..runner.character_ids.len()).step_by(config.party_size as usize) {
        let leader = runner.character_ids[first];
        let leader_party = runner.party_for(leader)?;
        party_ids.push(leader_party.id.clone());
        let end = (first + config.party_size as usize).min(runner.character_ids.len());
        for agent in first + 1..end {
            let member = runner.character_ids[agent];
            let result = reducer_call!(runner, "request_general_party_join", |cb| runner
                .connection
                .reducers
                .request_general_party_join_then(member, leader_party.id.clone(), cb));
            runner.call(result)?;
            runner.metrics.joins_requested += 1;
            runner.event(
                agent as u32,
                CoreLoopEventKind::RequestJoin,
                leader_party.id.clone(),
            );
            let request = runner
                .connection
                .db
                .party_join_request()
                .iter()
                .find(|row| row.character_id == member && row.party_id == leader_party.id)
                .ok_or("join reducer completed without a coherent request row")?;
            let result = reducer_call!(runner, "accept_party_join_request", |cb| runner
                .connection
                .reducers
                .accept_party_join_request_then(leader, request.id, cb));
            runner.call(result)?;
            runner.metrics.joins_accepted += 1;
            runner.event(
                agent as u32,
                CoreLoopEventKind::AcceptJoin,
                leader_party.id.clone(),
            );
        }
    }
    let result = reducer_call!(runner, "ensure_settlement_activity", |cb| runner
        .connection
        .reducers
        .ensure_settlement_activity_then(settlement.clone(), cb));
    runner.call(result)?;

    let duration_minutes = u64::from(config.duration_days) * 1_440;
    for cycle in 0..config.cycles {
        let mut active = false;
        let mut held = false;
        for party_id in &party_ids {
            runner.observe_deaths();
            runner.observe_external_generated_closures();
            let party_time_before = runner.public_party_elapsed_max(party_id);
            let Some((pre_recovery_leader, _)) = runner.current_leader(party_id) else {
                continue;
            };
            let recovery_started_in_budget = runner
                .connection
                .db
                .character_time()
                .iter()
                .find(|row| row.character_id == pre_recovery_leader)
                .ok_or("missing pre-recovery leader clock")?
                .minutes
                < duration_minutes;
            if !recovery_started_in_budget {
                continue;
            }
            let recovery_outcome = runner.recover_or_evacuate_off_settlement(party_id, cycle)?;
            match recovery_outcome {
                ExpeditionRecoveryOutcome::None | ExpeditionRecoveryOutcome::Resumed => {}
                ExpeditionRecoveryOutcome::Evacuated => {
                    active = true;
                    let result = reducer_call!(
                        runner,
                        "ensure_settlement_activity_after_evacuation",
                        |cb| {
                            runner
                                .connection
                                .reducers
                                .ensure_settlement_activity_then(settlement.clone(), cb)
                        }
                    );
                    runner.call(result)?;
                    continue;
                }
                ExpeditionRecoveryOutcome::Held => {
                    held = true;
                    if runner.public_party_elapsed_max(party_id) > party_time_before {
                        active = true;
                    }
                    continue;
                }
            }
            let Some((leader, _)) = runner.current_leader(party_id) else {
                continue;
            };
            let elapsed = runner
                .connection
                .db
                .character_time()
                .iter()
                .find(|row| row.character_id == leader)
                .ok_or("missing leader clock")?
                .minutes;
            if elapsed >= duration_minutes
                && !(recovery_outcome == ExpeditionRecoveryOutcome::Resumed
                    && recovery_started_in_budget)
            {
                continue;
            }
            match runner.continue_public_active_journey(party_id)? {
                None | Some(JourneyTravelOutcome::Completed) => {}
                Some(
                    JourneyTravelOutcome::HeldNoActionableActor
                    | JourneyTravelOutcome::HeldForRecovery,
                ) => {
                    held = true;
                    if runner.public_party_elapsed_max(party_id) > party_time_before {
                        active = true;
                    }
                    continue;
                }
            }
            let Some((leader, leader_agent)) = runner.current_leader(party_id) else {
                continue;
            };
            active = true;
            let profile = runner.profiles[leader_agent as usize].clone();
            let mixed = config.seed
                ^ u64::from(leader_agent).wrapping_mul(0x9e37_79b9_7f4a_7c15)
                ^ u64::from(cycle).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            let selector = (mixed >> 11) as f64 / ((1_u64 << 53) as f64);
            let quest_propensity = profile.activity_vs_quest_propensity;
            let wants_quest = selector < f64::from(quest_propensity);
            let party = runner.party_for(leader)?;
            let settlement_id = party.current_settlement_id.as_deref();
            let offered_contracts = settlement_id.map_or(0, |settlement_id| {
                runner
                    .connection
                    .db
                    .backend_contracts()
                    .iter()
                    .filter(|contract| {
                        contract.settlement_id == settlement_id
                            && contract.status == ContractStatus::Offered
                    })
                    .count()
            });
            let open_generated_cases = runner.owned_open_generated_cases(leader);
            for (case_id, title) in &open_generated_cases {
                runner.observe_generated_case_intake(
                    leader_agent,
                    leader,
                    case_id,
                    title,
                    "owner_projection_continuation",
                );
            }
            let projected_investigation_actions = runner
                .connection
                .db
                .backend_investigation_actions()
                .iter()
                .filter(|row| {
                    row.owner_character_id == leader
                        && open_generated_cases
                            .iter()
                            .any(|(case_id, _)| case_id == &row.case_id)
                })
                .count();
            let direct_quest_chosen =
                wants_quest && runner.choose_quest(&party, &profile).is_some();
            let active_direct_contract = runner.active_direct_contract(&party);
            let quest_path = if active_direct_contract.is_some() {
                "direct_contract_continuation"
            } else if !open_generated_cases.is_empty() {
                "generated_open_case"
            } else if direct_quest_chosen {
                "direct_contract"
            } else if wants_quest {
                "generated_discovery"
            } else {
                "activity"
            };
            let quest_selected = quest_path != "activity";
            runner.event(
                leader_agent,
                CoreLoopEventKind::QuestDecision,
                format_quest_decision_detail(
                    cycle,
                    wants_quest,
                    selector,
                    quest_propensity,
                    settlement_id,
                    offered_contracts,
                    open_generated_cases.len(),
                    projected_investigation_actions,
                    quest_path,
                    wants_quest,
                    quest_selected,
                    if quest_selected {
                        "none"
                    } else {
                        "policy_prefers_activity"
                    },
                ),
            );
            match quest_path {
                "generated_open_case" => {
                    let (case_id, title) = open_generated_cases[0].clone();
                    let progressed = runner.advance_generated_case(
                        party_id,
                        leader,
                        leader_agent,
                        cycle,
                        &case_id,
                        &title,
                    )?;
                    if !progressed && runner.party_for(leader)?.current_settlement_id.is_some() {
                        runner.settlement_activity_day(leader_agent)?;
                    }
                }
                "direct_contract" | "direct_contract_continuation" => {
                    runner.cycle(party_id, cycle)?
                }
                "generated_discovery" => {
                    let discovery = runner.discover_generated_case(leader, leader_agent, cycle)?;
                    if discovery.case_discovered() {
                        let Some((case_id, title)) =
                            runner.owned_open_generated_cases(leader).into_iter().next()
                        else {
                            continue;
                        };
                        let progressed = runner.advance_generated_case(
                            party_id,
                            leader,
                            leader_agent,
                            cycle,
                            &case_id,
                            &title,
                        )?;
                        if !progressed && runner.party_for(leader)?.current_settlement_id.is_some()
                        {
                            runner.settlement_activity_day(leader_agent)?;
                        }
                    } else {
                        runner.settlement_activity_day(leader_agent)?;
                    }
                }
                _ => runner.settlement_activity_day(leader_agent)?,
            }
            let result = reducer_call!(runner, "ensure_settlement_activity", |cb| runner
                .connection
                .reducers
                .ensure_settlement_activity_then(settlement.clone(), cb));
            runner.call(result)?;
        }
        if active {
            let result = reducer_call!(runner, "advance_simulation_world_time", |cb| runner
                .connection
                .reducers
                .advance_simulation_world_time_then(
                    config.run_nonce.clone(),
                    adventuresim_core::strategic_time::MINUTES_PER_DAY,
                    cb,
                ));
            runner.call(result)?;
            let result = reducer_call!(runner, "ensure_settlement_activity", |cb| runner
                .connection
                .reducers
                .ensure_settlement_activity_then(settlement.clone(), cb));
            runner.call(result)?;
        }
        if !active && held {
            break;
        }
        if !active {
            break;
        }
    }
    // One final bounded rescue pass runs even when the scenario duration or
    // cycle budget ended immediately after an off-settlement injury.
    for party_id in &party_ids {
        runner.observe_deaths();
        runner.recover_or_evacuate_off_settlement(party_id, config.cycles)?;
    }
    // Bounded final settlement cleanup prevents a duration cutoff from
    // stranding medical care or completed smith orders.
    for agent in 0..runner.character_ids.len() as u32 {
        let character_id = runner.character_ids[agent as usize];
        let at_settlement = runner
            .connection
            .db
            .character()
            .iter()
            .find(|row| row.id == character_id)
            .is_some_and(|row| row.alive && row.current_settlement_id.is_some());
        if at_settlement && runner.ensure_medically_safe(agent)? {
            runner.maintain_equipment(agent)?;
        }
    }
    runner.observe_deaths();

    let final_agents = runner
        .character_ids
        .iter()
        .enumerate()
        .map(|(agent, character_id)| {
            let character = runner
                .connection
                .db
                .character()
                .iter()
                .find(|row| row.id == *character_id)
                .ok_or("missing final character")?;
            let equipped_ids = runner
                .connection
                .db
                .character_equipped_item()
                .iter()
                .filter(|row| row.character_id == *character_id)
                .map(|row| row.inventory_item_id)
                .collect::<HashSet<_>>();
            let mut equipment_item_ids: Vec<String> = runner
                .connection
                .db
                .inventory_item()
                .iter()
                .filter(|row| row.character_id == *character_id)
                .filter(|row| equipped_ids.contains(&row.id))
                .map(|row| row.item_id)
                .collect();
            equipment_item_ids.sort();
            let capability = runner
                .connection
                .db
                .character_capability()
                .iter()
                .find(|row| row.character_id == *character_id)
                .ok_or("missing final capability")?;
            let condition = runner
                .connection
                .db
                .character_strategic_condition()
                .iter()
                .find(|row| row.character_id == *character_id)
                .ok_or("missing final condition")?;
            let elapsed_minutes = runner
                .connection
                .db
                .character_time()
                .iter()
                .find(|row| row.character_id == *character_id)
                .ok_or("missing final clock")?
                .minutes;
            let personal_gold_coin: u64 = runner
                .connection
                .db
                .inventory_item()
                .iter()
                .filter(|row| row.character_id == *character_id && is_currency_id(&row.item_id))
                .map(|row| u64::from(row.quantity))
                .sum();
            let worst_equipment_condition = equipped_ids
                .into_iter()
                .filter_map(|id| {
                    runner
                        .connection
                        .db
                        .item_condition()
                        .iter()
                        .find(|row| row.inventory_item_id == id)
                })
                .map(|row| {
                    1.0 - (row.tier_1 + row.tier_2 + row.tier_3 + row.tier_4 + row.tier_5)
                        .clamp(0.0, 1.0)
                })
                .fold(1.0_f32, f32::min);
            let outstanding_repair_orders = runner
                .connection
                .db
                .repair_order()
                .iter()
                .filter(|row| row.owner_character_id == *character_id)
                .count() as u32;
            let party_id = character.party_id.clone().ok_or("missing final party")?;
            let party_treasury = runner
                .connection
                .db
                .party_inventory_item()
                .iter()
                .filter(|row| row.party_id == party_id && is_currency_id(&row.item_id))
                .map(|row| u64::from(row.quantity))
                .sum();
            let party_stake = runner
                .connection
                .db
                .party_stake()
                .iter()
                .find(|row| row.party_id == party_id && row.character_id == *character_id)
                .map_or(0, |row| row.value);
            let public = runner
                .public_failure_agent(agent as u32, *character_id)
                .ok_or("missing final public diagnostic state")?;
            Ok(FinalAgentState {
                agent_id: agent as u32,
                character_id: *character_id,
                gold: personal_gold_coin.min(u64::from(u32::MAX)) as u32,
                equipment_item_ids,
                capability_summary: format!(
                    "melee={};ranged={};heavy={};athletics={:.2};endurance={:.2}",
                    capability.melee,
                    capability.ranged,
                    capability.heavy,
                    capability.athletics,
                    capability.endurance
                ),
                condition_status: condition.status,
                worst_equipment_condition,
                outstanding_repair_orders,
                alive: character.alive,
                elapsed_minutes,
                personal_gold_coin,
                party_treasury,
                party_stake,
                hunger: public.hunger,
                thirst: public.thirst,
                food_days: public.food_days,
                water_days: public.water_days,
                visible_food_kcal: public.visible_food_kcal,
                visible_water_ml: public.visible_water_ml,
                settlement_id: public.settlement_id,
                current_case_site_id: public.current_case_site_id,
                journey_destination: public.journey_destination,
                symptomatic: public.symptomatic,
                critical: public.critical,
                settlement_services: public.settlement_services,
                visible_herbalist_quote: public.visible_herbalist_quote,
                visible_inn_full_board_cost: public.visible_inn_full_board_cost,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let elapsed_game_minutes = final_agents
        .iter()
        .map(|agent| agent.elapsed_minutes)
        .max()
        .unwrap_or(0);
    let total_event_count = runner.sequence;
    let trace_truncated = total_event_count > runner.trace.len() as u64;
    Ok(CoreLoopReport {
        backend_kind: "spacetimedb_authoritative_core_loop".into(),
        seed: config.seed,
        server_origin: config.host.clone(),
        database: config.database,
        run_nonce: config.run_nonce,
        deployment_identity_note: "server origin, database, and claimed run nonce identify this deployment; the SDK does not expose a deployed module binary digest".into(),
        world_artifact_id: world_import.as_ref().map(|import| import.artifact_id.clone()),
        world_manifest_digest: world_import
            .as_ref()
            .map(|import| import.manifest_digest.clone()),
        starting_settlement_id,
        profiles: runner.profiles,
        metrics: runner.metrics,
        trace: runner.trace,
        trace_truncated,
        total_event_count,
        final_agents,
        elapsed_game_minutes,
        policy_seed_note: "seed controls profiles and policy choices only; authoritative autoresolve seeds are server RNG values recorded in the trace".into(),
    })
}
