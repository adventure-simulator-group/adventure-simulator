#[cfg(test)]
mod rest_form_tests {
    use adventuresim_core::strategic_time::{is_walking_time, minutes_until_next_walking_start};
    use serde_json::json;

    use super::{
        RestForm, SETTLEMENTS_SOURCE, calculate_rest_supply_availability,
        calculate_soap_rest_preview, camp_continue_block_reason, field_shelter_argument,
        rest_spending_breakdown, safe_rest_error, settlement_rest_minutes, travel_rest_minutes,
    };
    use crate::spacetimedb::{
        Character, CharacterFilth, CharacterPersonality, Conscience, Conviction, Drive,
        FilthOrigin, FilthSubstance, Hygiene, InventoryItem, InventoryItemAmount, ItemDefinition,
        Nerve, Outlook, PartyInventoryItem, PartyItemAmount, SelfRegard, Sociability, Temperance,
    };
    use crate::templates::settlement::SoapRestPreview;

    fn form(duration: &str, unit: &str, requested_minutes: Option<u64>) -> RestForm {
        RestForm {
            duration: duration.into(),
            unit: unit.into(),
            requested_minutes,
            shelter: "bivouac".into(),
            advance_development_clock: false,
        }
    }

    #[test]
    fn field_shelter_is_a_typed_unit_variant() {
        let mut request = form("08:00", "hours", None);
        assert_eq!(
            field_shelter_argument(&request).unwrap(),
            json!({"bivouac": {}})
        );
        request.shelter = "tent".into();
        assert_eq!(
            field_shelter_argument(&request).unwrap(),
            json!({"tent": {}})
        );
        request.shelter = "inn".into();
        assert!(field_shelter_argument(&request).is_err());
    }

    #[test]
    fn fireplace_convenience_rest_accepts_exact_minutes() {
        assert_eq!(travel_rest_minutes(&form("37", "minutes", None)), Ok(37));
        assert!(travel_rest_minutes(&form("0", "minutes", None)).is_err());
        assert!(travel_rest_minutes(&form("1.5", "minutes", None)).is_err());
    }

    fn member(id: u64) -> Character {
        Character {
            id,
            name: format!("Member {id}"),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: None,
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 30,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        }
    }

    fn personality(character_id: u64, temperance: Temperance) -> CharacterPersonality {
        CharacterPersonality {
            character_id,
            nerve: Nerve::Neutral,
            drive: Drive::Neutral,
            outlook: Outlook::Neutral,
            sociability: Sociability::Neutral,
            conscience: Conscience::Neutral,
            self_regard: SelfRegard::Neutral,
            conviction: Conviction::Neutral,
            hygiene: Hygiene::Neutral,
            temperance,
            ..CharacterPersonality::neutral(character_id)
        }
    }

    #[test]
    fn soap_preview_exactly_splits_personal_and_shared_units() {
        let filth = [
            CharacterFilth {
                id: 1,
                character_id: 1,
                substance: FilthSubstance::Dirt,
                origin: FilthOrigin::Unknown,
                amount: 26,
                deposited_at: 0,
            },
            CharacterFilth {
                id: 2,
                character_id: 2,
                substance: FilthSubstance::Blood,
                origin: FilthOrigin::Foreign,
                amount: 30,
                deposited_at: 0,
            },
        ];
        let personal = [InventoryItem {
            id: 1,
            character_id: 1,
            item_id: "soft_soap".into(),
            qty: 1,
        }];
        let shared = [
            PartyInventoryItem {
                id: 2,
                party_id: "party".into(),
                item_id: "soft_soap".into(),
                quantity: 1,
            },
            PartyInventoryItem {
                id: 3,
                party_id: "party".into(),
                item_id: "soft_soap".into(),
                quantity: 1,
            },
        ];
        let personal_amounts = [InventoryItemAmount {
            inventory_item_id: 1,
            remaining_milliunits: 1_000_000,
        }];
        let party_amounts = [
            PartyItemAmount {
                party_inventory_item_id: 2,
                remaining_milliunits: 1_000_000,
            },
            PartyItemAmount {
                party_inventory_item_id: 3,
                remaining_milliunits: 1_000_000,
            },
        ];
        let preview = calculate_soap_rest_preview(
            &[member(1), member(2)],
            &filth,
            &personal,
            &shared,
            &personal_amounts,
            &party_amounts,
            Some("party"),
        );
        assert_eq!(preview.personal_units, 25);
        assert_eq!(preview.shared_units, 31);
        assert_eq!(preview.total_units, 56);
    }

