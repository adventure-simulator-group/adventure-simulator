use maud::{Markup, html};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{
    character_details::religion_name,
    context::LocationView,
    trade::{item_name_with_food_lot, trade_inventory_table_header},
};
use crate::spacetimedb::{Character, FoodLot, InventoryItem};
use crate::templates::{decorative_game_icon, item_display_name, item_type_icon, scene_interactable_link, sidebar_section, SceneInteractableLink};

#[derive(Debug, Clone, Default)]
pub struct SocialPresentation {
    pub affinity: f32,
    pub familiarity_hours: f32,
    pub religion_id: Option<String>,
    pub fame: f32,
    pub infamy: f32,
    pub beliefs: Vec<crate::spacetimedb::SocialBelief>,
    pub shared_concerns: Vec<adventuresim_core::social::SocialTopic>,
    pub addressed_source_ids: Vec<String>,
    pub automatic_chat_enabled: bool,
    pub joke_blocked: bool,
    pub flirt_blocked: bool,
    pub prayer_disabled_reason: Option<String>,
    pub relationship_answer: Option<String>,
    pub feedback: Option<SocialFeedback>,
    pub unavailable: bool,
}

#[derive(Debug, Clone)]
pub struct SocialFeedback {
    pub message: &'static str,
    pub is_error: bool,
}

fn casual_chat_action_id() -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("chat-{nanos:x}-{sequence:x}")
}

fn social_actions(
    is_self: bool,
    topic: adventuresim_core::social::SocialTopic,
    joke_blocked: bool,
    flirt_blocked: bool,
) -> Vec<(
    &'static str,
    adventuresim_core::social::SocialActionKind,
    &'static str,
)> {
    use adventuresim_core::social::SocialActionKind::*;
    if is_self {
        return vec![("inner-self", Reflect, "reflect")];
    }
    [
        ("awareness", Listen, "listen"),
        ("awareness", Commiserate, "commiserate"),
        ("prayer", Pray, "pray"),
        ("heart-beats", Reassure, "reassure"),
        ("juggler", LightenMood, "lighten_mood"),
        ("crown", Rally, "command"),
        ("conversation", Reframe, "deception"),
        ("rose", Flirt, "flirt"),
    ]
    .into_iter()
    .filter(|(_, action, _)| {
        action.available_for(topic)
            && !(*action == LightenMood && joke_blocked)
            && !(*action == Flirt && flirt_blocked)
    })
    .collect()
}

fn social_action_label(
    action: adventuresim_core::social::SocialActionKind,
    shares_concern: bool,
) -> &'static str {
    use adventuresim_core::social::SocialActionKind;
    match action {
        SocialActionKind::Reflect => "Reflect",
        SocialActionKind::Listen => "Listen",
        SocialActionKind::Commiserate if shares_concern => "Commiserate",
        SocialActionKind::Commiserate => "Feign sympathy",
        SocialActionKind::Pray => "Pray",
        SocialActionKind::Reassure => "Reassure",
        SocialActionKind::LightenMood => "Joke",
        SocialActionKind::Rally => "Rally",
        SocialActionKind::Reframe => "Reframe",
        SocialActionKind::Flirt => "Flirt",
    }
}

fn perceived_trait(
    axis: crate::spacetimedb::BeliefAxis,
    value: i8,
) -> (&'static str, &'static str) {
    let axis = axis.core();
    axis.value_label(value)
        .map_or(("Personality", "Uncertain"), |value| (axis.label(), value))
}

fn familiarity_label(hours: f32) -> String {
    if hours.is_finite() && hours > 0.0 && hours < 1.0 {
        "<1 hours".into()
    } else {
        format!("{:.0} hours", hours.max(0.0))
    }
}

fn reputation_answer(fame: f32, infamy: f32) -> &'static str {
    match (fame, infamy) {
        (good, ill) if good >= 0.5 && ill >= 0.5 => {
            "My name is much abroad here, for deeds both praised and blamed."
        }
        (good, _) if good >= 0.5 => "Folk here speak well of me, and my good name is widely known.",
        (_, ill) if ill >= 0.5 => "Ill report follows me here, and many speak my name with care.",
        (good, ill) if good >= 0.15 && ill >= 0.15 => {
            "Some speak well of me, and some ill; my name is not unknown here."
        }
        (good, _) if good >= 0.15 => "I have won some small good report in these parts.",
        (_, ill) if ill >= 0.15 => "Some little ill report follows me in these parts.",
        _ => "My name bears little report in these parts, either fair or foul.",
    }
}

fn belief_style(confidence: f32) -> String {
    format!(
        "--belief-confidence:{:.0}%",
        confidence.clamp(0.0, 1.0) * 100.0
    )
}

fn personality_reaction_hint(axis: crate::spacetimedb::BeliefAxis, value: i8) -> &'static str {
    let axis = axis.core().slug();
    match (axis, value) {
        ("drive", 1) => {
            "Likely reaction: Rallying can motivate them after defeat; pity or flippancy may offend."
        }
        ("drive", 2) => {
            "Likely reaction: Listening and commiseration are safer than pressuring them to prove themselves."
        }
        ("self_regard", 1) => {
            "Likely reaction: Injury is touchy; admiration may land better than pity or minimizing the wound."
        }
        ("self_regard", 2) => {
            "Likely reaction: Plain sympathy is safer; conspicuous flattery may feel insincere."
        }
        ("conviction", 1) => {
            "Likely reaction: Treat moral concerns seriously; jokes and false reassurance are especially risky."
        }
        ("conviction", 2) => {
            "Likely reaction: Gentle reframing may work better than appeals to duty or conviction."
        }
        ("hygiene", 2) => {
            "Likely reaction: Filth is genuinely upsetting; acknowledge it rather than dismissing the concern."
        }
        ("hygiene", 1) => {
            "Likely reaction: They may not share strong concern about grime, so forceful reassurance can seem strange."
        }
        _ => "Likely reaction: Their response to riskier social actions remains uncertain.",
    }
}

fn belief_tooltip(belief: &crate::spacetimedb::SocialBelief) -> String {
    format!(
        "Confidence: {:.0}%\n{}",
        belief.confidence.clamp(0.0, 1.0) * 100.0,
        personality_reaction_hint(belief.axis, belief.perceived_value)
    )
}

