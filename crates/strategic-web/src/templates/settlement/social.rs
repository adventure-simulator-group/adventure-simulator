use super::*;

pub fn party_stats_page(
    location: &LocationView,
    selected: &Character,
    active_character: &Character,
    party_members: &[Character],
    capability: Option<&CharacterCapability>,
    selected_attributes: Option<&CharacterAttributes>,
    selected_skills: Option<&CharacterSkills>,
    selected_limbs: Option<&CharacterLimbs>,
    combat_profile: CombatTrainingProfile,
    condition: Option<&CharacterStrategicCondition>,
    morale_sources: &[crate::spacetimedb::CharacterMoraleSource],
    religion_id: Option<&str>,
    active_party: Option<&Party>,
    selected_party: Option<&Party>,
    notoriety: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    medical: &MedicalPresentation,
    can_examine: bool,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    filth: &[crate::spacetimedb::CharacterFilth],
    character_action_dialog: Option<Markup>,
    surgery_open: Option<&str>,
    social_open: bool,
) -> Markup {
    let selected_attributes_title = format!("{}'s attributes", selected.name);
    let selected_skills_title = format!("{}'s skills", selected.name);
    let examination_action = location.preserve_building(format!(
        "{}/party/{}/examine",
        location.base_path(),
        selected.id
    ));
    let surgery_path_template = location.preserve_building(format!(
        "{}/party/{}/surgery/__limb__",
        location.base_path(),
        selected.id
    ));
    let content = html! {
        aside class="left-sidebar" {
            (party_attributes_rail(&selected_attributes_title, selected_attributes, selected_limbs, medical, Some((&surgery_path_template, surgery_open)), injuries, projectiles))
            (strategic_condition_rail(condition, morale_sources, filth, &location.preserve_building(format!("{}/party/{}/social", location.base_path(), selected.id)), social_open))
            (medical_rail(medical, &location.base_path(), active_character.id, selected.id, true))
        }
        @if medical.examination_id.is_none() {
            @if let Some(dialog) = character_action_dialog { (dialog) }
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(
                party_members,
                Some(active_character),
                &location.base_path(),
                Some(selected.id),
                can_examine,
            ))
            (visual_stage("character", &selected.name, "Party member identity and capabilities"))
            (player_chat_area(selected, active_character))
            (medical_examination_popup(medical, location, selected.id, selected_limbs, injuries, projectiles))
        }
        aside class="right-sidebar" {
            (character_summary_rail(capability))
            (character_bio_rail(
                selected,
                religion_id,
                notoriety,
                personality,
                selected.id == active_character.id,
                &location.base_path(),
            ))
            (party_skills_rail(
                &selected_skills_title, selected_skills, selected_limbs, None, None, None,
                religion_id.is_some(), 0.0, religion_id.or(location.religion_id.as_deref()),
                combat_profile,
                CharacterSkillActions {
                    examination_action: can_examine.then_some(examination_action.as_str()),
                    examination_open: medical.examination_id.is_some(),
                    ..Default::default()
                },
            ))
            @if selected.id != active_character.id {
                @if active_character.party_id == selected.party_id {
                    @if active_party.is_some_and(|party| party.leader_id == selected.id) {
                        (sidebar_section("Party", html! {
                            form method="post" action=(format!("{}/party/{}/remove", location.base_path(), active_character.id)) {
                                button type="submit" class="btn btn-danger btn-block" { "Leave party" }
                            }
                        }))
                    } @else {
                        (sidebar_section("Party", html! {
                            form method="post" action=(format!("{}/party/{}/remove", location.base_path(), selected.id)) {
                                button type="submit" class="btn btn-danger btn-block" {
                                    @if active_party.is_some_and(|party| party.leader_id == active_character.id) { "Kick from party" }
                                    @else { "Request kick" }
                                }
                            }
                        }))
                    }
                } @else if let Some(party) = selected_party {
                    (sidebar_section("Party", html! {
                        p { (&party.name) }
                        form method="post" action=(format!("/parties/{}/join-general", party.id)) {
                            button type="submit" class="btn btn-primary btn-block" { "Request to join party" }
                        }
                    }))
                }
            }
        }
    };
    location.render_layout("Party stats", content, Some(&active_character.name))
}