    #[test]
    fn rest_supply_availability_greys_alcohol_for_temperate_characters() {
        let supplies = [
            InventoryItem {
                id: 1,
                character_id: 1,
                item_id: "soft_soap".into(),
                qty: 1,
            },
            InventoryItem {
                id: 2,
                character_id: 1,
                item_id: "table_wine".into(),
                qty: 1,
            },
        ];
        let alcohol = ItemDefinition {
            id: "table_wine".into(),
            alcohol_serving_ml: 250,
            alcohol_abv_basis_points: 1_200,
            alcohol_potable: true,
            ..ItemDefinition::default()
        };
        let amounts = [
            InventoryItemAmount {
                inventory_item_id: 1,
                remaining_milliunits: 1_000_000,
            },
            InventoryItemAmount {
                inventory_item_id: 2,
                remaining_milliunits: 1_000_000,
            },
        ];
        let mut preview = SoapRestPreview::default();
        calculate_rest_supply_availability(
            &mut preview,
            &[member(1)],
            &supplies,
            &[],
            &amounts,
            &[],
            &[alcohol.clone()],
            &[personality(1, Temperance::Temperate)],
            Some("party"),
        );
        assert_eq!(preview.available_units, 25);
        assert!(preview.alcohol_available);
        assert!(!preview.alcohol_will_be_consumed);

        calculate_rest_supply_availability(
            &mut preview,
            &[member(1)],
            &supplies,
            &[],
            &amounts,
            &[],
            &[alcohol],
            &[personality(1, Temperance::Neutral)],
            Some("party"),
        );
        assert!(preview.alcohol_will_be_consumed);
    }

    #[test]
    fn exact_hours_preserve_minutes_and_enforce_one_day() {
        assert_eq!(
            settlement_rest_minutes(&form("24:01", "hours", Some(1_441))),
            Ok(1_441)
        );
        assert_eq!(
            settlement_rest_minutes(&form("36:32", "hours", Some(2_192))),
            Ok(2_192)
        );
        assert!(settlement_rest_minutes(&form("23:59", "hours", Some(1_439))).is_err());
    }

    #[test]
    fn field_rest_accepts_sub_day_wake_times() {
        assert_eq!(
            travel_rest_minutes(&form("01:30", "hours", Some(90))),
            Ok(90)
        );
        assert!(travel_rest_minutes(&form("00:00", "hours", Some(0))).is_err());
    }

    #[test]
    fn hours_fallback_parses_hh_mm() {
        assert_eq!(
            settlement_rest_minutes(&form("24:31", "hours", None)),
            Ok(1_471),
        );
        assert!(settlement_rest_minutes(&form("24:60", "hours", None)).is_err());
        assert!(settlement_rest_minutes(&form("24.5", "hours", None)).is_err());
    }

    #[test]
    fn days_are_independent_whole_days_with_a_minimum_of_one() {
        assert_eq!(settlement_rest_minutes(&form("1", "days", None)), Ok(1_440));
        assert_eq!(
            settlement_rest_minutes(&form("2", "days", Some(1_441))),
            Ok(2_880)
        );
        assert!(settlement_rest_minutes(&form("0", "days", None)).is_err());
        assert!(settlement_rest_minutes(&form("1.5", "days", None)).is_err());
        assert_eq!(
            settlement_rest_minutes(&form("365", "days", None)),
            Ok(365 * 1_440)
        );
        assert!(settlement_rest_minutes(&form("366", "days", None)).is_err());
    }

    #[test]
    fn rest_spending_itemizes_full_board_and_other_downtime_costs() {
        assert_eq!(rest_spending_breakdown(4, true, 1_440), (2, 2));
        assert_eq!(rest_spending_breakdown(10, true, 2_880), (4, 6));
        assert_eq!(rest_spending_breakdown(2, false, 1_440), (0, 2));
    }

    #[test]
    fn days_form_omits_disabled_exact_minutes_and_hours_reject_contradictions() {
        let parsed: RestForm =
            serde_urlencoded::from_str("duration=2&unit=days").expect("days form parses");
        assert_eq!(parsed.requested_minutes, None);
        assert_eq!(settlement_rest_minutes(&parsed), Ok(2_880));
        let blank: RestForm = serde_urlencoded::from_str("duration=2&unit=days&requested_minutes=")
            .expect("blank disabled-field fallback parses");
        assert_eq!(blank.requested_minutes, None);
        assert_eq!(settlement_rest_minutes(&blank), Ok(2_880));
        assert!(settlement_rest_minutes(&form("24:00", "hours", Some(1_441))).is_err());
    }

    #[test]
    fn rest_failures_have_safe_visible_prose() {
        assert_eq!(
            safe_rest_error("Not enough coin to pay for the inn stay"),
            "You do not have enough coin for that inn stay."
        );
        assert!(!safe_rest_error("private injury authority 123").contains("123"));
    }

