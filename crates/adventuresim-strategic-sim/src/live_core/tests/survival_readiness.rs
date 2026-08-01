#[test]
fn departure_readiness_applies_continuous_load_thermal_and_ammo_floors() {
    assert_eq!(public_encumbrance_remaining_bps(0.0, 100.0), 10_000);
    assert_eq!(public_encumbrance_remaining_bps(80.0, 100.0), 2_000);
    assert_eq!(public_encumbrance_remaining_bps(81.0, 100.0), 1_900);
    assert!(survival_equipment_ready(
        "ready",
        MAX_DEPARTURE_WETNESS_BPS,
        MAX_DEPARTURE_ABS_THERMAL_STRAIN as i32,
        true,
        RANGED_AMMUNITION_FLOOR,
        MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS,
    ));
    assert!(!survival_equipment_ready(
        "ready",
        MAX_DEPARTURE_WETNESS_BPS,
        0,
        true,
        RANGED_AMMUNITION_FLOOR - 1,
        MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS,
    ));
    assert!(!survival_equipment_ready(
        "ready",
        MAX_DEPARTURE_WETNESS_BPS + 1,
        0,
        false,
        0,
        MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS,
    ));
}

#[test]
fn every_field_rest_uses_the_single_party_shelter_boundary() {
    let production = LIVE_CORE_SOURCE.split("#[cfg(test)]").next().unwrap();
    assert_eq!(production.matches(".rest_at_camp_then(").count(), 1);
    assert_eq!(
        production
            .matches("rest_at_camp_with_party_shelter(")
            .count(),
        4,
        "one helper plus ordinary camp, recovery/passive rest, and investigation wait"
    );
    let helper = production
        .split("fn rest_at_camp_with_party_shelter")
        .nth(1)
        .expect("party shelter helper");
    assert!(helper.contains("FieldShelter::Tent"));
    assert!(helper.contains("PARTY_TENT_ITEM_ID"));
    assert!(helper.contains("tent_field_rests"));
    assert!(helper.contains("tent_field_rest_failures"));
}

#[test]
fn readiness_buys_shared_tent_and_personal_ammunition_through_ordinary_trade() {
    let source = LIVE_CORE_SOURCE;
    let tent = source
        .split("fn ensure_party_tent")
        .nth(1)
        .and_then(|tail| tail.split("fn ensure_ranged_ammunition").next())
        .expect("tent readiness");
    assert!(tent.contains("finalize_merchant_trade_then"));
    assert!(tent.contains("PARTY_TENT_ITEM_ID.to_owned()"));
    assert!(tent.contains("party tent purchase completed without party custody"));
    assert!(tent.contains("true,"), "tent purchase must use party scope");
    assert!(tent.contains("public_general_storefront_exists"));
    assert!(tent.contains("tent_provider_unavailable_bivouac"));
    assert!(tent.contains("shelter=bivouac"));
    assert!(tent.contains("return Ok(DepartureReadiness::Ready)"));

    let ammo = source
        .split("fn ensure_ranged_ammunition")
        .nth(1)
        .and_then(|tail| tail.split("fn validate_party_departure_readiness").next())
        .expect("ammunition readiness");
    assert!(ammo.contains("capabilities()"));
    assert!(ammo.contains("RANGED_AMMUNITION_FLOOR"));
    assert!(ammo.contains("withdraw_stake_for_personal_purchase"));
    assert!(ammo.contains("finalize_merchant_trade_then"));
    assert!(ammo.contains("false,"), "arrows must remain personal");
    assert!(ammo.contains("ammo_before"));
    assert!(ammo.contains("ammo_after"));
}

#[test]
fn preparation_is_party_wide_and_rejects_overweight_upgrades() {
    let source = LIVE_CORE_SOURCE;
    let preparation = source
        .split("fn prepare_party_for_departure")
        .nth(1)
        .and_then(|tail| tail.split("fn rest_at_camp_with_party_shelter").next())
        .expect("party readiness preparation");
    assert!(preparation.contains("for agent in party_agents"));
    assert!(preparation.contains("self.try_upgrade(agent"));
    assert!(preparation.contains("ensure_ranged_ammunition"));
    assert!(preparation.contains("validate_party_departure_readiness"));

    let upgrades = source
        .split("fn try_upgrade")
        .nth(1)
        .expect("upgrade policy");
    assert!(upgrades.contains("public_party_load_and_capacity"));
    assert!(upgrades.contains("MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS"));
    assert!(upgrades.contains("public_equipment_storefront_offer"));
    assert!(upgrades.contains(
        "purchase_personal_storefront_with_party_stake_then"
    ));
    assert!(upgrades.contains("earned_shortfall"));
    assert!(upgrades.contains("medical_reserve"));

    let storefront = source
        .split("fn public_equipment_storefront_offer")
        .nth(1)
        .and_then(|tail| tail.split("fn withdraw_stake_for_personal_purchase").next())
        .expect("equipment storefront routing");
    for route in [
        "Storefront::Weapons",
        "Storefront::Armor",
        "Storefront::Clothing",
    ] {
        assert!(
            storefront.contains(route),
            "missing storefront route {route}"
        );
    }
    assert!(storefront.contains("public_storefront_available"));
    assert!(upgrades.contains("storefront_offer_unchanged"));
    assert!(upgrades.contains("stake_before_trade"));
    assert!(upgrades.contains("stake_after_trade"));
}

