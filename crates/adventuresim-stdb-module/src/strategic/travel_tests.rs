#[cfg(test)]
mod departure_invariant_tests {
    use super::{
        CampDurationMode, CaseSiteId, JourneyCaseSiteEndpoint, JourneyEndpoint,
        JourneyPrecipitation, JourneyRoutePlan, JourneyRoutePoint, JourneySettlementEndpoint,
        JourneyTerrainKind, JourneyTerrainSpan, JourneyTerrainWeights, Party, PartyJourneyRoute,
        common_movement_prefix, core_encounter_terrain, departure_requires_ready_party,
        departure_snapshot_allows_travel, party_can_continue_travel,
        pending_incident_allows_departure, reconstruct_legacy_journey_coordinates,
        route_position_at_minute, set_party_journey_state, straight_line_distance_m,
        authoritative_straight_line_case_route,
        terrain_training_exposure, validate_camp_redirect_weather_interval,
        validate_journey_route_payload, validate_route_departure_weather_interval,
        zero_boundary_requires_settlement,
    };

    fn endpoint_name(endpoint: &JourneyEndpoint) -> &str {
        match endpoint {
            JourneyEndpoint::Settlement(endpoint) => &endpoint.name,
            JourneyEndpoint::CaseSite(endpoint) => &endpoint.name,
            JourneyEndpoint::Camp(name) => name,
        }
    }

    #[test]
    fn departure_requires_unchanged_party_members_and_incident_snapshot() {
        assert!(!departure_snapshot_allows_travel(true, true, false));
        assert!(!departure_snapshot_allows_travel(false, true, true));
        assert!(!departure_snapshot_allows_travel(true, false, true));
        assert!(departure_snapshot_allows_travel(true, true, true));
    }

    #[test]
    fn only_case_site_withdrawal_may_bypass_departure_readiness() {
        assert!(!departure_requires_ready_party(None, Some("site:a"), true));
        assert!(departure_requires_ready_party(
            Some("settlement:a"),
            None,
            true
        ));
        assert!(departure_requires_ready_party(None, None, true));
        assert!(departure_requires_ready_party(None, Some("site:a"), false));
    }

    #[test]
    fn settlement_travel_requests_the_bypass_only_for_case_site_origins() {
        let source = crate::strategic::STRATEGIC_SOURCE;
        let travel = source
            .split("fn travel_to_settlement_impl")
            .nth(1)
            .and_then(|tail| tail.split("pub fn set_party_camp_fatigue_percent").next())
            .expect("settlement travel implementation");
        assert!(travel.contains("(origin_kind == \"case_site\").then_some(origin_id.as_str())"));
        assert!(travel.contains("origin_kind == \"case_site\","));
        assert!(travel.contains("require_party_ready(ctx, &party.id)?"));
    }

    #[test]
    fn only_the_exact_departing_incident_site_may_be_avoided() {
        assert!(pending_incident_allows_departure(None, std::iter::empty()));
        assert!(pending_incident_allows_departure(
            Some("site:a"),
            ["site:a"].into_iter()
        ));
        assert!(!pending_incident_allows_departure(
            Some("site:a"),
            ["site:b"].into_iter()
        ));
        assert!(!pending_incident_allows_departure(
            None,
            ["site:a"].into_iter()
        ));
        assert!(!pending_incident_allows_departure(
            Some("site:a"),
            ["site:a", "site:b"].into_iter()
        ));
    }

    #[test]
    fn legacy_journey_never_falls_back_to_day_one() {
        assert_eq!(
            reconstruct_legacy_journey_coordinates(20_000, 600),
            (19_400, 600)
        );
        assert_eq!(reconstruct_legacy_journey_coordinates(300, 600), (0, 600));
    }

