use super::AppState;
use crate::spacetimedb::HerbalistExaminationRow;
use crate::{session::Session, spacetimedb::sql_string_literal};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/dialogue/start", post(start))
        .route("/api/dialogue/{session_id}", get(view))
        .route("/api/dialogue/topic", post(topic))
        .route("/api/dialogue/answer", post(answer))
        .route("/api/dialogue/join", post(join))
        .route(
            "/api/settlements/{settlement_id}/locations/{location_id}/npcs",
            get(location_npcs),
        )
}

#[derive(Clone, Deserialize)]
struct SessionRow {
    id: String,
    settlement_id: String,
    location_id: String,
    catalog_revision: String,
    revision: u64,
}
#[derive(Deserialize)]
struct DialogueCharacterPlace {
    current_settlement_id: Option<String>,
}
#[derive(Deserialize)]
struct DialogueCharacterTime {
    minutes: u64,
}
#[derive(Clone, Deserialize, Serialize)]
struct ParticipantRow {
    id: String,
    session_id: String,
    role: String,
    character_id: Option<u64>,
    actor_id: String,
    display_name: String,
}
#[derive(Deserialize)]
struct EventRow {
    sequence: u32,
    speaker_role: String,
    fragments_json: String,
    source_refs_json: String,
}
#[derive(Deserialize)]
struct BackendProblemRumorRow {
    #[serde(rename = "receipt_id")]
    _receipt_id: String,
    character_id: u64,
    session_id: String,
    delivery_text: String,
}
#[derive(Deserialize)]
struct PromptRow {
    id: String,
    mode: String,
    choices_json: String,
    min_choices: u32,
    max_choices: u32,
    state: String,
    source_refs_json: String,
}
#[derive(Deserialize)]
struct TopicRow {
    id: String,
    topic_id: String,
    label: String,
    source_ref_json: String,
}

#[derive(Clone, Deserialize, Serialize)]
struct SettlementNpcRow {
    id: String,
    home_settlement_id: String,
    name: String,
    age_band: String,
    sex: String,
    height: String,
    build: String,
    hair: String,
    facial_hair: String,
    complexion: String,
    visible_features: String,
    clothing: String,
    profession: String,
    household: String,
    local_role: String,
    service_id: String,
    conversation_id: String,
}

#[derive(Deserialize)]
struct NpcPresenceRow {
    npc_id: String,
    settlement_id: String,
    location_id: String,
    start_minute: u16,
    end_minute: u16,
    is_default: bool,
}

#[derive(Serialize)]
struct NpcView {
    id: String,
    name: String,
    initials: String,
    description: String,
    is_default: bool,
}

#[derive(Serialize)]
struct ConversationView {
    session_id: String,
    revision: u64,
    catalog_revision: String,
    participants: Vec<ParticipantRow>,
    events: Vec<EventView>,
    topics: Vec<TopicView>,
    open_prompt: Option<PromptView>,
    examination: Option<ExaminationView>,
}
#[derive(Serialize)]
struct EventView {
    sequence: u32,
    speaker_role: String,
    speaker_name: String,
    speaker_is_player: bool,
    fragments: Vec<FragmentView>,
}
#[derive(Serialize)]
struct FragmentView {
    fragment: adventuresim_dialogue::Fragment,
    source: Option<EditSource>,
}
#[derive(Serialize)]
struct TopicView {
    id: String,
    label: String,
    source: Option<EditSource>,
}
#[derive(Serialize)]
struct PromptView {
    id: String,
    mode: String,
    min_choices: u32,
    max_choices: u32,
    choices: Vec<ChoiceView>,
}
#[derive(Serialize)]
struct ChoiceView {
    id: String,
    label: String,
    source: Option<EditSource>,
}
#[derive(Serialize)]
struct ExaminationView {
    diagnoses: Vec<DiagnosisView>,
    message: String,
}
#[derive(Serialize)]
struct DiagnosisView {
    disease_name: String,
    medication_name: String,
}
#[derive(Serialize)]
struct EditSource {
    file: String,
    line: usize,
    column: usize,
    edit_url: String,
}

