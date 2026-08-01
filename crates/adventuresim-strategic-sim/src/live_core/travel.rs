impl LiveRunner {
    pub(super) fn provision_case_site_journey(
        &mut self,
        party_id: &str,
        leader: u64,
        agent: u32,
        finance_key: &str,
        distance_m: u64,
    ) -> Result<TravelProvisionDecision, String> {
        let party = self.party_by_id(party_id)?;
        let Some(settlement_id) = party.current_settlement_id.clone() else {
            return Ok(TravelProvisionDecision::Deferred(
                "provisioning_requires_settlement",
            ));
        };
        let planning_minutes =
            projected_case_site_journey_minutes(distance_m, party.walking_minutes_per_day)
                .ok_or("journey provisioning projection is incoherent")?;
        let settlement = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|row| row.id == settlement_id)
            .ok_or("journey provisioning projection is incoherent")?;
        let ration = self
            .connection
            .db
            .item()
            .iter()
            .find(|row| {
                row.id == adventuresim_core::provisioning::STANDARD_TRAVEL_RATION_ID
                    && row.nutrition_kcal > 0.0
            })
            .ok_or("journey provisioning projection is incoherent")?;
        let waterskin = self
            .connection
            .db
            .item()
            .iter()
            .find(|row| {
                row.id == adventuresim_core::provisioning::STANDARD_WATERSKIN_ID
                    && row.water_capacity_ml > 0
            })
            .ok_or("journey provisioning projection is incoherent")?;
        let members = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|row| row.party_id == party_id)
            .filter_map(|membership| {
                self.connection
                    .db
                    .backend_characters()
                    .iter()
                    .find(|row| row.id == membership.character_id && row.alive)
            })
            .collect::<Vec<_>>();
        if members.is_empty() {
            return Err("journey provisioning projection is incoherent".into());
        }
        let member_ids = members.iter().map(|row| row.id).collect::<HashSet<_>>();
        let personal_inventory = self
            .connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| member_ids.contains(&row.character_id))
            .collect::<Vec<_>>();
        let personal_inventory_ids = personal_inventory
            .iter()
            .map(|row| row.id)
            .collect::<HashSet<_>>();
        let party_inventory = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party_id)
            .collect::<Vec<_>>();
        let party_inventory_ids = party_inventory
            .iter()
            .map(|row| row.id)
            .collect::<HashSet<_>>();
        let food_reserve_kcal = members
            .iter()
            .filter_map(|member| {
                self.connection
                    .db
                    .backend_character_needs()
                    .iter()
                    .find(|row| row.character_id == member.id)
            })
            .map(|needs| needs.food_balance_kcal.max(0.0))
            .sum();
        let water_reserve_ml = members
            .iter()
            .filter_map(|member| {
                self.connection
                    .db
                    .backend_character_needs()
                    .iter()
                    .find(|row| row.character_id == member.id)
            })
            .map(|needs| needs.water_balance_ml.max(0.0))
            .sum();
        let food_lot_kcal = self
            .connection
            .db
            .food_lot()
            .iter()
            .filter(|lot| {
                lot.inventory_item_id
                    .is_some_and(|id| personal_inventory_ids.contains(&id))
                    || lot
                        .party_inventory_item_id
                        .is_some_and(|id| party_inventory_ids.contains(&id))
            })
            .map(|lot| lot.nutrition_kcal.max(0.0))
            .sum();
        let count_item = |item_id: &str| {
            personal_inventory
                .iter()
                .filter(|row| row.item_id == item_id)
                .map(|row| row.quantity)
                .chain(
                    party_inventory
                        .iter()
                        .filter(|row| row.item_id == item_id)
                        .map(|row| row.quantity),
                )
                .sum::<u32>()
        };
        let inputs = adventuresim_core::provisioning::PartyProvisioningInputs {
            planning_minutes,
            target_surplus_days: TRAVEL_PROVISION_RESERVE_DAYS,
            living_members: members.len() as u32,
            food_reserve_kcal,
            food_lot_kcal,
            water_reserve_ml,
            ration_count: count_item(adventuresim_core::provisioning::STANDARD_TRAVEL_RATION_ID),
            waterskin_count: count_item(adventuresim_core::provisioning::STANDARD_WATERSKIN_ID),
            ration_kcal: ration.nutrition_kcal,
            waterskin_capacity_ml: waterskin.water_capacity_ml,
            emergency_alcohol_hydration_ml: 0,
        };
        let forecast = inputs.forecast();
        let rations_to_buy = forecast.rations_to_buy;
        let waterskins_to_buy = forecast.waterskins_to_buy;
        if rations_to_buy == 0 && waterskins_to_buy == 0 {
            self.event(
                agent,
                CoreLoopEventKind::Purchase,
                format!(
                    "journey_provisions=ready;planning_minutes={planning_minutes};reserve_days={TRAVEL_PROVISION_RESERVE_DAYS:.1};food_days={:.2};water_days={:.2}",
                    forecast.food_days, forecast.water_days,
                ),
            );
            return Ok(TravelProvisionDecision::Ready);
        }
        if rations_to_buy > MAX_TRAVEL_PROVISION_UNITS_PER_ITEM
            || waterskins_to_buy > MAX_TRAVEL_PROVISION_UNITS_PER_ITEM
        {
            return Err("journey provisioning projection is incoherent".into());
        }
        // The public storefront contract guarantees both travel staples at
        // every General storefront. Read the generated public settlement
        // projection directly rather than converting it to server schema
        // types or inspecting private merchant authority.
        let general_storefront_visible = settlement.economy.services.iter().any(|service| {
            matches!(
                service,
                SettlementService::Market | SettlementService::GeneralStore
            )
        });
        let ration_stocked = general_storefront_visible;
        let waterskin_stocked = general_storefront_visible;
        if (rations_to_buy > 0 && !ration_stocked) || (waterskins_to_buy > 0 && !waterskin_stocked)
        {
            self.event(
                agent,
                CoreLoopEventKind::QuestSuppressed,
                format!(
                    "reason=journey_essentials_unavailable;planning_minutes={planning_minutes};rations_needed={rations_to_buy};waterskins_needed={waterskins_to_buy}"
                ),
            );
            return Ok(TravelProvisionDecision::Deferred(
                "journey_essentials_unavailable",
            ));
        }
        let party_coin = party_inventory
            .iter()
            .filter(|row| is_currency_id(&row.item_id))
            .map(|row| u64::from(row.quantity))
            .sum::<u64>();
        let upper_bound_cost_for = |buy_bps: i32| -> Option<u64> {
            let unit_price = |item: &Item| {
                let base = adventuresim_core::strategic_economy::merchant_buy_price(
                    item.base_value.unwrap_or(1),
                );
                let language_bound =
                    adventuresim_core::strategic_economy::language_adjusted_buy_price(base, 0.0);
                adventuresim_core::local_problem::adjust_price(language_bound, buy_bps)
            };
            u64::from(unit_price(&ration))
                .checked_mul(u64::from(rations_to_buy))?
                .checked_add(
                    u64::from(unit_price(&waterskin)).checked_mul(u64::from(waterskins_to_buy))?,
                )
        };
        let mut payer_options = members
            .iter()
            .filter(|member| member.current_settlement_id.as_deref() == Some(&settlement_id))
            .filter_map(|member| {
                let payer_minute = self
                    .connection
                    .db
                    .backend_character_times()
                    .iter()
                    .find(|row| row.character_id == member.id)?
                    .minutes;
                let merchant_count = self
                    .connection
                    .db
                    .backend_settlement_residents()
                    .iter()
                    .filter(|npc| {
                        npc.home_settlement_id == settlement_id && npc.service_id == "merchants"
                    })
                    .filter(|npc| {
                        self.connection
                            .db
                            .settlement_resident_presence()
                            .iter()
                            .any(|presence| {
                                presence.character_id == npc.character_id
                                    && presence.settlement_id == settlement_id
                                    && presence.location_id == "market"
                                    && presence.is_default
                                    && npc_is_publicly_present(
                                        presence.start_minute,
                                        presence.end_minute,
                                        payer_minute,
                                    )
                            })
                    })
                    .count();
                if merchant_count != 1 {
                    return None;
                }
                let buy_bps = self
                    .connection
                    .db
                    .backend_local_problem_trade_effects()
                    .iter()
                    .find(|row| {
                        row.character_id == member.id && row.settlement_id == settlement_id
                    })?
                    .buy_bps;
                let upper_bound_cost = upper_bound_cost_for(buy_bps)?;
                let personal = self.personal_gold(member.id);
                let committed_reserve = self
                    .observable_medical_reserve(member.id, &settlement_id)
                    .unwrap_or(0);
                let spendable = party_coin
                    .saturating_add(personal)
                    .saturating_sub(committed_reserve);
                Some((
                    spendable >= upper_bound_cost,
                    spendable,
                    member.id,
                    personal,
                    committed_reserve,
                    upper_bound_cost,
                ))
            })
            .collect::<Vec<_>>();
        payer_options.sort_by_key(|option| (option.0, option.1, option.2));
        let Some((affordable, spendable, payer, personal, committed_reserve, upper_bound_cost)) =
            payer_options.pop()
        else {
            return Ok(TravelProvisionDecision::Deferred(
                "journey_payer_provider_projection_unavailable",
            ));
        };
        let stake = self
            .connection
            .db
            .party_stake()
            .iter()
            .find(|row| row.party_id == party_id && row.character_id == payer)
            .map_or(0, |row| row.value);
        let finance_cache_key = (party_id.to_owned(), leader, finance_key.to_owned());
        if !affordable {
            if self
                .generated_seen_cases
                .contains(&(leader, finance_key.to_owned()))
            {
                self.metrics.generated_finance_blocked_cycles = self
                    .metrics
                    .generated_finance_blocked_cycles
                    .saturating_add(1);
            }
            let public_funds = party_coin.saturating_add(personal);
            let signature = (upper_bound_cost, public_funds);
            if self.generated_finance_blocks.get(&finance_cache_key) == Some(&signature) {
                return Ok(TravelProvisionDecision::Deferred("journey_finance_backoff"));
            }
            self.generated_finance_blocks
                .insert(finance_cache_key, signature);
            self.event(
                agent,
                CoreLoopEventKind::QuestSuppressed,
                format!(
                    "reason=journey_essentials_unaffordable;planning_minutes={planning_minutes};payer={payer};upper_bound_cost={upper_bound_cost};treasury={party_coin};payer_purse={personal};claimable_stake={stake};committed_reserve={committed_reserve};spendable={spendable};deficit={};rations_needed={rations_to_buy};waterskins_needed={waterskins_to_buy}",
                    upper_bound_cost.saturating_sub(spendable),
                ),
            );
            return Ok(TravelProvisionDecision::Deferred(
                "journey_essentials_unaffordable",
            ));
        }
        self.generated_finance_blocks.remove(&finance_cache_key);
        let mut item_ids = Vec::new();
        let mut quantities = Vec::new();
        if rations_to_buy > 0 {
            item_ids.push(ration.id.clone());
            quantities.push(rations_to_buy);
        }
        if waterskins_to_buy > 0 {
            item_ids.push(waterskin.id.clone());
            quantities.push(waterskins_to_buy);
        }
        let result = reducer_call!(self, "purchase_journey_provisions", |cb| self
            .connection
            .reducers
            .finalize_merchant_trade_then(
                payer,
                settlement_id.clone(),
                item_ids.clone(),
                quantities.clone(),
                vec![],
                vec![],
                true,
                cb,
            ));
        self.call(result)?;
        let after_party_coin = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party_id && is_currency_id(&row.item_id))
            .map(|row| u64::from(row.quantity))
            .sum::<u64>();
        let actual_spent = party_coin
            .saturating_add(personal)
            .saturating_sub(after_party_coin.saturating_add(self.personal_gold(payer)));
        self.metrics.journey_provision_purchases += 1;
        self.metrics.journey_provision_party_gold_spent = self
            .metrics
            .journey_provision_party_gold_spent
            .saturating_add(actual_spent);
        self.event(
            agent,
            CoreLoopEventKind::Purchase,
            format!(
                "journey_provisions=purchased;planning_minutes={planning_minutes};reserve_days={TRAVEL_PROVISION_RESERVE_DAYS:.1};payer={payer};treasury_before={party_coin};payer_purse_before={personal};claimable_stake={stake};upper_bound_cost={upper_bound_cost};actual_spent={actual_spent};rations={rations_to_buy};waterskins={waterskins_to_buy}"
            ),
        );
        Ok(TravelProvisionDecision::Ready)
    }

    pub(super) fn public_active_camp_observation(
        &self,
        party_id: &str,
    ) -> Option<PublicActiveCampObservation> {
        let party = self
            .connection
            .db
            .party()
            .iter()
            .find(|party| party.id == party_id)?;
        let camp_destination = party.camp_destination.as_ref()?;
        if party.current_settlement_id.is_some() || party.camp_remaining_minutes == 0 {
            return None;
        }
        let journeys = self
            .connection
            .db
            .party_journey()
            .iter()
            .filter(|journey| journey.party_id == party_id)
            .collect::<Vec<_>>();
        let itineraries = self
            .connection
            .db
            .party_journey_itinerary()
            .iter()
            .filter(|itinerary| itinerary.party_id == party_id)
            .collect::<Vec<_>>();
        let [journey] = journeys.as_slice() else {
            return None;
        };
        let [itinerary] = itineraries.as_slice() else {
            return None;
        };
        if &journey.destination != camp_destination
            || journey.completed_elapsed_minutes >= journey.total_elapsed_minutes
        {
            return None;
        }
        let (active_interval_start, active_interval_minutes) = projected_camp_rest_minutes(
            journey.completed_elapsed_minutes,
            journey.total_elapsed_minutes,
            &itinerary.forecast_camp_intervals,
        )?;
        (active_interval_minutes > 0).then_some(PublicActiveCampObservation {
            completed_elapsed_minutes: journey.completed_elapsed_minutes,
            total_elapsed_minutes: journey.total_elapsed_minutes,
            active_interval_start,
            active_interval_minutes,
        })
    }

    pub(super) fn public_camp_coherence_error(&self, party_id: &str, reason: &str) -> String {
        let journeys = self
            .connection
            .db
            .party_journey()
            .iter()
            .filter(|journey| journey.party_id == party_id)
            .collect::<Vec<_>>();
        let itineraries = self
            .connection
            .db
            .party_journey_itinerary()
            .iter()
            .filter(|itinerary| itinerary.party_id == party_id)
            .collect::<Vec<_>>();
        let completed_elapsed = journeys.first().map_or_else(
            || "none".into(),
            |journey| {
                bounded_public_journey_diagnostic(journey.completed_elapsed_minutes).to_string()
            },
        );
        let total_elapsed = journeys.first().map_or_else(
            || "none".into(),
            |journey| bounded_public_journey_diagnostic(journey.total_elapsed_minutes).to_string(),
        );
        let forecast_count = itineraries.first().map_or_else(
            || "none".into(),
            |itinerary| {
                bounded_public_forecast_count(itinerary.forecast_camp_intervals.len()).to_string()
            },
        );
        let active_interval_count: String = journeys.first().zip(itineraries.first()).map_or_else(
            || "unavailable".to_string(),
            |(journey, itinerary)| {
                match projected_active_camp_interval_count(
                    journey.completed_elapsed_minutes,
                    journey.total_elapsed_minutes,
                    &itinerary.forecast_camp_intervals,
                ) {
                    0 => "0",
                    1 => "1",
                    _ => ">1",
                }
                .to_string()
            },
        );
        format!(
            "travel_camps failed: journey camp projection is incoherent: reason={};active_interval_count={active_interval_count};completed_elapsed={completed_elapsed};total_elapsed={total_elapsed};forecast_count={forecast_count};journey_count={};itinerary_count={}",
            bounded_event_field(reason),
            journeys.len(),
            itineraries.len(),
        )
    }

    pub(super) fn public_post_encounter_journey_action(
        &self,
        party_id: &str,
        actionable_actor: bool,
        unsafe_member_count: usize,
        evacuation: bool,
    ) -> Result<PostEncounterJourneyAction, String> {
        let unresolved_encounter = self.party_has_unresolved_public_encounter(party_id);
        let party = self.party_by_id(party_id)?;
        let journeys = self
            .connection
            .db
            .party_journey()
            .iter()
            .filter(|journey| journey.party_id == party_id)
            .collect::<Vec<_>>();
        let itineraries = self
            .connection
            .db
            .party_journey_itinerary()
            .iter()
            .filter(|itinerary| itinerary.party_id == party_id)
            .collect::<Vec<_>>();
        let destination_matches = party.camp_destination.as_ref().is_some_and(|destination| {
            matches!(
                (journeys.as_slice(), itineraries.as_slice()),
                ([journey], [itinerary])
                    if &journey.destination == destination && itinerary.party_id == party_id
            )
        });
        let active_interval_count = match (journeys.as_slice(), itineraries.as_slice()) {
            ([journey], [itinerary]) => projected_active_camp_interval_count(
                journey.completed_elapsed_minutes,
                journey.total_elapsed_minutes,
                &itinerary.forecast_camp_intervals,
            ),
            _ => 0,
        };
        classify_post_encounter_journey(PublicPostEncounterJourneyState {
            unresolved_encounter,
            active_destination: party.camp_destination.is_some(),
            journey_count: journeys.len(),
            itinerary_count: itineraries.len(),
            destination_matches,
            active_interval_count,
            actionable_actor,
            unsafe_member_count,
            evacuation,
        })
        .map_err(|reason| self.public_camp_coherence_error(party_id, reason))
    }

    pub(super) fn party_has_unresolved_public_encounter(&self, party_id: &str) -> bool {
        self.connection
            .db
            .strategic_encounter()
            .iter()
            .any(|row| row.party_id == party_id && row.status == "awaiting_choice")
    }

    pub(super) fn travel_camps(&mut self, party_id: &str) -> Result<JourneyTravelOutcome, String> {
        for _ in 0..MAX_CAMPS_PER_LEG {
            let party = self.party_by_id(party_id)?;
            if party.camp_destination.is_none() {
                self.metrics.travel_legs += 1;
                return Ok(JourneyTravelOutcome::Completed);
            }
            let remaining_before = party.camp_remaining_minutes;
            let Some((travel_actor, _, _)) = self.expedition_recovery_actor(party_id) else {
                self.observe_deaths();
                return self.record_journey_hold(
                    party_id,
                    "journey_stalled",
                    "journey_held_no_actionable_actor",
                );
            };
            let pending_encounter = {
                let table = self.connection.db.strategic_encounter();
                table
                    .iter()
                    .find(|row| row.party_id == party_id && row.status == "awaiting_choice")
            };
            if let Some(encounter) = pending_encounter {
                self.metrics.encounters += 1;
                if encounter.run_ineligibility.is_none() {
                    self.metrics.encounter_escape_eligible += 1;
                } else {
                    self.metrics.encounter_escape_ineligible += 1;
                }
                let evacuation = self.public_journey_is_evacuation(party_id);
                let choice = select_expedition_encounter_choice(
                    &encounter.available_choices,
                    encounter.roll_index,
                    evacuation,
                )
                .ok_or("encounter offers no protective evacuation choice")?;
                match choice.as_str() {
                    "sneak" => self.metrics.encounter_sneaks += 1,
                    "detour" => self.metrics.encounter_detours += 1,
                    "attack" => self.metrics.encounter_attacks += 1,
                    "run" => self.metrics.encounter_runs += 1,
                    "surrender" => {
                        self.metrics.encounter_surrenders += 1;
                        self.metrics.encounter_surrender_items_lost =
                            self.metrics.encounter_surrender_items_lost.saturating_add(
                                encounter
                                    .loss_preview
                                    .iter()
                                    .map(|loss| loss.quantity)
                                    .sum(),
                            );
                        self.metrics.encounter_surrender_value_lost =
                            self.metrics.encounter_surrender_value_lost.saturating_add(
                                encounter
                                    .loss_preview
                                    .iter()
                                    .map(|loss| {
                                        u64::from(loss.quantity) * u64::from(loss.value_each)
                                    })
                                    .sum::<u64>(),
                            );
                    }
                    _ => return Err("encounter exposed an unknown choice".into()),
                }
                let encounter_id = encounter.encounter_id.clone();
                let encounter_revision = encounter.revision;
                let action_id = format!(
                    "strategic-sim:{encounter_id}:{encounter_revision}:{choice}"
                );
                let result = reducer_call!(self, "resolve_strategic_encounter", |cb| self
                    .connection
                    .reducers
                    .resolve_strategic_encounter_then(
                        travel_actor,
                        encounter_id.clone(),
                        choice.clone(),
                        encounter_revision,
                        action_id,
                        cb,
                    ));
                self.call(result)?;
                self.observe_deaths();
                let resolved_outcome = {
                    let table = self.connection.db.strategic_encounter();
                    table
                        .iter()
                        .find(|row| row.encounter_id == encounter_id)
                        .ok_or("resolved encounter row disappeared")?
                        .outcome
                };
                if resolved_outcome.as_deref() == Some("defeat") {
                    self.metrics.encounter_defeats += 1;
                    if self.current_leader(party_id).is_none() {
                        self.metrics.encounter_wipes += 1;
                    }
                }
                self.event(
                    self.current_leader(party_id).map_or(0, |(_, agent)| agent),
                    CoreLoopEventKind::Encounter,
                    format!("id={encounter_id};choice={choice};outcome={resolved_outcome:?}"),
                );
                if self.current_leader(party_id).is_none() {
                    return self.record_journey_hold(
                        party_id,
                        "journey_stalled_after_encounter",
                        "journey_held_no_actionable_actor",
                    );
                }
                let recovery_actor = self.expedition_recovery_actor(party_id);
                let unsafe_after_encounter = recovery_actor
                    .map(|(actor, _, _)| self.party_agents(actor))
                    .transpose()?
                    .map_or_else(Vec::new, |agents| self.unsafe_party_agents(&agents));
                let post_encounter_action = self.public_post_encounter_journey_action(
                    party_id,
                    recovery_actor.is_some(),
                    unsafe_after_encounter.len(),
                    self.public_journey_is_evacuation(party_id),
                )?;
                match post_encounter_action {
                    PostEncounterJourneyAction::ReclassifyPublicState
                    | PostEncounterJourneyAction::HandleActiveCamp => {}
                    PostEncounterJourneyAction::HoldNoActionableActor => {
                        return self.record_journey_hold(
                            party_id,
                            "journey_stalled_after_encounter",
                            "journey_held_no_actionable_actor",
                        );
                    }
                    PostEncounterJourneyAction::HoldForRecovery => {
                        self.metrics.expedition_holds =
                            self.metrics.expedition_holds.saturating_add(1);
                        for unsafe_agent in unsafe_after_encounter {
                            self.metrics.quests_suppressed_for_health =
                                self.metrics.quests_suppressed_for_health.saturating_add(1);
                            self.event(
                                unsafe_agent,
                                CoreLoopEventKind::QuestSuppressed,
                                "reason=journey_encounter_member_not_ready;plan=off_settlement_recovery_next_cycle",
                            );
                        }
                        return Ok(JourneyTravelOutcome::HeldForRecovery);
                    }
                    PostEncounterJourneyAction::ContinueTravel => {
                        let Some((continue_actor, agent, continue_actor_role)) = recovery_actor
                        else {
                            return self.record_journey_hold(
                                party_id,
                                "journey_stalled_after_encounter",
                                "journey_held_no_actionable_actor",
                            );
                        };
                        let leg_members_before = self.expedition_member_observations(party_id)?;
                        let leg_supplies_before = self.expedition_supplies(party_id);
                        let result = reducer_call!(self, "continue_camp_travel", |cb| self
                            .connection
                            .reducers
                            .continue_camp_travel_then(continue_actor, cb));
                        self.call(result)?;
                        self.observe_deaths();
                        let leg_members_after = self.expedition_member_observations(party_id)?;
                        let leg_supplies_after = self.expedition_supplies(party_id);
                        self.emit_expedition_diagnostics(
                            party_id,
                            "journey_leg",
                            "continue_camp_travel",
                            &format!("quest_leg_resumed_after_{choice}_{continue_actor_role}"),
                            &leg_members_before,
                            &leg_members_after,
                            leg_supplies_before,
                            leg_supplies_after,
                        );
                        if self.current_leader(party_id).is_none() {
                            return self.record_journey_hold(
                                party_id,
                                "journey_stalled_after_encounter_continuation",
                                "journey_held_no_actionable_actor",
                            );
                        }
                        self.event(
                            agent,
                            CoreLoopEventKind::Travel,
                            format!(
                                "phase=post_encounter_continue;party={};choice={choice};remaining_movement={}",
                                bounded_event_field(party_id),
                                self.party_by_id(party_id)?.camp_remaining_minutes,
                            ),
                        );
                    }
                }
                continue;
            }
            let camp = self
                .public_active_camp_observation(party_id)
                .ok_or_else(|| {
                    self.public_camp_coherence_error(party_id, "no_unique_active_public_camp")
                })?;
            let camp_start = camp.active_interval_start;
            let rest_minutes = camp.active_interval_minutes;
            self.event(
                self.current_leader(party_id).map_or(0, |(_, agent)| agent),
                CoreLoopEventKind::Camp,
                format!(
                    "phase=pre_rest;party={};completed_elapsed={};total_elapsed={};camp_start={camp_start};rest_minutes={rest_minutes};remaining_movement={remaining_before}",
                    bounded_event_field(party_id),
                    camp.completed_elapsed_minutes,
                    camp.total_elapsed_minutes,
                ),
            );
            let camp_members_before = self.expedition_member_observations(party_id)?;
            let camp_supplies_before = self.expedition_supplies(party_id);
            let expected_completed_elapsed = camp_start.saturating_add(rest_minutes);
            let result = reducer_call!(self, "rest_at_camp", |cb| self
                .connection
                .reducers
                .rest_at_camp_then(travel_actor, rest_minutes, FieldShelter::Bivouac, cb));
            self.call(result)?;
            self.observe_deaths();
            let Some((continue_actor, agent, continue_actor_role)) =
                self.expedition_recovery_actor(party_id)
            else {
                return self.record_journey_hold(
                    party_id,
                    "journey_stalled_after_rest",
                    "journey_held_no_actionable_actor",
                );
            };
            let unsafe_after_rest = self.unsafe_party_agents(&self.party_agents(continue_actor)?);
            let camp_members_after = self.expedition_member_observations(party_id)?;
            let camp_supplies_after = self.expedition_supplies(party_id);
            self.emit_expedition_diagnostics(
                party_id,
                "journey_camp",
                "rest_at_camp",
                if unsafe_after_rest.is_empty() {
                    "quest_leg_rest_complete"
                } else {
                    "quest_suppressed_member_not_ready_after_camp"
                },
                &camp_members_before,
                &camp_members_after,
                camp_supplies_before,
                camp_supplies_after,
            );
            let after_rest_party = self.party_by_id(party_id)?;
            let after_rest_journey = self
                .connection
                .db
                .party_journey()
                .iter()
                .find(|row| row.party_id == party_id)
                .ok_or("journey camp projection is incoherent: journey disappeared after rest")?;
            let after_rest_itinerary = self
                .connection
                .db
                .party_journey_itinerary()
                .iter()
                .find(|row| row.party_id == party_id)
                .ok_or("journey camp projection is incoherent: itinerary disappeared after rest")?;
            if after_rest_party.camp_destination.is_none()
                || after_rest_journey.completed_elapsed_minutes != expected_completed_elapsed
                || after_rest_journey.completed_elapsed_minutes
                    > after_rest_journey.total_elapsed_minutes
                || after_rest_itinerary.party_id != party_id
            {
                return Err(
                    "journey camp projection is incoherent: rest did not produce a safe forecast boundary"
                        .into(),
                );
            }
            self.event(
                agent,
                CoreLoopEventKind::Camp,
                format!(
                    "phase=post_rest;party={};completed_elapsed={};total_elapsed={};rest_minutes={rest_minutes};remaining_movement={}",
                    bounded_event_field(party_id),
                    after_rest_journey.completed_elapsed_minutes,
                    after_rest_journey.total_elapsed_minutes,
                    after_rest_party.camp_remaining_minutes,
                ),
            );
            let evacuation_leg = matches!(
                after_rest_party.camp_destination,
                Some(JourneyEndpoint::Settlement(_))
            );
            if !unsafe_after_rest.is_empty() && !evacuation_leg {
                self.metrics.expedition_holds = self.metrics.expedition_holds.saturating_add(1);
                for unsafe_agent in unsafe_after_rest {
                    self.metrics.quests_suppressed_for_health =
                        self.metrics.quests_suppressed_for_health.saturating_add(1);
                    self.event(
                        unsafe_agent,
                        CoreLoopEventKind::QuestSuppressed,
                        "reason=journey_camp_member_not_ready;plan=off_settlement_recovery_next_cycle",
                    );
                }
                return Ok(JourneyTravelOutcome::HeldForRecovery);
            }
            let leg_members_before = self.expedition_member_observations(party_id)?;
            let leg_supplies_before = self.expedition_supplies(party_id);
            let result = reducer_call!(self, "continue_camp_travel", |cb| self
                .connection
                .reducers
                .continue_camp_travel_then(continue_actor, cb));
            self.call(result)?;
            self.observe_deaths();
            self.metrics.camp_stops += 1;
            self.event(
                agent,
                CoreLoopEventKind::Camp,
                format!(
                    "phase=post_continue;party={};remaining_before={remaining_before};remaining_after={}",
                    bounded_event_field(party_id),
                    self.party_by_id(party_id)?.camp_remaining_minutes,
                ),
            );
            let leg_members_after = self.expedition_member_observations(party_id)?;
            let leg_supplies_after = self.expedition_supplies(party_id);
            let recovery_needed = leg_members_after
                .iter()
                .any(expedition_member_needs_recovery);
            let leg_reason = if evacuation_leg {
                format!("quest_suppressed_evacuation_continues_{continue_actor_role}")
            } else if recovery_needed {
                "quest_suppressed_member_not_ready_after_leg;plan=off_settlement_recovery_next_cycle"
                    .into()
            } else {
                "quest_leg_resumed_all_members_ready".into()
            };
            self.emit_expedition_diagnostics(
                party_id,
                "journey_leg",
                "continue_camp_travel",
                &leg_reason,
                &leg_members_before,
                &leg_members_after,
                leg_supplies_before,
                leg_supplies_after,
            );
            let after = self.party_by_id(party_id)?;
            if after.camp_destination.is_some() && after.camp_remaining_minutes >= remaining_before
            {
                self.metrics.stuck_detections += 1;
                return Err("camp continuation made no progress".into());
            }
        }
        self.metrics.stuck_detections += 1;
        Err("camp bound exhausted".into())
    }

    pub(super) fn continue_public_active_journey(
        &mut self,
        party_id: &str,
    ) -> Result<Option<JourneyTravelOutcome>, String> {
        let party = self.party_by_id(party_id)?;
        let has_public_journey = party.camp_destination.is_some()
            || self
                .connection
                .db
                .party_journey()
                .iter()
                .any(|journey| journey.party_id == party_id);
        if !has_public_journey {
            return Ok(None);
        }
        let outcome = self.travel_camps(party_id)?;
        if outcome != JourneyTravelOutcome::Completed {
            return Ok(Some(outcome));
        }

        self.observe_deaths();
        let Some((leader, agent)) = self.current_leader(party_id) else {
            return self
                .record_journey_hold(
                    party_id,
                    "journey_arrival_revalidation",
                    "journey_held_arrival_not_proven",
                )
                .map(Some);
        };
        let party_agents = self.party_agents(leader)?;
        let arrived = self.party_by_id(party_id)?;
        let location_is_publicly_coherent = arrived.camp_destination.is_none()
            && (arrived.current_settlement_id.is_some() ^ arrived.current_case_site_id.is_some());
        if arrived.id != party_id
            || !self.unsafe_party_agents(&party_agents).is_empty()
            || !location_is_publicly_coherent
        {
            self.metrics.expedition_holds = self.metrics.expedition_holds.saturating_add(1);
            self.event(
                agent,
                CoreLoopEventKind::QuestSuppressed,
                "reason=journey_continuation_arrival_not_actionable",
            );
            return Ok(Some(JourneyTravelOutcome::HeldForRecovery));
        }
        self.event(
            agent,
            CoreLoopEventKind::Travel,
            format!(
                "journey_continuation=completed;settlement={};case_site={}",
                arrived
                    .current_settlement_id
                    .as_deref()
                    .map_or_else(|| "none".into(), bounded_event_field),
                arrived
                    .current_case_site_id
                    .as_ref()
                    .map_or_else(|| "none".into(), |site| bounded_event_field(&site.value)),
            ),
        );
        Ok(Some(JourneyTravelOutcome::Completed))
    }

    pub(super) fn choose_quest(
        &self,
        party: &Party,
        profile: &AgentProfile,
    ) -> Option<BackendContract> {
        let settlement = party.current_settlement_id.as_ref()?;
        let mut quests: Vec<_> = self
            .connection
            .db
            .backend_contracts()
            .iter()
            .filter(|q| q.settlement_id == *settlement && q.status == ContractStatus::Offered)
            .collect();
        quests.sort_by_key(|q| {
            let risk_target = (profile.risk_tolerance * 10.0).round() as i32;
            ((q.difficulty - risk_target).abs(), q.id.clone())
        });
        quests.into_iter().next()
    }

    pub(super) fn active_direct_contract(&self, party: &Party) -> Option<BackendContract> {
        let contract_id = party.active_contract_id.as_ref()?;
        self.connection
            .db
            .backend_contracts()
            .iter()
            .find(|contract| {
                contract.id == *contract_id
                    && contract.accepted_by.as_deref() == Some(party.id.as_str())
                    && matches!(
                        contract.status,
                        ContractStatus::Accepted | ContractStatus::ReadyToReport
                    )
            })
    }
}
