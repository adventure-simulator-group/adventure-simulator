fn bounded_grams(weight_kg: f32) -> u64 {
    if !weight_kg.is_finite() || weight_kg <= 0.0 {
        0
    } else {
        (weight_kg * 1_000.0).round().min(u64::MAX as f32) as u64
    }
}

fn public_encumbrance_remaining_bps(load_kg: f32, capacity_kg: f32) -> u32 {
    (adventuresim_core::equipment::encumbrance_remaining_multiplier(load_kg, capacity_kg)
        .clamp(0.0, 1.0)
        * 10_000.0)
        .round() as u32
}

fn survival_equipment_ready(
    condition_status: &str,
    wetness_bps: u16,
    thermal_strain: i32,
    ranged: bool,
    ammunition: u32,
    encumbrance_remaining_bps: u32,
) -> bool {
    condition_status == "ready"
        && wetness_bps <= MAX_DEPARTURE_WETNESS_BPS
        && thermal_strain.unsigned_abs() <= MAX_DEPARTURE_ABS_THERMAL_STRAIN
        && (!ranged || ammunition >= RANGED_AMMUNITION_FLOOR)
        && encumbrance_remaining_bps >= MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS
}

impl LiveRunner {
    fn living_party_member_ids(&self, party_id: &str) -> Vec<u64> {
        let mut ids = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|row| row.party_id == party_id)
            .filter_map(|row| {
                self.connection
                    .db
                    .backend_characters()
                    .iter()
                    .find(|character| character.id == row.character_id && character.alive)
                    .map(|character| character.id)
            })
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids
    }

    fn item_definition(&self, item_id: &str) -> Option<Item> {
        self.connection
            .db
            .item()
            .iter()
            .find(|item| item.id == item_id)
    }

    fn personal_item_quantity(&self, character_id: u64, item_id: &str) -> u32 {
        self.connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| row.character_id == character_id && row.item_id == item_id)
            .map(|row| row.quantity)
            .sum()
    }

    fn party_item_quantity(&self, party_id: &str, item_id: &str) -> u32 {
        self.connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party_id && row.item_id == item_id)
            .map(|row| row.quantity)
            .sum()
    }

    fn public_stack_weight_kg(&self, item_id: &str, quantity: u32) -> f32 {
        self.item_definition(item_id)
            .map_or(0.0, |item| item.weight.max(0.0) * quantity as f32)
    }

    fn public_personal_load_kg(&self, character_id: u64) -> f32 {
        let water_weight = self
            .connection
            .db
            .backend_character_needs()
            .iter()
            .find(|row| row.character_id == character_id)
            .map_or(0.0, |row| row.carried_water_ml.max(0.0) / 1_000.0);
        let inventory_weight = self
            .connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| row.character_id == character_id)
            .map(|row| {
                self.connection
                    .db
                    .food_lot()
                    .iter()
                    .find(|lot| lot.inventory_item_id == Some(row.id))
                    .map_or_else(
                        || self.public_stack_weight_kg(&row.item_id, row.quantity),
                        |lot| lot.mass_kg.max(0.0),
                    )
            })
            .sum::<f32>();
        // Match StrategicEquipment::load: encumbrance is carried inventory
        // plus carried water. A character's own body mass is combat data, not
        // cargo, and including it creates a false overweight feedback loop
        // when a staggered condition temporarily halves carrying capacity.
        water_weight + inventory_weight
    }

    fn public_character_capacity_kg(&self, character_id: u64) -> f32 {
        let Some(attributes) = self
            .connection
            .db
            .backend_character_attributes()
            .iter()
            .find(|row| row.character_id == character_id)
        else {
            return 0.0;
        };
        let Some(limbs) = self
            .connection
            .db
            .backend_character_limbs()
            .iter()
            .find(|row| row.character_id == character_id)
        else {
            return 0.0;
        };
        let condition_multiplier = self
            .connection
            .db
            .backend_character_strategic_conditions()
            .iter()
            .find(|row| row.character_id == character_id)
            .map_or(0.0, |row| match row.status.as_str() {
                "ready" => 1.0,
                "staggered" => 0.5,
                _ => 0.0,
            });
        let adjusted_leg_strength = (attributes.left_leg_strength
            * limbs.left_leg_health.clamp(0.0, 1.0)
            + attributes.right_leg_strength * limbs.right_leg_health.clamp(0.0, 1.0))
            * 0.5;
        adventuresim_core::equipment::encumbrance_capacity_kg(adjusted_leg_strength)
            * condition_multiplier
    }

    fn public_party_load_and_capacity(&self, party_id: &str) -> (f32, f32, u32) {
        let living_ids = self.living_party_member_ids(party_id);
        let personal_load = living_ids
            .iter()
            .map(|id| self.public_personal_load_kg(*id))
            .sum::<f32>();
        let pooled_load = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party_id)
            .map(|row| {
                self.connection
                    .db
                    .food_lot()
                    .iter()
                    .find(|lot| lot.party_inventory_item_id == Some(row.id))
                    .map_or_else(
                        || self.public_stack_weight_kg(&row.item_id, row.quantity),
                        |lot| lot.mass_kg.max(0.0),
                    )
            })
            .sum::<f32>();
        let capacity = living_ids
            .iter()
            .map(|id| self.public_character_capacity_kg(*id))
            .sum::<f32>();
        let load = personal_load + pooled_load;
        (
            load,
            capacity,
            public_encumbrance_remaining_bps(load, capacity),
        )
    }

    pub(super) fn public_survival_observation(
        &self,
        character_id: u64,
    ) -> Option<PublicSurvivalObservation> {
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
        let capability = self
            .connection
            .db
            .backend_character_capabilities()
            .iter()
            .find(|row| row.character_id == character_id)?;
        let carried_load_kg = self.public_personal_load_kg(character_id);
        let carry_capacity_kg = self.public_character_capacity_kg(character_id);
        let encumbrance_remaining_bps =
            public_encumbrance_remaining_bps(carried_load_kg, carry_capacity_kg);
        let ammunition = self.personal_item_quantity(character_id, RANGED_AMMUNITION_ITEM_ID);
        let party_tent_quantity = character.party_id.as_deref().map_or(0, |party_id| {
            self.party_item_quantity(party_id, PARTY_TENT_ITEM_ID)
        });
        Some(PublicSurvivalObservation {
            thermal: condition.thermal,
            wetness_bps: condition.wetness_bps,
            thermal_strain: condition.thermal_strain,
            ammunition,
            carried_load_kg,
            carry_capacity_kg,
            encumbrance_remaining_bps,
            equipment_ready: survival_equipment_ready(
                &condition.status,
                condition.wetness_bps,
                condition.thermal_strain,
                capability.ranged,
                ammunition,
                encumbrance_remaining_bps,
            ),
            party_tent_quantity,
        })
    }

    pub(super) fn observe_survival_telemetry(&mut self, party_id: &str) {
        let (load, capacity, remaining_bps) = self.public_party_load_and_capacity(party_id);
        self.metrics.max_party_carried_load_grams = self
            .metrics
            .max_party_carried_load_grams
            .max(bounded_grams(load));
        self.metrics.max_party_carry_capacity_grams = self
            .metrics
            .max_party_carry_capacity_grams
            .max(bounded_grams(capacity));
        if self.metrics.survival_observations == 0 {
            self.metrics.min_party_encumbrance_remaining_bps = remaining_bps;
        } else {
            self.metrics.min_party_encumbrance_remaining_bps = self
                .metrics
                .min_party_encumbrance_remaining_bps
                .min(remaining_bps);
        }
        self.metrics.survival_observations = self.metrics.survival_observations.saturating_add(1);
        let member_ids = self.living_party_member_ids(party_id);
        for character_id in member_ids {
            if let Some(condition) = self
                .connection
                .db
                .backend_character_strategic_conditions()
                .iter()
                .find(|row| row.character_id == character_id)
            {
                self.metrics.max_observed_wetness_bps = self
                    .metrics
                    .max_observed_wetness_bps
                    .max(condition.wetness_bps);
                self.metrics.max_observed_abs_thermal_strain = self
                    .metrics
                    .max_observed_abs_thermal_strain
                    .max(condition.thermal_strain.unsigned_abs());
            }
        }
    }

    fn public_general_store_quote(
        &self,
        character_id: u64,
        settlement_id: &str,
        item: &Item,
    ) -> Option<u64> {
        let character = self
            .connection
            .db
            .backend_characters()
            .iter()
            .find(|row| row.id == character_id && row.alive)?;
        if character.current_settlement_id.as_deref() != Some(settlement_id) {
            return None;
        }
        let settlement = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|row| row.id == settlement_id)?;
        if !settlement.economy.services.iter().any(|service| {
            matches!(
                service,
                SettlementService::Market | SettlementService::GeneralStore
            )
        }) {
            return None;
        }
        let provider = self.public_default_storefront_provider(
            character_id,
            settlement_id,
            "merchants",
            "market",
        )?;
        let _ = provider;
        let buy_bps = self
            .connection
            .db
            .backend_local_problem_trade_effects()
            .iter()
            .find(|row| row.character_id == character_id && row.settlement_id == settlement_id)?
            .buy_bps;
        let base =
            adventuresim_core::strategic_economy::merchant_buy_price(item.base_value.unwrap_or(1));
        let language_bound =
            adventuresim_core::strategic_economy::language_adjusted_buy_price(base, 0.0);
        Some(u64::from(adventuresim_core::local_problem::adjust_price(
            language_bound,
            buy_bps,
        )))
    }

    fn public_general_storefront_exists(&self, character_id: u64, settlement_id: &str) -> bool {
        self.connection
            .db
            .settlement()
            .iter()
            .find(|row| row.id == settlement_id)
            .is_some_and(|settlement| {
                settlement.economy.services.iter().any(|service| {
                    matches!(
                        service,
                        SettlementService::Market | SettlementService::GeneralStore
                    )
                })
            })
            && self
                .public_default_storefront_provider(
                    character_id,
                    settlement_id,
                    "merchants",
                    "market",
                )
                .is_some()
    }

    fn public_default_storefront_provider(
        &self,
        character_id: u64,
        settlement_id: &str,
        service_id: &str,
        location_id: &str,
    ) -> Option<u64> {
        let minute = self
            .connection
            .db
            .backend_character_times()
            .iter()
            .find(|row| row.character_id == character_id)?
            .minutes;
        let providers = self
            .connection
            .db
            .backend_settlement_residents()
            .iter()
            .filter(|npc| npc.home_settlement_id == settlement_id && npc.service_id == service_id)
            .filter(|npc| {
                self.connection
                    .db
                    .settlement_resident_presence()
                    .iter()
                    .any(|presence| {
                        presence.character_id == npc.character_id
                            && presence.settlement_id == settlement_id
                            && presence.location_id == location_id
                            && presence.is_default
                    })
            })
            .filter_map(|npc| {
                self.connection
                    .db
                    .settlement_resident_presence()
                    .iter()
                    .find(|presence| presence.character_id == npc.character_id)
                    .map(|presence| (npc.character_id, presence.start_minute, presence.end_minute))
            })
            .collect::<Vec<_>>();
        visible_unique_default_provider(&providers, minute)
    }

    pub(super) fn public_equipment_storefront_offer(
        &self,
        character_id: u64,
        settlement_id: &str,
        item: &Item,
    ) -> Option<(String, u64, u64)> {
        let (storefront, service_id, location_id) = match item.kind {
            ItemKind::Weapon | ItemKind::Shield => (
                adventuresim_core::settlement_economy::Storefront::Weapons,
                "weapons",
                "forge",
            ),
            ItemKind::Armor => (
                adventuresim_core::settlement_economy::Storefront::Armor,
                "armor",
                "armoury",
            ),
            ItemKind::Clothing => (
                adventuresim_core::settlement_economy::Storefront::Clothing,
                "clothing",
                "tailor",
            ),
            _ => return None,
        };
        let settlement = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|settlement| settlement.id == settlement_id)?;
        if !public_storefront_available(&settlement.economy, storefront)
            || !public_storefront_stocks(&settlement.economy, storefront, item)
        {
            return None;
        }
        let provider = self.public_default_storefront_provider(
            character_id,
            settlement_id,
            service_id,
            location_id,
        )?;
        let buy_bps = self
            .connection
            .db
            .backend_local_problem_trade_effects()
            .iter()
            .find(|row| row.character_id == character_id && row.settlement_id == settlement_id)?
            .buy_bps;
        let base =
            adventuresim_core::strategic_economy::merchant_buy_price(item.base_value.unwrap_or(1));
        let language_upper_bound =
            adventuresim_core::strategic_economy::language_adjusted_buy_price(base, 0.0);
        let quote = adventuresim_core::local_problem::adjust_price(language_upper_bound, buy_bps);
        Some((service_id.to_owned(), provider, u64::from(quote)))
    }

    fn withdraw_stake_for_personal_purchase(
        &mut self,
        character_id: u64,
        party_id: &str,
        needed: u64,
    ) -> Result<bool, String> {
        if needed == 0 {
            return Ok(true);
        }
        let stake = self
            .connection
            .db
            .party_stake()
            .iter()
            .find(|row| row.party_id == party_id && row.character_id == character_id)
            .map_or(0, |row| row.value);
        if stake < needed {
            return Ok(false);
        }
        let mut treasury = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party_id && is_currency_id(&row.item_id))
            .collect::<Vec<_>>();
        treasury.sort_by_key(|row| (row.item_id.clone(), row.id));
        if treasury
            .iter()
            .map(|row| u64::from(row.quantity))
            .sum::<u64>()
            < needed
        {
            return Ok(false);
        }
        let mut remaining = needed;
        for stack in treasury {
            let quantity = remaining.min(u64::from(stack.quantity)) as u32;
            let result = reducer_call!(self, "withdraw_purchase_coin", |cb| self
                .connection
                .reducers
                .withdraw_party_inventory_item_then(character_id, stack.id, quantity, cb));
            self.call(result)?;
            remaining -= u64::from(quantity);
            if remaining == 0 {
                break;
            }
        }
        self.metrics.earned_gold_withdrawn =
            self.metrics.earned_gold_withdrawn.saturating_add(needed);
        Ok(true)
    }

    fn ensure_party_tent(
        &mut self,
        party_id: &str,
        settlement_id: &str,
        event_agent: u32,
    ) -> Result<DepartureReadiness, String> {
        if self.party_item_quantity(party_id, PARTY_TENT_ITEM_ID) > 0 {
            return Ok(DepartureReadiness::Ready);
        }
        let tent = self
            .item_definition(PARTY_TENT_ITEM_ID)
            .ok_or("public field-tent definition is unavailable")?;
        let (load, capacity, _) = self.public_party_load_and_capacity(party_id);
        if public_encumbrance_remaining_bps(load + tent.weight.max(0.0), capacity)
            < MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS
        {
            self.metrics.load_readiness_suppressions =
                self.metrics.load_readiness_suppressions.saturating_add(1);
            return Ok(DepartureReadiness::Deferred("party_tent_would_overload"));
        }
        let party_coin = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party_id && is_currency_id(&row.item_id))
            .map(|row| u64::from(row.quantity))
            .sum::<u64>();
        let member_ids = self.living_party_member_ids(party_id);
        let mut payers = member_ids
            .into_iter()
            .filter_map(|character_id| {
                let quote = self.public_general_store_quote(character_id, settlement_id, &tent)?;
                let purse = self.personal_gold(character_id);
                let reserve = self
                    .observable_medical_reserve(character_id, settlement_id)
                    .unwrap_or(0);
                let spendable = party_coin.saturating_add(purse).saturating_sub(reserve);
                Some((
                    spendable >= quote,
                    spendable,
                    character_id,
                    purse,
                    reserve,
                    quote,
                ))
            })
            .collect::<Vec<_>>();
        payers.sort_by_key(|payer| (payer.0, payer.1, payer.2));
        let Some((affordable, _, payer, purse_before, reserve, quote)) = payers.pop() else {
            let public_provider_exists =
                self.living_party_member_ids(party_id)
                    .into_iter()
                    .any(|character_id| {
                        self.public_general_storefront_exists(character_id, settlement_id)
                    });
            if !public_provider_exists {
                self.metrics.tent_provider_unavailable_bivouac_departures = self
                    .metrics
                    .tent_provider_unavailable_bivouac_departures
                    .saturating_add(1);
                self.event(
                    event_agent,
                    CoreLoopEventKind::QuestDecision,
                    "survival_readiness=tent_provider_unavailable_bivouac;shelter=bivouac",
                );
                return Ok(DepartureReadiness::Ready);
            }
            return Ok(DepartureReadiness::Deferred("party_tent_quote_unavailable"));
        };
        if !affordable {
            return Ok(DepartureReadiness::Deferred("party_tent_unaffordable"));
        }
        let result = reducer_call!(self, "purchase_party_tent", |cb| self
            .connection
            .reducers
            .finalize_merchant_trade_then(
                payer,
                settlement_id.to_owned(),
                vec![PARTY_TENT_ITEM_ID.to_owned()],
                vec![1],
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
            .saturating_add(purse_before)
            .saturating_sub(after_party_coin.saturating_add(self.personal_gold(payer)));
        if self.party_item_quantity(party_id, PARTY_TENT_ITEM_ID) == 0 {
            return Err("party tent purchase completed without party custody".into());
        }
        self.metrics.party_tents_purchased = self.metrics.party_tents_purchased.saturating_add(1);
        self.metrics.party_tent_gold_spent = self
            .metrics
            .party_tent_gold_spent
            .saturating_add(actual_spent);
        self.event(
            event_agent,
            CoreLoopEventKind::Purchase,
            format!(
                "survival_item=field_tent;custody=party;payer={payer};upper_bound_quote={quote};actual_spent={actual_spent};medical_reserve={reserve}"
            ),
        );
        Ok(DepartureReadiness::Ready)
    }

    fn ensure_ranged_ammunition(
        &mut self,
        party_id: &str,
        settlement_id: &str,
    ) -> Result<DepartureReadiness, String> {
        let arrow = self
            .item_definition(RANGED_AMMUNITION_ITEM_ID)
            .ok_or("public ammunition definition is unavailable")?;
        let member_ids = self.living_party_member_ids(party_id);
        for character_id in member_ids {
            let ranged = self
                .connection
                .db
                .backend_character_capabilities()
                .iter()
                .find(|row| row.character_id == character_id)
                .is_some_and(|row| row.ranged);
            if !ranged {
                continue;
            }
            let before_quantity =
                self.personal_item_quantity(character_id, RANGED_AMMUNITION_ITEM_ID);
            if before_quantity >= RANGED_AMMUNITION_FLOOR {
                continue;
            }
            let missing = RANGED_AMMUNITION_FLOOR - before_quantity;
            let (load, capacity, _) = self.public_party_load_and_capacity(party_id);
            let added_weight = arrow.weight.max(0.0) * missing as f32;
            if public_encumbrance_remaining_bps(load + added_weight, capacity)
                < MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS
            {
                self.metrics.load_readiness_suppressions =
                    self.metrics.load_readiness_suppressions.saturating_add(1);
                self.metrics.ammunition_shortage_suppressions = self
                    .metrics
                    .ammunition_shortage_suppressions
                    .saturating_add(1);
                return Ok(DepartureReadiness::Deferred("ammunition_would_overload"));
            }
            let Some(unit_quote) =
                self.public_general_store_quote(character_id, settlement_id, &arrow)
            else {
                self.metrics.ammunition_shortage_suppressions = self
                    .metrics
                    .ammunition_shortage_suppressions
                    .saturating_add(1);
                return Ok(DepartureReadiness::Deferred(
                    "ammunition_provider_projection_unavailable",
                ));
            };
            let quote = unit_quote.saturating_mul(u64::from(missing));
            let reserve = self
                .observable_medical_reserve(character_id, settlement_id)
                .unwrap_or(0);
            let purse_before = self.personal_gold(character_id);
            let personal_spendable = purse_before.saturating_sub(reserve);
            let shortfall = quote.saturating_sub(personal_spendable);
            if !self.withdraw_stake_for_personal_purchase(character_id, party_id, shortfall)? {
                self.metrics.ammunition_shortage_suppressions = self
                    .metrics
                    .ammunition_shortage_suppressions
                    .saturating_add(1);
                return Ok(DepartureReadiness::Deferred("ammunition_unaffordable"));
            }
            let purse_after_withdrawal = self.personal_gold(character_id);
            if purse_after_withdrawal.saturating_sub(reserve) < quote {
                self.metrics.ammunition_shortage_suppressions = self
                    .metrics
                    .ammunition_shortage_suppressions
                    .saturating_add(1);
                return Ok(DepartureReadiness::Deferred("ammunition_unaffordable"));
            }
            let result = reducer_call!(self, "purchase_ammunition", |cb| self
                .connection
                .reducers
                .finalize_merchant_trade_then(
                    character_id,
                    settlement_id.to_owned(),
                    vec![RANGED_AMMUNITION_ITEM_ID.to_owned()],
                    vec![missing],
                    vec![],
                    vec![],
                    false,
                    cb,
                ));
            self.call(result)?;
            let after_quantity =
                self.personal_item_quantity(character_id, RANGED_AMMUNITION_ITEM_ID);
            if after_quantity < RANGED_AMMUNITION_FLOOR {
                return Err("ammunition purchase completed below the readiness floor".into());
            }
            let actual_spent =
                purse_after_withdrawal.saturating_sub(self.personal_gold(character_id));
            self.metrics.ammunition_purchases = self.metrics.ammunition_purchases.saturating_add(1);
            self.metrics.ammunition_units_purchased = self
                .metrics
                .ammunition_units_purchased
                .saturating_add(after_quantity.saturating_sub(before_quantity));
            self.metrics.ammunition_gold_spent = self
                .metrics
                .ammunition_gold_spent
                .saturating_add(actual_spent);
            if let Some(agent) = self.character_ids.iter().position(|id| *id == character_id) {
                self.event(
                    agent as u32,
                    CoreLoopEventKind::Purchase,
                    format!(
                        "survival_item=arrow;custody=personal;ammo_before={before_quantity};ammo_after={after_quantity};floor={RANGED_AMMUNITION_FLOOR};upper_bound_quote={quote};actual_spent={actual_spent};medical_reserve={reserve}"
                    ),
                );
            }
        }
        Ok(DepartureReadiness::Ready)
    }

    pub(super) fn validate_party_departure_readiness(&self, party_id: &str) -> DepartureReadiness {
        let (_, _, remaining_bps) = self.public_party_load_and_capacity(party_id);
        if remaining_bps < MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS {
            return DepartureReadiness::Deferred("party_load_unsafe");
        }
        let member_ids = self.living_party_member_ids(party_id);
        for character_id in member_ids {
            let Some(observation) = self.public_survival_observation(character_id) else {
                return DepartureReadiness::Deferred("survival_projection_unavailable");
            };
            if observation.wetness_bps > MAX_DEPARTURE_WETNESS_BPS
                || observation.thermal_strain.unsigned_abs() > MAX_DEPARTURE_ABS_THERMAL_STRAIN
            {
                return DepartureReadiness::Deferred("thermal_recovery_required");
            }
            if !observation.equipment_ready {
                return DepartureReadiness::Deferred("equipment_not_ready");
            }
        }
        DepartureReadiness::Ready
    }

    fn public_equipped_insulation_bps(&self, character_id: u64) -> Option<u16> {
        let mut layers = Vec::new();
        for equipped in self
            .connection
            .db
            .character_equipped_item()
            .iter()
            .filter(|row| row.character_id == character_id)
        {
            let inventory = self
                .connection
                .db
                .inventory_item()
                .iter()
                .find(|row| row.id == equipped.inventory_item_id)?;
            if inventory.character_id != character_id {
                return None;
            }
            let item = self
                .connection
                .db
                .item()
                .iter()
                .find(|row| row.id == inventory.item_id)?;
            let placement = item
                .equipment_placements
                .iter()
                .find(|placement| placement.id == equipped.placement_id)?;
            let protected_regions = placement
                .protection
                .iter()
                .copied()
                .collect::<HashSet<_>>()
                .len();
            layers.extend(std::iter::repeat((item.padding, item.coverage)).take(protected_regions));
        }
        Some(adventuresim_core::survival::insulation_from_layers(layers))
    }

    pub(super) fn validate_case_site_thermal_readiness(
        &mut self,
        party_id: &str,
        leader_agent: u32,
        pin: &BackendCaseSitePin,
    ) -> DepartureReadiness {
        let Ok(party) = self.party_by_id(party_id) else {
            return DepartureReadiness::Deferred("route_weather_projection_unavailable");
        };
        let origin = match (
            party.current_settlement_id.as_deref(),
            party.current_case_site_id.as_ref(),
        ) {
            (Some(origin_id), None) => self
                .connection
                .db
                .settlement()
                .iter()
                .find(|row| row.id == origin_id)
                .map(|row| {
                    (
                        PublicRoutePoint {
                            latitude_microdegrees: (row.coord_y * 1_000_000.0).round() as i32,
                            longitude_microdegrees: (row.coord_x * 1_000_000.0).round() as i32,
                            // Straight-line case-site route authority currently
                            // samples both endpoints at zero elevation.
                            elevation_m: 0,
                        },
                        row.source_node_id.is_some(),
                    )
                }),
            (None, Some(origin_id)) => self
                .connection
                .db
                .backend_case_site_pins()
                .iter()
                .find(|row| {
                    row.owner_character_id == pin.owner_character_id
                        && row.case_site_id == origin_id.value
                })
                .map(|row| {
                    (
                        PublicRoutePoint {
                            latitude_microdegrees: row.latitude_e_7 / 10,
                            longitude_microdegrees: row.longitude_e_7 / 10,
                            elevation_m: 0,
                        },
                        row.coordinates_are_geographic,
                    )
                }),
            _ => None,
        };
        let Some((origin, origin_geographic)) = origin else {
            return DepartureReadiness::Deferred("route_weather_projection_unavailable");
        };
        let destination = PublicRoutePoint {
            latitude_microdegrees: pin.latitude_e_7 / 10,
            longitude_microdegrees: pin.longitude_e_7 / 10,
            elevation_m: 0,
        };
        if origin_geographic != pin.coordinates_are_geographic {
            return DepartureReadiness::Deferred("route_weather_projection_unavailable");
        }
        let distance_m = public_straight_line_distance_m(origin, destination, origin_geographic);
        let Some(movement_minutes) = case_site_movement_minutes(distance_m) else {
            return DepartureReadiness::Deferred("route_weather_projection_unavailable");
        };
        let member_ids = self.living_party_member_ids(party_id);
        let starting_minute = member_ids
            .iter()
            .filter_map(|character_id| {
                self.connection
                    .db
                    .backend_character_times()
                    .iter()
                    .find(|row| row.character_id == *character_id)
                    .map(|row| row.minutes)
            })
            .max();
        let Some(starting_minute) = starting_minute else {
            return DepartureReadiness::Deferred("route_weather_projection_unavailable");
        };
        let mut itinerary_members = Vec::new();
        for character_id in &member_ids {
            let attributes = self
                .connection
                .db
                .backend_character_attributes()
                .iter()
                .find(|row| row.character_id == *character_id);
            let limbs = self
                .connection
                .db
                .backend_character_limbs()
                .iter()
                .find(|row| row.character_id == *character_id);
            let stats = self
                .connection
                .db
                .backend_character_stats()
                .iter()
                .find(|row| row.character_id == *character_id);
            let schedule = self
                .connection
                .db
                .backend_character_training_schedules()
                .iter()
                .find(|row| row.character_id == *character_id);
            let (Some(attributes), Some(limbs), Some(stats), Some(schedule)) =
                (attributes, limbs, stats, schedule)
            else {
                return DepartureReadiness::Deferred("route_weather_projection_unavailable");
            };
            let fatigue_capacity = (attributes.endurance * limbs.chest_health).max(0.01) * 1_000.0;
            let downtime = &schedule.downtime;
            itinerary_members.push(adventuresim_core::strategic_time::ItineraryMember {
                fatigue_capacity,
                calories_used: stats.calories_used,
                camp_schedule: adventuresim_core::strategic_schedule::DailySchedule {
                    combat_training_minutes: downtime.combat_training_minutes,
                    carousing_minutes: downtime.carousing_minutes,
                    socializing_minutes: downtime.socializing_minutes,
                    prayer: downtime.prayer_minutes,
                    ..Default::default()
                },
            });
        }
        let camp_policy = match party.camp_duration_mode {
            CampDurationMode::Auto => adventuresim_core::strategic_time::CampDurationPolicy::Auto,
            CampDurationMode::Fixed => {
                adventuresim_core::strategic_time::CampDurationPolicy::FixedMinutes(
                    party.fixed_camp_minutes,
                )
            }
        };
        let mut projected_actions = self
            .connection
            .db
            .backend_investigation_actions()
            .iter()
            .filter(|action| {
                action.owner_character_id == pin.owner_character_id && action.case_id == pin.case_id
            })
            .collect::<Vec<_>>();
        let has_tent = self.party_item_quantity(party_id, PARTY_TENT_ITEM_ID) > 0;
        let action_minutes = if projected_actions.is_empty() {
            // Direct contract sites have no generated investigation action.
            0
        } else {
            let Some(profile) = self.profiles.get(leader_agent as usize) else {
                return DepartureReadiness::Deferred("route_weather_projection_unavailable");
            };
            let mut candidate_projection_unavailable = false;
            let mut candidate_complete_projection = false;
            let mut selected_case_site_plan = None;
            let Some(selected_action) = select_generated_travel_action(
                profile,
                &mut projected_actions,
                |action| {
                    let action_minutes = u64::from(action.duration_max_minutes);
                    let selected_plan = [false, true].into_iter().find_map(
                        |allow_case_site_recovery| select_generated_case_site_plan(
                        if allow_case_site_recovery {
                            adventuresim_core::strategic_time::MAX_WALKING_MINUTES_PER_DAY
                        } else {
                            party.walking_minutes_per_day
                        },
                        movement_minutes,
                        action_minutes,
                        party.travel_at_night,
                        starting_minute,
                        |candidate_walking_minutes, candidate_travel_at_night, candidate_wait| {
                            let mut accepted_plan = None;
                            let plan_safe = (|| {
                                let candidate_start =
                                    starting_minute.saturating_add(candidate_wait);
                                if !adventuresim_core::strategic_time::is_walking_time(
                                    candidate_start,
                                    candidate_walking_minutes,
                                    candidate_travel_at_night,
                                ) {
                                    candidate_projection_unavailable = true;
                                    return false;
                                }
                                let Some(candidate_outbound) =
                                    adventuresim_core::strategic_time::forecast_itinerary(
                                        candidate_start,
                                        movement_minutes,
                                        candidate_walking_minutes,
                                        candidate_travel_at_night,
                                        camp_policy,
                                        &itinerary_members,
                                    )
                                else {
                                    candidate_projection_unavailable = true;
                                    return false;
                                };
                                if candidate_outbound.truncated {
                                    candidate_projection_unavailable = true;
                                    return false;
                                }
                                if candidate_outbound.member_final_fatigue.len()
                                    != itinerary_members.len()
                                {
                                    candidate_projection_unavailable = true;
                                    return false;
                                }
                                let candidate_return_members = itinerary_members
                                    .iter()
                                    .enumerate()
                                    .map(|(member_index, member)| {
                                        adventuresim_core::strategic_time::ItineraryMember {
                                            fatigue_capacity: member.fatigue_capacity,
                                            calories_used: calories_after_strenuous_action(
                                                member.fatigue_capacity
                                                    * candidate_outbound.member_final_fatigue
                                                        [member_index],
                                                action_minutes,
                                            ),
                                            camp_schedule: member.camp_schedule,
                                        }
                                    })
                                    .collect::<Vec<_>>();
                                let candidate_return_start = candidate_start
                                    .saturating_add(candidate_outbound.total_elapsed_minutes)
                                    .saturating_add(action_minutes);
                                let Some(candidate_return) =
                                    adventuresim_core::strategic_time::forecast_itinerary(
                                        candidate_return_start,
                                        movement_minutes,
                                        candidate_walking_minutes,
                                        candidate_travel_at_night,
                                        camp_policy,
                                        &candidate_return_members,
                                    )
                                else {
                                    candidate_projection_unavailable = true;
                                    return false;
                                };
                                if candidate_return.truncated {
                                    candidate_projection_unavailable = true;
                                    return false;
                                }
                                if action.required_case_site_id != pin.case_site_id {
                                    return false;
                                }
                                let mut projection_available = true;
                                let mut minimum_insulation_bps = u16::MAX;
                                let action_survivable = member_ids
                                    .iter()
                                    .zip(&itinerary_members)
                                    .enumerate()
                                    .all(|(member_index, (character_id, member))| {
                                        let condition = self
                                            .connection
                                            .db
                                            .backend_character_strategic_conditions()
                                            .iter()
                                            .find(|row| row.character_id == *character_id);
                                        let Some(condition) = condition else {
                                            projection_available = false;
                                            return false;
                                        };
                                        let Some(insulation_bps) =
                                            self.public_equipped_insulation_bps(*character_id)
                                        else {
                                            projection_available = false;
                                            return false;
                                        };
                                        minimum_insulation_bps =
                                            minimum_insulation_bps.min(insulation_bps);
                                        let starting_state =
                                            adventuresim_core::survival::SurvivalState {
                                                wetness_bps: condition.wetness_bps,
                                                thermal_strain: condition.thermal_strain,
                                                frostbite_progress_minutes: 0,
                                            };
                                        let Some(thermal_safe) = projected_round_trip_thermal_safe(
                                            candidate_start,
                                            &candidate_outbound,
                                            &candidate_return,
                                            action_minutes,
                                            origin,
                                            destination,
                                            starting_state,
                                            insulation_bps,
                                            has_tent,
                                        ) else {
                                            projection_available = false;
                                            return false;
                                        };
                                        let nonfatigue_incapacitation =
                                            (condition.incapacitation - condition.fatigue).max(0.0);
                                        let outbound_calories = member.fatigue_capacity
                                            * candidate_outbound.member_final_fatigue[member_index];
                                        let actor_ready_at_arrival = character_id
                                            != &pin.owner_character_id
                                            || projected_action_ready(
                                                nonfatigue_incapacitation,
                                                outbound_calories,
                                                member.fatigue_capacity,
                                            );
                                        projected_itinerary_survivable(
                                            nonfatigue_incapacitation,
                                            &candidate_outbound,
                                            member_index,
                                            member.fatigue_capacity,
                                        ) && actor_ready_at_arrival
                                            && projected_action_survivable(
                                                nonfatigue_incapacitation,
                                                calories_after_strenuous_action(
                                                    outbound_calories,
                                                    action_minutes,
                                                ),
                                                member.fatigue_capacity,
                                            )
                                            && projected_itinerary_survivable(
                                                nonfatigue_incapacitation,
                                                &candidate_return,
                                                member_index,
                                                member.fatigue_capacity,
                                            )
                                            && thermal_safe
                                    });
                                if allow_case_site_recovery
                                    && projection_available
                                    && !action_survivable
                                {
                                    let arrival_members = itinerary_members
                                        .iter()
                                        .enumerate()
                                        .map(|(member_index, member)| {
                                            adventuresim_core::strategic_time::ItineraryMember {
                                                fatigue_capacity: member.fatigue_capacity,
                                                calories_used: member.fatigue_capacity
                                                    * candidate_outbound.member_final_fatigue
                                                        [member_index],
                                                camp_schedule: member.camp_schedule,
                                            }
                                        })
                                        .collect::<Vec<_>>();
                                    let recovery_minutes =
                                        adventuresim_core::strategic_time::common_fatigue_clear_minutes(
                                            &arrival_members,
                                        );
                                    if (1..=1_440).contains(&recovery_minutes) {
                                        let recovered_members = arrival_members
                                            .iter()
                                            .map(|member| {
                                                adventuresim_core::strategic_time::ItineraryMember {
                                                    fatigue_capacity: member.fatigue_capacity,
                                                    calories_used: adventuresim_core::strategic_time::camp_fatigue_after(
                                                        member.calories_used,
                                                        recovery_minutes,
                                                        member.camp_schedule,
                                                    ),
                                                    camp_schedule: member.camp_schedule,
                                                }
                                            })
                                            .collect::<Vec<_>>();
                                        let recovered_return_members = recovered_members
                                            .iter()
                                            .map(|member| {
                                                adventuresim_core::strategic_time::ItineraryMember {
                                                    fatigue_capacity: member.fatigue_capacity,
                                                    calories_used: calories_after_strenuous_action(
                                                        member.calories_used,
                                                        action_minutes,
                                                    ),
                                                    camp_schedule: member.camp_schedule,
                                                }
                                            })
                                            .collect::<Vec<_>>();
                                        let recovered_return_start = candidate_start
                                            .saturating_add(
                                                candidate_outbound.total_elapsed_minutes,
                                            )
                                            .saturating_add(recovery_minutes)
                                            .saturating_add(action_minutes);
                                        if let Some(recovered_return) =
                                            adventuresim_core::strategic_time::forecast_itinerary(
                                                recovered_return_start,
                                                movement_minutes,
                                                candidate_walking_minutes,
                                                candidate_travel_at_night,
                                                camp_policy,
                                                &recovered_return_members,
                                            )
                                            && !recovered_return.truncated
                                        {
                                            let recovery_safe = member_ids
                                                .iter()
                                                .zip(&itinerary_members)
                                                .enumerate()
                                                .all(|(member_index, (character_id, member))| {
                                                    let Some(condition) = self
                                                        .connection
                                                        .db
                                                        .backend_character_strategic_conditions()
                                                        .iter()
                                                        .find(|row| row.character_id == *character_id)
                                                    else {
                                                        return false;
                                                    };
                                                    let Some(insulation_bps) =
                                                        self.public_equipped_insulation_bps(*character_id)
                                                    else {
                                                        return false;
                                                    };
                                                    let nonfatigue = (condition.incapacitation
                                                        - condition.fatigue)
                                                        .max(0.0);
                                                    let recovered_calories =
                                                        recovered_members[member_index].calories_used;
                                                    let thermal_safe =
                                                        projected_recovery_round_trip_thermal_safe(
                                                            candidate_start,
                                                            &candidate_outbound,
                                                            recovery_minutes,
                                                            &recovered_return,
                                                            action_minutes,
                                                            origin,
                                                            destination,
                                                            adventuresim_core::survival::SurvivalState {
                                                                wetness_bps: condition.wetness_bps,
                                                                thermal_strain: condition.thermal_strain,
                                                                frostbite_progress_minutes: 0,
                                                            },
                                                            insulation_bps,
                                                            has_tent,
                                                        )
                                                        .unwrap_or(false);
                                                    projected_itinerary_survivable(
                                                        nonfatigue,
                                                        &candidate_outbound,
                                                        member_index,
                                                        member.fatigue_capacity,
                                                    )
                                                        && (character_id != &pin.owner_character_id
                                                            || projected_action_ready(
                                                                nonfatigue,
                                                                recovered_calories,
                                                                member.fatigue_capacity,
                                                            ))
                                                        && projected_action_survivable(
                                                            nonfatigue,
                                                            calories_after_strenuous_action(
                                                                recovered_calories,
                                                                action_minutes,
                                                            ),
                                                            member.fatigue_capacity,
                                                        )
                                                        && projected_itinerary_survivable(
                                                            nonfatigue,
                                                            &recovered_return,
                                                            member_index,
                                                            member.fatigue_capacity,
                                                        )
                                                        && thermal_safe
                                                });
                                            if recovery_safe {
                                                accepted_plan = Some(SelectedCaseSitePlan {
                                                    walking_minutes_per_day:
                                                        candidate_walking_minutes,
                                                    travel_at_night: candidate_travel_at_night,
                                                    departure_wait_minutes: candidate_wait,
                                                    outbound: candidate_outbound.clone(),
                                                    returned: recovered_return,
                                                    minimum_insulation_bps,
                                                    case_site_recovery_minutes: recovery_minutes,
                                                });
                                                return true;
                                            }
                                        }
                                    }
                                }
                                candidate_projection_unavailable |= !projection_available;
                                candidate_complete_projection |= projection_available;
                                if projection_available && action_survivable {
                                    accepted_plan = Some(SelectedCaseSitePlan {
                                        walking_minutes_per_day: candidate_walking_minutes,
                                        travel_at_night: candidate_travel_at_night,
                                        departure_wait_minutes: candidate_wait,
                                        outbound: candidate_outbound,
                                        returned: candidate_return,
                                        minimum_insulation_bps,
                                        case_site_recovery_minutes: 0,
                                    });
                                }
                                action_survivable
                            })();
                            plan_safe.then(|| {
                                accepted_plan.expect("safe combined plan must retain forecasts")
                            })
                        },
                    ));
                    if let Some(selected_plan) = selected_plan {
                        selected_case_site_plan = Some(selected_plan);
                        true
                    } else {
                        false
                    }
                },
            ) else {
                return DepartureReadiness::Deferred(joint_case_site_plan_failure_reason(
                    candidate_complete_projection,
                    candidate_projection_unavailable,
                ));
            };
            if selected_action.required_case_site_id != pin.case_site_id {
                return DepartureReadiness::Deferred("route_weather_projection_unavailable");
            }
            let Some(selected_plan) = selected_case_site_plan else {
                return DepartureReadiness::Deferred("route_weather_projection_unavailable");
            };
            self.event(
                leader_agent,
                CoreLoopEventKind::QuestDecision,
                format!(
                    "case={};survival_readiness=ready;route_weather_projection=combined_public_round_trip;outbound_minutes={};action_minutes={};return_minutes={};movement_minutes={movement_minutes};minimum_insulation_bps={};weatherproofing=conservative_zero",
                    bounded_event_field(&pin.case_id),
                    selected_plan.outbound.total_elapsed_minutes,
                    selected_action.duration_max_minutes,
                    selected_plan.returned.total_elapsed_minutes,
                    selected_plan.minimum_insulation_bps.min(
                        adventuresim_core::survival::MAX_CLOTHING_INSULATION_BPS
                    ),
                ),
            );
            return if selected_plan.departure_wait_minutes == 0 {
                DepartureReadiness::ReadyWithItinerary {
                    walking_minutes_per_day: selected_plan.walking_minutes_per_day,
                    travel_at_night: selected_plan.travel_at_night,
                    case_site_recovery_minutes: selected_plan.case_site_recovery_minutes,
                }
            } else {
                DepartureReadiness::WaitForSafeDeparture {
                    wait_minutes: selected_plan.departure_wait_minutes,
                    walking_minutes_per_day: selected_plan.walking_minutes_per_day,
                    travel_at_night: selected_plan.travel_at_night,
                    case_site_recovery_minutes: selected_plan.case_site_recovery_minutes,
                }
            };
        };
        let Some(planned_walking_minutes) = round_trip_walking_window_minutes(
            party.walking_minutes_per_day,
            movement_minutes,
            action_minutes,
        ) else {
            return DepartureReadiness::Deferred("route_thermal_risk");
        };
        let planned_travel_at_night = party.travel_at_night;
        let Some(itinerary) = adventuresim_core::strategic_time::forecast_itinerary(
            starting_minute,
            movement_minutes,
            planned_walking_minutes,
            planned_travel_at_night,
            camp_policy,
            &itinerary_members,
        ) else {
            return DepartureReadiness::Deferred("route_weather_projection_unavailable");
        };
        if itinerary.truncated || itinerary.member_final_fatigue.len() != itinerary_members.len() {
            return DepartureReadiness::Deferred("route_weather_projection_unavailable");
        }
        let return_start_minute = starting_minute
            .saturating_add(itinerary.total_elapsed_minutes)
            .saturating_add(action_minutes);
        let return_members = itinerary_members
            .iter()
            .enumerate()
            .map(
                |(member_index, member)| adventuresim_core::strategic_time::ItineraryMember {
                    fatigue_capacity: member.fatigue_capacity,
                    calories_used: calories_after_strenuous_action(
                        member.fatigue_capacity * itinerary.member_final_fatigue[member_index],
                        action_minutes,
                    ),
                    camp_schedule: member.camp_schedule,
                },
            )
            .collect::<Vec<_>>();
        let Some(return_itinerary) = adventuresim_core::strategic_time::forecast_itinerary(
            return_start_minute,
            movement_minutes,
            planned_walking_minutes,
            planned_travel_at_night,
            camp_policy,
            &return_members,
        ) else {
            return DepartureReadiness::Deferred("route_weather_projection_unavailable");
        };
        if return_itinerary.truncated {
            return DepartureReadiness::Deferred("route_weather_projection_unavailable");
        }
        let delayed_forecast = adventuresim_core::strategic_time::minutes_until_next_walking_start(
            starting_minute,
            planned_walking_minutes,
            planned_travel_at_night,
        )
        .and_then(representable_safe_departure_wait_minutes)
        .filter(|wait_minutes| {
            adventuresim_core::strategic_time::is_walking_time(
                starting_minute.saturating_add(*wait_minutes),
                planned_walking_minutes,
                planned_travel_at_night,
            )
        })
        .and_then(|wait_minutes| {
            let delayed_start = starting_minute.saturating_add(wait_minutes);
            let outbound = adventuresim_core::strategic_time::forecast_itinerary(
                delayed_start,
                movement_minutes,
                planned_walking_minutes,
                planned_travel_at_night,
                camp_policy,
                &itinerary_members,
            )?;
            if outbound.truncated || outbound.member_final_fatigue.len() != itinerary_members.len()
            {
                return None;
            }
            let members = itinerary_members
                .iter()
                .enumerate()
                .map(
                    |(member_index, member)| adventuresim_core::strategic_time::ItineraryMember {
                        fatigue_capacity: member.fatigue_capacity,
                        calories_used: calories_after_strenuous_action(
                            member.fatigue_capacity * outbound.member_final_fatigue[member_index],
                            action_minutes,
                        ),
                        camp_schedule: member.camp_schedule,
                    },
                )
                .collect::<Vec<_>>();
            let return_start = delayed_start
                .saturating_add(outbound.total_elapsed_minutes)
                .saturating_add(action_minutes);
            let returned = adventuresim_core::strategic_time::forecast_itinerary(
                return_start,
                movement_minutes,
                planned_walking_minutes,
                planned_travel_at_night,
                camp_policy,
                &members,
            )?;
            if returned.truncated {
                return None;
            }
            Some((wait_minutes, delayed_start, outbound, returned))
        });
        let mut minimum_insulation_bps = u16::MAX;
        let mut immediate_safe = true;
        let mut delayed_safe = delayed_forecast.is_some();
        for (member_index, character_id) in member_ids.into_iter().enumerate() {
            let Some(condition) = self
                .connection
                .db
                .backend_character_strategic_conditions()
                .iter()
                .find(|row| row.character_id == character_id)
            else {
                return DepartureReadiness::Deferred("route_weather_projection_unavailable");
            };
            let Some(insulation_bps) = self.public_equipped_insulation_bps(character_id) else {
                return DepartureReadiness::Deferred("route_weather_projection_unavailable");
            };
            let attributes = self
                .connection
                .db
                .backend_character_attributes()
                .iter()
                .find(|row| row.character_id == character_id);
            let limbs = self
                .connection
                .db
                .backend_character_limbs()
                .iter()
                .find(|row| row.character_id == character_id);
            let (Some(attributes), Some(limbs)) = (attributes, limbs) else {
                return DepartureReadiness::Deferred("route_weather_projection_unavailable");
            };
            let fatigue_capacity = (attributes.endurance * limbs.chest_health).max(0.01) * 1_000.0;
            minimum_insulation_bps = minimum_insulation_bps.min(insulation_bps);
            let starting_state = adventuresim_core::survival::SurvivalState {
                wetness_bps: condition.wetness_bps,
                thermal_strain: condition.thermal_strain,
                frostbite_progress_minutes: 0,
            };
            let Some(member_immediate_thermal_safe) = projected_round_trip_thermal_safe(
                starting_minute,
                &itinerary,
                &return_itinerary,
                action_minutes,
                origin,
                destination,
                starting_state,
                insulation_bps,
                has_tent,
            ) else {
                return DepartureReadiness::Deferred("route_weather_projection_unavailable");
            };
            let nonfatigue_incapacitation = (condition.incapacitation - condition.fatigue).max(0.0);
            let immediate_outbound_fatigue = itinerary.member_final_fatigue[member_index];
            let member_immediate_actor_ready = character_id != pin.owner_character_id
                || projected_action_ready(
                    nonfatigue_incapacitation,
                    immediate_outbound_fatigue * fatigue_capacity,
                    fatigue_capacity,
                );
            let member_immediate_action_survivable = projected_action_survivable(
                nonfatigue_incapacitation,
                calories_after_strenuous_action(
                    immediate_outbound_fatigue * fatigue_capacity,
                    action_minutes,
                ),
                fatigue_capacity,
            );
            immediate_safe &= member_immediate_thermal_safe
                && projected_itinerary_survivable(
                    nonfatigue_incapacitation,
                    &itinerary,
                    member_index,
                    fatigue_capacity,
                )
                && member_immediate_actor_ready
                && member_immediate_action_survivable
                && projected_itinerary_survivable(
                    nonfatigue_incapacitation,
                    &return_itinerary,
                    member_index,
                    fatigue_capacity,
                );
            if let Some((_, delayed_start, delayed_outbound, delayed_return)) = &delayed_forecast {
                let delayed_thermal_safe = projected_round_trip_thermal_safe(
                    *delayed_start,
                    delayed_outbound,
                    delayed_return,
                    action_minutes,
                    origin,
                    destination,
                    starting_state,
                    insulation_bps,
                    has_tent,
                )
                .unwrap_or(false);
                let Some(&delayed_outbound_fatigue) =
                    delayed_outbound.member_final_fatigue.get(member_index)
                else {
                    return DepartureReadiness::Deferred("route_weather_projection_unavailable");
                };
                let delayed_actor_ready = character_id != pin.owner_character_id
                    || projected_action_ready(
                        nonfatigue_incapacitation,
                        delayed_outbound_fatigue * fatigue_capacity,
                        fatigue_capacity,
                    );
                delayed_safe &= delayed_thermal_safe
                    && projected_itinerary_survivable(
                        nonfatigue_incapacitation,
                        delayed_outbound,
                        member_index,
                        fatigue_capacity,
                    )
                    && delayed_actor_ready
                    && projected_action_survivable(
                        nonfatigue_incapacitation,
                        calories_after_strenuous_action(
                            delayed_outbound_fatigue * fatigue_capacity,
                            action_minutes,
                        ),
                        fatigue_capacity,
                    )
                    && projected_itinerary_survivable(
                        nonfatigue_incapacitation,
                        delayed_return,
                        member_index,
                        fatigue_capacity,
                    );
            }
            if !member_immediate_thermal_safe
                || !member_immediate_actor_ready
                || !member_immediate_action_survivable
            {
                self.event(
                    leader_agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!(
                        "case={};reason={};member={character_id};outbound_minutes={};action_minutes={action_minutes};return_minutes={};movement_minutes={movement_minutes};insulation_bps={insulation_bps};weatherproofing=conservative_zero",
                        bounded_event_field(&pin.case_id),
                        if member_immediate_thermal_safe {
                            "route_fatigue_risk"
                        } else {
                            "route_thermal_risk"
                        },
                        itinerary.total_elapsed_minutes,
                        return_itinerary.total_elapsed_minutes,
                    ),
                );
            }
        }
        if !immediate_safe {
            let wait_minutes = safe_departure_wait_minutes(
                immediate_safe,
                delayed_safe,
                delayed_forecast.as_ref().map(|forecast| forecast.0),
            );
            if let Some(wait_minutes) = wait_minutes {
                return DepartureReadiness::WaitForSafeDeparture {
                    wait_minutes,
                    walking_minutes_per_day: planned_walking_minutes,
                    travel_at_night: planned_travel_at_night,
                    case_site_recovery_minutes: 0,
                };
            }
            return DepartureReadiness::Deferred("route_thermal_risk");
        }
        self.event(
            leader_agent,
            CoreLoopEventKind::QuestDecision,
            format!(
                "case={};survival_readiness=ready;route_weather_projection=deterministic_public_round_trip;outbound_minutes={};action_minutes={action_minutes};return_minutes={};movement_minutes={movement_minutes};minimum_insulation_bps={};weatherproofing=conservative_zero",
                bounded_event_field(&pin.case_id),
                itinerary.total_elapsed_minutes,
                return_itinerary.total_elapsed_minutes,
                minimum_insulation_bps.min(adventuresim_core::survival::MAX_CLOTHING_INSULATION_BPS),
            ),
        );
        DepartureReadiness::ReadyWithItinerary {
            walking_minutes_per_day: planned_walking_minutes,
            travel_at_night: planned_travel_at_night,
            case_site_recovery_minutes: 0,
        }
    }

    pub(super) fn field_recovery_rest_thermal_safe(
        &self,
        party_id: &str,
        rest_minutes: u64,
    ) -> bool {
        let Ok(party) = self.party_by_id(party_id) else {
            return false;
        };
        let Some(site_id) = party
            .current_case_site_id
            .as_ref()
            .map(|site| site.value.as_str())
        else {
            // Active journey camps already carry an authority-produced camp
            // interval. Preserve that exact journey progress instead of
            // inventing a case-site route or restarting the journey.
            return self
                .public_active_camp_observation(party_id)
                .is_some_and(|camp| {
                    camp.active_interval_start
                        .saturating_add(camp.active_interval_minutes)
                        .saturating_sub(camp.completed_elapsed_minutes)
                        >= rest_minutes
                });
        };
        let Some(pin) = self
            .connection
            .db
            .backend_case_site_pins()
            .iter()
            .find(|pin| pin.case_site_id == site_id)
        else {
            return false;
        };
        let location = PublicRoutePoint {
            latitude_microdegrees: pin.latitude_e_7 / 10,
            longitude_microdegrees: pin.longitude_e_7 / 10,
            elevation_m: 0,
        };
        let member_ids = self.living_party_member_ids(party_id);
        if member_ids.is_empty() {
            return false;
        }
        let has_tent = self.party_item_quantity(party_id, PARTY_TENT_ITEM_ID) > 0;
        member_ids.into_iter().all(|character_id| {
            let starting_minute = self
                .connection
                .db
                .backend_character_times()
                .iter()
                .find(|row| row.character_id == character_id)
                .map(|row| row.minutes);
            let condition = self
                .connection
                .db
                .backend_character_strategic_conditions()
                .iter()
                .find(|row| row.character_id == character_id);
            let insulation_bps = self.public_equipped_insulation_bps(character_id);
            let (Some(starting_minute), Some(condition), Some(insulation_bps)) =
                (starting_minute, condition, insulation_bps)
            else {
                return false;
            };
            projected_stationary_field_thermal_state(
                starting_minute,
                rest_minutes,
                location,
                adventuresim_core::survival::SurvivalState {
                    wetness_bps: condition.wetness_bps,
                    thermal_strain: condition.thermal_strain,
                    frostbite_progress_minutes: 0,
                },
                insulation_bps,
                has_tent,
            )
            .is_some_and(|projection| projection.safe)
        })
    }

    pub(super) fn generated_case_site_sync_safe(
        &self,
        party_id: &str,
        pin: &BackendCaseSitePin,
    ) -> bool {
        let member_ids = self.living_party_member_ids(party_id);
        let Some(target_minute) = member_ids
            .iter()
            .filter_map(|character_id| {
                self.connection
                    .db
                    .backend_character_times()
                    .iter()
                    .find(|row| row.character_id == *character_id)
                    .map(|row| row.minutes)
            })
            .max()
        else {
            return false;
        };
        let site = PublicRoutePoint {
            latitude_microdegrees: pin.latitude_e_7 / 10,
            longitude_microdegrees: pin.longitude_e_7 / 10,
            elevation_m: 0,
        };
        member_ids.into_iter().all(|character_id| {
            if self
                .connection
                .db
                .character_illness_status()
                .iter()
                .any(|row| row.character_id == character_id && row.critical)
            {
                return false;
            }
            let minute = self
                .connection
                .db
                .backend_character_times()
                .iter()
                .find(|row| row.character_id == character_id)
                .map(|row| row.minutes);
            let condition = self
                .connection
                .db
                .backend_character_strategic_conditions()
                .iter()
                .find(|row| row.character_id == character_id);
            let insulation = self.public_equipped_insulation_bps(character_id);
            let (Some(minute), Some(condition), Some(insulation)) = (minute, condition, insulation)
            else {
                return false;
            };
            projected_stationary_outdoor_thermal_state(
                minute,
                target_minute.saturating_sub(minute),
                site,
                adventuresim_core::survival::SurvivalState {
                    wetness_bps: condition.wetness_bps,
                    thermal_strain: condition.thermal_strain,
                    frostbite_progress_minutes: 0,
                },
                insulation,
            )
            .is_some_and(|projection| projection.safe)
        })
    }

    pub(super) fn generated_action_return_thermal_decision(
        &self,
        party_id: &str,
        pin: &BackendCaseSitePin,
        action_minutes: u64,
    ) -> OnSiteActionDecision {
        let Ok(party) = self.party_by_id(party_id) else {
            return OnSiteActionDecision::Hold;
        };
        if party
            .current_case_site_id
            .as_ref()
            .map(|site| site.value.as_str())
            != Some(pin.case_site_id.as_str())
        {
            return OnSiteActionDecision::Hold;
        }
        let Some(origin_row) = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|row| row.id == pin.origin_settlement_id)
        else {
            return OnSiteActionDecision::Hold;
        };
        let origin = PublicRoutePoint {
            latitude_microdegrees: (origin_row.coord_y * 1_000_000.0).round() as i32,
            longitude_microdegrees: (origin_row.coord_x * 1_000_000.0).round() as i32,
            // Match the authoritative straight-line route weather model.
            elevation_m: 0,
        };
        let site = PublicRoutePoint {
            latitude_microdegrees: pin.latitude_e_7 / 10,
            longitude_microdegrees: pin.longitude_e_7 / 10,
            elevation_m: 0,
        };
        if origin_row.source_node_id.is_some() != pin.coordinates_are_geographic {
            return OnSiteActionDecision::Hold;
        }
        let Some(movement_minutes) = case_site_movement_minutes(public_straight_line_distance_m(
            site,
            origin,
            pin.coordinates_are_geographic,
        )) else {
            return OnSiteActionDecision::Hold;
        };
        let member_ids = self.living_party_member_ids(party_id);
        let Some(starting_minute) = member_ids
            .iter()
            .filter_map(|character_id| {
                self.connection
                    .db
                    .backend_character_times()
                    .iter()
                    .find(|row| row.character_id == *character_id)
                    .map(|row| row.minutes)
            })
            .max()
        else {
            return OnSiteActionDecision::Hold;
        };
        let camp_policy = adventuresim_core::strategic_time::CampDurationPolicy::FixedMinutes(
            party.fixed_camp_minutes,
        );
        let has_tent = self.party_item_quantity(party_id, PARTY_TENT_ITEM_ID) > 0;
        let recovery_members = member_ids
            .iter()
            .map(|character_id| {
                let attributes = self
                    .connection
                    .db
                    .backend_character_attributes()
                    .iter()
                    .find(|row| row.character_id == *character_id)?;
                let limbs = self
                    .connection
                    .db
                    .backend_character_limbs()
                    .iter()
                    .find(|row| row.character_id == *character_id)?;
                let stats = self
                    .connection
                    .db
                    .backend_character_stats()
                    .iter()
                    .find(|row| row.character_id == *character_id)?;
                let schedule = self
                    .connection
                    .db
                    .backend_character_training_schedules()
                    .iter()
                    .find(|row| row.character_id == *character_id)?;
                Some(adventuresim_core::strategic_time::ItineraryMember {
                    fatigue_capacity: (attributes.endurance * limbs.chest_health).max(0.01)
                        * 1_000.0,
                    calories_used: stats.calories_used,
                    camp_schedule: adventuresim_core::strategic_schedule::DailySchedule {
                        combat_training_minutes: schedule.downtime.combat_training_minutes,
                        carousing_minutes: schedule.downtime.carousing_minutes,
                        socializing_minutes: schedule.downtime.socializing_minutes,
                        prayer: schedule.downtime.prayer_minutes,
                        ..Default::default()
                    },
                })
            })
            .collect::<Option<Vec<_>>>();
        let Some(recovery_members) = recovery_members else {
            return OnSiteActionDecision::Hold;
        };
        let recovery_minutes =
            adventuresim_core::strategic_time::common_fatigue_clear_minutes(&recovery_members);
        let bounded_recovery = (1..=1_440).contains(&recovery_minutes);
        let mut action_return_safe = true;
        let mut rest_action_return_safe = bounded_recovery;
        let mut return_now_safe = true;
        for (member_index, character_id) in member_ids.into_iter().enumerate() {
            let Some(condition) = self
                .connection
                .db
                .backend_character_strategic_conditions()
                .iter()
                .find(|row| row.character_id == character_id)
            else {
                return OnSiteActionDecision::Hold;
            };
            let medically_critical = self
                .connection
                .db
                .character_illness_status()
                .iter()
                .find(|row| row.character_id == character_id)
                .is_some_and(|row| row.critical);
            let Some(insulation_bps) = self.public_equipped_insulation_bps(character_id) else {
                return OnSiteActionDecision::Hold;
            };
            let attributes = self
                .connection
                .db
                .backend_character_attributes()
                .iter()
                .find(|row| row.character_id == character_id);
            let limbs = self
                .connection
                .db
                .backend_character_limbs()
                .iter()
                .find(|row| row.character_id == character_id);
            let stats = self
                .connection
                .db
                .backend_character_stats()
                .iter()
                .find(|row| row.character_id == character_id);
            let schedule = self
                .connection
                .db
                .backend_character_training_schedules()
                .iter()
                .find(|row| row.character_id == character_id);
            let (Some(attributes), Some(limbs), Some(stats), Some(schedule)) =
                (attributes, limbs, stats, schedule)
            else {
                return OnSiteActionDecision::Hold;
            };
            let fatigue_capacity = (attributes.endurance * limbs.chest_health).max(0.01) * 1_000.0;
            let camp_schedule = adventuresim_core::strategic_schedule::DailySchedule {
                combat_training_minutes: schedule.downtime.combat_training_minutes,
                carousing_minutes: schedule.downtime.carousing_minutes,
                socializing_minutes: schedule.downtime.socializing_minutes,
                prayer: schedule.downtime.prayer_minutes,
                ..Default::default()
            };
            let member = |calories_used| {
                [adventuresim_core::strategic_time::ItineraryMember {
                    fatigue_capacity,
                    calories_used,
                    camp_schedule,
                }]
            };
            let Some(return_now) = adventuresim_core::strategic_time::forecast_itinerary(
                starting_minute,
                movement_minutes,
                party.walking_minutes_per_day,
                party.travel_at_night,
                camp_policy,
                &member(stats.calories_used),
            ) else {
                return OnSiteActionDecision::Hold;
            };
            let action_calories =
                calories_after_strenuous_action(stats.calories_used, action_minutes);
            let Some(return_after_action) = adventuresim_core::strategic_time::forecast_itinerary(
                starting_minute.saturating_add(action_minutes),
                movement_minutes,
                party.walking_minutes_per_day,
                party.travel_at_night,
                camp_policy,
                &member(action_calories),
            ) else {
                return OnSiteActionDecision::Hold;
            };
            let nonfatigue_incapacitation = (condition.incapacitation - condition.fatigue).max(0.0);
            let actor_ready_before_action =
                character_id != pin.owner_character_id || condition.status == "ready";
            let action_survivable = projected_action_survivable(
                nonfatigue_incapacitation,
                action_calories,
                fatigue_capacity,
            );
            let state = adventuresim_core::survival::SurvivalState {
                wetness_bps: condition.wetness_bps,
                thermal_strain: condition.thermal_strain,
                frostbite_progress_minutes: 0,
            };
            return_now_safe &= projected_itinerary_thermal_safe(
                starting_minute,
                &return_now,
                site,
                origin,
                state,
                insulation_bps,
                has_tent,
            )
            .unwrap_or(false)
                && projected_itinerary_survivable(
                    nonfatigue_incapacitation,
                    &return_now,
                    0,
                    fatigue_capacity,
                );
            let Some(action) = projected_stationary_outdoor_thermal_state(
                starting_minute,
                action_minutes,
                site,
                state,
                insulation_bps,
            ) else {
                return OnSiteActionDecision::Hold;
            };
            action_return_safe &= action.safe
                && projected_itinerary_thermal_safe(
                    starting_minute.saturating_add(action_minutes),
                    &return_after_action,
                    site,
                    origin,
                    action.state,
                    insulation_bps,
                    has_tent,
                )
                .unwrap_or(false)
                && actor_ready_before_action
                && action_survivable
                && projected_itinerary_survivable(
                    nonfatigue_incapacitation,
                    &return_after_action,
                    0,
                    fatigue_capacity,
                )
                && !medically_critical;
            if bounded_recovery {
                let recovered_calories = adventuresim_core::strategic_time::camp_fatigue_after(
                    stats.calories_used,
                    recovery_minutes,
                    recovery_members[member_index].camp_schedule,
                );
                let recovered_action_calories =
                    calories_after_strenuous_action(recovered_calories, action_minutes);
                let Some(recovered_return) = adventuresim_core::strategic_time::forecast_itinerary(
                    starting_minute
                        .saturating_add(recovery_minutes)
                        .saturating_add(action_minutes),
                    movement_minutes,
                    party.walking_minutes_per_day,
                    party.travel_at_night,
                    camp_policy,
                    &member(recovered_action_calories),
                ) else {
                    return OnSiteActionDecision::Hold;
                };
                if recovered_return.truncated {
                    return OnSiteActionDecision::Hold;
                }
                let recovery = projected_stationary_field_thermal_state(
                    starting_minute,
                    recovery_minutes,
                    site,
                    state,
                    insulation_bps,
                    has_tent,
                );
                let recovery_action_return_thermal_safe = recovery
                    .and_then(|recovery| {
                        let action = projected_stationary_outdoor_thermal_state(
                            starting_minute.saturating_add(recovery_minutes),
                            action_minutes,
                            site,
                            recovery.state,
                            insulation_bps,
                        )?;
                        Some(
                            recovery.safe
                                && action.safe
                                && projected_itinerary_thermal_safe(
                                    starting_minute
                                        .saturating_add(recovery_minutes)
                                        .saturating_add(action_minutes),
                                    &recovered_return,
                                    site,
                                    origin,
                                    action.state,
                                    insulation_bps,
                                    has_tent,
                                )?,
                        )
                    })
                    .unwrap_or(false);
                rest_action_return_safe &= recovery_action_return_thermal_safe
                    && (character_id != pin.owner_character_id
                        || projected_action_ready(
                            nonfatigue_incapacitation,
                            recovered_calories,
                            fatigue_capacity,
                        ))
                    && projected_action_survivable(
                        nonfatigue_incapacitation,
                        recovered_action_calories,
                        fatigue_capacity,
                    )
                    && projected_itinerary_survivable(
                        nonfatigue_incapacitation,
                        &recovered_return,
                        0,
                        fatigue_capacity,
                    )
                    && !medically_critical;
            }
        }
        classify_on_site_action_decision(
            action_return_safe,
            rest_action_return_safe,
            recovery_minutes,
            return_now_safe,
        )
    }

    pub(super) fn prepare_party_for_departure(
        &mut self,
        party_id: &str,
        leader: u64,
        leader_agent: u32,
    ) -> Result<DepartureReadiness, String> {
        let party = self.party_by_id(party_id)?;
        let Some(settlement_id) = party.current_settlement_id.clone() else {
            return Ok(DepartureReadiness::Deferred(
                "survival_readiness_requires_settlement",
            ));
        };
        if let DepartureReadiness::Deferred(reason) =
            self.ensure_party_tent(party_id, &settlement_id, leader_agent)?
        {
            return Ok(DepartureReadiness::Deferred(reason));
        }
        let party_agents = self.party_agents(leader)?;
        for agent in party_agents {
            self.try_upgrade(agent, &settlement_id)?;
        }
        if let DepartureReadiness::Deferred(reason) =
            self.ensure_ranged_ammunition(party_id, &settlement_id)?
        {
            return Ok(DepartureReadiness::Deferred(reason));
        }
        self.observe_survival_telemetry(party_id);
        let (load, capacity, remaining_bps) = self.public_party_load_and_capacity(party_id);
        if let DepartureReadiness::Deferred(reason) =
            self.validate_party_departure_readiness(party_id)
        {
            if reason == "party_load_unsafe" {
                self.metrics.load_readiness_suppressions =
                    self.metrics.load_readiness_suppressions.saturating_add(1);
            }
            if reason == "thermal_recovery_required" {
                self.metrics.current_condition_readiness_suppressions = self
                    .metrics
                    .current_condition_readiness_suppressions
                    .saturating_add(1);
            }
            return Ok(DepartureReadiness::Deferred(reason));
        }
        self.event(
            leader_agent,
            CoreLoopEventKind::QuestDecision,
            format!(
                "survival_readiness=ready;party={};tent_quantity={};load_kg={load:.3};capacity_kg={capacity:.3};encumbrance_remaining_bps={remaining_bps};ammo_floor={RANGED_AMMUNITION_FLOOR};route_weather_projection=pending_exact_case_site",
                bounded_event_field(party_id),
                self.party_item_quantity(party_id, PARTY_TENT_ITEM_ID),
            ),
        );
        Ok(DepartureReadiness::Ready)
    }

    pub(super) fn rest_at_camp_with_party_shelter(
        &mut self,
        character_id: u64,
        minutes: u64,
        operation: &str,
    ) -> Result<FieldShelter, String> {
        let party_id = self
            .connection
            .db
            .backend_characters()
            .iter()
            .find(|row| row.id == character_id)
            .and_then(|row| row.party_id)
            .ok_or("field-rest actor has no public party")?;
        let shelter = if self.party_item_quantity(&party_id, PARTY_TENT_ITEM_ID) > 0 {
            FieldShelter::Tent
        } else {
            FieldShelter::Bivouac
        };
        self.observe_survival_telemetry(&party_id);
        let result = reducer_call!(self, operation, |cb| self
            .connection
            .reducers
            .rest_at_camp_then(character_id, minutes, shelter.clone(), cb));
        match self.call(result) {
            Ok(()) => {
                if matches!(shelter, FieldShelter::Tent) {
                    self.metrics.tent_field_rests = self.metrics.tent_field_rests.saturating_add(1);
                } else {
                    self.metrics.bivouac_field_rests =
                        self.metrics.bivouac_field_rests.saturating_add(1);
                }
                self.observe_survival_telemetry(&party_id);
                Ok(shelter)
            }
            Err(error) => {
                if matches!(shelter, FieldShelter::Tent) {
                    self.metrics.tent_field_rest_failures =
                        self.metrics.tent_field_rest_failures.saturating_add(1);
                }
                Err(error)
            }
        }
    }
}
