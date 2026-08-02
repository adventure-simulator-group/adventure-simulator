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
    backend_case_battles_table::BackendCaseBattlesTableAccess,
    backend_character_attributes_table::BackendCharacterAttributesTableAccess,
    backend_character_capabilities_table::BackendCharacterCapabilitiesTableAccess,
    backend_character_conditions_table::BackendCharacterConditionsTableAccess,
    backend_character_custodies_table::BackendCharacterCustodiesTableAccess,
    backend_character_deaths_table::BackendCharacterDeathsTableAccess,
    backend_character_limbs_table::BackendCharacterLimbsTableAccess,
    backend_character_morale_sources_table::BackendCharacterMoraleSourcesTableAccess,
    backend_character_needs_table::BackendCharacterNeedsTableAccess,
    backend_character_skills_table::BackendCharacterSkillsTableAccess,
    backend_character_stats_table::BackendCharacterStatsTableAccess,
    backend_character_strategic_conditions_table::BackendCharacterStrategicConditionsTableAccess,
    backend_character_training_schedules_table::BackendCharacterTrainingSchedulesTableAccess,
    backend_characters_table::BackendCharactersTableAccess,
    backend_context_characters_table::BackendContextCharactersTableAccess,
    backend_context_dispositions_table::BackendContextDispositionsTableAccess,
    backend_contracts_table::BackendContractsTableAccess,
    backend_dialogue_events_table::BackendDialogueEventsTableAccess,
    backend_dialogue_participants_table::BackendDialogueParticipantsTableAccess,
    backend_dialogue_prompts_table::BackendDialoguePromptsTableAccess,
    backend_dialogue_sessions_table::BackendDialogueSessionsTableAccess,
    backend_dialogue_topic_options_table::BackendDialogueTopicOptionsTableAccess,
    backend_dialogue_witness_claims_table::BackendDialogueWitnessClaimsTableAccess,
    backend_legal_properties_table::BackendLegalPropertiesTableAccess,
    backend_local_chat_messages_table::BackendLocalChatMessagesTableAccess,
    backend_property_events_table::BackendPropertyEventsTableAccess,
    battle_loot_item_table::BattleLootItemTableAccess,
    battle_participant_table::BattleParticipantTableAccess,
    battle_result_table::BattleResultTableAccess,
    character_equipped_item_table::CharacterEquippedItemTableAccess,
    character_filth_table::CharacterFilthTableAccess,
    character_settlement_reputation_table::CharacterSettlementReputationTableAccess,
    equipment_occupancy_table::EquipmentOccupancyTableAccess, food_lot_table::FoodLotTableAccess,
    inventory_item_amount_table::InventoryItemAmountTableAccess,
    inventory_item_table::InventoryItemTableAccess,
    inventory_quantity_target_table::InventoryQuantityTargetTableAccess,
    item_condition_table::ItemConditionTableAccess, limb_injury_table::LimbInjuryTableAccess,
    morale_event_table::MoraleEventTableAccess,
    organization_membership_table::OrganizationMembershipTableAccess,
    organization_presentation_table::OrganizationPresentationTableAccess,
    party_action_request_table::PartyActionRequestTableAccess,
    party_inventory_item_table::PartyInventoryItemTableAccess,
    party_inventory_state_table::PartyInventoryStateTableAccess,
    party_item_amount_table::PartyItemAmountTableAccess,
    party_join_request_table::PartyJoinRequestTableAccess,
    party_journey_itinerary_table::PartyJourneyItineraryTableAccess,
    party_journey_table::PartyJourneyTableAccess,
    party_leader_vote_table::PartyLeaderVoteTableAccess,
    party_member_table::PartyMemberTableAccess,
    party_recruitment_role_table::PartyRecruitmentRoleTableAccess,
    party_stake_table::PartyStakeTableAccess, party_table::PartyTableAccess,
    recruitment_offer_table::RecruitmentOfferTableAccess,
    religious_demand_table::ReligiousDemandTableAccess, repair_order_table::RepairOrderTableAccess,
    retained_projectile_table::RetainedProjectileTableAccess,
    saved_recruitment_role_table::SavedRecruitmentRoleTableAccess,
    settlement_alias_table::SettlementAliasTableAccess,
    settlement_description_table::SettlementDescriptionTableAccess,
    settlement_outbreak_table::SettlementOutbreakTableAccess,
    settlement_smith_table::SettlementSmithTableAccess,
    strategic_encounter_table::StrategicEncounterTableAccess,
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
    spacetimedb::{
        BackendCharacterCaseSiteLocation as HttpBackendCharacterCaseSiteLocation, Character, Party,
        sql_string_literal,
    },
};

