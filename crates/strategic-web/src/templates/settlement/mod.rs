//! Settlement templates.
//!
//! Settlement pages deliberately keep the same ownership model: services and
//! settlement-owned information on the left, service context in the center,
//! and the active player's party on the right.

use adventuresim_core::{
    activity::{PRAYER_MORALE_LIMIT, PRAYER_MORALE_SCALE_MINUTES, settlement_population_scale},
    bestiary::ThreatId,
    equipment::EncumbranceSummary,
    prelude::Skill,
    strategic_schedule::{
        BASELINE_FATIGUE_PER_DAY, CombatTrainingProfile, DailySchedule,
        FATIGUE_RESERVOIR_PER_PREVIEW_POINT, LABOR_FATIGUE_PER_HOUR,
        LEISURE_FATIGUE_RECOVERY_PER_HOUR, LEISURE_MORALE_LIMIT, LEISURE_MORALE_SCALE_FATIGUE,
        LeisureOutcome, settlement_leisure_outcome,
    },
    strategic_time::{ItinerarySegment, ItinerarySegmentKind, MINUTES_PER_DAY},
};
use adventuresim_world_schema::OfficialReligion;
use maud::{Markup, html};
use std::{collections::BTreeSet, fmt, str::FromStr};

use super::inventory_browser::{InventoryBrowser, InventoryColumnSet};
use super::{
    camp_location_layout_with_session, decorative_game_icon, empty_state, game_icon,
    item_display_name, item_type_header, item_type_icon, population_description,
    quest_location_layout_with_session, religion_icon, settlement_layout_with_session,
    sidebar_section, stat_icon_path,
};
use crate::medical::MedicalPresentation;
use crate::routes::travel::{TravelDestination, TravelProvisionForecast};
use crate::spacetimedb::{
    Character, CharacterApprenticeship, CharacterAttributes, CharacterCapability,
    CharacterCondition, CharacterEquip, CharacterLimbs, CharacterSkills, CharacterStats,
    CharacterStrategicCondition, CharacterTrainingSchedule, ContractPresentation, FoodLot,
    InventoryItem, InventoryQuantityTarget, ItemDefinition, ItemSlot, JourneyTerrainKind,
    LimbInjury, LimbRegion, Party, PartyInventoryItem, PartyJourney, PartyJourneyItinerary,
    PartyJourneyRoute, ProjectileKind, RetainedProjectile, ScheduleAllocation, Settlement,
    SettlementAlias, SettlementCategory, SettlementDescription, SettlementDescriptionKind,
    StrategicEncounter,
};

mod character_details;
mod character_health;
mod character_skills;
mod chrome;
mod context;
mod rest;
mod social;
mod trade;
mod travel;

pub use character_details::*;
pub use character_health::*;
pub use character_skills::*;
pub use chrome::*;
pub use context::*;
pub use rest::*;
pub use social::*;
pub use trade::*;
pub use travel::*;

#[cfg(test)]
mod tests {
    use super::{
        Character, CharacterCondition, LocationKind, MerchantShop, RestSummary, SoapRestPreview,
        encumbrance_inventory_rail, encumbrance_meter, filth_status_bar, format_rest_duration,
        live_merchant_shop_page, merchant_inventory_sell_price, merchant_inventory_weight,
        need_balance_meter, repair_custody_panel, repair_submit_control, rest_default_minutes,
        rest_service_menu, settlement_rest_duration_control, strategic_condition_rail,
        strategic_encounter_panel,
    };
    use crate::spacetimedb::{
        CharacterFilth, CharacterStrategicCondition, FilthOrigin, FilthSubstance, FoodLot,
        FoodPreparation, ItemKind, StrategicEncounter, StrategicEncounterLoss,
    };
    use adventuresim_core::equipment::EncumbranceSummary;

    #[test]
    fn post_rest_result_points_live_refresh_at_a_gettable_location() {
        let summary = RestSummary {
            minutes: 480,
            gold_spent: 1,
            gold_earned: 0,
            notoriety_gained: 0.0,
            healed: Vec::new(),
            trained: Vec::new(),
        };
        let markup = rest_service_menu(
            "Inn",
            "riverdale",
            "inn",
            None,
            Some(&summary),
            SoapRestPreview::default(),
        )
        .into_string();
        assert!(markup.contains("data-live-refresh-url=\"/settlements/riverdale/inn\""));
        assert!(!markup.contains("data-live-refresh-url=\"/settlements/riverdale/rest/inn\""));
    }

    #[test]
    fn encounter_panel_renders_only_authoritative_choices_and_exact_losses() {
        let encounter = StrategicEncounter {
            party_id: "party".into(),
            encounter_id: "party:3".into(),
            archetype: "bandits".into(),
            enemy_count: 4,
            roll_index: 3,
            journey_movement_minute: 540,
            journey_elapsed_minute: 700,
            absolute_minute: 1_700,
            longitude_e7: 1,
            latitude_e7: 2,
            terrain: "road".into(),
            party_aware: false,
            enemy_aware: true,
            available_choices: vec!["attack".into(), "surrender".into()],
            status: "awaiting_choice".into(),
            selected_choice: None,
            selection_explanation: "deterministic awareness".into(),
            party_speed_m_per_minute: 60,
            enemy_speed_m_per_minute: 80,
            run_ineligibility: Some("too slow".into()),
            penalty_minutes: 0,
            loss_preview: vec![StrategicEncounterLoss {
                owner_kind: "member".into(),
                owner_id: 7,
                inventory_id: 8,
                item_id: "gold_coin".into(),
                quantity: 12,
                value_each: 1,
            }],
            outcome: None,
        };
        let rendered = strategic_encounter_panel(&encounter).into_string();
        assert!(rendered.contains("The enemy surprised your party"));
        assert!(rendered.contains("Cannot run: too slow"));
        assert!(rendered.contains("12 × gold_coin"));
        assert!(rendered.contains("value=\"attack\""));
        assert!(rendered.contains("value=\"surrender\""));
        assert!(!rendered.contains("value=\"run\""));
        assert!(!rendered.contains("value=\"sneak\""));
    }

    #[test]
    fn inferred_general_blacksmith_exposes_limited_weapon_and_armor_stock() {
        let industries = adventuresim_world_schema::InferredIndustryProfile::new(vec![
            adventuresim_world_schema::IndustryEvidence::Fallback(
                adventuresim_world_schema::FallbackIndustry::CommonAggregate,
            ),
        ])
        .unwrap();
        let economy =
            adventuresim_world_schema::infer_settlement_economy(2, 500, 1, false, &industries)
                .unwrap();

        assert!(adventuresim_core::settlement_economy::storefront_stocks(
            &economy,
            adventuresim_core::settlement_economy::Storefront::Weapons,
            "club",
            adventuresim_core::settlement_economy::CatalogKind::Weapon,
        ));
        assert!(adventuresim_core::settlement_economy::storefront_stocks(
            &economy,
            adventuresim_core::settlement_economy::Storefront::Armor,
            "leather vest",
            adventuresim_core::settlement_economy::CatalogKind::Armor,
        ));
        for category in [
            adventuresim_world_schema::StockCategory::Weapons,
            adventuresim_world_schema::StockCategory::Armor,
        ] {
            assert_eq!(
                economy
                    .stock
                    .iter()
                    .find(|stock| stock.category == category)
                    .unwrap()
                    .abundance,
                1
            );
        }
    }

    #[test]
    fn merchant_food_quote_and_weight_follow_remaining_lot() {
        let mut lot = FoodLot {
            id: 1,
            inventory_item_id: Some(9),
            party_inventory_item_id: None,
            display_name: "Cooked meal".into(),
            preparation: FoodPreparation::Stewed,
            ingredient_item_ids: vec!["raw_venison".into()],
            ingredient_quantities: vec![1.0],
            mass_kg: 25.0,
            nutrition_kcal: 5_000.0,
            total_value: 10.0,
            created_at_minute: 1,
        };
        assert_eq!(merchant_inventory_weight(None, Some(&lot)), "25");
        assert_eq!(merchant_inventory_sell_price(None, Some(&lot)), 8);
        lot.mass_kg = 6.25;
        lot.total_value = 2.5;
        assert_eq!(merchant_inventory_weight(None, Some(&lot)), "6.25");
        assert_eq!(merchant_inventory_sell_price(None, Some(&lot)), 2);
        lot.total_value = 0.5;
        let zero = merchant_inventory_sell_price(None, Some(&lot));
        assert_eq!(zero, 0);
        assert_eq!(
            adventuresim_core::strategic_economy::language_adjusted_sell_price(zero, 0.0),
            0
        );
    }

    #[test]
    fn public_filth_serialization_and_template_expose_only_aggregate_origin() {
        let deposit = CharacterFilth {
            id: 1,
            character_id: 7,
            substance: FilthSubstance::Blood,
            origin: FilthOrigin::Foreign,
            amount: 2,
            deposited_at: 10,
        };
        let serialized = serde_json::to_value(&deposit).unwrap();
        assert!(serialized.get("source_character_id").is_none());
        assert_eq!(
            serialized.get("origin").and_then(|value| value.as_str()),
            Some("Foreign")
        );
        let markup = filth_status_bar(&[deposit]).into_string();
        assert!(markup.contains("2 foreign"));
        assert!(!markup.contains("source_character_id"));
        assert!(!markup.contains("filth-legend"));
        assert!(!markup.contains("/100 filth</span>"));
        assert!(markup.contains("data-strategic-tooltip=\"Filth accumulates"));
        assert!(markup.contains("data-strategic-tooltip=\"Blood\n2\""));
    }

    #[test]
    fn status_rail_places_filth_after_water() {
        let condition = CharacterStrategicCondition {
            character_id: 7,
            morale: 0.0,
            morale_bonus: 0.0,
            morale_bonus_cap: 0.0,
            fervor: 0.0,
            pain: 0.0,
            blood_loss: 0.0,
            fear: 0.0,
            fatigue: 0.0,
            hunger: 0.0,
            thirst: 0.0,
            food_days: 1.0,
            water_days: 1.0,
            water_capacity_ml: 2_000,
            incapacitation: 0.0,
            check_multiplier: 1.0,
            status: "ready".into(),
        };
        let markup =
            strategic_condition_rail(Some(&condition), &[], &[], "/social", false).into_string();
        assert!(markup.contains("class=\"morale-meter\""));
        assert!(markup.contains("href=\"/social\" title=\"Open social menu\""));
        assert!(markup.contains("aria-haspopup=\"dialog\" aria-expanded=\"false\""));
        let water = markup.find("Water").expect("water meter");
        let filth = markup.find("Filth").expect("filth meter");
        assert!(water < filth);
    }

    #[test]
    fn social_catalog_labels_are_generic_grounded_and_accessible() {
        use adventuresim_core::social::{SocialActionKind, SocialTopic};
        let defeat = SocialActionKind::Commiserate.description(SocialTopic::Defeat, true);
        assert_eq!(defeat, "Commiserate about the defeat");
        assert!(!defeat.to_ascii_lowercase().contains("goblin"));
        let actions = social_actions(false, SocialTopic::Defeat);
        assert_eq!(actions.len(), 6);
        assert!(
            actions
                .iter()
                .any(|(_, action, _)| *action == SocialActionKind::Listen)
        );
        assert_eq!(
            social_actions(true, SocialTopic::Defeat),
            vec![("inner-self", SocialActionKind::Reflect, "reflect")]
        );
        assert_eq!(social_actions(false, SocialTopic::Hunger).len(), 3);
        assert_eq!(social_actions(false, SocialTopic::Faith).len(), 4);
        assert_eq!(SocialActionKind::Commiserate.skill_name(false), "Deception");
        assert_eq!(
            SocialActionKind::Flirt.description(SocialTopic::Injury, false),
            "Tell them the scar makes them look striking"
        );
        assert_eq!(familiarity_label(0.0), "0 hours");
        assert_eq!(familiarity_label(0.4), "<1 hours");
        assert_eq!(familiarity_label(9.4), "9 hours");
        let tooltip = belief_tooltip(&crate::spacetimedb::SocialBelief {
            id: "belief".into(),
            observer_id: 1,
            subject_id: 2,
            axis: "self_regard".into(),
            perceived_value: 1,
            confidence: 0.64,
            observed_at_minute: 0,
        });
        assert!(tooltip.contains("Confidence: 64%"));
        assert!(tooltip.contains("Injury is touchy"));
    }

