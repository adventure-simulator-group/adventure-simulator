use super::{
    AppState, BackendSettlementResidentRow as SettlementResidentRow, SocialActionId, SocialDuration,
};
use crate::spacetimedb::{
    AffinityBand, BackendCharacterRelationshipStatus, CourtshipKind, FamiliarityBand, MoraleBand,
    Settlement, SettlementCategory, SocialChatOutcome, SocialChatTargetKind,
};
use crate::{session::Session, spacetimedb::sql_string_literal};
use adventuresim_core::courtship::{CourtshipRejectionCode, parse_courtship_rejection};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::json;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/dialogue/start", post(start))
        .route("/api/dialogue/{session_id}", get(view))
        .route("/api/dialogue/topic", post(topic))
        .route("/api/dialogue/answer", post(answer))
        .route(
            "/api/dialogue/accept-order-errantry",
            post(accept_order_errantry),
        )
        .route("/api/dialogue/join", post(join))
        .route("/api/dialogue/claim-response", post(witness_approach))
        .route(
            "/api/settlements/{settlement_id}/locations/{location_id}/npcs",
            get(location_npcs),
        )
        .route(
            "/api/settlements/{settlement_id}/locations/{location_id}/npcs/{resident_character_id}/social",
            get(npc_social).post(chat_with_npc),
        )
        .route(
            "/api/settlements/{settlement_id}/locations/{location_id}/npcs/{resident_character_id}/romance/{action}",
            post(npc_romance_action),
        )
}

#[derive(Deserialize)]
struct SessionRow {
    id: String,
    conversation_id: String,
    settlement_id: String,
    catalog_revision: String,
    revision: u64,
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

#[derive(Clone, Deserialize)]
struct WitnessClaimRow {
    event_sequence: u32,
    claim_order: u32,
    challenge_token: String,
    displayed_text: String,
    charm_response: Option<String>,
    command_response: Option<String>,
    bluff_response: Option<String>,
    assessment_direction: String,
    assessment_strength: f32,
    resolved: bool,
    outcome: String,
    affinity_delta: f32,
}

#[derive(Deserialize)]
struct NpcPresenceRow {
    character_id: u64,
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

#[derive(Deserialize)]
struct NpcSocialRelationshipRow {
    #[serde(deserialize_with = "crate::spacetimedb::deserialize_affinity_band")]
    affinity_band: AffinityBand,
    #[serde(deserialize_with = "crate::spacetimedb::deserialize_familiarity_band")]
    familiarity_band: FamiliarityBand,
    #[serde(deserialize_with = "crate::spacetimedb::deserialize_morale_band")]
    morale_band: MoraleBand,
}

fn serialize_affinity_band<S>(value: &AffinityBand, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(match value {
        AffinityBand::Hostile => "hostile",
        AffinityBand::Reserved => "reserved",
        AffinityBand::Warm => "warm",
        AffinityBand::Trusted => "trusted",
    })
}

fn serialize_familiarity_band<S>(value: &FamiliarityBand, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(match value {
        FamiliarityBand::New => "new",
        FamiliarityBand::Known => "known",
        FamiliarityBand::Familiar => "familiar",
        FamiliarityBand::WellKnown => "well_known",
    })
}

fn serialize_morale_band<S>(value: &MoraleBand, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(match value {
        MoraleBand::Uncertain => "uncertain",
        MoraleBand::Distressed => "distressed",
        MoraleBand::Guarded => "guarded",
        MoraleBand::Settled => "settled",
    })
}

fn serialize_optional_social_outcome<S>(
    value: &Option<SocialChatOutcome>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(SocialChatOutcome::Positive) => serializer.serialize_some("positive"),
        Some(SocialChatOutcome::Mixed) => serializer.serialize_some("mixed"),
        Some(SocialChatOutcome::Negative) => serializer.serialize_some("negative"),
        None => serializer.serialize_none(),
    }
}

fn serialize_optional_courtship_kind<S>(
    value: &Option<CourtshipKind>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(CourtshipKind::Formal) => serializer.serialize_some("formal"),
        Some(CourtshipKind::Informal) => serializer.serialize_some("informal"),
        None => serializer.serialize_none(),
    }
}

