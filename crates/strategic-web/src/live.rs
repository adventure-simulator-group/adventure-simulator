//! SpacetimeDB-driven invalidation and Datastar SSE delivery.
//!
//! The browser never connects to SpacetimeDB directly. This process maintains
//! one subscription, coalesces table changes into revisions, and lets each
//! authenticated browser stream receive a small server-rendered patch.

use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use adventuresim_stdb_client::spacetimedb_sdk::{DbContext, Table, TableWithPrimaryKey};
use adventuresim_stdb_client::{
    DbConnection, battle_loot_item_table::BattleLootItemTableAccess,
    battle_participant_table::BattleParticipantTableAccess,
    battle_result_table::BattleResultTableAccess,
    character_attributes_table::CharacterAttributesTableAccess,
    character_capability_table::CharacterCapabilityTableAccess,
    character_equip_table::CharacterEquipTableAccess,
    character_limbs_table::CharacterLimbsTableAccess,
    character_skills_table::CharacterSkillsTableAccess,
    character_stats_table::CharacterStatsTableAccess, character_table::CharacterTableAccess,
    character_training_schedule_table::CharacterTrainingScheduleTableAccess,
    inventory_item_table::InventoryItemTableAccess,
    inventory_quantity_target_table::InventoryQuantityTargetTableAccess,
    local_chat_message_table::LocalChatMessageTableAccess,
    party_action_request_table::PartyActionRequestTableAccess,
    party_inventory_item_table::PartyInventoryItemTableAccess,
    party_inventory_state_table::PartyInventoryStateTableAccess,
    party_join_request_table::PartyJoinRequestTableAccess,
    party_leader_vote_table::PartyLeaderVoteTableAccess,
    party_member_table::PartyMemberTableAccess,
    party_recruitment_role_table::PartyRecruitmentRoleTableAccess,
    party_stake_table::PartyStakeTableAccess, party_table::PartyTableAccess,
    quest_issuer_table::QuestIssuerTableAccess, quest_table::QuestTableAccess,
    saved_recruitment_role_table::SavedRecruitmentRoleTableAccess,
    tactical_server_request_table::TacticalServerRequestTableAccess,
    tactical_server_table::TacticalServerTableAccess,
};
use axum::{
    Json, Router,
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
};
use futures_util::{Stream, StreamExt, stream};
use maud::html;
use serde::Serialize;
use tokio::sync::broadcast;

use crate::{routes::AppState, session::Session, spacetimedb::Character};

struct LiveInner {
    revision: AtomicU64,
    invalidation_pending: AtomicBool,
    changes: broadcast::Sender<u64>,
    runtime: tokio::runtime::Handle,
    // Keeping the connection alive also keeps its WebSocket subscription alive.
    _connection: DbConnection,
}

#[derive(Clone)]
pub struct LiveState(Arc<LiveInner>);