    #[test]
    fn social_skill_family_has_an_average_and_six_expandable_icon_rows() {
        let skills = CharacterSkills {
            character_id: 7,
            insight_hours: 100.0,
            self_awareness_hours: 80.0,
            humor_hours: 60.0,
            command_hours: 40.0,
            deception_hours: 20.0,
            seduction_hours: 10.0,
            ..CharacterSkills::default()
        };
        let markup = social_skill_rows(&skills, 1.0, None).into_string();
        assert!(markup.contains("data-social-primary"));
        assert!(markup.contains("Average of all six Social skills"));
        assert_eq!(markup.matches("data-social-detail").count(), 6);
        for icon in [
            "conversation.svg",
            "awareness.svg",
            "inner-self.svg",
            "juggler.svg",
            "crown.svg",
            "rose.svg",
        ] {
            assert!(markup.contains(icon), "missing social icon {icon}");
        }
    }

    #[test]
    fn herbalist_stock_template_includes_every_prepared_course_and_ingredients() {
        let ingredient = crate::spacetimedb::ItemDefinition {
            kind: ItemKind::Ingredient,
            ..Default::default()
        };
        let medication = crate::spacetimedb::ItemDefinition {
            kind: ItemKind::Medication,
            ..Default::default()
        };
        assert!(MerchantShop::Herbalist.stocks(&ingredient));
        assert!(MerchantShop::Herbalist.stocks(&medication));
        let apple = crate::spacetimedb::ItemDefinition {
            id: "apple".into(),
            kind: ItemKind::Food,
            ..Default::default()
        };
        let pan = crate::spacetimedb::ItemDefinition {
            id: "cooking_pan".into(),
            ..Default::default()
        };
        assert!(MerchantShop::Inn.stocks(&apple));
        assert!(MerchantShop::Inn.stocks(&pan));
        assert!(!MerchantShop::Inn.stocks(&medication));
        assert_eq!(adventuresim_core::disease::MEDICATION_RECIPES.len(), 8);
        let definition = crate::spacetimedb::ItemDefinition {
            id: "black_death_tonic".into(),
            kind: ItemKind::Medication,
            ..Default::default()
        };
        let rendered =
            item_name_with_display("black_death_tonic", "Black Death tonic", Some(&definition))
                .into_string();
        assert!(rendered.contains("data-item-name=\"black_death_tonic\""));
        assert!(rendered.contains("data-item-kind=\"medication\""));
        assert!(rendered.contains(">Black Death tonic</span>"));
    }

    #[test]
    fn location_kind_rejects_unknown_path_segments() {
        assert_eq!("quest".parse(), Ok(LocationKind::Quest));
        assert!("merchant".parse::<LocationKind>().is_err());
    }

    #[test]
    fn encumbrance_meter_formats_exact_text_and_accessible_linear_position() {
        let markup = encumbrance_meter(EncumbranceSummary::new(85.36, 150.0)).into_string();
        assert!(markup.contains(">85.4 / 150.0 kg<"));
        assert!(markup.contains(">-56.9%<"));
        assert!(!markup.contains(">Weight"));
        assert!(!markup.contains(">Penalty"));
        assert!(markup.contains("Weight 85.4 / 150.0 kilograms; Penalty -56.9%"));
        assert!(markup.contains("class=\"encumbrance-values\" aria-hidden=\"true\""));
        assert!(markup.contains(
            "<span class=\"encumbrance-weight\">85.4 / 150.0 kg</span><span class=\"encumbrance-penalty\">-56.9%</span>"
        ));
        assert!(
            markup.contains(
                "</div><div class=\"encumbrance-visual\"><div class=\"encumbrance-meter\""
            )
        );
        assert!(markup.contains("role=\"meter\""));
        assert!(markup.contains("aria-valuenow=\"56.9\""));
        assert!(markup.contains("--encumbrance-position: 56.9067%"));
    }

    #[test]
    fn overloaded_meter_keeps_burden_but_clamps_penalty_and_marker() {
        let markup = encumbrance_meter(EncumbranceSummary::new(185.4, 150.0)).into_string();
        assert!(markup.contains(">185.4 / 150.0 kg<"));
        assert!(markup.contains(">-100.0%<"));
        assert!(markup.contains("--encumbrance-position: 100.0000%"));
    }

    #[test]
    fn encumbrance_css_uses_a_linear_midpoint_gradient_and_contrast_marker() {
        let css = include_str!("../../static/css/strategic.css");
        assert!(css.contains("linear-gradient(90deg, #238b45 0%, #f4d03f 50%, #c62828 100%)"));
        assert!(css.contains(".encumbrance-marker"));
        assert!(css.contains("background: #fff"));
    }

    #[test]
    fn encumbrance_rail_scrolls_items_but_keeps_footer_and_meter_outside() {
        let markup = encumbrance_inventory_rail(
            maud::html! { table class="test-items" {} },
            maud::html! { button class="test-footer" {} },
            EncumbranceSummary::new(10.0, 100.0),
        )
        .into_string();
        assert!(markup.contains(
            "<div class=\"encumbrance-inventory-scroll\"><table class=\"test-items\"></table></div><button class=\"test-footer\"></button><div class=\"encumbrance\">"
        ));

        let css = include_str!("../../static/css/strategic.css");
        assert!(css.contains(".sidebar-section:has(> .encumbrance-inventory-rail)"));
        assert!(css.contains(".encumbrance-inventory-scroll"));
        assert!(css.contains("overflow-y: auto"));
        assert!(css.contains("padding-left: 3.25rem"));
        assert!(css.contains("padding-right: 1.75rem"));
        assert!(css.contains("container-type: inline-size"));
        assert!(css.contains("flex: 0 0 50%"));
        assert!(css.contains("width: 50%"));
        assert!(css.contains("font-size: clamp(0.55rem, 4cqi, 0.78rem)"));
        assert!(css.contains(".encumbrance-meter"));
        assert!(css.contains("width: 100%"));
        assert!(css.contains("@container (max-width: 12rem)"));
        assert!(css.contains("padding-inline: 0.2rem"));
        assert!(css.contains("font-size: 0.5rem"));
        assert!(css.contains("@container (max-width: 10rem)"));
        assert!(css.contains("padding-inline: 0.1rem"));
        assert!(css.contains("padding-right: 0.05rem"));
        assert!(css.contains("padding-left: 0.05rem"));
        assert!(css.contains("font-size: 0.43rem"));
    }

    #[test]
    fn merchant_tabs_render_personal_and_party_encumbrance_as_applicable() {
        let character = Character {
            id: 1,
            name: "Trader".into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: Some("viabundus-1".into()),
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive: true,
            temporary: false,
        };
        let render = |shop| {
            live_merchant_shop_page(
                &settlement(),
                &character,
                &[],
                &[],
                &[],
                &[],
                None,
                &[],
                &[],
                &[],
                shop,
                1.0,
                0,
                0,
                &[],
                None,
                &[],
                0,
                EncumbranceSummary::new(10.0, 100.0),
                EncumbranceSummary::new(30.0, 200.0),
                None,
                SoapRestPreview::default(),
            )
            .into_string()
        };
        let merchant = render(MerchantShop::Weapons);
        assert!(merchant.contains("data-inventory-pane=\"player\""));
        assert!(merchant.contains("data-inventory-pane=\"party\""));
        assert!(merchant.contains(">10.0 / 100.0 kg<"));
        assert!(merchant.contains(">30.0 / 200.0 kg<"));

        let herbalist = render(MerchantShop::Herbalist);
        assert!(herbalist.contains(">10.0 / 100.0 kg<"));
        assert!(!herbalist.contains("data-inventory-pane=\"party\""));
        assert!(!herbalist.contains(">30.0 / 200.0 kg<"));

        let inn = render(MerchantShop::Inn);
        assert!(inn.contains("Cooking supplies"));
        assert!(inn.contains("aria-label=\"Inn rest service\""));
    }

    #[test]
    fn inn_catalog_renders_an_authoritatively_quoted_travel_ration_purchase() {
        let mut town = settlement();
        town.economy.services = vec![adventuresim_world_schema::SettlementService::Inn];
        let character = Character {
            id: 1,
            name: "Traveller".into(),
            xp: 0,
            level: 1,
            gold: 20,
            current_settlement_id: Some(town.id.clone()),
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive: true,
            temporary: false,
        };
        let ration = crate::spacetimedb::ItemDefinition {
            id: "travel_ration".into(),
            weight: 0.65,
            base_value: Some(3),
            nutrition_kcal: 2_500.0,
            kind: ItemKind::Food,
            ..Default::default()
        };

        let markup = live_merchant_shop_page(
            &town,
            &character,
            &[],
            std::slice::from_ref(&ration),
            &[],
            &[],
            None,
            &[],
            &[],
            &[],
            MerchantShop::Inn,
            1.0,
            0,
            0,
            &[],
            None,
            &[],
            0,
            EncumbranceSummary::default(),
            EncumbranceSummary::default(),
            None,
            SoapRestPreview::default(),
        )
        .into_string();

        assert!(markup.contains("data-merchant-item=\"travel_ration\""));
        assert!(markup.contains("data-merchant-buy=\"travel_ration\""));
        assert!(markup.contains("data-merchant-buy-price=\"5\""));
        assert!(markup.contains(">0.65<"));
    }

    #[test]
    fn rest_recommendation_includes_blood_recovery() {
        let condition = CharacterCondition {
            character_id: 1,
            body_weight_kg: 70.0,
            current_blood_ml: 4_900.0,
            maximum_blood_ml: 5_000.0,
            religion_id: None,
        };

        assert_eq!(
            rest_default_minutes(None, None, Some(&condition), 0, 0),
            Some(2_880)
        );
    }

    #[test]
    fn settlement_wake_control_is_accessible_and_defaults_to_eight() {
        let markup = settlement_rest_duration_control(1_440, "hours").into_string();
        assert!(markup.contains("data-wake-time"));
        assert!(markup.contains("type=\"range\""));
        assert!(markup.contains("step=\"60\""));
        assert!(markup.contains("value=\"480\""));
        assert!(markup.contains("type=\"text\""));
        assert!(markup.contains("value=\"24:00\""));
        assert!(markup.contains("pattern=\"[0-9]+:[0-5][0-9]\""));
        assert!(markup.contains("aria-label=\"Wake time\""));
        assert!(markup.contains("aria-valuetext=\"08:00\""));
        assert!(markup.contains("name=\"requested_minutes\""));
    }

    #[test]
    fn rest_supplies_are_icons_with_hover_only_details() {
        let markup = soap_wash_preview(SoapRestPreview {
            total_units: 1,
            personal_units: 1,
            available_units: 1,
            alcohol_available: true,
            alcohol_will_be_consumed: false,
            ..SoapRestPreview::default()
        })
        .into_string();
        assert!(markup.contains("aria-label=\"Soap\""));
        assert!(markup.contains("aria-label=\"Alcohol\""));
        assert!(markup.contains("water-drop.svg"));
        assert!(markup.contains("beer-stein.svg"));
        assert!(markup.contains("rest-consumable-indicator available"));
        assert!(markup.contains("rest-consumable-indicator unavailable"));
        assert!(!markup.contains("rest-soap-preview"));
        assert!(markup.contains("Temperate characters do not drink"));
    }

    #[test]
    fn days_recommendation_keeps_slider_disabled_and_minimum_one() {
        let markup = settlement_rest_duration_control(3 * 1_440, "days").into_string();
        assert!(markup.contains("value=\"days\" checked"));
        assert!(markup.contains("aria-disabled=\"true\""));
        assert!(
            markup.contains(
                "value=\"480\" aria-label=\"Wake time\" aria-valuetext=\"08:00\" disabled"
            )
        );
        assert!(markup.contains("type=\"number\" name=\"duration\" value=\"3\""));
        assert!(markup.contains("min=\"1\" max=\"365\" step=\"1\""));
        assert!(markup.contains("name=\"requested_minutes\" disabled"));
    }

    #[test]
    fn rest_summary_duration_keeps_subday_hours_and_minutes() {
        assert_eq!(format_rest_duration(1_441), "1 day 1 minute");
        assert_eq!(format_rest_duration(1_920), "1 day 8 hours");
        assert_eq!(format_rest_duration(2_879), "1 day 23 hours 59 minutes");
    }