#[derive(Deserialize)]
struct SocialChatReceiptRow {
    #[serde(deserialize_with = "crate::spacetimedb::deserialize_social_chat_target_kind")]
    target_kind: SocialChatTargetKind,
    target_id: String,
    requested_minutes: u64,
    #[serde(deserialize_with = "crate::spacetimedb::deserialize_social_chat_outcome")]
    outcome: SocialChatOutcome,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RomanceAction {
    FormalCourtship,
    InformalCourtship,
    ScheduleWedding,
    CancelWedding,
}

#[derive(Serialize)]
struct NpcSocialView {
    resident_character_id: String,
    name: String,
    #[serde(serialize_with = "serialize_affinity_band")]
    affinity: AffinityBand,
    #[serde(serialize_with = "serialize_familiarity_band")]
    familiarity: FamiliarityBand,
    #[serde(serialize_with = "serialize_morale_band")]
    morale: MoraleBand,
    #[serde(serialize_with = "serialize_optional_social_outcome")]
    last_outcome: Option<SocialChatOutcome>,
    #[serde(serialize_with = "serialize_optional_courtship_kind")]
    courtship_kind: Option<CourtshipKind>,
    courtship_exposed: bool,
    wedding_countdown_days: Option<u64>,
    romantic_actions: Vec<RomanceAction>,
}

#[derive(Serialize)]
struct NpcRomanceActionResult {
    ok: bool,
    message: String,
    view: NpcSocialView,
}

#[derive(Deserialize)]
struct NpcChatRequest {
    requested_minutes: SocialDuration,
    action_id: SocialActionId,
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
    order_errantry_offer: bool,
}

#[derive(Deserialize)]
struct AcceptOrderErrantryRequest {
    session_id: String,
    action_id: String,
}

#[derive(Serialize)]
struct AcceptOrderErrantryResponse {
    redirect: &'static str,
}

async fn accept_order_errantry(
    State(state): State<AppState>,
    session: Session,
    Json(request): Json<AcceptOrderErrantryRequest>,
) -> Result<Json<AcceptOrderErrantryResponse>, StatusCode> {
    let character_id = session.character_id_u64().ok_or(StatusCode::UNAUTHORIZED)?;
    state
        .db
        .call(
            "accept_order_errantry",
            &[
                json!(character_id),
                json!(request.session_id),
                json!(request.action_id),
            ],
        )
        .await
        .map_err(|error| {
            tracing::warn!(%error, character_id, "Order errantry acceptance rejected");
            StatusCode::UNPROCESSABLE_ENTITY
        })?;
    Ok(Json(AcceptOrderErrantryResponse {
        redirect: "/quests",
    }))
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
    fragment: adventuresim_dialogue::ResolvedFragment,
    source: Option<EditSource>,
    claim: Option<ClaimView>,
}
#[derive(Serialize)]
struct ClaimView {
    challenge_token: String,
    charm_response: Option<String>,
    command_response: Option<String>,
    bluff_response: Option<String>,
    assessment_direction: String,
    assessment_strength: f32,
    resolved: bool,
    outcome: String,
    affinity_delta: f32,
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

fn npc_location_is_navigable(
    profile: &adventuresim_world_schema::SettlementEconomyProfile,
    category: &SettlementCategory,
    settlement_id: &str,
    location_id: &str,
) -> bool {
    let has_keep = matches!(
        category,
        SettlementCategory::Town | SettlementCategory::City | SettlementCategory::Capital
    );
    adventuresim_core::settlement_economy::npc_location_is_navigable(
        profile,
        has_keep,
        settlement_id,
        location_id,
    )
}

fn npc_presence_contains(start_minute: u16, end_minute: u16, minute: u64) -> bool {
    let minute = minute % 1_440;
    let start = u64::from(start_minute);
    let end = u64::from(end_minute);
    if start == end {
        false
    } else if start < end {
        start <= minute && minute < end
    } else {
        minute >= start || minute < end
    }
}

fn npc_matches_location_binding(
    npc: &SettlementResidentRow,
    settlement_id: &str,
    location_id: &str,
    profile: &adventuresim_world_schema::SettlementEconomyProfile,
) -> bool {
    if npc.organization_id.is_empty() {
        return npc.conversation_id != "organization-representative";
    }
    let Some(organization) = adventuresim_core::organization::organization(&npc.organization_id)
    else {
        return false;
    };
    let Some(chapter) = organization.chapter(settlement_id) else {
        return false;
    };
    let expected_id = adventuresim_core::organization::organization_representative_id(
        settlement_id,
        &organization.id,
    );
    adventuresim_core::organization::chapter_effective_location_id(organization, chapter, profile)
        == location_id
        && adventuresim_core::organization::exact_representative_fields_match(
            npc.character_id,
            expected_id,
            &npc.home_settlement_id,
            settlement_id,
            &npc.organization_id,
            &organization.id,
            &npc.conversation_id,
        )
}

#[cfg(test)]
mod npc_navigation_tests {
    use super::{
        AffinityBand, CourtshipKind, CourtshipRejectionCode, FamiliarityBand, MoraleBand,
        NpcChatRequest, NpcSocialView, RomanceAction, SettlementResidentRow, SocialChatOutcome,
        npc_location_is_navigable, npc_matches_location_binding, npc_presence_contains,
        romantic_rejection_message,
    };
    use crate::spacetimedb::SettlementCategory;

    fn npc(id: u64, organization_id: &str, conversation_id: &str) -> SettlementResidentRow {
        SettlementResidentRow {
            character_id: id,
            home_settlement_id: "viabundus-0".into(),
            name: "Greta Test".into(),
            age_band: "adult".into(),
            presentation: "woman".into(),
            height: "average".into(),
            build: "sturdy".into(),
            hair: "brown hair".into(),
            facial_hair: "none visible".into(),
            complexion: "fair".into(),
            visible_features: "work-worn hands".into(),
            clothing: "working clothes".into(),
            profession: "merchant".into(),
            household: "market household".into(),
            local_role: "market steward".into(),
            service_id: "merchants".into(),
            organization_id: organization_id.into(),
            conversation_id: conversation_id.into(),
        }
    }

