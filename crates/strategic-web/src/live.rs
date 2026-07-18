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
use adventuresim_stdb_client::*;
use adventuresim_stdb_client::{
    DbConnection, autoresolve_report_table::AutoresolveReportTableAccess,
    battle_loot_item_table::BattleLootItemTableAccess,
    battle_participant_table::BattleParticipantTableAccess,
    battle_result_table::BattleResultTableAccess,
    character_attributes_table::CharacterAttributesTableAccess,
    character_capability_table::CharacterCapabilityTableAccess,
    character_condition_table::CharacterConditionTableAccess,
    character_equip_table::CharacterEquipTableAccess,
    character_limbs_table::CharacterLimbsTableAccess,
    character_morale_source_table::CharacterMoraleSourceTableAccess,
    character_needs_table::CharacterNeedsTableAccess,
    character_notoriety_table::CharacterNotorietyTableAccess,
    character_personality_table::CharacterPersonalityTableAccess,
    character_skills_table::CharacterSkillsTableAccess,
    character_stats_table::CharacterStatsTableAccess,
    character_strategic_condition_table::CharacterStrategicConditionTableAccess,
    character_table::CharacterTableAccess,
    character_training_schedule_table::CharacterTrainingScheduleTableAccess,
    disease_notice_table::DiseaseNoticeTableAccess, inventory_item_table::InventoryItemTableAccess,
    inventory_quantity_target_table::InventoryQuantityTargetTableAccess,
    item_condition_table::ItemConditionTableAccess,
    local_chat_message_table::LocalChatMessageTableAccess,
    morale_event_table::MoraleEventTableAccess,
    party_action_request_table::PartyActionRequestTableAccess,
    party_inventory_item_table::PartyInventoryItemTableAccess,
    party_inventory_state_table::PartyInventoryStateTableAccess,
    party_join_request_table::PartyJoinRequestTableAccess,
    party_journey_table::PartyJourneyTableAccess,
    party_leader_vote_table::PartyLeaderVoteTableAccess,
    party_member_table::PartyMemberTableAccess,
    party_recruitment_role_table::PartyRecruitmentRoleTableAccess,
    party_stake_table::PartyStakeTableAccess, party_table::PartyTableAccess,
    quest_issuer_table::QuestIssuerTableAccess, quest_table::QuestTableAccess,
    religious_demand_table::ReligiousDemandTableAccess, repair_order_table::RepairOrderTableAccess,
    saved_recruitment_role_table::SavedRecruitmentRoleTableAccess,
    settlement_alias_table::SettlementAliasTableAccess,
    settlement_description_table::SettlementDescriptionTableAccess,
    settlement_smith_table::SettlementSmithTableAccess,
    settlement_outbreak_table::SettlementOutbreakTableAccess,
    strategic_incident_table::StrategicIncidentTableAccess,
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

use crate::{
    routes::AppState,
    session::Session,
    spacetimedb::{Character, Party},
};

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
            .with_database_name(database)
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
        // These tables cover location/navigation, party state and requests,
        // recruitment, quest state, local conversations, and mission readiness.
        invalidate_on_changes!(state.0._connection.db.character());
        invalidate_on_changes!(state.0._connection.db.character_attributes());
        invalidate_on_changes!(state.0._connection.db.character_stats());
        invalidate_on_changes!(state.0._connection.db.character_skills());
        invalidate_on_changes!(state.0._connection.db.character_limbs());
        invalidate_on_changes!(state.0._connection.db.character_training_schedule());
        invalidate_on_changes!(state.0._connection.db.party());
        invalidate_on_changes!(state.0._connection.db.party_journey());
        invalidate_on_changes!(state.0._connection.db.party_member());
        invalidate_on_changes!(state.0._connection.db.party_action_request());
        invalidate_on_changes!(state.0._connection.db.party_join_request());
        invalidate_on_changes!(state.0._connection.db.party_leader_vote());
        invalidate_on_changes!(state.0._connection.db.party_recruitment_role());
        invalidate_on_changes!(state.0._connection.db.saved_recruitment_role());
        invalidate_on_changes!(state.0._connection.db.settlement_alias());
        invalidate_on_changes!(state.0._connection.db.settlement_description());
        invalidate_on_changes!(state.0._connection.db.inventory_item());
        invalidate_on_changes!(state.0._connection.db.item_condition());
        invalidate_on_changes!(state.0._connection.db.repair_order());
        invalidate_on_changes!(state.0._connection.db.settlement_smith());
        invalidate_on_changes!(state.0._connection.db.character_time());
        invalidate_on_changes!(state.0._connection.db.inventory_quantity_target());
        invalidate_on_changes!(state.0._connection.db.party_inventory_item());
        invalidate_on_changes!(state.0._connection.db.party_inventory_state());
        invalidate_on_changes!(state.0._connection.db.party_stake());
        invalidate_on_changes!(state.0._connection.db.character_equip());
        invalidate_on_changes!(state.0._connection.db.character_capability());
        invalidate_on_changes!(state.0._connection.db.character_condition());
        invalidate_on_changes!(state.0._connection.db.character_needs());
        invalidate_on_changes!(state.0._connection.db.character_strategic_condition());
        invalidate_on_changes!(state.0._connection.db.character_morale_source());
        invalidate_on_changes!(state.0._connection.db.character_personality());
        invalidate_on_changes!(state.0._connection.db.character_notoriety());
        invalidate_on_changes!(state.0._connection.db.morale_event());
        invalidate_on_changes!(state.0._connection.db.disease_notice());
        invalidate_on_changes!(state.0._connection.db.religious_demand());
        invalidate_on_changes!(state.0._connection.db.strategic_incident());
        invalidate_on_changes!(state.0._connection.db.quest());
        invalidate_on_changes!(state.0._connection.db.quest_issuer());
        invalidate_on_changes!(state.0._connection.db.local_chat_message());
        invalidate_on_changes!(state.0._connection.db.battle_result());
        invalidate_on_changes!(state.0._connection.db.autoresolve_report());
        invalidate_on_changes!(state.0._connection.db.battle_loot_item());
        invalidate_on_changes!(state.0._connection.db.battle_participant());
        invalidate_on_changes!(state.0._connection.db.tactical_server_request());
        invalidate_on_changes!(state.0._connection.db.tactical_server());
        invalidate_on_changes!(state.0._connection.db.settlement_outbreak());

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
            .add_query(|query| query.from.battle_loot_item())
            .add_query(|query| query.from.battle_participant())
            .add_query(|query| query.from.battle_result())
            .add_query(|query| query.from.autoresolve_report())
            .add_query(|query| query.from.character())
            .add_query(|query| query.from.character_attributes())
            .add_query(|query| query.from.character_capability())
            .add_query(|query| query.from.character_condition())
            .add_query(|query| query.from.character_equip())
            .add_query(|query| query.from.character_limbs())
            .add_query(|query| query.from.character_morale_source())
            .add_query(|query| query.from.character_personality())
            .add_query(|query| query.from.character_needs())
            .add_query(|query| query.from.character_notoriety())
            .add_query(|query| query.from.character_skills())
            .add_query(|query| query.from.character_stats())
            .add_query(|query| query.from.character_strategic_condition())
            .add_query(|query| query.from.character_time())
            .add_query(|query| query.from.character_training_schedule())
            .add_query(|query| query.from.connected_players())
            .add_query(|query| query.from.disease_notice())
            .add_query(|query| query.from.inventory_item())
            .add_query(|query| query.from.inventory_quantity_target())
            .add_query(|query| query.from.item())
            .add_query(|query| query.from.item_condition())
            .add_query(|query| query.from.local_chat_message())
            .add_query(|query| query.from.morale_event())
            .add_query(|query| query.from.party())
            .add_query(|query| query.from.party_action_request())
            .add_query(|query| query.from.party_inventory_item())
            .add_query(|query| query.from.party_inventory_state())
            .add_query(|query| query.from.party_join_request())
            .add_query(|query| query.from.party_leader_vote())
            .add_query(|query| query.from.party_member())
            .add_query(|query| query.from.party_recruitment_role())
            .add_query(|query| query.from.party_stake())
            .add_query(|query| query.from.quest())
            .add_query(|query| query.from.quest_issuer())
            .add_query(|query| query.from.religious_demand())
            .add_query(|query| query.from.repair_order())
            .add_query(|query| query.from.saved_recruitment_role())
            .add_query(|query| query.from.settlement())
            .add_query(|query| query.from.settlement_alias())
            .add_query(|query| query.from.settlement_description())
            .add_query(|query| query.from.settlement_smith())
            .add_query(|query| query.from.settlement_outbreak())
            .add_query(|query| query.from.strategic_incident())
            .add_query(|query| query.from.tactical_server())
            .add_query(|query| query.from.tactical_server_request())
            .add_query(|query| query.from.travel_edge())
            .add_query(|query| query.from.world_clock())
            .add_query(|query| query.from.world_data_import())
            .add_query(|query| query.from.world_node())
            .subscribe();
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
    let Some(character_id) = session.character_id_u64() else {
        return Json(NavigationState {
            kind: None,
            id: None,
            path: "/characters".into(),
        });
    };

    // Subscription updates can precede visibility through the SQL API. A
    // selected character having neither a location nor a camp is only valid
    // during that short transition, so retry it rather than navigating away.
    for attempt in 0..4 {
        let character = state
            .db
            .query::<Character>(&format!(
                "SELECT * FROM character WHERE id = {character_id}"
            ))
            .await
            .ok()
            .and_then(|rows| rows.into_iter().next());
        let Some(character) = character else {
            break;
        };
        if let Some(party_id) = character.party_id.as_deref()
            && state
                .db
                .query_one::<Party>(&format!("SELECT * FROM party WHERE id = '{party_id}'"))
                .await
                .ok()
                .flatten()
                .is_some_and(|party| party.camp_destination_id.is_some())
        {
            return Json(NavigationState {
                kind: Some("camp"),
                path: "/camp".into(),
                id: None,
            });
        }
        if let Some(id) = character.current_quest_location_id {
            return Json(NavigationState {
                kind: Some("quest"),
                path: format!("/locations/quest/{id}"),
                id: Some(id),
            });
        }
        if let Some(id) = character.current_settlement_id {
            return Json(NavigationState {
                kind: Some("settlement"),
                path: format!("/locations/settlement/{id}"),
                id: Some(id),
            });
        }
        if attempt < 3 {
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }
    Json(NavigationState {
        kind: None,
        id: None,
        path: "/characters".into(),
    })
}