    #[test]
    fn need_meter_places_reserve_right_and_incapacitation_left() {
        let reserve =
            need_balance_meter("Food", "meal", "Hunger", "Full", "hunger", 0.5, 0.0).into_string();
        assert!(reserve.contains("--need-reserve: 50.0%; --need-deficit: 0.0%"));
        assert!(reserve.contains("aria-valuenow=\"50\""));
        assert!(reserve.contains(">Full</span>"));

        let hydration = need_balance_meter(
            "Water",
            "water-drop",
            "Thirst",
            "Hydrated",
            "thirst",
            1.0,
            0.0,
        )
        .into_string();
        assert!(hydration.contains(">Hydrated</span>"));

        let deficit =
            need_balance_meter("Food", "meal", "Hunger", "Full", "hunger", 0.0, 1.0 / 9.0)
                .into_string();
        assert!(deficit.contains("--need-reserve: 0.0%; --need-deficit: 11.1%"));
        assert!(deficit.contains("aria-valuenow=\"-11\""));
    }

    #[test]
    fn canonical_imported_religions_have_ui_labels() {
        for (id, label) in [
            ("roman_catholic", "Roman Catholic"),
            ("lutheran", "Lutheran"),
            ("reformed", "Reformed"),
            ("anglican", "Anglican"),
            ("eastern_orthodox", "Eastern Orthodox"),
            ("islamic", "Islamic"),
            ("judaism", "Jewish"),
        ] {
            assert_eq!(religion_name(Some(id)), label);
        }
    }
    fn settlement() -> Settlement {
        Settlement {
            id: "viabundus-1".into(),
            name: "Lübeck".into(),
            coord_x: 10.0,
            coord_y: 53.0,
            population_level: 4,
            population_estimate: 12_000,
            category: crate::spacetimedb::SettlementCategory::City,
            languages: adventuresim_world_schema::SettlementLanguageProfile {
                east_central_bp: 2_000,
                west_central_bp: 2_000,
                low_bp: 6_000,
                yiddish_incidence_bp: 75,
            },
            industries: adventuresim_world_schema::InferredIndustryProfile::new(vec![
                adventuresim_world_schema::IndustryEvidence::Fallback(
                    adventuresim_world_schema::FallbackIndustry::CroplandGrain,
                ),
            ])
            .unwrap(),
            economy: adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder(),
            religious_status: adventuresim_world_schema::SettlementReligiousStatus::Established {
                religion: adventuresim_world_schema::OfficialReligion::RomanCatholic,
            },
            scene_key: "hills".into(),
            religion_id: "western_church".into(),
            currency_id: "lubeck_mark".into(),
            source_node_id: Some(1),
        }
    }

    #[test]
    fn disabled_repair_explanation_is_hoverable_and_focusable() {
        let condition = crate::spacetimedb::ItemCondition {
            inventory_item_id: 4,
            tier_1: 0.0,
            tier_2: 0.0,
            tier_3: 0.0,
            tier_4: 0.2,
            tier_5: 0.0,
        };
        let rendered =
            repair_submit_control(&settlement(), "weapons", 4, Some(&condition), 3).into_string();
        assert!(rendered.contains("disabled-repair-explanation"));
        assert!(rendered.contains("tabindex=\"0\""));
        assert!(rendered.contains("All damage requires Smithing"));
        assert!(rendered.contains("disabled"));
    }

    #[test]
    fn tailor_repair_control_targets_the_clothing_service() {
        let condition = crate::spacetimedb::ItemCondition {
            inventory_item_id: 4,
            tier_1: 0.25,
            tier_2: 0.0,
            tier_3: 0.0,
            tier_4: 0.0,
            tier_5: 0.0,
        };
        let rendered =
            repair_submit_control(&settlement(), "clothing", 4, Some(&condition), 2).into_string();
        assert!(rendered.contains("/clothing/repair"));
        assert!(rendered.contains("row-repair-form"));
        assert!(!rendered.contains("disabled"));
    }

    #[test]
    fn collapsed_currency_label_hides_the_historical_denomination() {
        let definition = crate::spacetimedb::ItemDefinition {
            id: "lubeck_mark".into(),
            kind: crate::spacetimedb::ItemKind::Currency,
            base_value: Some(1),
            weight: 0.01,
            ..Default::default()
        };
        let rendered = item_name_with_quality(&definition.id, Some(&definition)).into_string();
        assert!(rendered.contains(">Coin<"));
        assert!(rendered.contains("data-currency-name=\"Lübeck mark\""));
        assert!(!rendered.contains(">Lübeck mark<"));
    }

    #[test]
    fn alcohol_labels_expose_a_shared_inventory_group() {
        let definition = crate::spacetimedb::ItemDefinition {
            id: "small_beer".into(),
            kind: crate::spacetimedb::ItemKind::Simple,
            alcohol_serving_ml: 500,
            ..Default::default()
        };
        let rendered = item_name_with_quality(&definition.id, Some(&definition)).into_string();
        assert!(rendered.contains("data-item-name=\"small_beer\""));
        assert!(rendered.contains("data-item-group=\"alcohol\""));
        assert!(rendered.contains("data-group-name=\"Alcohol\""));
    }

    #[test]
    fn smith_player_actions_keep_sell_and_repair_in_one_hover_area() {
        let repair = repair_submit_control(&settlement(), "weapons", 4, None, 3);
        let rendered =
            merchant_sell_repair_controls(4, "torch", 2, 3, 1, true, Some(repair)).into_string();

        assert!(rendered.starts_with("<div class=\"inventory-row-actions smith-player-actions\">"));
        assert_eq!(rendered.matches("data-merchant-sell=\"").count(), 1);
        assert!(rendered.contains("data-dynamic-transfer"));
        assert!(rendered.contains("data-default-transfer-mode=\"one\""));
        assert!(rendered.contains("data-label-target=\"Sell surplus Torch\""));
        assert!(rendered.contains("data-label-all=\"Sell all Torch\""));
        assert!(rendered.contains("row-repair-form"));
        assert_eq!(rendered.matches("smith-player-actions").count(), 1);
    }

    #[test]
    fn equipped_smith_items_retain_the_repair_action_without_sell_controls() {
        let repair = repair_submit_control(&settlement(), "weapons", 4, None, 3);
        let rendered =
            merchant_sell_repair_controls(4, "sword", 10, 1, 0, false, Some(repair)).into_string();

        assert!(rendered.contains("smith-player-actions"));
        assert!(rendered.contains("row-repair-form"));
        assert!(!rendered.contains("data-merchant-sell"));
        assert!(rendered.contains("Equipped items cannot be sold"));
        assert!(rendered.contains("trade-transfer trade-transfer-left"));
        assert!(rendered.contains("disabled"));
    }

    #[test]
    fn non_smith_sell_controls_do_not_reserve_a_repair_slot() {
        let rendered = merchant_sell_repair_controls(4, "shirt", 3, 1, 0, true, None).into_string();

        assert!(rendered.starts_with("<div class=\"inventory-row-actions\">"));
        assert!(!rendered.contains("smith-player-actions"));
        assert!(rendered.contains("data-merchant-sell"));
    }

    #[test]
    fn unavailable_transfer_button_keeps_a_disabled_action_slot() {
        let rendered =
            disabled_transfer_button("left", "Equipped items cannot be transferred").into_string();

        assert!(rendered.contains("trade-transfer trade-transfer-left"));
        assert!(rendered.contains("Equipped items cannot be transferred"));
        assert!(rendered.contains("disabled"));
        assert!(rendered.contains("inventory-transfer-glyph"));
    }

    #[test]
    fn durability_bar_uses_qualitative_copy_and_marks_smith_repairable_damage() {
        let condition = crate::spacetimedb::ItemCondition {
            inventory_item_id: 4,
            tier_1: 0.1,
            tier_2: 0.0,
            tier_3: 0.2,
            tier_4: 0.1,
            tier_5: 0.0,
        };
        let rendered = condition_bar(Some(&condition), Some(3)).into_string();
        assert!(rendered.contains("condition-repairable"));
        for tier in 1..=5 {
            assert!(rendered.contains(&format!("condition-tier-{tier}")));
        }
        assert!(rendered.contains("flashing portion can be repaired"));
        assert!(!rendered.contains("condition-number"));
        assert!(!rendered.contains("% condition"));
    }

    #[test]
    fn durable_item_names_expose_quality_color_and_description() {
        let definition = crate::spacetimedb::ItemDefinition {
            id: "commissioned_sword".into(),
            weight: 1.0,
            slot: ItemSlot::AnyHolding,
            kind: crate::spacetimedb::ItemKind::Weapon,
            base_value: None,
            nutrition_kcal: 0.0,
            water_capacity_ml: 0,
            quality: 4,
            durability_yield: 0.0,
            durability_fracture: 0.0,
            durability_wear: 0.0,
            durability_failure_share: 0.0,
            edge_sensitivity: 0.0,
            handling_sensitivity: 0.0,
            ..Default::default()
        };

        let rendered = item_name_with_quality(&definition.id, Some(&definition)).into_string();
        assert!(rendered.contains("item-quality-4"));
        assert!(rendered.contains("knightly commission"));
    }

    #[test]
    fn completed_repair_bar_projects_the_condition_before_retrieval() {
        let condition = crate::spacetimedb::ItemCondition {
            inventory_item_id: 4,
            tier_1: 0.1,
            tier_2: 0.2,
            tier_3: 0.0,
            tier_4: 0.0,
            tier_5: 0.0,
        };

        let rendered = completed_repair_condition_bar(Some(&condition), 3).into_string();
        assert!(rendered.contains("Full durability"));
        assert!(rendered.contains("width:100%"));
    }

    #[test]
    fn smith_player_inventory_uses_the_compact_seven_column_table() {
        let rendered = trade_inventory_table(
            "test",
            InventoryColumnSet::Weapons,
            true,
            true,
            true,
            html! {},
        )
        .into_string();
        assert!(rendered.contains("smith-player-inventory-table"));
        assert!(rendered.contains("inventory-column-type"));
        assert!(rendered.contains("aria-label=\"Item type\""));
        assert!(rendered.contains("inventory-column-durability"));
        assert!(rendered.contains("hammer-nails.svg"));
        assert!(!rendered.contains("Repair all eligible items"));
        assert!(!rendered.contains("durability-header-label"));
    }

    #[test]
    fn repair_all_precedes_the_sell_bulk_control() {
        let rendered = inventory_footer_controls_with_leading(
            Some(repair_all_control(&settlement(), "weapons")),
            "sell",
            "Sell surplus",
            "Sell everything",
        )
        .into_string();
        let repair = rendered.find("inventory-footer-repair").unwrap();
        let sell = rendered.find("data-inventory-bulk=\"sell\"").unwrap();
        assert!(rendered.contains("inventory-footer-actions-grouped"));
        assert!(repair < sell);
    }

    #[test]
    fn skill_meter_and_schedule_use_segmented_rank_and_text_time_controls() {
        let meter =
            skill_rank_bar(3.5, 2.75, "Skill test", SkillRankBarOptions::default()).into_string();
        for tier in 1..=5 {
            assert!(meter.contains(&format!("skill-rank-segment-{tier}")));
        }
        assert!(meter.contains("role=\"meter\""));
        assert!(meter.contains("aria-valuenow=\"2.8\""));
        assert!(meter.contains("class=\"skill-rank-value\""));
        assert!(!meter.contains("tabindex"));
        let allocation = schedule_allocation_cell("smithing_minutes", 75, true).into_string();
        assert!(allocation.contains("data-schedule-input"));
        assert!(allocation.contains("data-schedule-display"));
        assert!(allocation.contains("Click to enter a time such as 8, 8:30, or 830"));
        assert!(!allocation.contains("data-schedule-step"));
        assert!(!allocation.contains("type=\"range\""));
        assert!(!allocation.contains("schedule-handle"));
    }

    #[test]
    fn surgery_supplies_are_icon_counts_with_hover_labels() {
        let supply = surgery_supply("Bandages", "bandage-roll", 8).into_string();
        assert!(supply.contains("class=\"surgery-supply\""));
        assert!(supply.contains("data-strategic-tooltip=\"Bandages: 8 available\""));
        assert!(supply.contains("bandage-roll.svg"));
        assert!(supply.contains(">x8</span>"));
        assert!(!supply.contains(">Bandages</span>"));
    }

    #[test]
    fn surgery_item_icons_explain_consumed_reusable_and_equipped_supplies() {
        let bandage =
            surgery_item_requirement(SurgeryItemRequirement::BandageConsumed).into_string();
        assert!(bandage.contains("data-strategic-tooltip=\"Expend one bandage\""));
        assert!(bandage.contains(">x1</span>"));

        let kit =
            surgery_item_requirement(SurgeryItemRequirement::SurgeryKitReusable).into_string();
        assert!(kit.contains("data-strategic-tooltip=\"Requires surgery kit\""));
        assert!(kit.contains("aria-label=\"Requires surgery kit; reusable and not consumed\""));
        assert!(kit.contains("medical-pack.svg"));
        assert!(!kit.contains("surgery-item-overlay"));

        let splint = surgery_item_requirement(SurgeryItemRequirement::SplintEquipped).into_string();
        assert!(splint.contains("data-strategic-tooltip=\"Equips 1 splint\""));
        assert!(splint.contains("check-mark.svg"));
    }