/// Tables subscribed by the strategic read cache. Keep this list explicit:
/// large immutable world/import tables and item definitions are read on demand.
pub const STRATEGIC_CACHE_SUBSCRIPTIONS: &[&str] = &[
    "backend_characters",
    "backend_context_characters",
    "backend_context_dispositions",
    "backend_character_custodies",
    "backend_legal_properties",
    "backend_property_events",
    "backend_character_case_site_locations",
    "backend_character_attributes",
    "backend_character_stats",
    "backend_character_skills",
    "backend_character_limbs",
    "limb_injury",
    "retained_projectile",
    "backend_character_training_schedules",
    "organization_membership",
    "organization_presentation",
    "party",
    "party_journey",
    "party_journey_itinerary",
    "party_member",
    "party_action_request",
    "party_join_request",
    "party_leader_vote",
    "party_recruitment_role",
    "saved_recruitment_role",
    "settlement_alias",
    "settlement_description",
    "inventory_item",
    "inventory_item_amount",
    "food_lot",
    "item_condition",
    "repair_order",
    "settlement_smith",
    "backend_character_times",
    "inventory_quantity_target",
    "party_inventory_item",
    "party_item_amount",
    "party_inventory_state",
    "party_stake",
    "character_equipped_item",
    "equipment_occupancy",
    "character_filth",
    "backend_character_capabilities",
    "backend_character_conditions",
    "backend_character_needs",
    "backend_character_strategic_conditions",
    "backend_character_deaths",
    "backend_character_morale_sources",
    "character_settlement_reputation",
    "morale_event",
    "religious_demand",
    "recruitment_offer",
    "strategic_encounter",
    "backend_contracts",
    "backend_local_chat_messages",
    "backend_dialogue_sessions",
    "backend_dialogue_participants",
    "backend_dialogue_events",
    "backend_dialogue_witness_claims",
    "backend_dialogue_prompts",
    "backend_dialogue_topic_options",
    "battle_result",
    "backend_case_battles",
    "autoresolve_report",
    "battle_loot_item",
    "battle_participant",
    "tactical_server_request",
    "tactical_server",
    "settlement_outbreak",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheStatus {
    pub ready: bool,
    pub rows: u64,
}

#[derive(Default)]
struct CacheLifecycle {
    ready: AtomicBool,
}

impl CacheLifecycle {
    fn connected(&self) {
        self.ready.store(false, Ordering::Release);
    }

    fn applied(&self) {
        self.ready.store(true, Ordering::Release);
    }

    fn failed(&self) {
        self.ready.store(false, Ordering::Release);
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}

struct LiveInner {
    revision: AtomicU64,
    invalidation_pending: AtomicBool,
    changes: broadcast::Sender<u64>,
    runtime: tokio::runtime::Handle,
    cache_lifecycle: Arc<CacheLifecycle>,
    cache_rows: AtomicU64,
    // Keeping the connection alive also keeps its WebSocket subscription alive.
    _connection: DbConnection,
}

#[derive(Clone)]
pub struct LiveState(Arc<LiveInner>);

impl LiveState {
    pub fn connect(host: &str, database: &str, token: Option<String>) -> anyhow::Result<Self> {
        let (changes, _) = broadcast::channel(64);
        tracing::debug!(tables = ?STRATEGIC_CACHE_SUBSCRIPTIONS, "strategic cache subscription inventory");
        let cache_lifecycle = Arc::new(CacheLifecycle::default());
        let disconnect_lifecycle = cache_lifecycle.clone();
        let connection = DbConnection::builder()
            .with_uri(host)
            .with_database_name(database)
            .with_token(token)
            .on_connect({
                let lifecycle = cache_lifecycle.clone();
                move |_ctx, identity, _| {
                    // A reconnect invalidates completeness until the
                    // subscription is applied again.
                    lifecycle.connected();
                    tracing::info!(%identity, "live SpacetimeDB subscription connected");
                }
            })
            .on_connect_error({
                let lifecycle = cache_lifecycle.clone();
                move |_ctx, error| {
                    lifecycle.failed();
                    tracing::error!(%error, "live SpacetimeDB connection failed");
                }
            })
            .on_disconnect(move |_ctx, error| {
                disconnect_lifecycle.failed();
                tracing::warn!(?error, "live SpacetimeDB subscription disconnected")
            })
            .build()?;

        let state = Self(Arc::new(LiveInner {
            revision: AtomicU64::new(1),
            invalidation_pending: AtomicBool::new(false),
            changes,
            runtime: tokio::runtime::Handle::current(),
            cache_lifecycle,
            cache_rows: AtomicU64::new(0),
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
        macro_rules! invalidate_on_view_changes {
            ($table:expr) => {{
                let live = state.clone();
                $table.on_insert(move |_, _| live.invalidate());
                let live = state.clone();
                $table.on_delete(move |_, _| live.invalidate());
            }};
        }
        // These tables cover location/navigation, party state and requests,
        // recruitment, quest state, local conversations, and mission readiness.
        invalidate_on_view_changes!(state.0._connection.db.backend_characters());
        invalidate_on_view_changes!(state.0._connection.db.backend_context_characters());
        invalidate_on_view_changes!(state.0._connection.db.backend_context_dispositions());
        invalidate_on_view_changes!(state.0._connection.db.backend_character_custodies());
        invalidate_on_view_changes!(state.0._connection.db.backend_legal_properties());
        invalidate_on_view_changes!(state.0._connection.db.backend_property_events());
        invalidate_on_view_changes!(
            state
                .0
                ._connection
                .db
                .backend_character_case_site_locations()
        );
        invalidate_on_view_changes!(state.0._connection.db.backend_character_attributes());
        invalidate_on_view_changes!(state.0._connection.db.backend_character_stats());
        invalidate_on_view_changes!(state.0._connection.db.backend_character_skills());
        invalidate_on_view_changes!(state.0._connection.db.backend_character_limbs());
        invalidate_on_changes!(state.0._connection.db.limb_injury());
        invalidate_on_changes!(state.0._connection.db.retained_projectile());
        invalidate_on_view_changes!(
            state
                .0
                ._connection
                .db
                .backend_character_training_schedules()
        );
        invalidate_on_changes!(state.0._connection.db.organization_membership());
        invalidate_on_changes!(state.0._connection.db.organization_presentation());
        invalidate_on_view_changes!(state.0._connection.db.party());
        invalidate_on_view_changes!(state.0._connection.db.party_journey());
        invalidate_on_changes!(state.0._connection.db.party_journey_itinerary());
        invalidate_on_changes!(state.0._connection.db.party_member());
        invalidate_on_view_changes!(state.0._connection.db.party_action_request());
        invalidate_on_changes!(state.0._connection.db.party_join_request());
        invalidate_on_changes!(state.0._connection.db.party_leader_vote());
        invalidate_on_changes!(state.0._connection.db.party_recruitment_role());
        invalidate_on_changes!(state.0._connection.db.saved_recruitment_role());
        invalidate_on_changes!(state.0._connection.db.settlement_alias());
        invalidate_on_changes!(state.0._connection.db.settlement_description());
        invalidate_on_changes!(state.0._connection.db.inventory_item());
        invalidate_on_changes!(state.0._connection.db.inventory_item_amount());
        invalidate_on_changes!(state.0._connection.db.food_lot());
        invalidate_on_changes!(state.0._connection.db.item_condition());
        invalidate_on_changes!(state.0._connection.db.repair_order());
        invalidate_on_changes!(state.0._connection.db.settlement_smith());
        invalidate_on_view_changes!(state.0._connection.db.backend_character_times());
        invalidate_on_changes!(state.0._connection.db.inventory_quantity_target());
        invalidate_on_changes!(state.0._connection.db.party_inventory_item());
        invalidate_on_changes!(state.0._connection.db.party_item_amount());
        invalidate_on_changes!(state.0._connection.db.party_inventory_state());
        invalidate_on_changes!(state.0._connection.db.party_stake());
        invalidate_on_changes!(state.0._connection.db.character_equipped_item());
        invalidate_on_changes!(state.0._connection.db.equipment_occupancy());
        invalidate_on_changes!(state.0._connection.db.character_filth());
        invalidate_on_view_changes!(state.0._connection.db.backend_character_capabilities());
        invalidate_on_view_changes!(state.0._connection.db.backend_character_conditions());
        invalidate_on_view_changes!(state.0._connection.db.backend_character_needs());
        invalidate_on_view_changes!(
            state
                .0
                ._connection
                .db
                .backend_character_strategic_conditions()
        );
        invalidate_on_view_changes!(state.0._connection.db.backend_character_deaths());
        invalidate_on_view_changes!(state.0._connection.db.backend_character_morale_sources());
        invalidate_on_changes!(state.0._connection.db.character_settlement_reputation());
        invalidate_on_changes!(state.0._connection.db.morale_event());
        invalidate_on_changes!(state.0._connection.db.religious_demand());
        invalidate_on_changes!(state.0._connection.db.recruitment_offer());
        invalidate_on_changes!(state.0._connection.db.strategic_encounter());
        invalidate_on_view_changes!(state.0._connection.db.backend_contracts());
        invalidate_on_view_changes!(state.0._connection.db.backend_local_chat_messages());
        invalidate_on_view_changes!(state.0._connection.db.backend_dialogue_sessions());
        invalidate_on_view_changes!(state.0._connection.db.backend_dialogue_participants());
        invalidate_on_view_changes!(state.0._connection.db.backend_dialogue_events());
        invalidate_on_view_changes!(state.0._connection.db.backend_dialogue_witness_claims());
        invalidate_on_view_changes!(state.0._connection.db.backend_dialogue_prompts());
        invalidate_on_view_changes!(state.0._connection.db.backend_dialogue_topic_options());
        invalidate_on_changes!(state.0._connection.db.battle_result());
        invalidate_on_view_changes!(state.0._connection.db.backend_case_battles());
        invalidate_on_changes!(state.0._connection.db.autoresolve_report());
        invalidate_on_changes!(state.0._connection.db.battle_loot_item());
        invalidate_on_changes!(state.0._connection.db.battle_participant());
        invalidate_on_view_changes!(state.0._connection.db.tactical_server_request());
        invalidate_on_view_changes!(state.0._connection.db.tactical_server());
        invalidate_on_changes!(state.0._connection.db.settlement_outbreak());

        state
            .0
            ._connection
            .subscription_builder()
            .on_applied({
                let live = state.clone();
                move |ctx| {
                    let rows = [
                        ctx.db().backend_characters().count(),
                        ctx.db().backend_character_case_site_locations().count(),
                        ctx.db().party().count(),
                        ctx.db().party_member().count(),
                        ctx.db().party_journey().count(),
                    ]
                    .into_iter()
                    .sum();
                    live.0.cache_rows.store(rows, Ordering::Release);
                    live.0.cache_lifecycle.applied();
                    tracing::info!("live SpacetimeDB subscription applied");
                    live.invalidate();
                }
            })
            .on_error({
                let live = state.clone();
                move |_, error| {
                    live.0.cache_lifecycle.failed();
                    tracing::error!(%error, "live SpacetimeDB subscription error");
                }
            })
            .add_query(|query| query.from.battle_loot_item())
            .add_query(|query| query.from.battle_participant())
            .add_query(|query| query.from.battle_result())
            .add_query(|query| query.from.backend_case_battles())
            .add_query(|query| query.from.autoresolve_report())
            .add_query(|query| query.from.strategic_encounter())
            .add_query(|query| query.from.backend_characters())
            .add_query(|query| query.from.backend_context_characters())
            .add_query(|query| query.from.backend_context_dispositions())
            .add_query(|query| query.from.backend_character_custodies())
            .add_query(|query| query.from.backend_legal_properties())
            .add_query(|query| query.from.backend_property_events())
            .add_query(|query| query.from.backend_character_case_site_locations())
            .add_query(|query| query.from.backend_character_attributes())
            .add_query(|query| query.from.backend_character_capabilities())
            .add_query(|query| query.from.backend_character_conditions())
            .add_query(|query| query.from.character_equipped_item())
            .add_query(|query| query.from.equipment_occupancy())
            .add_query(|query| query.from.character_filth())
            .add_query(|query| query.from.backend_character_limbs())
            .add_query(|query| query.from.backend_character_deaths())
            .add_query(|query| query.from.limb_injury())
            .add_query(|query| query.from.retained_projectile())
            .add_query(|query| query.from.backend_character_morale_sources())
            .add_query(|query| query.from.backend_character_needs())
            .add_query(|query| query.from.character_settlement_reputation())
            .add_query(|query| query.from.backend_character_skills())
            .add_query(|query| query.from.backend_character_stats())
            .add_query(|query| query.from.backend_character_strategic_conditions())
            .add_query(|query| query.from.backend_character_times())
            .add_query(|query| query.from.backend_character_training_schedules())
            .add_query(|query| query.from.organization_membership())
            .add_query(|query| query.from.organization_presentation())
            .add_query(|query| query.from.party_journey())
            .add_query(|query| query.from.party_journey_itinerary())
            .add_query(|query| query.from.inventory_item())
            .add_query(|query| query.from.inventory_item_amount())
            .add_query(|query| query.from.food_lot())
            .add_query(|query| query.from.inventory_quantity_target())
            .add_query(|query| query.from.item_condition())
            .add_query(|query| query.from.backend_local_chat_messages())
            .add_query(|query| query.from.backend_dialogue_sessions())
            .add_query(|query| query.from.backend_dialogue_participants())
            .add_query(|query| query.from.backend_dialogue_events())
            .add_query(|query| query.from.backend_dialogue_witness_claims())
            .add_query(|query| query.from.backend_dialogue_prompts())
            .add_query(|query| query.from.backend_dialogue_topic_options())
            .add_query(|query| query.from.morale_event())
            .add_query(|query| query.from.party())
            .add_query(|query| query.from.party_action_request())
            .add_query(|query| query.from.party_inventory_item())
            .add_query(|query| query.from.party_item_amount())
            .add_query(|query| query.from.party_inventory_state())
            .add_query(|query| query.from.party_join_request())
            .add_query(|query| query.from.party_leader_vote())
            .add_query(|query| query.from.party_member())
            .add_query(|query| query.from.party_recruitment_role())
            .add_query(|query| query.from.recruitment_offer())
            .add_query(|query| query.from.party_stake())
            .add_query(|query| query.from.backend_contracts())
            .add_query(|query| query.from.religious_demand())
            .add_query(|query| query.from.repair_order())
            .add_query(|query| query.from.saved_recruitment_role())
            .add_query(|query| query.from.settlement_alias())
            .add_query(|query| query.from.settlement_description())
            .add_query(|query| query.from.settlement_smith())
            .add_query(|query| query.from.settlement_outbreak())
            .add_query(|query| query.from.tactical_server())
            .add_query(|query| query.from.tactical_server_request())
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

    pub fn cache_status(&self) -> CacheStatus {
        CacheStatus {
            ready: self.0.cache_lifecycle.is_ready(),
            rows: self.0.cache_rows.load(Ordering::Acquire),
        }
    }

    /// Public character rows are safe to read from the shared cache. Private
    /// owner projections intentionally do not have a cache facade.
    pub fn cached_characters(&self) -> Option<Vec<crate::spacetimedb::Character>> {
        self.cache_status().ready.then(|| {
            self.0
                ._connection
                .db
                .backend_characters()
                .iter()
                .map(character_from_sdk)
                .collect()
        })
    }

    pub fn cached_character(&self, id: u64) -> Option<Option<crate::spacetimedb::Character>> {
        self.cache_status().ready.then(|| {
            self.0
                ._connection
                .db
                .backend_characters()
                .iter()
                .find(|character| character.id == id)
                .map(character_from_sdk)
        })
    }

    pub fn cached_party_has_camp(&self, id: &str) -> Option<bool> {
        self.cache_status().ready.then(|| {
            self.0
                ._connection
                .db
                .party()
                .iter()
                .any(|party| party.id == id && party.camp_destination.is_some())
        })
    }
}

fn character_from_sdk(value: adventuresim_stdb_client::Character) -> crate::spacetimedb::Character {
    crate::spacetimedb::Character {
        id: value.id,
        name: value.name,
        xp: value.xp,
        level: value.level,
        gold: value.gold,
        current_settlement_id: value.current_settlement_id,
        current_case_site_id: None,
        party_id: value.party_id,
        age_years: value.age_years,
        alive: value.alive,
        temporary: value.temporary,
        social_notification_count: 0,
        automatic_social_chat_enabled: false,
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
    // Subscribe before taking the baseline. Otherwise an invalidation between
    // the load and subscribe operations can be lost forever by a new stream.
    let receiver = state.live.subscribe();
    let revision = state.live.revision();
    let initial = stream::iter([Ok(revision_patch(revision))]);
    let updates = stream::unfold(receiver, move |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(next_revision) if next_revision > revision => {
                    return Some((Ok(revision_patch(next_revision)), receiver));
                }
                Ok(_) => continue,
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
                "SELECT * FROM backend_characters WHERE id = {character_id}"
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
                .query_one::<Party>(&format!(
                    "SELECT * FROM party WHERE id = {}",
                    sql_string_literal(party_id)
                ))
                .await
                .ok()
                .flatten()
                .is_some_and(|party| party.camp_destination.is_some())
        {
            return Json(NavigationState {
                kind: Some("camp"),
                path: "/camp".into(),
                id: None,
            });
        }
        let current_case_site_id = state
            .db
            .query_one::<HttpBackendCharacterCaseSiteLocation>(&format!(
                "SELECT * FROM backend_character_case_site_locations WHERE character_id = {character_id}"
            ))
            .await
            .ok()
            .flatten()
            .map(|row| row.case_site_id.value);
        if let Some(id) = current_case_site_id {
            return Json(NavigationState {
                kind: Some("case_site"),
                path: format!("/locations/case-site/{id}"),
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

#[cfg(test)]
mod tests {
    use super::{CacheLifecycle, STRATEGIC_CACHE_SUBSCRIPTIONS};

    #[test]
    fn cache_is_unavailable_until_applied_and_after_reconnect_failure() {
        let lifecycle = CacheLifecycle::default();
        assert!(!lifecycle.is_ready());
        lifecycle.applied();
        assert!(lifecycle.is_ready());
        lifecycle.connected();
        assert!(!lifecycle.is_ready());
        lifecycle.applied();
        lifecycle.failed();
        assert!(!lifecycle.is_ready());
    }

    #[test]
    fn subscription_inventory_is_the_explicit_add_query_inventory() {
        let source = include_str!("live.rs");
        for table in STRATEGIC_CACHE_SUBSCRIPTIONS {
            assert!(
                source.contains(&format!("from.{table}()")),
                "{table} is documented but not added to the subscription"
            );
        }
        for excluded in [
            "item",
            "settlement",
            "travel_edge",
            "world_clock",
            "world_data_import",
            "world_node",
        ] {
            assert!(
                !source.contains(&format!("add_query(|query| query.from.{excluded}())")),
                "static table {excluded} must remain on demand"
            );
        }
        assert!(STRATEGIC_CACHE_SUBSCRIPTIONS.contains(&"party_journey_itinerary"));
        // Private projections may be subscribed for live invalidation, but no
        // renderer-facing cache accessor may expose their rows.
        assert!(source.contains("backend_local_chat_messages"));
        assert!(source.contains("backend_dialogue_sessions"));
        assert!(source.contains("backend_dialogue_witness_claims"));
    }

    #[test]
    fn live_navigation_uses_the_canonical_case_site_contract() {
        let source = include_str!("live.rs");
        assert!(source.contains("kind: Some(\"case_site\")"));
        assert!(source.contains("path: format!(\"/locations/case-site/{id}\")"));
        assert!(!source.contains("kind: Some(\"quest\")"));
    }

    #[test]
    fn public_cache_facade_does_not_offer_private_projection_reads() {
        let source = include_str!("live.rs");
        for private_table in [
            "backend_local_chat_messages",
            "backend_dialogue_sessions",
            "backend_dialogue_witness_claims",
            "backend_case_battles",
            "settlement_outbreak",
            "backend_investigation_cases",
        ] {
            assert!(
                !source.contains(&format!("cached_{private_table}")),
                "private projection {private_table} must not have a cache accessor"
            );
        }
    }
}
