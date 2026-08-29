#[derive(Clone, Debug)]
#[table(accessor = local_chat_message)]
pub struct LocalChatMessage {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub audience_party_id: String,
    pub other_party_id: String,
    pub resident_character_id: Option<u64>,
    pub sender_id: u64,
    pub sender_name: String,
    pub body: String,
    pub created_micros: i64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendLocalChatMessage {
    pub id: u64,
    pub owner_character_id: u64,
    pub conversation_kind: String,
    pub subject_party_id: String,
    pub subject_resident_character_id: String,
    pub sender_id: u64,
    pub sender_name: String,
    pub body: String,
    pub created_micros: i64,
}

fn local_chat_party_viewer_ids(ctx: &ViewContext, party_id: &str) -> Vec<u64> {
    ctx.db
        .party_member()
        .party_id()
        .filter(party_id)
        .map(|member| member.character_id)
        .collect()
}

fn project_local_chat_message(
    row: LocalChatMessage,
    audience_viewers: &[u64],
    other_viewers: &[u64],
) -> Vec<BackendLocalChatMessage> {
    let mut projections = Vec::new();
    let mut push =
        |owner_character_id, conversation_kind, subject_party_id, subject_resident_character_id| {
            projections.push(BackendLocalChatMessage {
                id: row.id,
                owner_character_id,
                conversation_kind,
                subject_party_id,
                subject_resident_character_id,
                sender_id: row.sender_id,
                sender_name: row.sender_name.clone(),
                body: row.body.clone(),
                created_micros: row.created_micros,
            });
        };
    if row.resident_character_id.is_none() {
        for &owner_character_id in audience_viewers {
            push(
                owner_character_id,
                "player".into(),
                row.other_party_id.clone(),
                String::new(),
            );
        }
        if row.other_party_id != row.audience_party_id {
            for &owner_character_id in other_viewers {
                push(
                    owner_character_id,
                    "player".into(),
                    row.audience_party_id.clone(),
                    String::new(),
                );
            }
        }
    } else {
        for &owner_character_id in audience_viewers {
            push(
                owner_character_id,
                "npc".into(),
                String::new(),
                row.resident_character_id.map_or_else(String::new, |id| id.to_string()),
            );
        }
    }
    projections
}

#[view(accessor = backend_local_chat_messages, public)]
pub fn backend_local_chat_messages(ctx: &ViewContext) -> Vec<BackendLocalChatMessage> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .local_chat_message()
        .gateway_bucket()
        .filter(0u8)
        .flat_map(|row| {
            let audience_viewers = local_chat_party_viewer_ids(ctx, &row.audience_party_id);
            let other_viewers = if row.resident_character_id.is_none() {
                local_chat_party_viewer_ids(ctx, &row.other_party_id)
            } else {
                Vec::new()
            };
            project_local_chat_message(row, &audience_viewers, &other_viewers)
        })
        .collect()
}

/// Scripted dialogue is authoritative and intentionally separate from free-form local chat.
#[derive(Clone, Debug)]
#[table(accessor = dialogue_session)]
pub struct DialogueSession {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub conversation_id: String,
    pub catalog_revision: String,
    pub settlement_id: String,
    pub location_id: String,
    pub owner_character_id: u64,
    pub owner_party_id: String,
    pub state: String,
    pub revision: u64,
    pub created_micros: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DialogueSessionState {
    Active,
}

impl DialogueSessionState {
    const fn stable_id(self) -> &'static str {
        match self {
            Self::Active => "active",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "active" => Ok(Self::Active),
            _ => Err("Dialogue session has an unknown state".into()),
        }
    }
}

struct ParsedDialogueSessionId<'a> {
    owner_character_id: u64,
    nonce: &'a str,
}