    #[test]
    fn surgery_difficulty_uses_shared_skill_meter_for_met_and_unmet_ranks() {
        let meter = surgery_difficulty_meter("Remove ball", 4.0, 2.0).into_string();
        assert!(meter.contains("stat-icon-surgeon"));
        assert!(meter.contains("role=\"meter\""));
        for tier in 1..=5 {
            assert!(meter.contains(&format!("skill-rank-segment-{tier}")));
        }
        assert_eq!(
            meter
                .matches("class=\"rank-current\" style=\"width:100.0%\"")
                .count(),
            2
        );
        assert_eq!(meter.matches("left:0.0%;width:100.0%").count(), 2);
        assert!(!meter.contains("skill-rank-value"));
        assert!(!meter.contains(">4.0<"));
        assert!(meter.contains(
            "aria-label=\"Remove ball: requires 4.0 procedure skill; current effective skill 2.0\""
        ));
        let over_cap = surgery_difficulty_meter("Remove ball", 7.2, 5.0).into_string();
        assert!(over_cap.contains("surgery-difficulty-over-cap-marker"));
        assert!(over_cap.contains("requires 7.2 procedure skill; current effective skill 5.0"));
        assert!(!adventuresim_core::surgery::extraction_requires_surgery_kit(1.0));
        assert!(adventuresim_core::surgery::extraction_requires_surgery_kit(
            1.01
        ));
    }

    #[test]
    fn surgery_preview_uses_the_same_asymmetric_procedure_composition_as_reducers() {
        let checks = [5.0, 5.0, 0.0];
        let extraction = surgery_procedure_skill("extract", checks, false);
        let stitching = surgery_procedure_skill("stitch", checks, false);
        assert_eq!(
            extraction,
            adventuresim_core::surgery::procedure_skill("extract", 5.0, 5.0, 0.0, false)
        );
        assert_eq!(
            stitching,
            adventuresim_core::surgery::procedure_skill("stitch", 5.0, 5.0, 0.0, false)
        );
        assert!(extraction >= 4.0);
        assert!(stitching < 4.0);
        assert_eq!(surgery_procedure_skill("extract", checks, true), 2.5);
    }

    #[test]
    fn unavailable_surgery_rows_are_greyed_and_buttons_keep_procedure_names() {
        let row = surgery_procedure_row(
            "/test",
            "Stitch",
            "scalpel",
            "stitch",
            &[SurgeryItemRequirement::SurgeryKitReusable],
            10,
            2.0,
            0.0,
            Some("No injury is present"),
            Some("No injury is present"),
            None,
            true,
            true,
            None,
        )
        .into_string();
        assert!(row.contains("surgery-procedure-unavailable"));
        assert!(row.contains("data-strategic-tooltip=\"No injury is present\""));
        assert!(row.contains("aria-label=\"Stitch: No injury is present\" tabindex=\"0\""));
        assert!(row.contains("disabled title=\"No injury is present\""));
        assert!(row.contains(">Stitch</button>"));
        assert!(!row.contains(">No injury is present</button>"));
    }

    #[test]
    fn bloody_procedure_names_concrete_automatic_alcohol_without_risk_numbers() {
        let row = surgery_procedure_row(
            "/test",
            "Bandage",
            "bandage-roll",
            "bandage",
            &[],
            10,
            0.0,
            1.0,
            None,
            None,
            None,
            false,
            true,
            Some("aqua_vitae"),
        )
        .into_string();
        assert!(row.contains("Consumes 1 Aqua vitae"));
        assert!(row.contains("beer-stein.svg"));
        assert!(!row.contains("infection probability"));
        assert!(!row.contains("use_alcohol"));
    }

    #[test]
    fn schedule_table_uses_compact_accessible_icon_headers() {
        let skills = CharacterSkills {
            character_id: 1,
            polearm_hours: 0.0,
            axe_hours: 0.0,
            bludgeon_hours: 0.0,
            sword_hours: 0.0,
            knife_hours: 0.0,
            dodge_hours: 0.0,
            block_hours: 0.0,
            bow_hours: 0.0,
            crossbow_hours: 0.0,
            firearm_hours: 0.0,
            throw_hours: 0.0,
            will_hours: 0.0,
            insight_hours: 0.0,
            self_awareness_hours: 0.0,
            humor_hours: 0.0,
            command_hours: 0.0,
            deception_hours: 0.0,
            seduction_hours: 0.0,
            medicine_hours: 0.0,
            cooking_hours: 0.0,
            religion_hours: adventuresim_world_schema::ReligionHours {
                roman_catholic: 1_000.0,
                ..Default::default()
            },
            oral_languages: Default::default(),
            written_languages: Default::default(),
            stealth_hours: 0.0,
            balance_hours: 0.0,
            terrain_plains_hours: 0.0,
            terrain_forest_hours: 0.0,
            terrain_hills_hours: 0.0,
            terrain_urban_hours: 0.0,
            anatomy_hours: 0.0,
            tailoring_hours: 0.0,
            smithing_hours: 0.0,
        };
        let schedule = CharacterTrainingSchedule {
            character_id: 1,
            downtime: crate::spacetimedb::ScheduleAllocation {
                combat_training_minutes: 90,
                prayer_minutes: 120,
                ..Default::default()
            },
            travel: crate::spacetimedb::ScheduleAllocation::default(),
        };
        let rendered = skills_table(
            "Your skills",
            &skills,
            1.0,
            1.0,
            1.0,
            Some(&schedule),
            None,
            false,
            0.0,
            Some(OfficialReligion::Judaism),
            CombatTrainingProfile::default(),
            false,
            CharacterSkillActions::default(),
        )
        .into_string();

        assert!(rendered.contains(
            "scope=\"colgroup\" colspan=\"8\" class=\"schedule-table-title\">Your skills"
        ));
        assert_eq!(rendered.matches("<colgroup>").count(), 2);
        assert!(rendered.contains(
            "<col class=\"religion-auto-column\"><col class=\"party-skill-time-column\"><col class=\"religion-expand-column\">"
        ));
        assert_eq!(
            rendered.matches("aria-label=\"Daily allocation\"").count(),
            1
        );
        assert!(!rendered.contains("aria-label=\"Automatic training\""));
        for label in ["Currency", "Virtue", "Morale", "Fatigue"] {
            assert!(rendered.contains(&format!("aria-label=\"{label}\"")));
        }
        assert!(rendered.contains("data-religion-expand"));
        assert!(!rendered.contains("class=\"skill-rank-value\""));
        assert_eq!(
            rendered.matches("class=\"party-skill-row").count(),
            rendered.matches("class=\"religion-expand-cell\"").count(),
        );
        assert!(rendered.contains("aria-expanded=\"false\""));
        assert!(rendered.contains("data-religion-primary=\"judaism\""));
        assert!(rendered.contains("Expand Judaism Religion skill"));
        assert!(rendered.contains("title=\"Judaism\""));
        assert!(rendered.contains("/static/icons/religion/fontawesome-star-of-david.svg"));
        assert!(!rendered.contains("data-combat-auto-toggle"));
        for group in ["melee", "ranged", "defense"] {
            assert!(rendered.contains(&format!("data-combat-expand=\"{group}\"")));
            assert!(rendered.contains(&format!("data-combat-detail=\"{group}\"")));
        }
        assert!(!rendered.contains("aria-label=\"Religion details\""));
        assert!(rendered.contains("aria-label=\"Skill details\""));
        assert!(rendered.contains("Sparring and target practice"));
        assert!(rendered.contains("Carousing"));
        assert_eq!(rendered.matches("data-religion-detail").count(), 1);
        assert!(!rendered.contains("title=\"Lutheranism\""));
        assert!(!rendered.contains("religion_judaism_minutes"));
        assert!(!rendered.contains("effective /"));
        assert!(rendered.contains("100.0 effective hours; 0.0 directly studied hours"));
        let primary_icon = rendered
            .find("/static/icons/religion/fontawesome-star-of-david.svg")
            .unwrap();
        let expand = rendered.find("data-religion-expand").unwrap();
        assert!(primary_icon < expand);
        assert!(rendered.contains("class=\"religion-expand-cell\"><button"));
        assert!(rendered.contains("aria-label=\"Will\""));
        assert!(!rendered.contains("data-religion-auto-budget disabled"));
        assert!(!rendered.contains("data-religion-manual-budget disabled"));
        assert!(rendered.contains("/static/icons/game/coins.svg"));
        assert!(!rendered.contains(">Gold</th>"));
        assert!(!rendered.contains(">Virt.</th>"));

        let rail = party_skills_rail(
            "Your skills",
            Some(&skills),
            None,
            Some(&schedule),
            Some("/schedule"),
            None,
            false,
            0.0,
            Some("judaism"),
            CombatTrainingProfile::default(),
            CharacterSkillActions::default(),
        )
        .into_string();
        assert!(!rail.contains("class=\"sidebar-header\">Your skills"));
        assert!(rail.contains("<h3 class=\"sr-only\">Your skills</h3>"));
        assert!(rail.contains("data-schedule-save-status"));
        assert!(rail.contains("role=\"status\" aria-live=\"polite\" hidden"));
        assert!(rail.contains("data-schedule-retry>Retry</button>"));
        assert!(!rail.contains("data-activity-modal"));
        assert!(!rail.contains("data-activity-open"));
        let settlement_rail = party_skills_rail(
            "Your skills",
            Some(&skills),
            None,
            Some(&schedule),
            Some("/locations/settlement/lubeck/party/1/schedule"),
            None,
            false,
            0.0,
            Some("judaism"),
            CombatTrainingProfile::default(),
            CharacterSkillActions::default(),
        )
        .into_string();
        assert!(settlement_rail.contains("data-activity-modal"));
        assert!(settlement_rail.contains("data-activity-open"));
        assert!(!rail.contains(">⚙</span>"));
        assert!(!rail.contains("aria-label=\"Automatic training\""));
    }

    #[test]
    fn defense_will_uses_head_health_for_its_injury_adjusted_rank() {
        let skills = CharacterSkills {
            will_hours: 5_000.0,
            ..Default::default()
        };
        let rendered = combat_skill_rows(
            &skills,
            0.2,
            0.8,
            1.0,
            None,
            CombatTrainingProfile::default(),
        )
        .into_string();
        let will = rendered.find("aria-label=\"Will\"").unwrap();
        let start = rendered[..will].rfind("<tr").unwrap();
        let end = will + rendered[will..].find("</tr>").unwrap() + "</tr>".len();
        let will_row = &rendered[start..end];
        let rank = Skill::Will.training_rank(5_000.0);
        let expected = skill_rank_bar(
            rank,
            rank * 0.2,
            "5000 hours invested",
            skill_rail_bar_options(),
        )
        .into_string();
        assert!(will_row.contains(&expected));
    }

