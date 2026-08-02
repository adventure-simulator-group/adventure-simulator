#[derive(Deserialize)]
pub(super) struct TravelConfigurationForm {
    walking_hours: f32,
    #[serde(default)]
    travel_at_night: bool,
}

pub(super) async fn update_travel_configuration(
    State(state): State<AppState>,
    Path(_id): Path<String>,
    session: Session,
    Form(form): Form<TravelConfigurationForm>,
) -> Response {
    save_travel_configuration(&state, &session, form).await
}

pub(super) async fn update_camp_travel_configuration(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<TravelConfigurationForm>,
) -> Response {
    save_travel_configuration(&state, &session, form).await
}

pub(super) async fn save_travel_configuration(
    state: &AppState,
    session: &Session,
    form: TravelConfigurationForm,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let walking_minutes = (form.walking_hours.clamp(0.0, 24.0) * 60.0).round() as u16;
    match state
        .db
        .call(
            "set_party_travel_itinerary",
            &[
                json!(character_id),
                json!(walking_minutes),
                json!(form.travel_at_night),
                json!(false),
                json!((24 * 60_u16).saturating_sub(walking_minutes)),
            ],
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Default, Deserialize)]
pub(super) struct CampQuery {
    forage: Option<bool>,
    forage_receipt: Option<String>,
    forage_error: Option<String>,
    road_occurrence: Option<String>,
}