#[test]
fn resident_presence_cannot_create_an_unavailable_equipment_storefront() {
    let canonical = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
    let profile = SettlementEconomyProfile {
        rules_version: canonical.rules_version,
        prosperity_score: 0,
        prosperity_tier: ProsperityTier::Subsistence,
        services: vec![SettlementService::Inn],
        specializations: vec![],
        stock: vec![],
    };
    assert!(!public_storefront_available(
        &profile,
        adventuresim_core::settlement_economy::Storefront::Weapons,
    ));
}

#[test]
fn equipment_storefront_requires_service_and_matching_stock_category() {
    let canonical = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
    let mut profile = SettlementEconomyProfile {
        rules_version: canonical.rules_version,
        prosperity_score: 0,
        prosperity_tier: ProsperityTier::Subsistence,
        services: vec![SettlementService::Weaponsmith],
        specializations: vec![],
        stock: vec![SettlementStock {
            category: StockCategory::GeneralGoods,
            abundance: 1,
            provenance: ProfileFactProvenance::DeterministicGapFill,
        }],
    };
    let storefront = adventuresim_core::settlement_economy::Storefront::Weapons;
    let projected = public_settlement_economy_profile(&profile).unwrap();
    assert!(!adventuresim_core::settlement_economy::storefront_stocks(
        &projected,
        storefront,
        "club",
        adventuresim_core::settlement_economy::CatalogKind::Weapon,
    ));
    profile.stock.insert(
        0,
        SettlementStock {
            category: StockCategory::Weapons,
            abundance: 1,
            provenance: ProfileFactProvenance::DeterministicGapFill,
        },
    );
    let projected = public_settlement_economy_profile(&profile).unwrap();
    assert!(adventuresim_core::settlement_economy::storefront_stocks(
        &projected,
        storefront,
        "club",
        adventuresim_core::settlement_economy::CatalogKind::Weapon,
    ));
}

#[test]
fn staggered_default_providers_remain_ambiguous_before_hours_filtering() {
    assert_eq!(
        visible_unique_default_provider(&[(7, 0, 720), (8, 720, 1_440)], 300),
        None
    );
    assert_eq!(visible_unique_default_provider(&[(7, 0, 720)], 300), Some(7));
    assert_eq!(visible_unique_default_provider(&[(7, 0, 720)], 900), None);
}

#[test]
fn equipment_quote_revalidation_is_fail_closed() {
    let selected = ("weapons".to_string(), 7, 12);
    assert!(storefront_offer_unchanged(
        &selected,
        Some(selected.clone())
    ));
    assert!(!storefront_offer_unchanged(&selected, None));
    assert!(!storefront_offer_unchanged(
        &selected,
        Some(("armor".into(), 7, 12)),
    ));
    assert!(!storefront_offer_unchanged(
        &selected,
        Some(("weapons".into(), 8, 12)),
    ));
    assert!(!storefront_offer_unchanged(
        &selected,
        Some(("weapons".into(), 7, 13)),
    ));
}

#[test]
fn departure_checks_only_living_members_and_records_public_weather_boundary() {
    let source = LIVE_CORE_SOURCE;
    let living = source
        .split("fn living_party_member_ids")
        .nth(1)
        .and_then(|tail| tail.split("fn item_definition").next())
        .expect("living party projection");
    assert!(living.contains(".filter(|row| row.party_id == party_id)"));
    assert!(living.contains("character.id == row.character_id"));
    assert!(living.contains("character.alive"));

    for (helper, next_helper) in [
        ("ensure_party_tent", "ensure_ranged_ammunition"),
        ("ensure_ranged_ammunition", "validate_party_departure_readiness"),
        ("validate_party_departure_readiness", "prepare_party_for_departure"),
    ] {
        let body = source
            .split(&format!("fn {helper}"))
            .nth(1)
            .and_then(|tail| tail.split(&format!("fn {next_helper}")).next())
            .expect(helper);
        assert!(
            body.contains("living_party_member_ids"),
            "{helper} must exclude dead members"
        );
    }
    assert!(source.contains("route_weather_projection=unavailable"));
    assert!(source.contains("weather_gate=current_public_condition_only"));
}

#[test]
fn survival_report_schema_has_per_agent_aggregate_and_failure_context() {
    let metrics = serde_json::to_value(CoreLoopMetrics::default()).unwrap();
    for field in [
        "party_tents_purchased",
        "tent_field_rests",
        "tent_field_rest_failures",
        "ammunition_units_purchased",
        "ammunition_shortage_suppressions",
        "tent_provider_unavailable_bivouac_departures",
        "current_condition_readiness_suppressions",
        "route_weather_projection_unavailable_departures",
        "survival_observations",
        "max_party_carried_load_grams",
        "max_party_carry_capacity_grams",
        "min_party_encumbrance_remaining_bps",
        "max_observed_wetness_bps",
        "max_observed_abs_thermal_strain",
    ] {
        assert!(metrics.get(field).is_some(), "missing {field}");
    }
    let source = LIVE_CORE_SOURCE;
    let death = source
        .split("fn observe_deaths")
        .nth(1)
        .expect("death telemetry");
    for field in [
        "cause=",
        "source=",
        "source_id=",
        "strategic_minute=",
        "thermal=",
        "wetness_bps=",
        "ammo=",
        "carried_load_kg=",
        "equipment_ready=",
        "party_tent_quantity=",
    ] {
        assert!(death.contains(field), "missing death context {field}");
    }
    assert_eq!(CORE_LOOP_FAILURE_SCHEMA_VERSION, 9);
    assert_eq!(crate::FORMAT_VERSION, 9);
}