fn edit_source(source: adventuresim_dialogue::SourceRef) -> Option<EditSource> {
    let edit_url = adventuresim_dialogue::github_edit_url(
        "adventure-simulator-group/adventure-simulator",
        option_env!("ADVENTURESIM_SOURCE_REF").unwrap_or("main"),
        &source,
    )?;
    Some(EditSource {
        file: source.file,
        line: source.line,
        column: source.column,
        edit_url,
    })
}
async fn location_npcs(
    State(state): State<AppState>,
    Path((settlement_id, location_id)): Path<(String, String)>,
    session: Session,
) -> Result<Json<Vec<NpcView>>, StatusCode> {
    let character_id = session.character_id_u64().ok_or(StatusCode::UNAUTHORIZED)?;
    let character = state
        .db
        .query_one::<crate::spacetimedb::Character>(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if character.current_settlement_id.as_deref() != Some(settlement_id.as_str()) {
        return Err(StatusCode::FORBIDDEN);
    }
    let npcs = state
        .db
        .query::<SettlementNpcRow>(&format!(
            "SELECT * FROM settlement_npc WHERE home_settlement_id = {}",
            sql_string_literal(&settlement_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let presences = state
        .db
        .query::<NpcPresenceRow>(&format!(
            "SELECT * FROM settlement_npc_presence WHERE settlement_id = {}",
            sql_string_literal(&settlement_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let minute = state
        .db
        .query_one::<crate::spacetimedb::CharacterTime>(&format!(
            "SELECT * FROM character_time WHERE character_id = {character_id}"
        ))
        .await
        .ok()
        .flatten()
        .map_or(720, |time| time.minutes)
        % 1_440;
    let mut views = presences.into_iter().filter(|presence| presence.settlement_id == settlement_id && presence.location_id == location_id && u64::from(presence.start_minute) <= minute && minute < u64::from(presence.end_minute)).filter_map(|presence| {
        let npc = npcs.iter().find(|npc| npc.id == presence.npc_id)?;
        let facial = if npc.facial_hair == "none visible" { String::new() } else { format!(", with {}", npc.facial_hair) };
        Some(NpcView { id: npc.id.clone(), name: npc.name.clone(), initials: npc.name.split_whitespace().filter_map(|part| part.chars().next()).take(2).collect(), description: format!("{} is a {} {} {} with a {} build, {}{}, and a {} complexion. Visible details include {}. They wear {}. Occupation: {}. Household: {}. Local role: {}.", npc.name, npc.height, npc.age_band.to_lowercase(), npc.sex.to_lowercase(), npc.build, npc.hair, facial, npc.complexion, npc.visible_features, npc.clothing, npc.profession, npc.household, npc.local_role), is_default: presence.is_default })
    }).collect::<Vec<_>>();
    views.sort_by_key(|view| (!view.is_default, view.name.clone()));
    Ok(Json(views))
}

async fn build_view(
    state: &AppState,
    character_id: u64,
    session_id: &str,
) -> Result<ConversationView, StatusCode> {
    let session = state
        .db
        .query_one::<SessionRow>(&format!(
            "SELECT * FROM dialogue_session WHERE id = {}",
            sql_string_literal(session_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut participants = state
        .db
        .query::<ParticipantRow>(&format!(
            "SELECT * FROM dialogue_participant WHERE session_id = {}",
            sql_string_literal(session_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !participants
        .iter()
        .any(|p| p.character_id == Some(character_id))
    {
        return Err(StatusCode::FORBIDDEN);
    }
    participants.sort_by(|a, b| a.id.cmp(&b.id));
    let names: std::collections::BTreeMap<_, _> = participants
        .iter()
        .map(|p| (p.role.clone(), p.display_name.clone()))
        .collect();
    let mut events = state
        .db
        .query::<EventRow>(&format!(
            "SELECT * FROM dialogue_event WHERE session_id = {}",
            sql_string_literal(session_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    events.sort_by_key(|event| event.sequence);
    let mut events: Vec<_> = events
        .into_iter()
        .map(|event| {
            let fragments: Vec<adventuresim_dialogue::Fragment> =
                serde_json::from_str(&event.fragments_json).unwrap_or_default();
            let sources: Vec<Option<adventuresim_dialogue::SourceRef>> =
                serde_json::from_str(&event.source_refs_json).unwrap_or_default();
            let speaker_is_player = participants
                .iter()
                .find(|p| p.role == event.speaker_role)
                .is_some_and(|p| p.character_id.is_some());
            EventView {
                sequence: event.sequence,
                speaker_name: names
                    .get(&event.speaker_role)
                    .cloned()
                    .unwrap_or_else(|| "Unknown participant".into()),
                speaker_is_player,
                speaker_role: event.speaker_role,
                fragments: fragments
                    .into_iter()
                    .enumerate()
                    .map(|(index, fragment)| FragmentView {
                        fragment,
                        source: sources.get(index).cloned().flatten().and_then(edit_source),
                    })
                    .collect(),
            }
        })
        .collect();
    let rumor_sql = format!(
        "SELECT * FROM backend_local_problem_rumors WHERE character_id = {character_id} AND session_id = {}",
        sql_string_literal(session_id)
    );
    let rumors = state
        .db
        .query::<BackendProblemRumorRow>(&rumor_sql)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let npc_actor = participants
        .iter()
        .find(|p| p.character_id.is_none())
        .map(|p| p.actor_id.clone());
    let place = state
        .db
        .query_one::<DialogueCharacterPlace>(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
        .ok()
        .flatten();
    let time = state
        .db
        .query_one::<DialogueCharacterTime>(&format!(
            "SELECT * FROM character_time WHERE character_id = {character_id}"
        ))
        .await
        .ok()
        .flatten()
        .map_or(0, |r| r.minutes);
    let presence = if let Some(actor) = npc_actor.as_ref() {
        state
            .db
            .query_one::<NpcPresenceRow>(&format!(
                "SELECT * FROM settlement_npc_presence WHERE npc_id = {}",
                sql_string_literal(actor)
            ))
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    let minute_of_day = (time % 1440) as u16;
    let live = place.is_some_and(|p| {
        p.current_settlement_id.as_deref() == Some(session.settlement_id.as_str())
    }) && presence.is_some_and(|p| {
        p.settlement_id == session.settlement_id
            && p.location_id == session.location_id
            && p.start_minute <= minute_of_day
            && minute_of_day < p.end_minute
    });
    if live
        && let Some(rumor) = rumors
            .into_iter()
            .find(|r| r.character_id == character_id && r.session_id == session_id)
    {
        let speaker_role = participants
            .iter()
            .find(|p| p.character_id.is_none())
            .map_or("npc", |p| p.role.as_str())
            .to_owned();
        events.push(EventView {
            sequence: events
                .iter()
                .map(|e| e.sequence)
                .max()
                .map_or(0, |n| n.saturating_add(1)),
            speaker_name: names
                .get(&speaker_role)
                .cloned()
                .unwrap_or_else(|| "Local resident".into()),
            speaker_is_player: false,
            speaker_role,
            fragments: vec![FragmentView {
                fragment: adventuresim_dialogue::Fragment::Text {
                    value: rumor.delivery_text,
                },
                source: None,
            }],
        });
    }
    let mut topics = state
        .db
        .query::<TopicRow>(&format!(
            "SELECT * FROM dialogue_topic_option WHERE session_id = {}",
            sql_string_literal(session_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    topics.sort_by(|a, b| a.id.cmp(&b.id));
    let topics = topics
        .into_iter()
        .map(|topic| TopicView {
            id: topic.topic_id,
            label: topic.label,
            source: serde_json::from_str::<Option<adventuresim_dialogue::SourceRef>>(
                &topic.source_ref_json,
            )
            .ok()
            .flatten()
            .and_then(edit_source),
        })
        .collect();
    let mut prompts = state
        .db
        .query::<PromptRow>(&format!(
            "SELECT * FROM dialogue_prompt WHERE session_id = {}",
            sql_string_literal(session_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    prompts.retain(|prompt| prompt.state == "open");
    prompts.sort_by(|a, b| a.id.cmp(&b.id));
    let open_prompt = prompts.pop().map(|prompt| {
        let choices: Vec<adventuresim_dialogue::Choice> =
            serde_json::from_str(&prompt.choices_json).unwrap_or_default();
        let sources: Vec<Option<adventuresim_dialogue::SourceRef>> =
            serde_json::from_str(&prompt.source_refs_json).unwrap_or_default();
        PromptView {
            id: prompt.id,
            mode: prompt.mode,
            min_choices: prompt.min_choices,
            max_choices: prompt.max_choices,
            choices: choices
                .into_iter()
                .enumerate()
                .map(|(index, choice)| ChoiceView {
                    id: choice.id,
                    label: choice.label,
                    source: sources.get(index).cloned().flatten().and_then(edit_source),
                })
                .collect(),
        }
    });
    Ok(ConversationView {
        session_id: session.id,
        revision: session.revision,
        catalog_revision: session.catalog_revision,
        participants,
        events,
        topics,
        open_prompt,
        examination: None,
    })
}

async fn consume_herbalist_examination(
    state: &AppState,
    character_id: u64,
    view: &ConversationView,
) -> Option<ExaminationView> {
    let actor = view
        .participants
        .iter()
        .find(|participant| participant.actor_id.contains(":herbalist:"))?;
    let npc = state
        .db
        .query_one::<SettlementNpcRow>(&format!(
            "SELECT * FROM settlement_npc WHERE id = {}",
            sql_string_literal(&actor.actor_id)
        ))
        .await
        .ok()
        .flatten()?;
    let settlement_id = npc.home_settlement_id;
    let result = state
        .db
        .query::<HerbalistExaminationRow>(&format!(
            "SELECT * FROM backend_herbalist_examinations WHERE patient_id = {character_id}"
        ))
        .await
        .ok()?
        .into_iter()
        .filter(|row| row.settlement_id == settlement_id)
        .max_by_key(|row| row.id)?;
    let diagnoses = result
        .disease_names
        .iter()
        .zip(&result.medication_names)
        .map(|(disease_name, medication_name)| DiagnosisView {
            disease_name: disease_name.clone(),
            medication_name: medication_name.clone(),
        })
        .collect::<Vec<_>>();
    if let Err(error) = state
        .db
        .call(
            "dismiss_herbalist_examination",
            &[json!(character_id), json!(result.id)],
        )
        .await
    {
        tracing::warn!(%error, character_id, "dialogue herbalist result was not dismissed");
    }
    Some(ExaminationView {
        message: if diagnoses.is_empty() {
            "I am sorry, but I cannot name your illness with confidence. Seek a more skilled physician.".into()
        } else {
            String::new()
        },
        diagnoses,
    })
}

#[derive(Deserialize)]
struct StartRequest {
    npc_actor_id: String,
    location_id: String,
}
async fn start(
    State(state): State<AppState>,
    session: Session,
    Json(request): Json<StartRequest>,
) -> Result<Json<ConversationView>, StatusCode> {
    let character_id = session.character_id_u64().ok_or(StatusCode::UNAUTHORIZED)?;
    let npc = state
        .db
        .query_one::<SettlementNpcRow>(&format!(
            "SELECT * FROM settlement_npc WHERE id = {}",
            sql_string_literal(&request.npc_actor_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::BAD_REQUEST)?;
    let conversation = npc.conversation_id;
    // Selecting an NPC starts a fresh encounter. Historical sessions remain
    // available for prior-interaction facts but never become an indefinite live view.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_micros());
    let session_id = format!("dialogue:{character_id}:{nonce}");
    state
        .db
        .call(
            "start_dialogue",
            &[
                json!(character_id),
                json!(&session_id),
                json!(conversation),
                json!(request.npc_actor_id),
                json!(request.location_id),
                json!(adventuresim_dialogue::CATALOG_DIGEST),
            ],
        )
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    Ok(Json(build_view(&state, character_id, &session_id).await?))
}
async fn view(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    session: Session,
) -> Result<Json<ConversationView>, StatusCode> {
    let character_id = session.character_id_u64().ok_or(StatusCode::UNAUTHORIZED)?;
    Ok(Json(build_view(&state, character_id, &session_id).await?))
}

#[derive(Deserialize)]
struct TopicRequest {
    session_id: String,
    topic_id: String,
    action_id: String,
    expected_revision: u64,
}
async fn topic(
    State(state): State<AppState>,
    session: Session,
    Json(request): Json<TopicRequest>,
) -> Result<Json<ConversationView>, StatusCode> {
    let character_id = session.character_id_u64().ok_or(StatusCode::UNAUTHORIZED)?;
    state
        .db
        .call(
            "choose_dialogue_topic",
            &[
                json!(character_id),
                json!(&request.session_id),
                json!(request.topic_id),
                json!(request.action_id),
                json!(request.expected_revision),
                json!(adventuresim_dialogue::CATALOG_DIGEST),
            ],
        )
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    Ok(Json(
        build_view(&state, character_id, &request.session_id).await?,
    ))
}
#[derive(Deserialize)]
struct AnswerRequest {
    session_id: String,
    prompt_row_id: String,
    choice_ids: Vec<String>,
    action_id: String,
    expected_revision: u64,
}
async fn answer(
    State(state): State<AppState>,
    session: Session,
    Json(request): Json<AnswerRequest>,
) -> Result<Json<ConversationView>, StatusCode> {
    let character_id = session.character_id_u64().ok_or(StatusCode::UNAUTHORIZED)?;
    if !request
        .prompt_row_id
        .starts_with(&format!("{}:prompt:", request.session_id))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    state
        .db
        .call(
            "answer_dialogue_prompt",
            &[
                json!(character_id),
                json!(request.prompt_row_id),
                json!(serde_json::to_string(&request.choice_ids).unwrap()),
                json!(request.action_id),
                json!(request.expected_revision),
                json!(adventuresim_dialogue::CATALOG_DIGEST),
            ],
        )
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    let mut view = build_view(&state, character_id, &request.session_id).await?;
    if request
        .prompt_row_id
        .contains(":prompt:request-examination:")
        && request.choice_ids == ["yes"]
    {
        view.examination = consume_herbalist_examination(&state, character_id, &view).await;
    }
    Ok(Json(view))
}
#[derive(Deserialize)]
struct JoinRequest {
    session_id: String,
    role: String,
    action_id: String,
    expected_revision: u64,
}
async fn join(
    State(state): State<AppState>,
    session: Session,
    Json(request): Json<JoinRequest>,
) -> Result<Json<ConversationView>, StatusCode> {
    let character_id = session.character_id_u64().ok_or(StatusCode::UNAUTHORIZED)?;
    state
        .db
        .call(
            "join_dialogue_session",
            &[
                json!(character_id),
                json!(&request.session_id),
                json!(request.role),
                json!(request.action_id),
                json!(request.expected_revision),
                json!(adventuresim_dialogue::CATALOG_DIGEST),
            ],
        )
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    Ok(Json(
        build_view(&state, character_id, &request.session_id).await?,
    ))
}
