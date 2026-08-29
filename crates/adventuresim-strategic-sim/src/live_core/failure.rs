impl LiveRunner {
    pub(super) fn event(
        &mut self,
        agent_id: u32,
        kind: CoreLoopEventKind,
        detail: impl Into<String>,
    ) {
        self.emit_event(agent_id, CoreLoopEventPayload::agent(kind, detail));
    }

    pub(super) fn direct_contract_event(
        &mut self,
        agent_id: u32,
        kind: CoreLoopEventKind,
        party_id: &str,
        contract_id: &str,
        detail: impl Into<String>,
    ) {
        self.emit_event(
            agent_id,
            CoreLoopEventPayload::direct_contract(kind, party_id, contract_id, detail),
        );
    }

    pub(super) fn character_event(
        &mut self,
        agent_id: u32,
        kind: CoreLoopEventKind,
        character_id: u64,
        detail: impl Into<String>,
    ) {
        self.emit_event(
            agent_id,
            CoreLoopEventPayload::character(kind, character_id, detail),
        );
    }

    pub(super) fn generated_case_event(
        &mut self,
        agent_id: u32,
        kind: CoreLoopEventKind,
        party_id: &str,
        case_id: &str,
        detail: impl Into<String>,
    ) {
        self.emit_event(
            agent_id,
            CoreLoopEventPayload::generated_case(kind, party_id, case_id, detail),
        );
    }

    pub(super) fn investigation_action_event(
        &mut self,
        agent_id: u32,
        kind: CoreLoopEventKind,
        case_id: &str,
        action_id: &str,
        detail: impl Into<String>,
    ) {
        self.emit_event(
            agent_id,
            CoreLoopEventPayload::investigation_action(kind, case_id, action_id, detail),
        );
    }

    pub(super) fn item_event(
        &mut self,
        agent_id: u32,
        kind: CoreLoopEventKind,
        inventory_item_id: u64,
        detail: impl Into<String>,
    ) {
        self.emit_event(
            agent_id,
            CoreLoopEventPayload::item(kind, inventory_item_id, detail),
        );
    }

    pub(super) fn encounter_event(
        &mut self,
        agent_id: u32,
        kind: CoreLoopEventKind,
        party_id: &str,
        encounter_id: &str,
        detail: impl Into<String>,
    ) {
        self.emit_event(
            agent_id,
            CoreLoopEventPayload::encounter(kind, party_id, encounter_id, detail),
        );
    }

    fn emit_event(&mut self, agent_id: u32, payload: CoreLoopEventPayload) {
        self.sequence += 1;
        let semantic = payload.semantic_key(agent_id);
        if is_duplicate_semantic_event(self.last_semantic_event.as_ref(), &semantic) {
            self.metrics.duplicate_semantic_events += 1;
        }
        self.last_semantic_event = Some(semantic.clone());
        if self.trace.len() < MAX_CORE_TRACE_EVENTS {
            self.trace
                .push(payload.into_public(self.sequence, agent_id));
            self.semantic_event_keys.push(semantic);
        }
        self.capture_failure_diagnostics();
    }

    pub(super) fn observe_call_result(
        &mut self,
        result: Result<(), CoreLoopError>,
    ) -> Result<(), CoreLoopError> {
        match result {
            Ok(()) => Ok(()),
            Err(error) => {
                self.metrics.reducer_failures += 1;
                self.failure_recorder.record(error.clone());
                self.capture_failure_diagnostics();
                Err(error)
            }
        }
    }

    pub(super) fn call(&mut self, result: Result<(), CoreLoopError>) -> Result<(), String> {
        self.observe_call_result(result)
            .map_err(|error| error.to_string())
    }

    pub(super) fn capture_failure_diagnostics(&self) {
        let (trace, trace_truncated) = bounded_failure_trace(&self.trace, self.sequence);
        let final_agents = self
            .character_ids
            .iter()
            .enumerate()
            .filter_map(|(agent, character_id)| {
                self.public_failure_agent(agent as u32, *character_id)
            })
            .collect();
        self.failure_recorder.update(FailureDraft {
            metrics: self.metrics.clone(),
            total_event_count: self.sequence,
            trace_truncated,
            trace,
            final_agents,
        });
    }

    pub(super) fn public_failure_agent(
        &self,
        agent_id: u32,
        character_id: u64,
    ) -> Option<CoreLoopFailureAgent> {
        let character = self
            .connection
            .db
            .backend_characters()
            .iter()
            .find(|row| row.id == character_id)?;
        let condition = self
            .connection
            .db
            .backend_character_strategic_conditions()
            .iter()
            .find(|row| row.character_id == character_id)?;
        let illness = self
            .connection
            .db
            .character_illness_status()
            .iter()
            .find(|row| row.character_id == character_id);
        let (visible_food_kcal, visible_water_ml) = self.visible_rest_supplies(character_id);
        let party = character.party_id.as_deref().and_then(|party_id| {
            self.connection
                .db
                .party()
                .iter()
                .find(|row| row.id == party_id)
        });
        let current_case_site_id = party
            .as_ref()
            .and_then(|row| row.current_case_site_id.as_ref())
            .map(|site| site.value.clone());
        let journey_destination = party.as_ref().and_then(|party| {
            self.connection
                .db
                .party_journey()
                .iter()
                .find(|journey| journey.party_id == party.id)
                .map(|journey| public_journey_endpoint(&journey.destination))
        });
        let settlement = character
            .current_settlement_id
            .as_deref()
            .and_then(|settlement_id| {
                self.connection
                    .db
                    .settlement()
                    .iter()
                    .find(|row| row.id == settlement_id)
            });
        let mut settlement_services = settlement.as_ref().map_or_else(Vec::new, |row| {
            row.economy
                .services
                .iter()
                .map(|service| settlement_service_key(*service).to_owned())
                .collect()
        });
        settlement_services.sort();
        let settlement_id = character.current_settlement_id.clone();
        let visible_herbalist_quote = settlement_id
            .as_deref()
            .and_then(|id| self.observable_medical_quote(character_id, id));
        let visible_inn_full_board_cost = settlement
            .is_some_and(|row| row.economy.services.contains(&SettlementService::Inn))
            .then(|| adventuresim_core::strategic_economy::inn_full_board_cost(MINUTES_PER_DAY))
            .flatten();
        let survival = self.public_survival_observation(character_id)?;
        Some(CoreLoopFailureAgent {
            agent_id,
            character_id,
            alive: character.alive,
            condition_status: domain_incapacitation_status(condition.status),
            thermal: survival.thermal,
            wetness_bps: survival.wetness_bps,
            thermal_strain: survival.thermal_strain,
            ammunition: survival.ammunition,
            carried_load_kg: survival.carried_load_kg,
            carry_capacity_kg: survival.carry_capacity_kg,
            encumbrance_remaining_bps: survival.encumbrance_remaining_bps,
            equipment_ready: survival.equipment_ready,
            party_tent_quantity: survival.party_tent_quantity,
            hunger: condition.hunger,
            thirst: condition.thirst,
            food_days: condition.food_days,
            water_days: condition.water_days,
            visible_food_kcal,
            visible_water_ml,
            personal_gold_coin: self.personal_gold(character_id),
            settlement_id,
            current_case_site_id,
            journey_destination,
            symptomatic: illness.as_ref().is_some_and(|row| row.symptomatic),
            critical: illness.as_ref().is_some_and(|row| row.critical),
            settlement_services,
            visible_herbalist_quote,
            visible_inn_full_board_cost,
        })
    }

    pub(super) fn settlement_activity_venue(
        &self,
        character_id: u64,
        committed_reserve: u64,
    ) -> Result<Option<DomainSettlementActionService>, String> {
        let settlement_id = self
            .connection
            .db
            .backend_characters()
            .iter()
            .find(|row| row.id == character_id)
            .and_then(|character| character.current_settlement_id)
            .ok_or("simulation character is not at a settlement")?;
        let settlement = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|settlement| settlement.id == settlement_id)
            .ok_or("simulation settlement is unavailable")?;
        let inn_available = settlement
            .economy
            .services
            .contains(&SettlementService::Inn);
        let temple_available = settlement
            .economy
            .services
            .contains(&SettlementService::Temple);
        if !inn_available && !temple_available {
            return Err("simulation settlement offers neither an Inn nor a Temple".to_string());
        }
        let (visible_food_kcal, _) = self.visible_rest_supplies(character_id);
        Ok(select_settlement_activity_venue(
            inn_available,
            temple_available,
            temple_food_covers_one_day(visible_food_kcal),
            self.personal_gold(character_id),
            committed_reserve,
            adventuresim_core::strategic_economy::inn_full_board_cost(MINUTES_PER_DAY),
        ))
    }

    /// Non-activity waits retain the ordinary public-service preference. Their
    /// requested duration can be shorter than the one-day activity planner's
    /// supply horizon.
    pub(super) fn settlement_rest_service(
        &self,
        character_id: u64,
    ) -> Result<DomainSettlementActionService, String> {
        let settlement_id = self
            .connection
            .db
            .backend_characters()
            .iter()
            .find(|row| row.id == character_id)
            .and_then(|character| character.current_settlement_id)
            .ok_or("simulation character is not at a settlement")?;
        let settlement = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|settlement| settlement.id == settlement_id)
            .ok_or("simulation settlement is unavailable")?;
        let service =
            adventuresim_core::settlement_economy::select_available_settlement_rest_service(
                settlement
                    .economy
                    .services
                    .contains(&SettlementService::Inn),
                settlement
                    .economy
                    .services
                    .contains(&SettlementService::Temple),
            )
            .ok_or("simulation settlement offers neither an Inn nor a Temple")?;
        Ok(service)
    }

    pub(super) fn party_for(&self, character_id: u64) -> Result<Party, String> {
        let character = self
            .connection
            .db
            .backend_characters()
            .iter()
            .find(|row| row.id == character_id)
            .ok_or("character missing from coherent subscription")?;
        let party_id = character.party_id.ok_or("character has no party")?;
        self.connection
            .db
            .party()
            .iter()
            .find(|row| row.id == party_id)
            .ok_or_else(|| "party missing from coherent subscription".into())
    }

    pub(super) fn party_by_id(&self, party_id: &str) -> Result<Party, String> {
        self.connection
            .db
            .party()
            .iter()
            .find(|row| row.id == party_id)
            .ok_or_else(|| "party missing from coherent subscription".into())
    }

    pub(super) fn current_leader(&self, party_id: &str) -> Option<(u64, u32)> {
        let party = self
            .connection
            .db
            .party()
            .iter()
            .find(|row| row.id == party_id)?;
        let leader = self.connection.db.backend_characters().iter().find(|row| {
            leader_is_actionable(
                party_id,
                party.leader_id,
                row.id,
                row.alive,
                row.party_id.as_deref(),
            )
        })?;
        let agent = self.character_ids.iter().position(|id| *id == leader.id)? as u32;
        Some((leader.id, agent))
    }

    pub(super) fn public_party_elapsed_max(&self, party_id: &str) -> u64 {
        let member_ids = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|row| row.party_id == party_id)
            .map(|row| row.character_id)
            .collect::<HashSet<_>>();
        self.connection
            .db
            .backend_character_times()
            .iter()
            .filter(|row| member_ids.contains(&row.character_id))
            .map(|row| row.minutes)
            .max()
            .unwrap_or(0)
    }

    pub(super) fn observe_deaths(&mut self) {
        let mut newly_dead = self
            .connection
            .db
            .backend_characters()
            .iter()
            .filter(|row| !row.alive && self.character_ids.contains(&row.id))
            .filter_map(|row| self.recorded_deaths.insert(row.id).then_some(row.id))
            .collect::<Vec<_>>();
        newly_dead.sort_unstable();
        for character_id in newly_dead {
            if let Some(agent) = self.character_ids.iter().position(|id| *id == character_id) {
                let death = self
                    .connection
                    .db
                    .backend_character_deaths()
                    .iter()
                    .find(|row| row.character_id == character_id);
                let source = death.as_ref().map(|row| row.source);
                if source == Some(DeathSource::Disease) {
                    self.metrics.disease_deaths += 1;
                }
                let cause = death.as_ref().map_or_else(
                    || "unavailable".to_owned(),
                    |row| death_cause_key(row.cause).to_owned(),
                );
                let source_id = death
                    .as_ref()
                    .and_then(|row| row.source_id.as_deref())
                    .map_or_else(|| "none".to_owned(), bounded_event_field);
                let strategic_minute = death.as_ref().map_or_else(
                    || "unavailable".to_owned(),
                    |row| row.strategic_minute.to_string(),
                );
                let survival = self.public_survival_observation(character_id);
                let condition = self
                    .connection
                    .db
                    .backend_character_strategic_conditions()
                    .iter()
                    .find(|row| row.character_id == character_id);
                self.character_event(
                    agent as u32,
                    CoreLoopEventKind::Death,
                    character_id,
                    format!(
                        "terminal=authoritative;cause={cause};source={source:?};source_id={source_id};strategic_minute={strategic_minute};condition={};thermal={:.3};wetness_bps={};thermal_strain={};ammo={};carried_load_kg={:.3};carry_capacity_kg={:.3};encumbrance_remaining_bps={};equipment_ready={};party_tent_quantity={}",
                        condition.as_ref().map_or("unavailable", |row| {
                            domain_incapacitation_status(row.status).as_str()
                        }),
                        survival.map_or(0.0, |row| row.thermal),
                        survival.map_or(0, |row| row.wetness_bps),
                        survival.map_or(0, |row| row.thermal_strain),
                        survival.map_or(0, |row| row.ammunition),
                        survival.map_or(0.0, |row| row.carried_load_kg),
                        survival.map_or(0.0, |row| row.carry_capacity_kg),
                        survival.map_or(0, |row| row.encumbrance_remaining_bps),
                        survival.is_some_and(|row| row.equipment_ready),
                        survival.map_or(0, |row| row.party_tent_quantity),
                    ),
                );
            }
        }
    }
}
