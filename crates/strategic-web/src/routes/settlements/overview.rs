#[derive(Default, Deserialize)]
pub(super) struct LocationMapQuery {
    destination: Option<String>,
}

pub(super) async fn settlement_map(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<LocationMapQuery>,
    session: Session,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query("SELECT * FROM settlement")
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.iter().find(|settlement| settlement.id == id) else {
        return Html("<h1>Settlement not found</h1>".to_string());
    };
    let edges: Vec<TravelEdge> = state
        .db
        .query("SELECT * FROM travel_edge")
        .await
        .unwrap_or_default();
    let map_data_initialized = crate::strategic_map::has_geographic_source(settlement);
    let mut destinations = if map_data_initialized {
        connected_destinations(settlement, &settlements, &edges)
    } else {
        Vec::new()
    };
    let quests: Vec<ContractPresentation> = state
        .db
        .query("SELECT * FROM backend_contracts")
        .await
        .unwrap_or_default();
    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let active_party = if let Some(party_id) = active_character
        .as_ref()
        .and_then(|(character, _)| character.party_id.as_ref())
    {
        state
            .db
            .query::<Party>(&crate::spacetimedb::party_by_id(party_id))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
    } else {
        None
    };
    let active_contract_id = active_party
        .as_ref()
        .and_then(|party| party.active_contract_id.as_deref());
    let active_contract =
        active_contract_id.and_then(|id| quests.iter().find(|contract| contract.id == id));
    let case_sites = if let Some(character_id) = session.character_id_u64() {
        state
            .db
            .query::<BackendCaseSitePin>(&format!(
                "SELECT * FROM backend_case_site_pins WHERE owner_character_id = {character_id}"
            ))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let is_current_settlement = active_character.as_ref().is_some_and(|(character, _)| {
        character.current_settlement_id.as_deref() == Some(&settlement.id)
    });
    // The interactive map bundle is optional presentation. Exact observer-owned
    // case sites must remain selectable and travelable through the HTML fallback.
    let can_travel =
        settlement_html_travel_available(is_current_settlement, active_party.is_some());
    if can_travel {
        for site in &case_sites {
            let distance_m = crate::routes::quests::straight_line_distance_m(site, settlement);
            destinations.push(TravelDestination {
                id: site.case_site_id.clone(),
                name: site.name.clone(),
                description: site.description.clone(),
                summary: CaseSiteKnowledgePresentation::from_stage(site.knowledge_stage)
                    .map(|knowledge| knowledge.label().to_string()),
                travel_action: format!("/case-sites/{}/travel", site.case_site_id),
                track_action: Some(format!("/case-sites/{}/track", site.case_site_id)),
                tracked: site.tracked,
                distance_m,
                journey_minutes: crate::routes::quests::offroad_journey_minutes(distance_m),
                camp_stop_minutes: Vec::new(),
                camp_forecasts: Vec::new(),
                departure_minute: 0,
                itinerary_total_elapsed_minutes: crate::routes::quests::offroad_journey_minutes(
                    distance_m,
                )
                .saturating_mul(2),
                itinerary_segments: Vec::new(),
                round_trip_destination: true,
                case_site_knowledge: CaseSiteKnowledgePresentation::from_stage(
                    site.knowledge_stage,
                ),
                active_contract_destination: case_site_has_active_contract(
                    &site.case_id,
                    active_contract,
                ),
                provision_forecast: None,
                terrain_route: None,
                return_terrain_route: None,
                uses_straight_line_estimate: true,
            });
        }
    }
    if let Some(selected_id) = query.destination.as_deref()
        && let Some(destination) = destinations
            .iter_mut()
            .find(|destination| destination.id == selected_id)
    {
        let goal = if let Some(site) = case_sites
            .iter()
            .find(|site| site.case_site_id == destination.id)
        {
            Some((
                f64::from(site.latitude_e7) / 10_000_000.0,
                f64::from(site.longitude_e7) / 10_000_000.0,
            ))
        } else {
            settlements
                .iter()
                .find(|candidate| candidate.id == destination.id)
                .map(|candidate| (candidate.coord_y, candidate.coord_x))
        };
        if let Some(goal) = goal {
            let terrain_profile = if let Some((character, _)) = active_character.as_ref() {
                crate::routes::party_terrain_profile(&state, character)
                    .await
                    .unwrap_or_default()
                    .0
            } else {
                adventuresim_terrain::TerrainSkillProfile::default()
            };
            crate::routes::travel::apply_terrain_route(
                destination,
                state.terrain.as_deref(),
                (settlement.coord_y, settlement.coord_x),
                goal,
                terrain_profile,
            )
            .await;
        }
    }
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let living_party_members = living_party_members(&party_members);
    let stats: Vec<CharacterStats> = state
        .db
        .query("SELECT * FROM backend_character_stats")
        .await
        .unwrap_or_default();
    let default_rest_minutes = living_party_members
        .iter()
        .filter_map(|member| stats.iter().find(|row| row.character_id == member.id))
        .map(|row| {
            (row.calories_used.max(0.0) / STRATEGIC_TRAVEL_KCAL_PER_DAY
                * adventuresim_core::strategic_time::MINUTES_PER_DAY as f32)
                .ceil() as u64
        })
        .max()
        .unwrap_or(0)
        .max(1);
    if can_travel && let Some(party) = active_party.as_ref() {
        let attributes: Vec<CharacterAttributes> = state
            .db
            .query("SELECT * FROM backend_character_attributes")
            .await
            .unwrap_or_default();
        let limbs: Vec<CharacterLimbs> = state
            .db
            .query("SELECT * FROM backend_character_limbs")
            .await
            .unwrap_or_default();
        let times: Vec<CharacterTime> = state
            .db
            .query("SELECT * FROM backend_character_times")
            .await
            .unwrap_or_default();
        let schedules: Vec<CharacterTrainingSchedule> = state
            .db
            .query("SELECT * FROM backend_character_training_schedules")
            .await
            .unwrap_or_default();
        let member_ids: Vec<_> = living_party_members
            .iter()
            .map(|member| member.id)
            .collect();
        populate_itinerary_forecasts(
            &mut destinations,
            ItineraryForecastSources {
                party_members: &member_ids,
                attributes: &attributes,
                limbs: &limbs,
                stats: &stats,
                times: &times,
                schedules: &schedules,
                party,
            },
        );
    }
    if can_travel {
        for destination in &mut destinations {
            destination.provision_forecast = travel_provision_forecast(
                &state,
                active_party.as_ref(),
                &living_party_members,
                destination,
                true,
            )
            .await
            .ok()
            .flatten();
        }
    }
    let provision_forecast = query
        .destination
        .as_deref()
        .and_then(|id| destinations.iter().find(|destination| destination.id == id))
        .and_then(|destination| destination.provision_forecast.as_ref());
    let soap_preview = soap_rest_preview(
        &state,
        &party_members,
        active_party.as_ref().map(|party| party.id.as_str()),
    )
    .await;
    let provisioning_path = if can_travel {
        provisioning_storefront_path(&state, settlement).await
    } else {
        None
    };
    Html(
        settlement_map_page(
            settlement,
            &settlements,
            &case_sites,
            state.strategic_map.as_deref(),
            &destinations,
            query.destination.as_deref(),
            active_character.as_ref().map(|(character, _)| character),
            active_party.as_ref(),
            &party_members,
            default_rest_minutes,
            soap_preview,
            can_travel,
            provision_forecast,
            provisioning_path.as_deref(),
            is_current_settlement,
            active_contract.filter(|contract| {
                can_abandon_active_contract(
                    contract,
                    active_character
                        .as_ref()
                        .and_then(|(character, _)| character.current_case_site_id.as_deref()),
                )
            }),
            active_character
                .as_ref()
                .map(|(character, _)| character.name.as_str()),
        )
        .into_string(),
    )
}

pub(super) fn settlement_html_travel_available(
    is_current_settlement: bool,
    has_party: bool,
) -> bool {
    is_current_settlement && has_party
}

pub(super) fn case_site_has_active_contract(
    case_id: &str,
    active_contract: Option<&ContractPresentation>,
) -> bool {
    active_contract.is_some_and(|contract| contract.case_id == case_id)
}

pub(super) fn can_abandon_active_contract(
    contract: &ContractPresentation,
    current_case_site_id: Option<&str>,
) -> bool {
    contract.status == ContractStatus::Accepted && current_case_site_id.is_none()
}

#[cfg(test)]
mod map_quest_tests {
    use super::*;

    #[test]
    fn exact_owned_case_sites_use_the_current_settlement_as_the_map_origin() {
        let source = SETTLEMENTS_SOURCE;
        let map = source
            .split("async fn settlement_map(")
            .nth(1)
            .and_then(|tail| tail.split("fn settlement_html_travel_available").next())
            .expect("settlement map route");

        assert!(map.contains("for site in &case_sites"));
        assert!(map.contains("straight_line_distance_m(site, settlement)"));
        assert!(!map.contains("site.origin_settlement_id == settlement.id"));
    }

    #[test]
    fn html_case_site_travel_does_not_depend_on_optional_map_data() {
        assert!(settlement_html_travel_available(true, true));
        assert!(!settlement_html_travel_available(false, true));
        assert!(!settlement_html_travel_available(true, false));
    }

    fn quest(status: ContractStatus) -> ContractPresentation {
        ContractPresentation {
            id: "active".into(),
            case_id: "case:active".into(),
            title: "Active quest".into(),
            description: String::new(),
            difficulty: 1,
            gold_reward: 1,
            xp_reward: 1,
            settlement_id: "issuer".into(),
            service_id: "inn".into(),
            issuer_resident_character_id: String::new(),
            status,
            accepted_by: Some("party".into()),
            opposition_wording: "unknown opposition".into(),
            opposition_count_wording: "an unknown number of".into(),
        }
    }

    #[test]
    fn accepted_active_quest_can_only_be_abandoned_before_reaching_its_location() {
        assert!(can_abandon_active_contract(
            &quest(ContractStatus::Accepted),
            None
        ));
        assert!(!can_abandon_active_contract(
            &quest(ContractStatus::Accepted),
            Some("active")
        ));
        assert!(!can_abandon_active_contract(
            &quest(ContractStatus::ReadyToReport),
            None
        ));
    }

    #[test]
    fn case_site_badge_requires_an_explicit_active_contract_case_match() {
        let active = quest(ContractStatus::Accepted);

        assert!(case_site_has_active_contract("case:active", Some(&active)));
        assert!(!case_site_has_active_contract(
            "case:reported-decoy",
            Some(&active)
        ));
        assert!(!case_site_has_active_contract("case:active", None));
    }
}