impl<'a> ParsedDialogueSessionId<'a> {
    fn parse(value: &'a str) -> Result<Self, String> {
        if value.len() > 160 || value.chars().any(char::is_control) {
            return Err("Invalid dialogue session ID".into());
        }
        let mut parts = value.splitn(3, ':');
        let domain = parts.next();
        let owner_character_id = parts.next().and_then(|part| part.parse::<u64>().ok());
        let nonce = parts.next().filter(|part| !part.is_empty());
        match (domain, owner_character_id, nonce) {
            (Some("dialogue"), Some(owner_character_id), Some(nonce)) => Ok(Self {
                owner_character_id,
                nonce,
            }),
            _ => Err("Invalid dialogue session ID".into()),
        }
    }
}

#[derive(Clone, Debug)]
#[table(accessor = dialogue_participant)]
pub struct DialogueParticipant {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub session_id: String,
    pub role: String,
    pub character_id: Option<u64>,
    #[index(btree)]
    pub actor_id: String,
    pub display_name: String,
}

#[derive(Clone, Debug)]
#[table(accessor = dialogue_event)]
pub struct DialogueEvent {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub session_id: String,
    pub sequence: u32,
    pub response_id: String,
    pub speaker_role: String,
    pub fragments_json: String,
    pub source_refs_json: String,
    pub created_micros: i64,
}

#[derive(Clone, Debug)]
#[table(accessor = dialogue_prompt)]
pub struct DialoguePrompt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub session_id: String,
    pub prompt_id: String,
    pub mode: String,
    pub respondent_role: String,
    pub resolution_policy: String,
    pub choices_json: String,
    pub min_choices: u32,
    pub max_choices: u32,
    pub state: String,
    pub resolved_choice_ids_json: String,
    pub source_refs_json: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DialoguePromptMode {
    YesNo,
    Single,
    Multi,
}