pub(super) async fn camp(
    State(state): State<AppState>,
    Query(query): Query<CampQuery>,
    session: Session,
) -> Response {
    let Some((character, _inventory)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Redirect::to("/characters").into_response();
    };
    let Some(party_id) = character.party_id.as_deref() else {
        return Redirect::to("/").into_response();
    };
    // A reducer response can arrive a fraction before its row is visible to
    // the SQL endpoint. Retry briefly so a completed travel POST resolves to
    // camp rather than falling through to the character picker.
    let mut party = None;
    for attempt in 0..4 {
        party = state
            .db
            .query_one::<Party>(&format!(
                "SELECT * FROM party WHERE id = {}",
                sql_string_literal(party_id)
            ))
            .await
            .ok()
            .flatten();
        if party
            .as_ref()
            .is_some_and(|party| party.camp_destination.is_some())
        {
            break;
        }
        if attempt < 3 {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    }
    let Some(party) = party else {
        return Redirect::to("/").into_response();
    };
    if camp_entry_redirect(true, party.camp_destination.is_some()).is_some() {
        return Redirect::to("/").into_response();
    }
    let Some(destination) = party.camp_destination.as_ref() else {
        return Redirect::to("/").into_response();
    };
    let destination_name = destination.name().to_string();
    // The party and journey rows are committed atomically, but the SQL view
    // can observe the camp row a fraction before the journey projection.
    // Retry briefly so the first camp render retains the original start.
    let mut journey = None;
    for attempt in 0..4 {
        journey = state
            .db
            .query_one::<PartyJourney>(&format!(
                "SELECT * FROM party_journey WHERE party_id = {}",
                sql_string_literal(&party.id)
            ))
            .await
            .ok()
            .flatten();
        if journey.is_some() || attempt == 3 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    let party_members = get_active_party_members(&state, Some(&character)).await;
    let member_times: Vec<CharacterTime> = state
        .db
        .query("SELECT * FROM backend_character_times")
        .await
        .unwrap_or_default();
    let current_party_minute = party_members
        .iter()
        .filter_map(|member| {
            member_times
                .iter()
                .find(|time| time.character_id == member.id)
        })
        .map(|time| time.minutes)
        .max()
        .unwrap_or(0);
    if let Some(legacy) = journey.as_mut().filter(|journey| journey.plan_version == 0) {
        legacy.completed_elapsed_minutes = legacy.completed_minutes;
        legacy.departure_minute =
            current_party_minute.saturating_sub(legacy.completed_elapsed_minutes);
        legacy.total_elapsed_minutes = if legacy.destination.case_site_id().is_some() {
            legacy.total_minutes.saturating_mul(2)
        } else {
            legacy.total_minutes
        };
    }
    let direct_demo_contract_prefix = format!("contract:errantry-puzzle:demo:{}:", character.id);
    let expects_direct_demo = party
        .active_contract_id
        .as_deref()
        .is_some_and(|id| id.starts_with(&direct_demo_contract_prefix));
    let mut challenges = Vec::new();
    for attempt in 0..4 {
        match state
            .db
            .query::<BackendChallenge>(&format!(
                "SELECT * FROM backend_challenges WHERE owner_character_id = {}",
                character.id
            ))
            .await
        {
            Ok(rows) => challenges = rows,
            Err(error) => tracing::warn!(
                %error,
                character_id = character.id,
                "camp challenge state unavailable"
            ),
        }
        if !expects_direct_demo
            || challenges
                .iter()
                .any(|challenge| is_direct_demo_challenge_id(&challenge.id, character.id))
            || attempt == 3
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    if let Some(path) = direct_demo_challenge_redirect(&challenges, character.id) {
        return Redirect::to(&path).into_response();
    }
    let itinerary = state
        .db
        .query_one::<PartyJourneyItinerary>(&format!(
            "SELECT * FROM party_journey_itinerary WHERE party_id = {}",
            sql_string_literal(&party.id)
        ))
        .await
        .ok()
        .flatten();
    let terrain_route = state
        .db
        .query_one::<PartyJourneyRoute>(&format!(
            "SELECT * FROM party_journey_route WHERE party_id = {}",
            sql_string_literal(&party.id)
        ))
        .await
        .ok()
        .flatten();
    let encounter = match state
        .db
        .query_one::<StrategicEncounter>(&format!(
            "SELECT * FROM strategic_encounter WHERE party_id = {}",
            sql_string_literal(&party.id)
        ))
        .await
    {
        Ok(encounter) => encounter,
        Err(error) => {
            tracing::warn!(
                %error,
                party_id = %party.id,
                "camp encounter state unavailable; refusing to render travel controls"
            );
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Encounter details are temporarily unavailable. Reload camp before continuing travel.",
            )
                .into_response();
        }
    };
    let mut counterparties = Vec::new();
    let mut dispositions:Vec<BackendContextDisposition>=Vec::new();
    if let Some(encounter)=encounter.as_ref(){dispositions=state.db.query(&format!("SELECT * FROM backend_context_dispositions WHERE observer_party_id = {} AND contact_ref = {}",sql_string_literal(&party.id),sql_string_literal(&encounter.encounter_id))).await.unwrap_or_default();}
    if let Some(encounter) = encounter.as_ref().filter(|row| row.status == "awaiting_choice") {
        let memberships: Vec<BackendContextCharacter> = state
            .db
            .query(&format!(
                "SELECT * FROM backend_context_characters WHERE contact_ref = {} AND party_id = {}",
                sql_string_literal(&encounter.encounter_id),
                sql_string_literal(&party.id)
            ))
            .await
            .unwrap_or_default();
        for membership in memberships.into_iter().filter(|row| row.alive) {
            if let Ok(Some(character)) = state
                .db
                .query_one::<Character>(&format!(
                    "SELECT * FROM backend_characters WHERE id = {}",
                    membership.character_id
                ))
                .await
            {
                counterparties.push(character);
            }
        }
    }
    let stats: Vec<CharacterStats> = state
        .db
        .query("SELECT * FROM backend_character_stats")
        .await
        .unwrap_or_default();
    let fatigue_rest_minutes = party_members
        .iter()
        .filter_map(|member| stats.iter().find(|stat| stat.character_id == member.id))
        .map(|stat| ((stat.calories_used / STRATEGIC_TRAVEL_KCAL_PER_DAY) * 1_440.0).ceil() as u64)
        .max()
        .unwrap_or(0);
    let default_rest_minutes = minutes_until_next_walking_start(
        current_party_minute,
        party.walking_minutes_per_day,
        party.travel_at_night,
    )
    .unwrap_or(fatigue_rest_minutes)
    .max(1);
    let planned_wake_minute =
        (current_party_minute.saturating_add(default_rest_minutes) % 1_440) as u16;
    let continue_block_reason = camp_continue_block_reason(
        encounter
            .as_ref()
            .map(|encounter| encounter.status.as_str()),
        is_walking_time(
            current_party_minute,
            party.walking_minutes_per_day,
            party.travel_at_night,
        ),
    );
    let remaining_journey_minutes = journey
        .as_ref()
        .map_or(party.camp_remaining_minutes, |row| {
            row.total_elapsed_minutes
                .saturating_sub(row.completed_elapsed_minutes)
        });
    let remaining_rest_intervals: Vec<_> = journey
        .as_ref()
        .zip(itinerary.as_ref())
        .into_iter()
        .flat_map(|(journey, itinerary)| {
            let remaining_start = journey.completed_elapsed_minutes;
            let remaining_end = journey.total_elapsed_minutes;
            itinerary
                .forecast_camp_intervals
                .iter()
                .filter_map(move |camp| {
                    let camp_start = camp.elapsed_start_minute.max(remaining_start);
                    let camp_end = camp
                        .elapsed_start_minute
                        .saturating_add(camp.elapsed_minutes)
                        .min(remaining_end);
                    (camp_end > camp_start).then(|| {
                        (
                            journey.departure_minute.saturating_add(camp_start),
                            camp_end - camp_start,
                        )
                    })
                })
        })
        .collect();
    let provision_forecast = travel_provision_forecast_for_minutes(
        &state,
        Some(&party),
        &party_members,
        remaining_journey_minutes,
        &remaining_rest_intervals,
        false,
    )
    .await
    .ok()
    .flatten();
    let camp_destinations = camp_settlement_destinations(&state, &party, journey.as_ref()).await;
    let soap_preview = soap_rest_preview(&state, &party_members, Some(&party.id)).await;
    let trial = challenges
        .iter()
        .find(|challenge| challenge.active && challenge.open && !challenge.solved);
    let tactical_insight = challenges.iter().find_map(|challenge| {
        (challenge.active && challenge.solved)
            .then(|| {
                Some((
                    challenge.tactical_insight_text.as_deref()?,
                    challenge.tactical_preparation_text.as_deref()?,
                ))
            })
            .flatten()
    });
    let road_challenges = match state
        .db
        .query::<BackendRoadChallenge>(&format!(
            "SELECT * FROM backend_road_challenges WHERE owner_character_id = {}",
            character.id
        ))
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(
                %error,
                character_id = character.id,
                "camp road challenge state unavailable"
            );
            Vec::new()
        }
    };
    let active_road_trial = road_challenges
        .iter()
        .find(|challenge| challenge.active && challenge.open);
    let mut road_history = road_challenges
        .iter()
        .filter(|challenge| !challenge.open)
        .collect::<Vec<_>>();
    road_history.sort_by(|left, right| {
        right
            .absolute_minute
            .cmp(&left.absolute_minute)
            .then_with(|| right.id.cmp(&left.id))
    });
    if let Some(requested) = query.road_occurrence.as_deref()
        && let Some(index) = road_history
            .iter()
            .position(|challenge| challenge.id == requested)
    {
        road_history.swap(0, index);
    }
    road_history.truncate(10);
    let foraging_dialog = if query.forage.unwrap_or(false) {
        Some(
            crate::routes::foraging::activity_dialog(
                &state,
                &character,
                "/camp",
                query.forage_receipt.as_deref(),
                query.forage_error.as_deref(),
            )
            .await,
        )
    } else {
        None
    };
    Html(
        camp_page(
            &party,
            journey.as_ref(),
            itinerary.as_ref(),
            terrain_route.as_ref(),
            &destination_name,
            Some(&character),
            &party_members,
            &camp_destinations,
            provision_forecast.as_ref(),
            default_rest_minutes,
            soap_preview,
            planned_wake_minute,
            continue_block_reason,
            encounter.as_ref(),
            &counterparties,
            &dispositions,
            trial.map(|trial| {
                (
                    trial.case_id.as_str(),
                    trial.id.as_str(),
                    trial.presenter_catalog_id,
                )
            }),
            tactical_insight,
            active_road_trial,
            &road_history,
            foraging_dialog,
            Some(&character.name),
        )
        .into_string(),
    )
    .into_response()
}

