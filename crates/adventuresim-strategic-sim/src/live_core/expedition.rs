#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthoritySurrenderOutcome {
    NotApplicable,
    Surrendered,
    Held,
}

pub(super) fn select_affordable_authority_surrender_action(
    actions: impl IntoIterator<Item = BackendAuthorityArrestAction>,
    party_id: &str,
    current_site_id: &str,
    controlled_character_ids: &HashSet<u64>,
) -> Option<BackendAuthorityArrestAction> {
    let mut eligible = actions
        .into_iter()
        .filter(|action| {
            action.party_id == party_id
                && action.case_site_id == current_site_id
                && action.affordable
                && controlled_character_ids.contains(&action.instigator_id)
        })
        .collect::<Vec<_>>();
    eligible.sort_by(|left, right| left.action_token.cmp(&right.action_token));
    let [action] = eligible.as_slice() else {
        return None;
    };
    Some(action.clone())
}

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
                    .backend_characters()
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
                    .backend_characters()
                    .iter()
                    .find(|row| row.id == id)
                    .is_some_and(|row| row.alive);
                let ready = self
                    .connection
                    .db
                    .backend_character_strategic_conditions()
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
                    .backend_characters()
                    .iter()
                    .find(|row| row.id == character_id)
                    .ok_or("expedition member projection is unavailable")?;
                let condition = self
                    .connection
                    .db
                    .backend_character_strategic_conditions()
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
                    .backend_character_times()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .map_or(0, |row| row.minutes);
                let survival = self
                    .public_survival_observation(character_id)
                    .unwrap_or_default();
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
                    thermal: survival.thermal,
                    wetness_bps: survival.wetness_bps,
                    thermal_strain: survival.thermal_strain,
                    ammunition: survival.ammunition,
                    carried_load_kg: survival.carried_load_kg,
                    carry_capacity_kg: survival.carry_capacity_kg,
                    encumbrance_remaining_bps: survival.encumbrance_remaining_bps,
                    equipment_ready: survival.equipment_ready,
                    party_tent_quantity: survival.party_tent_quantity,
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
            .filter(|row| {
                member_ids.contains(&row.character_id)
                    && self.public_row_is_carried(
                        "personal",
                        &row.character_id.to_string(),
                        row.id,
                    )
            })
            .map(|row| row.id)
            .collect::<HashSet<_>>();
        let party_inventory_ids = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| {
                row.party_id == party_id
                    && self.public_row_is_carried("party", party_id, row.id)
            })
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
            .backend_character_needs()
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
        let contained_water_ml = member_ids
            .iter()
            .map(|character_id| {
                self.public_contained_water_ml("personal", &character_id.to_string())
            })
            .sum::<f32>()
            + self.public_contained_water_ml("party", party_id);
        ExpeditionSuppliesObservation {
            stored_food_kcal,
            portable_water_ml: carried_water_ml + pooled_water_ml + contained_water_ml,
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
                    "party={};phase={};action={};reason={};member={};alive_before={};alive_after={};condition_before={};condition_after={};hunger_before={:.3};hunger_after={:.3};thirst_before={:.3};thirst_after={:.3};food_days_before={:.2};food_days_after={:.2};water_days_before={:.2};water_days_after={:.2};thermal_before={:.3};thermal_after={:.3};wetness_bps_before={};wetness_bps_after={};thermal_strain_before={};thermal_strain_after={};ammo_before={};ammo_after={};carried_load_kg_before={:.3};carried_load_kg_after={:.3};carry_capacity_kg_before={:.3};carry_capacity_kg_after={:.3};encumbrance_remaining_bps_before={};encumbrance_remaining_bps_after={};equipment_ready_before={};equipment_ready_after={};party_tent_quantity_before={};party_tent_quantity_after={};symptomatic_before={};symptomatic_after={};critical_before={};critical_after={};elapsed_before={};elapsed_after={};elapsed_delta={};stored_food_kcal_before={:.0};stored_food_kcal_after={:.0};stored_food_kcal_consumed={:.0};portable_water_ml_before={:.0};portable_water_ml_after={:.0};portable_water_ml_consumed={:.0}",
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
                    member_before.thermal,
                    member_after.thermal,
                    member_before.wetness_bps,
                    member_after.wetness_bps,
                    member_before.thermal_strain,
                    member_after.thermal_strain,
                    member_before.ammunition,
                    member_after.ammunition,
                    member_before.carried_load_kg,
                    member_after.carried_load_kg,
                    member_before.carry_capacity_kg,
                    member_after.carry_capacity_kg,
                    member_before.encumbrance_remaining_bps,
                    member_after.encumbrance_remaining_bps,
                    member_before.equipment_ready,
                    member_after.equipment_ready,
                    member_before.party_tent_quantity,
                    member_after.party_tent_quantity,
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

    pub(super) fn expedition_recovery_actor(
        &self,
        party_id: &str,
    ) -> Option<(u64, u32, &'static str)> {
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
            || (party.camp_destination.is_some()
                && self.public_journey_camp_state(party_id).is_err())
            || (party.camp_destination.is_none() && party.current_case_site_id.is_none())
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
        self.rest_at_camp_with_party_shelter(
            character_id,
            EXPEDITION_RECOVERY_REST_MINUTES,
            operation,
        )
        .map(|_| ())
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
        if !origins.is_empty() {
            return None;
        }
        observed_activity_return_origin(
            &self.observed_activity_site_origins,
            party_id,
            Some(current_site),
        )
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

    fn return_idle_ready_party_from_case_site(
        &mut self,
        party_id: &str,
    ) -> Result<Option<ExpeditionRecoveryOutcome>, String> {
        let party = self.party_by_id(party_id)?;
        let Some(current_site_id) = party
            .current_case_site_id
            .as_ref()
            .map(|site| site.value.clone())
        else {
            return Ok(None);
        };
        // A persisted journey, accepted direct contract, or unresolved generated
        // case already has its own continuation policy. This fallback is only
        // for a ready party left idle at a publicly known site by another
        // authoritative action, such as an activity incident.
        if party.camp_destination.is_some() || self.active_direct_contract(&party).is_some() {
            return Ok(None);
        }
        let member_ids = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|membership| membership.party_id == party_id)
            .map(|membership| membership.character_id)
            .collect::<HashSet<_>>();
        let mut pins = self
            .connection
            .db
            .backend_case_site_pins()
            .iter()
            .filter(|pin| {
                member_ids.contains(&pin.owner_character_id)
                    && pin.case_site_id == current_site_id
            })
            .collect::<Vec<_>>();
        if pins
            .iter()
            .any(|pin| pin.generated_case && !pin.case_resolved)
        {
            return Ok(None);
        }
        pins.sort_by_key(|pin| {
            (
                pin.origin_settlement_id.clone(),
                pin.owner_character_id,
                pin.case_id.clone(),
            )
        });
        let Some(return_settlement) = self.public_expedition_return_settlement(party_id) else {
            self.record_journey_hold(
                party_id,
                "idle_case_site_return",
                "journey_held_no_unique_public_idle_site_origin",
            )?;
            return Ok(Some(ExpeditionRecoveryOutcome::Held));
        };
        let return_pin = pins
            .into_iter()
            .find(|pin| pin.origin_settlement_id == return_settlement);
        let members = self.expedition_member_observations(party_id)?;
        let supplies = self.expedition_supplies(party_id);
        let observed_unpinned_activity_return = return_pin.is_none()
            && observed_activity_return_origin(
                &self.observed_activity_site_origins,
                party_id,
                Some(&current_site_id),
            )
            .as_deref()
                == Some(return_settlement.as_str());
        if !expedition_party_can_resume(&members) {
            self.record_journey_hold(
                party_id,
                "idle_case_site_return",
                "journey_held_idle_site_return_condition_not_ready",
            )?;
            return Ok(Some(ExpeditionRecoveryOutcome::Held));
        }
        if !observed_unpinned_activity_return
            && !expedition_supplies_cover_one_rest_day(&members, supplies)
        {
            self.record_journey_hold(
                party_id,
                "idle_case_site_return",
                "journey_held_idle_site_return_supplies_unavailable",
            )?;
            return Ok(Some(ExpeditionRecoveryOutcome::Held));
        }
        if !observed_unpinned_activity_return
            && !matches!(
                self.validate_party_departure_readiness(party_id),
                DepartureReadiness::Ready
            )
        {
            self.record_journey_hold(
                party_id,
                "idle_case_site_return",
                "journey_held_idle_site_return_departure_not_ready",
            )?;
            return Ok(Some(ExpeditionRecoveryOutcome::Held));
        }
        if return_pin.as_ref().is_some_and(|pin| {
            !matches!(
                self.generated_action_return_thermal_decision(party_id, pin, 0),
                OnSiteActionDecision::Ready | OnSiteActionDecision::ReturnNow
            )
        }) {
            self.record_journey_hold(
                party_id,
                "idle_case_site_return",
                "journey_held_unsafe_idle_site_return_forecast",
            )?;
            return Ok(Some(ExpeditionRecoveryOutcome::Held));
        }
        let Some((return_actor_id, return_actor_agent)) = self.current_leader(party_id) else {
            self.record_journey_hold(
                party_id,
                "idle_case_site_return",
                "journey_held_no_idle_site_return_actor",
            )?;
            return Ok(Some(ExpeditionRecoveryOutcome::Held));
        };
        let result = reducer_call!(self, "idle_case_site_return", |cb| self
            .connection
            .reducers
            .travel_to_settlement_then(return_actor_id, return_settlement.clone(), cb));
        self.call(result)?;
        self.event(
            return_actor_agent,
            CoreLoopEventKind::Travel,
            format!(
                "party={};phase=idle_case_site_return;case_site={};destination={};reason=public_idle_site_return",
                bounded_event_field(party_id),
                bounded_event_field(&current_site_id),
                bounded_event_field(&return_settlement),
            ),
        );
        if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
            return Ok(Some(ExpeditionRecoveryOutcome::Held));
        }
        self.observe_deaths();
        let returned_party = self.party_by_id(party_id)?;
        if returned_party.current_settlement_id.as_deref() != Some(return_settlement.as_str())
            || returned_party.camp_destination.is_some()
            || !self
                .expedition_member_observations(party_id)?
                .iter()
                .any(|member| member.alive)
        {
            self.record_journey_hold(
                party_id,
                "idle_case_site_return",
                "journey_held_idle_site_return_not_publicly_complete",
            )?;
            return Ok(Some(ExpeditionRecoveryOutcome::Held));
        }
        self.event(
            return_actor_agent,
            CoreLoopEventKind::Travel,
            format!(
                "party={};phase=idle_case_site_return_complete;destination={};reason=public_idle_site_return",
                bounded_event_field(party_id),
                bounded_event_field(&return_settlement),
            ),
        );
        self.observed_activity_site_origins
            .retain(|(observed_party_id, _), _| observed_party_id != party_id);
        Ok(Some(ExpeditionRecoveryOutcome::Returned))
    }

    fn surrender_affordable_authority_arrest(
        &mut self,
        party_id: &str,
        current_site_id: &str,
    ) -> Result<AuthoritySurrenderOutcome, String> {
        let controlled_character_ids = self.character_ids.iter().copied().collect::<HashSet<_>>();
        let Some(action) = select_affordable_authority_surrender_action(
            self.connection
                .db
                .backend_authority_arrest_actions()
                .iter(),
            party_id,
            current_site_id,
            &controlled_character_ids,
        ) else {
            return Ok(AuthoritySurrenderOutcome::NotApplicable);
        };
        let Some(agent) = self
            .character_ids
            .iter()
            .position(|character_id| *character_id == action.instigator_id)
            .map(|index| index as u32)
        else {
            return Ok(AuthoritySurrenderOutcome::NotApplicable);
        };
        if !self
            .connection
            .db
            .backend_characters()
            .iter()
            .find(|character| character.id == action.instigator_id)
            .is_some_and(|character| character.alive && character.party_id.as_deref() == Some(party_id))
        {
            return Ok(AuthoritySurrenderOutcome::NotApplicable);
        }

        let action_token = action.action_token;
        let origin_settlement_id = action.origin_settlement_id;
        let instigator_id = action.instigator_id;
        let fine = action.fine;
        let result = reducer_call!(self, "surrender_to_authority", |cb| self
            .connection
            .reducers
            .surrender_to_authority_then(instigator_id, action_token.clone(), cb));
        self.call(result)?;
        let action_remains = self
            .connection
            .db
            .backend_authority_arrest_actions()
            .iter()
            .any(|action| {
                action.action_token == action_token
                    && action.party_id == party_id
                    && action.case_site_id == current_site_id
            });
        let party_remains = self.party_by_id(party_id)?.current_case_site_id.as_ref().is_some_and(
            |site| site.value == current_site_id,
        );
        if action_remains || !party_remains {
            self.record_journey_hold(
                party_id,
                "authority_surrender",
                "journey_held_authority_surrender_not_publicly_confirmed",
            )?;
            self.event(
                agent,
                CoreLoopEventKind::AuthoritySurrender,
                format!(
                    "party={};case_site={};origin_settlement={};reason=authority_surrender_not_publicly_confirmed",
                    bounded_event_field(party_id),
                    bounded_event_field(current_site_id),
                    bounded_event_field(&origin_settlement_id),
                ),
            );
            return Ok(AuthoritySurrenderOutcome::Held);
        }
        self.observed_activity_site_origins
            .retain(|(observed_party_id, _), _| observed_party_id != party_id);
        self.observed_activity_site_origins.insert(
            (party_id.to_owned(), current_site_id.to_owned()),
            origin_settlement_id.clone(),
        );
        self.metrics.authority_surrenders = self.metrics.authority_surrenders.saturating_add(1);
        self.metrics.authority_fines_paid =
            self.metrics.authority_fines_paid.saturating_add(fine);
        self.event(
            agent,
            CoreLoopEventKind::AuthoritySurrender,
            format!(
                "party={};case_site={};origin_settlement={};fine={fine};reason=affordable_pending_authority_arrest",
                bounded_event_field(party_id),
                bounded_event_field(current_site_id),
                bounded_event_field(&origin_settlement_id),
            ),
        );
        Ok(AuthoritySurrenderOutcome::Surrendered)
    }

    pub(super) fn recover_or_evacuate_off_settlement(
        &mut self,
        party_id: &str,
        cycle: u32,
    ) -> Result<ExpeditionRecoveryOutcome, String> {
        let party = self.party_by_id(party_id)?;
        if party.current_settlement_id.is_some() {
            self.observed_activity_site_origins
                .retain(|(observed_party_id, _), _| observed_party_id != party_id);
            return Ok(ExpeditionRecoveryOutcome::None);
        }
        if let Some(current_site_id) = party
            .current_case_site_id
            .as_ref()
            .map(|site| site.value.as_str())
        {
            match self.surrender_affordable_authority_arrest(party_id, current_site_id)? {
                AuthoritySurrenderOutcome::Held => {
                    return Ok(ExpeditionRecoveryOutcome::Held);
                }
                AuthoritySurrenderOutcome::NotApplicable
                | AuthoritySurrenderOutcome::Surrendered => {}
            }
        }
        let mut before = self.expedition_member_observations(party_id)?;
        if !before.iter().any(expedition_member_needs_recovery) {
            if let Some(outcome) = self.return_idle_ready_party_from_case_site(party_id)? {
                return Ok(outcome);
            }
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
        let camp_state = if party.camp_destination.is_some() {
            match self.public_journey_camp_state(party_id) {
                Ok(state) => Some(state),
                Err(_) => {
                    self.record_journey_hold(
                        party_id,
                        "recovery_plan",
                        "journey_held_incoherent_public_camp",
                    )?;
                    return Ok(ExpeditionRecoveryOutcome::Held);
                }
            }
        } else {
            None
        };
        let coherent_camp = (camp_state == Some(PublicJourneyCampState::ActiveCamp))
            .then(|| self.public_active_camp_observation(party_id))
            .flatten();
        if camp_state == Some(PublicJourneyCampState::ActiveCamp) && coherent_camp.is_none() {
            self.record_journey_hold(
                party_id,
                "recovery_plan",
                "journey_held_incoherent_public_camp",
            )?;
            return Ok(ExpeditionRecoveryOutcome::Held);
        }
        let at_case_site = party.current_case_site_id.is_some();
        let plan_actor = (camp_state.is_some() || at_case_site)
            .then(|| self.expedition_recovery_rest_actor(party_id))
            .flatten()
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

        let can_attempt_field_recovery = (camp_state.is_some() || at_case_site)
            && before
                .iter()
                .all(|member| !member.alive || !member.critical)
            && expedition_supplies_cover_one_rest_day(&before, supplies_before)
            && self.field_recovery_rest_thermal_safe(party_id, EXPEDITION_RECOVERY_REST_MINUTES);
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
                    && self.public_journey_camp_state(party_id).is_err()
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
                if !self
                    .field_recovery_rest_thermal_safe(party_id, EXPEDITION_RECOVERY_REST_MINUTES)
                {
                    break;
                }
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
        let evacuation_party = self.party_by_id(party_id)?;
        let continuing_public_journey = evacuation_party.camp_destination.is_some()
            && self.public_journey_camp_state(party_id).is_ok();
        let observed_activity_return = observed_activity_return_origin(
            &self.observed_activity_site_origins,
            party_id,
            evacuation_party
                .current_case_site_id
                .as_ref()
                .map(|site| site.value.as_str()),
        )
        .as_deref()
            == Some(return_settlement.as_str());
        let evacuation_safe = continuing_public_journey
            || observed_activity_return
            || evacuation_party
                .current_case_site_id
                .as_ref()
                .and_then(|site| {
                    self.connection
                        .db
                        .backend_case_site_pins()
                        .iter()
                        .find(|pin| {
                            pin.case_site_id == site.value
                                && pin.origin_settlement_id == return_settlement
                        })
                })
                .is_some_and(|pin| {
                    matches!(
                        self.generated_action_return_thermal_decision(party_id, &pin, 0),
                        OnSiteActionDecision::Ready | OnSiteActionDecision::ReturnNow
                    )
                });
        if !evacuation_safe {
            self.record_journey_hold(
                party_id,
                "evacuation_plan",
                "journey_held_unsafe_return_forecast",
            )?;
            return Ok(ExpeditionRecoveryOutcome::Held);
        }
        let Some((evacuation_actor_id, evacuation_actor_agent, evacuation_actor_role)) = self
            .current_leader(party_id)
            .map(|(character_id, agent_id)| (character_id, agent_id, "living_leader"))
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
        if !self.public_journey_is_evacuation(party_id) {
            let result = reducer_call!(self, "expedition_health_evacuation", |cb| self
                .connection
                .reducers
                .travel_to_settlement_then(evacuation_actor_id, return_settlement.clone(), cb));
            self.call(result)?;
        }
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
        self.observed_activity_site_origins
            .retain(|(observed_party_id, _), _| observed_party_id != party_id);
        Ok(ExpeditionRecoveryOutcome::Evacuated)
    }
}
