impl LiveRunner {
    pub(super) fn personal_gold(&self, character_id: u64) -> u64 {
        self.connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| row.character_id == character_id && is_currency_id(&row.item_id))
            .map(|row| u64::from(row.quantity))
            .sum()
    }

    pub(super) fn settlement_rest_sponsor(
        &self,
        patient_id: u64,
        settlement_id: &str,
        public_quote: u64,
    ) -> Option<SettlementRestSponsor> {
        let patient_purse = self.personal_gold(patient_id);
        if patient_purse >= public_quote {
            return None;
        }
        let patient_contribution = patient_purse.min(public_quote);
        let sponsor_quote = public_quote.saturating_sub(patient_contribution);
        let patient = self
            .connection
            .db
            .character()
            .iter()
            .find(|row| row.id == patient_id && row.alive)?;
        let party_id = patient.party_id.as_deref()?;
        if patient.current_settlement_id.as_deref() != Some(settlement_id)
            || !self
                .connection
                .db
                .party_member()
                .iter()
                .any(|member| member.party_id == party_id && member.character_id == patient_id)
        {
            return None;
        }
        let party_treasury = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party_id && is_currency_id(&row.item_id))
            .map(|row| u64::from(row.quantity))
            .sum();
        let mut options = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|member| member.party_id == party_id && member.character_id != patient_id)
            .filter_map(|member| {
                let payer = self.connection.db.character().iter().find(|row| {
                    row.id == member.character_id
                        && row.alive
                        && row.current_settlement_id.as_deref() == Some(settlement_id)
                })?;
                let payer_agent_id =
                    self.character_ids.iter().position(|id| *id == payer.id)? as u32;
                let purse = self.personal_gold(payer.id);
                let medical_reserve = self
                    .observable_medical_reserve(payer.id, settlement_id)
                    .unwrap_or(0);
                let spendable = purse.saturating_sub(medical_reserve);
                (spendable >= sponsor_quote).then(|| SettlementRestSponsor {
                    payer_id: payer.id,
                    payer_agent_id,
                    purse,
                    medical_reserve,
                    spendable,
                    patient_contribution,
                    sponsor_quote,
                    party_treasury,
                    party_stake: self
                        .connection
                        .db
                        .party_stake()
                        .iter()
                        .find(|stake| stake.party_id == party_id && stake.character_id == payer.id)
                        .map_or(0, |stake| stake.value),
                })
            })
            .collect::<Vec<_>>();
        options.sort_by_key(|option| {
            (
                std::cmp::Reverse(option.spendable),
                option.payer_id,
                option.payer_agent_id,
            )
        });
        options.into_iter().next()
    }

    pub(super) fn activity_observation(&self, character_id: u64) -> Result<ActivityObservation, String> {
        let condition = self
            .connection
            .db
            .character_strategic_condition()
            .iter()
            .find(|row| row.character_id == character_id)
            .ok_or("missing activity condition")?;
        let elapsed_minutes = self
            .connection
            .db
            .character_time()
            .iter()
            .find(|row| row.character_id == character_id)
            .ok_or("missing activity clock")?
            .minutes;
        let (visible_food_kcal, visible_water_ml) = self.visible_rest_supplies(character_id);
        Ok(ActivityObservation {
            personal_gold_coin: self.personal_gold(character_id),
            condition_status: condition.status,
            hunger: condition.hunger,
            thirst: condition.thirst,
            food_days: condition.food_days,
            water_days: condition.water_days,
            visible_food_kcal,
            visible_water_ml,
            elapsed_minutes,
        })
    }

    /// Total concrete food energy and water volume visible to the character
    /// for a non-inn rest. Public food lots expose nutrition, while public
    /// needs and party state expose physiological, carried, and pooled water.
    pub(super) fn visible_rest_supplies(&self, character_id: u64) -> (f32, f32) {
        let Some(character) = self
            .connection
            .db
            .character()
            .iter()
            .find(|row| row.id == character_id)
        else {
            return (0.0, 0.0);
        };
        let party_id = character.party_id;
        let personal_ids = self
            .connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| row.character_id == character_id)
            .map(|row| row.id)
            .collect::<HashSet<_>>();
        let party_ids = party_id.as_deref().map_or_else(HashSet::new, |party_id| {
            self.connection
                .db
                .party_inventory_item()
                .iter()
                .filter(|row| row.party_id == party_id)
                .map(|row| row.id)
                .collect()
        });
        let stored_food_kcal = self
            .connection
            .db
            .food_lot()
            .iter()
            .filter(|lot| {
                lot.inventory_item_id
                    .is_some_and(|id| personal_ids.contains(&id))
                    || lot
                        .party_inventory_item_id
                        .is_some_and(|id| party_ids.contains(&id))
            })
            .map(|lot| lot.nutrition_kcal.max(0.0))
            .sum::<f32>();
        let needs = self
            .connection
            .db
            .character_needs()
            .iter()
            .find(|row| row.character_id == character_id);
        let physiological_food = needs
            .as_ref()
            .map_or(0.0, |row| row.food_balance_kcal.max(0.0));
        let personal_water = needs.as_ref().map_or(0.0, |row| {
            row.water_balance_ml.max(0.0) + row.carried_water_ml.max(0.0)
        });
        let party_water = party_id.as_deref().map_or(0.0, |party_id| {
            self.connection
                .db
                .party()
                .iter()
                .find(|row| row.id == party_id)
                .map_or(0.0, |row| row.pooled_water_ml.max(0.0))
        });
        (
            physiological_food + stored_food_kcal,
            personal_water + party_water,
        )
    }

    /// Reproduce the herbalist storefront/reducer quote from the same item
    /// definition and gateway-projected local-problem modifier visible to a
    /// player. No local-problem authority or infection state is transported.
    pub(super) fn observable_medical_quote(&self, character_id: u64, settlement_id: &str) -> Option<u64> {
        let settlement = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|row| row.id == settlement_id)?;
        if !settlement
            .economy
            .services
            .contains(&SettlementService::Herbalist)
        {
            return None;
        }
        let preparation = self
            .connection
            .db
            .item()
            .iter()
            .find(|row| row.id == "oral_rehydration_draught")?;
        // This is the generated-client equivalent of the public
        // `storefront_stocks(..., Herbalist, ..., Medication)` predicate:
        // the service must exist and the medication's Herbs category must be
        // present in visible settlement stock.
        if !observable_herbalist_stocks_medication(
            true,
            preparation.kind == ItemKind::Medication,
            settlement
                .economy
                .stock
                .iter()
                .any(|row| row.category == StockCategory::Herbs),
        ) {
            return None;
        }
        let buy_bps = self
            .connection
            .db
            .backend_local_problem_trade_effects()
            .iter()
            .find(|row| row.character_id == character_id && row.settlement_id == settlement_id)?
            .buy_bps;
        let base = adventuresim_core::strategic_economy::merchant_buy_price(
            preparation.base_value.unwrap_or(1),
        );
        Some(u64::from(adventuresim_core::local_problem::adjust_price(
            base, buy_bps,
        )))
    }

    pub(super) fn observable_medical_reserve(&self, character_id: u64, settlement_id: &str) -> Option<u64> {
        let quote = self.observable_medical_quote(character_id, settlement_id)?;
        let settlement = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|row| row.id == settlement_id)?;
        let (food_kcal, _) = self.visible_rest_supplies(character_id);
        let at_inn = affordable_medical_rest_venue(
            settlement
                .economy
                .services
                .contains(&SettlementService::Inn),
            settlement
                .economy
                .services
                .contains(&SettlementService::Temple),
            temple_food_covers_one_day(food_kcal),
            u64::MAX,
            quote,
        )?;
        if at_inn {
            quote.checked_add(adventuresim_core::strategic_economy::inn_full_board_cost(
                1_440,
            )?)
        } else {
            Some(quote)
        }
    }

    pub(super) fn set_medical_rest_schedule(&mut self, agent: u32) -> Result<(), String> {
        let character_id = self.character_ids[agent as usize];
        if self.medically_paused_schedules.contains(&character_id) {
            return Ok(());
        }
        let schedule = medical_rest_schedule();
        let result = reducer_call!(self, "pause_schedule_for_treatment", |cb| self
            .connection
            .reducers
            .update_training_schedule_then(
                character_id,
                schedule.clone(),
                medical_rest_schedule(),
                cb
            ));
        self.call(result)?;
        let installed = self
            .connection
            .db
            .character_training_schedule()
            .iter()
            .find(|row| row.character_id == character_id)
            .is_some_and(|row| row.downtime == schedule);
        if !installed {
            return Err("medical rest schedule was not authoritatively installed".into());
        }
        self.medically_paused_schedules.insert(character_id);
        Ok(())
    }

    pub(super) fn restore_profile_schedule(&mut self, agent: u32) -> Result<(), String> {
        let character_id = self.character_ids[agent as usize];
        if !self.medically_paused_schedules.contains(&character_id) {
            return Ok(());
        }
        let schedule = live_schedule(&self.profiles[agent as usize]);
        let result = reducer_call!(self, "restore_schedule_after_treatment", |cb| self
            .connection
            .reducers
            .update_training_schedule_then(
                character_id,
                schedule.clone(),
                medical_rest_schedule(),
                cb
            ));
        self.call(result)?;
        let restored = self
            .connection
            .db
            .character_training_schedule()
            .iter()
            .find(|row| row.character_id == character_id)
            .is_some_and(|row| row.downtime == schedule);
        if !restored {
            return Err("profile schedule was not authoritatively restored".into());
        }
        self.medically_paused_schedules.remove(&character_id);
        Ok(())
    }

    pub(super) fn install_activity_schedule(
        &mut self,
        character_id: u64,
        schedule: &ScheduleAllocation,
    ) -> Result<(), String> {
        let already_installed = self
            .connection
            .db
            .character_training_schedule()
            .iter()
            .find(|row| row.character_id == character_id)
            .is_some_and(|row| row.downtime == *schedule);
        if !already_installed {
            let result = reducer_call!(self, "install_activity_schedule", |cb| self
                .connection
                .reducers
                .update_training_schedule_then(
                    character_id,
                    schedule.clone(),
                    medical_rest_schedule(),
                    cb
                ));
            self.call(result)?;
        }
        let installed = self
            .connection
            .db
            .character_training_schedule()
            .iter()
            .find(|row| row.character_id == character_id)
            .is_some_and(|row| row.downtime == *schedule);
        if !installed {
            return Err("activity schedule was not authoritatively installed".into());
        }
        Ok(())
    }

    /// Observe only public condition plus the trusted one-shot herbalist result,
    /// filtered by the simulator-owned patient ID.
    /// Private infection episodes are deliberately absent from this policy.
    pub(super) fn ensure_medically_safe(&mut self, agent: u32) -> Result<bool, String> {
        let character_id = self.character_ids[agent as usize];
        for _ in 0..MAX_RECOVERY_ACTIONS {
            let character = self
                .connection
                .db
                .character()
                .iter()
                .find(|row| row.id == character_id)
                .ok_or("missing medical character")?;
            if !character.alive {
                self.medically_paused_schedules.remove(&character_id);
                if self.recorded_deaths.insert(character_id) {
                    let source = self
                        .connection
                        .db
                        .character_death()
                        .iter()
                        .find(|row| row.character_id == character_id)
                        .map(|row| row.source);
                    if source == Some(DeathSource::Disease) {
                        self.metrics.disease_deaths += 1;
                    }
                    self.event(
                        agent,
                        CoreLoopEventKind::Death,
                        format!("terminal medical state;source={source:?}"),
                    );
                }
                return Ok(false);
            }
            let condition = self
                .connection
                .db
                .character_strategic_condition()
                .iter()
                .find(|row| row.character_id == character_id)
                .ok_or("missing medical condition")?;
            let symptomatic = self
                .connection
                .db
                .character_illness_status()
                .iter()
                .find(|row| row.character_id == character_id)
                .is_some_and(|row| row.symptomatic);
            let settlement = character.current_settlement_id.clone();
            let herbalist_available = settlement.as_ref().is_some_and(|settlement| {
                self.connection
                    .db
                    .settlement()
                    .iter()
                    .find(|row| row.id == *settlement)
                    .is_some_and(|row| row.economy.services.contains(&SettlementService::Herbalist))
            });
            let purse = self.personal_gold(character_id);
            let observable_quote = settlement
                .as_deref()
                .and_then(|settlement| self.observable_medical_quote(character_id, settlement));
            let (inn_available, temple_available) =
                settlement.as_ref().map_or((false, false), |settlement_id| {
                    self.connection
                        .db
                        .settlement()
                        .iter()
                        .find(|row| row.id == *settlement_id)
                        .map_or((false, false), |row| {
                            (
                                row.economy.services.contains(&SettlementService::Inn),
                                row.economy.services.contains(&SettlementService::Temple),
                            )
                        })
                });
            let (visible_food_kcal, visible_water_ml) = self.visible_rest_supplies(character_id);
            let temple_food_covers_day = temple_food_covers_one_day(visible_food_kcal);
            let inn_cost = adventuresim_core::strategic_economy::inn_full_board_cost(1_440);
            let self_funded_natural_rest_venue = affordable_medical_rest_venue(
                inn_available,
                temple_available,
                temple_food_covers_day,
                purse,
                0,
            );
            let rest_sponsor = if !symptomatic && inn_available {
                settlement.as_deref().and_then(|settlement_id| {
                    inn_cost.and_then(|quote| {
                        self.settlement_rest_sponsor(character_id, settlement_id, quote)
                    })
                })
            } else {
                None
            };
            let emergency_temple_rest = self_funded_natural_rest_venue.is_none()
                && rest_sponsor.is_none()
                && temple_available;
            let natural_rest_venue = self_funded_natural_rest_venue
                .or_else(|| rest_sponsor.as_ref().map(|_| true))
                .or_else(|| emergency_temple_rest.then_some(false));
            let medicated_rest_venue = observable_quote.and_then(|quote| {
                affordable_medical_rest_venue(
                    inn_available,
                    temple_available,
                    temple_food_covers_day,
                    purse,
                    quote,
                )
            });
            let required_rest_cost = medicated_rest_venue
                .or(natural_rest_venue)
                .map(|at_inn| {
                    if at_inn {
                        adventuresim_core::strategic_economy::inn_full_board_cost(1_440)
                    } else {
                        Some(0)
                    }
                })
                .flatten();
            let observable_care_total =
                observable_quote
                    .zip(medicated_rest_venue)
                    .and_then(|(quote, at_inn)| {
                        let rest = if at_inn {
                            adventuresim_core::strategic_economy::inn_full_board_cost(1_440)?
                        } else {
                            0
                        };
                        quote.checked_add(rest)
                    });
            let (choice, reason) = choose_medical_action(
                &condition.status,
                symptomatic,
                settlement.is_some(),
                herbalist_available,
                purse,
                observable_quote,
                natural_rest_venue,
                medicated_rest_venue,
            );
            let selected_rest_venue = match choice {
                MedicalChoice::RestNaturally => natural_rest_venue,
                MedicalChoice::BuyAndRest => medicated_rest_venue,
                MedicalChoice::Ready | MedicalChoice::SuppressQuest => None,
            };
            self.event(
                agent,
                CoreLoopEventKind::MedicalDecision,
                format!(
                    "status={};symptomatic={symptomatic};settlement={};purse={purse};observable_quote={};rest_cost={};care_total={};rest_venue={};temple_food_kcal={visible_food_kcal:.0};temple_water_ml={visible_water_ml:.0};temple_food_covers_day={temple_food_covers_day};emergency_temple_rest={emergency_temple_rest};sponsor={};sponsor_purse={};sponsor_medical_reserve={};sponsor_spendable={};patient_contribution_quote={};sponsor_quote={};party_treasury={};sponsor_stake={};care_affordable={};action={choice:?};reason={reason}",
                    condition.status,
                    settlement.as_deref().unwrap_or("none"),
                    observable_quote.map_or_else(|| "unavailable".into(), |quote| quote.to_string()),
                    required_rest_cost.map_or_else(|| "unavailable".into(), |cost| cost.to_string()),
                    observable_care_total.map_or_else(|| "unavailable".into(), |cost| cost.to_string()),
                    selected_rest_venue.map_or("unavailable", |at_inn| if at_inn { "inn" } else { "temple" }),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.payer_id.to_string()),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.purse.to_string()),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.medical_reserve.to_string()),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.spendable.to_string()),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.patient_contribution.to_string()),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.sponsor_quote.to_string()),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.party_treasury.to_string()),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.party_stake.to_string()),
                    observable_quote.is_some() && medicated_rest_venue.is_some(),
                ),
            );
            if choice == MedicalChoice::Ready {
                self.restore_profile_schedule(agent)?;
                return Ok(true);
            }
            if choice == MedicalChoice::SuppressQuest {
                self.metrics.quests_suppressed_for_health += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!("status={};reason={reason}", condition.status),
                );
                return Ok(false);
            }
            let Some(settlement) = settlement else {
                unreachable!("a missing settlement is handled as quest suppression");
            };
            self.set_medical_rest_schedule(agent)?;
            if choice == MedicalChoice::RestNaturally {
                let at_inn = natural_rest_venue.expect("natural rest choice requires a venue");
                let rest_started_at = self
                    .connection
                    .db
                    .character_time()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .ok_or("missing patient clock before natural recovery rest")?
                    .minutes;
                let actual_rest_minutes = if at_inn
                    && purse < inn_cost.expect("inn venue requires a public quote")
                {
                    let sponsor = rest_sponsor
                        .as_ref()
                        .expect("unaffordable inn venue requires a selected sponsor");
                    let payer_purse_before = sponsor.purse;
                    let patient_purse_before = purse;
                    let condition_before = condition.status.clone();
                    let public_quote = inn_cost.expect("sponsored inn rest requires a quote");
                    let result = reducer_call!(self, "sponsor_party_member_inn_rest", |cb| self
                        .connection
                        .reducers
                        .sponsor_party_member_inn_rest_then(
                            sponsor.payer_id,
                            character_id,
                            settlement.clone(),
                            public_quote,
                            cb
                        ));
                    self.call(result)?;
                    let rest_ended_at = self
                        .connection
                        .db
                        .character_time()
                        .iter()
                        .find(|row| row.character_id == character_id)
                        .ok_or("missing patient clock after sponsored recovery rest")?
                        .minutes;
                    let actual_rest_minutes = rest_ended_at.saturating_sub(rest_started_at);
                    let payer_purse_after = self.personal_gold(sponsor.payer_id);
                    let patient_purse_after = self.personal_gold(character_id);
                    let sponsor_spend = payer_purse_before.saturating_sub(payer_purse_after);
                    let patient_spend = patient_purse_before.saturating_sub(patient_purse_after);
                    let actual_spend = sponsor_spend.saturating_add(patient_spend);
                    let condition_after = self
                        .connection
                        .db
                        .character_strategic_condition()
                        .iter()
                        .find(|row| row.character_id == character_id)
                        .map_or_else(|| "unavailable".into(), |row| row.status);
                    self.metrics.sponsored_settlement_rests =
                        self.metrics.sponsored_settlement_rests.saturating_add(1);
                    self.metrics.sponsored_settlement_rest_gold_spent = self
                        .metrics
                        .sponsored_settlement_rest_gold_spent
                        .saturating_add(sponsor_spend);
                    self.metrics.sponsored_settlement_rest_requested_minutes = self
                        .metrics
                        .sponsored_settlement_rest_requested_minutes
                        .saturating_add(1_440);
                    self.metrics.sponsored_settlement_rest_elapsed_minutes = self
                        .metrics
                        .sponsored_settlement_rest_elapsed_minutes
                        .saturating_add(actual_rest_minutes);
                    self.metrics.treatment_gold_spent = self
                        .metrics
                        .treatment_gold_spent
                        .saturating_add(actual_spend);
                    self.event(
                        sponsor.payer_agent_id,
                        CoreLoopEventKind::Recover,
                        format!(
                            "sponsored_settlement_rest=completed;payer={};patient={character_id};settlement={};venue=inn;public_quote={public_quote};patient_contribution_quote={};sponsor_quote={};payer_medical_reserve={};payer_spendable={};party_treasury={};payer_party_stake={};patient_spend={patient_spend};sponsor_spend={sponsor_spend};actual_spend={actual_spend};payer_purse_before={payer_purse_before};payer_purse_after={payer_purse_after};patient_purse_before={patient_purse_before};patient_purse_after={patient_purse_after};condition_before={};condition_after={};symptomatic={symptomatic};exposure=not_publicly_projected;requested_minutes=1440;actual_elapsed_minutes={actual_rest_minutes}",
                            sponsor.payer_id,
                            bounded_event_field(&settlement),
                            sponsor.patient_contribution,
                            sponsor.sponsor_quote,
                            sponsor.medical_reserve,
                            sponsor.spendable,
                            sponsor.party_treasury,
                            sponsor.party_stake,
                            bounded_event_field(&condition_before),
                            bounded_event_field(&condition_after),
                        ),
                    );
                    actual_rest_minutes
                } else {
                    let result = reducer_call!(self, "natural_illness_recovery_rest", |cb| self
                        .connection
                        .reducers
                        .rest_at_settlement_hours_then(character_id, 1_440, at_inn, cb));
                    self.call(result)?;
                    let rest_ended_at = self
                        .connection
                        .db
                        .character_time()
                        .iter()
                        .find(|row| row.character_id == character_id)
                        .ok_or("missing patient clock after natural recovery rest")?
                        .minutes;
                    rest_ended_at.saturating_sub(rest_started_at)
                };
                self.metrics.treatment_rest_minutes = self
                    .metrics
                    .treatment_rest_minutes
                    .saturating_add(actual_rest_minutes);
                self.metrics.recovery_rests += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::Recover,
                    format!(
                        "natural_recovery_requested_minutes=1440;natural_recovery_actual_minutes={actual_rest_minutes};venue={};emergency_free_rest={emergency_temple_rest};reason={reason}",
                        if at_inn { "inn" } else { "temple" }
                    ),
                );
                continue;
            }

            // NPCs react only to the public illness status. They may purchase a
            // pre-existing preparation and administer its versioned profile;
            // Physiology never diagnoses or crafts it.
            debug_assert_eq!(choice, MedicalChoice::BuyAndRest);
            let gold_before = purse;
            let preparation_id = "oral_rehydration_draught";
            let result = reducer_call!(self, "purchase_from_herbalist", |cb| self
                .connection
                .reducers
                .purchase_from_herbalist_then(
                    character_id,
                    settlement.clone(),
                    vec![preparation_id.into()],
                    vec![1],
                    cb
                ));
            self.call(result)?;
            self.metrics.preparations_purchased += 1;
            self.event(
                agent,
                CoreLoopEventKind::BuyMedication,
                format!(
                    "item={preparation_id};observable_quote={}",
                    observable_quote.expect("purchase choice requires a quote")
                ),
            );
            let preparation = self
                .connection
                .db
                .inventory_item()
                .iter()
                .find(|row| row.character_id == character_id && row.item_id == preparation_id)
                .ok_or("preparation purchase produced no concrete item")?;
            let result = reducer_call!(self, "administer_preparation", |cb| self
                .connection
                .reducers
                .administer_preparation_then(
                    character_id,
                    character_id,
                    preparation.id,
                    1,
                    "oral".into(),
                    1_000,
                    None,
                    cb
                ));
            self.call(result)?;
            self.metrics.interventions_administered += 1;
            self.event(
                agent,
                CoreLoopEventKind::AdministerPreparation,
                format!("administered={preparation_id};profile=1;route=oral"),
            );
            self.metrics.treatment_gold_spent +=
                gold_before.saturating_sub(self.personal_gold(character_id));

            let at_inn =
                medicated_rest_venue.expect("purchase choice requires an affordable venue");
            let result = reducer_call!(self, "medical_recovery_rest", |cb| self
                .connection
                .reducers
                .rest_at_settlement_hours_then(character_id, 1_440, at_inn, cb));
            self.call(result)?;
            self.metrics.treatment_rest_minutes += 1_440;
            self.metrics.recovery_rests += 1;
            self.event(
                agent,
                CoreLoopEventKind::Recover,
                "medical_rest_minutes=1440",
            );
            let after = self
                .connection
                .db
                .character()
                .iter()
                .find(|row| row.id == character_id)
                .ok_or("missing patient after medical rest")?;
            if !after.alive {
                continue;
            }
            let status = self
                .connection
                .db
                .character_strategic_condition()
                .iter()
                .find(|row| row.character_id == character_id)
                .ok_or("missing condition after medical rest")?
                .status;
            if status == "ready" {
                let symptomatic_after = self
                    .connection
                    .db
                    .character_illness_status()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .is_some_and(|row| row.symptomatic);
                if symptomatic_after {
                    continue;
                }
                self.restore_profile_schedule(agent)?;
                self.metrics.illness_recoveries += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::IllnessRecovered,
                    format!(
                        "recovery_context=public_symptoms;condition_before={};condition_after=ready;symptomatic_before={symptomatic};symptomatic_after={symptomatic_after}",
                        condition.status,
                    ),
                );
                return Ok(true);
            }
        }
        self.metrics.stuck_detections += 1;
        Err("medical recovery bound exhausted".into())
    }

    pub(super) fn settlement_activity_day(&mut self, leader_agent: u32) -> Result<(), String> {
        let leader = self.character_ids[leader_agent as usize];
        for agent in self.party_agents(leader)? {
            if !self.ensure_medically_safe(agent)? {
                continue;
            }
            self.maintain_equipment(agent)?;
            let character_id = self.character_ids[agent as usize];
            let before = self.activity_observation(character_id)?;
            let profile = self.profiles[agent as usize].clone();
            let settlement_id = self
                .connection
                .db
                .character()
                .iter()
                .find(|row| row.id == character_id)
                .and_then(|row| row.current_settlement_id)
                .ok_or("simulation character is not at a settlement")?;
            let inn_cost = adventuresim_core::strategic_economy::inn_full_board_cost(1_440);
            let committed_reserve = visible_activity_committed_reserve(
                before.personal_gold_coin,
                u64::from(profile.cash_reserve_target),
                self.observable_medical_reserve(character_id, &settlement_id),
                inn_cost,
            );
            let temple_food_covers_day = temple_food_covers_one_day(before.visible_food_kcal);
            let (schedule, effective_activity, fallback_reason) = activity_schedule_plan(
                &profile,
                temple_food_covers_day,
                before.personal_gold_coin,
                committed_reserve,
                inn_cost,
            );
            self.install_activity_schedule(character_id, &schedule)?;
            let venue = self.settlement_activity_venue(character_id, committed_reserve)?;
            let preferred_activity = format!("{:?}", profile.preferred_activity);
            let result = reducer_call!(self, "settlement_activity_rest", |cb| self
                .connection
                .reducers
                .rest_at_settlement_hours_then(character_id, 1_440, venue.at_inn(), cb));
            if let Err(error) = result {
                let error_category = safe_core_loop_failure(&error).0;
                self.event(
                    agent,
                    CoreLoopEventKind::Activity,
                    format_failed_activity_detail(
                        &preferred_activity,
                        effective_activity,
                        &schedule,
                        venue,
                        fallback_reason,
                        committed_reserve,
                        &before,
                        error_category,
                    ),
                );
                return self.call(Err(error));
            }
            let after = self.activity_observation(character_id)?;
            self.event(
                agent,
                CoreLoopEventKind::Activity,
                format_activity_detail(
                    &preferred_activity,
                    effective_activity,
                    &schedule,
                    venue,
                    fallback_reason,
                    committed_reserve,
                    &before,
                    &after,
                ),
            );
            self.metrics.activity_days += 1;
            self.ensure_medically_safe(agent)?;
        }
        Ok(())
    }

    /// NPCs use the same custody/rest/retrieval reducers and stable quotes as
    /// players, reserving current personal gold before entrusting work.
    pub(super) fn maintain_equipment(&mut self, agent: u32) -> Result<(), String> {
        let character_id = self.character_ids[agent as usize];
        let character = self
            .connection
            .db
            .character()
            .iter()
            .find(|row| row.id == character_id)
            .ok_or("missing maintenance character")?;
        let Some(settlement) = character.current_settlement_id.clone() else {
            return Ok(());
        };
        if !character.alive {
            return Ok(());
        }
        let repair_service_available = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|row| row.id == settlement)
            .is_some_and(|row| {
                row.economy.services.iter().any(|service| {
                    matches!(
                        service,
                        SettlementService::GeneralBlacksmith
                            | SettlementService::Weaponsmith
                            | SettlementService::Armorer
                            | SettlementService::Tailor
                    )
                })
            });
        if !repair_service_available {
            return Ok(());
        }
        let now = self
            .connection
            .db
            .character_time()
            .iter()
            .find(|row| row.character_id == character_id)
            .ok_or("missing maintenance clock")?
            .minutes;
        let medical_reserve = self.observable_medical_reserve(character_id, &settlement);
        let mut repair_budget = spending_budget_after_medical_reserve(
            self.personal_gold(character_id),
            medical_reserve,
        );

        let mut orders: Vec<_> = self
            .connection
            .db
            .repair_order()
            .iter()
            .filter(|row| row.owner_character_id == character_id && row.settlement_id == settlement)
            .collect();
        let mut reserved_quotes = self
            .connection
            .db
            .repair_order()
            .iter()
            .filter(|order| order.owner_character_id == character_id)
            .map(|order| (order.ready_at_minutes, order.id, order.quoted_cost))
            .collect::<Vec<_>>();
        reserved_quotes.sort_unstable();
        repair_budget = adventuresim_core::durability::repair_budget_after_reservations(
            repair_budget,
            &reserved_quotes
                .into_iter()
                .map(|(_, _, quote)| quote)
                .collect::<Vec<_>>(),
        );
        if orders.is_empty() {
            let smith = self
                .connection
                .db
                .settlement_smith()
                .iter()
                .find(|row| row.settlement_id == settlement)
                .ok_or("missing settlement smith services")?;
            let mut inventory: Vec<_> = self
                .connection
                .db
                .inventory_item()
                .iter()
                .filter(|row| row.character_id == character_id)
                .collect();
            inventory.sort_by_key(|row| row.id);
            for owned in inventory {
                let Some(definition) = self
                    .connection
                    .db
                    .item()
                    .iter()
                    .find(|row| row.id == owned.item_id)
                else {
                    continue;
                };
                let (skill, service) = match definition.kind {
                    ItemKind::Weapon | ItemKind::Shield => (smith.weaponsmith_skill, "weapons"),
                    ItemKind::Armor => (smith.armourer_skill, "armor"),
                    ItemKind::Clothing => (smith.tailor_skill, "clothing"),
                    _ => continue,
                };
                let Some(condition) = self
                    .connection
                    .db
                    .item_condition()
                    .iter()
                    .find(|row| row.inventory_item_id == owned.id)
                else {
                    continue;
                };
                let bins = [
                    condition.tier_1,
                    condition.tier_2,
                    condition.tier_3,
                    condition.tier_4,
                    condition.tier_5,
                ];
                let total = quantize_smithing_condition(bins.iter().sum());
                let red = quantize_smithing_condition(bins[2..].iter().sum());
                let repairable =
                    quantize_smithing_condition(bins.iter().take(skill as usize).sum());
                let quote = adventuresim_core::durability::repair_quote(
                    definition.base_value.unwrap_or(1),
                    repairable as f32 / SMITHING_DECISION_SCALE,
                );
                // Mild yellow wear is handled automatically by ordinary rest.
                if repairable > 0
                    && (red >= 20 || total >= 350)
                    && u64::from(quote) <= repair_budget
                {
                    let result = reducer_call!(self, "submit_item_for_repair", |cb| self
                        .connection
                        .reducers
                        .submit_item_for_repair_then(
                            character_id,
                            settlement.clone(),
                            service.to_string(),
                            owned.id,
                            cb
                        ));
                    self.call(result)?;
                    self.metrics.repair_submissions += 1;
                    repair_budget -= u64::from(quote);
                    self.event(
                        agent,
                        CoreLoopEventKind::SubmitRepair,
                        format!(
                            "item={};condition={:.3};smith={skill};quote={quote}",
                            owned.item_id,
                            1.0 - total as f32 / SMITHING_DECISION_SCALE
                        ),
                    );
                }
            }
            orders = self
                .connection
                .db
                .repair_order()
                .iter()
                .filter(|row| {
                    row.owner_character_id == character_id && row.settlement_id == settlement
                })
                .collect();
        }
        if orders.is_empty() {
            return Ok(());
        }
        orders.sort_by_key(|order| (order.ready_at_minutes, order.id));
        let mut retrieval_budget = spending_budget_after_medical_reserve(
            self.personal_gold(character_id),
            medical_reserve,
        );
        let affordable: Vec<_> = orders
            .into_iter()
            .filter(|order| {
                let cost = u64::from(order.quoted_cost);
                if cost <= retrieval_budget {
                    retrieval_budget -= cost;
                    true
                } else {
                    false
                }
            })
            .collect();
        if affordable.is_empty() {
            return Ok(());
        }
        let ready_at = affordable
            .iter()
            .map(|order| order.ready_at_minutes)
            .max()
            .unwrap_or(now);
        if ready_at > now {
            let mut remaining = ready_at - now;
            while remaining > 0 {
                let wait = remaining.min(1_440);
                let at_inn = self.settlement_rest_at_inn(character_id)?;
                let result = reducer_call!(self, "wait_for_repairs", |cb| self
                    .connection
                    .reducers
                    .rest_at_settlement_hours_then(character_id, wait, at_inn, cb));
                self.call(result)?;
                self.metrics.repair_wait_minutes += wait;
                self.event(
                    agent,
                    CoreLoopEventKind::WaitForRepair,
                    format!("minutes={wait};orders={}", affordable.len()),
                );
                self.observe_deaths();
                let alive = self
                    .connection
                    .db
                    .character()
                    .iter()
                    .find(|row| row.id == character_id)
                    .is_some_and(|row| row.alive);
                if !alive {
                    return Ok(());
                }
                if !self.ensure_medically_safe(agent)? {
                    return Ok(());
                }
                let current = self
                    .connection
                    .db
                    .character_time()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .ok_or("missing repair wait clock")?
                    .minutes;
                remaining = ready_at.saturating_sub(current);
            }
        }
        for order in affordable {
            let retrieval_character = self
                .connection
                .db
                .character()
                .iter()
                .find(|row| row.id == character_id)
                .ok_or("missing repair retrieval character")?;
            if retrieval_character.current_settlement_id.as_deref() != Some(&order.settlement_id) {
                return Err(format!(
                    "repair retrieval location changed: agent={agent};alive={};current={:?};origin={}",
                    retrieval_character.alive,
                    retrieval_character.current_settlement_id,
                    order.settlement_id
                ));
            }
            let current_medical_quote =
                self.observable_medical_reserve(character_id, &order.settlement_id);
            if !equipment_spend_is_still_affordable(
                self.personal_gold(character_id),
                current_medical_quote,
                u64::from(order.quoted_cost),
            ) {
                // Time and medical care can change both the purse and the
                // public local-problem quote while a smith holds the item.
                // Leave the completed order in custody for a later attempt.
                continue;
            }
            let result = reducer_call!(self, "retrieve_repaired_item", |cb| self
                .connection
                .reducers
                .retrieve_repaired_item_then(character_id, order.id, cb));
            self.call(result)?;
            self.metrics.repair_retrievals += 1;
            self.event(
                agent,
                CoreLoopEventKind::RetrieveRepair,
                format!(
                    "item={};order={};cost={}",
                    order.item_id, order.id, order.quoted_cost
                ),
            );
            if let Some(placement_id) = order.equipped_placement_id.as_deref() {
                let verified = self
                    .connection
                    .db
                    .character_equipped_item()
                    .iter()
                    .any(|row| {
                        row.character_id == character_id
                            && row.inventory_item_id == order.inventory_item_id
                            && row.placement_id == placement_id
                    })
                    && order.attachment_targets.iter().all(|target| {
                        self.connection.db.equipment_occupancy().iter().any(|row| {
                            row.inventory_item_id == order.inventory_item_id
                                && row.requirement_index == target.requirement_index
                                && row.parent_inventory_item_id
                                    == Some(target.parent_inventory_item_id)
                                && row.attachment_point_id
                                    == Some(target.attachment_point_id.clone())
                        })
                    });
                if !verified {
                    return Err("repaired equipped item was not authoritatively re-equipped".into());
                }
                self.event(
                    agent,
                    CoreLoopEventKind::Equip,
                    format!("repaired={}", order.item_id),
                );
            }
        }
        Ok(())
    }

}
