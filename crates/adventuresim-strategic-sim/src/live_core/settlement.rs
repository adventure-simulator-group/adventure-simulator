impl LiveRunner {
    fn acquire_first_aid_material(
        &mut self,
        actor_id: u64,
        party_id: &str,
        settlement_id: &str,
        item_id: &str,
        agent: u32,
    ) -> Result<bool, String> {
        let Some(item) = self.item_definition(item_id) else {
            return Ok(false);
        };
        let Some(quote) = self.public_general_store_quote(actor_id, settlement_id, &item) else {
            return Ok(false);
        };
        let reserve = self
            .observable_medical_reserve(actor_id, settlement_id)
            .unwrap_or(0);
        let personal_spendable = self.personal_gold(actor_id).saturating_sub(reserve);
        let shortfall = quote.saturating_sub(personal_spendable);
        if !self.withdraw_stake_for_personal_purchase(actor_id, party_id, shortfall)?
            || self.personal_gold(actor_id).saturating_sub(reserve) < quote
        {
            return Ok(false);
        }
        let quantity_before = self.personal_item_quantity(actor_id, item_id);
        let result = reducer_call!(self, "purchase_first_aid_material", |cb| self
            .connection
            .reducers
            .finalize_merchant_trade_then(
                actor_id,
                settlement_id.to_owned(),
                vec![item_id.to_owned()],
                vec![1],
                vec![],
                vec![],
                false,
                cb,
            ));
        self.call(result)?;
        let purchased = self.personal_item_quantity(actor_id, item_id) > quantity_before;
        if purchased {
            self.event(
                agent,
                CoreLoopEventKind::Purchase,
                format!(
                    "first_aid_material={item_id};actor={actor_id};public_quote={quote};medical_reserve={reserve}"
                ),
            );
        }
        Ok(purchased)
    }

    /// Apply ordinary visible first aid before paying for convalescence. Open
    /// cuts and unsplinted fractures do not heal through rest alone.
    fn apply_visible_first_aid(&mut self, patient_id: u64, agent: u32) -> Result<(), String> {
        let patient = self
            .connection
            .db
            .backend_characters()
            .iter()
            .find(|row| row.id == patient_id && row.alive)
            .ok_or("missing first-aid patient")?;
        let Some(party_id) = patient.party_id.clone() else {
            return Ok(());
        };
        let settlement_id = patient.current_settlement_id.clone();
        let injuries = self
            .connection
            .db
            .limb_injury()
            .iter()
            .filter(|row| row.character_id == patient_id)
            .collect::<Vec<_>>();
        for injury in injuries {
            let procedure = if injury.cut_damage > 0.0 && !injury.bandaged {
                Some(("bandage", "bandage"))
            } else if injury.fracture_damage > 0.0 && injury.splint_inventory_item_id.is_none() {
                Some(("splint", "splint"))
            } else {
                None
            };
            let Some((procedure, item_id)) = procedure else {
                continue;
            };
            let mut candidates = self
                .connection
                .db
                .party_member()
                .iter()
                .filter(|member| member.party_id == party_id)
                .filter_map(|member| {
                    let character = self.connection.db.backend_characters().iter().find(|row| {
                        row.id == member.character_id
                            && row.alive
                            && row.current_settlement_id == settlement_id
                    })?;
                    let agent_id = self.character_ids.iter().position(|id| *id == character.id)?;
                    let profile = &self.profiles[agent_id];
                    Some((
                        character.id,
                        profile.build.role == BuildRole::Healer,
                        profile.initial_skills.surgery,
                        character.id != patient_id,
                    ))
                })
                .collect::<Vec<_>>();
            candidates.sort_by(|left, right| {
                right
                    .1
                    .cmp(&left.1)
                    .then_with(|| right.2.total_cmp(&left.2))
                    .then_with(|| right.3.cmp(&left.3))
                    .then_with(|| left.0.cmp(&right.0))
            });
            let mut actor = candidates
                .iter()
                .find(|candidate| self.personal_item_quantity(candidate.0, item_id) > 0)
                .map(|candidate| candidate.0);
            if actor.is_none() {
                if let (Some(candidate), Some(settlement_id)) =
                    (candidates.first(), settlement_id.as_deref())
                {
                    if self.acquire_first_aid_material(
                        candidate.0,
                        &party_id,
                        settlement_id,
                        item_id,
                        agent,
                    )? {
                        actor = Some(candidate.0);
                    }
                }
            }
            let Some(actor_id) = actor else {
                self.event(
                    agent,
                    CoreLoopEventKind::MedicalDecision,
                    format!(
                        "first_aid=deferred;patient={patient_id};procedure={procedure};reason=material_unavailable"
                    ),
                );
                continue;
            };
            let limb_slug = match injury.limb {
                LimbRegion::LeftArm => "left-arm",
                LimbRegion::RightArm => "right-arm",
                LimbRegion::LeftLeg => "left-leg",
                LimbRegion::RightLeg => "right-leg",
                LimbRegion::Chest => "chest",
                LimbRegion::Stomach => "stomach",
                LimbRegion::Head => "head",
            };
            let result = reducer_call!(self, "visible_first_aid", |cb| self
                .connection
                .reducers
                .treat_limb_then(
                    actor_id,
                    patient_id,
                    limb_slug.to_owned(),
                    procedure.to_owned(),
                    None,
                    false,
                    cb,
                ));
            self.call(result)?;
            self.event(
                agent,
                CoreLoopEventKind::Recover,
                format!(
                    "first_aid=completed;actor={actor_id};patient={patient_id};limb={limb_slug};procedure={procedure};authority=public_limb_injury"
                ),
            );
        }
        Ok(())
    }

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
            .backend_characters()
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
                let payer = self.connection.db.backend_characters().iter().find(|row| {
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
                let party_stake = self
                    .connection
                    .db
                    .party_stake()
                    .iter()
                    .find(|stake| stake.party_id == party_id && stake.character_id == payer.id)
                    .map_or(0, |stake| stake.value);
                let spendable = purse
                    .saturating_add(party_stake.min(party_treasury))
                    .saturating_sub(medical_reserve);
                (spendable >= sponsor_quote).then(|| SettlementRestSponsor {
                    payer_id: payer.id,
                    payer_agent_id,
                    purse,
                    medical_reserve,
                    spendable,
                    patient_contribution,
                    sponsor_quote,
                    party_treasury,
                    party_stake,
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

    pub(super) fn activity_observation(
        &self,
        character_id: u64,
    ) -> Result<ActivityObservation, String> {
        let condition = self
            .connection
            .db
            .backend_character_strategic_conditions()
            .iter()
            .find(|row| row.character_id == character_id)
            .ok_or("missing activity condition")?;
        let elapsed_minutes = self
            .connection
            .db
            .backend_character_times()
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
            .backend_characters()
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
            .backend_character_needs()
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
    fn observable_preparation_quote(
        &self,
        character_id: u64,
        settlement_id: &str,
        preparation_id: &str,
    ) -> Option<u64> {
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
            .find(|row| row.id == preparation_id)?;
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

    pub(super) fn observable_medical_quote(
        &self,
        character_id: u64,
        settlement_id: &str,
    ) -> Option<u64> {
        self.observable_preparation_quote(character_id, settlement_id, "oral_rehydration_draught")
    }

    fn public_physician_chart(&self, patient_id: u64) -> Option<BackendPhysiologyChart> {
        let patient = self
            .connection
            .db
            .backend_characters()
            .iter()
            .find(|row| row.id == patient_id && row.alive)?;
        let party_id = patient.party_id.as_deref()?;
        let settlement_id = patient.current_settlement_id.as_deref()?;
        if !self
            .connection
            .db
            .party_member()
            .iter()
            .any(|member| member.party_id == party_id && member.character_id == patient_id)
        {
            return None;
        }
        let patient_minute = self
            .connection
            .db
            .backend_character_times()
            .iter()
            .find(|row| row.character_id == patient_id)?
            .minutes;
        let mut charts = self
            .connection
            .db
            .backend_physiology_charts()
            .iter()
            .filter(|chart| {
                chart.patient_id == patient_id
                    && public_chart_is_fresh(patient_minute, chart.observed_at)
                    && chart.confidence_bps >= MIN_ACTIONABLE_PHYSIOLOGY_CONFIDENCE_BPS
                    && chart.gap_from.is_none()
                    && chart.gap_to.is_none()
                    && !chart.possible_diseases.is_empty()
            })
            .filter(|chart| self.character_ids.contains(&chart.observer_id))
            .filter(|chart| {
                self.connection
                    .db
                    .backend_characters()
                    .iter()
                    .find(|observer| observer.id == chart.observer_id)
                    .is_some_and(|observer| {
                        observer.alive
                            && observer.party_id.as_deref() == Some(party_id)
                            && observer.current_settlement_id.as_deref() == Some(settlement_id)
                            && self.connection.db.party_member().iter().any(|member| {
                                member.party_id == party_id && member.character_id == observer.id
                            })
                    })
            })
            .collect::<Vec<_>>();
        charts.sort_by(|left, right| {
            compare_public_chart_rank(
                left.confidence_bps,
                left.observed_at,
                left.observer_id,
                &left.id,
                right.confidence_bps,
                right.observed_at,
                right.observer_id,
                &right.id,
            )
        });
        charts.into_iter().next()
    }

    fn public_intervention_offers(
        &self,
        patient_id: u64,
        settlement_id: &str,
        chart: &BackendPhysiologyChart,
    ) -> Vec<PublicInterventionOffer> {
        if !chart.known_interventions.is_empty() {
            return Vec::new();
        }
        let mut offers = adventuresim_core::physiology::INTERVENTION_PROFILES
            .iter()
            .filter_map(|profile| {
                let score = public_intervention_score(&chart.possible_diseases, profile);
                if score <= 0 {
                    return None;
                }
                let inventory_item_id = self
                    .connection
                    .db
                    .inventory_item()
                    .iter()
                    .filter(|row| {
                        row.character_id == patient_id
                            && row.item_id == profile.preparation_id
                            && row.quantity == 1
                    })
                    .map(|row| row.id)
                    .min();
                let storefront_quote = self.observable_preparation_quote(
                    patient_id,
                    settlement_id,
                    profile.preparation_id,
                );
                if inventory_item_id.is_none() && storefront_quote.is_none() {
                    return None;
                }
                Some(PublicInterventionOffer {
                    preparation_id: profile.preparation_id.to_owned(),
                    profile_version: profile.version,
                    route: intervention_route_name(profile.route).to_owned(),
                    public_score_micropoints: score,
                    storefront_quote,
                    inventory_item_id,
                })
            })
            .collect::<Vec<_>>();
        offers.sort_by(|left, right| {
            right
                .inventory_item_id
                .is_some()
                .cmp(&left.inventory_item_id.is_some())
                .then_with(|| {
                    right
                        .public_score_micropoints
                        .cmp(&left.public_score_micropoints)
                })
                .then_with(|| left.storefront_quote.cmp(&right.storefront_quote))
                .then_with(|| left.preparation_id.cmp(&right.preparation_id))
        });
        offers
    }

    pub(super) fn observable_medical_reserve(
        &self,
        character_id: u64,
        settlement_id: &str,
    ) -> Option<u64> {
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
            .backend_character_training_schedules()
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
            .backend_character_training_schedules()
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
            .backend_character_training_schedules()
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
            .backend_character_training_schedules()
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
        self.apply_visible_first_aid(character_id, agent)?;
        let mut last_natural_rest_burden = None;
        let mut nonprogressing_natural_rests = 0_u8;
        for _ in 0..MAX_RECOVERY_ACTIONS {
            let mut character = self
                .connection
                .db
                .backend_characters()
                .iter()
                .find(|row| row.id == character_id)
                .ok_or("missing medical character")?;
            if !character.alive {
                self.medically_paused_schedules.remove(&character_id);
                if self.recorded_deaths.insert(character_id) {
                    let source = self
                        .connection
                        .db
                        .backend_character_deaths()
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
            let mut condition = self
                .connection
                .db
                .backend_character_strategic_conditions()
                .iter()
                .find(|row| row.character_id == character_id)
                .ok_or("missing medical condition")?;
            let mut symptomatic = self
                .connection
                .db
                .character_illness_status()
                .iter()
                .find(|row| row.character_id == character_id)
                .is_some_and(|row| row.symptomatic);
            if (condition.status != "ready" || symptomatic)
                && character.current_settlement_id.is_some()
            {
                // Installing the medical schedule may synchronize a lagging
                // character clock. Nothing clinical is selected or purchased
                // until every public input is re-read after that authority call.
                self.set_medical_rest_schedule(agent)?;
                self.observe_deaths();
                character = self
                    .connection
                    .db
                    .backend_characters()
                    .iter()
                    .find(|row| row.id == character_id)
                    .ok_or("missing medical character after schedule synchronization")?;
                if !character.alive {
                    self.medically_paused_schedules.remove(&character_id);
                    return Ok(false);
                }
                condition = self
                    .connection
                    .db
                    .backend_character_strategic_conditions()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .ok_or("missing medical condition after schedule synchronization")?;
                symptomatic = self
                    .connection
                    .db
                    .character_illness_status()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .is_some_and(|row| row.symptomatic);
            }
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
            let chart = self.public_physician_chart(character_id);
            let intervention_offers = settlement.as_deref().zip(chart.as_ref()).map_or_else(
                Vec::new,
                |(settlement, chart)| {
                    self.public_intervention_offers(character_id, settlement, chart)
                },
            );
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
            let selected_intervention = intervention_offers
                .iter()
                .find(|offer| {
                    let purchase_cost = if offer.inventory_item_id.is_some() {
                        0
                    } else {
                        let Some(quote) = offer.storefront_quote else {
                            return false;
                        };
                        quote
                    };
                    affordable_medical_rest_venue(
                        inn_available,
                        temple_available,
                        temple_food_covers_day,
                        purse,
                        purchase_cost,
                    )
                    .is_some()
                })
                .cloned();
            let observable_quote = selected_intervention.as_ref().map(|offer| {
                if offer.inventory_item_id.is_some() {
                    0
                } else {
                    offer
                        .storefront_quote
                        .expect("purchased intervention requires a public quote")
                }
            });
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
            let (choice, base_reason) = choose_medical_action(
                &condition.status,
                symptomatic,
                settlement.is_some(),
                herbalist_available
                    || selected_intervention
                        .as_ref()
                        .is_some_and(|offer| offer.inventory_item_id.is_some()),
                purse,
                observable_quote,
                natural_rest_venue,
                medicated_rest_venue,
            );
            let reason = if symptomatic && choice == MedicalChoice::RestNaturally {
                if chart.is_none() {
                    "chart_unavailable_or_low_confidence"
                } else if chart
                    .as_ref()
                    .is_some_and(|chart| !chart.known_interventions.is_empty())
                {
                    "active_public_intervention_supportive_rest"
                } else if intervention_offers.is_empty() {
                    "no_positive_stocked_public_intervention"
                } else if selected_intervention.is_none() {
                    "useful_public_intervention_unaffordable"
                } else {
                    base_reason
                }
            } else {
                base_reason
            };
            let chart_differential = chart.as_ref().map_or_else(
                || "none".to_owned(),
                |chart| {
                    chart
                        .possible_diseases
                        .iter()
                        .take(3)
                        .map(|row| {
                            format!(
                                "{}:{}",
                                bounded_event_field(&row.disease_id),
                                row.likelihood_bps
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(",")
                },
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
                    "status={};symptomatic={symptomatic};settlement={};purse={purse};clinician={};chart_confidence_band={};chart_confidence_bps={};public_differential={};preparation={};public_score_micropoints={};route={};storefront_quote={};purchase_cost={};rest_cost={};care_total={};rest_venue={};temple_food_kcal={visible_food_kcal:.0};temple_water_ml={visible_water_ml:.0};temple_food_covers_day={temple_food_covers_day};emergency_temple_rest={emergency_temple_rest};sponsor={};sponsor_purse={};sponsor_medical_reserve={};sponsor_spendable={};patient_contribution_quote={};sponsor_quote={};party_treasury={};sponsor_stake={};care_affordable={};action={choice:?};reason={reason}",
                    condition.status,
                    settlement.as_deref().unwrap_or("none"),
                    chart.as_ref().map_or_else(|| "none".into(), |chart| chart.observer_id.to_string()),
                    chart.as_ref().map_or("none", |chart| public_confidence_band(chart.confidence_bps)),
                    chart.as_ref().map_or_else(|| "none".into(), |chart| chart.confidence_bps.to_string()),
                    chart_differential,
                    selected_intervention.as_ref().map_or("none", |offer| offer.preparation_id.as_str()),
                    selected_intervention.as_ref().map_or_else(|| "none".into(), |offer| offer.public_score_micropoints.to_string()),
                    selected_intervention.as_ref().map_or("none", |offer| offer.route.as_str()),
                    selected_intervention.as_ref().and_then(|offer| offer.storefront_quote).map_or_else(|| "unavailable".into(), |quote| quote.to_string()),
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
            let survival_before = self
                .public_survival_observation(character_id)
                .unwrap_or_default();
            if choice == MedicalChoice::RestNaturally {
                let at_inn = natural_rest_venue.expect("natural rest choice requires a venue");
                let rest_started_at = self
                    .connection
                    .db
                    .backend_character_times()
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
                    let sponsor_personal_spendable = sponsor
                        .purse
                        .saturating_sub(sponsor.medical_reserve);
                    let stake_shortfall = sponsor
                        .sponsor_quote
                        .saturating_sub(sponsor_personal_spendable);
                    let party_id = character
                        .party_id
                        .as_deref()
                        .expect("sponsored rest requires a party");
                    if !self.withdraw_stake_for_personal_purchase(
                        sponsor.payer_id,
                        party_id,
                        stake_shortfall,
                    )? {
                        return Err("selected rest sponsor could not withdraw its own stake".into());
                    }
                    let payer_purse_before = self.personal_gold(sponsor.payer_id);
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
                        .backend_character_times()
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
                        .backend_character_strategic_conditions()
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
                            "sponsored_settlement_rest=completed;payer={};patient={character_id};settlement={};venue=inn;public_quote={public_quote};patient_contribution_quote={};sponsor_quote={};payer_medical_reserve={};payer_spendable={};party_treasury={};payer_party_stake={};patient_spend={patient_spend};sponsor_spend={sponsor_spend};actual_spend={actual_spend};payer_purse_before={payer_purse_before};payer_purse_after={payer_purse_after};patient_purse_before={patient_purse_before};patient_purse_after={patient_purse_after};condition_before={};condition_after={};symptomatic={symptomatic};exposure=public_transition_recorded_in_patient_recovery_event;requested_minutes=1440;actual_elapsed_minutes={actual_rest_minutes}",
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
                        .backend_character_times()
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
                let survival_after = self
                    .public_survival_observation(character_id)
                    .unwrap_or_default();
                if let Some(party_id) = character.party_id.as_deref() {
                    self.observe_survival_telemetry(party_id);
                }
                self.event(
                    agent,
                    CoreLoopEventKind::Recover,
                    format!(
                        "natural_recovery_requested_minutes=1440;natural_recovery_actual_minutes={actual_rest_minutes};venue={};emergency_free_rest={emergency_temple_rest};reason={reason};thermal_before={:.3};thermal_after={:.3};wetness_bps_before={};wetness_bps_after={};thermal_strain_before={};thermal_strain_after={};ammo_before={};ammo_after={};carried_load_kg_before={:.3};carried_load_kg_after={:.3};carry_capacity_kg_before={:.3};carry_capacity_kg_after={:.3};encumbrance_remaining_bps_before={};encumbrance_remaining_bps_after={};equipment_ready_before={};equipment_ready_after={};party_tent_quantity_before={};party_tent_quantity_after={}",
                        if at_inn { "inn" } else { "temple" },
                        survival_before.thermal,
                        survival_after.thermal,
                        survival_before.wetness_bps,
                        survival_after.wetness_bps,
                        survival_before.thermal_strain,
                        survival_after.thermal_strain,
                        survival_before.ammunition,
                        survival_after.ammunition,
                        survival_before.carried_load_kg,
                        survival_after.carried_load_kg,
                        survival_before.carry_capacity_kg,
                        survival_after.carry_capacity_kg,
                        survival_before.encumbrance_remaining_bps,
                        survival_after.encumbrance_remaining_bps,
                        survival_before.equipment_ready,
                        survival_after.equipment_ready,
                        survival_before.party_tent_quantity,
                        survival_after.party_tent_quantity,
                    ),
                );
                let condition_after = self
                    .connection
                    .db
                    .backend_character_strategic_conditions()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .ok_or("missing medical condition after natural recovery rest")?;
                let burden_after = condition_after.pain
                    + condition_after.blood_loss
                    + condition_after.fear
                    + condition_after.fatigue
                    + condition_after.hunger
                    + condition_after.thirst
                    + condition_after.thermal;
                if !symptomatic
                    && last_natural_rest_burden
                        .is_some_and(|before| burden_after >= before - 0.000_1)
                {
                    nonprogressing_natural_rests = nonprogressing_natural_rests.saturating_add(1);
                } else {
                    nonprogressing_natural_rests = 0;
                }
                last_natural_rest_burden = Some(burden_after);
                if nonprogressing_natural_rests >= 2 {
                    self.metrics.quests_suppressed_for_health += 1;
                    self.event(
                        agent,
                        CoreLoopEventKind::QuestSuppressed,
                        format!(
                            "status={};reason=natural_rest_not_improving_public_condition;burden={burden_after:.4};rests_without_progress={nonprogressing_natural_rests}",
                            condition_after.status
                        ),
                    );
                    return Ok(false);
                }
                continue;
            }

            // The chosen preparation comes only from the observer-safe chart,
            // public generic profiles, and visible herbalist stock. The
            // authoritative reducer owns private response and adverse effects.
            debug_assert_eq!(choice, MedicalChoice::BuyAndRest);
            let gold_before = purse;
            let intervention =
                selected_intervention.expect("purchase choice requires a public intervention");
            let clinician_id = chart
                .as_ref()
                .expect("purchase choice requires a public chart")
                .observer_id;
            let preparation_id = intervention.preparation_id.as_str();
            let preparation_inventory_id = if let Some(inventory_item_id) =
                intervention.inventory_item_id
            {
                inventory_item_id
            } else {
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
                        "observer={clinician_id};item={preparation_id};route={};public_score_micropoints={};storefront_quote={}",
                        intervention.route,
                        intervention.public_score_micropoints,
                        intervention.storefront_quote.expect("purchase requires a public quote"),
                    ),
                );
                self.connection
                    .db
                    .inventory_item()
                    .iter()
                    .filter(|row| row.character_id == character_id && row.item_id == preparation_id)
                    .map(|row| row.id)
                    .min()
                    .ok_or("preparation purchase produced no concrete patient item")?
            };
            let result = reducer_call!(self, "administer_preparation", |cb| self
                .connection
                .reducers
                .administer_preparation_then(
                    clinician_id,
                    character_id,
                    preparation_inventory_id,
                    intervention.profile_version,
                    intervention.route.clone(),
                    1_000,
                    None,
                    cb
                ));
            self.call(result)?;
            self.metrics.interventions_administered += 1;
            let actual_treatment_spend =
                gold_before.saturating_sub(self.personal_gold(character_id));
            self.metrics.treatment_gold_spent = self
                .metrics
                .treatment_gold_spent
                .saturating_add(actual_treatment_spend);
            self.observe_deaths();
            let post_administration = self
                .connection
                .db
                .backend_characters()
                .iter()
                .find(|row| row.id == character_id)
                .ok_or("missing patient after authoritative intervention")?;
            let post_administration_condition = self
                .connection
                .db
                .backend_character_strategic_conditions()
                .iter()
                .find(|row| row.character_id == character_id)
                .map_or_else(|| "unavailable".to_owned(), |row| row.status);
            self.event(
                agent,
                CoreLoopEventKind::AdministerPreparation,
                format!(
                    "observer={clinician_id};administered={preparation_id};profile={};route={};public_score_micropoints={};storefront_quote={};actual_spend={actual_treatment_spend};alive_after={};condition_after={};outcome={}",
                    intervention.profile_version,
                    intervention.route,
                    intervention.public_score_micropoints,
                    intervention.storefront_quote.map_or_else(|| "not_required".to_owned(), |quote| quote.to_string()),
                    post_administration.alive,
                    bounded_event_field(&post_administration_condition),
                    if post_administration.alive { "authoritative_reducer_accepted" } else { "authoritative_terminal_boundary" },
                ),
            );
            if !post_administration.alive {
                self.medically_paused_schedules.remove(&character_id);
                return Ok(false);
            }

            let at_inn =
                medicated_rest_venue.expect("purchase choice requires an affordable venue");
            let medical_rest_started_at = self
                .connection
                .db
                .backend_character_times()
                .iter()
                .find(|row| row.character_id == character_id)
                .ok_or("missing patient clock before medical recovery rest")?
                .minutes;
            let result = reducer_call!(self, "medical_recovery_rest", |cb| self
                .connection
                .reducers
                .rest_at_settlement_hours_then(character_id, 1_440, at_inn, cb));
            self.call(result)?;
            let medical_rest_ended_at = self
                .connection
                .db
                .backend_character_times()
                .iter()
                .find(|row| row.character_id == character_id)
                .ok_or("missing patient clock after medical recovery rest")?
                .minutes;
            let actual_medical_rest_minutes =
                medical_rest_ended_at.saturating_sub(medical_rest_started_at);
            self.metrics.treatment_rest_minutes = self
                .metrics
                .treatment_rest_minutes
                .saturating_add(actual_medical_rest_minutes);
            self.metrics.recovery_rests += 1;
            self.observe_deaths();
            let survival_after = self
                .public_survival_observation(character_id)
                .unwrap_or_default();
            if let Some(party_id) = character.party_id.as_deref() {
                self.observe_survival_telemetry(party_id);
            }
            self.event(
                agent,
                CoreLoopEventKind::Recover,
                format!(
                    "medical_rest_requested_minutes=1440;medical_rest_actual_minutes={actual_medical_rest_minutes};thermal_before={:.3};thermal_after={:.3};wetness_bps_before={};wetness_bps_after={};thermal_strain_before={};thermal_strain_after={};ammo_before={};ammo_after={};carried_load_kg_before={:.3};carried_load_kg_after={:.3};carry_capacity_kg_before={:.3};carry_capacity_kg_after={:.3};encumbrance_remaining_bps_before={};encumbrance_remaining_bps_after={};equipment_ready_before={};equipment_ready_after={};party_tent_quantity_before={};party_tent_quantity_after={}",
                    survival_before.thermal,
                    survival_after.thermal,
                    survival_before.wetness_bps,
                    survival_after.wetness_bps,
                    survival_before.thermal_strain,
                    survival_after.thermal_strain,
                    survival_before.ammunition,
                    survival_after.ammunition,
                    survival_before.carried_load_kg,
                    survival_after.carried_load_kg,
                    survival_before.carry_capacity_kg,
                    survival_after.carry_capacity_kg,
                    survival_before.encumbrance_remaining_bps,
                    survival_after.encumbrance_remaining_bps,
                    survival_before.equipment_ready,
                    survival_after.equipment_ready,
                    survival_before.party_tent_quantity,
                    survival_after.party_tent_quantity,
                ),
            );
            let after = self
                .connection
                .db
                .backend_characters()
                .iter()
                .find(|row| row.id == character_id)
                .ok_or("missing patient after medical rest")?;
            if !after.alive {
                continue;
            }
            let status = self
                .connection
                .db
                .backend_character_strategic_conditions()
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
                .backend_characters()
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
            let preferred_activity = format!("{:?}", profile.preferred_activity);
            let Some(venue) = self.settlement_activity_venue(character_id, committed_reserve)?
            else {
                self.event(
                    agent,
                    CoreLoopEventKind::Activity,
                    format_deferred_activity_detail(
                        &preferred_activity,
                        effective_activity,
                        &schedule,
                        fallback_reason,
                        committed_reserve,
                        &before,
                    ),
                );
                continue;
            };
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
            .backend_characters()
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
            .backend_character_times()
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
                    .backend_characters()
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
                    .backend_character_times()
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
                .backend_characters()
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
