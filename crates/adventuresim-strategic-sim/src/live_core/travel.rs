#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CampContinuationOutcome {
    Advanced,
    DeferredForDaylightWindow,
}

pub(super) fn classify_camp_continuation(
    result: Result<(), CoreLoopError>,
) -> Result<CampContinuationOutcome, CoreLoopError> {
    match result {
        Ok(()) => Ok(CampContinuationOutcome::Advanced),
        Err(error)
            if error.reducer_code()
                == Some(ReducerErrorCode::JourneyDaylightWindowRequired) =>
        {
            Ok(CampContinuationOutcome::DeferredForDaylightWindow)
        }
        Err(error) => Err(error),
    }
}

impl LiveRunner {
    fn contribute_party_journey_currency(
        &mut self,
        party_id: &str,
        settlement_id: &str,
        needed: u64,
    ) -> Result<(u64, usize), String> {
        let mut contributors = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|member| member.party_id == party_id)
            .filter_map(|member| {
                let character = self.connection.db.backend_characters().iter().find(|row| {
                    row.id == member.character_id
                        && row.alive
                        && row.current_settlement_id.as_deref() == Some(settlement_id)
                })?;
                let ready = self
                    .connection
                    .db
                    .backend_character_strategic_conditions()
                    .iter()
                    .find(|row| row.character_id == character.id)
                    .is_some_and(|row| {
                        domain_incapacitation_status(row.status)
                            == DomainIncapacitationStatus::Ready
                    });
                let illness_safe = self
                    .connection
                    .db
                    .character_illness_status()
                    .iter()
                    .find(|row| row.character_id == character.id)
                    .is_none_or(|row| !row.symptomatic && !row.critical);
                (ready && illness_safe).then_some(character.id)
            })
            .collect::<Vec<_>>();
        contributors.sort_unstable();
        let mut remaining = needed;
        let mut contributed = 0_u64;
        let mut contributor_count = 0_usize;
        for character_id in contributors {
            if remaining == 0 {
                break;
            }
            let reserve = self
                .observable_medical_reserve(character_id, settlement_id)
                .unwrap_or(0);
            let available = self.personal_gold(character_id).saturating_sub(reserve);
            let mut contribution = remaining.min(available);
            if contribution == 0 {
                continue;
            }
            let mut stacks = self
                .connection
                .db
                .inventory_item()
                .iter()
                .filter(|row| row.character_id == character_id && is_currency_id(&row.item_id))
                .collect::<Vec<_>>();
            stacks.sort_by_key(|row| (row.item_id.clone(), row.id));
            let planned = contribution;
            for stack in stacks {
                if contribution == 0 {
                    break;
                }
                let quantity = contribution.min(u64::from(stack.quantity)) as u32;
                let result = reducer_call!(self, ReducerOperation::ContributeJourneyCurrency, |cb| self
                    .connection
                    .reducers
                    .deposit_party_inventory_item_then(character_id, stack.id, quantity, cb));
                self.call(result)?;
                contribution -= u64::from(quantity);
            }
            let deposited = planned.saturating_sub(contribution);
            if deposited > 0 {
                contributed = contributed.saturating_add(deposited);
                remaining = remaining.saturating_sub(deposited);
                contributor_count += 1;
            }
        }
        Ok((contributed, contributor_count))
    }

    pub(super) fn public_party_matchup_assessment(
        &self,
        party_id: &str,
        difficulty: i32,
        opposition_count: u32,
        opposition_combat_power: u64,
    ) -> PublicContractAssessment {
        let living_ids = self
            .connection
            .db
            .backend_characters()
            .iter()
            .filter(|character| character.alive)
            .map(|character| character.id)
            .collect::<HashSet<_>>();
        let member_ids = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|member| member.party_id == party_id)
            .map(|member| member.character_id)
            .filter(|character_id| living_ids.contains(character_id))
            .collect::<HashSet<_>>();
        let members = self
            .connection
            .db
            .backend_character_capabilities()
            .iter()
            .filter(|row| member_ids.contains(&row.character_id))
            .map(|capability| {
                let condition_ready = self
                    .connection
                    .db
                    .backend_character_strategic_conditions()
                    .iter()
                    .find(|row| row.character_id == capability.character_id)
                    .is_some_and(|row| {
                        domain_incapacitation_status(row.status)
                            == DomainIncapacitationStatus::Ready
                    });
                let illness_safe = self
                    .connection
                    .db
                    .character_illness_status()
                    .iter()
                    .find(|row| row.character_id == capability.character_id)
                    .is_none_or(|row| !row.symptomatic && !row.critical);
                PublicPartyCombatant {
                    capability,
                    ready: condition_ready && illness_safe,
                }
            })
            .collect::<Vec<_>>();
        public_contract_assessment(
            difficulty,
            opposition_count,
            opposition_combat_power,
            &members,
        )
    }

    pub(super) fn public_party_contract_assessment(
        &self,
        party_id: &str,
        contract: &BackendContract,
    ) -> PublicContractAssessment {
        self.public_party_matchup_assessment(
            party_id,
            contract.difficulty,
            contract.opposition_count,
            contract.opposition_combat_power,
        )
    }

    pub(super) fn public_party_combat_fingerprint(
        &self,
        party_id: &str,
    ) -> PublicCombatFingerprint {
        let living_ids = self
            .connection
            .db
            .backend_characters()
            .iter()
            .filter(|character| character.alive)
            .map(|character| character.id)
            .collect::<HashSet<_>>();
        let member_ids = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|member| member.party_id == party_id)
            .map(|member| member.character_id)
            .filter(|character_id| living_ids.contains(character_id))
            .collect::<HashSet<_>>();
        public_combat_fingerprint(
            self.connection
                .db
                .backend_character_capabilities()
                .iter()
                .filter(|row| member_ids.contains(&row.character_id))
                .collect(),
        )
    }

    pub(super) fn provision_case_site_journey(
        &mut self,
        party_id: &str,
        leader: u64,
        agent: u32,
        finance_key: &str,
        distance_m: u64,
        additional_plan_minutes: u64,
    ) -> Result<TravelProvisionDecision, String> {
        let party = self.party_by_id(party_id)?;
        let Some(settlement_id) = party.current_settlement_id.clone() else {
            return Ok(TravelProvisionDecision::Deferred(
                TravelProvisionDeferralReason::RequiresSettlement,
            ));
        };
        let planning_minutes =
            projected_case_site_journey_minutes(distance_m, party.walking_minutes_per_day)
                .and_then(|minutes| minutes.checked_add(additional_plan_minutes))
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
                row.id == adventuresim_core::item_references::STANDARD_TRAVEL_RATION_ID
                    && row.nutrition_kcal > 0.0
            })
            .ok_or("journey provisioning projection is incoherent")?;
        let waterskin = self
            .connection
            .db
            .item()
            .iter()
            .find(|row| {
                row.id == adventuresim_core::item_references::STANDARD_WATERSKIN_ID
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
            waterskin_count: count_item(adventuresim_core::item_references::STANDARD_WATERSKIN_ID),
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
                TravelProvisionDeferralReason::EssentialsUnavailable,
            ));
        }
        let party_coin = party_inventory
            .iter()
            .filter(|row| is_currency_id(&row.item_id))
            .map(|row| u64::from(row.quantity))
            .sum::<u64>();
        let party_personal_funds = members
            .iter()
            .filter(|member| member.current_settlement_id.as_deref() == Some(&settlement_id))
            .filter(|member| {
                self.connection
                    .db
                    .backend_character_strategic_conditions()
                    .iter()
                    .find(|row| row.character_id == member.id)
                    .is_some_and(|row| {
                        domain_incapacitation_status(row.status)
                            == DomainIncapacitationStatus::Ready
                    })
                    && self
                        .connection
                        .db
                        .character_illness_status()
                        .iter()
                        .find(|row| row.character_id == member.id)
                        .is_none_or(|row| !row.symptomatic && !row.critical)
            })
            .map(|member| {
                let reserve = self
                    .observable_medical_reserve(member.id, &settlement_id)
                    .unwrap_or(0);
                self.personal_gold(member.id).saturating_sub(reserve)
            })
            .sum::<u64>();
        let party_spendable = party_coin.saturating_add(party_personal_funds);
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
                                        presence.context_suppressed,
                                        presence.health_suppressed,
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
                let spendable = party_spendable;
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
                TravelProvisionDeferralReason::PayerProviderProjectionUnavailable,
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
            let public_funds = party_spendable;
            let signature = (upper_bound_cost, public_funds);
            if self.generated_finance_blocks.get(&finance_cache_key) == Some(&signature) {
                return Ok(TravelProvisionDecision::Deferred(
                    TravelProvisionDeferralReason::FinanceBackoff,
                ));
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
                TravelProvisionDeferralReason::EssentialsUnaffordable,
            ));
        }
        self.generated_finance_blocks.remove(&finance_cache_key);
        let total_personal_before = members
            .iter()
            .map(|member| self.personal_gold(member.id))
            .sum::<u64>();
        let contribution_needed = upper_bound_cost.saturating_sub(party_coin);
        let (contributed, contributor_count) =
            self.contribute_party_journey_currency(party_id, &settlement_id, contribution_needed)?;
        let funded_party_coin = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party_id && is_currency_id(&row.item_id))
            .map(|row| u64::from(row.quantity))
            .sum::<u64>();
        if funded_party_coin < upper_bound_cost {
            return Ok(TravelProvisionDecision::Deferred(
                TravelProvisionDeferralReason::ContributionRevalidationFailed,
            ));
        }
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
        let result = reducer_call!(self, ReducerOperation::PurchaseJourneyProvisions, |cb| self
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
        if let Err(error) = result {
            if merchant_provider_unavailable_failure(&error) {
                return Ok(TravelProvisionDecision::Deferred(
                    TravelProvisionDeferralReason::PayerProviderProjectionUnavailable,
                ));
            }
            return self
                .call(Err(error))
                .map(|_| TravelProvisionDecision::Ready);
        }
        let after_party_coin = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party_id && is_currency_id(&row.item_id))
            .map(|row| u64::from(row.quantity))
            .sum::<u64>();
        let total_personal_after = members
            .iter()
            .map(|member| self.personal_gold(member.id))
            .sum::<u64>();
        let actual_spent = party_coin
            .saturating_add(total_personal_before)
            .saturating_sub(after_party_coin.saturating_add(total_personal_after));
        self.metrics.journey_provision_purchases += 1;
        self.metrics.journey_provision_party_gold_spent = self
            .metrics
            .journey_provision_party_gold_spent
            .saturating_add(actual_spent);
        self.event(
            agent,
            CoreLoopEventKind::Purchase,
            format!(
                "journey_provisions=purchased;planning_minutes={planning_minutes};reserve_days={TRAVEL_PROVISION_RESERVE_DAYS:.1};payer={payer};treasury_before={party_coin};payer_purse_before={personal};claimable_stake={stake};party_personal_spendable={party_personal_funds};contributed={contributed};contributors={contributor_count};upper_bound_cost={upper_bound_cost};actual_spent={actual_spent};rations={rations_to_buy};waterskins={waterskins_to_buy}"
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
        let [journey] = journeys.as_slice() else {
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
            &journey.forecast_camp_intervals,
        )?;
        (active_interval_minutes > 0).then_some(PublicActiveCampObservation {
            completed_elapsed_minutes: journey.completed_elapsed_minutes,
            total_elapsed_minutes: journey.total_elapsed_minutes,
            active_interval_start,
            active_interval_minutes,
        })
    }

    pub(super) fn public_generated_case_site_assessment(
        &self,
        party_id: &str,
        pin: &BackendCaseSitePin,
    ) -> PublicContractAssessment {
        match (pin.opposition_count, pin.opposition_combat_power) {
            (Some(count), Some(power)) if pin.combat_available && count > 0 && power > 0 => {
                self.public_party_matchup_assessment(party_id, 1, count, power)
            }
            _ => PublicContractAssessment {
                eligible: false,
                reason: "missing_generated_opposition_assessment",
                enemy_count: pin.opposition_count,
                ready_combatants: 0,
                party_power_milli: 0,
                enemy_power_milli: pin.opposition_combat_power.unwrap_or(0),
            },
        }
    }

    pub(super) fn public_journey_camp_state(
        &self,
        party_id: &str,
    ) -> Result<PublicJourneyCampState, String> {
        let party = self.party_by_id(party_id)?;
        let destination = party
            .camp_destination
            .as_ref()
            .ok_or_else(|| self.public_camp_coherence_error(party_id, "no_active_destination"))?;
        if party.current_settlement_id.is_some() || party.camp_remaining_minutes == 0 {
            return Err(self.public_camp_coherence_error(
                party_id,
                "active_destination_has_no_remaining_movement",
            ));
        }
        let journeys = self
            .connection
            .db
            .party_journey()
            .iter()
            .filter(|journey| journey.party_id == party_id)
            .collect::<Vec<_>>();
        let [journey] = journeys.as_slice() else {
            return Err(self.public_camp_coherence_error(party_id, "no_unique_public_journey"));
        };
        if &journey.destination != destination {
            return Err(
                self.public_camp_coherence_error(party_id, "public_journey_destination_mismatch")
            );
        }
        if journey.completed_elapsed_minutes >= journey.total_elapsed_minutes {
            return Err(self
                .public_camp_coherence_error(party_id, "active_destination_journey_is_complete"));
        }
        classify_public_journey_camp_state(projected_active_camp_interval_count(
            journey.completed_elapsed_minutes,
            journey.total_elapsed_minutes,
            &journey.forecast_camp_intervals,
        ))
        .map_err(|reason| self.public_camp_coherence_error(party_id, reason))
    }

    pub(super) fn public_camp_coherence_error(&self, party_id: &str, reason: &str) -> String {
        let journeys = self
            .connection
            .db
            .party_journey()
            .iter()
            .filter(|journey| journey.party_id == party_id)
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
        let forecast_count = journeys.first().map_or_else(
            || "none".into(),
            |journey| {
                bounded_public_forecast_count(journey.forecast_camp_intervals.len()).to_string()
            },
        );
        let active_interval_count: String = journeys.first().map_or_else(
            || "unavailable".to_string(),
            |journey| {
                match projected_active_camp_interval_count(
                    journey.completed_elapsed_minutes,
                    journey.total_elapsed_minutes,
                    &journey.forecast_camp_intervals,
                ) {
                    0 => "0",
                    1 => "1",
                    _ => ">1",
                }
                .to_string()
            },
        );
        format!(
            "travel_camps failed: journey camp projection is incoherent: reason={};active_interval_count={active_interval_count};completed_elapsed={completed_elapsed};total_elapsed={total_elapsed};forecast_count={forecast_count};journey_count={}",
            bounded_event_field(reason),
            journeys.len(),
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
        let destination_matches = party.camp_destination.as_ref().is_some_and(|destination| {
            matches!(journeys.as_slice(), [journey] if &journey.destination == destination)
        });
        let active_interval_count = match journeys.as_slice() {
            [journey] => projected_active_camp_interval_count(
                journey.completed_elapsed_minutes,
                journey.total_elapsed_minutes,
                &journey.forecast_camp_intervals,
            ),
            _ => 0,
        };
        classify_post_encounter_journey(PublicPostEncounterJourneyState {
            unresolved_encounter,
            active_destination: party.camp_destination.is_some(),
            journey_count: journeys.len(),
            destination_matches,
            active_interval_count,
            actionable_actor,
            unsafe_member_count,
            evacuation,
        })
        .map_err(|reason| self.public_camp_coherence_error(party_id, reason))
    }

    pub(super) fn party_has_unresolved_public_encounter(&self, party_id: &str) -> bool {
        self.connection.db.strategic_encounter().iter().any(|row| {
            row.party_id == party_id && row.status == StrategicEncounterStatus::AwaitingChoice
        })
    }

    pub(super) fn active_public_narrative_challenge(
        &self,
        leader_id: u64,
    ) -> Option<BackendRoadChallenge> {
        let mut challenges = self
            .connection
            .db
            .backend_road_challenges()
            .iter()
            .filter(|row| row.owner_character_id == leader_id && row.open && row.active)
            .collect::<Vec<_>>();
        challenges.sort_by(|left, right| {
            left.absolute_minute
                .cmp(&right.absolute_minute)
                .then_with(|| left.id.cmp(&right.id))
        });
        challenges.into_iter().next()
    }

    pub(super) fn party_has_public_travel_interruption(
        &self,
        party_id: &str,
        leader_id: u64,
    ) -> bool {
        self.party_has_unresolved_public_encounter(party_id)
            || self.active_public_narrative_challenge(leader_id).is_some()
    }

    pub(super) fn travel_camps(&mut self, party_id: &str) -> Result<JourneyTravelOutcome, String> {
        let result = (|| {
            for _ in 0..MAX_CAMPS_PER_LEG {
                let party = self.party_by_id(party_id)?;
                if party.camp_destination.is_none() {
                    self.metrics.travel_legs += 1;
                    return Ok(JourneyTravelOutcome::Completed);
                }
                if let Some(challenge) = self.active_public_narrative_challenge(party.leader_id) {
                    let Some((leader_id, leader_agent)) = self.current_leader(party_id) else {
                        return self.record_journey_hold(
                            party_id,
                            "journey_narrative_encounter",
                            "narrative_encounter_has_no_actionable_leader",
                        );
                    };
                    let profile = self
                        .profiles
                        .get(leader_agent as usize)
                        .ok_or("narrative encounter leader profile is unavailable")?;
                    let policy_choice = match select_public_narrative_encounter_choice(
                        &challenge.presentation_json,
                        profile,
                    ) {
                        Ok(Some(choice)) => choice,
                        Ok(None) => {
                            self.encounter_event(
                                leader_agent,
                                CoreLoopEventKind::Encounter,
                                party_id,
                                &challenge.id,
                                format!(
                                "kind=narrative;id={};revision={};status=held_no_available_choice",
                                bounded_event_field(&challenge.id),
                                challenge.revision,
                            ),
                            );
                            return self.record_journey_hold(
                                party_id,
                                "journey_narrative_encounter",
                                "narrative_encounter_has_no_available_public_choice",
                            );
                        }
                        Err(_) => {
                            self.encounter_event(
                                leader_agent,
                                CoreLoopEventKind::Encounter,
                                party_id,
                                &challenge.id,
                                format!(
                                "kind=narrative;id={};revision={};status=held_invalid_public_presentation",
                                bounded_event_field(&challenge.id),
                                challenge.revision,
                            ),
                            );
                            return self.record_journey_hold(
                                party_id,
                                "journey_narrative_encounter",
                                "narrative_encounter_public_presentation_invalid",
                            );
                        }
                    };
                    let choice = policy_choice.choice.clone();
                    let action_id = format!(
                        "sim-road-{}-r{}",
                        blake3::hash(challenge.id.as_bytes()).to_hex(),
                        challenge.revision,
                    );
                    let challenge_id = challenge.id.clone();
                    let revision = challenge.revision;
                    let result = reducer_call!(self, ReducerOperation::ResolveErrantryRoadChallenge, |cb| self
                        .connection
                        .reducers
                        .resolve_errantry_road_challenge_then(
                            leader_id,
                            challenge_id.clone(),
                            revision,
                            choice.clone(),
                            action_id.clone(),
                            cb,
                        ));
                    self.call(result)?;
                    self.observe_deaths();
                    self.metrics.encounters = self.metrics.encounters.saturating_add(1);
                    self.encounter_event(
                        leader_agent,
                        CoreLoopEventKind::Encounter,
                        party_id,
                        &challenge_id,
                        format!(
                        "kind=narrative;id={};revision={revision};choice={};status=resolved;reason={};visible_alternatives={};eligible_meaningful_alternatives={}",
                        bounded_event_field(&challenge_id),
                        bounded_event_field(&choice),
                        policy_choice.reason,
                        bounded_event_field(&policy_choice.visible_alternatives.join(",")),
                        bounded_event_field(
                            &policy_choice.eligible_meaningful_alternatives.join(",")
                        ),
                        ),
                    );
                    continue;
                }
                let remaining_before = party.camp_remaining_minutes;
                let Some((travel_actor, travel_agent, _)) =
                    self.expedition_recovery_actor(party_id)
                else {
                    self.observe_deaths();
                    return self.record_journey_hold(
                        party_id,
                        "journey_stalled",
                        "journey_held_no_actionable_actor",
                    );
                };
                let pending_encounter = {
                    let table = self.connection.db.strategic_encounter();
                    table.iter().find(|row| {
                        row.party_id == party_id
                            && row.status == StrategicEncounterStatus::AwaitingChoice
                    })
                };
                if let Some(encounter) = pending_encounter {
                    self.metrics.encounters += 1;
                    if encounter.run_ineligibility.is_none() {
                        self.metrics.encounter_escape_eligible += 1;
                    } else {
                        self.metrics.encounter_escape_ineligible += 1;
                    }
                    let evacuation = self.public_journey_is_evacuation(party_id);
                    let policy_choice = select_expedition_encounter_choice(
                        &encounter.available_choices,
                        evacuation,
                    )
                    .ok_or("encounter offers no protective evacuation choice")?;
                    let choice = policy_choice.choice;
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
                    let action_id =
                        format!("strategic-sim:{encounter_id}:{encounter_revision}:{choice}");
                    let result = reducer_call!(self, ReducerOperation::ResolveStrategicEncounter, |cb| self
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
                    self.encounter_event(
                        self.current_leader(party_id).map_or(0, |(_, agent)| agent),
                        CoreLoopEventKind::Encounter,
                        party_id,
                        &encounter_id,
                        format!(
                        "id={encounter_id};choice={choice};reason={};evacuation={evacuation};run_eligible={};available_choices={};outcome={resolved_outcome:?}",
                        policy_choice.reason,
                        encounter.run_ineligibility.is_none(),
                        encounter.available_choices.join(","),
                        ),
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
                            let leg_members_before =
                                self.expedition_member_observations(party_id)?;
                            let leg_supplies_before = self.expedition_supplies(party_id);
                            let result = reducer_call!(self, ReducerOperation::ContinueCampTravel, |cb| self
                                .connection
                                .reducers
                                .continue_camp_travel_then(continue_actor, cb));
                            match classify_camp_continuation(result) {
                                Ok(CampContinuationOutcome::Advanced) => {}
                                Ok(CampContinuationOutcome::DeferredForDaylightWindow) => {
                                    return Ok(JourneyTravelOutcome::DeferredForDaylightWindow);
                                }
                                Err(error) => self.call(Err(error))?,
                            }
                            self.observe_deaths();
                            let leg_members_after =
                                self.expedition_member_observations(party_id)?;
                            let leg_supplies_after = self.expedition_supplies(party_id);
                            self.emit_expedition_diagnostics(
                                ExpeditionDiagnosticContext {
                                    party_id,
                                    phase: "journey_leg",
                                    action: "continue_camp_travel",
                                    reason: &format!(
                                        "quest_leg_resumed_after_{choice}_{continue_actor_role}"
                                    ),
                                },
                                ExpeditionObservationChange {
                                    members_before: &leg_members_before,
                                    members_after: &leg_members_after,
                                    supplies_before: leg_supplies_before,
                                    supplies_after: leg_supplies_after,
                                },
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
                if self.public_journey_camp_state(party_id)? == PublicJourneyCampState::BetweenCamps
                {
                    let leg_members_before = self.expedition_member_observations(party_id)?;
                    let leg_supplies_before = self.expedition_supplies(party_id);
                    let result = reducer_call!(self, ReducerOperation::ContinueCampTravel, |cb| self
                        .connection
                        .reducers
                        .continue_camp_travel_then(travel_actor, cb));
                    match classify_camp_continuation(result) {
                        Ok(CampContinuationOutcome::Advanced) => {}
                        Ok(CampContinuationOutcome::DeferredForDaylightWindow) => {
                            return Ok(JourneyTravelOutcome::DeferredForDaylightWindow);
                        }
                        Err(error) => self.call(Err(error))?,
                    }
                    self.observe_deaths();
                    let leg_members_after = self.expedition_member_observations(party_id)?;
                    let leg_supplies_after = self.expedition_supplies(party_id);
                    self.emit_expedition_diagnostics(
                        ExpeditionDiagnosticContext {
                            party_id,
                            phase: "journey_leg",
                            action: "continue_camp_travel",
                            reason: "continue_between_forecast_camps",
                        },
                        ExpeditionObservationChange {
                            members_before: &leg_members_before,
                            members_after: &leg_members_after,
                            supplies_before: leg_supplies_before,
                            supplies_after: leg_supplies_after,
                        },
                    );
                    self.event(
                        travel_agent,
                        CoreLoopEventKind::Travel,
                        format!(
                            "phase=between_camps_continue;party={};remaining_movement={}",
                            bounded_event_field(party_id),
                            self.party_by_id(party_id)?.camp_remaining_minutes,
                        ),
                    );
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
                let before_completed_elapsed = camp.completed_elapsed_minutes;
                let leader_before_rest = party.leader_id;
                let shelter = self.rest_at_camp_with_party_shelter(
                    travel_actor,
                    rest_minutes,
                    ReducerOperation::RestAtCamp,
                )?;
                self.observe_deaths();
                let camp_members_after = self.expedition_member_observations(party_id)?;
                let before_liveness = camp_members_before
                    .iter()
                    .map(|member| (member.character_id, member.alive))
                    .collect::<Vec<_>>();
                let after_liveness = camp_members_after
                    .iter()
                    .map(|member| (member.character_id, member.alive))
                    .collect::<Vec<_>>();
                let terminal_ids = public_alive_to_dead_ids(&before_liveness, &after_liveness);
                let terminal_state_change = !terminal_ids.is_empty();
                let before_member_elapsed = camp_members_before
                    .iter()
                    .map(|member| (member.character_id, member.elapsed_minutes))
                    .collect::<Vec<_>>();
                let after_member_elapsed = camp_members_after
                    .iter()
                    .map(|member| (member.character_id, member.elapsed_minutes))
                    .collect::<Vec<_>>();
                let terminal_rest_elapsed = public_terminal_rest_elapsed(
                    &terminal_ids,
                    &before_member_elapsed,
                    &after_member_elapsed,
                );
                let unsafe_after_rest = camp_members_after
                    .iter()
                    .filter(|member| expedition_member_needs_recovery(member))
                    .map(|member| member.agent_id)
                    .collect::<Vec<_>>();
                let camp_supplies_after = self.expedition_supplies(party_id);
                self.emit_expedition_diagnostics(
                    ExpeditionDiagnosticContext {
                        party_id,
                        phase: "journey_camp",
                        action: "rest_at_camp",
                        reason: if terminal_state_change {
                            "journey_terminal_state_reclassified"
                        } else if unsafe_after_rest.is_empty() {
                            "quest_leg_rest_complete"
                        } else {
                            "quest_suppressed_member_not_ready_after_camp"
                        },
                    },
                    ExpeditionObservationChange {
                        members_before: &camp_members_before,
                        members_after: &camp_members_after,
                        supplies_before: camp_supplies_before,
                        supplies_after: camp_supplies_after,
                    },
                );
                let after_rest_party = self.party_by_id(party_id)?;
                let after_rest_journeys = self
                    .connection
                    .db
                    .party_journey()
                    .iter()
                    .filter(|row| row.party_id == party_id)
                    .collect::<Vec<_>>();
                let (after_completed_elapsed, after_total_elapsed) = match (
                    after_rest_party.camp_destination.as_ref(),
                    after_rest_journeys.as_slice(),
                ) {
                    (Some(after_rest_destination), [after_rest_journey])
                        if &after_rest_journey.destination == after_rest_destination =>
                    {
                        (
                            after_rest_journey.completed_elapsed_minutes,
                            after_rest_journey.total_elapsed_minutes,
                        )
                    }
                    (None, []) if terminal_state_change => {
                        let terminal_rest_elapsed = terminal_rest_elapsed.ok_or(
                        "journey camp projection is incoherent: terminal rest elapsed is unavailable",
                    )?;
                        let after_completed_elapsed = before_completed_elapsed
                        .checked_add(terminal_rest_elapsed)
                        .ok_or("journey camp projection is incoherent: terminal rest elapsed overflowed")?;
                        (after_completed_elapsed, camp.total_elapsed_minutes)
                    }
                    _ => {
                        return Err(
                        "journey camp projection is incoherent: rest changed the active journey identity"
                            .into(),
                    );
                    }
                };
                let interrupted =
                    self.party_has_public_travel_interruption(party_id, after_rest_party.leader_id);
                let post_rest_progress = classify_post_rest_progress(
                before_completed_elapsed,
                rest_minutes,
                after_completed_elapsed,
                after_total_elapsed,
                interrupted,
                terminal_state_change,
            )
            .map_err(|reason| {
                format!(
                    "journey camp projection is incoherent: rest did not produce a safe forecast boundary: reason={reason}"
                )
            })?;
                let actual_rest_minutes = post_rest_progress.actual_rest_minutes();
                let post_rest_agent = self
                    .current_leader(party_id)
                    .map_or(travel_agent, |(_, agent)| agent);
                self.event(
                post_rest_agent,
                CoreLoopEventKind::Camp,
                format!(
                    "phase=post_rest;party={};completed_elapsed={after_completed_elapsed};total_elapsed={after_total_elapsed};requested_rest_minutes={rest_minutes};actual_rest_minutes={actual_rest_minutes};terminal_state_change={terminal_state_change};terminal_deaths={};leader_before={leader_before_rest};leader_after={};shelter={shelter:?};remaining_movement={}",
                    bounded_event_field(party_id),
                    terminal_ids.len(),
                    after_rest_party.leader_id,
                    after_rest_party.camp_remaining_minutes,
                ),
            );
                if actual_rest_minutes > 0 {
                    self.metrics.camp_stops = self.metrics.camp_stops.saturating_add(1);
                }
                if terminal_state_change {
                    if self.expedition_recovery_actor(party_id).is_none() {
                        return self.record_journey_hold(
                            party_id,
                            "journey_stalled_after_terminal_rest",
                            "journey_held_no_actionable_actor",
                        );
                    }
                    continue;
                }
                if interrupted {
                    continue;
                }
                let Some((continue_actor, agent, continue_actor_role)) =
                    self.expedition_recovery_actor(party_id)
                else {
                    return self.record_journey_hold(
                        party_id,
                        "journey_stalled_after_rest",
                        "journey_held_no_actionable_actor",
                    );
                };
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
                let result = reducer_call!(self, ReducerOperation::ContinueCampTravel, |cb| self
                    .connection
                    .reducers
                    .continue_camp_travel_then(continue_actor, cb));
                match classify_camp_continuation(result) {
                    Ok(CampContinuationOutcome::Advanced) => {}
                    Ok(CampContinuationOutcome::DeferredForDaylightWindow) => {
                        return Ok(JourneyTravelOutcome::DeferredForDaylightWindow);
                    }
                    Err(error) => self.call(Err(error))?,
                }
                self.observe_deaths();
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
                    ExpeditionDiagnosticContext {
                        party_id,
                        phase: "journey_leg",
                        action: "continue_camp_travel",
                        reason: &leg_reason,
                    },
                    ExpeditionObservationChange {
                        members_before: &leg_members_before,
                        members_after: &leg_members_after,
                        supplies_before: leg_supplies_before,
                        supplies_after: leg_supplies_after,
                    },
                );
                let after = self.party_by_id(party_id)?;
                if after.camp_destination.is_some()
                    && after.camp_remaining_minutes >= remaining_before
                {
                    self.metrics.stuck_detections += 1;
                    return Err("camp continuation made no progress".into());
                }
            }
            self.metrics.stuck_detections += 1;
            Err("camp bound exhausted".into())
        })();
        if let Err(detail) = &result {
            self.failure_recorder.record(CoreLoopError::operation(
                ReducerOperation::TravelCamps,
                detail.clone(),
            ));
        }
        result
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
        quests.retain(|quest| {
            self.public_party_contract_assessment(&party.id, quest)
                .eligible
        });
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