#[derive(Debug, Clone, Default)]
pub struct SocialPresentation {
    pub affinity: f32,
    pub familiarity_hours: f32,
    pub religion_id: Option<String>,
    pub virtue: f32,
    pub beliefs: Vec<crate::spacetimedb::SocialBelief>,
    pub shared_concerns: Vec<adventuresim_core::social::SocialTopic>,
    pub unavailable: bool,
}

pub(super) fn social_actions(
    is_self: bool,
    topic: adventuresim_core::social::SocialTopic,
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
        ("juggler", LightenMood, "humor"),
        ("crown", Rally, "command"),
        ("conversation", Reframe, "deception"),
        ("rose", Flirt, "seduction"),
    ]
    .into_iter()
    .filter(|(_, action, _)| action.available_for(topic))
    .collect()
}

pub(super) fn perceived_trait(axis: &str, value: i8) -> (&'static str, &'static str) {
    match (axis, value.signum()) {
        ("drive", 1) => ("Drive", "Ambitious"),
        ("drive", -1) => ("Drive", "Content"),
        ("self_regard", 1) => ("Self-regard", "Proud"),
        ("self_regard", -1) => ("Self-regard", "Humble"),
        ("conviction", 1) => ("Conviction", "Zealous"),
        ("conviction", -1) => ("Conviction", "Irreverent"),
        ("hygiene", 1) => ("Hygiene", "Cleanly"),
        ("hygiene", -1) => ("Hygiene", "Slovenly"),
        ("drive", _) => ("Drive", "Neutral"),
        ("self_regard", _) => ("Self-regard", "Neutral"),
        ("conviction", _) => ("Conviction", "Neutral"),
        ("hygiene", _) => ("Hygiene", "Neutral"),
        _ => ("Personality", "Uncertain"),
    }
}

pub(super) fn familiarity_label(hours: f32) -> String {
    if hours.is_finite() && hours > 0.0 && hours < 1.0 {
        "<1 hours".into()
    } else {
        format!("{:.0} hours", hours.max(0.0))
    }
}

pub(super) fn belief_style(confidence: f32) -> String {
    format!(
        "--belief-confidence:{:.0}%",
        confidence.clamp(0.0, 1.0) * 100.0
    )
}

pub(super) fn personality_reaction_hint(axis: &str, value: i8) -> &'static str {
    match (axis, value.signum()) {
        ("drive", 1) => {
            "Likely reaction: Rallying can motivate them after defeat; pity or flippancy may offend."
        }
        ("drive", -1) => {
            "Likely reaction: Listening and commiseration are safer than pressuring them to prove themselves."
        }
        ("self_regard", 1) => {
            "Likely reaction: Injury is touchy; admiration may land better than pity or minimizing the wound."
        }
        ("self_regard", -1) => {
            "Likely reaction: Plain sympathy is safer; conspicuous flattery may feel insincere."
        }
        ("conviction", 1) => {
            "Likely reaction: Treat moral concerns seriously; jokes and false reassurance are especially risky."
        }
        ("conviction", -1) => {
            "Likely reaction: Gentle reframing may work better than appeals to duty or conviction."
        }
        ("hygiene", 1) => {
            "Likely reaction: Filth is genuinely upsetting; acknowledge it rather than dismissing the concern."
        }
        ("hygiene", -1) => {
            "Likely reaction: They may not share strong concern about grime, so forceful reassurance can seem strange."
        }
        _ => "Likely reaction: Their response to riskier social actions remains uncertain.",
    }
}

pub(super) fn belief_tooltip(belief: &crate::spacetimedb::SocialBelief) -> String {
    format!(
        "Confidence: {:.0}%\n{}",
        belief.confidence.clamp(0.0, 1.0) * 100.0,
        personality_reaction_hint(&belief.axis, belief.perceived_value)
    )
}

