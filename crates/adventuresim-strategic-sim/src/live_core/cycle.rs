impl LiveRunner {
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
            let result = reducer_call!(self, "unsafe_contract_retreat_to_settlement", |cb| self
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
        let result = reducer_call!(self, "abandon_unsafe_active_contract", |cb| self
            .connection
            .reducers
            .abandon_contract_then(leader, quest.id.clone(), cb));
        self.call(result)?;
        self.metrics.direct_contracts_safely_abandoned += 1;
        self.event(
            leader_agent,
            CoreLoopEventKind::AbandonQuest,
            format!(
                "quest={};reason=active_contract_public_matchup_unsafe",
                bounded_event_field(&quest.id)
            ),
        );
        Ok(())
    }

    pub(super) fn cycle(&mut self, party_id: &str, cycle: u32) -> Result<(), String> {
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
            .or_else(|| self.choose_quest(&party, &self.profiles[leader_agent as usize]))
            .ok_or("no suitable available or accepted quest")?;
        if quest.status == ContractStatus::ReadyToReport {
            return self.turn_in_ready_direct_contract(party_id, leader, leader_agent, &quest);
        }
        let assessment = self.public_party_contract_assessment(party_id, &quest);
        if !assessment.eligible {
            if resuming_contract {
                self.abandon_unsafe_active_contract(
                    party_id,
                    quest_owner,
                    &quest,
                    assessment,
                )?;
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
            if let TravelProvisionDecision::Deferred(reason) = self.provision_case_site_journey(
                party_id,
                leader,
                leader_agent,
                &quest.case_id,
                quest.distance_m,
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
            self.metrics.quests_attempted += 1;
            self.metrics.direct_contracts_attempted += 1;
            let result = reducer_call!(self, "interact_accept_contract", |cb| self
                .connection
                .reducers
                .simulate_contract_issuer_interaction_then(
                    leader,
                    quest.id.clone(),
                    ContractInteractionStage::Accept,
                    cb,
                ));
            self.call(result)?;
            let result = reducer_call!(self, "accept_quest", |cb| self
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
            .min_by_key(|site| (site.distance_m, site.case_site_id.clone()))
            .ok_or("accepted quest did not disclose an exact case site")?;
        let already_at_case_site = party
            .current_case_site_id
            .as_ref()
            .is_some_and(|site| site.value == case_site.case_site_id);
        if !already_at_case_site {
            if matches!(
                self.provision_case_site_journey(
                    party_id,
                    leader,
                    leader_agent,
                    &quest.case_id,
                    case_site.distance_m,
                )?,
                TravelProvisionDecision::Deferred(_)
            ) {
                return Err(
                    "accepted contract provisioning projection changed after disclosure".into(),
                );
            }
            if !resuming_contract {
                self.event(
                    leader_agent,
                    CoreLoopEventKind::AcceptContract,
                    format!(
                        "cycle={cycle};quest={};title={};difficulty={};opposition={} {};distance_m={}",
                        quest.id,
                        quest.title,
                        quest.difficulty,
                        quest.opposition_count_wording,
                        quest.opposition_wording,
                        case_site.distance_m
                    ),
                );
            } else {
                self.event(
                    leader_agent,
                    CoreLoopEventKind::Travel,
                    format!(
                        "direct_contract={};continuation=outbound;case_site={}",
                        bounded_event_field(&quest.id),
                        bounded_event_field(&case_site.case_site_id),
                    ),
                );
            }

            let outbound_before = self.expedition_member_observations(party_id)?;
            let outbound_supplies_before = self.expedition_supplies(party_id);
            let result = reducer_call!(self, "travel_to_case_site", |cb| self
                .connection
                .reducers
                .travel_to_case_site_then(
                    leader,
                    CaseSiteId {
                        value: case_site.case_site_id.clone(),
                    },
                    cb,
                ));
            self.call(result)?;
            let outbound_after = self.expedition_member_observations(party_id)?;
            let outbound_supplies_after = self.expedition_supplies(party_id);
            self.emit_expedition_diagnostics(
                party_id,
                "journey_leg",
                "travel_to_case_site",
                if outbound_after.iter().any(expedition_member_needs_recovery) {
                    "quest_suppressed_member_not_ready_after_outbound_leg"
                } else {
                    "quest_leg_outbound_all_members_ready"
                },
                &outbound_before,
                &outbound_after,
                outbound_supplies_before,
                outbound_supplies_after,
            );
            self.event(
                leader_agent,
                CoreLoopEventKind::Travel,
                format!("outbound={}", case_site.case_site_id),
            );
            if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
                return Ok(());
            }
        } else {
            self.event(
                leader_agent,
                CoreLoopEventKind::Travel,
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
            let result = reducer_call!(self, "illness_retreat_to_settlement", |cb| self
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
            let result = reducer_call!(self, "abandon_unsafe_quest", |cb| self
                .connection
                .reducers
                .abandon_contract_then(leader, quest.id.clone(), cb));
            self.call(result)?;
            self.metrics.direct_contracts_safely_abandoned += 1;
            self.event(leader_agent, CoreLoopEventKind::AbandonQuest, quest.id);
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
            case_site.case_site_id
        );
        let battle_id = format!("battle:{mission_id}");
        let result = reducer_call!(self, "autoresolve_mission", |cb| self
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
        let report = self.connection.db.autoresolve_report().iter()
            .find(|report| report.battle_id == battle_id)
            .ok_or("autoresolve completed without a report")?;
        let victory = self.connection.db.battle_result().iter()
            .any(|result| result.battle_id == battle_id);
        let winning_battle_id = victory.then_some(battle_id);
        self.event(
            leader_agent,
            if victory { CoreLoopEventKind::AutoresolveVictory } else { CoreLoopEventKind::AutoresolveDefeat },
            format!("seed={};rounds={};summary={};log={:?}", report.seed, report.rounds, report.summary, report.log),
        );
        if !victory {
            self.metrics.defeats += 1;
        }
        if !victory {
            let result = reducer_call!(self, "defeat_retreat_to_settlement", |cb| self
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
            let result = reducer_call!(self, "abandon_defeated_quest", |cb| self
                .connection
                .reducers
                .abandon_contract_then(leader, quest.id.clone(), cb));
            self.call(result)?;
            self.metrics.direct_contracts_safely_abandoned += 1;
            self.event(
                leader_agent,
                CoreLoopEventKind::AbandonQuest,
                format!("quest={};reason=unchanged_defeated_threat", bounded_event_field(&quest.id)),
            );
            let result = reducer_call!(self, "replenish_quests_after_abandon", |cb| self
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
        let result = reducer_call!(self, "store_battle_loot", |cb| self
            .connection
            .reducers
            .store_battle_loot_then(leader, winning_battle_id, vec![], vec![], cb,));
        self.call(result)?;
        self.event(
            leader_agent,
            CoreLoopEventKind::StoreLoot,
            format!("stacks={}", loot.len()),
        );

        let result = reducer_call!(self, "return_to_settlement", |cb| self
            .connection
            .reducers
            .travel_to_settlement_then(leader, quest.settlement_id.clone(), cb));
        self.call(result)?;
        self.event(
            leader_agent,
            CoreLoopEventKind::Travel,
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
            let result = reducer_call!(self, "liquidate_party_inventory", |cb| self
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
        let (party_load_kg, party_capacity_kg, _) =
            self.public_party_load_and_capacity(&party_id);
        let mut candidates = definitions
            .iter()
            .filter_map(|candidate| {
                let utility = equipment_utility(&profile, candidate)?;
                let armor = matches!(candidate.kind, ItemKind::Armor | ItemKind::Clothing);
                let current = equipped_definitions
                    .iter()
                    .filter(|(item, _)| {
                        if candidate.melee || candidate.ranged {
                            item.melee || item.ranged
                        } else {
                            armor
                                && matches!(item.kind, ItemKind::Armor | ItemKind::Clothing)
                                && item.slot == candidate.slot
                        }
                    })
                    .filter_map(|(item, condition)| {
                        equipment_utility(&profile, item).map(|utility| utility * *condition)
                    })
                    .fold(0.0, f32::max);
                let (service_id, provider_id, quoted_cost) = self
                    .public_equipment_storefront_offer(character_id, settlement, candidate)?;
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
        if !self.withdraw_stake_for_personal_purchase(
            character_id,
            &party_id,
            earned_shortfall,
        )? {
            return Ok(());
        }
        let purse_before_trade = self.personal_gold(character_id);
        if purse_before_trade.saturating_sub(medical_reserve) < quoted_cost {
            return Ok(());
        }
        let result = reducer_call!(self, "finalize_storefront_trade", |cb| self
            .connection
            .reducers
            .finalize_storefront_trade_then(
                character_id,
                settlement.to_string(),
                service_id.clone(),
                provider_id,
                vec![candidate.id.clone()],
                vec![1],
                vec![],
                vec![],
                false,
                cb,
            ));
        self.call(result)?;
        let actual_cost = purse_before_trade.saturating_sub(self.personal_gold(character_id));
        self.metrics.equipment_purchases += 1;
        self.event(
            agent,
            CoreLoopEventKind::Purchase,
            format!(
                "item={};storefront={service_id};provider={provider_id};upper_bound_quote={quoted_cost};actual_cost={actual_cost};earned_shortfall={earned_shortfall};medical_reserve={medical_reserve};utility_gain={improvement:.3}",
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
                ItemSlot::LeftHolding
            } else if !held
                .iter()
                .any(|row| row.location == Some(EquipmentLocation::RightHand))
            {
                ItemSlot::RightHolding
            } else {
                if let Some(displaced) = held
                    .iter()
                    .find(|row| row.location == Some(EquipmentLocation::RightHand))
                    .map(|row| row.inventory_item_id)
                {
                    let result = reducer_call!(self, "unequip_upgrade_conflict", |cb| self
                        .connection
                        .reducers
                        .equip_item_then(character_id, displaced, ItemSlot::None, cb));
                    self.call(result)?;
                }
                ItemSlot::RightHolding
            }
        } else {
            candidate.slot
        };
        let wearable = !candidate.equipment_placements.is_empty();
        let placement_index = if wearable {
            let (placement_index, placement) = candidate
                .equipment_placements
                .iter()
                .enumerate()
                .find(|(_, placement)| {
                    placement.parents.is_empty()
                        && placement.occupancy.iter().any(|requirement| {
                            root_requirement_matches_slot(requirement, destination)
                        })
                })
                .ok_or("wearable upgrade lacks a compatible authored root placement")?;
            let conflicts = self
                .connection
                .db
                .equipment_occupancy()
                .iter()
                .filter(|row| {
                    row.character_id == character_id
                        && placement.occupancy.iter().any(|requirement| {
                            row.location == Some(requirement.location)
                                && row.channel == requirement.channel
                                && row.order == requirement.order
                        })
                })
                .map(|row| row.inventory_item_id)
                .collect::<HashSet<_>>();
            for displaced in conflicts {
                let result = reducer_call!(self, "unequip_wearable_upgrade_conflict", |cb| self
                    .connection
                    .reducers
                    .equip_item_then(character_id, displaced, ItemSlot::None, cb));
                self.call(result)?;
            }
            Some(placement_index as u16)
        } else {
            None
        };
        let result = if wearable {
            reducer_call!(self, "equip_item_at_placement", |cb| self
                .connection
                .reducers
                .equip_item_at_placement_then(
                    character_id,
                    inventory.id,
                    placement_index.expect("wearable placement index"),
                    cb
                ))
        } else {
            reducer_call!(self, "equip_item", |cb| self
                .connection
                .reducers
                .equip_item_then(character_id, inventory.id, destination, cb))
        };
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
        self.event(agent, CoreLoopEventKind::Equip, candidate.id);
        Ok(())
    }
}
