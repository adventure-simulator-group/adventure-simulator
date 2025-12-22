use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{atomic::AtomicU16, Arc},
};

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::{sse::Event, sse::KeepAlive, Html, IntoResponse, Json, Sse},
    routing::{get, post},
    Router,
};
use async_stream::stream;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use strategic_core::{
    Character, CharacterLifeState, CommitMissionRequest, CreatePartyRequest, InventoryItem,
    JoinPartyRequest, LootBag, PlayerState, Quest, RewardGrant, StartMissionRequest,
    StartMissionResponse,
};
use strategic_db::StrategicDb;
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::{error, info, warn};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Port range for tactical servers
const TACTICAL_PORT_START: u16 = 6000;
const TACTICAL_PORT_END: u16 = 6100;

/// Scene allowlist: maps scene_key -> asset path
fn get_scene_allowlist() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    map.insert("town_a", "assets/TownA.glb");
    map.insert("town_b", "assets/TownB.glb");
    map
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("strategic_api=info".parse().unwrap()),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/strategic".to_string());

    let hmac_secret = std::env::var("HMAC_SECRET").unwrap_or_else(|_| {
        warn!("HMAC_SECRET not set, using default (insecure for production)");
        "dev-secret-change-in-production".to_string()
    });

    let strategic_api_url = std::env::var("STRATEGIC_API_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());

    let db = StrategicDb::connect(&database_url, 5).await?;
    db.ensure_schema().await?;

    let (tx, _rx) = broadcast::channel::<UiEvent>(64);

    let state = AppState {
        db: Arc::new(db),
        tx,
        hmac_secret,
        strategic_api_url,
        next_port: Arc::new(AtomicU16::new(TACTICAL_PORT_START)),
        active_missions: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        // Health
        .route("/health", get(health))
        // Strategic Map UI
        .route("/", get(strategic_map_page))
        // Party endpoints
        .route("/api/party/create", post(create_party))
        .route("/api/party/join", post(join_party))
        .route("/api/party/{id}", get(get_party))
        // Mission endpoints
        .route("/api/mission/start", post(start_mission))
        .route("/api/mission/commit", post(commit_mission))
        // Player state
        .route("/api/me/state", get(get_my_state))
        .route("/api/characters/{id}/state", get(get_character_state))
        // Legacy endpoints (from original strategic-server)
        .route("/quests/{id}", get(get_quest))
        .route("/api/quests", post(upsert_quest_api))
        .route("/api/characters", post(upsert_character_api))
        .route("/characters/{id}/state", get(character_state_html))
        .route("/characters/{id}/events", get(character_events))
        .route("/api/characters/{id}", get(get_character_json))
        .route("/api/characters/{id}/inventory", get(get_inventory_json))
        .route("/api/characters/{id}/items/add", post(add_item))
        .route("/api/characters/{id}/damage", post(damage_character))
        .route("/api/characters/{id}/respawn", post(respawn_character))
        .route("/api/characters/{id}/loot_bags", get(list_loot_bags))
        .route("/api/loot_bags/{id}/claim", post(claim_loot_bag))
        .route(
            "/api/characters/{character_id}/quests/{quest_id}/start",
            post(start_character_quest),
        )
        .route(
            "/api/characters/{character_id}/quests/{quest_id}",
            get(get_character_quest_status),
        )
        .route(
            "/api/characters/{character_id}/quests/{quest_id}/complete",
            post(complete_character_quest),
        )
        .with_state(state)
        .nest_service(
            "/overlay",
            ServeDir::new("ui/datastar_overlay").append_index_html_on_directories(true),
        )
        .nest_service("/assets", ServeDir::new("assets"))
        .layer(
            CorsLayer::new()
                .allow_methods(Any)
                .allow_origin(Any)
                .allow_headers(Any),
        );

    let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
    info!("strategic-api listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Clone)]
struct AppState {
    db: Arc<StrategicDb>,
    tx: broadcast::Sender<UiEvent>,
    hmac_secret: String,
    strategic_api_url: String,
    next_port: Arc<AtomicU16>,
    active_missions: Arc<RwLock<HashMap<String, ActiveMission>>>,
}

struct ActiveMission {
    #[allow(dead_code)]
    port: u16,
    #[allow(dead_code)]
    child: tokio::process::Child,
}

impl AppState {
    fn allocate_port(&self) -> u16 {
        let port = self
            .next_port
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if port >= TACTICAL_PORT_END {
            self.next_port
                .store(TACTICAL_PORT_START, std::sync::atomic::Ordering::SeqCst);
        }
        port
    }

    fn generate_join_token(&self, mission_id: &str, party_id: &str) -> String {
        let mut mac =
            HmacSha256::new_from_slice(self.hmac_secret.as_bytes()).expect("HMAC init failed");
        let payload = format!("{}:{}", mission_id, party_id);
        mac.update(payload.as_bytes());
        let result = mac.finalize();
        let signature = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            result.into_bytes(),
        );
        format!("{}:{}", payload, signature)
    }
}

// ============ Strategic Map UI ============

async fn strategic_map_page() -> Html<&'static str> {
    Html(include_str!("../static/map.html"))
}

// ============ Health ============

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

// ============ Party Endpoints ============

async fn create_party(
    State(state): State<AppState>,
    Json(req): Json<CreatePartyRequest>,
) -> impl IntoResponse {
    let party_id = Uuid::new_v4().to_string();
    match state
        .db
        .create_party(&party_id, &req.name, &req.leader_id)
        .await
    {
        Ok(party) => Json(party).into_response(),
        Err(err) => {
            error!("failed to create party: {err:?}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn join_party(
    State(state): State<AppState>,
    Json(req): Json<JoinPartyRequest>,
) -> impl IntoResponse {
    if let Err(err) = state
        .db
        .join_party(&req.party_id, &req.character_id)
        .await
    {
        error!("failed to join party: {err:?}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    match state.db.get_party(&req.party_id).await {
        Ok(Some(party)) => Json(party).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            error!("failed to get party: {err:?}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn get_party(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match state.db.get_party(&id).await {
        Ok(Some(party)) => Json(party).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            error!("failed to get party: {err:?}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ============ Mission Endpoints ============

async fn start_mission(
    State(state): State<AppState>,
    Json(req): Json<StartMissionRequest>,
) -> impl IntoResponse {
    // Validate scene_key against allowlist
    let allowlist = get_scene_allowlist();
    let Some(asset_path) = allowlist.get(req.scene_key.as_str()) else {
        warn!("invalid scene_key: {}", req.scene_key);
        return (StatusCode::BAD_REQUEST, "invalid scene_key").into_response();
    };

    // Verify party exists
    let party = match state.db.get_party(&req.party_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return (StatusCode::NOT_FOUND, "party not found").into_response(),
        Err(err) => {
            error!("failed to get party: {err:?}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let mission_id = Uuid::new_v4().to_string();
    let port = state.allocate_port();
    let join_token = state.generate_join_token(&mission_id, &party.id);

    // Create mission in DB
    if let Err(err) = state
        .db
        .create_mission(&mission_id, &party.id, &req.scene_key, port, &join_token)
        .await
    {
        error!("failed to create mission: {err:?}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    // Spawn tactical_server process
    let child = match tokio::process::Command::new("cargo")
        .args([
            "run",
            "--bin",
            "tactical-server",
            "--",
            "--port",
            &port.to_string(),
            "--mission-id",
            &mission_id,
            "--scene-key",
            &req.scene_key,
            "--asset-path",
            asset_path,
            "--strategic-api-url",
            &state.strategic_api_url,
            "--hmac-secret",
            &state.hmac_secret,
        ])
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            error!("failed to spawn tactical_server: {err:?}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    info!(
        "spawned tactical_server for mission {} on port {}",
        mission_id, port
    );

    // Track active mission
    {
        let mut missions = state.active_missions.write().await;
        missions.insert(mission_id.clone(), ActiveMission { port, child });
    }

    // Return connection info
    // Note: certificate_digest will be filled in by the tactical server on startup
    // For now, client needs to handle self-signed cert acceptance
    let response = StartMissionResponse {
        mission_id,
        server_addr: "127.0.0.1".to_string(),
        server_port: port,
        join_token,
        certificate_digest: "pending".to_string(), // Would be populated via callback in production
    };

    Json(response).into_response()
}

async fn commit_mission(
    State(state): State<AppState>,
    Json(req): Json<CommitMissionRequest>,
) -> impl IntoResponse {
    match state
        .db
        .commit_mission(&req.mission_id, req.success, req.xp_gained, &req.items_gained)
        .await
    {
        Ok(true) => {
            info!("mission {} committed successfully", req.mission_id);
            // Clean up active mission tracking
            {
                let mut missions = state.active_missions.write().await;
                missions.remove(&req.mission_id);
            }
            StatusCode::OK.into_response()
        }
        Ok(false) => {
            info!("mission {} already committed (idempotent)", req.mission_id);
            StatusCode::OK.into_response()
        }
        Err(err) => {
            error!("failed to commit mission: {err:?}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ============ Player State ============

#[derive(serde::Deserialize)]
struct MeStateQuery {
    character_id: String,
}

async fn get_my_state(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<MeStateQuery>,
) -> impl IntoResponse {
    let character = match state.db.get_character(&query.character_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            error!("failed to get character: {err:?}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let inventory = state
        .db
        .list_inventory(&query.character_id)
        .await
        .unwrap_or_default();

    let party = state
        .db
        .get_party_for_character(&query.character_id)
        .await
        .ok()
        .flatten();

    Json(PlayerState {
        character,
        inventory,
        party,
    })
    .into_response()
}

async fn get_character_state(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let character = match state.db.get_character(&id).await {
        Ok(Some(c)) => c,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            error!("failed to get character: {err:?}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let inventory = state.db.list_inventory(&id).await.unwrap_or_default();

    let party = state.db.get_party_for_character(&id).await.ok().flatten();

    Json(PlayerState {
        character,
        inventory,
        party,
    })
    .into_response()
}

// ============ Legacy Endpoints (from original strategic-server) ============

async fn get_quest(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match state.db.get_quest(&id).await {
        Ok(Some(quest)) => Json(quest).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            error!("failed to fetch quest: {err:?}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn upsert_quest_api(
    State(state): State<AppState>,
    Json(quest): Json<Quest>,
) -> impl IntoResponse {
    if let Err(err) = state.db.upsert_quest(&quest).await {
        error!("failed to upsert quest: {err:?}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn upsert_character_api(
    State(state): State<AppState>,
    Json(character): Json<Character>,
) -> impl IntoResponse {
    if let Err(err) = state.db.upsert_character(&character).await {
        error!("failed to upsert character: {err:?}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    broadcast_character_update(&state, &character.id, Some("Character upserted")).await;
    StatusCode::NO_CONTENT.into_response()
}

async fn character_state_html(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match render_overlay_inner_html(&state.db, &id, None).await {
        Ok(html) => (
            [
                (
                    axum::http::header::CONTENT_TYPE,
                    "text/html; charset=utf-8",
                ),
                (
                    axum::http::HeaderName::from_static("datastar-selector"),
                    "#overlay-root",
                ),
                (
                    axum::http::HeaderName::from_static("datastar-mode"),
                    "inner",
                ),
            ],
            html,
        )
            .into_response(),
        Err(RenderError::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(RenderError::Db(err)) => {
            error!("failed to render character state: {err:?}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn character_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mut rx = state.tx.subscribe();
    let stream = stream! {
        if let Ok(initial) = render_overlay_inner_html(&state.db, &id, Some("Connected")).await {
            yield Ok::<Event, std::convert::Infallible>(datastar_patch_event(&initial));
        }
        while let Ok(evt) = rx.recv().await {
            if evt.scope != UiScope::Character || evt.id != id {
                continue;
            }
            yield Ok::<Event, std::convert::Infallible>(datastar_patch_event(&evt.html));
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn get_character_json(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_character(&id).await {
        Ok(Some(character)) => Json(character).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            error!("failed to get character: {err:?}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn get_inventory_json(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.list_inventory(&id).await {
        Ok(items) => Json(items).into_response(),
        Err(err) => {
            error!("failed to list inventory: {err:?}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn list_loot_bags(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.list_loot_bags(&id).await {
        Ok(bags) => Json(bags).into_response(),
        Err(err) => {
            error!("failed to list loot bags: {err:?}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(serde::Deserialize)]
struct ClaimLootBagRequest {
    character_id: String,
}

async fn claim_loot_bag(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ClaimLootBagRequest>,
) -> impl IntoResponse {
    match state.db.claim_loot_bag(&id, &req.character_id).await {
        Ok(_moved) => {
            broadcast_character_update(&state, &req.character_id, Some("Loot claimed")).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => {
            error!("failed to claim loot bag: {err:?}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(serde::Deserialize)]
struct AddItemRequest {
    item_id: String,
    qty: i32,
}

async fn add_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AddItemRequest>,
) -> impl IntoResponse {
    if req.qty <= 0 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if let Err(err) = state.db.add_item(&id, &req.item_id, req.qty).await {
        error!("failed to add item: {err:?}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    broadcast_character_update(&state, &id, Some("Item added")).await;
    StatusCode::NO_CONTENT.into_response()
}

#[derive(serde::Deserialize)]
struct DamageRequest {
    amount: i32,
    world_pos: Option<[f32; 3]>,
}

async fn damage_character(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<DamageRequest>,
) -> impl IntoResponse {
    let respawn_delay_ms = 5_000;
    match state
        .db
        .apply_damage(&id, req.amount.max(0), respawn_delay_ms, req.world_pos)
        .await
    {
        Ok(character) => {
            broadcast_character_update(&state, &id, Some("Damage applied")).await;
            Json(character).into_response()
        }
        Err(err) => {
            error!("failed to apply damage: {err:?}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn respawn_character(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.respawn(&id).await {
        Ok(character) => {
            broadcast_character_update(&state, &id, Some("Respawn attempt")).await;
            Json(character).into_response()
        }
        Err(err) => {
            error!("failed to respawn: {err:?}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn start_character_quest(
    State(state): State<AppState>,
    Path((character_id, quest_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(err) = state
        .db
        .start_character_quest(&character_id, &quest_id)
        .await
    {
        error!("failed to start character quest: {err:?}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    broadcast_character_update(&state, &character_id, Some("Quest started")).await;
    StatusCode::NO_CONTENT.into_response()
}

async fn get_character_quest_status(
    State(state): State<AppState>,
    Path((character_id, quest_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match state
        .db
        .get_character_quest_status(&character_id, &quest_id)
        .await
    {
        Ok(status) => Json(status.unwrap_or_else(|| "not-started".to_string())).into_response(),
        Err(err) => {
            error!("failed to get character quest status: {err:?}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn complete_character_quest(
    State(state): State<AppState>,
    Path((character_id, quest_id)): Path<(String, String)>,
    body: Bytes,
) -> impl IntoResponse {
    if body.is_empty() {
        if let Err(err) = state
            .db
            .complete_character_quest(&character_id, &quest_id)
            .await
        {
            error!("failed to complete character quest: {err:?}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    } else {
        let rewards: RewardGrant = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        };
        match state
            .db
            .complete_character_quest_with_rewards(&character_id, &quest_id, rewards)
            .await
        {
            Ok(true) => {}
            Ok(false) => return StatusCode::CONFLICT.into_response(),
            Err(err) => {
                error!("failed to complete character quest with rewards: {err:?}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }
    broadcast_character_update(&state, &character_id, Some("Quest completed")).await;
    StatusCode::NO_CONTENT.into_response()
}

// ============ UI Event Broadcasting ============

#[derive(Clone, Debug)]
struct UiEvent {
    scope: UiScope,
    id: String,
    html: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UiScope {
    Character,
}

async fn broadcast_character_update(state: &AppState, character_id: &str, banner: Option<&str>) {
    if let Ok(html) = render_overlay_inner_html(&state.db, character_id, banner).await {
        let _ = state.tx.send(UiEvent {
            scope: UiScope::Character,
            id: character_id.to_string(),
            html,
        });
    }
}

enum RenderError {
    NotFound,
    Db(sqlx::Error),
}

async fn render_overlay_inner_html(
    db: &StrategicDb,
    character_id: &str,
    banner: Option<&str>,
) -> Result<String, RenderError> {
    let character = db
        .get_character(character_id)
        .await
        .map_err(RenderError::Db)?
        .ok_or(RenderError::NotFound)?;
    let inventory = db
        .list_inventory(character_id)
        .await
        .map_err(RenderError::Db)?;
    let quests = db.list_quests().await.map_err(RenderError::Db)?;
    let quest_statuses = db
        .list_character_quest_statuses(character_id)
        .await
        .map_err(RenderError::Db)?;
    let loot_bags = db
        .list_loot_bags(character_id)
        .await
        .map_err(RenderError::Db)?;

    let mut out = String::new();
    out.push_str(&render_banner_html(banner));
    out.push_str(&render_character_html(&character));
    out.push_str(&render_inventory_html(&inventory));
    out.push_str(&render_quests_html(character_id, &quests, &quest_statuses));
    out.push_str(&render_loot_html(&loot_bags));
    Ok(out)
}

fn datastar_patch_event(html: &str) -> Event {
    Event::default()
        .event("datastar-patch-elements")
        .data(format!(
            "selector #overlay-root\nmode inner\nelements {html}"
        ))
}

fn render_banner_html(banner: Option<&str>) -> String {
    let text = banner.unwrap_or("Waiting for updates...");
    format!(
        r#"<div id="overlay-banner" class="banner">{}</div>"#,
        escape_html(text)
    )
}

fn render_character_html(c: &Character) -> String {
    let state = match c.life {
        CharacterLifeState::Alive => "alive",
        CharacterLifeState::Dead => "dead",
    };
    let respawn = c
        .respawn_at_ms
        .map(|ms| ms.to_string())
        .unwrap_or_else(|| "-".to_string());
    format!(
        r#"<div id="overlay-character" class="panel"><div><strong>{}</strong> (<code>{}</code>)</div><div>Status: <strong>{}</strong></div><div>HP: <strong>{}/{}</strong></div><div>Deaths: <strong>{}</strong> - XP: <strong>{}</strong></div><div>Respawn at (ms): <code>{}</code></div></div>"#,
        escape_html(&c.name),
        escape_html(&c.id),
        state,
        c.hp_current,
        c.hp_max,
        c.deaths,
        c.xp,
        respawn
    )
}

fn render_inventory_html(items: &[InventoryItem]) -> String {
    if items.is_empty() {
        return r#"<div id="overlay-inventory" class="panel"><div><strong>Inventory</strong></div><div>(empty)</div></div>"#.to_string();
    }
    let rows = items
        .iter()
        .map(|it| {
            format!(
                r#"<li><code>{}</code> x <strong>{}</strong></li>"#,
                escape_html(&it.item_id),
                it.qty
            )
        })
        .collect::<Vec<_>>()
        .join("");
    format!(
        r#"<div id="overlay-inventory" class="panel"><div><strong>Inventory</strong></div><ul>{}</ul></div>"#,
        rows
    )
}

fn render_quests_html(
    character_id: &str,
    quests: &[Quest],
    statuses: &[(String, String)],
) -> String {
    let status_map: HashMap<&str, &str> = statuses
        .iter()
        .map(|(qid, s)| (qid.as_str(), s.as_str()))
        .collect();

    if quests.is_empty() {
        return r#"<div id="overlay-quests" class="panel"><div><strong>Quests</strong></div><div>(no quests seeded)</div></div>"#.to_string();
    }

    let mut rows = String::new();
    for q in quests {
        let status = status_map
            .get(q.id.as_str())
            .copied()
            .unwrap_or("not-started");
        let accept = format!(
            r#"@post('/api/characters/{}/quests/{}/start')"#,
            escape_html(character_id),
            escape_html(&q.id)
        );
        let complete = format!(
            r#"@post('/api/characters/{}/quests/{}/complete')"#,
            escape_html(character_id),
            escape_html(&q.id)
        );

        rows.push_str(&format!(
            r#"<li><div class="quest-row"><div class="quest-title"><strong>{}</strong> <code>{}</code></div><div class="quest-meta">Status: <strong>{}</strong></div><div class="quest-desc">{}</div><div class="overlay-actions"><button data-on:click="{}">Accept</button><button data-on:click="{}">Complete</button></div></div></li>"#,
            escape_html(&q.title),
            escape_html(&q.id),
            escape_html(status),
            escape_html(&q.description),
            accept,
            complete
        ));
    }

    format!(
        r#"<div id="overlay-quests" class="panel"><div><strong>Quests</strong></div><ul class="quest-list">{}</ul></div>"#,
        rows
    )
}

fn render_loot_html(loot_bags: &[LootBag]) -> String {
    if loot_bags.is_empty() {
        return r#"<div id="overlay-loot" class="panel"><div><strong>Loot</strong></div><div>(no loot bags)</div></div>"#.to_string();
    }

    let mut rows = String::new();
    for bag in loot_bags.iter().take(5) {
        let pos = bag
            .world_pos
            .map(|p| format!("{:.1}, {:.1}, {:.1}", p[0], p[1], p[2]))
            .unwrap_or_else(|| "-".to_string());

        let items = if bag.items.is_empty() {
            "(empty)".to_string()
        } else {
            bag.items
                .iter()
                .map(|it| format!(r#"<code>{}</code>x{}"#, escape_html(&it.item_id), it.qty))
                .collect::<Vec<_>>()
                .join(" - ")
        };

        rows.push_str(&format!(
            r#"<li><div class="loot-row"><div><code>{}</code></div><div>At: <code>{}</code></div><div>{}</div></div></li>"#,
            escape_html(&bag.id),
            escape_html(&pos),
            items
        ));
    }

    format!(
        r#"<div id="overlay-loot" class="panel"><div><strong>Loot Bags</strong> (latest)</div><ul class="loot-list">{}</ul></div>"#,
        rows
    )
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