/// Conversation Dock projection. It intentionally receives observer-specific
/// beliefs rather than authoritative personality.
pub fn party_social_dialog(
    location: &LocationView,
    selected: &Character,
    active_character: &Character,
    morale_sources: &[crate::spacetimedb::CharacterMoraleSource],
    social: &SocialPresentation,
) -> Markup {
    let social_href = location.preserve_building(format!(
        "{}/party/{}/social",
        location.base_path(),
        selected.id
    ));
    let automatic_href = location.preserve_building(format!(
        "{}/party/{}/social/automatic",
        location.base_path(),
        selected.id
    ));
    let chat_href = location.preserve_building(format!(
        "{}/party/{}/social/chat",
        location.base_path(),
        selected.id
    ));
    let chat_action_id = casual_chat_action_id();
    let affinity_label = match social.affinity {
        value if value >= 50.0 => "Devoted",
        value if value >= 15.0 => "Warm",
        value if value <= -50.0 => "Hostile",
        value if value <= -15.0 => "Cold",
        _ => "Neutral",
    };
    let is_self = selected.id == active_character.id;
    let affinity_face = match social.affinity {
        value if value >= 50.0 => ("very-warm", "Very warm regard", "☺"),
        value if value >= 15.0 => ("warm", "Warm regard", "🙂"),
        value if value <= -50.0 => ("hostile", "Hostile regard", "☹"),
        value if value <= -15.0 => ("cold", "Cold regard", "🙁"),
        _ => ("neutral", "Neutral regard", "😐"),
    };
    html! {
        section class="settlement-chat social-conversation-dock" aria-label=(format!("Conversation with {}", selected.name))
          data-social-conversation data-social-subject=(selected.id) data-social-self=[is_self.then_some("true")]
          data-local-chat-kind="player" data-local-chat-subject=(selected.id) {
          div class="settlement-chat-layout" {
            div class="settlement-chat-conversation" {
              header class="conversation-dock-header" {
                div class="settlement-chat-filters" role="group" aria-label="Visible chat channels" {
                  @for (channel, label, abbreviation) in [("local", "Local", "L"), ("party", "Party", "P"), ("info", "Info", "I")] {
                    label class=(format!("chat-channel-filter chat-channel-filter-{channel}")) title=(label) {
                      input type="checkbox" checked data-chat-filter=(channel) aria-label=(label) title=(label);
                      span aria-hidden="true" { (abbreviation) }
                    }
                  }
                }
                div class="conversation-dock-tools" {
                  @if !is_self {
                    div class=(format!("affinity-popover affinity-{}", affinity_face.0)) data-affinity-popover {
                      button type="button" class="affinity-face" aria-label=(affinity_face.1)
                        aria-expanded="false" aria-controls=(format!("affinity-details-{}", selected.id))
                        data-affinity-trigger data-strategic-tooltip=(format!("{}; click to pin details", affinity_face.1)) {
                        span aria-hidden="true" { (affinity_face.2) }
                      }
                      section id=(format!("affinity-details-{}", selected.id)) class="affinity-details"
                        data-affinity-details aria-label=(format!("Your impression of {}", selected.name)) {
                        h3 { "Thy impression" }
                        dl class="social-biography" {
                          div { dt { "Regard" } dd { (affinity_label) } }
                          div { dt { "Familiarity" } dd { (familiarity_label(social.familiarity_hours)) } }
                        }
                        @if social.unavailable {
                          p class="social-unavailable" role="status" { "Your impressions are unavailable right now." }
                        } @else if social.beliefs.is_empty() {
                          p class="text-muted small-copy" { "You have not formed a confident impression of their character yet." }
                        } @else {
                          ul class="perceived-traits" aria-label="Perceived personality traits" {
                            @for belief in &social.beliefs {
                              @let (axis, value) = perceived_trait(belief.axis, belief.perceived_value);
                              li class="perceived-trait" style=(belief_style(belief.confidence)) tabindex="0"
                                data-strategic-tooltip=(belief_tooltip(belief)) {
                                strong { (value) }
                                span class="visually-hidden" { (axis) "; hover or focus for confidence and approach hints" }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                  div class="conversation-tabs" role="tablist" aria-label="Conversation topics" {
                    @for (id, label, icon, selected_tab) in [
                      ("quests", "Quests", "treasure-map", false),
                      ("lore", "Lore", "open-book", false),
                      ("tidings", "Recent Tidings", "calendar", true),
                      ("about", "Of Thee", "person", false),
                    ] {
                      button type="button" role="tab" class="conversation-tab"
                        id=(format!("conversation-tab-{id}-{}", selected.id))
                        aria-controls=(format!("conversation-panel-{id}-{}", selected.id))
                        aria-selected=(if selected_tab { "true" } else { "false" }) tabindex=(if selected_tab { "0" } else { "-1" })
                        data-conversation-tab=(id) data-strategic-tooltip=(if id == "about" { "About this person" } else { label }) {
                        (decorative_game_icon(icon)) span class="sr-only" { (label) }
                      }
                    }
                  }
                }
              }
              section role="tabpanel" class="conversation-panel" id=(format!("conversation-panel-quests-{}", selected.id))
                aria-labelledby=(format!("conversation-tab-quests-{}", selected.id)) hidden { p class="conversation-empty" { "No discovered quest matter is ready to discuss." } }
              section role="tabpanel" class="conversation-panel" id=(format!("conversation-panel-lore-{}", selected.id))
                aria-labelledby=(format!("conversation-tab-lore-{}", selected.id)) hidden { p class="conversation-empty" { "No discovered lore is ready to discuss." } }
              section role="tabpanel" class="conversation-panel social-rail" id=(format!("conversation-panel-tidings-{}", selected.id))
                aria-labelledby=(format!("conversation-tab-tidings-{}", selected.id)) data-social-panel data-target-id=(selected.id) {
            @if !is_self {
                (sidebar_section("Spend time together", html! {
                    form class="social-chat-activity" method="post" action=(&chat_href)
                        data-social-chat-form data-chat-start-minutes="30" {
                        input type="hidden" name="action_id" value=(&chat_action_id);
                        label for=(format!("social-chat-duration-{}", selected.id)) {
                            strong { "Chat" }
                            span class="text-muted small-copy" {
                                "An ordinary conversation can strengthen or strain the relationship."
                            }
                        }
                        div class="social-chat-duration" {
                            input id=(format!("social-chat-duration-{}", selected.id))
                                type="range" name="requested_minutes" min="15" max="480" step="15"
                                value="30" data-social-chat-duration
                                aria-label=(format!("Time spent chatting with {}", selected.name))
                                aria-valuetext="30 minutes";
                            output for=(format!("social-chat-duration-{}", selected.id))
                                data-social-chat-output { "30 minutes" }
                        }
                        button type="submit" class="btn btn-primary btn-small"
                            data-social-chat-submit { "Chat for 30 minutes" }
                    }
                }))
                form class="automatic-social-chat" method="post" action=(&automatic_href) {
                    label {
                        input type="checkbox" name="enabled" value="true"
                            checked[social.automatic_chat_enabled]
                            data-automatic-social-chat
                            aria-label="Automatic chats during downtime"
                            data-strategic-tooltip="During downtime, you choose an approach from your available social actions according to your personality and relevant skills. Normal risks, cooldowns, and outcomes apply.";
                        span {
                            strong { "Automatic chats during downtime" }
                        }
                    }
                }
            }
            (sidebar_section("Recent Tidings", html! {
                @if morale_sources.is_empty() { p class="text-muted" { "No current morale effects." } }
                div class="social-source-list" {
                    @for source in morale_sources {
                        @let topic = adventuresim_core::social::topic_for_source_kind(&source.kind);
                        @let addressed = source.magnitude < 0.0 && social.addressed_source_ids.contains(&source.id);
                        @let magnitude = source.magnitude.clamp(-5.0, 5.0);
                        @let topic_color = if magnitude < 0.0 {
                            format!("color-mix(in srgb, #d7b650 {:.0}%, #cf4f4f)", (100.0 - magnitude.abs() * 18.0).max(10.0))
                        } else if magnitude > 0.0 {
                            format!("color-mix(in srgb, #d7b650 {:.0}%, #4fae67)", (100.0 - magnitude * 18.0).max(10.0))
                        } else { "#d7b650".to_owned() };
                        @let strength = if magnitude > 0.0 { format!("Positive morale, {magnitude:+.1}") } else if magnitude < 0.0 { format!("Negative morale, {magnitude:+.1}") } else { "Neutral morale, +0.0".to_owned() };
                        article class=(if addressed { "social-source social-source-negative social-source-addressed" } else if source.magnitude < 0.0 { "social-source social-source-negative" } else if source.magnitude > 0.0 { "social-source social-source-positive" } else { "social-source social-source-neutral" })
                            style=(format!("--social-topic-color:{topic_color}")) aria-label=(format!("{}: {strength}", source.label)) {
                            div class="social-source-context" {
                                div { strong { (&source.label) } span class="social-source-strength" { (&strength) } }
                                @if addressed {
                                    p class="social-addressed-status" { "Addressed by you" }
                                }
                                @if let Some(axis) = topic.and_then(adventuresim_core::social::axis_for_topic) {
                                    @if let Some(belief) = social.beliefs.iter().find(|belief| belief.axis.core() == axis) {
                                        @let (axis_name, value) = perceived_trait(belief.axis, belief.perceived_value);
                                        p class="belief-copy" style=(belief_style(belief.confidence))
                                            tabindex="0" data-strategic-tooltip=(belief_tooltip(belief)) {
                                            "You think their " (axis_name) " is " (value) "."
                                        }
                                    } @else {
                                        p { "The relevant personality trait is uncertain." }
                                    }
                                } @else {
                                    p { "No specific personality trait is known to govern this concern." }
                                }
                            }
                            @if source.magnitude < 0.0 && !addressed {
                                @if let Some(topic) = topic {
                                  div class="social-actions" aria-label=(format!("Actions for {}", source.label)) {
                                    @let shares_concern = social.shared_concerns.contains(&topic);
                                    @for (default_icon, action, value) in social_actions(is_self, topic, social.joke_blocked, social.flirt_blocked) {
                                      @let action_shares_concern = action != adventuresim_core::social::SocialActionKind::Commiserate || shares_concern;
                                      @let icon = if action == adventuresim_core::social::SocialActionKind::Commiserate && !shares_concern { "conversation" } else { default_icon };
                                       @let prayer_approach = social.religion_id.as_deref()
                                           .and_then(adventuresim_world_schema::OfficialReligion::from_id)
                                           .and_then(|religion| adventuresim_core::social::prayer_approach(religion, topic));
                                       @let reassurance_approach = adventuresim_core::social::bedside_reassurance_approach(topic);
                                       @let description = if action == adventuresim_core::social::SocialActionKind::Pray {
                                           prayer_approach.map_or_else(
                                               || action.description(topic, action_shares_concern).to_owned(),
                                               |approach| format!("{} {}", approach.devotion, approach.intention),
                                           )
                                       } else if action == adventuresim_core::social::SocialActionKind::Reassure {
                                           reassurance_approach.map_or_else(
                                               || action.description(topic, action_shares_concern).to_owned(),
                                               |approach| approach.counsel.to_owned(),
                                           )
                                       } else {
                                           action.description(topic, action_shares_concern).to_owned()
                                       };
                                      @let label = social_action_label(action, action_shares_concern);
                                      @let disabled_reason = if action == adventuresim_core::social::SocialActionKind::Pray { social.prayer_disabled_reason.as_deref() } else { None };
                                        @let risk = match action {
                                            adventuresim_core::social::SocialActionKind::Pray => {
                                                prayer_approach.map_or(action.risk(), |approach| approach.risk)
                                            }
                                            adventuresim_core::social::SocialActionKind::Reassure => {
                                                reassurance_approach.map_or(action.risk(), |approach| approach.risk)
                                            }
                                            _ => action.risk(),
                                        };
                                      @let tooltip = if let Some(reason) = disabled_reason {
                                          format!("{}\nUnavailable: {}", description, reason)
                                      } else {
                                          format!("{}\nTakes {} minutes.\n{} · {} risk", description, adventuresim_core::social::SOCIAL_RESPONSE_MINUTES, action.skill_name(action_shares_concern), if risk >= 0.6 { "high" } else if risk >= 0.3 { "moderate" } else { "low" })
                                      };
                                    form method="post" action=(&social_href) {
                                        input type="hidden" name="source_id" value=(&source.id);
                                        button type="submit" name="action_kind" value=(value) class="social-action"
                                            disabled[disabled_reason.is_some()]
                                            aria-disabled=[disabled_reason.map(|_| "true")]
                                            aria-label=(if let Some(reason) = disabled_reason { format!("{}. Unavailable: {}.", label, reason) } else { format!("{}. {}. Takes {} minutes.", label, description, adventuresim_core::social::SOCIAL_RESPONSE_MINUTES) })
                                            data-strategic-tooltip=(&tooltip) {
                                            span class="social-action-icon"
                                                data-strategic-tooltip=(&tooltip) {
                                                (decorative_game_icon(icon))
                                            }
                                            span class="social-action-label" {
                                                (label)
                                            }
                                        }
                                    }
                                    }
                                  }
                                }
                            }
                        }
                    }
                }
            }))
                }
                section role="tabpanel" class="conversation-panel about-person-panel"
                    id=(format!("conversation-panel-about-{}", selected.id))
                    aria-labelledby=(format!("conversation-tab-about-{}", selected.id)) hidden data-about-person=(selected.name) {
                    @if is_self {
                        p { "To know thyself, seek thy Recent Tidings and reflect thereupon." }
                    } @else {
                        h3 { "Of Thee" }
                        p class="text-muted small-copy" { "Ask, and hear the answer in their own words." }
                        div class="about-person-topics" {
                            @for (question, answer) in [
                                ("In what regard am I held?", format!("My regard is {}.", affinity_label.to_ascii_lowercase())),
                                ("How long have we been acquainted?", format!("Our acquaintance hath endured {}.", familiarity_label(social.familiarity_hours))),
                                ("How fares the spirit?", if morale_sources.iter().any(|source| source.magnitude < 0.0) { "My spirit is troubled; the recent tidings shall show why.".to_owned() } else { "My spirit rests easily enough at present.".to_owned() }),
                                ("Is there a pledge to another?", social.relationship_answer.clone().unwrap_or_else(|| "I shall speak of courtship or marriage only when such suit is lawful between us.".to_owned())),
                                ("How many years have passed?", format!("I have seen {} years.", selected.age_years)),
                                ("What faith is professed?", match religion_name(social.religion_id.as_deref()) { "None" => "I profess no settled faith.".to_owned(), faith => format!("I am of the {faith} confession.") }),
                                ("What report is borne in these parts?", reputation_answer(social.fame, social.infamy).to_owned()),
                            ] {
                                button type="button" class="about-person-topic" data-about-question=(question) data-about-answer=(answer) { (question) }
                            }
                        }
                    }
                }
                div class="settlement-chat-messages" aria-live="polite" data-social-message-stream {
                    @if let Some(feedback) = &social.feedback {
                        div class=(if feedback.is_error { "chat-system-message social-feedback social-feedback-error" } else { "chat-system-message social-feedback social-feedback-result" })
                            data-chat-channel="info" role=(if feedback.is_error { "alert" } else { "status" }) {
                            span class="chat-timestamp" { "[--:--] " }
                            (feedback.message)
                        }
                    }
                }
                @if !is_self {
                    div class="settlement-chat-composer" {
                        div class="settlement-chat-input-shell" { span class="settlement-chat-completion" data-dialogue-completion aria-hidden="true" {}
                            input type="text" name="body" aria-label="Local message" autocomplete="off" placeholder=(format!("Speak with {}", selected.name));
                        }
                        button type="button" class="btn btn-primary btn-icon" aria-label="Send message" { (decorative_game_icon("plain-arrow")) }
                    }
                }
            }
            }
        }
    }
}

/// Shared chat panel. Local conversations are live; the remaining channel
/// filters are present so their messages can join the same stream as their
/// backends become available.
pub(crate) fn settlement_chat_area(location: &str, active_character: Option<&Character>) -> Markup {
    chat_area(location, active_character, None, None, None, None, &[])
}

pub(crate) fn settlement_chat_area_with_info(
    location: &str,
    active_character: Option<&Character>,
    info_messages: &[String],
) -> Markup {
    chat_area(
        location,
        active_character,
        None,
        None,
        None,
        None,
        info_messages,
    )
}

pub(super) fn player_chat_area(
    location: &LocationView,
    subject: &Character,
    active_character: &Character,
) -> Markup {
    let context = ("player", subject.id.to_string());
    chat_area(
        &subject.name,
        Some(active_character),
        None,
        Some(context),
        None,
        Some(location.preserve_building(format!(
            "{}/party/{}/social",
            location.base_path(),
            subject.id
        ))),
        &[],
    )
}

pub(super) fn npc_location_id(service_id: &str) -> &str {
    match adventuresim_core::organization::service_npc_location_id(service_id) {
        Some(location_id) => location_id,
        None => service_id,
    }
}

/// Presentation projection of the non-character interactables available at a
/// settlement location. This is deliberately separate from the NPC loader so
/// templates cannot accidentally infer fixtures from the people present.
struct LocationFixture {
    kind: &'static str,
    label: &'static str,
    aria_label: &'static str,
    icon: &'static str,
    action_label: &'static str,
    href: String,
}

fn location_fixtures(
    settlement_id: &str,
    location_id: &str,
    organization_service: Option<&str>,
) -> Vec<LocationFixture> {
    let mut fixtures = Vec::new();
    if !matches!(location_id, "overview" | "public-square" | "map") {
        fixtures.push(LocationFixture {
            kind: "fixture fireplace-portrait",
            label: "Fireplace",
            aria_label: "Cook at fireplace",
            icon: "campfire",
            action_label: "Cook",
            href: format!("/locations/settlement/{settlement_id}/fireplace?building={location_id}"),
        });
    }
    if organization_service == Some("weapons") {
        fixtures.push(LocationFixture {
            kind: "fixture forge-portrait",
            label: "Forge",
            aria_label: "Forge a weapon",
            icon: "anvil",
            action_label: "Forge",
            href: format!("/settlements/{settlement_id}/weapons"),
        });
    }
    fixtures
}

pub(super) fn npc_portrait_strip(settlement_id: &str, location_id: &str) -> Markup {
    let organization_service =
        adventuresim_core::organization::organization_chapter_at(settlement_id, location_id)
            .and_then(|(organization, _)| organization.service_id.as_deref());
    html! {
        nav class="scene-interactable-strip" aria-label="People and things here" data-npc-strip
            data-npc-settlement=(settlement_id) data-npc-location=(location_id) {
            @for fixture in location_fixtures(settlement_id, location_id, organization_service) {
                span data-location-fixture {
                    (scene_interactable_link(SceneInteractableLink {
                        kind: fixture.kind, href: &fixture.href, label: fixture.label,
                        aria_label: fixture.aria_label, icon: fixture.icon, action_label: Some(fixture.action_label),
                    }))
                }
            }
            span class="text-muted" data-npc-loading { "Finding the people here…" }
        }
    }
}

pub(super) fn npc_description_stage(name: &str, fallback: &str) -> Markup {
    html! { section class="visual-stage npc-description-stage" data-npc-description aria-live="polite" {
        div class="visual-stage-placeholder npc-portrait-silhouette" aria-hidden="true" {}
        h2 { (name) }
        p { (fallback) }
    } }
}

pub(super) fn forge_description_stage(name: &str, fallback: &str) -> Markup {
    html! { section class="visual-stage npc-description-stage forge-description-stage" data-npc-description data-bevy-scene="forge" aria-live="polite" {
        div class="visual-stage-placeholder npc-portrait-silhouette" aria-hidden="true" {}
        h2 { (name) }
        p { (fallback) }
    } }
}

pub(super) fn settlement_resident_chat_area(
    location: &str,
    active_character: Option<&Character>,
    settlement_id: &str,
    location_id: &str,
    service_id: Option<&str>,
) -> Markup {
    chat_area(
        location,
        active_character,
        Some((settlement_id, service_id.unwrap_or(""))),
        Some(("npc", String::new())),
        Some(location_id),
        None,
        &[],
    )
}

fn chat_area(
    location: &str,
    active_character: Option<&Character>,
    service_context: Option<(&str, &str)>,
    local_context: Option<(&str, String)>,
    local_location_id: Option<&str>,
    party_social_href: Option<String>,
    info_messages: &[String],
) -> Markup {
    let is_self_chat = local_context.as_ref().is_some_and(|(kind, subject)| {
        *kind == "player"
            && active_character.is_some_and(|active| subject == &active.id.to_string())
    });
    html! {
        section class="settlement-chat" aria-label="Settlement chat"
            data-service-quest-settlement=[service_context.map(|context| context.0)]
            data-service-quest-id=[service_context.map(|context| context.1)]
            data-dialogue-catalog-revision=[service_context.map(|_| adventuresim_dialogue::CATALOG_DIGEST)]
            data-herbalist-exam-fee=[service_context
                .filter(|context| context.1 == "herbalist")
                .map(|_| adventuresim_core::strategic_economy::NPC_HERBALIST_EXAM_FEE)]
            data-local-chat-kind=[local_context.as_ref().map(|context| context.0)]
            data-local-chat-subject=[local_context.as_ref().map(|context| context.1.as_str())]
            data-local-chat-location=[local_location_id]
            data-party-social-href=[party_social_href.as_deref()] {
            div class="settlement-chat-resize" role="separator" aria-label="Resize chat"
                aria-orientation="horizontal" aria-valuemin="128" aria-valuemax="640"
                aria-valuenow="184" tabindex="0" title="Drag to resize chat" {
                span aria-hidden="true" {}
            }
            div class="settlement-chat-layout" {
                div class="settlement-chat-conversation" {
                  header class="conversation-dock-header" {
                    div class="settlement-chat-filters" role="group" aria-label="Visible chat channels" {
                        @for (channel, label, abbreviation) in [
                            ("local", "Local", "L"),
                            ("party", "Party", "P"),
                            ("settlement", "Settlement", "S"),
                            ("dm", "DMs", "D"),
                            ("guild", "Guild", "G"),
                            ("info", "Info", "I"),
                        ] {
                            label class=(format!("chat-channel-filter chat-channel-filter-{channel}")) title=(label) {
                                input type="checkbox" checked data-chat-filter=(channel)
                                    aria-label=(label) title=(label);
                                span aria-hidden="true" { (abbreviation) }
                            }
                        }
                    }
                    div class="conversation-tabs" role="tablist" aria-label="Conversation topics" data-dialogue-category-tabs {
                      @for (id, label, icon, selected_tab) in [
                        ("quest", "Quests", "treasure-map", false),
                        ("lore", "Lore", "open-book", true),
                        ("tidings", "Recent Tidings", "calendar", false),
                        ("about", "Of Thee", "person", false),
                      ] {
                        button type="button" role="tab" class="conversation-tab" id=(format!("dialogue-category-tab-{id}"))
                          aria-controls=(format!("dialogue-category-panel-{id}")) aria-selected=(if selected_tab { "true" } else { "false" })
                          tabindex=(if selected_tab { "0" } else { "-1" }) data-dialogue-category=(id)
                          data-strategic-tooltip=(if id == "about" { "About this person" } else { label }) {
                          (decorative_game_icon(icon)) span class="sr-only" { (label) }
                        }
                      }
                    }
                  }
                    @for (id, label, selected_panel) in [("quest", "Quests", false), ("lore", "Lore", true), ("tidings", "Recent Tidings", false), ("about", "Of Thee", false)] {
                      section role="tabpanel" class="dialogue-category-panel" id=(format!("dialogue-category-panel-{id}"))
                        aria-labelledby=(format!("dialogue-category-tab-{id}")) hidden[!selected_panel] data-dialogue-category-panel=(id) {
                        p class="conversation-empty" data-dialogue-category-empty { "No discovered " (label) " topics are ready to discuss." }
                      }
                    }
                    div class="settlement-chat-messages" aria-live="polite" {
                        @if local_context.is_none() { div class="chat-system-message" data-chat-channel="info" {
                            span class="chat-timestamp" { "[--:--] " }
                            " Select a local character or settlement service to begin talking."
                        } }
                        @for message in info_messages {
                            div class="chat-system-message" data-chat-channel="info" {
                                span class="chat-timestamp" { "[--:--] " }
                                (message)
                            }
                        }
                    }
                    div class="settlement-chat-composer" {
                        div class="settlement-chat-input-shell" {
                            span class="settlement-chat-completion" data-dialogue-completion aria-hidden="true" {}
                            input type="text" name="body" disabled[local_context.is_none() || is_self_chat]
                                aria-label="Local message"
                                autocomplete="off"
                                placeholder=(if is_self_chat { "Select Recent Tidings to reflect".to_owned() } else { format!("Message {location} (Local)") });
                        }
                        button type="button" class="btn btn-primary btn-icon" disabled[local_context.is_none() || is_self_chat]
                            aria-label="Send message" {
                            (decorative_game_icon("plain-arrow"))
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn merchant_offers_rail(title: &str, unavailable_offers: &[&str]) -> Markup {
    html! {
        (sidebar_section(title, html! {
            ul class="service-offering-list" aria-label="Expected offerings" title="No stock is listed at present" {
                @for offer in unavailable_offers {
                    li { (decorative_game_icon("shop")) span { (offer) } }
                }
            }
        }))
    }
}

pub(super) fn inventory_rail(
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    food_lots: &[FoodLot],
    trade_action: Option<(&str, &str)>,
    _show_repair: bool,
) -> Markup {
    let title = active_character
        .map(|character| format!("{}'s inventory", character.name))
        .unwrap_or_else(|| "Your inventory".to_string());

    html! {
        (sidebar_section(&title, html! {
            div class="inventory-browser" data-inventory-browser="service-personal" data-optional-columns="" {
            div class="inventory-browser-toolbar" {
                label class="inventory-browser-search" {
                    span class="sr-only" { "Search items by name" }
                    input type="search" data-inventory-search placeholder="Search items" autocomplete="off"
                        aria-label="Search items by name";
                }
            }
            @if inventory.is_empty() {
                p class="text-muted small-copy" { "No items carried." }
            } @else {
                div class="inventory-browser-table-frame" {
                table class="trade-inventory-table" {
                    (trade_inventory_table_header(false, None))
                    tbody {
                    @for item in inventory {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let food_lot = food_lots.iter().find(|lot| lot.inventory_item_id == Some(item.id));
                        @let display_name = food_lot.map_or_else(|| item_display_name(&item.item_id), |lot| lot.display_name.clone());
                        @let item_name = item_display_name(&item.item_id);
                        tr class=(if trade_action.is_some() { "trade-inventory-row" } else { "trade-inventory-row inventory-row-readonly" }) data-personal-inventory-id=(item.id) {
                            td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                            td class="inventory-item-name" {
                                (item_name_with_food_lot(&item.item_id, &display_name, definition, food_lot))
                                @if let Some((action, tooltip)) = trade_action {
                                button type="button" class="trade-transfer trade-transfer-left" disabled
                                    aria-label=(format!("{action} {item_name}"))
                                    title=(tooltip) { "◀" }
                                }
                            }
                            td class="inventory-count" { (item.qty) }
                            td class="inventory-weight" { "—" }
                            td class="inventory-gold" { "—" }
                        }
                    }
                }
                }
            }
            }
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacetimedb::*;
    use crate::templates::settlement::settlement_resident_location_page;
    use crate::templates::settlement::test_support::*;

    #[test]
    fn service_inventory_mounts_the_shared_inventory_browser() {
        let rendered = inventory_rail(None, &[], &[], &[], None, false).into_string();
        assert!(rendered.contains("data-inventory-browser=\"service-personal\""));
        let inventory = [InventoryItem {
            id: 7,
            character_id: 1,
            item_id: "cooking_pot".into(),
            qty: 1,
        }];
        let rendered = inventory_rail(None, &inventory, &[], &[], None, false).into_string();
        assert!(rendered.contains("data-inventory-browser=\"service-personal\""));
        assert!(rendered.contains("data-personal-inventory-id=\"7\""));
    }

    #[test]
    fn npc_description_fallback_uses_the_neutral_person_silhouette() {
        let markup = npc_description_stage("Residents", "Select a resident.").into_string();
        assert!(markup.contains(
            "class=\"visual-stage-placeholder npc-portrait-silhouette\" aria-hidden=\"true\""
        ));
        assert!(!markup.contains('?',));

        let client = include_str!("../../../static/dialogue-client.js");
        assert!(client.contains("\"visual-stage-placeholder npc-portrait-silhouette\""));
        assert!(!client.contains("npc.initials || \"?\""));
    }

    #[test]
    fn companion_social_dialog_has_persisted_accessible_automation_control() {
        let character = |id: u64, name: &str| Character {
            id,
            name: name.into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: Some("lubeck".into()),
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        };
        let actor = character(1, "Ada");
        let target = character(2, "Greta");
        let location = LocationView {
            kind: super::super::LocationKind::Settlement,
            id: "lubeck".into(),
            name: "Lubeck".into(),
            religion_id: None,
            category: None,
            economy: None,
            active_building: Some("inn".into()),
        };
        let social = SocialPresentation {
            religion_id: Some("lutheran".into()),
            fame: 0.6,
            infamy: 0.2,
            automatic_chat_enabled: true,
            addressed_source_ids: vec!["concern".into()],
            ..Default::default()
        };
        let source = crate::spacetimedb::CharacterMoraleSource {
            id: "concern".into(),
            character_id: target.id,
            kind: "defeat".into(),
            label: "Recent defeat".into(),
            magnitude: -2.0,
        };
        let markup =
            party_social_dialog(&location, &target, &actor, &[source], &social).into_string();
        assert!(markup.contains("Automatic chats during downtime"));
        assert!(markup.contains("name=\"enabled\" value=\"true\" checked"));
        assert!(markup.contains("/party/2/social/automatic?building=inn"));
        assert!(markup.contains("data-automatic-social-chat"));
        assert!(markup.contains("name=\"action_id\" value=\"chat-"));
        assert!(markup.contains("name=\"requested_minutes\" min=\"15\" max=\"480\" step=\"15\""));
        assert!(markup.contains("/party/2/social/chat?building=inn"));
        assert!(markup.contains("according to your personality and relevant skills"));
        assert!(!markup.contains(">Save</button>"));
        assert!(!markup.contains("Use low-risk listening"));
        assert!(markup.contains("social-source-addressed"));
        assert!(markup.contains("Addressed by you"));
        assert!(!markup.contains("class=\"social-actions\""));
        assert!(markup.contains("role=\"tablist\" aria-label=\"Conversation topics\""));
        assert!(markup.contains(">Recent Tidings</span>"));
        assert!(markup.contains("data-affinity-trigger"));
        assert!(markup.contains("aria-expanded=\"false\""));
        assert!(markup.contains("Negative morale, -2.0"));
        assert!(markup.contains("--social-topic-color:color-mix"));
        assert!(!markup.contains("Local fame"));
        assert!(markup.contains("How many years hast thou seen?"));
        assert!(markup.contains("I have seen 20 years."));
        assert!(markup.contains("What faith dost thou profess?"));
        assert!(markup.contains("I am of the Lutheran confession."));
        assert!(markup.contains("What report dost thou bear in these parts?"));
        assert!(markup.contains("Folk here speak well of me"));
        assert!(markup.contains("data-local-chat-kind=\"player\" data-local-chat-subject=\"2\""));
        assert!(markup.contains("class=\"settlement-chat-messages\""));
        assert!(markup.contains("class=\"settlement-chat-composer\""));

        let self_markup = party_social_dialog(
            &location,
            &actor,
            &actor,
            &[CharacterMoraleSource {
                id: "self-concern".into(),
                character_id: actor.id,
                kind: "defeat".into(),
                label: "A private defeat".into(),
                magnitude: -1.0,
            }],
            &SocialPresentation::default(),
        )
        .into_string();
        assert!(self_markup.contains("value=\"reflect\""));
        assert!(self_markup.contains("data-social-self=\"true\""));
        assert!(!self_markup.contains("class=\"settlement-chat-composer\""));
        let ordinary_self = player_chat_area(&location, &actor, &actor).into_string();
        assert!(ordinary_self.contains(
            "data-party-social-href=\"/locations/settlement/lubeck/party/1/social?building=inn\""
        ));
        assert!(ordinary_self.contains("placeholder=\"Select Recent Tidings to reflect\""));
        assert!(ordinary_self.contains("name=\"body\" disabled"));

        let feedback_social = SocialPresentation {
            feedback: Some(SocialFeedback {
                message: "That approach needs time before it can be tried again.",
                is_error: true,
            }),
            ..social
        };
        let feedback_markup =
            party_social_dialog(&location, &target, &actor, &[], &feedback_social).into_string();
        assert!(
            feedback_markup
                .contains("class=\"chat-system-message social-feedback social-feedback-error\"")
        );
        assert!(feedback_markup.contains("role=\"alert\""));
        assert!(feedback_markup.contains("That approach needs time"));

        let response_social = SocialPresentation::default();
        let response_source = CharacterMoraleSource {
            id: "fresh-concern".into(),
            character_id: target.id,
            kind: "defeat".into(),
            label: "Another defeat".into(),
            magnitude: -2.0,
        };
        let response_markup = party_social_dialog(
            &location,
            &target,
            &actor,
            &[response_source],
            &response_social,
        )
        .into_string();
        assert!(response_markup.contains("class=\"social-action-icon\""));
        assert!(response_markup.contains("class=\"social-action-label\">Listen</span>"));
        assert!(!response_markup.contains(">Listen ("));
        assert!(response_markup.contains("Takes 5 minutes."));
        assert!(response_markup.contains("data-strategic-tooltip=\"Ask how they feel"));
        assert!(!response_markup.contains(
            "class=\"social-action\" aria-label=\"Ask how they feel about the defeat\" title="
        ));

        let injury_source = CharacterMoraleSource {
            id: "fresh-injury".into(),
            character_id: target.id,
            kind: "injury".into(),
            label: "Painful injury".into(),
            magnitude: -2.0,
        };
        let injury_markup = party_social_dialog(
            &location,
            &target,
            &actor,
            &[injury_source],
            &SocialPresentation::default(),
        )
        .into_string();
        assert!(injury_markup.contains("value=\"reassure\""));
        assert!(injury_markup.contains("class=\"social-action-label\">Reassure</span>"));
        assert!(injury_markup.contains("Sit at their bedside"));
        assert!(injury_markup.contains("Reassure. Sit at their bedside"));
        let reassurance_button = injury_markup
            .split("value=\"reassure\"")
            .nth(1)
            .and_then(|tail| tail.split("</button>").next())
            .expect("rendered Reassure button");
        assert!(reassurance_button.contains("Physiology"));
        assert!(reassurance_button.contains("low risk"));
        assert!(injury_markup.contains("value=\"deception\""));
        assert!(injury_markup.contains("Claim the injury is less serious than it looks"));
        let reframe_button = injury_markup
            .split("value=\"deception\"")
            .nth(1)
            .and_then(|tail| tail.split("</button>").next())
            .expect("rendered Injury Reframe button");
        assert!(reframe_button.contains("Deception"));
        assert!(reframe_button.contains("high risk"));
    }

    #[test]
    fn prayer_is_themed_and_disabled_with_an_accessible_reason() {
        let character = |id: u64, name: &str| Character {
            id,
            name: name.into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: Some("lubeck".into()),
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        };
        let location = LocationView {
            kind: super::super::LocationKind::Settlement,
            id: "lubeck".into(),
            name: "Lubeck".into(),
            religion_id: None,
            category: None,
            economy: None,
            active_building: None,
        };
        let source = CharacterMoraleSource {
            id: "defeat".into(),
            character_id: 2,
            kind: "defeat".into(),
            label: "Recent defeat".into(),
            magnitude: -2.0,
        };
        let social = SocialPresentation {
            religion_id: Some("lutheran".into()),
            prayer_disabled_reason: Some(
                "Your Zealous conviction prevents you from leading a companion's prayer.".into(),
            ),
            ..Default::default()
        };
        let markup = party_social_dialog(
            &location,
            &character(2, "Greta"),
            &character(1, "Ada"),
            &[source],
            &social,
        )
        .into_string();
        assert!(markup.contains("value=\"pray\""));
        assert!(markup.contains("Pray a psalm and recall Christ"));
        assert!(markup.contains("value=\"pray\" class=\"social-action\" disabled"));
        assert!(markup.contains("aria-disabled=\"true\""));
        assert!(markup.contains("Unavailable: Your Zealous conviction"));
        let css = include_str!("../../../static/css/strategic.css");
        assert!(css.contains(".social-action:disabled"));
        assert!(css.contains("filter: grayscale(0.8)"));
    }

    #[test]
    fn social_catalog_labels_are_generic_grounded_and_accessible() {
        use adventuresim_core::social::{SocialActionKind, SocialTopic};
        let defeat = SocialActionKind::Commiserate.description(SocialTopic::Defeat, true);
        assert_eq!(defeat, "Commiserate about the defeat");
        assert!(!defeat.to_ascii_lowercase().contains("goblin"));
        let actions = social_actions(false, SocialTopic::Defeat, false, false);
        assert_eq!(actions.len(), 7);
        assert!(
            actions
                .iter()
                .any(|(_, action, _)| *action == SocialActionKind::Listen)
        );
        assert_eq!(
            social_actions(true, SocialTopic::Defeat, true, true),
            vec![("inner-self", SocialActionKind::Reflect, "reflect")]
        );
        assert_eq!(
            social_action_label(SocialActionKind::Listen, false),
            "Listen"
        );
        assert_eq!(
            social_action_label(SocialActionKind::Commiserate, false),
            "Feign sympathy"
        );
        assert_eq!(
            social_actions(false, SocialTopic::Hunger, false, false).len(),
            5
        );
        assert_eq!(
            social_actions(false, SocialTopic::Faith, false, false).len(),
            5
        );
        let reserved = social_actions(false, SocialTopic::Defeat, true, true);
        assert_eq!(reserved.len(), 5);
        assert!(reserved.iter().all(|(_, action, _)| !matches!(
            action,
            SocialActionKind::LightenMood | SocialActionKind::Flirt
        )));
        assert!(
            reserved
                .iter()
                .any(|(_, action, _)| *action == SocialActionKind::Rally)
        );
        assert_eq!(SocialActionKind::Commiserate.skill_name(false), "Deception");
        assert_eq!(SocialActionKind::Reassure.skill_name(false), "Physiology");
        for topic in [
            SocialTopic::Injury,
            SocialTopic::Fatigue,
            SocialTopic::Hunger,
        ] {
            assert!(
                social_actions(false, topic, false, false)
                    .iter()
                    .any(|(_, action, value)| *action == SocialActionKind::Reassure
                        && *value == "reassure")
            );
        }
        for topic in [SocialTopic::Defeat, SocialTopic::Faith, SocialTopic::Filth] {
            assert!(
                social_actions(false, topic, false, false)
                    .iter()
                    .all(|(_, action, _)| *action != SocialActionKind::Reassure)
            );
        }
        assert_eq!(
            SocialActionKind::Flirt.description(SocialTopic::Injury, false),
            "Tell them the scar makes them look striking"
        );
        assert_eq!(familiarity_label(0.0), "0 hours");
        assert_eq!(familiarity_label(0.4), "<1 hours");
        assert_eq!(familiarity_label(9.4), "9 hours");
        let tooltip = belief_tooltip(&crate::spacetimedb::SocialBelief {
            id: "belief".into(),
            observer_id: 1,
            subject_id: 2,
            axis: crate::spacetimedb::BeliefAxis::SelfRegard,
            perceived_value: 1,
            confidence: 0.64,
            observed_at_minute: 0,
        });
        assert!(tooltip.contains("Confidence: 64%"));
        assert!(tooltip.contains("Injury is touchy"));
        assert_eq!(
            perceived_trait(crate::spacetimedb::BeliefAxis::Inclination, 1),
            ("Inclination", "Attracted to men and women")
        );
        assert_eq!(
            perceived_trait(crate::spacetimedb::BeliefAxis::Inclination, 3),
            ("Inclination", "Attracted to neither")
        );
        assert_eq!(
            perceived_trait(crate::spacetimedb::BeliefAxis::Conscience, 2),
            ("Conscience", "Callous")
        );
        assert_eq!(
            perceived_trait(crate::spacetimedb::BeliefAxis::Conscience, 3),
            ("Conscience", "Cruel")
        );
        assert_eq!(
            perceived_trait(crate::spacetimedb::BeliefAxis::Presentation, 1),
            ("Presentation", "Ambiguous")
        );
        assert_eq!(
            perceived_trait(crate::spacetimedb::BeliefAxis::Inclination, -1),
            ("Personality", "Uncertain")
        );
    }

    #[test]
    fn chat_uses_one_stream_with_all_channel_filters() {
        let markup = chat_area("Lubeck", None, None, None, None, None, &[]).into_string();

        assert!(markup.contains("role=\"tablist\""));
        assert!(markup.contains("data-dialogue-category=\"tidings\""));
        for channel in ["local", "party", "settlement", "dm", "guild", "info"] {
            assert!(
                markup.contains(&format!("data-chat-filter=\"{channel}\"")),
                "missing {channel} filter"
            );
        }
        assert!(markup.contains("data-chat-channel=\"info\""));
        assert!(!markup.contains("chat-channel-badge"));
        assert!(markup.contains("class=\"settlement-chat-layout\""));
        assert!(!markup.contains("data-dialogue-topic-pane"));
        assert!(!markup.contains("data-dialogue-topic-list"));
        assert!(markup.contains("data-dialogue-completion"));
        assert!(markup.contains("autocomplete=\"off\""));
        for label in ["Local", "Party", "Settlement", "DMs", "Guild", "Info"] {
            assert!(markup.contains(&format!("aria-label=\"{label}\" title=\"{label}\"")));
            assert!(!markup.contains(&format!(">{label}</")));
        }
        assert_eq!(markup.matches("class=\"conversation-tab\"").count(), 4);
        assert_eq!(markup.matches("class=\"sr-only\"").count(), 4);
        assert!(!markup.contains("class=\"visually-hidden\""));
    }

    #[test]
    fn settlement_resident_strip_exposes_accessible_authoritative_context() {
        let strip = npc_portrait_strip("lubeck", "market").into_string();
        assert!(strip.contains("aria-label=\"People here\""));
        assert!(strip.contains("data-npc-settlement=\"lubeck\""));
        assert!(strip.contains("data-npc-location=\"market\""));
        assert!(strip.contains("aria-label=\"Cook at fireplace\""));
        assert!(strip.contains("/locations/settlement/lubeck/fireplace?building=market"));
        let chat =
            settlement_resident_chat_area("Market", None, "lubeck", "market", Some("merchants"))
                .into_string();
        assert!(chat.contains("data-local-chat-kind=\"npc\""));
        assert!(chat.contains("data-local-chat-location=\"market\""));
        assert!(chat.contains("data-dialogue-catalog-revision"));
        assert!(!chat.contains("lubeck:merchants"));
        assert_eq!(npc_location_id("religion"), "church");
        assert_eq!(npc_location_id("inn"), "inn");
        assert_eq!(npc_location_id("books"), "bookstore");
        let church_strip = npc_portrait_strip("lubeck", "church").into_string();
        let square = npc_portrait_strip("lubeck", "public-square").into_string();
        assert!(!square.contains("Cook at fireplace"));
        assert!(church_strip.contains("Finding the people here…"));
        assert!(!church_strip.contains("â"));
    }

    #[test]
    fn weapons_service_chapter_exposes_the_generic_forge_entry_point() {
        let strip =
            npc_portrait_strip("viabundus-0", "organization-weaponsmith-guild").into_string();
        assert!(strip.contains("aria-label=\"Forge a weapon\""));
        assert!(strip.contains("href=\"/settlements/viabundus-0/weapons\""));
        assert_eq!(strip.matches("data-location-fixture").count(), 2);

        let client = include_str!("../../../static/dialogue-client.js");
        assert!(client.contains("querySelectorAll(\"[data-location-fixture]\")"));

        let unrelated = npc_portrait_strip("viabundus-0", "residences").into_string();
        assert!(!unrelated.contains("Forge a weapon"));
    }

    #[test]
    fn location_fixture_projection_keeps_people_and_things_independent() {
        let ordinary = location_fixtures("lubeck", "market", None);
        assert_eq!(ordinary.len(), 1);
        assert_eq!(ordinary[0].label, "Fireplace");

        let weapons = location_fixtures("lubeck", "organization-weaponsmith-guild", Some("weapons"));
        assert_eq!(weapons.len(), 2);
        assert_eq!(weapons[1].label, "Forge");

        assert!(location_fixtures("lubeck", "public-square", None).is_empty());
    }

    #[test]
    fn non_service_locations_use_the_same_authoritative_npc_shell() {
        let character = Character {
            id: 1,
            name: "Visitor".into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: Some("viabundus-1".into()),
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        };
        for location in ["residences", "keep"] {
            let markup = settlement_resident_location_page(
                &settlement(),
                &character,
                &[],
                location,
                Some("Visitor"),
            )
            .into_string();
            assert!(markup.contains(&format!("data-npc-location=\"{location}\"")));
            assert!(markup.contains("data-npc-strip"));
            assert!(markup.contains("data-dialogue-catalog-revision"));
            assert!(markup.contains("aria-label=\"Settlement places\""));
            assert!(markup.contains("href=\"/locations/settlement/viabundus-1/party/1\""));
            assert!(markup.contains("href=\"/locations/settlement/viabundus-1/party-inventory\""));
            assert!(!markup.contains(&format!("/places/{location}/party/")));
        }
    }

    #[test]
    fn chat_palette_meets_contrast_across_every_supported_theme() {
        fn linear_channel(channel: u8) -> f64 {
            let channel = f64::from(channel) / 255.0;
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        }

        fn luminance([red, green, blue]: [u8; 3]) -> f64 {
            0.2126 * linear_channel(red)
                + 0.7152 * linear_channel(green)
                + 0.0722 * linear_channel(blue)
        }

        fn contrast(first: [u8; 3], second: [u8; 3]) -> f64 {
            let (lighter, darker) = if luminance(first) > luminance(second) {
                (luminance(first), luminance(second))
            } else {
                (luminance(second), luminance(first))
            };
            (lighter + 0.05) / (darker + 0.05)
        }

        fn mix(accent: [u8; 3], text: [u8; 3], accent_percent: u16) -> [u8; 3] {
            std::array::from_fn(|index| {
                let mixed = u16::from(accent[index]) * accent_percent
                    + u16::from(text[index]) * (100 - accent_percent);
                ((mixed + 50) / 100) as u8
            })
        }

        // Dark themes use the lightest possible 88% panel composite (over
        // white); light themes use the darkest possible composite (over
        // black). This brackets the image content beneath the translucent chat.
        let legacy_palettes = [
            (
                "Dark Arcanum",
                [46, 49, 67],
                [200, 202, 208],
                [154, 158, 176],
                [96, 165, 250],
                [251, 191, 36],
                [215, 169, 239],
                [52, 211, 153],
            ),
            (
                "Fraktur Nocturne",
                [60, 49, 44],
                [241, 227, 207],
                [205, 185, 157],
                [125, 159, 197],
                [213, 166, 76],
                [213, 167, 237],
                [120, 173, 114],
            ),
            (
                "Fraktur Texturina",
                [216, 209, 190],
                [42, 31, 20],
                [74, 60, 44],
                [58, 106, 138],
                [184, 134, 11],
                [116, 66, 141],
                [74, 124, 63],
            ),
            (
                "Imperial Crimson",
                [217, 213, 204],
                [26, 26, 26],
                [61, 61, 61],
                [26, 74, 138],
                [196, 136, 11],
                [113, 63, 140],
                [45, 106, 48],
            ),
            (
                "Northern Frost",
                [211, 215, 220],
                [28, 40, 51],
                [52, 73, 94],
                [46, 109, 164],
                [212, 160, 23],
                [115, 66, 147],
                [39, 174, 96],
            ),
            (
                "Renaissance Gold",
                [216, 209, 190],
                [42, 31, 20],
                [74, 60, 44],
                [58, 106, 138],
                [184, 134, 11],
                [123, 63, 145],
                [74, 124, 63],
            ),
            (
                "Verdant Chronicle",
                [218, 214, 202],
                [26, 60, 26],
                [45, 90, 45],
                [74, 122, 106],
                [184, 115, 51],
                [116, 66, 141],
                [58, 122, 58],
            ),
        ];
        for (palette, surface, primary, secondary, info, gold, dm, success) in legacy_palettes {
            let channels = [
                ("Local", primary),
                ("Party", mix(info, primary, 40)),
                ("Settlement", mix(gold, primary, 35)),
                ("DM", mix(dm, primary, 35)),
                ("Guild", mix(success, primary, 40)),
                ("Info", secondary),
            ];
            let distinct = channels
                .iter()
                .map(|(_, color)| color)
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(
                distinct.len(),
                channels.len(),
                "{palette} channels must remain visually distinct"
            );
            for (channel, color) in channels {
                assert!(
                    contrast(color, surface) >= 4.5,
                    "{palette} {channel} does not meet WCAG AA text contrast"
                );
            }
        }
    }

    #[test]
    fn chat_css_keeps_fallbacks_and_mobile_message_space() {
        let css = include_str!("../../../static/css/strategic.css");
        let utilities = include_str!("../../../static/css/utilities.css");
        let trade_script = include_str!("../../../static/party-trade.js");
        let fallback = css
            .find("background: rgb(33 21 15 / 88%);")
            .expect("chat needs a background fallback");
        let enhanced = css
            .find("background: color-mix(in srgb, var(--panel-bg) 88%, transparent);")
            .expect("chat should derive its translucent surface from the fixed palette");

        assert!(fallback < enhanced);
        assert!(css.contains("background: color-mix(in srgb, var(--header-bg) 86%, transparent);"));
        assert!(css.contains(".chat-channel-filter input::after"));
        assert!(css.contains("text-decoration: line-through"));
        assert!(css.contains("outline: 2px solid var(--text-primary);"));
        assert!(css.contains("0 0 0 2px var(--panel-bg)"));
        assert!(css.contains(
            "--chat-party-color: color-mix(in srgb, var(--info) 40%, var(--text-primary));"
        ));
        assert!(css.contains("--chat-settlement-color: color-mix(in srgb, var(--gold-color) 35%, var(--text-primary));"));
        assert!(css.contains("--chat-dm-color: color-mix(in srgb, var(--icon-instinct, #7b3f91) 35%, var(--text-primary));"));
        assert!(css.contains(
            "--chat-guild-color: color-mix(in srgb, var(--success) 40%, var(--text-primary));"
        ));
        for variable in ["local", "party", "settlement", "dm", "guild", "info"] {
            assert!(css.contains(&format!("var(--chat-{variable}-color)")));
        }
        assert!(!css.contains(".chat-channel-badge"));
        assert!(css.contains("@media (max-width: 768px)"));
        assert!(css.contains("flex-wrap: nowrap;"));
        assert!(css.contains("min-height: 10rem;"));
        assert!(css.contains(".repair-custody-list { margin-top: auto; }"));
        assert!(css.contains("max-height: 50%;"));
        assert!(css.contains("@keyframes repairable-damage-pulse"));
        assert!(css.contains("@media (prefers-reduced-motion: reduce)"));
        let repairable_rule = css
            .split(".condition-repairable {")
            .nth(1)
            .and_then(|tail| tail.split('}').next())
            .expect("repairable condition segments need a style rule");
        assert!(!repairable_rule.contains("background-image"));
        assert!(!repairable_rule.contains("box-shadow"));
        for tier in 1..=5 {
            assert!(css.contains(&format!(".condition-tier-{tier}")));
            assert!(css.contains(&format!(".item-quality-{tier}")));
        }
        assert!(css.contains("color-mix(in srgb, var(--quality-color) 50%, var(--text-primary))"));
        assert!(css.contains("filter: brightness(1.15)"));
        assert!(css.contains("0%, 58%, 82%, 100%"));
        assert!(css.contains("66%, 74%"));
        assert!(!css.contains("left: -7rem;"));
        assert!(css.contains(".smith-wares-scroll .trade-inventory-table"));
        assert!(css.contains(".inn-rest-panel"));
        assert!(css.contains("max-height: 52%;"));
        assert!(css.contains(".inn-rest-panel > .rest-service-menu"));
        assert!(css.contains(".service-inventory-area .smith-wares-scroll"));
        assert!(css.contains("overflow-y: auto;"));
        assert!(css.contains("--inventory-merchant-action-overhang"));
        assert!(css.contains("--inventory-merchant-scrollbar-reserve: 8px;"));
        assert!(css.contains("padding-left: var(--inventory-merchant-scrollbar-reserve);"));
        assert!(css.contains("padding-right: var(--inventory-merchant-action-overhang);"));
        assert!(css.contains("direction: rtl;"));
        assert!(css.contains(".smith-wares-scroll > * { direction: ltr; }"));
        assert!(css.contains("scrollbar-gutter: stable;"));
        assert!(css.contains("overflow-x: clip;"));
        assert!(css.contains("col.inventory-column-item { width: auto; }"));
        assert!(css.contains(".smith-player-inventory-table"));
        assert!(css.contains("width: 3.65rem;"));
        assert!(css.contains("--repair-custody-action-overhang"));
        assert!(css.contains("width: calc(100% + var(--repair-custody-action-overhang));"));
        assert!(css.contains("padding-right: var(--repair-custody-action-overhang);"));
        assert!(css.contains("scrollbar-gutter: stable;"));
        assert!(utilities.contains(".inventory-row-actions.smith-player-actions"));
        assert!(utilities.contains("--inventory-action-bridge:.3rem"));
        assert!(!utilities.contains(".smith-wares-scroll .inventory-row-actions"));
        assert!(utilities.contains(".inventory-actions-cell"));
        assert!(
            utilities.contains(".left-sidebar .inventory-actions-cell > .inventory-row-actions")
        );
        assert!(
            utilities.contains(".right-sidebar .inventory-actions-cell > .inventory-row-actions")
        );
        assert!(utilities.contains("background:var(--inventory-row-background"));
        assert!(utilities.contains("top:0; bottom:0;"));
        assert!(utilities.contains(
            ".trade-inventory-row:not(:last-child) .inventory-row-actions { bottom:-1px; }"
        ));
        assert!(utilities.contains(".inventory-row-actions .trade-transfer:disabled"));
        assert!(utilities.contains("opacity:.42; transform:none;"));
        assert!(utilities.contains("left:100%; padding-left:var(--inventory-action-bridge);"));
        assert!(utilities.contains("right:100%; padding-right:var(--inventory-action-bridge);"));
        assert!(css.contains(".inventory-browser-table-frame"));
        assert!(css.contains("width:max-content;"));
        assert!(utilities.contains(".inventory-footer-repair .repair-all-button"));
        assert!(utilities.contains("grid-template-columns:repeat(2,1.35rem)"));
        assert!(utilities.contains(".inventory-actions-header > .inventory-footer-actions"));
        assert!(utilities.contains("thead:hover .inventory-footer-actions"));
        assert!(utilities.contains("background:var(--panel-bg)"));
        assert!(
            utilities
                .contains(".smith-player-actions .row-repair-form { position:static; order:0;")
        );
        assert!(trade_script.contains("if (stockRow) changeTradeDraftCount(stockRow, amount);"));
        assert!(trade_script.contains("function applyDynamicTransferModifiers(event)"));
        assert!(trade_script.contains("event.key === \"Shift\" || event.key === \"Control\""));
        assert!(trade_script.contains("controlKey ? \"all\""));
    }
}
