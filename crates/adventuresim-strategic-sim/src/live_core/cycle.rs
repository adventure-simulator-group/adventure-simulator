impl LiveRunner {
    pub(super) fn public_contract_issuer_available(
        &self,
        character_id: u64,
        quest: &BackendContract,
    ) -> bool {
        self.visible_npc_candidates(character_id, None, None)
            .into_iter()
            .any(|candidate| candidate.resident_character_id == quest.issuer_resident_character_id)
    }

    pub(super) fn defer_unavailable_contract_issuer(
        &mut self,
        party_id: &str,
        leader_agent: u32,
        quest: &BackendContract,
        provenance: &str,
    ) -> Result<(), String> {
        self.event(
            leader_agent,
            CoreLoopEventKind::QuestSuppressed,
            format!(
                "quest={};reason=contract_issuer_unavailable;provenance={}",
                bounded_event_field(&quest.id),
                bounded_event_field(provenance),
            ),
        );
        if self.party_by_id(party_id)?.current_settlement_id.as_deref()
            == Some(quest.settlement_id.as_str())
        {
            self.settlement_activity_day(leader_agent)?;
        }
        Ok(())
    }

    fn abandon_unsafe_active_contract(
        &mut self,
        party_id: &str,
        quest_owner: u64,
        quest: &BackendContract,
        assessment: PublicContractAssessment,
    ) -> Result<(), String> {
        let agent = self.current_leader(party_id).map_or(0, |(_, agent)| agent);
        self.event(
            agent,
            CoreLoopEventKind::QuestSuppressed,
            format!(
                "quest={};reason=active_contract_public_matchup_unsafe;assessment={};enemy_count={};ready_combatants={};party_power_milli={};enemy_power_milli={}",
                bounded_event_field(&quest.id),
                assessment.reason,
                assessment.enemy_count.map_or_else(|| "unknown".into(), |count| count.to_string()),
                assessment.ready_combatants,
                assessment.party_power_milli,
                assessment.enemy_power_milli,
            ),
        );
        if self.party_by_id(party_id)?.current_settlement_id.is_none() {
            let Some((leader, _)) = self.current_leader(party_id) else {
                return Ok(());
            };
            let result = reducer_call!(self, ReducerOperation::UnsafeContractRetreatToSettlement, |cb| self
                .connection
                .reducers
                .travel_to_settlement_then(leader, quest.settlement_id.clone(), cb));
            self.call(result)?;
            if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
                return Ok(());
            }
        }
        self.observe_deaths();
        let Some((leader, leader_agent)) = self.current_leader(party_id) else {
            return Ok(());
        };
        if leader != quest_owner {
            return Ok(());
        }
        let result = reducer_call!(self, ReducerOperation::AbandonUnsafeActiveContract, |cb| self
            .connection
            .reducers
            .abandon_contract_then(leader, quest.id.clone(), cb));
        self.call(result)?;
        self.metrics.direct_contracts_safely_abandoned += 1;
        self.direct_contract_event(
            leader_agent,
            CoreLoopEventKind::AbandonQuest,
            party_id,
            &quest.id,
            format!(
                "quest={};reason=active_contract_public_matchup_unsafe",
                bounded_event_field(&quest.id)
            ),
        );
        Ok(())
    }

    pub(super) fn cycle(
        &mut self,
        party_id: &str,
        cycle: u32,
        reserved_contract_id: Option<&str>,
    ) -> Result<(), String> {
        let Some((quest_owner, _)) = self.current_leader(party_id) else {
            self.observe_deaths();
            return Ok(());
        };
        let party_agents = self.party_agents(quest_owner)?;
        for &agent in &party_agents {
            if !self.ensure_medically_safe(agent)? {
                self.metrics.quests_suppressed_for_health += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!("cycle={cycle}"),
                );
                return Ok(());
            }
            self.maintain_equipment(agent)?;
        }
        let Some((mut leader_agent, mut party)) =
            self.refreshed_safe_party_for_owner(party_id, quest_owner)?
        else {
            return Ok(());
        };
        let mut leader = quest_owner;
        if party.current_settlement_id.is_some() {
            if let DepartureReadiness::Deferred(reason) =
                self.prepare_party_for_departure(party_id, leader, leader_agent)?
            {
                self.event(
                    leader_agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!("cycle={cycle};reason={reason};phase=survival_readiness"),
                );
                self.settlement_activity_day(leader_agent)?;
                return Ok(());
            }
            let Some((refreshed_agent, refreshed_party)) =
                self.refreshed_safe_party_for_owner(party_id, quest_owner)?
            else {
                return Ok(());
            };
            leader_agent = refreshed_agent;
            party = refreshed_party;
        }
        let active_contract = self.active_direct_contract(&party);
        let resuming_contract = active_contract.is_some();
        let quest = active_contract
            .or_else(|| {
                reserved_contract_id.and_then(|contract_id| {
                    self.connection
                        .db
                        .backend_contracts()
                        .iter()
                        .find(|contract| {
                            contract.id == contract_id
                                && contract.settlement_id
                                    == party.current_settlement_id.clone().unwrap_or_default()
                                && contract.status == ContractStatus::Offered
                        })
                })
            })
            .or_else(|| {
                reserved_contract_id
                    .is_none()
                    .then(|| self.choose_quest(&party, &self.profiles[leader_agent as usize]))
                    .flatten()
            })
            .ok_or("no suitable available or accepted quest")?;
        if quest.status == ContractStatus::ReadyToReport {
            return self.turn_in_ready_direct_contract(party_id, leader, leader_agent, &quest);
        }
        let assessment = self.public_party_contract_assessment(party_id, &quest);
        if !assessment.eligible {
            if resuming_contract {
                self.abandon_unsafe_active_contract(party_id, quest_owner, &quest, assessment)?;
            } else {
                self.event(
                    leader_agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!(
                        "quest={};reason=no_safe_contract;assessment={}",
                        bounded_event_field(&quest.id),
                        assessment.reason,
                    ),
                );
            }
            return Ok(());
        }
        if !resuming_contract {
            if !self.public_contract_issuer_available(leader, &quest) {
                return self.defer_unavailable_contract_issuer(
                    party_id,
                    leader_agent,
                    &quest,
                    "public_presence_projection",
                );
            }
            if let TravelProvisionDecision::Deferred(reason) = self.provision_case_site_journey(
                party_id,
                leader,
                leader_agent,
                &quest.case_id,
                quest.distance_m,
                0,
            )? {
                self.event(
                    leader_agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!(
                        "quest={};acceptance_deferred={reason}",
                        bounded_event_field(&quest.id)
                    ),
                );
                self.settlement_activity_day(leader_agent)?;
                return Ok(());
            }
            if !self.public_contract_issuer_available(leader, &quest) {
                return self.defer_unavailable_contract_issuer(
                    party_id,
                    leader_agent,
                    &quest,
                    "public_presence_projection",
                );
            }
            let result = reducer_call!(self, ReducerOperation::InteractAcceptContract, |cb| self
                .connection
                .reducers
                .simulate_contract_issuer_interaction_then(
                    leader,
                    quest.id.clone(),
                    ContractInteractionStage::Accept,
                    cb,
                ));
            if let Err(error) = result {
                if contract_issuer_unavailable_failure(&error) {
                    return self.defer_unavailable_contract_issuer(
                        party_id,
                        leader_agent,
                        &quest,
                        "authoritative_interaction_rejection",
                    );
                }
                return self.call(Err(error));
            }
            self.metrics.quests_attempted += 1;
            self.metrics.direct_contracts_attempted += 1;
            let result = reducer_call!(self, ReducerOperation::AcceptQuest, |cb| self
                .connection
                .reducers
                .accept_contract_then(leader, quest.id.clone(), cb));
            self.call(result)?;
        }
        let case_site = self
            .connection
            .db
            .backend_case_site_pins()
            .iter()
            .filter(|site| site.owner_character_id == leader && site.case_id == quest.case_id)
            .min_by_key(|site| (site.distance_m, site.case_site_id.value.clone()))
            .ok_or("accepted quest did not disclose an exact case site")?;
        let already_at_case_site = party
            .current_case_site_id
            .as_ref()
            .is_some_and(|site| site == &case_site.case_site_id);
        if !already_at_case_site {
            if party.current_settlement_id.is_some() ^ party.current_case_site_id.is_some() {
                match self.validate_case_site_thermal_readiness(party_id, leader_agent, &case_site)
                {
                    DepartureReadiness::Ready => {}
                    DepartureReadiness::ReadyWithItinerary {
                        walking_minutes_per_day,
                        travel_at_night,
                        ..
                    } => self.configure_safe_departure_itinerary(
                        leader,
                        walking_minutes_per_day,
                        travel_at_night,
                        None,
                    )?,
                    DepartureReadiness::WaitForSafeDeparture {
                        reason,
                        wait_minutes,
                        walking_minutes_per_day,
                        travel_at_night,
                        ..
                    } => {
                        if party.current_settlement_id.is_some() {
                            self.wait_for_safe_departure_at_settlement(SettlementDepartureWait {
                                character_id: leader,
                                agent: leader_agent,
                                case_id: &quest.case_id,
                                reason,
                                wait_minutes,
                                walking_minutes_per_day,
                                travel_at_night,
                            })?;
                        }
                        return Ok(());
                    }
                    DepartureReadiness::Deferred(reason) => {
                        self.event(
                            leader_agent,
                            CoreLoopEventKind::QuestSuppressed,
                            format!(
                                "quest={};reason={reason};phase=route_thermal_readiness",
                                bounded_event_field(&quest.id)
                            ),
                        );
                        return Ok(());
                    }
                }
            }
            if matches!(
                self.provision_case_site_journey(
                    party_id,
                    leader,
                    leader_agent,
                    &quest.case_id,
                    case_site.distance_m,
                    0,
                )?,
                TravelProvisionDecision::Deferred(_)
            ) {
                return Err(
                    "accepted contract provisioning projection changed after disclosure".into(),
                );
            }
            if !resuming_contract {
                self.direct_contract_event(
                    leader_agent,
                    CoreLoopEventKind::AcceptContract,
                    party_id,
                    &quest.id,
                    format!(
                        "cycle={cycle};party={};quest={};title={};difficulty={};opposition={} {};distance_m={}",
                        bounded_event_field(party_id),
                        quest.id,
                        quest.title,
                        quest.difficulty,
                        quest.opposition_count_wording,
                        quest.opposition_wording,
                        case_site.distance_m
                    ),
                );
            } else {
                self.direct_contract_event(
                    leader_agent,
                    CoreLoopEventKind::Travel,
                    party_id,
                    &quest.id,
                    format!(
                        "direct_contract={};continuation=outbound;case_site={}",
                        bounded_event_field(&quest.id),
                        bounded_event_field(&case_site.case_site_id.value),
                    ),
                );
            }

            let outbound_before = self.expedition_member_observations(party_id)?;
            let outbound_supplies_before = self.expedition_supplies(party_id);
            let result = reducer_call!(self, ReducerOperation::TravelToCaseSite, |cb| self
                .connection
                .reducers
                .travel_to_case_site_then(
                    leader,
                    case_site.case_site_id.clone(),
                    cb,
                ));
            self.call(result)?;
            let outbound_after = self.expedition_member_observations(party_id)?;
            let outbound_supplies_after = self.expedition_supplies(party_id);
            self.emit_expedition_diagnostics(
                ExpeditionDiagnosticContext {
                    party_id,
                    phase: "journey_leg",
                    action: "travel_to_case_site",
                    reason: if outbound_after.iter().any(expedition_member_needs_recovery) {
                        "quest_suppressed_member_not_ready_after_outbound_leg"
                    } else {
                        "quest_leg_outbound_all_members_ready"
                    },
                },
                ExpeditionObservationChange {
                    members_before: &outbound_before,
                    members_after: &outbound_after,
                    supplies_before: outbound_supplies_before,
                    supplies_after: outbound_supplies_after,
                },
            );
            self.direct_contract_event(
                leader_agent,
                CoreLoopEventKind::Travel,
                party_id,
                &quest.id,
                format!(
                    "party={};direct_contract={};outbound={}",
                    bounded_event_field(party_id),
                    bounded_event_field(&quest.id),
                    bounded_event_field(&case_site.case_site_id.value)
                ),
            );
            if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
                return Ok(());
            }
        } else {
            self.direct_contract_event(
                leader_agent,
                CoreLoopEventKind::Travel,
                party_id,
                &quest.id,
                format!(
                    "direct_contract={};continuation=arrived_case_site",
                    bounded_event_field(&quest.id)
                ),
            );
        }

        // Travel advances every member's disease clock. Re-observe public
        // life/condition state before attempting a living-only combat reducer.
        let unsafe_after_travel = self.unsafe_party_agents(&party_agents);
        if !unsafe_after_travel.is_empty() {
            for &agent in &unsafe_after_travel {
                self.metrics.quests_suppressed_for_health += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!("after_travel;cycle={cycle}"),
                );
            }
            self.observe_deaths();
            let Some((current, _)) = self.current_leader(party_id) else {
                return Ok(());
            };
            leader = current;
            let result = reducer_call!(self, ReducerOperation::IllnessRetreatToSettlement, |cb| self
                .connection
                .reducers
                .travel_to_settlement_then(leader, quest.settlement_id.clone(), cb));
            self.call(result)?;
            if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
                return Ok(());
            }
            for &agent in &party_agents {
                self.ensure_medically_safe(agent)?;
            }
            let Some((current_agent, _)) =
                self.refreshed_safe_party_for_owner(party_id, quest_owner)?
            else {
                return Ok(());
            };
            leader = quest_owner;
            leader_agent = current_agent;
            let result = reducer_call!(self, ReducerOperation::AbandonUnsafeQuest, |cb| self
                .connection
                .reducers
                .abandon_contract_then(leader, quest.id.clone(), cb));
            self.call(result)?;
            self.metrics.direct_contracts_safely_abandoned += 1;
            self.direct_contract_event(
                leader_agent,
                CoreLoopEventKind::AbandonQuest,
                party_id,
                &quest.id,
                quest.id.clone(),
            );
            return Ok(());
        }

        let assessment_after_travel = self.public_party_contract_assessment(party_id, &quest);
        if !assessment_after_travel.eligible {
            self.abandon_unsafe_active_contract(
                party_id,
                quest_owner,
                &quest,
                assessment_after_travel,
            )?;
            return Ok(());
        }

        let mission_id = format!(
            "mission:sim-autoresolve:{party_id}:{}:{cycle}",
            case_site.case_site_id.value
        );
        let battle_id = format!("battle:{mission_id}");
        let result = reducer_call!(self, ReducerOperation::AutoresolveMission, |cb| self
            .connection
            .reducers
            .autoresolve_mission_then(leader, mission_id.clone(), cb));
        self.call(result)?;
        self.observe_deaths();
        let Some((current, current_agent)) = self.current_leader(party_id) else {
            return Ok(());
        };
        leader = current;
        leader_agent = current_agent;
        let report = self
            .connection
            .db
            .autoresolve_report()
            .iter()
            .find(|report| report.battle_id == battle_id)
            .ok_or("autoresolve completed without a report")?;
        let victory = self
            .connection
            .db
            .battle_result()
            .iter()
            .any(|result| result.battle_id == battle_id);
        let winning_battle_id = victory.then_some(battle_id);
        self.direct_contract_event(
            leader_agent,
            if victory {
                CoreLoopEventKind::AutoresolveVictory
            } else {
                CoreLoopEventKind::AutoresolveDefeat
            },
            party_id,
            &quest.id,
            format!(
                "party={};quest={};seed={};rounds={};summary={};log={:?}",
                bounded_event_field(party_id),
                bounded_event_field(&quest.id),
                report.seed,
                report.rounds,
                report.summary,
                report.log
            ),
        );
        if !victory {
            self.metrics.defeats += 1;
        }
        if !victory {
            let result = reducer_call!(self, ReducerOperation::DefeatRetreatToSettlement, |cb| self
                .connection
                .reducers
                .travel_to_settlement_then(leader, quest.settlement_id.clone(), cb));
            self.call(result)?;
            if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
                return Ok(());
            }
            self.observe_deaths();
            let Some((current, _)) = self.current_leader(party_id) else {
                return Ok(());
            };
            leader = current;
            for agent in self.party_agents(leader)? {
                self.ensure_medically_safe(agent)?;
            }
            let Some((current_agent, _)) =
                self.refreshed_safe_party_for_owner(party_id, quest_owner)?
            else {
                return Ok(());
            };
            leader = quest_owner;
            leader_agent = current_agent;
            let result = reducer_call!(self, ReducerOperation::AbandonDefeatedQuest, |cb| self
                .connection
                .reducers
                .abandon_contract_then(leader, quest.id.clone(), cb));
            self.call(result)?;
            self.metrics.direct_contracts_safely_abandoned += 1;
            self.direct_contract_event(
                leader_agent,
                CoreLoopEventKind::AbandonQuest,
                party_id,
                &quest.id,
                format!(
                    "quest={};reason=unchanged_defeated_threat",
                    bounded_event_field(&quest.id)
                ),
            );
            let result = reducer_call!(self, ReducerOperation::ReplenishQuestsAfterAbandon, |cb| self
                .connection
                .reducers
                .ensure_settlement_activity_then(quest.settlement_id.clone(), cb));
            self.call(result)?;
            return Ok(());
        }
        let winning_battle_id = winning_battle_id.ok_or("victory had no battle authority")?;

        let loot: Vec<_> = self
            .connection
            .db
            .battle_loot_item()
            .iter()
            .filter(|row| row.loot_battle_id == winning_battle_id)
            .collect();
        let definitions: HashMap<_, _> = self
            .connection
            .db
            .item()
            .iter()
            .map(|item| (item.id.clone(), item))
            .collect();
        for entry in &loot {
            self.metrics.loot_items = self.metrics.loot_items.saturating_add(entry.quantity);
            self.metrics.loot_value = self.metrics.loot_value.saturating_add(
                u64::from(entry.quantity)
                    * u64::from(
                        definitions
                            .get(&entry.item_id)
                            .and_then(|i| i.base_value)
                            .unwrap_or(0),
                    ),
            );
        }
        let result = reducer_call!(self, ReducerOperation::StoreBattleLoot, |cb| self
            .connection
            .reducers
            .store_battle_loot_then(leader, winning_battle_id, vec![], vec![], cb,));
        self.call(result)?;
        self.event(
            leader_agent,
            CoreLoopEventKind::StoreLoot,
            format!("stacks={}", loot.len()),
        );

        let result = reducer_call!(self, ReducerOperation::ReturnToSettlement, |cb| self
            .connection
            .reducers
            .travel_to_settlement_then(leader, quest.settlement_id.clone(), cb));
        self.call(result)?;
        self.direct_contract_event(
            leader_agent,
            CoreLoopEventKind::Travel,
            party_id,
            &quest.id,
            format!("return={}", quest.settlement_id),
        );
        if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
            return Ok(());
        }
        self.observe_deaths();
        let Some((current, current_agent)) = self.current_leader(party_id) else {
            return Ok(());
        };
        leader = current;
        leader_agent = current_agent;
        self.turn_in_ready_direct_contract(party_id, leader, leader_agent, &quest)?;

        let party = self.party_for(leader)?;
        let sale: Vec<_> = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party.id && !is_currency_id(&row.item_id))
            .collect();
        if !sale.is_empty() {
            let before_coins: u64 = self
                .connection
                .db
                .party_inventory_item()
                .iter()
                .filter(|row| row.party_id == party.id && is_currency_id(&row.item_id))
                .map(|row| u64::from(row.quantity))
                .sum();
            let ids = sale.iter().map(|row| row.id).collect();
            let quantities = sale.iter().map(|row| row.quantity).collect();
            let result = reducer_call!(self, ReducerOperation::LiquidatePartyInventory, |cb| self
                .connection
                .reducers
                .liquidate_party_inventory_then(
                    leader,
                    quest.settlement_id.clone(),
                    ids,
                    quantities,
                    cb
                ));
            self.call(result)?;
            let after_coins: u64 = self
                .connection
                .db
                .party_inventory_item()
                .iter()
                .filter(|row| row.party_id == party.id && is_currency_id(&row.item_id))
                .map(|row| u64::from(row.quantity))
                .sum();
            self.metrics.sale_proceeds += after_coins.saturating_sub(before_coins);
            self.event(
                leader_agent,
                CoreLoopEventKind::Liquidate,
                format!("stacks={}", sale.len()),
            );
        }
        // Spending priority is medical care, then repairs, then upgrades.
        for agent in self.party_agents(leader)? {
            if self.ensure_medically_safe(agent)? {
                self.maintain_equipment(agent)?;
            }
        }
        if let Some((current_agent, _)) =
            self.refreshed_safe_party_for_owner(party_id, quest_owner)?
        {
            let current_leader = self.character_ids[current_agent as usize];
            for party_agent in self.party_agents(current_leader)? {
                self.try_upgrade(party_agent, &quest.settlement_id)?;
            }
        }
        Ok(())
    }

    pub(super) fn try_upgrade(&mut self, agent: u32, settlement: &str) -> Result<(), String> {
        let character_id = self.character_ids[agent as usize];
        let profile = self.profiles[agent as usize].clone();
        let equipped_ids = self
            .connection
            .db
            .character_equipped_item()
            .iter()
            .filter(|row| row.character_id == character_id)
            .map(|row| row.inventory_item_id)
            .collect::<HashSet<_>>();
        let inventories: Vec<_> = self
            .connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| row.character_id == character_id)
            .collect();
        let definitions: Vec<_> = self.connection.db.item().iter().collect();
        let equipped_definitions = inventories
            .iter()
            .filter(|row| equipped_ids.contains(&row.id))
            .filter_map(|row| {
                let definition = definitions.iter().find(|item| item.id == row.item_id)?;
                let condition = self
                    .connection
                    .db
                    .item_condition()
                    .iter()
                    .find(|value| value.inventory_item_id == row.id)
                    .map_or(1.0, |value| {
                        1.0 - (value.tier_1
                            + value.tier_2
                            + value.tier_3
                            + value.tier_4
                            + value.tier_5)
                            .clamp(0.0, 1.0)
                    });
                Some((definition, condition))
            })
            .collect::<Vec<_>>();
        let character = self
            .connection
            .db
            .backend_characters()
            .iter()
            .find(|row| row.id == character_id)
            .ok_or("missing upgrade character")?;
        let party_id = character.party_id.ok_or("missing upgrade party")?;
        let stake = self
            .connection
            .db
            .party_stake()
            .iter()
            .find(|row| row.party_id == party_id && row.character_id == character_id)
            .map_or(0, |row| row.value);
        let (party_load_kg, party_capacity_kg, _) = self.public_party_load_and_capacity(&party_id);
        let mut candidates = definitions
            .iter()
            .filter_map(|candidate| {
                let utility = equipment_utility(&profile, candidate)?;
                let armor = matches!(
                    candidate.kind,
                    PersistedItemKind::Armor | PersistedItemKind::Clothing
                );
                let current = equipped_definitions
                    .iter()
                    .filter(|(item, _)| {
                        if candidate.melee || candidate.ranged {
                            item.melee || item.ranged
                        } else {
                            armor
                                && matches!(
                                    item.kind,
                                    PersistedItemKind::Armor | PersistedItemKind::Clothing
                                )
                                && item.slot == candidate.slot
                        }
                    })
                    .filter_map(|(item, condition)| {
                        equipment_utility(&profile, item).map(|utility| utility * *condition)
                    })
                    .fold(0.0, f32::max);
                let (service_id, provider_id, quoted_cost) =
                    self.public_equipment_storefront_offer(character_id, settlement, candidate)?;
                let medical_reserve = self
                    .observable_medical_reserve(character_id, settlement)
                    .unwrap_or(0);
                let personal_spendable = self
                    .personal_gold(character_id)
                    .saturating_sub(medical_reserve);
                let earned_shortfall = quoted_cost.saturating_sub(personal_spendable);
                let projected_remaining = public_encumbrance_remaining_bps(
                    party_load_kg + candidate.weight.max(0.0),
                    party_capacity_kg,
                );
                (utility > current
                    && earned_shortfall <= stake
                    && projected_remaining >= MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS)
                    .then_some((
                        utility - current,
                        quoted_cost,
                        earned_shortfall,
                        medical_reserve,
                        service_id,
                        provider_id,
                        candidate.clone(),
                    ))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            right
                .0
                .total_cmp(&left.0)
                .then_with(|| left.6.id.cmp(&right.6.id))
        });
        let Some((
            improvement,
            quoted_cost,
            earned_shortfall,
            medical_reserve,
            service_id,
            provider_id,
            candidate,
        )) = candidates.into_iter().next()
        else {
            return Ok(());
        };
        let selected_offer = (service_id.clone(), provider_id, quoted_cost);
        if !storefront_offer_unchanged(
            &selected_offer,
            self.public_equipment_storefront_offer(character_id, settlement, &candidate),
        ) {
            return Ok(());
        }
        let purse_before_trade = self.personal_gold(character_id);
        let stake_before_trade = self
            .connection
            .db
            .party_stake()
            .iter()
            .find(|row| row.party_id == party_id && row.character_id == character_id)
            .map_or(0, |row| row.value);
        let maximum_personal_payment = quoted_cost.saturating_sub(earned_shortfall);
        let result = reducer_call!(self, ReducerOperation::PurchasePersonalStorefrontWithPartyStake,
            |cb| self
                .connection
                .reducers
                .purchase_personal_storefront_with_party_stake_then(
                    character_id,
                    settlement.to_string(),
                    service_id.clone(),
                    provider_id,
                    candidate.id.clone(),
                    1,
                    quoted_cost,
                    maximum_personal_payment,
                    earned_shortfall,
                    cb,
                )
        );
        if let Err(error) = result {
            if merchant_provider_unavailable_failure(&error) {
                return Ok(());
            }
            return self.call(Err(error));
        }
        let purse_after_trade = self.personal_gold(character_id);
        let stake_after_trade = self
            .connection
            .db
            .party_stake()
            .iter()
            .find(|row| row.party_id == party_id && row.character_id == character_id)
            .map_or(0, |row| row.value);
        let personal_spent = purse_before_trade.saturating_sub(purse_after_trade);
        let stake_spent = stake_before_trade.saturating_sub(stake_after_trade);
        let actual_cost = personal_spent.saturating_add(stake_spent);
        self.metrics.earned_gold_withdrawn = self
            .metrics
            .earned_gold_withdrawn
            .saturating_add(stake_spent);
        self.metrics.equipment_purchases += 1;
        self.event(
            agent,
            CoreLoopEventKind::Purchase,
            format!(
                "item={};storefront={service_id};provider={provider_id};upper_bound_quote={quoted_cost};actual_cost={actual_cost};personal_spent={personal_spent};stake_spent={stake_spent};authorized_stake_max={earned_shortfall};medical_reserve={medical_reserve};utility_gain={improvement:.3}",
                candidate.id,
            ),
        );
        let inventory = self
            .connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| row.character_id == character_id && row.item_id == candidate.id)
            .max_by_key(|row| row.id)
            .ok_or("purchase succeeded but inventory was not coherent")?;
        let destination = if candidate.melee || candidate.ranged {
            let held = self
                .connection
                .db
                .equipment_occupancy()
                .iter()
                .filter(|row| {
                    row.character_id == character_id && row.channel == EquipmentChannel::Held
                })
                .collect::<Vec<_>>();
            if !held
                .iter()
                .any(|row| row.location == Some(EquipmentLocation::LeftHand))
            {
                Slot::LeftHolding
            } else {
                Slot::RightHolding
            }
        } else {
            candidate.slot
        };
        let placement_index = candidate
            .equipment_placements
            .iter()
            .position(|placement| {
                placement.parents.is_empty()
                    && placement
                        .occupancy
                        .iter()
                        .any(|requirement| root_requirement_matches_slot(requirement, destination))
            })
            .and_then(|index| u16::try_from(index).ok())
            .ok_or("equipment upgrade lacks a compatible authored root placement")?;
        let result = reducer_call!(self, ReducerOperation::ReplaceItemAtPlacement, |cb| self
            .connection
            .reducers
            .replace_item_at_placement_then(
                character_id,
                inventory.id,
                placement_index,
                Vec::new(),
                cb,
            ));
        self.call(result)?;
        let verified = self
            .connection
            .db
            .character_equipped_item()
            .iter()
            .any(|row| row.character_id == character_id && row.inventory_item_id == inventory.id);
        if !verified {
            return Err("equip reducer completed without the requested equipped state".into());
        }
        self.metrics.equipment_upgrades += 1;
        self.item_event(
            agent,
            CoreLoopEventKind::Equip,
            inventory.id,
            candidate.id,
        );
        Ok(())
    }
}
