impl LiveRunner {
    pub(super) fn event(&mut self, agent_id: u32, kind: CoreLoopEventKind, detail: impl Into<String>) {
        self.sequence += 1;
        let detail = detail.into();
        let semantic = format!("{agent_id}:{kind:?}:{detail}");
        let repeatable = event_is_repeatable(&kind);
        if !repeatable && self.last_semantic_event.as_ref() == Some(&semantic) {
            self.metrics.duplicate_semantic_events += 1;
        }
        self.last_semantic_event = Some(semantic);
        if self.trace.len() < MAX_CORE_TRACE_EVENTS {
            self.trace.push(CoreLoopEvent {
                sequence: self.sequence,
                agent_id,
                kind,
                detail,
            });
        }
        self.capture_failure_diagnostics();
    }

    pub(super) fn call(&mut self, result: Result<(), String>) -> Result<(), String> {
        if result.is_err() {
            self.metrics.reducer_failures += 1;
            self.capture_failure_diagnostics();
        }
        result
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
            .character()
            .iter()
            .find(|row| row.id == character_id)?;
        let condition = self
            .connection
            .db
            .character_strategic_condition()
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
                .map(|service| format!("{service:?}"))
                .collect()
        });
        settlement_services.sort();
        let settlement_id = character.current_settlement_id.clone();
        let visible_herbalist_quote = settlement_id
            .as_deref()
            .and_then(|id| self.observable_medical_quote(character_id, id));
        let visible_inn_full_board_cost = settlement
            .is_some_and(|row| row.economy.services.contains(&SettlementService::Inn))
            .then(|| adventuresim_core::strategic_economy::inn_full_board_cost(1_440))
            .flatten();
        Some(CoreLoopFailureAgent {
            agent_id,
            character_id,
            alive: character.alive,
            condition_status: condition.status,
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
    ) -> Result<SettlementActivityVenue, String> {
        let settlement_id = self
            .connection
            .db
            .character()
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
        select_settlement_activity_venue(
            inn_available,
            temple_available,
            temple_food_covers_one_day(visible_food_kcal),
            self.personal_gold(character_id),
            committed_reserve,
            adventuresim_core::strategic_economy::inn_full_board_cost(1_440),
        )
        .ok_or_else(|| {
            "simulation character cannot afford an Inn while preserving visible reserves"
                .to_string()
        })
    }

    /// Non-activity waits retain the ordinary public-service preference. Their
    /// requested duration can be shorter than the one-day activity planner's
    /// supply horizon.
    pub(super) fn settlement_rest_at_inn(&self, character_id: u64) -> Result<bool, String> {
        let settlement_id = self
            .connection
            .db
            .character()
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
        Ok(adventuresim_core::settlement_economy::action_service_at_inn(service))
    }

    pub(super) fn party_for(&self, character_id: u64) -> Result<Party, String> {
        let character = self
            .connection
            .db
            .character()
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
        let leader = self.connection.db.character().iter().find(|row| {
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
            .character_time()
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
            .character()
            .iter()
            .filter(|row| !row.alive && self.character_ids.contains(&row.id))
            .filter_map(|row| self.recorded_deaths.insert(row.id).then_some(row.id))
            .collect::<Vec<_>>();
        newly_dead.sort_unstable();
        for character_id in newly_dead {
            if let Some(agent) = self.character_ids.iter().position(|id| *id == character_id) {
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
                    agent as u32,
                    CoreLoopEventKind::Death,
                    format!("authoritative terminal state;source={source:?}"),
                );
            }
        }
    }

}