impl DialoguePromptMode {
    const fn stable_id(self) -> &'static str {
        match self {
            Self::YesNo => "YesNo",
            Self::Single => "Single",
            Self::Multi => "Multi",
        }
    }

    fn from_authored(value: &adventuresim_dialogue::PromptMode) -> Self {
        match value {
            adventuresim_dialogue::PromptMode::YesNo => Self::YesNo,
            adventuresim_dialogue::PromptMode::Single => Self::Single,
            adventuresim_dialogue::PromptMode::Multi => Self::Multi,
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "YesNo" => Ok(Self::YesNo),
            "Single" => Ok(Self::Single),
            "Multi" => Ok(Self::Multi),
            _ => Err("Dialogue prompt has an unknown mode".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DialogueResolutionPolicy {
    FirstResponse,
    Unanimous,
    Majority,
    AllRespondents,
}

impl DialogueResolutionPolicy {
    const fn stable_id(self) -> &'static str {
        match self {
            Self::FirstResponse => "FirstResponse",
            Self::Unanimous => "Unanimous",
            Self::Majority => "Majority",
            Self::AllRespondents => "AllRespondents",
        }
    }

    fn from_authored(value: &adventuresim_dialogue::ResolutionPolicy) -> Self {
        match value {
            adventuresim_dialogue::ResolutionPolicy::FirstResponse => Self::FirstResponse,
            adventuresim_dialogue::ResolutionPolicy::Unanimous => Self::Unanimous,
            adventuresim_dialogue::ResolutionPolicy::Majority => Self::Majority,
            adventuresim_dialogue::ResolutionPolicy::AllRespondents => Self::AllRespondents,
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "FirstResponse" => Ok(Self::FirstResponse),
            "Unanimous" => Ok(Self::Unanimous),
            "Majority" => Ok(Self::Majority),
            "AllRespondents" => Ok(Self::AllRespondents),
            _ => Err("Dialogue prompt has an unknown resolution policy".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DialoguePromptState {
    Open,
    Resolved,
}

impl DialoguePromptState {
    const fn stable_id(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "open" => Ok(Self::Open),
            "resolved" => Ok(Self::Resolved),
            _ => Err("Dialogue prompt has an unknown state".into()),
        }
    }
}

#[derive(Clone, Debug)]
#[table(accessor = dialogue_action)]
pub struct DialogueAction {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub session_id: String,
    pub action_id: String,
    pub action_kind: String,
    pub resulting_revision: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = dialogue_answer)]
pub struct DialogueAnswer {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub prompt_row_id: String,
    pub character_id: u64,
    pub choice_ids_json: String,
    pub created_micros: i64,
}

#[derive(Clone, Debug)]
#[table(accessor = character_topic_knowledge)]
pub struct CharacterTopicKnowledge {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    pub conversation_id: String,
    pub topic_id: String,
    pub learned_micros: i64,
}

#[derive(Clone, Debug)]
#[table(accessor = dialogue_topic_option)]
pub struct DialogueTopicOption {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub session_id: String,
    pub topic_id: String,
    /// Empty for presentation-only topics; otherwise the exact observer-safe
    /// public case advanced by this projected option.
    pub public_case_id: String,
    pub label: String,
    pub source_ref_json: String,
}

#[derive(Clone, Debug)]
#[table(accessor = dialogue_investigation_binding)]
pub struct DialogueInvestigationBinding {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub session_id: String,
    pub character_id: u64,
    pub party_id: String,
    pub intended_recipient_id: String,
    pub action_family: String,
    pub source_scope: String,
    pub case_id: String,
    pub objective_id: String,
    pub expected_custody_version: Option<u32>,
    pub issued_revision: u64,
    pub consumed_by: String,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendDialogueSession {
    pub id: String,
    pub conversation_id: String,
    pub catalog_revision: String,
    pub settlement_id: String,
    pub location_id: String,
    pub state: String,
    pub revision: u64,
    pub created_micros: i64,
    pub owner_character_id: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendDialogueParticipant {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub character_id: Option<u64>,
    pub actor_id: String,
    pub display_name: String,
    pub owner_character_id: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendDialogueEvent {
    pub id: String,
    pub session_id: String,
    pub sequence: u32,
    pub response_id: String,
    pub speaker_role: String,
    pub fragments_json: String,
    pub source_refs_json: String,
    pub created_micros: i64,
    pub owner_character_id: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendDialoguePrompt {
    pub id: String,
    pub session_id: String,
    pub prompt_id: String,
    pub mode: String,
    pub respondent_role: String,
    pub resolution_policy: String,
    pub choices_json: String,
    pub min_choices: u32,
    pub max_choices: u32,
    pub state: String,
    pub resolved_choice_ids_json: String,
    pub source_refs_json: String,
    pub owner_character_id: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendDialogueTopicOption {
    pub id: String,
    pub session_id: String,
    pub topic_id: String,
    pub public_case_id: String,
    pub label: String,
    pub source_ref_json: String,
    pub owner_character_id: u64,
}

fn player_participant_ids(participants: impl Iterator<Item = Option<u64>>) -> Vec<u64> {
    participants.flatten().collect()
}

fn dialogue_viewer_ids(ctx: &ViewContext, session_id: &str) -> Vec<u64> {
    player_participant_ids(
        ctx.db
            .dialogue_participant()
            .session_id()
            .filter(session_id)
            .map(|participant| participant.character_id),
    )
}

#[view(accessor = backend_dialogue_sessions, public)]
pub fn backend_dialogue_sessions(ctx: &ViewContext) -> Vec<BackendDialogueSession> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .dialogue_session()
        .gateway_bucket()
        .filter(0u8)
        .flat_map(|row| {
            dialogue_viewer_ids(ctx, &row.id)
                .into_iter()
                .map(move |owner_character_id| BackendDialogueSession {
                    id: row.id.clone(),
                    conversation_id: row.conversation_id.clone(),
                    catalog_revision: row.catalog_revision.clone(),
                    settlement_id: row.settlement_id.clone(),
                    location_id: row.location_id.clone(),
                    state: row.state.clone(),
                    revision: row.revision,
                    created_micros: row.created_micros,
                    owner_character_id,
                })
        })
        .collect()
}

#[view(accessor = backend_dialogue_participants, public)]
pub fn backend_dialogue_participants(ctx: &ViewContext) -> Vec<BackendDialogueParticipant> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .dialogue_participant()
        .gateway_bucket()
        .filter(0u8)
        .flat_map(|row| {
            dialogue_viewer_ids(ctx, &row.session_id)
                .into_iter()
                .map(move |owner_character_id| BackendDialogueParticipant {
                    id: row.id.clone(),
                    session_id: row.session_id.clone(),
                    role: row.role.clone(),
                    character_id: row.character_id,
                    actor_id: row.actor_id.clone(),
                    display_name: row.display_name.clone(),
                    owner_character_id,
                })
        })
        .collect()
}

#[view(accessor = backend_dialogue_events, public)]
pub fn backend_dialogue_events(ctx: &ViewContext) -> Vec<BackendDialogueEvent> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .dialogue_event()
        .gateway_bucket()
        .filter(0u8)
        .flat_map(|row| {
            dialogue_viewer_ids(ctx, &row.session_id)
                .into_iter()
                .map(move |owner_character_id| BackendDialogueEvent {
                    id: row.id.clone(),
                    session_id: row.session_id.clone(),
                    sequence: row.sequence,
                    response_id: row.response_id.clone(),
                    speaker_role: row.speaker_role.clone(),
                    fragments_json: row.fragments_json.clone(),
                    source_refs_json: row.source_refs_json.clone(),
                    created_micros: row.created_micros,
                    owner_character_id,
                })
        })
        .collect()
}

#[view(accessor = backend_dialogue_prompts, public)]
pub fn backend_dialogue_prompts(ctx: &ViewContext) -> Vec<BackendDialoguePrompt> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .dialogue_prompt()
        .gateway_bucket()
        .filter(0u8)
        .flat_map(|row| {
            dialogue_viewer_ids(ctx, &row.session_id)
                .into_iter()
                .map(move |owner_character_id| BackendDialoguePrompt {
                    id: row.id.clone(),
                    session_id: row.session_id.clone(),
                    prompt_id: row.prompt_id.clone(),
                    mode: row.mode.clone(),
                    respondent_role: row.respondent_role.clone(),
                    resolution_policy: row.resolution_policy.clone(),
                    choices_json: row.choices_json.clone(),
                    min_choices: row.min_choices,
                    max_choices: row.max_choices,
                    state: row.state.clone(),
                    resolved_choice_ids_json: row.resolved_choice_ids_json.clone(),
                    source_refs_json: row.source_refs_json.clone(),
                    owner_character_id,
                })
        })
        .collect()
}

#[view(accessor = backend_dialogue_topic_options, public)]
pub fn backend_dialogue_topic_options(ctx: &ViewContext) -> Vec<BackendDialogueTopicOption> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .dialogue_topic_option()
        .gateway_bucket()
        .filter(0u8)
        .flat_map(|row| {
            dialogue_viewer_ids(ctx, &row.session_id)
                .into_iter()
                .map(move |owner_character_id| BackendDialogueTopicOption {
                    id: row.id.clone(),
                    session_id: row.session_id.clone(),
                    topic_id: row.topic_id.clone(),
                    public_case_id: row.public_case_id.clone(),
                    label: row.label.clone(),
                    source_ref_json: row.source_ref_json.clone(),
                    owner_character_id,
                })
        })
        .collect()
}

fn require_dialogue_revision(revision: &str) -> Result<(), String> {
    if revision == adventuresim_dialogue::CATALOG_DIGEST {
        Ok(())
    } else {
        Err("Dialogue catalog revision is stale".into())
    }
}

/// Revalidates the complete physical authority boundary for every dialogue mutation.
fn require_live_dialogue_presence(
    ctx: &ReducerContext,
    session: &DialogueSession,
    character_id: u64,
) -> Result<crate::settlement_population::SettlementResidentProfile, String> {
    let exact_place =
        require_navigable_npc_place(ctx, &session.settlement_id, &session.location_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if character.party_id.as_deref() != Some(session.owner_party_id.as_str()) {
        return Err("Dialogue joins are limited to the owning party".into());
    }
    if character.current_settlement_id.as_deref() != Some(session.settlement_id.as_str()) {
        return Err("Dialogue participant has left the settlement".into());
    }
    let npc_participants: Vec<_> = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .filter(|participant| participant.character_id.is_none())
        .collect();
    if npc_participants.is_empty() {
        return Err("Dialogue has no persistent NPC participant".into());
    }
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map(|time| time.minutes)
        .ok_or("Dialogue participant has no personal time authority")?;
    let actor_settlement_presence =
        adventuresim_core::strategic_presence::StrategicPresence::settlement_membership(
            character_id,
            session.settlement_id.clone(),
            adventuresim_core::strategic_presence::PresenceFrontier {
                observer_character_id: character_id,
                personal_minute: minute,
            },
        )
        .map_err(|_| "Dialogue settlement identity is invalid")?;
    let actor_presence =
        adventuresim_core::strategic_presence::StrategicPresence::validated_venue_selection(
            &actor_settlement_presence,
            exact_place,
        )
        .map_err(|_| "Dialogue location is not in the actor's settlement")?;
    let mut primary = None;
    for participant in npc_participants {
        let npc_character_id = participant
            .actor_id
            .parse::<u64>()
            .map_err(|_| "Dialogue NPC identity is invalid")?;
        let npc = ctx
            .db
            .settlement_resident_profile()
            .character_id()
            .find(npc_character_id)
            .ok_or("Dialogue NPC is no longer authoritative")?;
        let presence = ctx
            .db
            .settlement_resident_presence()
            .character_id()
            .find(npc.character_id)
            .ok_or("Dialogue NPC has no authoritative presence")?;
        let npc_presence = crate::settlement_population::npc_strategic_presence_at(
            ctx,
            &presence,
            character_id,
            minute,
        )
        .ok_or_else(|| {
            adventuresim_core::reducer_error::coded_reducer_error(
                adventuresim_core::reducer_error::ReducerErrorCode::DialogueContactUnavailable,
                "Dialogue NPC is not present at the session location and time",
            )
        })?;
        if npc.home_settlement_id != session.settlement_id
            || !adventuresim_core::strategic_presence::are_co_present(
                &actor_presence,
                npc_presence.presence(),
            )
        {
            return Err(adventuresim_core::reducer_error::coded_reducer_error(
                adventuresim_core::reducer_error::ReducerErrorCode::DialogueContactUnavailable,
                "Dialogue NPC is not present at the session location and time",
            ));
        }
        if (npc.organization_id.is_empty() && npc.conversation_id == "organization-representative")
            || (!npc.organization_id.is_empty()
                && exact_organization_representative(
                    ctx,
                    &npc,
                    &session.settlement_id,
                    &session.location_id,
                )
                .is_none())
        {
            return Err("Dialogue NPC has no exact authority at the session location".into());
        }
        primary.get_or_insert(npc);
    }
    primary.ok_or_else(|| "Dialogue requires an NPC participant".into())
}

fn require_navigable_npc_location(
    ctx: &ReducerContext,
    settlement_id: &str,
    location_id: &str,
) -> Result<(), String> {
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(settlement_id.to_owned())
        .ok_or("Settlement not found")?;
    let has_keep = matches!(
        settlement.category,
        SettlementCategory::Town | SettlementCategory::City | SettlementCategory::Capital
    );
    if adventuresim_core::settlement_economy::npc_location_is_navigable(
        &settlement.economy,
        has_keep,
        settlement_id,
        location_id,
    ) {
        Ok(())
    } else {
        Err("NPC location is not available in this settlement".into())
    }
}

fn require_navigable_npc_place(
    ctx: &ReducerContext,
    settlement_id: &str,
    location_id: &str,
) -> Result<adventuresim_core::strategic_place::StrategicPlaceId, String> {
    require_navigable_npc_location(ctx, settlement_id, location_id)?;
    crate::settlement_population::canonical_npc_place(settlement_id, location_id)
        .ok_or_else(|| "NPC location has no canonical strategic place".into())
}

#[cfg(test)]
mod stable_dialogue_schema_tests {
    use super::{
        DialoguePromptMode, DialoguePromptState, DialogueResolutionPolicy, DialogueSessionState,
        ParsedDialogueSessionId, CorpsePermissionTopicId,
    };

    #[test]
    fn dialogue_session_id_and_state_are_exact_tagged_values() {
        let parsed = ParsedDialogueSessionId::parse("dialogue:7:nonce:part").unwrap();
        assert_eq!(parsed.owner_character_id, 7);
        assert_eq!(parsed.nonce, "nonce:part");
        assert!(ParsedDialogueSessionId::parse("dialogue:7:").is_err());
        assert!(ParsedDialogueSessionId::parse("prefix-dialogue:7:nonce").is_err());
        assert_eq!(
            DialogueSessionState::parse("active").unwrap().stable_id(),
            "active"
        );
        assert!(DialogueSessionState::parse("active-session").is_err());
    }

    #[test]
    fn prompt_codes_reject_prose_and_suffix_matches() {
        for (value, expected) in [
            ("YesNo", DialoguePromptMode::YesNo),
            ("Single", DialoguePromptMode::Single),
            ("Multi", DialoguePromptMode::Multi),
        ] {
            assert_eq!(DialoguePromptMode::parse(value).unwrap(), expected);
            assert_eq!(expected.stable_id(), value);
        }
        for value in ["yes/no", "PrefixYesNo", "SingleChoice", "MultiSuffix"] {
            assert!(DialoguePromptMode::parse(value).is_err());
        }
        for (value, expected) in [
            ("FirstResponse", DialogueResolutionPolicy::FirstResponse),
            ("Unanimous", DialogueResolutionPolicy::Unanimous),
            ("Majority", DialogueResolutionPolicy::Majority),
            ("AllRespondents", DialogueResolutionPolicy::AllRespondents),
        ] {
            assert_eq!(DialogueResolutionPolicy::parse(value).unwrap(), expected);
            assert_eq!(expected.stable_id(), value);
        }
        assert!(DialogueResolutionPolicy::parse("FirstResponseWins").is_err());
        assert_eq!(
            DialoguePromptState::parse("resolved")
                .unwrap()
                .stable_id(),
            "resolved"
        );
        assert!(DialoguePromptState::parse("unresolved").is_err());
    }

    #[test]
    fn corpse_permission_topics_parse_once_as_exact_tagged_coordinates() {
        let parsed = CorpsePermissionTopicId::parse(
            "corpse-permission:examination:request:corpse:character:7",
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            parsed.scope,
            crate::corpse::CorpsePermissionScope::Examination
        );
        assert_eq!(parsed.approach, "request");
        assert_eq!(parsed.corpse_id, "corpse:character:7");
        assert!(CorpsePermissionTopicId::parse("ordinary-topic")
            .unwrap()
            .is_none());
        assert!(
            CorpsePermissionTopicId::parse(
                "prefix-corpse-permission:examination:request:corpse:7"
            )
            .unwrap()
            .is_none()
        );
        for malformed in [
            "corpse-permission:unknown:request:corpse:7",
            "corpse-permission:examination::corpse:7",
            "corpse-permission:examination:request:",
        ] {
            assert!(CorpsePermissionTopicId::parse(malformed).is_err());
        }
    }
}
