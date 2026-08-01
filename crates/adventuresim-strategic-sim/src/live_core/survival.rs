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
        let body_weight = self
            .connection
            .db
            .backend_character_conditions()
            .iter()
            .find(|row| row.character_id == character_id)
            .map_or(70.0, |row| {
                if row.body_weight_kg.is_finite() && (20.0..=300.0).contains(&row.body_weight_kg) {
                    row.body_weight_kg
                } else {
                    70.0
                }
            });
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
        body_weight + water_weight + inventory_weight
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
        (load, capacity, public_encumbrance_remaining_bps(load, capacity))
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
        let party_tent_quantity = character
            .party_id
            .as_deref()
            .map_or(0, |party_id| self.party_item_quantity(party_id, PARTY_TENT_ITEM_ID));
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
        self.metrics.survival_observations =
            self.metrics.survival_observations.saturating_add(1);
        let member_ids = self.living_party_member_ids(party_id);
        for character_id in member_ids {
            if let Some(condition) = self
                .connection
                .db
                .backend_character_strategic_conditions()
                .iter()
                .find(|row| row.character_id == character_id)
            {
                self.metrics.max_observed_wetness_bps =
                    self.metrics.max_observed_wetness_bps.max(condition.wetness_bps);
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
            matches!(service, SettlementService::Market | SettlementService::GeneralStore)
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
            .find(|row| {
                row.character_id == character_id && row.settlement_id == settlement_id
            })?
            .buy_bps;
        let base = adventuresim_core::strategic_economy::merchant_buy_price(
            item.base_value.unwrap_or(1),
        );
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
                    matches!(service, SettlementService::Market | SettlementService::GeneralStore)
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
            .filter(|npc| {
                npc.home_settlement_id == settlement_id && npc.service_id == service_id
            })
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
                    .map(|presence| {
                        (
                            npc.character_id,
                            presence.start_minute,
                            presence.end_minute,
                        )
                    })
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
            .find(|row| {
                row.character_id == character_id && row.settlement_id == settlement_id
            })?
            .buy_bps;
        let base = adventuresim_core::strategic_economy::merchant_buy_price(
            item.base_value.unwrap_or(1),
        );
        let language_upper_bound =
            adventuresim_core::strategic_economy::language_adjusted_buy_price(base, 0.0);
        let quote = adventuresim_core::local_problem::adjust_price(
            language_upper_bound,
            buy_bps,
        );
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
        if treasury.iter().map(|row| u64::from(row.quantity)).sum::<u64>() < needed {
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
                let spendable = party_coin
                    .saturating_add(purse)
                    .saturating_sub(reserve);
                Some((spendable >= quote, spendable, character_id, purse, reserve, quote))
            })
            .collect::<Vec<_>>();
        payers.sort_by_key(|payer| (payer.0, payer.1, payer.2));
        let Some((affordable, _, payer, purse_before, reserve, quote)) = payers.pop() else {
            let public_provider_exists = self.living_party_member_ids(party_id).into_iter().any(
                |character_id| {
                    self.public_general_storefront_exists(character_id, settlement_id)
                },
            );
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
        self.metrics.party_tent_gold_spent =
            self.metrics.party_tent_gold_spent.saturating_add(actual_spent);
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
            let actual_spent = purse_after_withdrawal.saturating_sub(self.personal_gold(character_id));
            self.metrics.ammunition_purchases =
                self.metrics.ammunition_purchases.saturating_add(1);
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

    pub(super) fn validate_party_departure_readiness(
        &self,
        party_id: &str,
    ) -> DepartureReadiness {
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
                || observation.thermal_strain.unsigned_abs()
                    > MAX_DEPARTURE_ABS_THERMAL_STRAIN
            {
                return DepartureReadiness::Deferred("thermal_recovery_required");
            }
            if !observation.equipment_ready {
                return DepartureReadiness::Deferred("equipment_not_ready");
            }
        }
        DepartureReadiness::Ready
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
        self.metrics.route_weather_projection_unavailable_departures = self
            .metrics
            .route_weather_projection_unavailable_departures
            .saturating_add(1);
        self.event(
            leader_agent,
            CoreLoopEventKind::QuestDecision,
            format!(
                "survival_readiness=ready;party={};tent_quantity={};load_kg={load:.3};capacity_kg={capacity:.3};encumbrance_remaining_bps={remaining_bps};ammo_floor={RANGED_AMMUNITION_FLOOR};route_weather_projection=unavailable;weather_gate=current_public_condition_only",
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
                    self.metrics.tent_field_rests =
                        self.metrics.tent_field_rests.saturating_add(1);
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