    #[test]
    fn language_families_are_expandable_accessible_and_color_coded() {
        let skills = CharacterSkills {
            oral_languages: adventuresim_world_schema::OralLanguageHours {
                east_central: 5_000.0,
                ..Default::default()
            },
            written_languages: adventuresim_world_schema::WrittenLanguageHours {
                german: 1_000.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let rendered = language_skill_rows(&skills, false).into_string();
        assert!(rendered.contains("Expand Oral languages"));
        assert!(rendered.contains("Expand Written languages"));
        assert!(rendered.contains("language-oral language-blackletter"));
        assert!(rendered.contains("language-written language-blackletter"));
        assert!(rendered.contains(
            "5000.0 effective hours; 5000.0 directly studied hours across Oral languages"
        ));
        assert!(rendered.contains(
            "1000.0 effective hours; 1000.0 directly studied hours across Written languages"
        ));
        assert!(rendered.contains("title=\"East-central — Ostmitteldeutsch\""));
        assert!(rendered.contains("5000.0 effective hours; 5000.0 directly studied hours"));
        assert!(rendered.contains("title=\"Latin — Latine\""));
        assert!(!rendered.contains("title=\"Romani — Romani\""));
        assert_eq!(rendered.matches("data-language-detail=\"oral\"").count(), 4);
        assert_eq!(
            rendered.matches("data-language-detail=\"written\"").count(),
            3
        );
    }

    #[test]
    fn language_families_are_hidden_without_effective_hours() {
        let rendered = language_skill_rows(&CharacterSkills::default(), false).into_string();
        assert!(!rendered.contains("Expand Oral languages"));
        assert!(!rendered.contains("Expand Written languages"));
        assert!(!rendered.contains("data-language-detail"));
    }

    #[test]
    fn activity_rows_show_signed_daily_effects_instead_of_allocation_bars() {
        let rendered = schedule_special_row(
            "Thievery",
            "market",
            "thievery_minutes",
            120,
            true,
            true,
            ActivityEffectRates::linear(2.0, -1.0, 0.0, 0.0),
            None,
            None,
            "Test activity",
        )
        .into_string();
        for effect in ["gold", "virtue", "morale", "fatigue"] {
            assert!(rendered.contains(&format!("data-activity-effect=\"{effect}\"")));
        }
        assert!(rendered.contains("schedule-effect-positive"));
        assert!(rendered.contains(">+4</td>"));
        assert!(rendered.contains("schedule-effect-negative"));
        assert!(rendered.contains(">-2.0</td>"));
        assert!(rendered.contains("<span class=\"sr-only\">Thievery</span>"));
        assert!(!rendered.contains("<strong>Thievery</strong>"));
        assert!(!rendered.contains("schedule-allocation-fill"));
        assert!(!rendered.contains("schedule-special-track"));
    }

    #[test]
    fn activity_training_column_totals_and_explains_effective_skill_hours() {
        let combat =
            activity_training_cell("Combat Training", "combat_training_minutes", 120, None)
                .into_string();
        assert!(combat.contains(">+2.00h<"));
        assert!(combat.contains("Relevant combat skills: +2.00h"));

        let carousing =
            activity_training_cell("Carousing", "carousing_minutes", 120, None).into_string();
        assert!(carousing.contains(">+0.50h<"));
        assert!(carousing.contains("Humor: +0.50h"));

        let profession = ProfessionActivityPreview {
            training_rates: vec![
                ("Medicine".into(), 0.5),
                ("Anatomy".into(), 1.0 / 6.0),
                ("Knife".into(), 1.0 / 6.0),
                ("Tailoring".into(), 1.0 / 6.0),
            ],
            apprenticeship_accrued: 0,
            practice_accrued: 0,
            practice_threshold: 8 * 60 * PROFESSION_ACCRUAL_SCALE,
            practice_reward: "gold",
            tier_label: "apprentice",
        };
        let apprenticeship = activity_training_cell(
            "Apprenticeship — herbalist",
            "apprenticeship_minutes",
            120,
            Some(&profession),
        )
        .into_string();
        assert!(apprenticeship.contains(">+2.00h<"));
        assert!(apprenticeship.contains("Medicine: +1.00h"));
        assert!(apprenticeship.contains("Anatomy: +0.33h"));
        assert!(apprenticeship.contains("Knife: +0.33h"));
        assert!(apprenticeship.contains("Tailoring: +0.33h"));

        let leisure = activity_training_cell("Leisure", "leisure_minutes", 480, None).into_string();
        assert!(leisure.contains(">—<"));
        assert!(leisure.contains("No skill training"));
    }

    #[test]
    fn profession_preview_uses_accrual_tier_reward_and_training_distribution() {
        let threshold = APPRENTICESHIP_REWARD_THRESHOLD;
        let row = CharacterApprenticeship {
            id: 1,
            character_id: 7,
            service_id: "weapons".into(),
            religion_id: None,
            started_minute: 0,
            apprenticeship_minutes_accrued: threshold - 60 * PROFESSION_ACCRUAL_SCALE,
            practice_minutes_accrued: 0,
        };
        let journeyman = CharacterSkills {
            smithing_hours: 4_000.0,
            ..Default::default()
        };
        let preview = ActivityPreviewRates::default()
            .with_professions(Some(&journeyman), std::slice::from_ref(&row));
        let smith = preview.profession.get("weapons").unwrap();
        assert_eq!(smith.tier_label, "journeyman");
        assert_eq!(smith.practice_threshold, 8 * 60 * PROFESSION_ACCRUAL_SCALE);
        assert_eq!(
            smith.reward_delta("apprenticeship_minutes", 60),
            [-1.0, 0.0]
        );
        assert_eq!(smith.training_rates, vec![("Smithing".into(), 1.0)]);

        let master = CharacterSkills {
            smithing_hours: 25_000.0,
            ..Default::default()
        };
        let preview = ActivityPreviewRates::default().with_professions(Some(&master), &[row]);
        let smith = preview.profession.get("weapons").unwrap();
        assert_eq!(smith.tier_label, "master");
        assert_eq!(smith.practice_threshold, 2 * 60 * PROFESSION_ACCRUAL_SCALE);
        assert_eq!(
            smith.reward_delta("profession_practice_minutes", 240),
            [2.0, 0.0]
        );
    }

    #[test]
    fn server_rendered_effects_normalize_negative_zero() {
        let rendered = activity_effect_cell("fatigue", -0.0006).into_string();
        assert!(rendered.contains("schedule-effect-neutral"));
        assert!(rendered.contains(">0</td>"));
        assert!(!rendered.contains("-0.0"));

        let negative = activity_effect_cell("fatigue", -0.06).into_string();
        assert!(negative.contains("schedule-effect-negative"));
        assert!(negative.contains(">-0.1</td>"));
    }

    #[test]
    fn prayer_preview_uses_zero_partial_and_full_party_religion_checks() {
        let minutes = 240;
        let full = ActivityEffectRates::prayer(1.0).values(minutes)[2];
        assert_eq!(ActivityEffectRates::prayer(0.0).values(minutes)[2], 0.0);
        assert!((ActivityEffectRates::prayer(0.5).values(minutes)[2] - full * 0.5).abs() < 0.001);
        assert!(full > 0.0);
        assert!((ActivityEffectRates::meditation().values(minutes)[2] - full * 0.25).abs() < 0.001);
    }

    #[test]
    fn leisure_and_labor_previews_decompose_the_shared_fatigue_outcome() {
        let schedule = ScheduleAllocation {
            labor_minutes: 240,
            combat_training_minutes: 720,
            ..Default::default()
        };
        let leisure = leisure_preview(&schedule, 0.0);
        assert_eq!(leisure.outcome.leisure_hours, 8.0);
        assert_eq!(leisure.outcome.fatigue_delta, 0.0);
        assert_eq!(leisure.outcome.morale, 0.0);
        assert_eq!(leisure.fatigue_display, -2.0);
        assert_eq!(
            ActivityEffectRates::linear(
                0.0,
                0.0,
                0.0,
                LABOR_FATIGUE_PER_HOUR / FATIGUE_RESERVOIR_PER_PREVIEW_POINT,
            )
            .values(schedule.labor_minutes)[3],
            2.0
        );
        let rendered = schedule_special_row(
            "Leisure",
            "inn",
            "leisure_minutes",
            0,
            false,
            false,
            ActivityEffectRates::default(),
            Some(leisure),
            None,
            "Test leisure",
        )
        .into_string();
        for attribute in [
            "data-leisure-baseline-fatigue",
            "data-leisure-labor-fatigue-rate",
            "data-leisure-recovery-rate",
            "data-leisure-morale-limit",
            "data-leisure-morale-scale",
            "data-leisure-fatigue-preview-divisor",
        ] {
            assert!(rendered.contains(attribute));
        }
        assert!(rendered.contains(">-2.0</td>"));
    }

    #[test]
    fn equipment_checkbox_is_enabled_only_for_equippable_items() {
        let inventory = InventoryItem {
            id: 7,
            character_id: 9,
            item_id: "sword".into(),
            qty: 1,
        };
        let mut definition = crate::spacetimedb::ItemDefinition {
            id: "sword".into(),
            weight: 1.0,
            slot: ItemSlot::AnyHolding,
            kind: crate::spacetimedb::ItemKind::Weapon,
            base_value: None,
            nutrition_kcal: 0.0,
            water_capacity_ml: 0,
            quality: 3,
            durability_yield: 0.0,
            durability_fracture: 0.0,
            durability_wear: 0.0,
            durability_failure_share: 0.0,
            edge_sensitivity: 0.0,
            handling_sensitivity: 0.0,
            ..Default::default()
        };
        let enabled = equipment_checkbox(&inventory, Some(&definition), false).into_string();
        assert!(enabled.contains("data-equipment-toggle"));
        assert!(!enabled.contains(" disabled"));
        definition.slot = ItemSlot::None;
        let disabled = equipment_checkbox(&inventory, Some(&definition), false).into_string();
        assert!(disabled.contains(" disabled"));
    }

    #[test]
    fn merchant_stock_table_hides_quantity_and_target_columns() {
        let rendered = trade_inventory_table(
            "merchant-left",
            InventoryColumnSet::Basic,
            false,
            false,
            false,
            html! {},
        )
        .into_string();
        assert!(rendered.contains("<colgroup>"));
        assert!(!rendered.contains("inventory-column-count"));
        assert!(!rendered.contains("inventory-column-target"));
        assert!(rendered.contains("inventory-column-type"));
        assert!(rendered.contains("inventory-column-weight"));
        assert!(rendered.contains("inventory-column-gold"));
        assert!(rendered.contains("title=\"Currency\""));
        assert!(rendered.contains("aria-label=\"Currency\""));
        assert!(rendered.contains("/static/icons/game/coins.svg"));
    }

    #[test]
    fn inventory_type_header_and_row_share_the_first_column() {
        let rendered = trade_inventory_table(
            "test",
            InventoryColumnSet::Basic,
            true,
            false,
            false,
            html! {
                tr class="trade-inventory-row" {
                    td class="inventory-item-type" { (item_type_icon("arming_sword")) }
                    td class="inventory-item-name" { "Arming sword" }
                    td { "1" } td { "1" } td { "12" }
                }
            },
        )
        .into_string();
        let header = rendered.find("inventory-column-type").unwrap();
        let item_header = rendered.find("inventory-column-item").unwrap();
        let type_cell = rendered.find("inventory-item-type").unwrap();
        let item_cell = rendered.find("inventory-item-name").unwrap();
        assert!(header < item_header && type_cell < item_cell);
        assert!(rendered.contains("/static/icons/game/broadsword.svg"));
    }

    #[test]
    fn smith_custody_panel_shows_only_matching_service_orders() {
        let orders = [
            crate::spacetimedb::RepairOrder {
                id: 1,
                owner_character_id: 9,
                inventory_item_id: 11,
                item_id: "sword".into(),
                settlement_id: "viabundus-1".into(),
                smith_skill: 3,
                submitted_at_minutes: 0,
                ready_at_minutes: 10,
                target_condition: 1.0,
                quoted_cost: 12,
            },
            crate::spacetimedb::RepairOrder {
                id: 2,
                owner_character_id: 9,
                inventory_item_id: 12,
                item_id: "cuirass".into(),
                settlement_id: "viabundus-1".into(),
                smith_skill: 3,
                submitted_at_minutes: 0,
                ready_at_minutes: 10,
                target_condition: 1.0,
                quoted_cost: 24,
            },
        ];
        let items = [
            crate::spacetimedb::ItemDefinition {
                id: "sword".into(),
                weight: 1.0,
                slot: ItemSlot::AnyHolding,
                kind: crate::spacetimedb::ItemKind::Weapon,
                base_value: None,
                nutrition_kcal: 0.0,
                water_capacity_ml: 0,
                quality: 3,
                durability_yield: 0.0,
                durability_fracture: 0.0,
                durability_wear: 0.0,
                durability_failure_share: 0.0,
                edge_sensitivity: 0.0,
                handling_sensitivity: 0.0,
                ..Default::default()
            },
            crate::spacetimedb::ItemDefinition {
                id: "cuirass".into(),
                weight: 1.0,
                slot: ItemSlot::Chest,
                kind: crate::spacetimedb::ItemKind::Armor,
                base_value: None,
                nutrition_kcal: 0.0,
                water_capacity_ml: 0,
                quality: 3,
                durability_yield: 0.0,
                durability_fracture: 0.0,
                durability_wear: 0.0,
                durability_failure_share: 0.0,
                edge_sensitivity: 0.0,
                handling_sensitivity: 0.0,
                ..Default::default()
            },
        ];
        let weapons = repair_custody_panel(
            &settlement(),
            MerchantShop::Weapons,
            &orders,
            &[],
            &items,
            0,
            4,
        )
        .into_string();
        let armor = repair_custody_panel(
            &settlement(),
            MerchantShop::Armor,
            &orders,
            &[],
            &items,
            0,
            3,
        )
        .into_string();
        assert!(weapons.contains("sword"));
        assert!(!weapons.contains("cuirass"));
        assert!(weapons.contains("repair-custody-table"));
        assert!(weapons.contains("Smithing 4"));
        assert!(weapons.contains("stat-icon-smithing"));
        for tier in 1..=5 {
            assert!(weapons.contains(&format!("skill-rank-segment-{tier}")));
        }
        assert!(weapons.contains("repair-custody-header-actions"));
        assert!(weapons.contains("inventory-actions-header"));
        assert!(weapons.contains("inventory-actions-cell"));
        assert!(weapons.contains("Durability"));
        assert!(weapons.contains("ETA"));
        assert!(weapons.contains("Full repair cost"));
        assert!(weapons.contains("repair-retrieve-all"));
        assert!(weapons.contains("Retrieve up to two completed matching items"));
        assert!(!weapons.to_lowercase().contains("affordable prefix"));
        assert!(weapons.contains("/repairs/1/retrieve"));
        assert!(weapons.contains(">12<"));
        assert!(!weapons.contains("Target "));
        assert!(armor.contains("cuirass"));
        assert!(!armor.contains("sword"));
    }

    #[test]
    fn aliases_are_deduplicated_and_do_not_repeat_the_canonical_name() {
        let aliases = [
            SettlementAlias {
                id: "1".into(),
                settlement_id: "viabundus-1".into(),
                name: "Lubeke".into(),
                prefix: None,
                language: Some("deu".into()),
            },
            SettlementAlias {
                id: "2".into(),
                settlement_id: "viabundus-1".into(),
                name: "Lübeck".into(),
                prefix: None,
                language: None,
            },
        ];

        assert_eq!(settlement_alias_labels(&settlement(), &aliases), ["Lubeke"]);
    }

    #[test]
    fn english_settlement_description_is_preferred_deterministically() {
        let descriptions = [
            SettlementDescription {
                id: "1".into(),
                settlement_id: "viabundus-1".into(),
                kind: SettlementDescriptionKind::Settlement,
                language: Some("deu".into()),
                body: "Deutsch".into(),
            },
            SettlementDescription {
                id: "2".into(),
                settlement_id: "viabundus-1".into(),
                kind: SettlementDescriptionKind::City,
                language: Some("eng".into()),
                body: "English city".into(),
            },
            SettlementDescription {
                id: "3".into(),
                settlement_id: "viabundus-1".into(),
                kind: SettlementDescriptionKind::Settlement,
                language: Some("eng".into()),
                body: "English settlement".into(),
            },
        ];

        assert_eq!(
            preferred_settlement_description(&descriptions)
                .unwrap()
                .body,
            "English settlement"
        );
    }

    #[test]
    fn settlement_overview_renders_enrichment_as_escaped_text() {
        let aliases = [SettlementAlias {
            id: "1".into(),
            settlement_id: "viabundus-1".into(),
            name: "Lubeke".into(),
            prefix: None,
            language: Some("deu".into()),
        }];
        let descriptions = [SettlementDescription {
            id: "1".into(),
            settlement_id: "viabundus-1".into(),
            kind: SettlementDescriptionKind::Settlement,
            language: Some("deu".into()),
            body: "Burg & Markt <alt>".into(),
        }];

        let markup =
            settlement_overview_page(&settlement(), &aliases, &descriptions, None, &[], None)
                .into_string();

        assert!(markup.contains("Also known as"));
        assert!(markup.contains("Lubeke"));
        assert!(markup.contains("Historical description — German"));
        assert!(markup.contains("Burg &amp; Markt &lt;alt&gt;"));
        assert!(!markup.contains("<alt>"));
    }
    use super::*;

    fn quest_destination() -> TravelDestination {
        TravelDestination {
            id: "quest-location".to_string(),
            name: "Bandit camp".to_string(),
            description: "A camp beside the road.".to_string(),
            summary: Some("Active quest".to_string()),
            travel_action: "/quests/quest-location/travel".to_string(),
            track_action: Some("/case-sites/quest-location/track".to_string()),
            tracked: false,
            distance_m: 1_000,
            journey_minutes: 48,
            camp_stop_minutes: Vec::new(),
            camp_forecasts: Vec::new(),
            departure_minute: 0,
            itinerary_total_elapsed_minutes: 96,
            itinerary_segments: Vec::new(),
            quest_in_progress: true,
            provision_forecast: None,
            terrain_route: None,
            return_terrain_route: None,
            route_fallback: true,
        }
    }

    #[test]
    fn active_quest_destination_has_red_status_badge() {
        let destination = quest_destination();

        let markup = map_destination_list(&[destination], None, "/locations/settlement/test/map")
            .into_string();

        assert!(markup.contains("destination-quest-badge"));
        assert!(markup.contains("aria-label=\"Active quest destination\""));
        assert!(markup.contains("title=\"A camp beside the road.\nActive quest\""));
        assert!(!markup.contains("destination-turn-in-badge"));
    }

    #[test]
    fn current_settlement_has_no_conventional_quest_marker() {
        let markup = map_destination_list_with_context(
            &[],
            None,
            "/locations/settlement/market/map",
            Some(MapCurrentLocation { name: "Market" }),
            None,
            None,
        )
        .into_string();

        assert!(markup.contains("current-location-row"));
        assert!(markup.contains("aria-current=\"location\""));
        assert!(!markup.contains("destination-open-quest-badge"));
        assert!(!markup.contains("destination-quest-badge"));
        assert!(!markup.contains("href="));
    }

    #[test]
    fn map_exposes_abandon_action_for_an_eligible_active_quest() {
        let markup = map_destination_list_with_context(
            &[],
            None,
            "/locations/settlement/issuer/map",
            Some(MapCurrentLocation { name: "Issuer" }),
            Some(MapAbandonableQuest {
                id: "active",
                title: "Drive off the bandits",
            }),
            None,
        )
        .into_string();

        assert!(markup.contains("Active quest: "));
        assert!(markup.contains("Drive off the bandits"));
        assert!(markup.contains("action=\"/quests/active/abandon\""));
        assert!(markup.contains("Abandon active quest"));
    }

    #[test]
    fn map_rest_menu_is_pinned_below_the_destination_list() {
        let markup = map_destination_list_with_rest(
            &[],
            None,
            "/locations/case-site/active/map",
            html! { section class="rest-service-menu" { "Rest party" } },
        )
        .into_string();

        assert!(markup.contains("left-sidebar map-rest-sidebar"));
        assert!(markup.contains("map-rest-sidebar-content"));
        assert!(markup.contains("rest-service-menu"));
        assert!(markup.contains("Rest party"));
    }

    #[test]
    fn quest_location_travel_has_one_plain_action_without_settlement_buying() {
        let destination = quest_destination();
        let markup = map_destination_detail(
            Some(&destination),
            None,
            false,
            true,
            false,
            None,
            None,
            false,
            None,
            "/map",
        )
        .into_string();

        assert!(markup.contains("Begin journey"));
        assert!(markup.contains("action=\"/case-sites/quest-location/track\""));
        assert!(markup.contains("Track site"));
        assert!(!markup.contains("<p>A camp beside the road.</p>"));
        assert!(!markup.contains("Active quest"));
        assert!(!markup.contains("name=\"provisioning\""));
        assert!(!markup.contains("data-provision-buy"));
    }

    #[test]
    fn nonconnected_map_selection_has_detail_but_no_travel_form() {
        let mut destination = settlement();
        destination.id = "viabundus-99".into();
        destination.name = "Distant town".into();
        let markup = map_destination_detail(
            None,
            Some(&destination),
            false,
            true,
            true,
            None,
            None,
            false,
            None,
            "/locations/settlement/viabundus-1/map",
        )
        .into_string();

        assert!(markup.contains("Distant town"));
        assert!(markup.contains("No direct route."));
        assert!(!markup.contains("Begin journey"));
        assert!(!markup.contains("data-travel-submit"));
    }

    #[test]
    fn connected_settlement_selection_keeps_existing_travel_action() {
        let mut destination = quest_destination();
        destination.id = "viabundus-2".into();
        destination.name = "Connected town".into();
        destination.quest_in_progress = false;
        destination.travel_action = "/settlements/viabundus-2/travel".into();
        let markup = map_destination_detail(
            Some(&destination),
            None,
            false,
            true,
            true,
            None,
            None,
            false,
            None,
            "/locations/settlement/viabundus-1/map",
        )
        .into_string();

        assert!(markup.contains("action=\"/settlements/viabundus-2/travel\""));
        assert!(markup.contains("data-travel-submit"));
        assert!(markup.contains("Begin journey"));
        assert!(!markup.contains("No direct route"));
    }

    #[test]
    fn chat_uses_one_stream_with_all_channel_filters() {
        let markup = chat_area("Lubeck", None, None, None, None, &[]).into_string();

        assert!(!markup.contains("role=\"tablist\""));
        for channel in ["local", "party", "settlement", "dm", "guild", "info"] {
            assert!(
                markup.contains(&format!("data-chat-filter=\"{channel}\"")),
                "missing {channel} filter"
            );
        }
        assert!(markup.contains("data-chat-channel=\"info\""));
        assert!(!markup.contains("chat-channel-badge"));
        assert!(markup.contains("class=\"settlement-chat-layout\""));
        assert!(markup.contains("data-dialogue-topic-pane"));
        assert!(markup.contains("data-dialogue-topic-list"));
        assert!(markup.contains("data-dialogue-completion"));
        assert!(markup.contains("autocomplete=\"off\""));
        for label in ["Local", "Party", "Settlement", "DMs", "Guild", "Info"] {
            assert!(markup.contains(&format!("aria-label=\"{label}\" title=\"{label}\"")));
            assert!(!markup.contains(&format!(">{label}</")));
        }
    }

    #[test]
    fn settlement_npc_strip_exposes_accessible_authoritative_context() {
        let strip = npc_portrait_strip("lubeck", "market").into_string();
        assert!(strip.contains("aria-label=\"People here\""));
        assert!(strip.contains("data-npc-settlement=\"lubeck\""));
        assert!(strip.contains("data-npc-location=\"market\""));
        let chat = settlement_npc_chat_area("Market", None, "lubeck", "market", Some("merchants"))
            .into_string();
        assert!(chat.contains("data-local-chat-kind=\"npc\""));
        assert!(chat.contains("data-local-chat-location=\"market\""));
        assert!(chat.contains("data-dialogue-catalog-revision"));
        assert!(!chat.contains("lubeck:merchants"));
    }

    #[test]
    fn non_service_locations_use_the_same_authoritative_npc_shell() {
        let character = Character {
            id: 1,
            name: "Visitor".into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: Some("viabundus-1".into()),
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive: true,
            temporary: false,
        };
        for location in ["residences", "keep"] {
            let markup = settlement_npc_location_page(
                &settlement(),
                &character,
                &[],
                location,
                Some("Visitor"),
            )
            .into_string();
            assert!(markup.contains(&format!("data-npc-location=\"{location}\"")));
            assert!(markup.contains("data-npc-strip"));
            assert!(markup.contains("data-dialogue-catalog-revision"));
            assert!(markup.contains("aria-label=\"Settlement places\""));
            assert!(markup.contains("href=\"/locations/settlement/viabundus-1/party/1\""));
            assert!(markup.contains("href=\"/locations/settlement/viabundus-1/party-inventory\""));
            assert!(!markup.contains(&format!("/places/{location}/party/")));
        }
    }

    #[test]
    fn chat_palette_meets_contrast_across_every_supported_theme() {
        fn linear_channel(channel: u8) -> f64 {
            let channel = f64::from(channel) / 255.0;
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        }

        fn luminance([red, green, blue]: [u8; 3]) -> f64 {
            0.2126 * linear_channel(red)
                + 0.7152 * linear_channel(green)
                + 0.0722 * linear_channel(blue)
        }

        fn contrast(first: [u8; 3], second: [u8; 3]) -> f64 {
            let (lighter, darker) = if luminance(first) > luminance(second) {
                (luminance(first), luminance(second))
            } else {
                (luminance(second), luminance(first))
            };
            (lighter + 0.05) / (darker + 0.05)
        }

        fn mix(accent: [u8; 3], text: [u8; 3], accent_percent: u16) -> [u8; 3] {
            std::array::from_fn(|index| {
                let mixed = u16::from(accent[index]) * accent_percent
                    + u16::from(text[index]) * (100 - accent_percent);
                ((mixed + 50) / 100) as u8
            })
        }

        // Dark themes use the lightest possible 88% panel composite (over
        // white); light themes use the darkest possible composite (over
        // black). This brackets the image content beneath the translucent chat.
        let legacy_palettes = [
            (
                "Dark Arcanum",
                [46, 49, 67],
                [200, 202, 208],
                [154, 158, 176],
                [96, 165, 250],
                [251, 191, 36],
                [215, 169, 239],
                [52, 211, 153],
            ),
            (
                "Fraktur Nocturne",
                [60, 49, 44],
                [241, 227, 207],
                [205, 185, 157],
                [125, 159, 197],
                [213, 166, 76],
                [213, 167, 237],
                [120, 173, 114],
            ),
            (
                "Fraktur Texturina",
                [216, 209, 190],
                [42, 31, 20],
                [74, 60, 44],
                [58, 106, 138],
                [184, 134, 11],
                [116, 66, 141],
                [74, 124, 63],
            ),
            (
                "Imperial Crimson",
                [217, 213, 204],
                [26, 26, 26],
                [61, 61, 61],
                [26, 74, 138],
                [196, 136, 11],
                [113, 63, 140],
                [45, 106, 48],
            ),
            (
                "Northern Frost",
                [211, 215, 220],
                [28, 40, 51],
                [52, 73, 94],
                [46, 109, 164],
                [212, 160, 23],
                [115, 66, 147],
                [39, 174, 96],
            ),
            (
                "Renaissance Gold",
                [216, 209, 190],
                [42, 31, 20],
                [74, 60, 44],
                [58, 106, 138],
                [184, 134, 11],
                [123, 63, 145],
                [74, 124, 63],
            ),
            (
                "Verdant Chronicle",
                [218, 214, 202],
                [26, 60, 26],
                [45, 90, 45],
                [74, 122, 106],
                [184, 115, 51],
                [116, 66, 141],
                [58, 122, 58],
            ),
        ];
        for (palette, surface, primary, secondary, info, gold, dm, success) in legacy_palettes {
            let channels = [
                ("Local", primary),
                ("Party", mix(info, primary, 40)),
                ("Settlement", mix(gold, primary, 35)),
                ("DM", mix(dm, primary, 35)),
                ("Guild", mix(success, primary, 40)),
                ("Info", secondary),
            ];
            let distinct = channels
                .iter()
                .map(|(_, color)| color)
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(
                distinct.len(),
                channels.len(),
                "{palette} channels must remain visually distinct"
            );
            for (channel, color) in channels {
                assert!(
                    contrast(color, surface) >= 4.5,
                    "{palette} {channel} does not meet WCAG AA text contrast"
                );
            }
        }
    }

    #[test]
    fn chat_css_keeps_fallbacks_and_mobile_message_space() {
        let css = include_str!("../../static/css/strategic.css");
        let utilities = include_str!("../../static/css/utilities.css");
        let trade_script = include_str!("../../static/party-trade.js");
        let fallback = css
            .find("background: rgb(33 21 15 / 88%);")
            .expect("chat needs a background fallback");
        let enhanced = css
            .find("background: color-mix(in srgb, var(--panel-bg) 88%, transparent);")
            .expect("chat should derive its translucent surface from the fixed palette");

        assert!(fallback < enhanced);
        assert!(css.contains("background: color-mix(in srgb, var(--header-bg) 86%, transparent);"));
        assert!(css.contains(".chat-channel-filter input::after"));
        assert!(css.contains("text-decoration: line-through"));
        assert!(css.contains("outline: 2px solid var(--text-primary);"));
        assert!(css.contains("0 0 0 2px var(--panel-bg)"));
        assert!(css.contains(
            "--chat-party-color: color-mix(in srgb, var(--info) 40%, var(--text-primary));"
        ));
        assert!(css.contains("--chat-settlement-color: color-mix(in srgb, var(--gold-color) 35%, var(--text-primary));"));
        assert!(css.contains("--chat-dm-color: color-mix(in srgb, var(--icon-instinct, #7b3f91) 35%, var(--text-primary));"));
        assert!(css.contains(
            "--chat-guild-color: color-mix(in srgb, var(--success) 40%, var(--text-primary));"
        ));
        for variable in ["local", "party", "settlement", "dm", "guild", "info"] {
            assert!(css.contains(&format!("var(--chat-{variable}-color)")));
        }
        assert!(!css.contains(".chat-channel-badge"));
        assert!(css.contains("@media (max-width: 768px)"));
        assert!(css.contains("flex-wrap: nowrap;"));
        assert!(css.contains("min-height: 10rem;"));
        assert!(css.contains(".repair-custody-list { margin-top: auto; }"));
        assert!(css.contains("max-height: 50%;"));
        assert!(css.contains("@keyframes repairable-damage-pulse"));
        assert!(css.contains("@media (prefers-reduced-motion: reduce)"));
        let repairable_rule = css
            .split(".condition-repairable {")
            .nth(1)
            .and_then(|tail| tail.split('}').next())
            .expect("repairable condition segments need a style rule");
        assert!(!repairable_rule.contains("background-image"));
        assert!(!repairable_rule.contains("box-shadow"));
        for tier in 1..=5 {
            assert!(css.contains(&format!(".condition-tier-{tier}")));
            assert!(css.contains(&format!(".item-quality-{tier}")));
        }
        assert!(css.contains("color-mix(in srgb, var(--quality-color) 50%, var(--text-primary))"));
        assert!(css.contains("filter: brightness(1.15)"));
        assert!(css.contains("0%, 58%, 82%, 100%"));
        assert!(css.contains("66%, 74%"));
        assert!(!css.contains("left: -7rem;"));
        assert!(css.contains(".smith-wares-scroll .trade-inventory-table"));
        assert!(css.contains("--inventory-merchant-action-overhang"));
        assert!(css.contains("--inventory-merchant-scrollbar-reserve: 8px;"));
        assert!(css.contains("padding-left: var(--inventory-merchant-scrollbar-reserve);"));
        assert!(css.contains("padding-right: var(--inventory-merchant-action-overhang);"));
        assert!(css.contains("direction: rtl;"));
        assert!(css.contains(".smith-wares-scroll > * { direction: ltr; }"));
        assert!(css.contains("scrollbar-gutter: stable;"));
        assert!(css.contains("overflow-x: clip;"));
        assert!(css.contains("col.inventory-column-item { width: auto; }"));
        assert!(css.contains(".smith-player-inventory-table"));
        assert!(css.contains("width: 3.65rem;"));
        assert!(css.contains("--repair-custody-action-overhang"));
        assert!(css.contains("width: calc(100% + var(--repair-custody-action-overhang));"));
        assert!(css.contains("padding-right: var(--repair-custody-action-overhang);"));
        assert!(css.contains("scrollbar-gutter: stable;"));
        assert!(utilities.contains(".inventory-row-actions.smith-player-actions"));
        assert!(utilities.contains("--inventory-action-bridge:.3rem"));
        assert!(!utilities.contains(".smith-wares-scroll .inventory-row-actions"));
        assert!(utilities.contains(".inventory-actions-cell"));
        assert!(
            utilities.contains(".left-sidebar .inventory-actions-cell > .inventory-row-actions")
        );
        assert!(
            utilities.contains(".right-sidebar .inventory-actions-cell > .inventory-row-actions")
        );
        assert!(utilities.contains("background:var(--inventory-row-background"));
        assert!(utilities.contains("top:0; bottom:0;"));
        assert!(utilities.contains(
            ".trade-inventory-row:not(:last-child) .inventory-row-actions { bottom:-1px; }"
        ));
        assert!(utilities.contains(".inventory-row-actions .trade-transfer:disabled"));
        assert!(utilities.contains("opacity:.42; transform:none;"));
        assert!(utilities.contains("left:100%; padding-left:var(--inventory-action-bridge);"));
        assert!(utilities.contains("right:100%; padding-right:var(--inventory-action-bridge);"));
        assert!(css.contains(".inventory-browser-table-frame"));
        assert!(css.contains("width:max-content;"));
        assert!(utilities.contains(".inventory-footer-repair .repair-all-button"));
        assert!(utilities.contains("grid-template-columns:repeat(2,1.35rem)"));
        assert!(utilities.contains(".inventory-actions-header > .inventory-footer-actions"));
        assert!(utilities.contains("thead:hover .inventory-footer-actions"));
        assert!(utilities.contains("background:var(--panel-bg)"));
        assert!(
            utilities
                .contains(".smith-player-actions .row-repair-form { position:static; order:0;")
        );
        assert!(trade_script.contains("if (stockRow) changeTradeDraftCount(stockRow, amount);"));
        assert!(trade_script.contains("function applyDynamicTransferModifiers(event)"));
        assert!(trade_script.contains("event.key === \"Shift\" || event.key === \"Control\""));
        assert!(trade_script.contains("controlKey ? \"all\""));
    }

    #[test]
    fn schedule_and_equipment_scripts_use_the_new_interactions() {
        let schedule = include_str!("../../static/training-schedule.js");
        let numeric = include_str!("../../static/numeric-editor.js");
        let equipment = include_str!("../../static/equipment-toggle.js");
        let live_regions = include_str!("../../static/live-regions.js");
        let immediate_activity = include_str!("../../static/immediate-activity.js");
        let css = include_str!("../../static/css/strategic.css");
        assert!(schedule.contains("function parseClock(value)"));
        assert!(schedule.contains("window.StrategicNumericEditor.open"));
        assert!(numeric.contains("input.type = 'text'"));
        assert!(numeric.contains("confirm.addEventListener('click', () => finish(true))"));
        assert!(numeric.contains("cancel.addEventListener('click', () => finish(false))"));
        assert!(numeric.contains("input.addEventListener('wheel'"));
        assert!(!numeric.contains("document.addEventListener('wheel'"));
        assert!(schedule.contains("/^\\d{3,4}$/"));
        assert!(schedule.contains("Math.round(wanted / STEP) * STEP"));
        assert!(schedule.contains("function renderActivityPreview(row, minutes)"));
        assert!(schedule.contains("function calculateLeisurePreview"));
        assert!(schedule.contains("row.dataset.leisureFatiguePreviewDivisor"));
        assert!(schedule.contains("function mountSchedules(root = document)"));
        assert!(schedule.contains("[data-social-expand]"));
        assert!(schedule.contains(".social-detail-row"));
        assert!(schedule.contains("'strategic-live-regions-refreshed'"));
        assert!(schedule.contains("event.detail.regions.includes('left-sidebar')"));
        assert!(schedule.contains("function createLatestSaveQueue(send"));
        assert!(schedule.contains("data-schedule-pending"));
        assert!(schedule.contains("retry()"));
        assert!(schedule.contains("data-schedule-save-status"));
        assert!(schedule.contains("data-schedule-retry"));
        assert!(schedule.contains("strategic-live-refresh-requested"));
        assert!(schedule.contains("schedule-effect-positive"));
        assert!(!schedule.contains("scheduleDrag"));
        assert!(!schedule.contains("travel_"));
        assert!(equipment.contains("'/api/equipment'"));
        assert!(equipment.contains("window.location.reload()"));
        assert!(live_regions.contains("const scrollOffsets = (selector)"));
        assert!(live_regions.contains("region.scrollTop = offsets.top"));
        assert!(live_regions.contains("replaced.includes(\"left-sidebar\")"));
        assert!(live_regions.contains("document.querySelector('.numeric-editor')"));
        assert!(live_regions.contains("[data-activity-modal]:not([hidden])"));
        assert!(live_regions.contains("scheduleEditorIsPending"));
        assert!(live_regions.contains("const schedulePendingAtStart = scheduleEditorIsPending()"));
        assert!(live_regions.contains("!schedulePendingAtStart && !scheduleEditorIsPending()"));
        assert!(immediate_activity.contains("typeof window === 'undefined'"));
        assert!(immediate_activity.contains("input:not([type=\"hidden\"]):not(:disabled)"));
        assert!(immediate_activity.contains("wrappedFocusTarget"));
        assert!(immediate_activity.contains("strategic-editor-idle"));
        assert!(css.contains(".numeric-editor-input {"));
        assert!(css.contains("position: fixed;"));
        assert!(css.contains("z-index: 80;"));
        assert!(css.contains(".numeric-editor {"));
        assert!(css.contains("right: auto;"));
        assert!(css.contains("left: 50%;"));
        assert!(css.contains("transform: translate(-50%, -50%);"));
        assert!(css.contains(".numeric-editor-input::selection {"));
        assert!(numeric.contains("document.body.append(editor)"));
        assert!(numeric.contains("display.style.visibility = 'hidden'"));
        assert!(!numeric.contains("display.hidden = true"));
        assert!(numeric.contains("window.addEventListener('resize', positionEditor)"));
        assert!(!css.contains(".party-skill-icon-column"));
        assert!(css.contains(".numeric-editor-action {"));
        assert!(css.contains(".numeric-editor-confirm { background: #2f7d3d; }"));
        assert!(css.contains(".numeric-editor-cancel { background: #9c3434; }"));
        assert!(css.contains(".schedule-save-status"));
    }

    #[test]
    fn low_medicine_medical_html_contains_no_hidden_payload() {
        let presentation = crate::medical::MedicalPresentation {
            unavailable: false,
            symptoms: vec!["coughing"],
            diagnoses: Vec::new(),
            ..Default::default()
        };
        let markup = medical_rail(&presentation, "/location", 1, 2, true).into_string();
        assert!(markup.contains("coughing"));
        assert!(!markup.contains("Examine"));
        assert!(!markup.contains("Visible injuries"));
        for forbidden in ["Vitals", "influenza", "infection_id", "disease", "humour-"] {
            assert!(!markup.contains(forbidden), "leaked {forbidden}: {markup}");
        }
    }

    #[test]
    fn active_medication_is_listed_beneath_symptoms() {
        let presentation = crate::medical::MedicalPresentation {
            symptoms: vec!["coughing"],
            medications: vec![crate::medical::MedicationPresentation {
                equipment_id: 11,
                disease_name: "Consumption",
            }],
            ..Default::default()
        };
        let markup = medical_rail(&presentation, "/location", 1, 2, false).into_string();
        let symptoms_at = markup.find("coughing").unwrap();
        let medication_at = markup.find("Taking medication for Consumption.").unwrap();
        assert!(medication_at > symptoms_at);
    }

    #[test]
    fn medicine_action_moves_from_portrait_to_the_skill_icon() {
        let doctor = Character {
            id: 1,
            name: "Doctor".into(),
            xp: 0,
            level: 1,
            gold: 100,
            current_settlement_id: Some("willowmere".into()),
            current_case_site_id: None,
            party_id: Some("demo".into()),
            age_years: 30,
            alive: true,
            temporary: false,
        };
        let portrait = party_portrait_overlay(
            &[doctor.clone()],
            Some(&doctor),
            "/locations/settlement/willowmere",
            Some(1),
            true,
        )
        .into_string();
        assert!(!portrait.contains("/examine"));
        assert!(!portrait.contains("party-medical-examine"));
        assert!(portrait.contains("party-alchemy-action"));

        let skill = skill_action_icon(
            "Medicine",
            "medicine",
            SkillAction::Post {
                href: "/place/party/1/examine",
                label: "Perform medical examination (15 minutes)",
                open: false,
            },
            false,
        )
        .into_string();
        assert!(skill.contains("/place/party/1/examine"));
        assert!(skill.contains("Perform medical examination (15 minutes)"));
        assert!(skill.contains("aria-haspopup=\"dialog\" aria-expanded=\"false\""));
    }

    #[test]
    fn pending_examination_is_a_one_shot_center_popup_not_sidebar_history() {
        let presentation = crate::medical::MedicalPresentation {
            findings: vec!["coughing".into(), "fatigued".into()],
            examination_id: Some(44),
            examined_at: Some(8_640),
            regional_humours: Some(
                [crate::medical::HumourVitals {
                    sanguine: 0.9,
                    phlegmatic: 0.6,
                    choleric: 0.8,
                    melancholic: 1.0,
                }; 7],
            ),
            possible_diagnoses: vec!["Catarrhal fever", "Consumption"],
            ..Default::default()
        };
        let sidebar = medical_rail(&presentation, "/location", 1, 2, true).into_string();
        assert!(!sidebar.contains("Four humours"));
        assert!(!sidebar.contains("Possible ailments"));
        assert!(!sidebar.contains("Observed at personal minute"));

        let location = LocationView {
            kind: LocationKind::Quest,
            id: "location".into(),
            name: "Location".into(),
            religion_id: None,
            category: None,
            active_building: Some("inn".into()),
        };
        let popup =
            medical_examination_popup(&presentation, &location, 2, None, &[], &[]).into_string();
        assert!(popup.contains("medical-examination-overlay"));
        assert!(popup.contains("aria-modal=\"true\""));
        assert!(popup.contains("Possible ailments"));
        assert!(popup.contains("Body regions"));
        assert!(popup.contains("attribute-health-phlegmatic"));
        assert!(popup.contains("Catarrhal fever"));
        assert!(popup.contains("Close examination findings"));
        assert!(popup.contains('×'));
        assert!(!popup.contains("This result is discarded"));
        assert!(popup.contains("/examination/44/dismiss"));
        assert!(popup.contains("?building=inn"));
        assert!(popup.contains("data-medical-examination"));
        let lifecycle = include_str!("../../static/medical-examination.js");
        assert!(lifecycle.contains("pagehide"));
        assert!(lifecycle.contains("navigator.sendBeacon"));
        assert!(lifecycle.contains("event.key !== \"Escape\""));
        assert!(lifecycle.contains("restoreFocus"));
        assert!(lifecycle.contains(".party-offer[role='dialog']"));
    }

    #[test]
    fn examined_region_meter_has_text_and_aria_not_color_alone() {
        let presentation = crate::medical::MedicalPresentation {
            regional_humours: Some(
                [crate::medical::HumourVitals {
                    phlegmatic: 0.4,
                    ..Default::default()
                }; 7],
            ),
            ..Default::default()
        };
        let markup = regional_health_bar("Chest", 1.0, &presentation, 4, &[], &[]).into_string();
        assert!(markup.contains("Phlegmatic"));
        assert!(markup.contains("role=\"meter\""));
        assert!(markup.contains("Chest:"));
    }

    #[test]
    fn surgery_button_is_explicit_and_the_health_meter_is_not_clickable() {
        let markup = attribute_group(
            "Head",
            "head",
            0.75,
            &crate::medical::MedicalPresentation::default(),
            6,
            Some(("/place/party/1/surgery", None)),
            &[],
            &[],
            &[("Intelligence", "intelligence", 3.0)],
        )
        .into_string();

        let link_start = markup.find("limb-surgery-button").unwrap();
        let health_bar = markup.find("class=\"attribute-health-bar\"").unwrap();
        let link_end = markup[link_start..].find("</a>").unwrap() + link_start;
        let attribute_row = markup.find("class=\"party-attribute-row\"").unwrap();

        assert!(link_start < link_end);
        assert!(link_end < health_bar);
        assert!(link_end < attribute_row);
        assert!(markup.contains("aria-label=\"Open surgery menu for Head\""));
        assert!(markup.contains("aria-haspopup=\"dialog\" aria-expanded=\"false\""));
    }

    #[test]
    fn treated_cuts_and_fractures_expose_banded_health_bar_states() {
        let injury = LimbInjury {
            id: "1:chest".into(),
            character_id: 1,
            limb: LimbRegion::Chest,
            cut_damage: 0.2,
            bruise_damage: 0.2,
            fracture_damage: 0.2,
            bandaged: true,
            stitched: false,
            stitch_quality: 0.0,
            splint_owner_id: Some(2),
            splint_inventory_item_id: Some(3),
            infection_exposure: 0.0,
            infection_checks: 0,
            infection_origin_minute: None,
        };
        let markup = regional_health_bar(
            "Chest",
            0.6,
            &crate::medical::MedicalPresentation::default(),
            4,
            &[injury],
            &[],
        )
        .into_string();

        assert!(markup.contains("attribute-health-cut bandaged-cut"));
        assert!(markup.contains("title=\"Bandaged cut damage\""));
        assert!(markup.contains("attribute-health-fracture splinted-fracture"));
        assert!(markup.contains("title=\"Splinted fracture\""));
        assert!(markup.contains("20% splinted fracture"));
    }

    #[test]
    fn persisted_quest_camp_keeps_turnaround_movement_after_elapsed_rest() {
        let mut journey = PartyJourney {
            party_id: "party".into(),
            gateway_bucket: 0,
            origin: crate::spacetimedb::JourneyEndpoint::Settlement(
                crate::spacetimedb::JourneySettlementEndpoint {
                    id: "home".into(),
                    name: "Home".into(),
                },
            ),
            destination: crate::spacetimedb::JourneyEndpoint::CaseSite(
                crate::spacetimedb::JourneyCaseSiteEndpoint {
                    id: crate::spacetimedb::CaseSiteId {
                        value: "quest".into(),
                    },
                    name: "Quest".into(),
                },
            ),
            total_minutes: 720,
            completed_minutes: 480,
            camp_stop_minutes: vec![480],
            forecast_camp_stop_minutes: vec![480],
            fatigue_percent: 50,
            plan_version: 1,
            departure_minute: 10_000,
            total_elapsed_minutes: 2_040,
            completed_elapsed_minutes: 780,
            walking_minutes_per_day: 480,
            travel_at_night: false,
            camp_duration_mode: crate::spacetimedb::CampDurationMode::Auto,
            fixed_camp_minutes: 0,
        };
        let camp = |start, duration, from, to| crate::spacetimedb::JourneyCampInterval {
            movement_minute: 480,
            elapsed_start_minute: start,
            elapsed_minutes: duration,
            average_fatigue_start: from,
            average_fatigue_end: to,
            maximum_fatigue_end: to,
        };
        let itinerary = PartyJourneyItinerary {
            party_id: "party".into(),
            actual_camp_intervals: vec![camp(480, 300, 0.5, 0.2)],
            forecast_camp_intervals: vec![camp(780, 300, 0.2, 0.0)],
        };
        assert!(
            !camp_fire_is_lit(Some(&journey), Some(&itinerary)),
            "resting at the current movement checkpoint leaves smoke-only embers"
        );
        journey.completed_minutes = 600;
        assert!(
            camp_fire_is_lit(Some(&journey), Some(&itinerary)),
            "reaching a later camp relights the fire"
        );
        journey.completed_minutes = 480;
        let encoded = format_persisted_itinerary(&journey, &itinerary);
        assert!(encoded.contains("w,0,480,0,480"));
        assert!(encoded.contains("m,480,600,480,0"));
        assert!(encoded.contains("w,1080,960,480,960"));
        assert_eq!(
            encoded
                .split('|')
                .filter(|segment| segment.starts_with("m,"))
                .count(),
            1,
            "one physical camp marker"
        );
    }

    #[test]
    fn intentional_stages_have_distinct_semantics_and_no_prototype_copy() {
        for (kind, label) in [
            ("settlement", "At the settlement gates"),
            ("route", "Roads and destinations"),
            ("camp", "Camp beside the road"),
            ("character", "Adventurer profile"),
            ("service", "At the counter"),
            ("quest", "Encounter ground"),
            ("alchemy", "The apothecary workbench"),
            ("chest", "Shared party stores"),
        ] {
            let markup = visual_stage(kind, "A Place", "An intentional scene").into_string();
            assert!(markup.contains(label));
            assert!(markup.contains("role=\"img\""));
            assert!(!markup.contains("placeholder"));
            assert!(!markup.contains("TODO"));
            assert!(!markup.contains("visual-scene-emblem"));
            assert!(!markup.contains("/static/icons/game/"));
        }
    }

    #[test]
    fn responsive_and_hidden_control_rules_keep_content_available() {
        let layout = include_str!("../../static/css/layout.css");
        let strategic = include_str!("../../static/css/strategic.css");
        let utilities = include_str!("../../static/css/utilities.css");
        assert!(layout.contains("grid-template-areas: \"main\" \"left\" \"right\""));
        assert!(layout.contains(".right-sidebar {\n    display: block;"));
        assert!(strategic.contains("@media (hover: none), (pointer: coarse)"));
        assert!(utilities.contains(".inventory-count:focus-within"));
        assert!(utilities.contains("@media (hover:none), (pointer:coarse)"));
        assert!(utilities.contains("width: 2.75rem"));
        assert!(utilities.contains("height: 2.75rem"));
        assert!(utilities.contains("grid-template-columns: 2.75rem 1.4rem 2.75rem"));
        for scene in [
            "settlement",
            "route",
            "camp",
            "quest",
            "character",
            "service",
            "alchemy",
            "chest",
        ] {
            assert!(strategic.contains(&format!(".service-visual-{scene}")));
        }
    }
}