impl LiveState {
    pub fn connect(host: &str, database: &str, token: Option<String>) -> anyhow::Result<Self> {
        let (changes, _) = broadcast::channel(64);
        let connection = DbConnection::builder()
            .with_uri(host)
            .with_module_name(database)
            .with_token(token)
            .on_connect(move |_ctx, identity, _| {
                tracing::info!(%identity, "live SpacetimeDB subscription connected");
            })
            .on_connect_error(
                |_ctx, error| tracing::error!(%error, "live SpacetimeDB connection failed"),
            )
            .on_disconnect(|_ctx, error| {
                tracing::warn!(?error, "live SpacetimeDB subscription disconnected")
            })
            .build()?;

        let state = Self(Arc::new(LiveInner {
            revision: AtomicU64::new(1),
            invalidation_pending: AtomicBool::new(false),
            changes,
            runtime: tokio::runtime::Handle::current(),
            _connection: connection,
        }));

        macro_rules! invalidate_on_changes {
            ($table:expr) => {{
                let live = state.clone();
                $table.on_insert(move |_, _| live.invalidate());
                let live = state.clone();
                $table.on_update(move |_, _, _| live.invalidate());
                let live = state.clone();
                $table.on_delete(move |_, _| live.invalidate());
            }};
        }
        macro_rules! invalidate_on_insert_or_delete {
            ($table:expr) => {{
                let live = state.clone();
                $table.on_insert(move |_, _| live.invalidate());
                let live = state.clone();
                $table.on_delete(move |_, _| live.invalidate());
            }};
        }

        // These tables cover location/navigation, party state and requests,
        // recruitment, quest state, local conversations, and mission readiness.
        invalidate_on_changes!(state.0._connection.db.character());
        invalidate_on_insert_or_delete!(state.0._connection.db.character_attributes());
        invalidate_on_insert_or_delete!(state.0._connection.db.character_stats());
        invalidate_on_insert_or_delete!(state.0._connection.db.character_skills());
        invalidate_on_insert_or_delete!(state.0._connection.db.character_limbs());
        invalidate_on_changes!(state.0._connection.db.character_training_schedule());
        invalidate_on_changes!(state.0._connection.db.party());
        invalidate_on_changes!(state.0._connection.db.party_member());
        invalidate_on_changes!(state.0._connection.db.party_action_request());
        invalidate_on_changes!(state.0._connection.db.party_join_request());
        invalidate_on_changes!(state.0._connection.db.party_leader_vote());
        invalidate_on_changes!(state.0._connection.db.party_recruitment_role());
        invalidate_on_changes!(state.0._connection.db.saved_recruitment_role());
        invalidate_on_changes!(state.0._connection.db.inventory_item());
        invalidate_on_changes!(state.0._connection.db.inventory_quantity_target());
        invalidate_on_changes!(state.0._connection.db.party_inventory_item());
        invalidate_on_changes!(state.0._connection.db.party_inventory_state());
        invalidate_on_changes!(state.0._connection.db.party_stake());
        invalidate_on_insert_or_delete!(state.0._connection.db.character_equip());
        invalidate_on_changes!(state.0._connection.db.character_capability());
        invalidate_on_changes!(state.0._connection.db.quest());
        invalidate_on_changes!(state.0._connection.db.quest_issuer());
        invalidate_on_changes!(state.0._connection.db.local_chat_message());
        invalidate_on_changes!(state.0._connection.db.battle_result());
        invalidate_on_changes!(state.0._connection.db.battle_loot_item());
        invalidate_on_changes!(state.0._connection.db.battle_participant());
        invalidate_on_changes!(state.0._connection.db.tactical_server_request());
        invalidate_on_changes!(state.0._connection.db.tactical_server());

        state
            .0
            ._connection
            .subscription_builder()
            .on_applied({
                let live = state.clone();
                move |_| {
                    tracing::info!("live SpacetimeDB subscription applied");
                    live.invalidate();
                }
            })
            .on_error(|_, error| tracing::error!(%error, "live SpacetimeDB subscription error"))
            .subscribe_to_all_tables();
        state.0._connection.run_threaded();
        Ok(state)
    }

    fn invalidate(&self) {
        if self.0.invalidation_pending.swap(true, Ordering::AcqRel) {
            return;
        }
        let live = self.clone();
        self.0.runtime.spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let revision = live.0.revision.fetch_add(1, Ordering::Relaxed) + 1;
            live.0.invalidation_pending.store(false, Ordering::Release);
            let _ = live.0.changes.send(revision);
        });
    }

    fn subscribe(&self) -> broadcast::Receiver<u64> {
        self.0.changes.subscribe()
    }

    fn revision(&self) -> u64 {
        self.0.revision.load(Ordering::Relaxed)
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/live", get(stream))
        .route("/api/live/navigation", get(navigation))
}

fn revision_patch(revision: u64) -> Event {
    let markup = html! {
        span id="strategic-live-revision" data-live-revision=(revision) hidden {}
    };
    Event::default()
        .event("datastar-patch-elements")
        .data(format!("elements {}", markup.into_string()))
}

async fn stream(
    State(state): State<AppState>,
    _session: Session,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let revision = state.live.revision();
    let initial = stream::iter([Ok(revision_patch(revision))]);
    let updates = stream::unfold(state.live.subscribe(), |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(revision) => return Some((Ok(revision_patch(revision)), receiver)),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(initial.chain(updates)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

#[derive(Serialize)]
struct NavigationState {
    kind: Option<&'static str>,
    id: Option<String>,
    path: String,
}

async fn navigation(State(state): State<AppState>, session: Session) -> Json<NavigationState> {
    let character = match session.character_id_u64() {
        Some(id) => state
            .db
            .query::<Character>(&format!("SELECT * FROM character WHERE id = {id}"))
            .await
            .ok()
            .and_then(|rows| rows.into_iter().next()),
        None => None,
    };
    let value = match character {
        Some(character) if character.current_quest_location_id.is_some() => {
            let id = character.current_quest_location_id.unwrap();
            NavigationState {
                kind: Some("quest"),
                path: format!("/locations/quest/{id}"),
                id: Some(id),
            }
        }
        Some(character) if character.current_settlement_id.is_some() => {
            let id = character.current_settlement_id.unwrap();
            NavigationState {
                kind: Some("settlement"),
                path: format!("/locations/settlement/{id}"),
                id: Some(id),
            }
        }
        _ => NavigationState {
            kind: None,
            id: None,
            path: "/characters".into(),
        },
    };
    Json(value)
}