    #[test]
    fn unplanned_case_route_persists_coherent_disclosed_straight_line_geometry() {
        let route = authoritative_straight_line_case_route(
            332_661,
            (10.0, 53.0),
            (10.01, 53.01),
            1_300,
            63,
        );
        assert_eq!(route.distance_m, 1_300);
        assert_eq!(route.minutes, 63);
        assert_eq!(route.package_digest.len(), 64);
        assert!(route.package_digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(route.points.len(), 2);
        assert_eq!(route.points[0].longitude_e7, 100_000_000);
        assert_eq!(route.points[0].latitude_e7, 530_000_000);
        assert_eq!(route.points[1].longitude_e7, 100_100_000);
        assert_eq!(route.points[1].latitude_e7, 530_100_000);
        assert_eq!(route.spans[0].duration_minutes, route.minutes);
        let return_route = route.return_route.unwrap();
        assert_eq!(return_route.points[0].longitude_e7, 100_100_000);
        assert_eq!(return_route.points[1].longitude_e7, 100_000_000);
    }

    #[test]
    fn journey_refresh_keeps_case_site_forecast_to_the_active_leg() {
        let source = include_str!("journey_camp.rs");
        let refresh = source
            .split("pub(crate) fn refresh_party_journey_forecast")
            .nth(1)
            .expect("journey refresh");
        assert!(refresh.contains("let planned_movement = journey.total_minutes"));
        assert!(!refresh.contains("journey.total_minutes.saturating_mul(2)"));
    }

    fn route_fixture() -> JourneyRoutePlan {
        let origin = (10.0, 53.0);
        let destination = (10.01, 53.0);
        JourneyRoutePlan {
            package_digest: "a".repeat(64),
            weather_rules_version: adventuresim_core::weather::WEATHER_RULES_VERSION,
            weather_interval_start: 0,
            precipitation: JourneyPrecipitation::Clear,
            intensity_bps: 0,
            ground_moisture_bps: 0,
            snow_cover_bps: 0,
            distance_m: straight_line_distance_m(
                origin.0,
                origin.1,
                destination.0,
                destination.1,
                true,
            ),
            minutes: 12,
            points: vec![
                JourneyRoutePoint {
                    latitude_e7: 530_000_000,
                    longitude_e7: 100_000_000,
                },
                JourneyRoutePoint {
                    latitude_e7: 530_000_000,
                    longitude_e7: 100_100_000,
                },
            ],
            spans: vec![
                JourneyTerrainSpan {
                    kind: JourneyTerrainKind::Road,
                    terrain: JourneyTerrainWeights {
                        plains: 1_000,
                        forest: 0,
                        hills: 0,
                        wetlands: 0,
                        urban: 0,
                    },
                    training_multiplier_permille: 250,
                    check_millirank: 0,
                    start_minute: 0,
                    duration_minutes: 5,
                },
                JourneyTerrainSpan {
                    kind: JourneyTerrainKind::Open,
                    terrain: JourneyTerrainWeights {
                        plains: 1_000,
                        forest: 0,
                        hills: 0,
                        wetlands: 0,
                        urban: 0,
                    },
                    training_multiplier_permille: 1_000,
                    check_millirank: 0,
                    start_minute: 5,
                    duration_minutes: 7,
                },
            ],
            return_route: None,
        }
    }

    #[test]
    fn planned_route_validation_binds_endpoints_geometry_and_exact_minutes() {
        let route = route_fixture();
        assert!(validate_journey_route_payload(&route, (10.0, 53.0), (10.01, 53.0)).is_ok());

        let mut bad = route.clone();
        bad.points[0].longitude_e7 += 1_000_000;
        assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());

        let mut bad = route.clone();
        bad.distance_m *= 2;
        assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());

