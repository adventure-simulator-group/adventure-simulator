impl LiveRunner {
    pub(super) fn party_agents(&self, leader: u64) -> Result<Vec<u32>, String> {
        let party = self.party_for(leader)?;
        let mut agents: Vec<_> = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|member| member.party_id == party.id)
            .filter(|member| {
                self.connection
                    .db
                    .character()
                    .iter()
                    .find(|row| row.id == member.character_id)
                    .is_some_and(|row| row.alive)
            })
            .filter_map(|member| {
                self.character_ids
                    .iter()
                    .position(|id| *id == member.character_id)
                    .map(|index| index as u32)
            })
            .collect();
        agents.sort_unstable();
        Ok(agents)
    }

    pub(super) fn unsafe_party_agents(&self, agents: &[u32]) -> Vec<u32> {
        let mut unsafe_agents = agents
            .iter()
            .copied()
            .filter(|agent| {
                let id = self.character_ids[*agent as usize];
                let alive = self
                    .connection
                    .db
                    .character()
                    .iter()
                    .find(|row| row.id == id)
                    .is_some_and(|row| row.alive);
                let ready = self
                    .connection
                    .db
                    .character_strategic_condition()
                    .iter()
                    .find(|row| row.character_id == id)
                    .is_some_and(|row| row.status == "ready")
                    && !self
                        .connection
                        .db
                        .character_illness_status()
                        .iter()
                        .find(|row| row.character_id == id)
                        .is_some_and(|row| row.symptomatic || row.critical);
                !alive || !ready
            })
            .collect::<Vec<_>>();
        unsafe_agents.sort_unstable();
        unsafe_agents
    }

    pub(super) fn expedition_member_observations(
        &self,
        party_id: &str,
    ) -> Result<Vec<ExpeditionMemberObservation>, String> {
        let mut member_ids = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|membership| membership.party_id == party_id)
            .map(|membership| membership.character_id)
            .collect::<Vec<_>>();
        member_ids.sort_unstable();
        member_ids
            .into_iter()
            .map(|character_id| {
                let agent_id = self
                    .character_ids
                    .iter()
                    .position(|id| *id == character_id)
                    .ok_or("expedition member is outside the simulator roster")?
                    as u32;
                let character = self
                    .connection
                    .db
                    .character()
                    .iter()
                    .find(|row| row.id == character_id)
                    .ok_or("expedition member projection is unavailable")?;
                let condition = self
                    .connection
                    .db
                    .character_strategic_condition()
                    .iter()
                    .find(|row| row.character_id == character_id);
                let illness = self
                    .connection
                    .db
                    .character_illness_status()
                    .iter()
                    .find(|row| row.character_id == character_id);
                let elapsed_minutes = self
                    .connection
                    .db
                    .character_time()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .map_or(0, |row| row.minutes);
                Ok(ExpeditionMemberObservation {
                    agent_id,
                    character_id,
                    alive: character.alive,
                    condition_status: condition
                        .as_ref()
                        .map_or_else(|| "unavailable".into(), |row| row.status.clone()),
                    hunger: condition.as_ref().map_or(0.0, |row| row.hunger),
                    thirst: condition.as_ref().map_or(0.0, |row| row.thirst),
                    food_days: condition.as_ref().map_or(0.0, |row| row.food_days),
                    water_days: condition.as_ref().map_or(0.0, |row| row.water_days),
                    symptomatic: illness.as_ref().is_some_and(|row| row.symptomatic),
                    critical: illness.as_ref().is_some_and(|row| row.critical),
                    elapsed_minutes,
                })
            })
            .collect()
    }

    pub(super) fn expedition_supplies(&self, party_id: &str) -> ExpeditionSuppliesObservation {
        let member_ids = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|membership| membership.party_id == party_id)
            .map(|membership| membership.character_id)
            .collect::<HashSet<_>>();
        let personal_inventory_ids = self
            .connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| member_ids.contains(&row.character_id))
            .map(|row| row.id)
            .collect::<HashSet<_>>();
        let party_inventory_ids = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party_id)
            .map(|row| row.id)
            .collect::<HashSet<_>>();
        let stored_food_kcal = self
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
        let carried_water_ml = self
            .connection
            .db
            .character_needs()
            .iter()
            .filter(|needs| member_ids.contains(&needs.character_id))
            .map(|needs| needs.carried_water_ml.max(0.0))
            .sum::<f32>();
        let pooled_water_ml = self
            .connection
            .db
            .party()
            .iter()
            .find(|party| party.id == party_id)
            .map_or(0.0, |party| party.pooled_water_ml.max(0.0));
        ExpeditionSuppliesObservation {
            stored_food_kcal,
            portable_water_ml: carried_water_ml + pooled_water_ml,
        }
    }

    pub(super) fn emit_expedition_diagnostics(
        &mut self,
        party_id: &str,
        phase: &str,
        action: &str,
        reason: &str,
        before: &[ExpeditionMemberObservation],
        after: &[ExpeditionMemberObservation],
        supplies_before: ExpeditionSuppliesObservation,
        supplies_after: ExpeditionSuppliesObservation,
    ) {
        for member_before in before {
            let member_after = after
                .iter()
                .find(|candidate| candidate.character_id == member_before.character_id)
                .unwrap_or(member_before);
            self.event(
                member_before.agent_id,
                CoreLoopEventKind::ExpeditionRecovery,
                format!(
                    "party={};phase={};action={};reason={};member={};alive_before={};alive_after={};condition_before={};condition_after={};hunger_before={:.3};hunger_after={:.3};thirst_before={:.3};thirst_after={:.3};food_days_before={:.2};food_days_after={:.2};water_days_before={:.2};water_days_after={:.2};symptomatic_before={};symptomatic_after={};critical_before={};critical_after={};exposure=not_publicly_projected;elapsed_before={};elapsed_after={};elapsed_delta={};stored_food_kcal_before={:.0};stored_food_kcal_after={:.0};stored_food_kcal_consumed={:.0};portable_water_ml_before={:.0};portable_water_ml_after={:.0};portable_water_ml_consumed={:.0}",
                    bounded_event_field(party_id),
                    bounded_event_field(phase),
                    bounded_event_field(action),
                    bounded_event_field(reason),
                    member_before.character_id,
                    member_before.alive,
                    member_after.alive,
                    bounded_event_field(&member_before.condition_status),
                    bounded_event_field(&member_after.condition_status),
                    member_before.hunger,
                    member_after.hunger,
                    member_before.thirst,
                    member_after.thirst,
                    member_before.food_days,
                    member_after.food_days,
                    member_before.water_days,
                    member_after.water_days,
                    member_before.symptomatic,
                    member_after.symptomatic,
                    member_before.critical,
                    member_after.critical,
                    member_before.elapsed_minutes,
                    member_after.elapsed_minutes,
                    member_after
                        .elapsed_minutes
                        .saturating_sub(member_before.elapsed_minutes),
                    supplies_before.stored_food_kcal,
                    supplies_after.stored_food_kcal,
                    (supplies_before.stored_food_kcal - supplies_after.stored_food_kcal).max(0.0),
                    supplies_before.portable_water_ml,
                    supplies_after.portable_water_ml,
                    (supplies_before.portable_water_ml - supplies_after.portable_water_ml).max(0.0),
                ),
            );
        }
    }

    pub(super) fn record_journey_hold(
        &mut self,
        party_id: &str,
        phase: &str,
        reason: &str,
    ) -> Result<JourneyTravelOutcome, String> {
        let party = self.party_by_id(party_id)?;
        let members = self.expedition_member_observations(party_id)?;
        let supplies = self.expedition_supplies(party_id);
        let living_count = members.iter().filter(|member| member.alive).count() as u32;
        let required_food_kcal =
            living_count as f32 * adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY;
        let required_water_ml = living_count as f32
            * adventuresim_core::provisioning::STRATEGIC_TRAVEL_WATER_ML_PER_DAY;
        let supplies_cover_one_day = expedition_supplies_cover_one_rest_day(&members, supplies);
        let journey = self
            .connection
            .db
            .party_journey()
            .iter()
            .find(|row| row.party_id == party_id);
        let itinerary = self
            .connection
            .db
            .party_journey_itinerary()
            .iter()
            .find(|row| row.party_id == party_id);
        let active_interval = journey.as_ref().and_then(|journey| {
            itinerary.as_ref().and_then(|itinerary| {
                projected_camp_rest_minutes(
                    journey.completed_elapsed_minutes,
                    journey.total_elapsed_minutes,
                    &itinerary.forecast_camp_intervals,
                )
            })
        });
        let journey_completed_elapsed = journey.as_ref().map_or_else(
            || "none".into(),
            |row| row.completed_elapsed_minutes.to_string(),
        );
        let journey_total_elapsed = journey.as_ref().map_or_else(
            || "none".into(),
            |row| row.total_elapsed_minutes.to_string(),
        );
        let journey_remaining_elapsed = journey.as_ref().map_or_else(
            || "none".into(),
            |row| {
                row.total_elapsed_minutes
                    .saturating_sub(row.completed_elapsed_minutes)
                    .to_string()
            },
        );
        let journey_destination = journey.as_ref().map_or_else(
            || {
                party
                    .camp_destination
                    .as_ref()
                    .map_or_else(|| "none".into(), public_journey_endpoint)
            },
            |row| public_journey_endpoint(&row.destination),
        );
        let (active_interval_start, active_interval_minutes) = active_interval.map_or_else(
            || ("none".into(), "none".into()),
            |(start, minutes)| (start.to_string(), minutes.to_string()),
        );
        self.metrics.expedition_holds = self.metrics.expedition_holds.saturating_add(1);
        let diagnostic_agent = members.first().map_or(0, |member| member.agent_id);
        self.event(
            diagnostic_agent,
            CoreLoopEventKind::ExpeditionRecovery,
            format!(
                "party={};phase={};action=hold_position;reason={};journey_completed_elapsed={journey_completed_elapsed};journey_total_elapsed={journey_total_elapsed};journey_remaining_elapsed={journey_remaining_elapsed};journey_destination={};camp_remaining_minutes={};active_forecast_interval_start={active_interval_start};active_forecast_interval_minutes={active_interval_minutes};living_count={living_count};one_day_food_kcal_required={required_food_kcal:.0};stored_food_kcal={:.0};one_day_water_ml_required={required_water_ml:.0};portable_water_ml={:.0};supplies_cover_one_rest_day={supplies_cover_one_day}",
                bounded_event_field(party_id),
                bounded_event_field(phase),
                bounded_event_field(reason),
                bounded_event_field(&journey_destination),
                party.camp_remaining_minutes,
                supplies.stored_food_kcal,
                supplies.portable_water_ml,
            ),
        );
        self.emit_expedition_diagnostics(
            party_id,
            phase,
            "hold_position",
            reason,
            &members,
            &members,
            supplies,
            supplies,
        );
        Ok(JourneyTravelOutcome::HeldNoActionableActor)
    }

    pub(super) fn expedition_recovery_actor(&self, party_id: &str) -> Option<(u64, u32, &'static str)> {
        let party = self
            .connection
            .db
            .party()
            .iter()
            .find(|party| party.id == party_id)?;
        let mut ready = self
            .expedition_member_observations(party_id)
            .ok()?
            .into_iter()
            .filter(|member| {
                member.alive
                    && member.condition_status == "ready"
                    && !member.symptomatic
                    && !member.critical
            })
            .collect::<Vec<_>>();
        ready.sort_by_key(|member| (member.character_id != party.leader_id, member.character_id));
        if let Some(actor) = ready.into_iter().next() {
            let role = if actor.character_id == party.leader_id {
                "ready_leader"
            } else {
                "ready_companion"
            };
            return Some((actor.character_id, actor.agent_id, role));
        }
        None
    }

    pub(super) fn expedition_recovery_rest_actor(
        &self,
        party_id: &str,
    ) -> Option<ExpeditionRecoveryRestActor> {
        let party = self
            .connection
            .db
            .party()
            .iter()
            .find(|party| party.id == party_id)?;
        let members = self.expedition_member_observations(party_id).ok()?;
        let supplies = self.expedition_supplies(party_id);
        if self.party_has_unresolved_public_encounter(party_id)
            || self.public_active_camp_observation(party_id).is_none()
        {
            return None;
        }
        if let Some((character_id, agent_id, role)) = self.expedition_recovery_actor(party_id) {
            return Some(ExpeditionRecoveryRestActor::Actionable(
                ActionableRecoveryRestActor {
                    character_id,
                    agent_id,
                    role,
                },
            ));
        }
        if !passive_no_actionable_rest_allowed(
            &members,
            supplies,
            party.current_settlement_id.is_none(),
            true,
            party.leader_id,
            false,
        ) {
            return None;
        }
        let leader = members
            .iter()
            .find(|member| member.character_id == party.leader_id && member.alive)?;
        Some(ExpeditionRecoveryRestActor::PassiveNoActionable(
            PassiveNoActionableRestActor {
                leader_id: leader.character_id,
                agent_id: leader.agent_id,
            },
        ))
    }

    pub(super) fn perform_expedition_recovery_rest(
        &mut self,
        actor: ExpeditionRecoveryRestActor,
    ) -> Result<(), String> {
        let (character_id, operation) = match actor {
            ExpeditionRecoveryRestActor::Actionable(actor) => {
                (actor.character_id, "expedition_recovery_rest")
            }
            ExpeditionRecoveryRestActor::PassiveNoActionable(actor) => {
                (actor.leader_id, "passive_no_actionable_rest")
            }
        };
        let result = reducer_call!(self, operation, |cb| self
            .connection
            .reducers
            .rest_at_camp_then(
                character_id,
                EXPEDITION_RECOVERY_REST_MINUTES,
                FieldShelter::Bivouac,
                cb,
            ));
        self.call(result)
    }

    pub(super) fn public_expedition_return_settlement(&self, party_id: &str) -> Option<String> {
        if let Some(journey) = self
            .connection
            .db
            .party_journey()
            .iter()
            .find(|journey| journey.party_id == party_id)
        {
            if let JourneyEndpoint::Settlement(origin) = journey.origin {
                return Some(origin.id);
            }
            if let JourneyEndpoint::Settlement(destination) = journey.destination {
                return Some(destination.id);
            }
        }
        let party = self
            .connection
            .db
            .party()
            .iter()
            .find(|party| party.id == party_id)?;
        let current_site = party.current_case_site_id.as_ref()?.value.as_str();
        let member_ids = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|membership| membership.party_id == party_id)
            .map(|membership| membership.character_id)
            .collect::<HashSet<_>>();
        let mut origins = self
            .connection
            .db
            .backend_case_site_pins()
            .iter()
            .filter(|pin| {
                member_ids.contains(&pin.owner_character_id) && pin.case_site_id == current_site
            })
            .map(|pin| pin.origin_settlement_id)
            .collect::<Vec<_>>();
        origins.sort();
        origins.dedup();
        if origins.len() == 1 {
            return origins.pop();
        }
        None
    }

    pub(super) fn public_journey_is_evacuation(&self, party_id: &str) -> bool {
        let Some(return_settlement) = self.public_expedition_return_settlement(party_id) else {
            return false;
        };
        self.connection
            .db
            .party_journey()
            .iter()
            .find(|journey| journey.party_id == party_id)
            .is_some_and(|journey| {
                matches!(
                    journey.destination,
                    JourneyEndpoint::Settlement(destination)
                        if destination.id == return_settlement
                )
            })
    }

    pub(super) fn recover_or_evacuate_off_settlement(
        &mut self,
        party_id: &str,
        cycle: u32,
    ) -> Result<ExpeditionRecoveryOutcome, String> {
        let party = self.party_by_id(party_id)?;
        if party.current_settlement_id.is_some() {
            return Ok(ExpeditionRecoveryOutcome::None);
        }
        let mut before = self.expedition_member_observations(party_id)?;
        if !before.iter().any(expedition_member_needs_recovery) {
            return Ok(ExpeditionRecoveryOutcome::None);
        }
        let supplies_before = self.expedition_supplies(party_id);
        self.metrics.expedition_recovery_plans =
            self.metrics.expedition_recovery_plans.saturating_add(1);
        self.metrics.quests_suppressed_for_health =
            self.metrics.quests_suppressed_for_health.saturating_add(
                before
                    .iter()
                    .filter(|member| expedition_member_needs_recovery(member))
                    .count() as u32,
            );
        if self.party_has_unresolved_public_encounter(party_id) {
            self.record_journey_hold(
                party_id,
                "recovery_plan",
                "journey_held_unresolved_encounter",
            )?;
            self.emit_expedition_diagnostics(
                party_id,
                "plan",
                "hold_position",
                "journey_held_unresolved_encounter",
                &before,
                &before,
                supplies_before,
                supplies_before,
            );
            return Ok(ExpeditionRecoveryOutcome::Held);
        }
        let actionable_actor = self.expedition_recovery_actor(party_id);
        let coherent_camp = self.public_active_camp_observation(party_id);
        if party.camp_destination.is_some() && coherent_camp.is_none() {
            self.record_journey_hold(
                party_id,
                "recovery_plan",
                "journey_held_incoherent_public_camp",
            )?;
            return Ok(ExpeditionRecoveryOutcome::Held);
        }
        let plan_actor = coherent_camp
            .and_then(|_| self.expedition_recovery_rest_actor(party_id))
            .or_else(|| {
                actionable_actor.map(|(character_id, agent_id, role)| {
                    ExpeditionRecoveryRestActor::Actionable(ActionableRecoveryRestActor {
                        character_id,
                        agent_id,
                        role,
                    })
                })
            });
        let Some(plan_actor) = plan_actor else {
            self.record_journey_hold(party_id, "recovery_plan", "journey_held_no_recovery_actor")?;
            self.emit_expedition_diagnostics(
                party_id,
                "plan",
                "hold_position",
                "journey_held_no_recovery_actor",
                &before,
                &before,
                supplies_before,
                supplies_before,
            );
            return Ok(ExpeditionRecoveryOutcome::Held);
        };
        let actor_id = plan_actor.character_id();
        let actor_agent = plan_actor.agent_id();
        let actor_role = plan_actor.role();
        self.emit_expedition_diagnostics(
            party_id,
            "plan",
            "field_recovery_then_evacuation",
            &format!("quest_suppressed_off_settlement_health_cycle_{cycle}_{actor_role}"),
            &before,
            &before,
            supplies_before,
            supplies_before,
        );
        self.event(
            actor_agent,
            CoreLoopEventKind::QuestSuppressed,
            format!(
                "cycle={cycle};reason=off_settlement_member_not_ready;plan=field_recovery_then_evacuation;actor={actor_id};actor_role={actor_role}"
            ),
        );

        let can_attempt_field_recovery = coherent_camp.is_some()
            && before
                .iter()
                .all(|member| !member.alive || !member.critical)
            && expedition_supplies_cover_one_rest_day(&before, supplies_before);
        if can_attempt_field_recovery {
            for rest_ordinal in 1..=MAX_EXPEDITION_RECOVERY_RESTS {
                if self.party_has_unresolved_public_encounter(party_id) {
                    self.record_journey_hold(
                        party_id,
                        "field_recovery_actor_reselection",
                        "journey_held_unresolved_encounter",
                    )?;
                    return Ok(ExpeditionRecoveryOutcome::Held);
                }
                let party_before_rest = self.party_by_id(party_id)?;
                if party_before_rest.camp_destination.is_some()
                    && self.public_active_camp_observation(party_id).is_none()
                {
                    self.record_journey_hold(
                        party_id,
                        "field_recovery_actor_reselection",
                        "journey_held_incoherent_public_camp",
                    )?;
                    return Ok(ExpeditionRecoveryOutcome::Held);
                }
                let Some(rest_actor) = self.expedition_recovery_rest_actor(party_id) else {
                    self.record_journey_hold(
                        party_id,
                        "field_recovery_actor_reselection",
                        "journey_held_no_recovery_actor",
                    )?;
                    return Ok(ExpeditionRecoveryOutcome::Held);
                };
                let rest_before = self.expedition_member_observations(party_id)?;
                let rest_supplies_before = self.expedition_supplies(party_id);
                if rest_actor.is_passive() {
                    self.metrics.expedition_passive_rest_attempts = self
                        .metrics
                        .expedition_passive_rest_attempts
                        .saturating_add(1);
                }
                self.perform_expedition_recovery_rest(rest_actor)?;
                self.observe_deaths();
                let rest_after = self.expedition_member_observations(party_id)?;
                let rest_supplies_after = self.expedition_supplies(party_id);
                let actual_elapsed_minutes = expedition_elapsed_delta(&rest_before, &rest_after);
                self.metrics.expedition_recovery_rests =
                    self.metrics.expedition_recovery_rests.saturating_add(1);
                self.metrics.recovery_rests = self.metrics.recovery_rests.saturating_add(1);
                if rest_actor.is_passive() {
                    self.metrics.expedition_passive_rest_minutes = self
                        .metrics
                        .expedition_passive_rest_minutes
                        .saturating_add(actual_elapsed_minutes);
                    self.event(
                        rest_actor.agent_id(),
                        CoreLoopEventKind::ExpeditionRecovery,
                        format!(
                            "party={};phase=passive_no_actionable_rest;action=rest_at_camp;rest_attempt={rest_ordinal};leader={};requested_minutes={EXPEDITION_RECOVERY_REST_MINUTES};actual_elapsed_minutes={actual_elapsed_minutes}",
                            bounded_event_field(party_id),
                            rest_actor.character_id(),
                        ),
                    );
                }
                self.emit_expedition_diagnostics(
                    party_id,
                    "field_rest",
                    "rest_at_camp",
                    &if rest_actor.is_passive() {
                        format!("passive_no_actionable_rest_attempt_{rest_ordinal}")
                    } else {
                        format!("bounded_recovery_rest_{rest_ordinal}")
                    },
                    &rest_before,
                    &rest_after,
                    rest_supplies_before,
                    rest_supplies_after,
                );
                if expedition_party_can_resume(&rest_after) {
                    self.metrics.expedition_resumes =
                        self.metrics.expedition_resumes.saturating_add(1);
                    self.emit_expedition_diagnostics(
                        party_id,
                        "resume",
                        "resume_expedition",
                        "quest_resumed_all_members_ready_and_asymptomatic",
                        &rest_after,
                        &rest_after,
                        rest_supplies_after,
                        rest_supplies_after,
                    );
                    return Ok(ExpeditionRecoveryOutcome::Resumed);
                }
                before = rest_after;
                if before.iter().any(|member| member.alive && member.critical)
                    || !expedition_supplies_cover_one_rest_day(&before, rest_supplies_after)
                {
                    break;
                }
            }
        }

        let Some(return_settlement) = self.public_expedition_return_settlement(party_id) else {
            let supplies_after = self.expedition_supplies(party_id);
            self.emit_expedition_diagnostics(
                party_id,
                "evacuation",
                "hold_position",
                "no_public_return_route",
                &before,
                &before,
                supplies_after,
                supplies_after,
            );
            return Ok(ExpeditionRecoveryOutcome::Held);
        };
        let evacuation_before = self.expedition_member_observations(party_id)?;
        let evacuation_supplies_before = self.expedition_supplies(party_id);
        let Some((evacuation_actor_id, evacuation_actor_agent, evacuation_actor_role)) =
            self.expedition_recovery_actor(party_id)
        else {
            self.record_journey_hold(
                party_id,
                "evacuation_plan",
                "journey_held_no_evacuation_actor",
            )?;
            return Ok(ExpeditionRecoveryOutcome::Held);
        };
        self.emit_expedition_diagnostics(
            party_id,
            "evacuation_plan",
            "return_to_settlement",
            "quest_suppressed_recovery_incomplete",
            &evacuation_before,
            &evacuation_before,
            evacuation_supplies_before,
            evacuation_supplies_before,
        );
        let result = reducer_call!(self, "expedition_health_evacuation", |cb| self
            .connection
            .reducers
            .travel_to_settlement_then(evacuation_actor_id, return_settlement.clone(), cb));
        self.call(result)?;
        if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
            return Ok(ExpeditionRecoveryOutcome::Held);
        }
        self.observe_deaths();
        let evacuation_after = self.expedition_member_observations(party_id)?;
        let evacuation_supplies_after = self.expedition_supplies(party_id);
        let evacuation_party = self.party_by_id(party_id)?;
        let evacuation_complete = evacuation_party.current_settlement_id.as_deref()
            == Some(return_settlement.as_str())
            && evacuation_party.camp_destination.is_none()
            && evacuation_after.iter().any(|member| member.alive);
        if !evacuation_complete {
            self.emit_expedition_diagnostics(
                party_id,
                "evacuation_stalled",
                "return_to_settlement",
                "public_state_does_not_prove_living_party_returned",
                &evacuation_before,
                &evacuation_after,
                evacuation_supplies_before,
                evacuation_supplies_after,
            );
            return Ok(ExpeditionRecoveryOutcome::Held);
        }
        self.metrics.expedition_evacuations = self.metrics.expedition_evacuations.saturating_add(1);
        self.event(
            evacuation_actor_agent,
            CoreLoopEventKind::ExpeditionRecovery,
            format!(
                "party={};phase=evacuation_authority;actor={evacuation_actor_id};actor_role={evacuation_actor_role};destination={}",
                bounded_event_field(party_id),
                bounded_event_field(&return_settlement),
            ),
        );
        self.emit_expedition_diagnostics(
            party_id,
            "evacuation_complete",
            "return_to_settlement",
            "quest_suppressed_settlement_recovery_required",
            &evacuation_before,
            &evacuation_after,
            evacuation_supplies_before,
            evacuation_supplies_after,
        );
        Ok(ExpeditionRecoveryOutcome::Evacuated)
    }

}