    #[test]
    fn rest_form_extraction_failures_are_logged_without_request_contents() {
        let source = SETTLEMENTS_SOURCE;
        let handler = source
            .split("async fn rest(")
            .nth(1)
            .and_then(|tail| tail.split("async fn query_single").next())
            .expect("settlement rest handler");
        let extraction = handler
            .split("let form = match form")
            .nth(1)
            .and_then(|tail| tail.split("let settlements").next())
            .expect("form extraction branch");
        assert!(extraction.contains("tracing::warn!("));
        for field in [
            "character_id",
            "requested_settlement_id_length = id.len()",
            "service = kind.as_str()",
            "rejection_status = %error.status()",
            "error = %error",
        ] {
            assert!(extraction.contains(field), "{field}");
        }
        assert!(extraction.contains("return error.into_response()"));
        assert!(!extraction.contains("requested_settlement_id = %id"));
        assert!(!extraction.contains("form.duration"));
        assert!(!extraction.contains("request body"));
        assert!(
            handler.find("let Some(character_id)") < handler.find("let form = match form"),
            "authentication precedes malformed-form warning"
        );
    }

    #[test]
    fn rest_duration_validation_logs_bounded_metadata_before_safe_notice() {
        let source = SETTLEMENTS_SOURCE;
        let handler = source
            .split("async fn rest(")
            .nth(1)
            .and_then(|tail| tail.split("async fn query_single").next())
            .expect("settlement rest handler");
        let validation = handler
            .split("let requested_minutes = match settlement_rest_minutes(&form)")
            .nth(1)
            .and_then(|tail| tail.split("let before_character").next())
            .expect("rest duration validation branch");
        let warning = validation.find("tracing::warn!(").expect("warning");
        let safe_notice = validation
            .find("strategic_notice_page(")
            .expect("safe notice");
        assert!(warning < safe_notice);
        for field in [
            "character_id",
            "requested_settlement_id = %id",
            "requested_minutes = ?form.requested_minutes",
            "at_inn",
            "service = kind.as_str()",
            "duration_length = form.duration.len()",
            "reason = message",
        ] {
            assert!(validation[..safe_notice].contains(field), "{field}");
        }
        for category in [
            "\"hours\" => \"hours\"",
            "\"days\" => \"days\"",
            "_ => \"unknown\"",
        ] {
            assert!(validation[..safe_notice].contains(category), "{category}");
        }
        assert!(!validation.contains("duration = %form.duration"));
        assert!(!validation.contains("unit = %form.unit"));
    }

    #[test]
    fn rest_reducer_rejections_are_logged_before_the_sanitized_notice() {
        let source = SETTLEMENTS_SOURCE;
        let handler = source
            .split("async fn rest(")
            .nth(1)
            .and_then(|tail| tail.split("async fn query_single").next())
            .expect("settlement rest handler");
        let reducer_error = handler
            .split("if let Err(error)")
            .nth(1)
            .expect("rest reducer error branch");
        let warning = reducer_error.find("tracing::warn!(").expect("warning");
        let sanitization = reducer_error
            .find("safe_rest_error(&error.to_string())")
            .expect("safe response");
        assert!(warning < sanitization);
        for field in [
            "character_id",
            "requested_settlement_id = %id",
            "character_settlement_id",
            "requested_minutes",
            "at_inn",
            "service = kind.as_str()",
            "error = %error",
        ] {
            assert!(reducer_error[..sanitization].contains(field), "{field}");
        }
        assert!(handler.contains("character.current_settlement_id.as_deref()"));
        assert!(handler.contains(".unwrap_or(\"<none>\")"));
        assert!(reducer_error.contains("settlement rest reducer rejected request"));
    }

    #[test]
    fn camp_wake_defaults_follow_the_absolute_daily_schedule() {
        assert_eq!(
            minutes_until_next_walking_start(60, 8 * 60, true),
            Some(19 * 60)
        );
        assert_eq!(
            minutes_until_next_walking_start(7 * 60, 8 * 60, false),
            Some(60)
        );
        assert!(!is_walking_time(7 * 60, 8 * 60, false));
        assert!(is_walking_time(9 * 60, 8 * 60, false));
        assert_eq!(
            minutes_until_next_walking_start(9 * 60, 8 * 60, false),
            Some(23 * 60)
        );
        assert_eq!(
            minutes_until_next_walking_start(18 * 60, 8 * 60, true),
            Some(2 * 60)
        );
        assert!(is_walking_time(21 * 60, 8 * 60, true));
    }

    #[test]
    fn unresolved_encounters_override_walking_time_for_camp_continuation() {
        assert_eq!(
            camp_continue_block_reason(Some("awaiting_choice"), true),
            Some("Resolve the encounter above before continuing travel.")
        );
        assert_eq!(camp_continue_block_reason(Some("resolved"), true), None);
        assert_eq!(camp_continue_block_reason(None, true), None);
        assert_eq!(
            camp_continue_block_reason(None, false),
            Some("Rest until the planned walking window begins.")
        );
    }
}