        let mut bad = route.clone();
        bad.spans[1].start_minute = 6;
        assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());

        let mut bad = route.clone();
        bad.spans[0].terrain.plains = 999;
        assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());

        let mut bad = route.clone();
        bad.weather_rules_version += 1;
        assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());

        let mut bad = route.clone();
        bad.precipitation = JourneyPrecipitation::Clear;
        bad.intensity_bps = 1;
        assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());

        for index in 0..2 {
            let mut bad = route.clone();
            bad.spans[index].terrain.plains -= 1;
            bad.spans[index].terrain.urban = 1;
            assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());
        }

        let mut bad = route.clone();
        bad.minutes = 1;
        bad.spans = vec![JourneyTerrainSpan {
            kind: JourneyTerrainKind::Road,
            terrain: JourneyTerrainWeights {
                plains: 1_000,
                forest: 0,
                hills: 0,
                wetlands: 0,
                urban: 0,
            },
            training_multiplier_permille: 250,
            check_millirank: 0,
            start_minute: 0,
            duration_minutes: 1,
        }];
        assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());

        let mut bad = route;
        bad.points[0].latitude_e7 = i32::MAX;
        assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());
    }

    #[test]
    fn departure_weather_interval_closes_clock_sync_boundary() {
        let mut route = route_fixture();
        route.weather_interval_start = 0;
        assert!(validate_route_departure_weather_interval(&route, 359).is_ok());
        assert!(validate_route_departure_weather_interval(&route, 360).is_err());
        route.weather_interval_start = 360;
        assert!(validate_route_departure_weather_interval(&route, 360).is_ok());
    }

    #[test]
    fn camp_redirect_rejects_stale_six_hour_weather_snapshot() {
        let mut route = route_fixture();
        route.weather_interval_start = 360;
        assert!(validate_camp_redirect_weather_interval(&route, 719).is_ok());
        assert!(validate_camp_redirect_weather_interval(&route, 720).is_err());
        route.weather_interval_start = 720;
        assert!(validate_camp_redirect_weather_interval(&route, 720).is_ok());
    }

    #[test]
    fn max_rank_seventy_five_hundred_metres_per_hour_route_is_accepted() {
        let mut route = route_fixture();
        route.minutes = route.distance_m.saturating_mul(60).div_ceil(7_500);
        route.spans[0].duration_minutes = 2;
        route.spans[1].start_minute = 2;
        route.spans[1].duration_minutes = route.minutes - 2;
        for span in &mut route.spans {
            span.check_millirank = 5_000;
        }
        assert_eq!(route.minutes, 6);
        assert!(validate_journey_route_payload(&route, (10.0, 53.0), (10.01, 53.0)).is_ok());
    }

    #[test]
    fn terrain_training_uses_exact_overlap_and_conserves_mixed_exposure() {
        let spans = route_fixture().spans;
        let exposure = terrain_training_exposure(&spans, 3, 9, 0);
        // Two road minutes at 25%, then four open minutes at full exposure.
        assert!((exposure[0] - 4.5 / 60.0).abs() < 0.0001);
        assert_eq!(exposure[1..], [0.0, 0.0, 0.0, 0.0, 0.0]);
        let none = terrain_training_exposure(&spans, 12, 30, 0);
        assert_eq!(none, [0.0; 6]);
    }

    #[test]
    fn persisted_wetland_weight_produces_wetland_exposure() {
        let mut span = route_fixture().spans.remove(0);
        span.duration_minutes = 60;
        span.training_multiplier_permille = 100;
        span.terrain = JourneyTerrainWeights {
            plains: 0,
            forest: 0,
            hills: 0,
            wetlands: 1_000,
            urban: 0,
        };
        let exposure = terrain_training_exposure(&[span], 0, 60, 0);
        assert_eq!(exposure, [0.0, 0.0, 0.0, 0.1, 0.0, 0.0]);
    }

    #[test]
    fn wetland_journey_kind_uses_existing_open_encounter_class() {
        assert_eq!(
            core_encounter_terrain(JourneyTerrainKind::Wetland),
            adventuresim_core::encounter::EncounterTerrain::Open
        );
    }

    #[test]
    fn terminal_prefix_and_retry_train_each_committed_minute_once() {
        let spans = route_fixture().spans;
        let first = common_movement_prefix(12, [12, 4]);
        assert_eq!(
            first, 4,
            "the earliest death boundary limits the whole party"
        );
        let retry = common_movement_prefix(12 - first, [8]);
        let chunked = terrain_training_exposure(&spans, 0, first, 8_000);
        let resumed = terrain_training_exposure(&spans, first, first + retry, 8_000);
        let whole = terrain_training_exposure(&spans, 0, 12, 8_000);
        for index in 0..6 {
            assert!((chunked[index] + resumed[index] - whole[index]).abs() < 0.0001);
        }
        assert!(whole[5] > 0.0, "snow supplements underlying exposure");
        assert!(
            (whole.into_iter().sum::<f32>() - 8.25 / 60.0).abs() < 0.0001,
            "snow splits rather than duplicates the road-discounted budget"
        );
    }

    #[test]
    fn zero_minute_terminal_is_settled_before_survivors_retry() {
        let first_prefixes = [12, 0];
        let first = common_movement_prefix(12, first_prefixes);
        assert_eq!(first, 0);
        assert!(!zero_boundary_requires_settlement(first, first_prefixes[0]));
        assert!(zero_boundary_requires_settlement(first, first_prefixes[1]));

        // Once the terminal member has been authoritatively removed from the
        // living traveler list, the survivor's retry advances normally and no
        // zero-boundary settlement is repeated.
        let retry_prefixes = [12];
        let retry = common_movement_prefix(12, retry_prefixes);
        assert_eq!(retry, 12);
        assert!(!zero_boundary_requires_settlement(retry, retry_prefixes[0]));
    }

    #[test]
    fn journey_state_update_preserves_elected_successor_authority() {
        let mut fresh_party = Party {
            id: "party".into(),
            gateway_bucket: 0,
            name: "Travelers".into(),
            leader_id: 2,
            current_settlement_id: None,
            current_case_site_id: None,
            active_contract_id: None,
            is_solo: true,
            camp_fatigue_percent: 50,
            walking_minutes_per_day: 480,
            travel_at_night: false,
            camp_duration_mode: CampDurationMode::Auto,
            fixed_camp_minutes: 0,
            camp_destination: Some(JourneyEndpoint::Settlement(JourneySettlementEndpoint {
                id: "destination".into(),
                name: "Destination".into(),
            })),
            camp_remaining_minutes: 30,
            pooled_water_ml: 0.0,
            physiology_target: 0.0,
            command_target: 0.0,
            religion_target: 0.0,
        };
        let settlement_destination = JourneyEndpoint::Settlement(JourneySettlementEndpoint {
            id: "destination".into(),
            name: "Distant town".into(),
        });
        set_party_journey_state(
            &mut fresh_party,
            None,
            None,
            Some(settlement_destination),
            30,
        );
        assert_eq!(
            fresh_party.camp_destination.as_ref().map(endpoint_name),
            Some("Distant town")
        );
        let case_site_destination = JourneyEndpoint::CaseSite(JourneyCaseSiteEndpoint {
            id: CaseSiteId::from("site:known".to_string()),
            name: "a camp in the woods".into(),
        });
        set_party_journey_state(
            &mut fresh_party,
            None,
            None,
            Some(case_site_destination),
            30,
        );
        assert_eq!(
            fresh_party.camp_destination.as_ref().map(endpoint_name),
            Some("a camp in the woods")
        );
        set_party_journey_state(&mut fresh_party, Some("destination".into()), None, None, 0);
        assert_eq!(fresh_party.leader_id, 2);
        assert_eq!(
            fresh_party.current_settlement_id.as_deref(),
            Some("destination")
        );
        assert!(fresh_party.camp_destination.is_none());
        assert_eq!(
            fresh_party.leader_id, 2,
            "the successor can continue leading"
        );
        assert!(party_can_continue_travel(&fresh_party, 2));
        assert!(!party_can_continue_travel(&fresh_party, 1));
    }

    #[test]
    fn camp_origin_is_interpolated_from_persisted_route_progress() {
        let route = route_fixture();
        let persisted = PartyJourneyRoute {
            party_id: "party".into(),
            gateway_bucket: 0,
            package_digest: route.package_digest,
            weather_rules_version: route.weather_rules_version,
            weather_interval_start: route.weather_interval_start,
            precipitation: route.precipitation,
            intensity_bps: route.intensity_bps,
            ground_moisture_bps: route.ground_moisture_bps,
            snow_cover_bps: route.snow_cover_bps,
            distance_m: route.distance_m,
            minutes: route.minutes,
            points: route.points,
            spans: route.spans,
            return_route: route.return_route,
        };
        let midpoint = route_position_at_minute(&persisted, persisted.minutes / 2).unwrap();
        assert!((midpoint.0 - 10.005).abs() < 0.000_1);
        assert!((midpoint.1 - 53.0).abs() < 0.000_1);
    }

    #[test]
    fn terminal_departure_sync_commits_and_stops_before_creating_a_journey() {
        let source = include_str!("travel_reducers.rs");
        assert_eq!(
            source.matches("let Some(departure_minute) = crate::time::synchronize_party_departure_time").count(),
            2,
        );
        assert_eq!(source.matches("else {\n        return Ok(());\n    };").count(), 2);
    }
}