/// Dedicated social view. It intentionally receives observer-specific beliefs
/// rather than authoritative personality.
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
    let affinity_label = match social.affinity {
        value if value >= 50.0 => "Devoted",
        value if value >= 15.0 => "Warm",
        value if value <= -50.0 => "Hostile",
        value if value <= -15.0 => "Cold",
        _ => "Neutral",
    };
    let is_self = selected.id == active_character.id;
    let affinity_certainty = if social.familiarity_hours >= 48.0 {
        "fairly certain"
    } else if social.familiarity_hours >= 8.0 {
        "tentative"
    } else {
        "uncertain"
    };
    let close_href = location.preserve_building(if is_self {
        format!("{}/party/{}", location.base_path(), selected.id)
    } else {
        format!("{}/party/{}/stats", location.base_path(), selected.id)
    });
    html! {
        div class="character-action-overlay" data-character-action-dialog {
            a class="character-action-backdrop" href=(&close_href) aria-label="Close social dialog" {}
            section class="character-action-dialog social-dialog" role="dialog" aria-modal="true" aria-labelledby="social-dialog-title" tabindex="-1" {
                header class="character-action-dialog-header" {
                    h2 id="social-dialog-title" { "Social — " (selected.name) }
                    a class="character-action-dialog-close" href=(&close_href) aria-label="Close social dialog" { "×" }
                }
                div class="social-rail" data-social-panel data-target-id=(selected.id) {
            (sidebar_section("What you believe", html! {
                dl class="social-biography" {
                    div { dt { "Age" } dd { (selected.age_years) } }
                    div { dt { "Religion" } dd { (religion_name(social.religion_id.as_deref())) } }
                    div { dt { "Virtue" } dd { (format!("{:+.0}", social.virtue)) } }
                    @if !is_self {
                        div { dt { "Affinity toward you" } dd { (affinity_label) " (" (affinity_certainty) ")" } }
                        div { dt { "Familiarity" } dd { (familiarity_label(social.familiarity_hours)) } }
                    }
                }
                @if social.unavailable {
                    p class="social-unavailable" role="status" { "Your impressions are unavailable right now." }
                } @else if social.beliefs.is_empty() {
                    p class="text-muted small-copy" { "You have not formed a confident impression of their personality yet." }
                } @else {
                    ul class="perceived-traits" aria-label="Perceived personality traits" {
                        @for belief in &social.beliefs {
                            @let (_, value) = perceived_trait(&belief.axis, belief.perceived_value);
                            li class="perceived-trait" style=(belief_style(belief.confidence))
                                tabindex="0" data-strategic-tooltip=(belief_tooltip(belief)) {
                                (value)
                            }
                        }
                    }
                }
            }))
            (sidebar_section("Morale sources", html! {
                @if morale_sources.is_empty() { p class="text-muted" { "No current morale effects." } }
                div class="social-source-list" {
                    @for source in morale_sources {
                        @let topic = adventuresim_core::social::topic_for_source_kind(&source.kind);
                        article class=(if source.magnitude < 0.0 { "social-source social-source-negative" } else { "social-source social-source-positive" }) {
                            div class="social-source-context" {
                                div { strong { (&source.label) } span { (format!("{:+.1}", source.magnitude)) } }
                                @if let Some(axis) = topic.and_then(adventuresim_core::social::axis_for_topic) {
                                    @if let Some(belief) = social.beliefs.iter().find(|belief| belief.axis == axis.slug()) {
                                        @let (axis_name, value) = perceived_trait(&belief.axis, belief.perceived_value);
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
                            @if source.magnitude < 0.0 {
                                @if let Some(topic) = topic {
                                  div class="social-actions" aria-label=(format!("Actions for {}", source.label)) {
                                    @let shares_concern = social.shared_concerns.contains(&topic);
                                    @for (default_icon, action, value) in social_actions(is_self, topic) {
                                      @let action_shares_concern = action != adventuresim_core::social::SocialActionKind::Commiserate || shares_concern;
                                      @let icon = if action == adventuresim_core::social::SocialActionKind::Commiserate && !shares_concern { "conversation" } else { default_icon };
                                      @let description = action.description(topic, action_shares_concern);
                                    form method="post" action=(&social_href) {
                                        input type="hidden" name="source_id" value=(&source.id);
                                        button type="submit" name="action_kind" value=(value) class="social-action"
                                            aria-label=(description) title=(description) data-strategic-tooltip=(format!("{}\n{} · {} risk", description, action.skill_name(action_shares_concern), if action.risk() >= 0.6 { "high" } else if action.risk() >= 0.3 { "moderate" } else { "low" })) {
                                            (decorative_game_icon(icon))
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
            }
        }
    }
}

/// Shared chat panel. Local conversations are live; the remaining channel
/// filters are present so their messages can join the same stream as their
/// backends become available.
pub(crate) fn settlement_chat_area(location: &str, active_character: Option<&Character>) -> Markup {
    chat_area(location, active_character, None, None, None, &[])
}

pub(crate) fn settlement_chat_area_with_info(
    location: &str,
    active_character: Option<&Character>,
    info_messages: &[String],
) -> Markup {
    chat_area(location, active_character, None, None, None, info_messages)
}

pub(super) fn player_chat_area(subject: &Character, active_character: &Character) -> Markup {
    let context = ("player", subject.id.to_string());
    chat_area(
        &subject.name,
        Some(active_character),
        None,
        Some(context),
        None,
        &[],
    )
}

pub(super) fn npc_location_id(service_id: &str) -> &str {
    match service_id {
        "merchants" => "market",
        "weapons" => "forge",
        "armor" => "armoury",
        "clothing" => "tailor",
        other => other,
    }
}

pub(super) fn npc_portrait_strip(settlement_id: &str, location_id: &str) -> Markup {
    html! {
        nav class="settlement-npc-strip" aria-label="People here" data-npc-strip
            data-npc-settlement=(settlement_id) data-npc-location=(location_id) {
            span class="text-muted" data-npc-loading { "Finding the people hereâ€¦" }
        }
    }
}

pub(super) fn npc_description_stage(name: &str, fallback: &str) -> Markup {
    html! { section class="visual-stage npc-description-stage" data-npc-description aria-live="polite" {
        div class="visual-stage-placeholder" aria-hidden="true" { "?" }
        h2 { (name) }
        p { (fallback) }
    } }
}

pub(super) fn settlement_npc_chat_area(
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
        &[],
    )
}

pub(super) fn chat_area(
    location: &str,
    _active_character: Option<&Character>,
    service_context: Option<(&str, &str)>,
    local_context: Option<(&str, String)>,
    local_location_id: Option<&str>,
    info_messages: &[String],
) -> Markup {
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
            data-local-chat-location=[local_location_id] {
            div class="settlement-chat-resize" role="separator" aria-label="Resize chat"
                aria-orientation="horizontal" aria-valuemin="128" aria-valuemax="640"
                aria-valuenow="184" tabindex="0" title="Drag to resize chat" {
                span aria-hidden="true" {}
            }
            div class="settlement-chat-layout" {
                div class="settlement-chat-conversation" {
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
                            input type="text" name="body" disabled[local_context.is_none()]
                                aria-label="Local message"
                                autocomplete="off"
                                placeholder=(format!("Message {location} (Local)"));
                        }
                        button type="button" class="btn btn-primary btn-icon" disabled[local_context.is_none()]
                            aria-label="Send message" {
                            (decorative_game_icon("plain-arrow"))
                        }
                    }
                }
                aside class="settlement-chat-topics" data-dialogue-topic-pane hidden
                    aria-label="Dialogue topics" {
                    h3 { "Topics" }
                    ul data-dialogue-topic-list {}
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
    trade_action: Option<(&str, &str)>,
    _show_repair: bool,
) -> Markup {
    let title = active_character
        .map(|character| format!("{}'s inventory", character.name))
        .unwrap_or_else(|| "Your inventory".to_string());

    html! {
        (sidebar_section(&title, html! {
            @if inventory.is_empty() {
                p class="text-muted small-copy" { "No items carried." }
            } @else {
                table class="trade-inventory-table" {
                    (trade_inventory_table_header(false, None))
                    tbody {
                    @for item in inventory {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let item_name = item_display_name(&item.item_id);
                        tr class=(if trade_action.is_some() { "trade-inventory-row" } else { "trade-inventory-row inventory-row-readonly" }) {
                            td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                            td class="inventory-item-name" {
                                (item_name_with_quality(&item.item_id, definition))
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
        }))
    }
}