#[derive(Deserialize)]
pub(super) struct ErrantryRoadChallengeForm {
    challenge_id: String,
    expected_revision: u32,
    choice: String,
    action_id: String,
}

pub(super) async fn resolve_errantry_road_challenge(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<ErrantryRoadChallengeForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    match state
        .db
        .call(
            "resolve_errantry_road_challenge",
            &[
                json!(character_id),
                json!(&form.challenge_id),
                json!(form.expected_revision),
                json!(form.choice),
                json!(form.action_id),
            ],
        )
        .await
    {
        Ok(()) => {
            Redirect::to(&format!("/camp?road_occurrence={}", form.challenge_id)).into_response()
        }
        Err(error) if error.to_string().contains("stale") => StatusCode::CONFLICT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

fn direct_demo_challenge_redirect(
    challenges: &[BackendChallenge],
    character_id: u64,
) -> Option<String> {
    let mut playable = challenges.iter().filter(|challenge| {
        challenge.owner_character_id == character_id
            && challenge.active
            && challenge.open
            && !challenge.solved
            && is_direct_demo_challenge_id(&challenge.id, character_id)
    });
    let challenge = playable.next()?;
    playable
        .next()
        .is_none()
        .then(|| format!("/quests/{}/challenges/{}", challenge.case_id, challenge.id))
}

fn is_direct_demo_challenge_id(challenge_id: &str, character_id: u64) -> bool {
    challenge_id.starts_with("challenge:")
        && challenge_id.contains(&format!(":demo:{character_id}:"))
}

pub(super) fn camp_continue_block_reason(
    encounter_status: Option<&str>,
    is_walking_time: bool,
) -> Option<&'static str> {
    if encounter_status == Some("awaiting_choice") {
        Some("Resolve the encounter above before continuing travel.")
    } else if !is_walking_time {
        Some("Rest until the planned walking window begins.")
    } else {
        None
    }
}

#[cfg(test)]
mod direct_demo_redirect_tests {
    use super::direct_demo_challenge_redirect;
    use crate::spacetimedb::{BackendChallenge, ChallengePresenterCatalogId};

    fn challenge(id: &str, active: bool, open: bool, solved: bool) -> BackendChallenge {
        BackendChallenge {
            id: id.into(),
            case_id: "case:errantry-puzzle:demo:7:0".into(),
            party_id: "party:7".into(),
            owner_character_id: 7,
            finale_case_site_id: "site:finale".into(),
            puzzle_projection_json: "{}".into(),
            presenter_catalog_id: ChallengePresenterCatalogId::LadyBeneathThornV1,
            revision: 0,
            open,
            solved,
            active,
            last_attempt_correct: None,
            last_submission_json: None,
            tactical_insight_text: None,
            tactical_preparation_text: None,
        }
    }

    #[test]
    fn camp_forwards_only_one_unsolved_active_direct_demo() {
        let demo = challenge("challenge:ordered-sigils:demo:7:0", true, true, false);
        assert_eq!(
            direct_demo_challenge_redirect(std::slice::from_ref(&demo), 7).as_deref(),
            Some(
                "/quests/case:errantry-puzzle:demo:7:0/challenges/challenge:ordered-sigils:demo:7:0"
            )
        );

        let production = challenge("challenge:ordered-sigils:order:7:0", true, true, false);
        assert_eq!(direct_demo_challenge_redirect(&[production], 7), None);

        let solved = challenge("challenge:ordered-sigils:demo:7:0", true, false, true);
        assert_eq!(direct_demo_challenge_redirect(&[solved], 7), None);

        let another = challenge("challenge:ordered-sigils:demo:7:1", true, true, false);
        assert_eq!(direct_demo_challenge_redirect(&[demo, another], 7), None);

        let witnesses = challenge("challenge:truthful-witnesses:demo:7:2", true, true, false);
        assert_eq!(
            direct_demo_challenge_redirect(&[witnesses], 7).as_deref(),
            Some(
                "/quests/case:errantry-puzzle:demo:7:0/challenges/challenge:truthful-witnesses:demo:7:2"
            )
        );
    }
}

#[cfg(test)]
mod road_challenge_route_tests {
    #[test]
    fn narrative_encounters_are_generic_chat_native_and_server_authoritative() {
        let route = include_str!("camp.rs");
        let router = include_str!("router.rs");
        let template = include_str!("../../templates/settlement/travel.rs");
        assert!(route.contains("SELECT * FROM backend_road_challenges"));
        assert!(route.contains("challenge.active && challenge.open"));
        assert!(route.contains("challenge.id == requested"));
        assert!(route.contains("\"resolve_errantry_road_challenge\""));
        assert!(router.contains("/camp/errantry-road-challenge"));
        assert!(template.contains("aria-label=\"Roadside conversation\""));
        assert!(template.contains("generic_road_encounter(road_trial)"));
        assert!(template.contains("presentation.choices"));
        assert!(template.contains("presentation.opening"));
        assert!(template.contains("presentation.cast"));
        assert!(template.contains("Roadside characters"));
        assert!(template.contains(">Talk<") || template.contains("{ \"Talk\" }"));
        assert!(template.contains(">Bandage<") || template.contains("{ \"Bandage\" }"));
        assert!(!template.contains("challenge.actor_character_id"));
        assert!(!template.contains("EncounterDefinition"));
        assert!(!template.contains("WoundedOrderCourierV1"));
        assert!(!template.contains("Black Knight's men"));
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct EncounterChoiceForm {
    encounter_id: String,
    choice: String,
    expected_revision: u32,
    action_id: String,
}

pub(super) async fn resolve_camp_encounter(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<EncounterChoiceForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    match state
        .db
        .call(
            "resolve_strategic_encounter",
            &[
                json!(character_id),
                json!(form.encounter_id),
                json!(form.choice),
                json!(form.expected_revision),
                json!(form.action_id),
            ],
        )
        .await
    {
        Ok(()) => Redirect::to("/camp").into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct CounterpartyContactForm {
    target_id: u64,
    contact_ref: String,
    expected_revision: u32,
    action_id: String,
}

pub(super) async fn contact_camp_counterparty(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<CounterpartyContactForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    match state
        .db
        .call(
            "contact_context_character",
            &[
                json!(character_id),
                json!(form.target_id),
                json!(form.contact_ref),
                json!(form.expected_revision),
                json!(form.action_id),
            ],
        )
        .await
    {
        Ok(()) => Redirect::to("/camp").into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Debug,Deserialize)]
pub(super) struct CounterpartySurrenderForm{target_id:u64,contact_ref:String,expected_revision:u32,action:String,source_id:String}
pub(super) async fn surrender_camp_counterparty(State(state):State<AppState>,session:Session,Form(form):Form<CounterpartySurrenderForm>)->Response{
    let Some(actor_id)=session.character_id_u64() else{return Redirect::to("/characters").into_response();};
    let action=match form.action.as_str(){"offer"=>json!({"Offer":[]}),"demand"=>json!({"Demand":[]}),_=>return (StatusCode::BAD_REQUEST,"Unsupported surrender action").into_response()};
    match state.db.call("resolve_context_surrender",&[json!(actor_id),json!(form.target_id),json!(form.contact_ref),action,json!(form.expected_revision),json!(form.source_id)]).await{Ok(())=>Redirect::to("/camp").into_response(),Err(error)=>(StatusCode::BAD_REQUEST,error.to_string()).into_response()}
}

#[derive(Debug, Deserialize)]
pub(super) struct CounterpartyBandageForm {
    patient_id: u64,
}

pub(super) async fn bandage_camp_counterparty(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<CounterpartyBandageForm>,
) -> Response {
    let Some(actor_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    match state
        .db
        .call(
            "treat_limb",
            &[
                json!(actor_id),
                json!(form.patient_id),
                json!("left-arm"),
                json!("bandage"),
                crate::spacetimedb::sats_option(None::<u64>),
                json!(false),
            ],
        )
        .await
    {
        Ok(()) => Redirect::to("/camp").into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn camp_settlement_destinations(
    state: &AppState,
    party: &Party,
    journey: Option<&PartyJourney>,
) -> Vec<CampTravelDestination> {
    let Some(journey) = journey else {
        return Vec::new();
    };
    let mut endpoints = Vec::new();
    if let Some(origin_id) = journey.origin.settlement_id()
        && journey.completed_minutes > 0
    {
        endpoints.push((origin_id, journey.completed_minutes));
    }
    if let Some(destination_id) = journey.destination.settlement_id() {
        endpoints.push((
            destination_id,
            journey
                .total_minutes
                .saturating_sub(journey.completed_minutes),
        ));
    }

    let mut destinations = Vec::new();
    for (id, journey_minutes) in endpoints {
        if destinations
            .iter()
            .any(|destination: &CampTravelDestination| destination.id == id)
        {
            continue;
        }
        let settlement = state
            .db
            .query_one::<Settlement>(&format!(
                "SELECT * FROM settlement WHERE id = {}",
                sql_string_literal(id)
            ))
            .await
            .ok()
            .flatten();
        if let Some(settlement) = settlement {
            destinations.push(CampTravelDestination {
                current: party
                    .camp_destination
                    .as_ref()
                    .and_then(|destination| destination.settlement_id())
                    == Some(id),
                id: settlement.id,
                name: settlement.name,
                journey_minutes,
            });
        }
    }
    destinations
}

pub(super) async fn rest_at_camp(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<RestForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let requested_minutes = match travel_rest_minutes(&form) {
        Ok(minutes) => minutes,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    let shelter = match field_shelter_argument(&form) {
        Ok(shelter) => shelter,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    match state
        .db
        .call(
            "rest_at_camp",
            &[json!(character_id), json!(requested_minutes), shelter],
        )
        .await
    {
        Ok(()) => {
            if form.advance_development_clock {
                let _ = state
                    .db
                    .call(
                        "sync_development_clock_to_character",
                        &[json!(character_id)],
                    )
                    .await;
            }
            Redirect::to("/camp").into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn continue_camp_travel(
    State(state): State<AppState>,
    session: Session,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    match state
        .db
        .call("continue_camp_travel", &[json!(character_id)])
        .await
    {
        // A normal form redirect re-renders the authoritative camp or arrival
        // state. This remains reliable even when the live revision races the
        // reducer response.
        Ok(()) => Redirect::to("/camp").into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn change_camp_destination(
    State(state): State<AppState>,
    session: Session,
    Path(settlement_id): Path<String>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    match state
        .db
        .call(
            "travel_to_settlement",
            &[json!(character_id), json!(settlement_id)],
        )
        .await
    {
        Ok(()) => Redirect::to("/camp").into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(crate) async fn travel_provision_forecast(
    state: &AppState,
    party: Option<&Party>,
    travelers: &[Character],
    destination: &TravelDestination,
    departing_settlement: bool,
) -> Result<Option<TravelProvisionForecast>, String> {
    let rest_intervals: Vec<_> = destination
        .itinerary_segments
        .iter()
        .filter(|segment| {
            segment.kind == adventuresim_core::strategic_time::ItinerarySegmentKind::Camp
        })
        .map(|segment| {
            (
                destination
                    .departure_minute
                    .saturating_add(segment.elapsed_start),
                segment.elapsed_minutes,
            )
        })
        .collect();
    travel_provision_forecast_for_minutes(
        state,
        party,
        travelers,
        destination.itinerary_total_elapsed_minutes,
        &rest_intervals,
        departing_settlement,
    )
    .await
}

pub(super) async fn travel_provision_forecast_for_minutes(
    state: &AppState,
    party: Option<&Party>,
    travelers: &[Character],
    planning_minutes: u64,
    rest_intervals: &[(u64, u64)],
    departing_settlement: bool,
) -> Result<Option<TravelProvisionForecast>, String> {
    let mut travelers: Vec<_> = travelers.iter().filter(|traveler| traveler.alive).collect();
    travelers.sort_by_key(|traveler| traveler.id);
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .map_err(|error| error.to_string())?;
    let Some(ration) = items
        .iter()
        .find(|item| item.id == STANDARD_TRAVEL_RATION_ID)
    else {
        return Ok(None);
    };
    let Some(waterskin) = items.iter().find(|item| item.id == STANDARD_WATERSKIN_ID) else {
        return Ok(None);
    };
    let food_lots: Vec<FoodLot> = state
        .db
        .query("SELECT * FROM food_lot")
        .await
        .map_err(|error| error.to_string())?;
    let mut food_reserve_kcal = 0.0;
    let mut food_lot_kcal = 0.0;
    let mut water_reserve_ml = 0.0;
    let mut ration_count = 0;
    let mut waterskin_count = 0;
    let mut alcohol_supplies = Vec::new();
    let mut expected_morale_demands = Vec::new();
    for traveler in &travelers {
        let Some(needs) = state
            .db
            .query_one::<CharacterNeeds>(&format!(
                "SELECT * FROM backend_character_needs WHERE character_id = {}",
                traveler.id
            ))
            .await
            .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        let inventory: Vec<InventoryItem> = state
            .db
            .query(&format!(
                "SELECT * FROM inventory_item WHERE character_id = {}",
                traveler.id
            ))
            .await
            .map_err(|error| error.to_string())?;
        for entry in &inventory {
            if let Some(def) = items.iter().find(|def| def.id == entry.item_id) {
                alcohol_supplies.push(adventuresim_core::alcohol::ScopedAlcoholSupply {
                    properties: adventuresim_core::alcohol::AlcoholProperties {
                        serving_ml: def.alcohol_serving_ml,
                        abv_basis_points: def.alcohol_abv_basis_points,
                        net_hydration_ml: def.alcohol_net_hydration_ml,
                        disinfectant_effectiveness: def.alcohol_disinfectant_effectiveness,
                        disinfectant_focused: def.alcohol_disinfectant_focused,
                        potable: def.alcohol_potable,
                    },
                    quantity: entry.qty,
                    item_id: def.id.clone(),
                    stable_id: entry.id,
                    owner: Some(traveler.id),
                });
            }
        }
        let time =
            query_single::<CharacterTime>(state, "backend_character_times", traveler.id).await;
        let personality = query_single::<CharacterPersonality>(
            state,
            "backend_character_personalities",
            traveler.id,
        )
        .await;
        if time.is_some() {
            let history = state
                .db
                .query::<AlcoholConsumption>(&format!(
                    "SELECT * FROM alcohol_consumption WHERE character_id = {}",
                    traveler.id
                ))
                .await
                .map_err(|error| error.to_string())?;
            let mut evenings: Vec<_> = rest_intervals
                .iter()
                .map(|(start, minutes)| {
                    adventuresim_core::alcohol::rest_evenings(
                        *start,
                        start.saturating_add(*minutes),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .filter(|evening| {
                    !history
                        .iter()
                        .any(|row| row.evening_id == *evening && row.morale_evaluated)
                })
                .collect();
            evenings.sort_unstable();
            evenings.dedup();
            match personality.map(|p| p.temperance) {
                Some(crate::spacetimedb::Temperance::Temperate) => {}
                Some(crate::spacetimedb::Temperance::Drunkard) => {
                    expected_morale_demands.extend(evenings.into_iter().map(|evening| {
                        (
                            evening,
                            traveler.id,
                            adventuresim_core::alcohol::HEAVY_ETHANOL_ML,
                        )
                    }));
                }
                _ => {
                    let mut heavy_evenings: Vec<u64> = history
                        .iter()
                        .filter(|row| adventuresim_core::alcohol::qualifying_heavy(row.ethanol_ml))
                        .map(|row| row.evening_id)
                        .collect();
                    for evening in evenings {
                        let had_recent_heavy = heavy_evenings.iter().any(|prior| {
                            *prior < evening
                                && evening - *prior < adventuresim_core::alcohol::ROLLING_WEEK_DAYS
                        });
                        let target = if had_recent_heavy {
                            adventuresim_core::alcohol::MODEST_ETHANOL_ML
                        } else {
                            heavy_evenings.push(evening);
                            adventuresim_core::alcohol::HEAVY_ETHANOL_ML
                        };
                        expected_morale_demands.push((evening, traveler.id, target));
                    }
                }
            }
        }
        let owned = |item_id: &str| {
            inventory
                .iter()
                .filter(|entry| entry.item_id == item_id)
                .map(|entry| entry.qty)
                .sum::<u32>()
        };
        food_reserve_kcal += needs.food_balance_kcal;
        food_lot_kcal += food_lots
            .iter()
            .filter(|lot| {
                lot.inventory_item_id
                    .is_some_and(|id| inventory.iter().any(|entry| entry.id == id))
            })
            .map(|lot| lot.nutrition_kcal.max(0.0))
            .sum::<f32>();
        water_reserve_ml += needs.water_balance_ml;
        ration_count += owned(STANDARD_TRAVEL_RATION_ID);
        let skins = owned(STANDARD_WATERSKIN_ID);
        if departing_settlement {
            waterskin_count += skins;
        } else {
            water_reserve_ml += needs.carried_water_ml.max(0.0);
        }
    }
    if let Some(party) = party {
        let pooled: Vec<PartyInventoryItem> = state
            .db
            .query(&format!(
                "SELECT * FROM party_inventory_item WHERE party_id = {}",
                sql_string_literal(&party.id)
            ))
            .await
            .map_err(|error| error.to_string())?;
        for entry in &pooled {
            if let Some(def) = items.iter().find(|def| def.id == entry.item_id) {
                alcohol_supplies.push(adventuresim_core::alcohol::ScopedAlcoholSupply {
                    properties: adventuresim_core::alcohol::AlcoholProperties {
                        serving_ml: def.alcohol_serving_ml,
                        abv_basis_points: def.alcohol_abv_basis_points,
                        net_hydration_ml: def.alcohol_net_hydration_ml,
                        disinfectant_effectiveness: def.alcohol_disinfectant_effectiveness,
                        disinfectant_focused: def.alcohol_disinfectant_focused,
                        potable: def.alcohol_potable,
                    },
                    quantity: entry.quantity,
                    item_id: def.id.clone(),
                    stable_id: entry.id,
                    owner: None,
                });
            }
        }
        ration_count += pooled
            .iter()
            .filter(|row| row.item_id == STANDARD_TRAVEL_RATION_ID)
            .map(|row| row.quantity)
            .sum::<u32>();
        food_lot_kcal += food_lots
            .iter()
            .filter(|lot| {
                lot.party_inventory_item_id
                    .is_some_and(|id| pooled.iter().any(|entry| entry.id == id))
            })
            .map(|lot| lot.nutrition_kcal.max(0.0))
            .sum::<f32>();
        let party_skins = pooled
            .iter()
            .filter(|row| row.item_id == STANDARD_WATERSKIN_ID)
            .map(|row| row.quantity)
            .sum::<u32>();
        if departing_settlement {
            waterskin_count += party_skins;
        } else {
            water_reserve_ml += party.pooled_water_ml.max(0.0);
        }
    }
    expected_morale_demands.sort_by_key(|(evening, character_id, _)| (*evening, *character_id));
    let ordered_morale_demands: Vec<_> = expected_morale_demands
        .into_iter()
        .map(|(_, character_id, target)| (character_id, target))
        .collect();
    let emergency_alcohol_hydration_ml =
        adventuresim_core::alcohol::hydration_after_expected_drinking(
            alcohol_supplies,
            &ordered_morale_demands,
        );
    let inputs = PartyProvisioningInputs {
        planning_minutes,
        living_members: travelers.len() as u32,
        food_reserve_kcal,
        food_lot_kcal,
        water_reserve_ml,
        ration_count,
        waterskin_count,
        ration_kcal: ration.nutrition_kcal,
        waterskin_capacity_ml: waterskin.water_capacity_ml,
        emergency_alcohol_hydration_ml,
        ..Default::default()
    };
    let result = inputs.forecast();
    Ok(Some(TravelProvisionForecast {
        planning_minutes,
        living_members: travelers.len() as u32,
        food_days: result.food_days,
        water_days: result.water_days,
        ordinary_water_days: result.ordinary_water_days,
        emergency_alcohol_days: result.emergency_alcohol_days,
        emergency_alcohol_hydration_ml,
        food_reserve_kcal,
        water_reserve_ml,
        ration_count,
        waterskin_count,
        ration_kcal: ration.nutrition_kcal,
        waterskin_capacity_ml: waterskin.water_capacity_ml,
        rations_to_buy: result.rations_to_buy,
        waterskins_to_buy: result.waterskins_to_buy,
    }))
}