    #[test]
    fn browser_npc_description_uses_presentation_not_private_sex() {
        let source = include_str!("dialogue.rs");
        let transport = include_str!("mod.rs");
        let row = transport
            .split("pub(crate) struct BackendSettlementResidentRow {")
            .nth(1)
            .and_then(|tail| tail.split_once('}').map(|(body, _)| body))
            .expect("NPC transport row");
        assert!(row.contains("presentation: String"));
        assert!(!row.contains("sex: String"));
        assert!(!row.contains("projection_id:"));
        let endpoint = source
            .rsplit_once("async fn location_npcs(")
            .map(|(_, tail)| tail)
            .and_then(|tail| tail.split("async fn build_view").next())
            .expect("NPC endpoint");
        assert!(endpoint.contains("npc.presentation.to_lowercase()"));
        assert!(endpoint.contains("character_is_alive_as_observed"));
        assert!(endpoint.contains("alive_npc_ids.contains"));
        assert!(!endpoint.contains("npc.sex"));
    }

    #[test]
    fn templeless_settlement_cannot_enumerate_hidden_npcs() {
        let profile = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
        assert!(npc_location_is_navigable(
            &profile,
            &SettlementCategory::Hamlet,
            "fixture-no-orgs",
            "inn"
        ));
        assert!(!npc_location_is_navigable(
            &profile,
            &SettlementCategory::Hamlet,
            "fixture-no-orgs",
            "church"
        ));
        assert!(!npc_location_is_navigable(
            &profile,
            &SettlementCategory::Hamlet,
            "fixture-no-orgs",
            "armoury"
        ));
        assert!(!npc_location_is_navigable(
            &profile,
            &SettlementCategory::Hamlet,
            "fixture-no-orgs",
            "keep"
        ));
    }

    #[test]
    fn chapter_navigation_accepts_only_the_authored_settlement_location_pair() {
        let profile = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
        assert!(npc_location_is_navigable(
            &profile,
            &SettlementCategory::City,
            "viabundus-0",
            "organization-merchant-guild"
        ));
        assert!(!npc_location_is_navigable(
            &profile,
            &SettlementCategory::City,
            "viabundus-0",
            "organization-hunt-pale-lantern"
        ));
        assert!(!npc_location_is_navigable(
            &profile,
            &SettlementCategory::City,
            "viabundus-0",
            "organization-not-authored"
        ));
    }

    #[test]
    fn service_location_accepts_default_visitor_and_only_exact_local_representatives() {
        let mut profile = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
        profile.services = vec![adventuresim_world_schema::SettlementService::Market];
        let representative_id = adventuresim_core::organization::organization_representative_id(
            "viabundus-0",
            "merchant_guild",
        );
        let provider = npc(101, "", "service-professions");
        let visitor = npc(102, "", "local-resident");
        let representative = npc(
            representative_id,
            "merchant_guild",
            "organization-representative",
        );
        assert!(npc_matches_location_binding(
            &provider,
            "viabundus-0",
            "market",
            &profile
        ));
        assert!(npc_matches_location_binding(
            &visitor,
            "viabundus-0",
            "market",
            &profile
        ));
        assert!(npc_matches_location_binding(
            &representative,
            "viabundus-0",
            "market",
            &profile
        ));

        for spoofed in [
            npc(101, "merchant_guild", "organization-representative"),
            npc(
                representative_id,
                "weaponsmith_guild",
                "organization-representative",
            ),
            npc(representative_id, "", "organization-representative"),
        ] {
            assert!(!npc_matches_location_binding(
                &spoofed,
                "viabundus-0",
                "market",
                &profile
            ));
        }
        let mut wrong_settlement = representative;
        wrong_settlement.home_settlement_id = "viabundus-2337".into();
        assert!(!npc_matches_location_binding(
            &wrong_settlement,
            "viabundus-0",
            "market",
            &profile
        ));
    }

    #[test]
    fn conversation_projection_does_not_reappend_problem_referrals() {
        let source = include_str!("dialogue.rs");
        let build_view = source
            .rsplit_once("async fn build_view(")
            .unwrap()
            .1
            .split("struct StartRequest")
            .next()
            .unwrap();
        assert!(build_view.contains("backend_dialogue_events"));
        assert!(!build_view.contains("backend_local_problem_rumors"));
        assert!(!build_view.contains("events.push"));
    }

    #[test]
    fn browser_presence_check_supports_wrapped_daily_windows() {
        assert!(npc_presence_contains(1_200, 120, 1_380));
        assert!(npc_presence_contains(1_200, 120, 60));
        assert!(!npc_presence_contains(1_200, 120, 600));
        assert!(!npc_presence_contains(480, 1_020, 1_020));
    }

