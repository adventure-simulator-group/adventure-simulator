//! Quest route handlers

use axum::{
    Form, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{
    AppState, PartyAction, PartyActionOutcome, execute_or_request_party_action,
    participates_in_party_readiness,
    settlements::{
        RestForm, get_active_party_members, living_party_members, soap_rest_preview,
        travel_rest_minutes,
    },
    travel::{
        TravelDestination, TravelForm, apply_terrain_route, populate_itinerary_forecasts,
        settlement_destination,
    },
};
use crate::session::Session;
use crate::spacetimedb::sql_string_literal;
use crate::spacetimedb::{
    AutoresolveReport, BackendCaseBattle, BackendCaseSitePin, BackendContextCharacter,
    BackendContextDisposition, BackendCorpse, BackendInvestigationAction, BattleLootItem,
    BattleResult, Character, CharacterAttributes, CharacterLimbs, CharacterStats,
    CharacterStrategicCondition, CharacterTime, CharacterTrainingSchedule, ContractPresentation,
    ContractPresentationStatus, FoodLot, InventoryQuantityTarget, ItemDefinition, Party,
    PartyInventoryItem, PartyStake, Settlement,
};
use crate::templates::quest::{
    CaseSitePagePresentation, CaseSiteRecoveryNotice, quest_location_enemy_page,
    quest_location_map_page,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/quests/{id}/accept", post(accept_quest_api))
        .route("/api/quests/{id}/turn-in", post(turn_in_quest_api))
        .route("/quests/{id}/abandon", post(abandon_quest))
        .route("/case-sites/{id}/travel", post(travel_to_case_site))
        .route("/case-sites/{id}/track", post(track_case_site))
        .route("/locations/case-site/{id}", get(quest_location_base))
        .route("/locations/case-site/{id}/map", get(quest_location_map))
        .route("/locations/case-site/{id}/enemy", get(quest_location_enemy))
        .route(
            "/locations/case-site/{id}/counterparty/contact",
            post(contact_quest_counterparty),
        )
        .route(
            "/locations/case-site/{id}/counterparty/bandage",
            post(bandage_quest_counterparty),
        )
        .route(
            "/locations/case-site/{id}/counterparty/surrender",
            post(surrender_quest_counterparty),
        )
        .route("/corpses/{corpse_id}/action", post(perform_corpse_action))
        .route(
            "/locations/case-site/{id}/loot",
            get(quest_location_legacy_loot),
        )
        .route(
            "/locations/case-site/{id}/rest",
            post(rest_at_quest_location),
        )
        .route(
            "/locations/case-site/{id}/map/rest",
            post(rest_at_quest_location_map),
        )
        .route("/quests/{id}/autoresolve", post(autoresolve_quest))
        .route("/quests/{id}/loot/store", post(store_battle_loot))
}

#[derive(Serialize)]
struct AcceptQuestResponse {
    accepted: bool,
    quest_id: String,
    title: String,
    message: String,
}

async fn accept_quest_api(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Json<AcceptQuestResponse> {
    let title = state
        .db
        .query::<ContractPresentation>(&format!(
            "SELECT * FROM backend_contracts WHERE id = {}",
            sql_string_literal(&id)
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next()
        .map(|quest| quest.title)
        .unwrap_or_else(|| "Quest".to_string());
    let result = match session.character_id_u64() {
        Some(character_id) => accept_quest_for_character(&state, character_id, &id).await,
        None => Err("Choose a character first".to_string()),
    };
    match result {
        Ok(outcome) => Json(AcceptQuestResponse {
            accepted: matches!(outcome, PartyActionOutcome::Executed),
            quest_id: id,
            title,
            message: if matches!(outcome, PartyActionOutcome::Executed) {
                "Quest accepted."
            } else {
                "Requested that the party accept this quest."
            }
            .to_string(),
        }),
        Err(error) => Json(AcceptQuestResponse {
            accepted: false,
            quest_id: id,
            title,
            message: error,
        }),
    }
}

async fn accept_quest_for_character(
    state: &AppState,
    character_id: u64,
    quest_id: &str,
) -> Result<PartyActionOutcome, String> {
    execute_or_request_party_action(
        state,
        character_id,
        PartyAction::AcceptContract {
            contract_id: quest_id.into(),
        },
    )
    .await
}

#[derive(Serialize)]
struct TurnInQuestResponse {
    claimed: bool,
    reward: i32,
    message: String,
}

async fn turn_in_quest_api(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Json<TurnInQuestResponse> {
    let reward = state
        .db
        .query::<ContractPresentation>(&format!(
            "SELECT * FROM backend_contracts WHERE id = {}",
            sql_string_literal(&id)
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next()
        .map_or(0, |quest| quest.gold_reward);
    let result = match session.character_id_u64() {
        Some(character_id) => state
            .db
            .call("report_contract", &[json!(character_id), json!(id)])
            .await
            .map_err(|error| error.to_string()),
        None => Err("Choose a character first".to_string()),
    };
    match result {
        Ok(()) => Json(TurnInQuestResponse {
            claimed: true,
            reward,
            message: "Quest reward added to the party inventory.".to_string(),
        }),
        Err(error) => Json(TurnInQuestResponse {
            claimed: false,
            reward: 0,
            message: error,
        }),
    }
}

async fn abandon_quest(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };

    let quests: Vec<ContractPresentation> = state
        .db
        .query(&format!(
            "SELECT * FROM backend_contracts WHERE id = {}",
            sql_string_literal(&id)
        ))
        .await
        .unwrap_or_default();
    let settlement_id = quests.first().map(|quest| quest.settlement_id.clone());
    let _ = execute_or_request_party_action(
        &state,
        character_id,
        PartyAction::AbandonContract {
            contract_id: id.clone(),
        },
    )
    .await;

    settlement_id.map_or_else(
        || Redirect::to("/"),
        |settlement_id| Redirect::to(&format!("/locations/settlement/{settlement_id}")),
    )
}

async fn travel_to_case_site(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    axum::Form(_form): axum::Form<TravelForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let outcome = execute_or_request_party_action(
        &state,
        character_id,
        PartyAction::TravelToCaseSite {
            case_site_id: id.clone(),
        },
    )
    .await;
    match outcome {
        Ok(PartyActionOutcome::Executed) => Redirect::to("/camp").into_response(),
        Ok(PartyActionOutcome::Requested) => (
            StatusCode::ACCEPTED,
            Html(
                crate::templates::strategic_notice_page(
                    "Travel requested",
                    "The party leader has been asked to begin this journey.",
                    "/quests",
                    "Return to the journal",
                    None,
                )
                .into_string(),
            ),
        )
            .into_response(),
        Err(error) => {
            tracing::warn!(%error, character_id, "case-site travel rejected");
            (
                StatusCode::BAD_REQUEST,
                Html(
                    crate::templates::strategic_notice_page(
                        "Travel could not begin",
                        safe_case_site_travel_error(&error),
                        "/quests",
                        "Return to the journal",
                        None,
                    )
                    .into_string(),
                ),
            )
                .into_response()
        }
    }
}

fn safe_case_site_travel_error(error: &str) -> &'static str {
    if error.contains("incapacitated") {
        "An incapacitated party member must recover before the party can travel."
    } else if error.contains("current journey") || error.contains("camped") {
        "Finish or change the party's current journey before starting another."
    } else if error.contains("party leader") {
        "Only the party leader can begin this journey immediately."
    } else {
        "The exact destination or the party's travel readiness changed. Review the journal before trying again."
    }
}

async fn track_case_site(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    match state
        .db
        .call(
            "track_case_site",
            &[json!(character_id), json!({ "value": id })],
        )
        .await
    {
        Ok(()) => Redirect::to("/").into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Default, serde::Deserialize)]
struct StoreLootForm {
    #[serde(default)]
    item_ids: String,
    #[serde(default)]
    quantities: String,
}

async fn store_battle_loot(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<StoreLootForm>,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    if let Err(error) = state
        .db
        .call(
            "store_battle_loot",
            &[
                json!(character_id),
                json!(id.clone()),
                json!(
                    form.item_ids
                        .split(',')
                        .filter_map(|v| v.parse::<u64>().ok())
                        .collect::<Vec<_>>()
                ),
                json!(
                    form.quantities
                        .split(',')
                        .filter_map(|v| v.parse::<u32>().ok())
                        .collect::<Vec<_>>()
                ),
            ],
        )
        .await
    {
        tracing::error!("Failed to store battle loot: {error:?}");
    }
    let case_site_id = super::data::character(&state, character_id)
        .await
        .ok()
        .flatten()
        .and_then(|character| character.current_case_site_id);
    case_site_id.map_or_else(
        || Redirect::to("/"),
        |case_site_id| Redirect::to(&format!("/locations/case-site/{case_site_id}/enemy")),
    )
}

#[derive(Clone, Default, serde::Deserialize)]
struct QuestMapQuery {
    destination: Option<String>,
}

#[derive(Clone, Default, serde::Deserialize)]
struct QuestEnemyQuery {
    corpse: Option<String>,
    medical: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QuestCounterpartyBandageForm {
    patient_id: u64,
}

async fn bandage_quest_counterparty(
    State(state): State<AppState>,
    Path(case_site_id): Path<String>,
    session: Session,
    Form(form): Form<QuestCounterpartyBandageForm>,
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
        Ok(()) => {
            Redirect::to(&format!("/locations/case-site/{case_site_id}/enemy")).into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

enum QuestLocationTab {
    Map(Option<String>),
    Enemy(QuestEnemyQuery),
}

fn case_site_page_presentation(
    site: &BackendCaseSitePin,
    legacy_quest: Option<&ContractPresentation>,
) -> Option<CaseSitePagePresentation> {
    if site.generated_case {
        if site.display_title.is_empty() {
            return None;
        }
        Some(CaseSitePagePresentation {
            title: site.display_title.clone(),
            action_id: site.case_site_id.clone(),
            allow_tactical_combat: false,
        })
    } else {
        let quest = legacy_quest?;
        Some(CaseSitePagePresentation {
            title: quest.title.clone(),
            action_id: quest.id.clone(),
            allow_tactical_combat: true,
        })
    }
}

fn case_site_combat_permitted(
    site: &BackendCaseSitePin,
    legacy_quest: Option<&ContractPresentation>,
    active_contract_id: Option<&str>,
    can_control: bool,
    party_ready: bool,
) -> bool {
    if !can_control || !party_ready {
        return false;
    }
    if site.generated_case {
        site.combat_available
    } else {
        legacy_quest.is_some_and(|quest| {
            quest.status == ContractPresentationStatus::Accepted
                && active_contract_id == Some(quest.id.as_str())
        })
    }
}

fn case_site_is_resolved(site: &BackendCaseSitePin, has_battle_result: bool) -> bool {
    site.case_resolved || has_battle_result
}

fn onsite_investigation_actions(
    actions: Vec<BackendInvestigationAction>,
    case_site_id: &str,
) -> Vec<BackendInvestigationAction> {
    actions
        .into_iter()
        .filter(|action| action.available && action.required_case_site_id == case_site_id)
        .collect()
}

fn character_and_party_are_at_case_site(
    character: Option<&Character>,
    party: Option<&Party>,
    case_site_id: &str,
) -> bool {
    character
        .is_some_and(|character| character.current_case_site_id.as_deref() == Some(case_site_id))
        && party.is_some_and(|party| {
            party
                .current_case_site_id
                .as_ref()
                .is_some_and(|id| id.value == case_site_id)
        })
}

fn case_site_recovery_notice(
    members: &[Character],
    conditions: &[CharacterStrategicCondition],
    site_id: &str,
    nearest_settlement: Option<&TravelDestination>,
) -> Option<CaseSiteRecoveryNotice> {
    let incapacitated = members
        .iter()
        .filter_map(|member| {
            let condition = conditions
                .iter()
                .find(|row| row.character_id == member.id && row.status == "incapacitated")?;
            Some((member, condition))
        })
        .collect::<Vec<_>>();
    if incapacitated.is_empty() {
        return None;
    }
    let member_names = incapacitated
        .iter()
        .map(|(member, _)| member.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let has = |select: fn(&CharacterStrategicCondition) -> f32| {
        incapacitated
            .iter()
            .any(|(_, condition)| select(condition) > 0.001)
    };
    let mut causes = Vec::new();
    if has(|condition| condition.hunger) {
        causes.push("hunger");
    }
    if has(|condition| condition.thirst) {
        causes.push("thirst");
    }
    if has(|condition| condition.pain) {
        causes.push("pain");
    }
    if has(|condition| condition.blood_loss) {
        causes.push("blood loss");
    }
    if has(|condition| condition.fatigue) {
        causes.push("fatigue");
    }
    if has(|condition| condition.fear) {
        causes.push("fear");
    }
    let resource_blocked = causes.contains(&"hunger") || causes.contains(&"thirst");
    let (withdrawal_destination, withdrawal_href) = nearest_settlement.map_or_else(
        || {
            (
                "a settlement".to_owned(),
                format!("/locations/case-site/{site_id}/map"),
            )
        },
        |destination| {
            (
                destination.name.clone(),
                format!(
                    "/locations/case-site/{site_id}/map?destination={}",
                    destination.id
                ),
            )
        },
    );
    Some(CaseSiteRecoveryNotice {
        member_names,
        causes: causes.join(", "),
        resource_blocked,
        withdrawal_destination,
        withdrawal_href,
    })
}

async fn quest_location_base(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Response {
    render_quest_location(state, id, session, QuestLocationTab::Map(None)).await
}

async fn quest_location_map(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<QuestMapQuery>,
    session: Session,
) -> Response {
    render_quest_location(state, id, session, QuestLocationTab::Map(query.destination)).await
}

async fn quest_location_enemy(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<QuestEnemyQuery>,
    session: Session,
) -> Response {
    render_quest_location(state, id, session, QuestLocationTab::Enemy(query)).await
}

#[derive(serde::Deserialize)]
struct CorpseActionForm {
    action_kind: String,
    discipline: String,
    stage: String,
    action_id: String,
    expected_revision: u32,
    #[serde(default)]
    confirm_unauthorized: bool,
    return_to: String,
}

async fn perform_corpse_action(
    State(state): State<AppState>,
    Path(corpse_id): Path<String>,
    session: Session,
    Form(form): Form<CorpseActionForm>,
) -> Redirect {
    let Some(actor_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    let result = match form.action_kind.as_str() {
        "open" => {
            state
                .db
                .call(
                    "open_corpse",
                    &[
                        json!(actor_id),
                        json!(&corpse_id),
                        json!(&form.action_id),
                        json!(form.expected_revision),
                        json!(form.confirm_unauthorized),
                    ],
                )
                .await
        }
        "exhume" => {
            state
                .db
                .call(
                    "exhume_corpse",
                    &[
                        json!(actor_id),
                        json!(&corpse_id),
                        json!(&form.action_id),
                        json!(form.expected_revision),
                        json!(form.confirm_unauthorized),
                    ],
                )
                .await
        }
        "bury" => {
            state
                .db
                .call(
                    "bury_corpse",
                    &[
                        json!(actor_id),
                        json!(&corpse_id),
                        json!(&form.action_id),
                        json!(form.expected_revision),
                    ],
                )
                .await
        }
        "burn" => {
            state
                .db
                .call(
                    "burn_corpse",
                    &[
                        json!(actor_id),
                        json!(&corpse_id),
                        json!(&form.action_id),
                        json!(form.expected_revision),
                        json!(form.confirm_unauthorized),
                    ],
                )
                .await
        }
        _ => {
            state
                .db
                .call(
                    "examine_corpse",
                    &[
                        json!(actor_id),
                        json!(&corpse_id),
                        json!(&form.discipline),
                        json!(&form.stage),
                        json!(&form.action_id),
                        json!(form.expected_revision),
                        json!(form.confirm_unauthorized),
                    ],
                )
                .await
        }
    };
    if let Err(error) = result {
        tracing::warn!(%error, actor_id, %corpse_id, "corpse medical action failed");
    }
    super::redirect_to_local(&form.return_to, "/")
}

async fn quest_location_legacy_loot(Path(id): Path<String>) -> Redirect {
    Redirect::to(&format!("/locations/case-site/{id}/enemy"))
}

async fn rest_at_quest_location(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<RestForm>,
) -> Response {
    rest_at_quest_location_with_redirect(
        state,
        id.clone(),
        session,
        form,
        &format!("/locations/case-site/{id}/enemy"),
    )
    .await
}

async fn rest_at_quest_location_map(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<RestForm>,
) -> Response {
    rest_at_quest_location_with_redirect(
        state,
        id.clone(),
        session,
        form,
        &format!("/locations/case-site/{id}/map"),
    )
    .await
}

async fn rest_at_quest_location_with_redirect(
    state: AppState,
    id: String,
    session: Session,
    form: RestForm,
    redirect_path: &str,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let character = super::data::character(&state, character_id)
        .await
        .ok()
        .flatten();
    if character
        .as_ref()
        .and_then(|row| row.current_case_site_id.as_deref())
        != Some(id.as_str())
    {
        return (
            StatusCode::BAD_REQUEST,
            "The party is not at this quest location",
        )
            .into_response();
    }
    let requested_minutes = match travel_rest_minutes(&form) {
        Ok(minutes) => minutes,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    let shelter = match super::settlements::field_shelter_argument(&form) {
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
        Ok(()) => Redirect::to(redirect_path).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn render_quest_location(
    state: AppState,
    case_site_id: String,
    session: Session,
    tab: QuestLocationTab,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let character = super::data::character(&state, character_id)
        .await
        .ok()
        .flatten();
    let known_site = state
        .db
        .query_one::<BackendCaseSitePin>(&format!(
            "SELECT * FROM backend_case_site_pins WHERE owner_character_id = {character_id} AND case_site_id = {}",
            sql_string_literal(&case_site_id)
        ))
        .await
        .ok()
        .flatten();
    let Some(site) = known_site.as_ref() else {
        return (
            StatusCode::NOT_FOUND,
            Html(
                crate::templates::strategic_notice_page(
                    "Case site not found",
                    "That exact destination has not been disclosed to this character.",
                    "/characters",
                    "Return to character select",
                    character.as_ref().map(|character| character.name.as_str()),
                )
                .into_string(),
            ),
        )
            .into_response();
    };
    let legacy_quest = if site.generated_case {
        None
    } else {
        state
            .db
            .query::<ContractPresentation>(&format!(
                "SELECT * FROM backend_contracts WHERE case_id = {}",
                sql_string_literal(&site.case_id)
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
    };
    let Some(mut presentation) = case_site_page_presentation(site, legacy_quest.as_ref()) else {
        return (
            StatusCode::NOT_FOUND,
            Html(
                crate::templates::strategic_notice_page(
                    "Quest location not found",
                    "The requested destination is no longer available.",
                    "/characters",
                    "Return to character select",
                    None,
                )
                .into_string(),
            ),
        )
            .into_response();
    };
    let party = if let Some(party_id) = character.as_ref().and_then(|c| c.party_id.as_ref()) {
        state
            .db
            .query::<Party>(&format!(
                "SELECT * FROM party WHERE id = {}",
                sql_string_literal(party_id)
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
    } else {
        None
    };
    let is_at_location = character_and_party_are_at_case_site(
        character.as_ref(),
        party.as_ref(),
        &site.case_site_id,
    );
    if !is_at_location {
        let return_href = character
            .as_ref()
            .and_then(|character| character.current_settlement_id.as_deref())
            .map(|settlement_id| format!("/locations/settlement/{settlement_id}"))
            .unwrap_or_else(|| "/characters".to_string());
        return (
            StatusCode::FORBIDDEN,
            Html(
                crate::templates::strategic_notice_page(
                    "Your party is elsewhere",
                    "Travel to this quest destination before opening its map or enemy views.",
                    &return_href,
                    "Return to your location",
                    character.as_ref().map(|character| character.name.as_str()),
                )
                .into_string(),
            ),
        )
            .into_response();
    }
    let settlements: Vec<Settlement> = state
        .db
        .query("SELECT * FROM settlement")
        .await
        .unwrap_or_default();
    let mut nearby: Vec<TravelDestination> = settlements
        .iter()
        .cloned()
        .map(|settlement| {
            let distance_m = straight_line_distance_m(site, &settlement);
            settlement_destination(settlement, distance_m, offroad_journey_minutes(distance_m))
        })
        .collect();
    nearby.sort_by_key(|destination| destination.distance_m);
    nearby.truncate(5);
    if let QuestLocationTab::Map(Some(selected_id)) = &tab
        && let Some(destination) = nearby
            .iter_mut()
            .find(|destination| destination.id == *selected_id)
        && let Some(settlement) = settlements
            .iter()
            .find(|settlement| settlement.id == destination.id)
    {
        let terrain_profile = if let Some(character) = character.as_ref() {
            crate::routes::party_terrain_profile(&state, character)
                .await
                .unwrap_or_default()
                .0
        } else {
            adventuresim_terrain::TerrainSkillProfile::default()
        };
        apply_terrain_route(
            destination,
            state.terrain.as_deref(),
            (
                f64::from(site.latitude_e7) / 10_000_000.0,
                f64::from(site.longitude_e7) / 10_000_000.0,
            ),
            (settlement.coord_y, settlement.coord_x),
            terrain_profile,
        )
        .await;
    }
    let can_control = character.as_ref().zip(party.as_ref()).is_some();
    let can_configure_travel = character
        .as_ref()
        .zip(party.as_ref())
        .is_some_and(|(character, party)| party.leader_id == character.id);
    let case_battle = if let Some(party) = party.as_ref() {
        state
            .db
            .query::<BackendCaseBattle>(&format!(
                "SELECT * FROM backend_case_battles WHERE owner_character_id = {character_id} AND public_case_id = {} AND party_id = {}",
                sql_string_literal(&site.case_id),
                sql_string_literal(&party.id),
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .find(|battle| battle.case_site_id.value == site.case_site_id)
    } else {
        None
    };
    if site.generated_case
        && let Some(case_battle) = case_battle.as_ref()
    {
        presentation.action_id = case_battle.battle_id.clone();
    }
    let results: Vec<BattleResult> = if let Some(case_battle) = case_battle.as_ref() {
        state
            .db
            .query(&format!(
                "SELECT * FROM battle_result WHERE battle_id = {}",
                sql_string_literal(&case_battle.battle_id)
            ))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let resolved = case_site_is_resolved(site, !results.is_empty());
    let autoresolve_report = if let Some(case_battle) = case_battle.as_ref() {
        state
            .db
            .query::<AutoresolveReport>(&format!(
                "SELECT * FROM autoresolve_report WHERE battle_id = {}",
                sql_string_literal(&case_battle.battle_id)
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
    } else {
        None
    };
    let loot: Vec<BattleLootItem> = if let Some(case_battle) = case_battle.as_ref() {
        state
            .db
            .query(&format!(
                "SELECT * FROM battle_loot_item WHERE loot_battle_id = {}",
                sql_string_literal(&case_battle.battle_id)
            ))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let pooled: Vec<PartyInventoryItem> = if let Some(party) = party.as_ref() {
        state
            .db
            .query(&format!(
                "SELECT * FROM party_inventory_item WHERE party_id = {}",
                sql_string_literal(&party.id)
            ))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let stakes: Vec<PartyStake> = if let Some(party) = party.as_ref() {
        state
            .db
            .query(&format!(
                "SELECT * FROM party_stake WHERE party_id = {}",
                sql_string_literal(&party.id)
            ))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let stake = character.as_ref().map_or(0, |character| {
        stakes
            .iter()
            .find(|stake| stake.character_id == character.id)
            .map_or(0, |stake| stake.value)
    });
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let targets = if let Some(party) = party.as_ref() {
        party_targets(&state, &party.id).await
    } else {
        Vec::new()
    };
    let food_lots: Vec<FoodLot> = state
        .db
        .query("SELECT * FROM food_lot")
        .await
        .unwrap_or_default();
    let party_members = get_active_party_members(&state, character.as_ref()).await;
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
            (row.calories_used.max(0.0)
                / adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY
                * adventuresim_core::strategic_time::MINUTES_PER_DAY as f32)
                .ceil() as u64
        })
        .max()
        .unwrap_or(0)
        .max(1);
    if let Some(party) = party.as_ref() {
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
            &mut nearby,
            &member_ids,
            &attributes,
            &limbs,
            &stats,
            &times,
            &schedules,
            party,
        );
        for destination in &mut nearby {
            destination.provision_forecast = super::settlements::travel_provision_forecast(
                &state,
                Some(party),
                &living_party_members,
                destination,
                false,
            )
            .await
            .ok()
            .flatten();
        }
    }
    let (party_ready, strategic_conditions) = party_readiness(&state, &party_members).await;
    let recovery_notice = case_site_recovery_notice(
        &living_party_members,
        &strategic_conditions,
        &site.case_site_id,
        nearby.first(),
    );
    let can_fight = case_site_combat_permitted(
        site,
        legacy_quest.as_ref(),
        party
            .as_ref()
            .and_then(|party| party.active_contract_id.as_deref()),
        can_control,
        party_ready,
    );
    let context_memberships: Vec<BackendContextCharacter> = state
        .db
        .query(&format!(
            "SELECT * FROM backend_context_characters WHERE location_id = {} AND party_id = {}",
            sql_string_literal(&site.case_site_id),
            sql_string_literal(party.as_ref().map_or("", |party| party.id.as_str()))
        ))
        .await
        .unwrap_or_default();
    let counterparty_contact = context_memberships
        .first()
        .map(|row| (row.contact_ref.clone(), row.revision));
    let dispositions:Vec<BackendContextDisposition>=state.db.query(&format!("SELECT * FROM backend_context_dispositions WHERE observer_party_id = {} AND contact_ref = {}",sql_string_literal(party.as_ref().map_or("",|party|party.id.as_str())),sql_string_literal(counterparty_contact.as_ref().map_or(&site.case_site_id,|(contact_ref,_)|contact_ref)))).await.unwrap_or_default();
    let mut counterparties = Vec::new();
    for membership in context_memberships.into_iter().filter(|row| row.alive) {
        if let Ok(Some(counterparty)) =
            super::data::character(&state, membership.character_id).await
        {
            counterparties.push(counterparty);
        }
    }
    let onsite_actions = onsite_investigation_actions(
        state
            .db
            .query::<BackendInvestigationAction>(&format!(
                "SELECT * FROM backend_investigation_actions WHERE owner_character_id = {character_id}"
            ))
            .await
            .unwrap_or_default(),
        &site.case_site_id,
    );
    let corpses = state
        .db
        .query::<BackendCorpse>(&format!(
            "SELECT * FROM backend_corpses WHERE owner_character_id = {character_id}"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|corpse| corpse.case_site_id == site.case_site_id && corpse.location == "scene")
        .collect::<Vec<_>>();
    let selected_corpse_coordinate = match &tab {
        QuestLocationTab::Enemy(query) => query.corpse.as_deref().and_then(|corpse_id| {
            corpses
                .iter()
                .position(|corpse| corpse.corpse_id == corpse_id)
                .map(|index| {
                    (
                        index,
                        if query.medical.as_deref() == Some("surgery") {
                            "surgery"
                        } else {
                            "physiology"
                        },
                    )
                })
        }),
        QuestLocationTab::Map(_) => None,
    };
    let selected_corpse =
        selected_corpse_coordinate.map(|(index, window)| (&corpses[index], window));
    let logged_in_as = character.as_ref().map(|c| c.name.as_str());
    let soap_preview = soap_rest_preview(
        &state,
        &party_members,
        party.as_ref().map(|party| party.id.as_str()),
    )
    .await;
    let page = match tab {
        QuestLocationTab::Map(selected) => quest_location_map_page(
            &presentation,
            site,
            &onsite_actions,
            &nearby,
            selected.as_deref(),
            character.as_ref(),
            &party_members,
            can_control,
            can_fight,
            resolved,
            autoresolve_report.as_ref(),
            party.as_ref(),
            can_configure_travel,
            default_rest_minutes,
            soap_preview,
            recovery_notice.as_ref(),
            logged_in_as,
            &corpses,
            None,
        ),
        QuestLocationTab::Enemy(_query) => quest_location_enemy_page(
            &presentation,
            site,
            &onsite_actions,
            character.as_ref(),
            &party_members,
            &counterparties,
            &dispositions,
            counterparty_contact
                .as_ref()
                .map(|(contact_ref, _)| contact_ref.as_str()),
            counterparty_contact
                .as_ref()
                .map_or(1, |(_, revision)| *revision),
            can_fight,
            resolved,
            autoresolve_report.as_ref(),
            party.as_ref(),
            can_configure_travel,
            default_rest_minutes,
            soap_preview,
            recovery_notice.as_ref(),
            &loot,
            &pooled,
            stake,
            &items,
            &food_lots,
            &targets,
            logged_in_as,
            &corpses,
            selected_corpse,
        ),
    };
    Html(page.into_string()).into_response()
}

async fn party_readiness(
    state: &AppState,
    members: &[Character],
) -> (bool, Vec<CharacterStrategicCondition>) {
    let mut ready = true;
    let mut conditions = Vec::new();
    for member in members
        .iter()
        .filter(|member| participates_in_party_readiness(member.alive))
    {
        if state
            .db
            .call(
                "refresh_strategic_condition",
                &[serde_json::json!(member.id)],
            )
            .await
            .is_err()
        {
            ready = false;
            continue;
        }
        let condition = state
            .db
            .query_one::<CharacterStrategicCondition>(&format!(
                "SELECT * FROM backend_character_strategic_conditions WHERE character_id = {}",
                member.id
            ))
            .await;
        match condition {
            Ok(Some(condition)) => {
                ready &= condition.status != "incapacitated";
                conditions.push(condition);
            }
            _ => ready = false,
        }
    }
    (ready, conditions)
}

async fn party_targets(state: &AppState, party_id: &str) -> Vec<InventoryQuantityTarget> {
    let party = state
        .db
        .query::<Party>(&format!(
            "SELECT * FROM party WHERE id = {}",
            sql_string_literal(party_id)
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    let Some(party) = party else {
        return Vec::new();
    };
    state.db.query(&format!("SELECT * FROM inventory_quantity_target WHERE owner_character_id = {} AND party_scope = true", party.leader_id)).await.unwrap_or_default()
}

async fn autoresolve_quest(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    let selected_case_site_id = super::data::character(&state, character_id)
        .await
        .ok()
        .flatten()
        .and_then(|character| character.current_case_site_id);
    if selected_case_site_id.as_deref() != Some(id.as_str()) {
        return selected_case_site_id.map_or_else(
            || Redirect::to("/characters"),
            |case_site_id| Redirect::to(&format!("/locations/case-site/{case_site_id}/enemy")),
        );
    }
    let outcome = execute_or_request_party_action(
        &state,
        character_id,
        PartyAction::AutoresolveMission {
            mission_id: format!("mission:autoresolve-{}", super::data::new_id()),
        },
    )
    .await;
    if let Err(ref error) = outcome {
        tracing::error!("Failed to autoresolve quest: {error:?}");
    }
    let case_site_id = super::data::character(&state, character_id)
        .await
        .ok()
        .flatten()
        .and_then(|character| character.current_case_site_id);
    autoresolve_redirect(case_site_id.as_deref(), outcome)
}

#[derive(Debug, Deserialize)]
struct QuestCounterpartyContactForm {
    target_id: u64,
    contact_ref: String,
    expected_revision: u32,
    action_id: String,
}

async fn contact_quest_counterparty(
    State(state): State<AppState>,
    Path(case_site_id): Path<String>,
    session: Session,
    Form(form): Form<QuestCounterpartyContactForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let return_to = format!("/locations/case-site/{case_site_id}/enemy");
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
        Ok(()) => Redirect::to(&return_to).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct QuestCounterpartySurrenderForm {
    target_id: u64,
    contact_ref: String,
    expected_revision: u32,
    action: String,
    source_id: String,
}
async fn surrender_quest_counterparty(
    State(state): State<AppState>,
    Path(case_site_id): Path<String>,
    session: Session,
    Form(form): Form<QuestCounterpartySurrenderForm>,
) -> Response {
    let Some(actor_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let action = match form.action.as_str() {
        "offer" => json!({"Offer": []}),
        "demand" => json!({"Demand": []}),
        _ => return (StatusCode::BAD_REQUEST, "Unsupported surrender action").into_response(),
    };
    match state
        .db
        .call(
            "resolve_context_surrender",
            &[
                json!(actor_id),
                json!(form.target_id),
                json!(form.contact_ref),
                action,
                json!(form.expected_revision),
                json!(form.source_id),
            ],
        )
        .await
    {
        Ok(()) => {
            Redirect::to(&format!("/locations/case-site/{case_site_id}/enemy")).into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

fn autoresolve_redirect<E>(
    case_site_id: Option<&str>,
    outcome: Result<PartyActionOutcome, E>,
) -> Redirect {
    match outcome {
        Ok(PartyActionOutcome::Executed) | Err(_) => case_site_id.map_or_else(
            || Redirect::to("/"),
            |id| Redirect::to(&format!("/locations/case-site/{id}/enemy")),
        ),
        Ok(PartyActionOutcome::Requested) => Redirect::to("/?party-requested=autoresolve"),
    }
}

pub(crate) fn offroad_journey_minutes(distance_m: u64) -> u64 {
    ((distance_m as f64 / 1_250.0) * 60.0).ceil() as u64
}

pub(crate) fn straight_line_distance_m(site: &BackendCaseSitePin, settlement: &Settlement) -> u64 {
    let longitude = f64::from(site.longitude_e7) / 10_000_000.0;
    let latitude = f64::from(site.latitude_e7) / 10_000_000.0;
    if site.coordinates_are_geographic && settlement.source_node_id.is_some() {
        let lat1 = latitude.to_radians();
        let lat2 = settlement.coord_y.to_radians();
        let delta_lat = (settlement.coord_y - latitude).to_radians();
        let delta_lon = (settlement.coord_x - longitude).to_radians();
        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
        (6_371_000.0 * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())).round() as u64
    } else {
        (((longitude - settlement.coord_x).powi(2) + (latitude - settlement.coord_y).powi(2))
            .sqrt()
            * 1_000.0)
            .round() as u64
    }
}

#[cfg(test)]
mod quest_route_tests {
    use axum::http::header::LOCATION;

    use super::*;

    fn redirect_location(redirect: Redirect) -> String {
        redirect
            .into_response()
            .headers()
            .get(LOCATION)
            .expect("redirect has a location")
            .to_str()
            .expect("redirect location is valid text")
            .to_owned()
    }

    fn case_site(generated_case: bool, combat_available: bool) -> BackendCaseSitePin {
        BackendCaseSitePin {
            owner_character_id: 7,
            case_id: "journal:case".into(),
            case_site_id: "site:known".into(),
            origin_settlement_id: "settlement".into(),
            name: "a camp in the woods".into(),
            description: "A known place.".into(),
            scene_key: "forest".into(),
            longitude_e7: 0,
            latitude_e7: 0,
            coordinates_are_geographic: false,
            distance_m: 4_000,
            knowledge_stage: "visited".into(),
            tracked: false,
            display_title: "Travellers have gone missing".into(),
            generated_case,
            case_resolved: false,
            combat_available,
            opposition_count: None,
            opposition_combat_power: None,
        }
    }

    fn accepted_contract() -> ContractPresentation {
        ContractPresentation {
            id: "contract:one".into(),
            case_id: "case:one".into(),
            title: "Legacy bounty".into(),
            description: String::new(),
            difficulty: 1,
            gold_reward: 10,
            xp_reward: 10,
            settlement_id: "settlement".into(),
            service_id: "tavern".into(),
            issuer_resident_character_id: "npc:issuer".into(),
            status: ContractPresentationStatus::Accepted,
            accepted_by: Some("party".into()),
            opposition_wording: "unknown opposition".into(),
            opposition_count_wording: "unknown number".into(),
        }
    }

    fn onsite_action(site: &str, available: bool) -> BackendInvestigationAction {
        BackendInvestigationAction {
            owner_character_id: 7,
            action_id: "action:inspect".into(),
            method: "inspect_site".into(),
            expected_version: 1,
            summary: "Inspect this place".into(),
            known_prerequisites: String::new(),
            duration_min_minutes: 15,
            duration_max_minutes: 45,
            uncertainty_bps: 2500,
            skill_contributions: "awareness".into(),
            weather_available: false,
            required_case_site_id: site.into(),
            available,
            can_travel_to_required_site: false,
            unavailable_reason: String::new(),
        }
    }

    fn character_at(case_site_id: Option<&str>) -> Character {
        Character {
            id: 7,
            name: "Ada".into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: None,
            current_case_site_id: case_site_id.map(str::to_owned),
            party_id: Some("party".into()),
            age_years: 25,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        }
    }

    fn party_at(case_site_id: Option<&str>) -> Party {
        Party {
            id: "party".into(),
            name: "Ada's party".into(),
            leader_id: 7,
            current_settlement_id: None,
            current_case_site_id: case_site_id.map(|value| crate::spacetimedb::CaseSiteId {
                value: value.to_owned(),
            }),
            active_contract_id: None,
            is_solo: true,
            camp_fatigue_percent: 50,
            walking_minutes_per_day: 480,
            travel_at_night: false,
            camp_duration_mode: crate::spacetimedb::CampDurationMode::Auto,
            fixed_camp_minutes: 0,
            camp_destination: None,
            camp_remaining_minutes: 0,
            pooled_water_ml: 0.0,
            physiology_target: 0.0,
            command_target: 0.0,
            religion_target: 0.0,
        }
    }

    fn strategic_condition(
        character_id: u64,
        status: &str,
        hunger: f32,
        thirst: f32,
        pain: f32,
        blood_loss: f32,
        fatigue: f32,
    ) -> CharacterStrategicCondition {
        CharacterStrategicCondition {
            character_id,
            morale: 0.0,
            morale_bonus: 0.0,
            morale_bonus_cap: 0.0,
            fervor: 0.0,
            pain,
            blood_loss,
            fear: 0.0,
            fatigue,
            hunger,
            thirst,
            thermal: 0.0,
            wetness_bps: 0,
            thermal_strain: 0,
            food_days: 0.0,
            water_days: 0.0,
            water_capacity_ml: 0,
            incapacitation: hunger + thirst + pain + blood_loss + fatigue,
            check_multiplier: 0.0,
            status: status.into(),
        }
    }

    fn withdrawal_destination() -> TravelDestination {
        TravelDestination {
            id: "ironforge".into(),
            name: "Ironforge".into(),
            description: String::new(),
            summary: None,
            travel_action: "/settlements/ironforge/travel".into(),
            track_action: None,
            tracked: false,
            distance_m: 1_000,
            journey_minutes: 60,
            camp_stop_minutes: Vec::new(),
            camp_forecasts: Vec::new(),
            departure_minute: 0,
            itinerary_total_elapsed_minutes: 60,
            itinerary_segments: Vec::new(),
            round_trip_destination: false,
            case_site_knowledge: None,
            active_contract_destination: false,
            provision_forecast: None,
            terrain_route: None,
            return_terrain_route: None,
            route_fallback: true,
        }
    }

    #[test]
    fn autoresolve_stays_on_the_enemy_lifecycle_except_while_requesting_approval() {
        let enemy = "/locations/case-site/case-site-1/enemy";
        assert_eq!(
            redirect_location(autoresolve_redirect::<()>(
                Some("case-site-1"),
                Ok(PartyActionOutcome::Executed),
            )),
            enemy,
        );
        assert_eq!(
            redirect_location(autoresolve_redirect::<()>(Some("case-site-1"), Err(()))),
            enemy,
        );
        assert_eq!(
            redirect_location(autoresolve_redirect::<()>(
                Some("case-site-1"),
                Ok(PartyActionOutcome::Requested),
            )),
            "/?party-requested=autoresolve",
        );
    }

    #[test]
    fn case_site_travel_errors_are_safe_and_actionable() {
        assert_eq!(
            safe_case_site_travel_error("An incapacitated member cannot act"),
            "An incapacitated party member must recover before the party can travel."
        );
        assert_eq!(
            safe_case_site_travel_error("private canonical site mismatch: site:secret"),
            "The exact destination or the party's travel readiness changed. Review the journal before trying again."
        );
        assert!(!safe_case_site_travel_error("site:secret").contains("site:secret"));
    }

    #[test]
    fn generated_location_is_contract_free_but_manual_location_is_not() {
        let generated = case_site(true, true);
        let generated_presentation =
            case_site_page_presentation(&generated, None).expect("generated presentation");
        assert_eq!(generated_presentation.title, "Travellers have gone missing");
        assert!(!generated_presentation.allow_tactical_combat);
        assert!(case_site_combat_permitted(
            &generated, None, None, true, true
        ));
        assert!(!case_site_combat_permitted(
            &generated, None, None, true, false
        ));

        let evidence_site = case_site(true, false);
        assert!(!case_site_combat_permitted(
            &evidence_site,
            None,
            None,
            true,
            true
        ));

        let manual = case_site(false, false);
        assert!(case_site_page_presentation(&manual, None).is_none());
        let contract = accepted_contract();
        let manual_presentation =
            case_site_page_presentation(&manual, Some(&contract)).expect("legacy presentation");
        assert_eq!(manual_presentation.title, "Legacy bounty");
        assert!(manual_presentation.allow_tactical_combat);
        assert!(case_site_combat_permitted(
            &manual,
            Some(&contract),
            Some("contract:one"),
            true,
            true,
        ));
        assert!(!case_site_combat_permitted(
            &manual,
            Some(&contract),
            Some("contract:other"),
            true,
            true,
        ));
    }

    #[test]
    fn generated_noncombat_completion_does_not_require_a_battle_result() {
        let mut site = case_site(true, false);
        site.case_resolved = true;

        assert!(case_site_is_resolved(&site, false));
        site.case_resolved = false;
        assert!(case_site_is_resolved(&site, true));
        assert!(!case_site_is_resolved(&site, false));
    }

    #[test]
    fn onsite_investigation_requires_exact_available_site_action() {
        let actions = onsite_investigation_actions(
            vec![
                onsite_action("site:known", true),
                onsite_action("site:other", true),
                onsite_action("site:known", false),
            ],
            "site:known",
        );
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action_id, "action:inspect");
    }

    #[test]
    fn case_site_guard_requires_matching_character_and_party_occupancy() {
        let character = character_at(Some("site:known"));
        let party = party_at(Some("site:known"));
        assert!(character_and_party_are_at_case_site(
            Some(&character),
            Some(&party),
            "site:known"
        ));

        let elsewhere_character = character_at(Some("site:other"));
        assert!(!character_and_party_are_at_case_site(
            Some(&elsewhere_character),
            Some(&party),
            "site:known"
        ));
        let elsewhere_party = party_at(Some("site:other"));
        assert!(!character_and_party_are_at_case_site(
            Some(&character),
            Some(&elsewhere_party),
            "site:known"
        ));
        assert!(!character_and_party_are_at_case_site(
            None,
            Some(&party),
            "site:known"
        ));
        assert!(!character_and_party_are_at_case_site(
            Some(&character),
            None,
            "site:known"
        ));
    }

    #[test]
    fn recovery_notice_distinguishes_resource_deficits_from_rest_recovery() {
        let member = character_at(Some("site:known"));
        let destination = withdrawal_destination();
        let resource_blocked = case_site_recovery_notice(
            std::slice::from_ref(&member),
            &[strategic_condition(
                member.id,
                "incapacitated",
                4.0,
                26.0,
                0.0,
                0.0,
                0.0,
            )],
            "site:known",
            Some(&destination),
        )
        .expect("resource deficit should produce recovery guidance");
        assert_eq!(resource_blocked.member_names, "Ada");
        assert_eq!(resource_blocked.causes, "hunger, thirst");
        assert!(resource_blocked.resource_blocked);
        assert_eq!(resource_blocked.withdrawal_destination, "Ironforge");
        assert_eq!(
            resource_blocked.withdrawal_href,
            "/locations/case-site/site:known/map?destination=ironforge"
        );

        let rest_recoverable = case_site_recovery_notice(
            std::slice::from_ref(&member),
            &[strategic_condition(
                member.id,
                "incapacitated",
                0.0,
                0.0,
                0.2,
                0.1,
                0.8,
            )],
            "site:known",
            None,
        )
        .expect("recoverable condition should produce guidance");
        assert_eq!(rest_recoverable.causes, "pain, blood loss, fatigue");
        assert!(!rest_recoverable.resource_blocked);
        assert_eq!(
            rest_recoverable.withdrawal_href,
            "/locations/case-site/site:known/map"
        );

        assert!(
            case_site_recovery_notice(
                &[member],
                &[strategic_condition(7, "ready", 0.0, 0.0, 0.0, 0.0, 0.0,)],
                "site:known",
                Some(&destination),
            )
            .is_none()
        );
    }

    #[test]
    fn generated_location_loader_keeps_owner_pin_and_occupancy_boundaries() {
        let source = include_str!("quests.rs");
        let loader = source
            .split("async fn render_quest_location")
            .nth(1)
            .and_then(|tail| tail.split("async fn party_readiness").next())
            .expect("case-site loader");
        assert!(loader.contains("owner_character_id = {character_id}"));
        assert!(loader.contains("case_site_id = {}"));
        assert!(loader.contains("character_and_party_are_at_case_site("));
        assert!(loader.contains("if site.generated_case"));
        assert!(loader.contains("case_site_combat_permitted"));
        assert!(loader.contains("battle.case_site_id.value == site.case_site_id"));
        assert!(!loader.contains("active_contract_id.as_deref() == Some(&presentation"));
        assert!(
            loader.contains("backend_context_characters WHERE location_id = {} AND party_id = {}")
        );
    }

    #[test]
    fn site_sensitive_handlers_use_the_authoritative_character_loader() {
        let source = include_str!("quests.rs");
        let raw_character_query = ["query_one::<", "Character>"].concat();
        let authoritative_loader = ["super::data::", "character(&state, character_id)"].concat();
        assert!(!source.contains(&raw_character_query));
        assert_eq!(source.matches(&authoritative_loader).count(), 5);
        for (start, end) in [
            ("async fn store_battle_loot", "enum QuestLocationTab"),
            (
                "async fn rest_at_quest_location_with_redirect",
                "async fn render_quest_location",
            ),
            ("async fn render_quest_location", "async fn party_readiness"),
            ("async fn autoresolve_quest", "fn autoresolve_redirect"),
        ] {
            let handler = source
                .split(start)
                .nth(1)
                .and_then(|tail| tail.split(end).next())
                .expect("site-sensitive handler");
            assert!(handler.contains(&authoritative_loader));
        }
    }

    #[test]
    fn case_site_map_keeps_settlement_withdrawal_visible_when_party_is_not_ready() {
        let source = include_str!("quests.rs");
        let rendering = source
            .split("let page = match tab")
            .nth(1)
            .and_then(|tail| tail.split("async fn party_readiness").next())
            .expect("case-site page rendering");
        let map = rendering
            .split("QuestLocationTab::Map(selected)")
            .nth(1)
            .and_then(|tail| tail.split("QuestLocationTab::Enemy").next())
            .expect("case-site map branch");
        assert!(map.contains("can_control,"));
        assert!(!map.contains("party_ready,"));
        assert!(map.contains("recovery_notice.as_ref()"));
    }

    #[test]
    fn autoresolve_url_site_must_match_authoritative_character_occupancy() {
        let source = include_str!("quests.rs");
        let handler = source
            .split("async fn autoresolve_quest")
            .nth(1)
            .and_then(|tail| tail.split("fn autoresolve_redirect").next())
            .expect("autoresolve handler");
        assert!(handler.contains("Path(id): Path<String>"));
        assert!(handler.contains("selected_case_site_id.as_deref() != Some(id.as_str())"));
        assert!(handler.contains("character.current_case_site_id"));
        let authoritative_loader = ["super::data::", "character(&state, character_id)"].concat();
        assert_eq!(handler.matches(&authoritative_loader).count(), 2);
        assert!(!handler.contains("Path(_id)"));
    }
}