    #[test]
    fn social_transport_serializes_closed_types_as_stable_wire_discriminants() {
        let view = NpcSocialView {
            resident_character_id: "42".into(),
            name: "Anna".into(),
            affinity: AffinityBand::Trusted,
            familiarity: FamiliarityBand::WellKnown,
            morale: MoraleBand::Settled,
            last_outcome: Some(SocialChatOutcome::Positive),
            courtship_kind: Some(CourtshipKind::Formal),
            courtship_exposed: false,
            wedding_countdown_days: None,
            romantic_actions: vec![RomanceAction::ScheduleWedding],
        };
        let value = serde_json::to_value(view).unwrap();
        assert_eq!(value["affinity"], "trusted");
        assert_eq!(value["familiarity"], "well_known");
        assert_eq!(value["morale"], "settled");
        assert_eq!(value["last_outcome"], "positive");
        assert_eq!(value["courtship_kind"], "formal");
        assert_eq!(value["romantic_actions"][0], "schedule_wedding");
    }

    #[test]
    fn social_request_parsing_enforces_duration_and_action_id_invariants() {
        let valid: NpcChatRequest = serde_json::from_value(serde_json::json!({
            "requested_minutes": 60,
            "action_id": "chat-19af-2"
        }))
        .unwrap();
        assert_eq!(valid.requested_minutes.minutes(), 60);
        assert_eq!(valid.action_id.as_str(), "chat-19af-2");
        assert!(
            serde_json::from_value::<NpcChatRequest>(serde_json::json!({
                "requested_minutes": 17,
                "action_id": "chat-19af-2"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<NpcChatRequest>(serde_json::json!({
                "requested_minutes": 60,
                "action_id": "chat:19af"
            }))
            .is_err()
        );
    }

    #[test]
    fn romantic_rejections_use_stable_codes_instead_of_prose_matching() {
        assert_eq!(
            romantic_rejection_message(CourtshipRejectionCode::FatherApproval),
            "My family would not approve of a formal courtship."
        );
        let source = include_str!("dialogue.rs");
        let mapper = source
            .split("fn romantic_rejection_message")
            .nth(1)
            .and_then(|tail| tail.split("async fn npc_romance_action").next())
            .expect("typed romance rejection mapper");
        assert!(!mapper.contains("contains("));
        assert!(source.contains("parse_courtship_rejection(&error_text)"));
    }

    #[test]
    fn npc_chat_http_retry_checks_receipt_before_current_presence() {
        let source = include_str!("dialogue.rs");
        let handler = source
            .rsplit("async fn chat_with_npc")
            .next()
            .and_then(|tail| tail.split("async fn build_view").next())
            .expect("NPC chat handler");
        let receipt = handler
            .find("backend_social_chat_receipts")
            .expect("receipt lookup");
        let presence = handler
            .find("available_social_npc")
            .expect("presence lookup");
        assert!(receipt < presence);
        assert!(
            handler.contains("receipt.target_kind != SocialChatTargetKind::SettlementResident")
        );
        assert!(handler.contains("receipt.target_id != resident_character_id"));
        assert!(
            handler.contains("receipt.requested_minutes != request.requested_minutes.minutes()")
        );
    }
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
            "SELECT * FROM backend_characters WHERE id = {character_id}"
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if character.current_settlement_id.as_deref() != Some(settlement_id.as_str()) {
        return Err(StatusCode::FORBIDDEN);
    }
    let settlement = state
        .db
        .query_one::<Settlement>(&format!(
            "SELECT * FROM settlement WHERE id = {}",
            sql_string_literal(&settlement_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if !npc_location_is_navigable(
        &settlement.economy,
        &settlement.category,
        &settlement_id,
        &location_id,
    ) {
        return Err(StatusCode::NOT_FOUND);
    }
    let npcs = state
        .db
        .query::<SettlementResidentRow>(&format!(
            "SELECT * FROM backend_settlement_residents WHERE home_settlement_id = {}",
            sql_string_literal(&settlement_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let presences = state
        .db
        .query::<NpcPresenceRow>(&format!(
            "SELECT * FROM settlement_resident_presence WHERE settlement_id = {}",
            sql_string_literal(&settlement_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut alive_npc_ids = std::collections::HashSet::new();
    for npc in &npcs {
        let alive = super::data::character_is_alive_as_observed(
            &state,
            npc.character_id,
            character_id,
        )
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(%error, resident_character_id = npc.character_id, observer_character_id = character_id, "could not project resident life state");
            true
        });
        if alive {
            alive_npc_ids.insert(npc.character_id);
        }
    }
    let minute = state
        .db
        .query_one::<crate::spacetimedb::CharacterTime>(&format!(
            "SELECT * FROM backend_character_times WHERE character_id = {character_id}"
        ))
        .await
        .ok()
        .flatten()
        .map_or(720, |time| time.minutes)
        % 1_440;
    let mut views = presences.into_iter().filter(|presence| presence.settlement_id == settlement_id && presence.location_id == location_id && npc_presence_contains(presence.start_minute, presence.end_minute, minute)).filter_map(|presence| {
        let npc = npcs.iter().find(|npc| {
            npc.character_id == presence.character_id
                && alive_npc_ids.contains(&npc.character_id)
                && npc_matches_location_binding(npc, &settlement_id, &location_id, &settlement.economy)
        })?;
        let facial = if npc.facial_hair == "none visible" { String::new() } else { format!(", with {}", npc.facial_hair) };
        Some(NpcView { id: npc.character_id.to_string(), name: npc.name.clone(), initials: npc.name.split_whitespace().filter_map(|part| part.chars().next()).take(2).collect(), description: format!("{} is a {} {} person with {} presentation, a {} build, {}{}, and a {} complexion. Visible details include {}. They wear {}. Occupation: {}. Household: {}. Local role: {}.", npc.name, npc.height, npc.age_band.to_lowercase(), npc.presentation.to_lowercase(), npc.build, npc.hair, facial, npc.complexion, npc.visible_features, npc.clothing, npc.profession, npc.household, npc.local_role), is_default: presence.is_default })
    }).collect::<Vec<_>>();
    views.sort_by_key(|view| (!view.is_default, view.name.clone()));
    Ok(Json(views))
}

async fn social_npc_in_scope(
    state: &AppState,
    character_id: u64,
    settlement_id: &str,
    location_id: &str,
    resident_character_id: u64,
) -> Result<SettlementResidentRow, StatusCode> {
    let character = state
        .db
        .query_one::<crate::spacetimedb::Character>(&format!(
            "SELECT * FROM backend_characters WHERE id = {character_id}"
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if character.current_settlement_id.as_deref() != Some(settlement_id) {
        return Err(StatusCode::FORBIDDEN);
    }
    let settlement = state
        .db
        .query_one::<Settlement>(&format!(
            "SELECT * FROM settlement WHERE id = {}",
            sql_string_literal(settlement_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if !npc_location_is_navigable(
        &settlement.economy,
        &settlement.category,
        settlement_id,
        location_id,
    ) {
        return Err(StatusCode::NOT_FOUND);
    }
    let npc = state
        .db
        .query_one::<SettlementResidentRow>(&format!(
            "SELECT * FROM backend_settlement_residents WHERE character_id = {resident_character_id}"
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .filter(|npc| {
            npc.home_settlement_id == settlement_id
                && npc_matches_location_binding(
                    npc,
                    settlement_id,
                    location_id,
                    &settlement.economy,
                )
        })
        .ok_or(StatusCode::NOT_FOUND)?;
    if !super::data::character_is_alive_as_observed(
        state,
        resident_character_id,
        character_id,
    )
    .await
    .unwrap_or_else(|error| {
        tracing::warn!(%error, resident_character_id, observer_character_id = character_id, "could not project resident life state");
        true
    }) {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(npc)
}

async fn available_social_npc(
    state: &AppState,
    character_id: u64,
    settlement_id: &str,
    location_id: &str,
    resident_character_id: u64,
) -> Result<SettlementResidentRow, StatusCode> {
    let npc = social_npc_in_scope(
        state,
        character_id,
        settlement_id,
        location_id,
        resident_character_id,
    )
    .await?;
    let minute = state
        .db
        .query_one::<crate::spacetimedb::CharacterTime>(&format!(
            "SELECT * FROM backend_character_times WHERE character_id = {character_id}"
        ))
        .await
        .ok()
        .flatten()
        .map_or(720, |time| time.minutes)
        % 1_440;
    let present = state
        .db
        .query::<NpcPresenceRow>(&format!(
            "SELECT * FROM settlement_resident_presence WHERE character_id = {resident_character_id}"
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .into_iter()
        .any(|presence| {
            presence.settlement_id == settlement_id
                && presence.location_id == location_id
                && npc_presence_contains(presence.start_minute, presence.end_minute, minute)
        });
    present.then_some(npc).ok_or(StatusCode::CONFLICT)
}

async fn npc_social_view(
    state: &AppState,
    character_id: u64,
    npc: SettlementResidentRow,
    last_outcome: Option<SocialChatOutcome>,
) -> Result<NpcSocialView, StatusCode> {
    let relationship = state
        .db
        .query_one::<NpcSocialRelationshipRow>(&format!(
            "SELECT * FROM backend_settlement_resident_relationships WHERE observer_character_id = {character_id} AND resident_character_id = {}",
            npc.character_id
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let status = state
        .db
        .query_one::<BackendCharacterRelationshipStatus>(&format!(
            "SELECT * FROM backend_character_relationship_statuses WHERE character_id = {character_id}"
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .filter(|status| status.character_id == character_id);
    let active_commitment = active_commitment_with(state, character_id, npc.character_id).await?;
    let actor_minute = state
        .db
        .query_one::<crate::spacetimedb::CharacterTime>(&format!(
            "SELECT * FROM backend_character_times WHERE character_id = {character_id}"
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .map_or(0, |time| time.minutes);
    let courting_this_npc = status
        .as_ref()
        .is_some_and(|status| status.courtship_partner_id == Some(npc.character_id));
    let mut romantic_actions = Vec::new();
    if active_commitment.is_some() {
        romantic_actions.push(RomanceAction::CancelWedding);
    } else if courting_this_npc {
        romantic_actions.push(RomanceAction::ScheduleWedding);
    } else if status
        .as_ref()
        .is_none_or(|status| status.spouse_id.is_none() && status.courtship_partner_id.is_none())
    {
        romantic_actions.extend([
            RomanceAction::FormalCourtship,
            RomanceAction::InformalCourtship,
        ]);
    }
    let courtship_kind = courting_this_npc
        .then(|| {
            status
                .as_ref()
                .and_then(|status| status.courtship_kind.clone())
        })
        .flatten();
    let courtship_exposed = courting_this_npc
        && status
            .as_ref()
            .is_some_and(|status| status.courtship_exposed);
    let wedding_countdown_days = active_commitment.as_ref().map(|commitment| {
        commitment
            .wedding_effective_minute
            .unwrap_or(actor_minute)
            .saturating_sub(actor_minute)
            .div_ceil(1_440)
    });
    Ok(NpcSocialView {
        resident_character_id: npc.character_id.to_string(),
        name: npc.name,
        affinity: relationship
            .as_ref()
            .map_or(AffinityBand::Reserved, |row| row.affinity_band),
        familiarity: relationship
            .as_ref()
            .map_or(FamiliarityBand::New, |row| row.familiarity_band),
        morale: relationship
            .as_ref()
            .map_or(MoraleBand::Uncertain, |row| row.morale_band),
        last_outcome,
        courtship_kind,
        courtship_exposed,
        wedding_countdown_days,
        romantic_actions,
    })
}

async fn active_commitment_with(
    state: &AppState,
    actor_id: u64,
    target_id: u64,
) -> Result<Option<BackendCharacterRelationshipStatus>, StatusCode> {
    let status = state
        .db
        .query_one::<BackendCharacterRelationshipStatus>(&format!(
            "SELECT * FROM backend_character_relationship_statuses WHERE character_id = {actor_id}"
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(status.filter(|row| {
        row.character_id == actor_id
            && row.wedding_commitment_id.is_some()
            && row.wedding_partner_id == Some(target_id)
    }))
}

fn romantic_rejection_message(code: CourtshipRejectionCode) -> &'static str {
    match code {
        CourtshipRejectionCode::Affinity => {
            "I care for you, but I am not ready for that relationship."
        }
        CourtshipRejectionCode::FatherApproval => {
            "My family would not approve of a formal courtship."
        }
        CourtshipRejectionCode::FormalRoute => {
            "I cannot agree unless we approach this through the available route."
        }
        CourtshipRejectionCode::MutualAttraction => "I care for you, but not romantically.",
        CourtshipRejectionCode::ExclusiveCommitment => {
            "I cannot make that promise while one of us is already committed."
        }
        CourtshipRejectionCode::AlreadyMarried => {
            "I cannot agree while one of us is already married."
        }
        CourtshipRejectionCode::CoLocation => "We need to be together before we can speak of that.",
        CourtshipRejectionCode::IneligibleCharacter => "I cannot enter that courtship.",
        CourtshipRejectionCode::CloseRelative => "We are too closely related for courtship.",
        CourtshipRejectionCode::ActiveCourtshipRequired => {
            "We must be courting before we can plan a wedding."
        }
        CourtshipRejectionCode::CeremonySettlementRequired => {
            "We must first agree where the ceremony will be held."
        }
        CourtshipRejectionCode::ResidenceRequired => "We need a suitable home before the wedding.",
    }
}

async fn npc_romance_action(
    State(state): State<AppState>,
    Path((settlement_id, location_id, resident_character_id, action)): Path<(
        String,
        String,
        String,
        RomanceAction,
    )>,
    session: Session,
) -> Result<Json<NpcRomanceActionResult>, StatusCode> {
    let character_id = session.character_id_u64().ok_or(StatusCode::UNAUTHORIZED)?;
    let resident_character_id = resident_character_id
        .parse::<u64>()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let npc = available_social_npc(
        &state,
        character_id,
        &settlement_id,
        &location_id,
        resident_character_id,
    )
    .await?;
    let (reducer, args, success) = match action {
        RomanceAction::FormalCourtship => (
            "begin_formal_courtship",
            vec![json!(character_id), json!(resident_character_id)],
            "Yes. Let us seek my family's blessing and proceed openly.",
        ),
        RomanceAction::InformalCourtship => (
            "begin_informal_courtship",
            vec![json!(character_id), json!(resident_character_id)],
            "Yes. We will keep this between ourselves.",
        ),
        RomanceAction::ScheduleWedding => (
            "schedule_wedding",
            vec![json!(character_id), json!(resident_character_id)],
            "Then it is settled. One year from today.",
        ),
        RomanceAction::CancelWedding => {
            let Some(commitment) =
                active_commitment_with(&state, character_id, resident_character_id).await?
            else {
                let view = npc_social_view(&state, character_id, npc, None).await?;
                return Ok(Json(NpcRomanceActionResult {
                    ok: false,
                    message: "There is no wedding between us to cancel.".into(),
                    view,
                }));
            };
            (
                "cancel_wedding",
                vec![
                    json!(character_id),
                    json!(
                        commitment
                            .wedding_commitment_id
                            .expect("projected active commitment has an id")
                    ),
                ],
                "I understand. The wedding will not go forward.",
            )
        }
    };
    let result = state.db.call(reducer, &args).await;
    let (ok, message) = match result {
        Ok(()) => (true, success.to_owned()),
        Err(error) => {
            let error_text = error.to_string();
            if let Some(code) = parse_courtship_rejection(&error_text) {
                (false, romantic_rejection_message(code).to_owned())
            } else {
                tracing::warn!(
                    character_id,
                    resident_character_id,
                    ?action,
                    error = %error_text,
                    "romantic action rejected"
                );
                (false, "I cannot make that promise right now.".into())
            }
        }
    };
    let view = npc_social_view(&state, character_id, npc, None).await?;
    Ok(Json(NpcRomanceActionResult { ok, message, view }))
}

async fn npc_social(
    State(state): State<AppState>,
    Path((settlement_id, location_id, resident_character_id)): Path<(String, String, String)>,
    session: Session,
) -> Result<Json<NpcSocialView>, StatusCode> {
    let character_id = session.character_id_u64().ok_or(StatusCode::UNAUTHORIZED)?;
    let resident_character_id = resident_character_id
        .parse::<u64>()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let npc = available_social_npc(
        &state,
        character_id,
        &settlement_id,
        &location_id,
        resident_character_id,
    )
    .await?;
    Ok(Json(
        npc_social_view(&state, character_id, npc, None).await?,
    ))
}

async fn chat_with_npc(
    State(state): State<AppState>,
    Path((settlement_id, location_id, resident_character_id)): Path<(String, String, String)>,
    session: Session,
    Json(request): Json<NpcChatRequest>,
) -> Result<Json<NpcSocialView>, StatusCode> {
    let character_id = session.character_id_u64().ok_or(StatusCode::UNAUTHORIZED)?;
    let resident_character_id = resident_character_id
        .parse::<u64>()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let npc = social_npc_in_scope(
        &state,
        character_id,
        &settlement_id,
        &location_id,
        resident_character_id,
    )
    .await?;
    let receipt_id = format!("{character_id}:{}", request.action_id.as_str());
    if let Some(receipt) = state
        .db
        .query_one::<SocialChatReceiptRow>(&format!(
            "SELECT * FROM backend_social_chat_receipts WHERE id = {} AND actor_id = {character_id}",
            sql_string_literal(&receipt_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    {
        if receipt.target_kind != SocialChatTargetKind::SettlementResident
            || receipt.target_id != resident_character_id.to_string()
            || receipt.requested_minutes != request.requested_minutes.minutes()
        {
            return Err(StatusCode::CONFLICT);
        }
        return Ok(Json(
            npc_social_view(&state, character_id, npc, Some(receipt.outcome)).await?,
        ));
    }
    let npc = available_social_npc(
        &state,
        character_id,
        &settlement_id,
        &location_id,
        resident_character_id,
    )
    .await?;
    state
        .db
        .call(
            "spend_time_with_settlement_resident",
            &[
                json!(character_id),
                json!(resident_character_id),
                json!(request.requested_minutes.minutes()),
                json!(request.action_id.as_str()),
            ],
        )
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    let receipt = state
        .db
        .query_one::<SocialChatReceiptRow>(&format!(
            "SELECT * FROM backend_social_chat_receipts WHERE id = {} AND actor_id = {character_id}",
            sql_string_literal(&receipt_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(
        npc_social_view(&state, character_id, npc, Some(receipt.outcome)).await?,
    ))
}

fn claim_view(
    event_sequence: u32,
    claim_order: u32,
    displayed_text: &str,
    claims: &[WitnessClaimRow],
) -> Option<ClaimView> {
    let claim = claims.iter().find(|claim| {
        claim.event_sequence == event_sequence
            && claim.claim_order == claim_order
            && claim.displayed_text == displayed_text
    })?;
    Some(ClaimView {
        challenge_token: claim.challenge_token.clone(),
        charm_response: claim.charm_response.clone(),
        command_response: claim.command_response.clone(),
        bluff_response: claim.bluff_response.clone(),
        assessment_direction: claim.assessment_direction.clone(),
        assessment_strength: claim.assessment_strength.clamp(0.0, 1.0),
        resolved: claim.resolved,
        outcome: claim.outcome.clone(),
        affinity_delta: claim.affinity_delta,
    })
}

async fn build_view(
    state: &AppState,
    character_id: u64,
    session_id: &str,
) -> Result<ConversationView, StatusCode> {
    let session = state
        .db
        .query_one::<SessionRow>(&format!(
            "SELECT * FROM backend_dialogue_sessions WHERE id = {} AND owner_character_id = {character_id}",
            sql_string_literal(session_id),
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut participants = state
        .db
        .query::<ParticipantRow>(&format!(
            "SELECT * FROM backend_dialogue_participants WHERE session_id = {} AND owner_character_id = {character_id}",
            sql_string_literal(session_id),
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
    let mut claims = state
        .db
        .query::<WitnessClaimRow>(&format!(
            "SELECT * FROM backend_dialogue_witness_claims WHERE session_id = {} AND observer_character_id = {character_id}",
            sql_string_literal(session_id),
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    claims.sort_by_key(|claim| (claim.event_sequence, claim.claim_order));
    let mut events = state
        .db
        .query::<EventRow>(&format!(
            "SELECT * FROM backend_dialogue_events WHERE session_id = {} AND owner_character_id = {character_id}",
            sql_string_literal(session_id),
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    events.sort_by_key(|event| event.sequence);
    let events: Vec<_> = events
        .into_iter()
        .map(|event| {
            let fragments: Vec<adventuresim_dialogue::ResolvedFragment> =
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
                    .map(|(index, fragment)| {
                        let claim = match &fragment {
                            adventuresim_dialogue::ResolvedFragment::Claim {
                                value,
                                claim_order,
                            } => claim_view(event.sequence, *claim_order, value, &claims),
                            _ => None,
                        };
                        FragmentView {
                            fragment,
                            source: sources.get(index).cloned().flatten().and_then(edit_source),
                            claim,
                        }
                    })
                    .collect(),
            }
        })
        .collect();
    let mut topics = state
        .db
        .query::<TopicRow>(&format!(
            "SELECT * FROM backend_dialogue_topic_options WHERE session_id = {} AND owner_character_id = {character_id}",
            sql_string_literal(session_id),
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
            "SELECT * FROM backend_dialogue_prompts WHERE session_id = {} AND owner_character_id = {character_id}",
            sql_string_literal(session_id),
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
    let expected_order_representative =
        adventuresim_core::organization::organization_representative_id(
            &session.settlement_id,
            "order_saint_george",
        )
        .to_string();
    let order_errantry_offer = session.conversation_id == "organization-representative"
        && participants.iter().any(|participant| {
            participant.character_id.is_none()
                && participant.actor_id == expected_order_representative
        });
    Ok(ConversationView {
        session_id: session.id,
        revision: session.revision,
        catalog_revision: session.catalog_revision,
        participants,
        events,
        topics,
        open_prompt,
        order_errantry_offer,
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
    let npc_actor_id = request
        .npc_actor_id
        .parse::<u64>()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let npc = state
        .db
        .query_one::<SettlementResidentRow>(&format!(
            "SELECT * FROM backend_settlement_residents WHERE character_id = {npc_actor_id}"
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::BAD_REQUEST)?;
    let settlement = state
        .db
        .query_one::<Settlement>(&format!(
            "SELECT * FROM settlement WHERE id = {}",
            sql_string_literal(&npc.home_settlement_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::BAD_REQUEST)?;
    if !npc_matches_location_binding(
        &npc,
        &npc.home_settlement_id,
        &request.location_id,
        &settlement.economy,
    ) {
        return Err(StatusCode::NOT_FOUND);
    }
    if !npc_location_is_navigable(
        &settlement.economy,
        &settlement.category,
        &npc.home_settlement_id,
        &request.location_id,
    ) {
        return Err(StatusCode::NOT_FOUND);
    }
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
        .map_err(|error| {
            tracing::warn!(
                %error,
                character_id,
                npc_actor_id = %request.npc_actor_id,
                location_id = %request.location_id,
                "start_dialogue reducer rejected an NPC encounter"
            );
            StatusCode::CONFLICT
        })?;
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
    Ok(Json(
        build_view(&state, character_id, &request.session_id).await?,
    ))
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

#[derive(Deserialize)]
struct WitnessApproachRequest {
    session_id: String,
    challenge_token: String,
    approach: String,
    action_id: String,
    expected_revision: u64,
}

async fn witness_approach(
    State(state): State<AppState>,
    session: Session,
    Json(request): Json<WitnessApproachRequest>,
) -> Result<Json<ConversationView>, StatusCode> {
    let character_id = session.character_id_u64().ok_or(StatusCode::UNAUTHORIZED)?;
    if !matches!(request.approach.as_str(), "charm" | "command" | "bluff") {
        return Err(StatusCode::BAD_REQUEST);
    }
    state
        .db
        .call(
            "approach_dialogue_witness",
            &[
                json!(character_id),
                json!(&request.session_id),
                json!(&request.challenge_token),
                json!(request.approach),
                json!(request.action_id),
                json!(request.expected_revision),
            ],
        )
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    Ok(Json(
        build_view(&state, character_id, &request.session_id).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(text: &str, order: u32) -> WitnessClaimRow {
        WitnessClaimRow {
            event_sequence: 4,
            claim_order: order,
            challenge_token: format!("opaque-{order}"),
            displayed_text: text.into(),
            charm_response: Some(format!("Charm {order}")),
            command_response: None,
            bluff_response: Some(format!("Bluff {order}")),
            assessment_direction: "likely_true".into(),
            assessment_strength: 0.5,
            resolved: false,
            outcome: String::new(),
            affinity_delta: 0.0,
        }
    }

    #[test]
    fn structured_claim_projection_requires_exact_event_order_and_text() {
        let rows = vec![claim("alone", 0), claim("a creature as tall as a tree", 1)];
        assert!(claim_view(4, 0, "alone", &rows).is_some());
        assert!(claim_view(4, 1, "a creature as tall as a tree", &rows).is_some());
        assert!(claim_view(3, 0, "alone", &rows).is_none());
        assert!(claim_view(4, 1, "alone", &rows).is_none());
        assert!(claim_view(4, 0, "different text", &rows).is_none());
    }
}
