//! Durable strategic relationships and authoritative social actions.

use adventuresim_core::skill::Skill;
use adventuresim_core::social::{
    AFFINITY_MAX, AFFINITY_MIN, PersonalityAxis, SOCIAL_COOLDOWN_MINUTES, SocialActionKind,
    SocialAttempt, SocialTopic, affinity_gain, axis_for_topic, canonical_cooldown_id,
    canonical_pair, choose_automatic_social_action, diagnosed_axis, diagnosis_for_axis,
    resolve_social_attempt, settle_affinity, social_source_eligible, topic_for_source_kind,
};
use spacetimedb::{ReducerContext, Table, ViewContext, reducer, table, view};

use crate::character::{character, character__view};
use crate::condition::{character_morale_source__view, morale_event};
use crate::strategic::strategic_gateway_authority__view;
use crate::{
    character_morale_source, character_personality, character_strategic_condition, character_time,
};

pub const MAX_AUTOMATIC_SOCIAL_ATTEMPTS_PER_DOWNTIME: usize = 3;

#[derive(Clone, Debug)]
#[table(accessor = character_affinity)]
pub struct CharacterAffinity {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub subject_id: u64,
    #[index(btree)]
    pub actor_id: u64,
    pub anchor: f32,
    pub anchor_minute: u64,
}

/// Symmetric relationship time. `low_id` is always lower than `high_id`.
#[derive(Clone, Debug)]
#[table(accessor = character_familiarity)]
pub struct CharacterFamiliarity {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub low_id: u64,
    #[index(btree)]
    pub high_id: u64,
    pub shared_minutes: u64,
    /// Last minimum personal clock observed while this pair shared a party.
    pub joint_minute_anchor: u64,
}

/// Observer-specific diagnosis. This table is intentionally private: the SSR
/// gateway requests only the current observer's rows and fails closed.
#[derive(Clone, Debug)]
#[table(accessor = social_belief)]
pub struct SocialBelief {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub observer_id: u64,
    #[index(btree)]
    pub subject_id: u64,
    pub axis: String,
    pub perceived_value: i8,
    pub confidence: f32,
    pub observed_at_minute: u64,
}

fn is_strategic_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|authority| authority.identity == ctx.sender())
}

#[view(accessor = backend_character_affinities, public)]
pub fn backend_character_affinities(ctx: &ViewContext) -> Vec<CharacterAffinity> {
    if !is_strategic_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .character_affinity()
        .subject_id()
        .filter(0u64..)
        .collect()
}

#[view(accessor = backend_character_familiarities, public)]
pub fn backend_character_familiarities(ctx: &ViewContext) -> Vec<CharacterFamiliarity> {
    if !is_strategic_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .character_familiarity()
        .low_id()
        .filter(0u64..)
        .collect()
}

/// Trusted SSR projection boundary. Browsers do not receive this view in the
/// live subscription set; the gateway filters it to the active observer.
#[view(accessor = backend_social_beliefs, public)]
pub fn backend_social_beliefs(ctx: &ViewContext) -> Vec<SocialBelief> {
    if !is_strategic_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .social_belief()
        .observer_id()
        .filter(0u64..)
        .collect()
}

#[derive(Clone, Debug)]
#[table(accessor = social_interaction)]
pub struct SocialInteraction {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub actor_id: u64,
    #[index(btree)]
    pub target_id: u64,
    pub source_id: String,
    pub topic: String,
    pub action_kind: String,
    pub succeeded: bool,
    pub morale_delta: f32,
    pub occurred_at_minute: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = social_address)]
pub struct SocialAddress {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub actor_id: u64,
    #[index(btree)]
    pub target_id: u64,
    pub source_id: String,
    pub addressed_at_minute: u64,
}

/// Compact current success projection. Durable attempts remain in
/// `social_interaction`; routine pages never replay that lifetime history.
#[view(accessor = backend_social_addresses, public)]
pub fn backend_social_addresses(ctx: &ViewContext) -> Vec<SocialAddress> {
    if !is_strategic_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .social_address()
        .actor_id()
        .filter(0u64..)
        .filter(|row| {
            ctx.db
                .character_morale_source()
                .id()
                .find(&row.source_id)
                .is_some_and(|source| source.character_id == row.target_id)
        })
        .collect()
}

#[derive(Clone, Debug)]
#[table(accessor = automatic_social_chat)]
pub struct AutomaticSocialChat {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub actor_id: u64,
    #[index(btree)]
    pub target_id: u64,
    pub enabled: bool,
}

#[view(accessor = backend_automatic_social_chats, public)]
pub fn backend_automatic_social_chats(ctx: &ViewContext) -> Vec<AutomaticSocialChat> {
    if !is_strategic_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .automatic_social_chat()
        .actor_id()
        .filter(0u64..)
        .filter(|row| row.enabled)
        .filter(|row| {
            let Some(actor) = ctx.db.character().id().find(row.actor_id) else {
                return false;
            };
            let Some(target) = ctx.db.character().id().find(row.target_id) else {
                return false;
            };
            actor.alive
                && target.alive
                && actor.party_id.is_some()
                && actor.party_id == target.party_id
        })
        .collect()
}

#[derive(Clone, Debug)]
#[table(accessor = social_action_cooldown)]
pub struct SocialActionCooldown {
    #[primary_key]
    pub id: String,
    pub actor_id: u64,
    pub target_id: u64,
    pub topic: String,
    pub action_kind: String,
    pub available_at_minute: u64,
}

fn affinity_id(subject_id: u64, actor_id: u64) -> String {
    format!("{subject_id}:{actor_id}")
}
fn pair_id(low_id: u64, high_id: u64) -> String {
    format!("{low_id}:{high_id}")
}

fn automatic_chat_id(actor_id: u64, target_id: u64) -> String {
    format!("{actor_id}:{target_id}")
}

fn social_address_id(actor_id: u64, target_id: u64, source_id: &str) -> String {
    format!("{actor_id}:{target_id}:{source_id}")
}

fn source_addressed(ctx: &ReducerContext, actor_id: u64, target_id: u64, source_id: &str) -> bool {
    ctx.db
        .social_address()
        .id()
        .find(&social_address_id(actor_id, target_id, source_id))
        .is_some()
}

#[reducer]
pub fn set_automatic_social_chat(
    ctx: &ReducerContext,
    actor_id: u64,
    target_id: u64,
    enabled: bool,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    if actor_id == target_id {
        return Err("Automatic chats require a companion".into());
    }
    let actor = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .ok_or("Actor not found")?;
    let target = ctx
        .db
        .character()
        .id()
        .find(target_id)
        .ok_or("Target not found")?;
    if !actor.alive || !target.alive {
        return Err("Both characters must be living".into());
    }
    if actor.party_id.is_none() || actor.party_id != target.party_id {
        return Err("Automatic chats require the same party".into());
    }
    let id = automatic_chat_id(actor_id, target_id);
    if !enabled {
        ctx.db.automatic_social_chat().id().delete(&id);
        return Ok(());
    }
    let row = AutomaticSocialChat {
        id: id.clone(),
        actor_id,
        target_id,
        enabled: true,
    };
    if ctx.db.automatic_social_chat().id().find(&id).is_some() {
        ctx.db.automatic_social_chat().id().update(row);
    } else {
        ctx.db.automatic_social_chat().insert(row);
    }
    Ok(())
}

/// Remove pair preferences that can no longer run. This is called from the
/// infrequent party/death lifecycle paths so the trusted view remains bounded
/// to current, living party relationships.
pub(crate) fn prune_invalid_automatic_social_chats(ctx: &ReducerContext) {
    for row in ctx
        .db
        .automatic_social_chat()
        .actor_id()
        .filter(0u64..)
        .collect::<Vec<_>>()
    {
        let valid = row.enabled
            && ctx
                .db
                .character()
                .id()
                .find(row.actor_id)
                .is_some_and(|actor| {
                    actor.alive
                        && actor.party_id.is_some()
                        && ctx
                            .db
                            .character()
                            .id()
                            .find(row.target_id)
                            .is_some_and(|target| target.alive && actor.party_id == target.party_id)
                });
        if !valid {
            ctx.db.automatic_social_chat().id().delete(&row.id);
        }
    }
}

pub fn current_affinity(ctx: &ReducerContext, subject_id: u64, actor_id: u64) -> f32 {
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(subject_id)
        .map_or(0, |v| v.minutes);
    ctx.db
        .character_affinity()
        .id()
        .find(&affinity_id(subject_id, actor_id))
        .map_or(0.0, |row| {
            settle_affinity(row.anchor, now.saturating_sub(row.anchor_minute))
        })
}

fn put_affinity(ctx: &ReducerContext, subject_id: u64, actor_id: u64, value: f32) {
    let id = affinity_id(subject_id, actor_id);
    let row = CharacterAffinity {
        id: id.clone(),
        subject_id,
        actor_id,
        anchor: value.clamp(AFFINITY_MIN, AFFINITY_MAX),
        anchor_minute: ctx
            .db
            .character_time()
            .character_id()
            .find(subject_id)
            .map_or(0, |v| v.minutes),
    };
    if ctx.db.character_affinity().id().find(&id).is_some() {
        ctx.db.character_affinity().id().update(row);
    } else {
        ctx.db.character_affinity().insert(row);
    }
}

/// Settle canonical familiarity whenever either member's personal strategic
/// clock changes. Taking the minimum clock makes simultaneous party time
/// partition-independent and prevents double counting.
pub fn settle_shared_party_time(ctx: &ReducerContext, character_id: u64) {
    let Some(subject) = ctx.db.character().id().find(character_id) else {
        return;
    };
    let Some(party_id) = subject.party_id.as_deref() else {
        return;
    };
    if !subject.alive {
        return;
    }
    let subject_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |v| v.minutes);
    let peers: Vec<_> = ctx
        .db
        .character()
        .iter()
        .filter(|v| v.party_id.as_deref() == Some(party_id) && v.alive && v.id != character_id)
        .collect();
    for peer in peers {
        let Some((low_id, high_id)) = canonical_pair(character_id, peer.id) else {
            continue;
        };
        let peer_minute = ctx
            .db
            .character_time()
            .character_id()
            .find(peer.id)
            .map_or(0, |v| v.minutes);
        let joint = subject_minute.min(peer_minute);
        let id = pair_id(low_id, high_id);
        if let Some(mut row) = ctx.db.character_familiarity().id().find(&id) {
            row.shared_minutes = row
                .shared_minutes
                .saturating_add(joint.saturating_sub(row.joint_minute_anchor));
            row.joint_minute_anchor = row.joint_minute_anchor.max(joint);
            ctx.db.character_familiarity().id().update(row);
        } else {
            ctx.db.character_familiarity().insert(CharacterFamiliarity {
                id,
                low_id,
                high_id,
                shared_minutes: 0,
                joint_minute_anchor: joint,
            });
        }
    }
}

/// Joining or rejoining starts a fresh joint-clock anchor so time spent apart
/// is never counted as familiarity.
pub fn reset_familiarity_after_join(ctx: &ReducerContext, character_id: u64) {
    let Some(subject) = ctx.db.character().id().find(character_id) else {
        return;
    };
    let Some(party_id) = subject.party_id.as_deref() else {
        return;
    };
    let subject_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |v| v.minutes);
    for peer in ctx
        .db
        .character()
        .iter()
        .filter(|v| v.alive && v.id != character_id && v.party_id.as_deref() == Some(party_id))
    {
        let Some((low_id, high_id)) = canonical_pair(character_id, peer.id) else {
            continue;
        };
        let joint = subject_minute.min(
            ctx.db
                .character_time()
                .character_id()
                .find(peer.id)
                .map_or(0, |v| v.minutes),
        );
        let id = pair_id(low_id, high_id);
        if let Some(mut row) = ctx.db.character_familiarity().id().find(&id) {
            row.joint_minute_anchor = joint;
            ctx.db.character_familiarity().id().update(row);
        } else {
            ctx.db.character_familiarity().insert(CharacterFamiliarity {
                id,
                low_id,
                high_id,
                shared_minutes: 0,
                joint_minute_anchor: joint,
            });
        }
    }
}

fn parse_action(value: &str) -> Result<SocialActionKind, String> {
    match value {
        "reflect" => Ok(SocialActionKind::Reflect),
        "listen" => Ok(SocialActionKind::Listen),
        "commiserate" => Ok(SocialActionKind::Commiserate),
        "humor" => Ok(SocialActionKind::LightenMood),
        "command" => Ok(SocialActionKind::Rally),
        "deception" => Ok(SocialActionKind::Reframe),
        "seduction" => Ok(SocialActionKind::Flirt),
        _ => Err("Unknown social action".into()),
    }
}

fn social_action_skill(action: SocialActionKind, shares_concern: bool) -> Skill {
    match action {
        SocialActionKind::Reflect => Skill::SelfAwareness,
        SocialActionKind::Listen => Skill::Insight,
        SocialActionKind::Commiserate if shares_concern => Skill::Insight,
        SocialActionKind::Commiserate => Skill::Deception,
        SocialActionKind::LightenMood => Skill::Humor,
        SocialActionKind::Rally => Skill::Command,
        SocialActionKind::Reframe => Skill::Deception,
        SocialActionKind::Flirt => Skill::Seduction,
    }
}

fn automatic_personality_fit(
    personality: &crate::personality::CharacterPersonality,
    action: SocialActionKind,
    topic: SocialTopic,
) -> f32 {
    use crate::personality::{
        Conscience, Conviction, Drive, Nerve, Outlook, SelfRegard, Sociability,
    };

    let mut fit = 0.0;
    match personality.conscience {
        Conscience::Compassionate
            if matches!(
                action,
                SocialActionKind::Listen | SocialActionKind::Commiserate
            ) =>
        {
            fit += 1.0;
        }
        Conscience::Callous | Conscience::Cruel if action == SocialActionKind::Reframe => {
            fit += 0.75;
        }
        Conscience::Callous | Conscience::Cruel
            if matches!(
                action,
                SocialActionKind::Listen | SocialActionKind::Commiserate
            ) =>
        {
            fit -= 0.75;
        }
        _ => {}
    }
    match personality.sociability {
        Sociability::Gregarious
            if matches!(
                action,
                SocialActionKind::Commiserate
                    | SocialActionKind::LightenMood
                    | SocialActionKind::Flirt
            ) =>
        {
            fit += 0.75;
        }
        Sociability::Solitary if action == SocialActionKind::Listen => fit += 0.5,
        Sociability::Solitary
            if matches!(
                action,
                SocialActionKind::LightenMood | SocialActionKind::Flirt
            ) =>
        {
            fit -= 0.75;
        }
        _ => {}
    }
    match personality.outlook {
        Outlook::Sanguine if action == SocialActionKind::LightenMood => fit += 1.0,
        Outlook::Brooding
            if matches!(action, SocialActionKind::Listen | SocialActionKind::Reframe) =>
        {
            fit += 0.5;
        }
        Outlook::Brooding if action == SocialActionKind::LightenMood => fit -= 0.5,
        _ => {}
    }
    match personality.drive {
        Drive::Ambitious if action == SocialActionKind::Rally => fit += 1.0,
        Drive::Content
            if matches!(
                action,
                SocialActionKind::Listen | SocialActionKind::Commiserate
            ) =>
        {
            fit += 0.5;
        }
        Drive::Content if action == SocialActionKind::Rally => fit -= 0.5,
        _ => {}
    }
    match personality.nerve {
        Nerve::Brave if action == SocialActionKind::Rally => fit += 0.75,
        Nerve::Fearful if action == SocialActionKind::Listen => fit += 0.5,
        Nerve::Fearful if action == SocialActionKind::Rally => fit -= 0.5,
        _ => {}
    }
    match personality.self_regard {
        SelfRegard::Proud
            if matches!(action, SocialActionKind::Rally | SocialActionKind::Flirt) =>
        {
            fit += 0.5;
        }
        SelfRegard::Humble
            if matches!(
                action,
                SocialActionKind::Listen | SocialActionKind::Commiserate
            ) =>
        {
            fit += 0.5;
        }
        SelfRegard::Humble if action == SocialActionKind::Flirt => fit -= 0.25,
        _ => {}
    }
    if topic == SocialTopic::Faith {
        match personality.conviction {
            Conviction::Zealous if action == SocialActionKind::Rally => fit += 0.75,
            Conviction::Irreverent if action == SocialActionKind::Reframe => fit += 0.75,
            _ => {}
        }
    }
    fit
}

fn automatic_social_action(
    ctx: &ReducerContext,
    actor_id: u64,
    target_id: u64,
    topic: SocialTopic,
) -> Result<Option<SocialActionKind>, String> {
    const ACTIONS: [SocialActionKind; 6] = [
        SocialActionKind::Listen,
        SocialActionKind::Commiserate,
        SocialActionKind::LightenMood,
        SocialActionKind::Rally,
        SocialActionKind::Reframe,
        SocialActionKind::Flirt,
    ];

    let shares_concern = shares_concern(ctx, actor_id, topic);
    let personality = crate::personality::personality_or_neutral(ctx, actor_id);
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(target_id)
        .map_or(0, |row| row.minutes);
    let language = crate::character::shared_language_coefficient(ctx, actor_id, target_id);
    let mut candidates = Vec::with_capacity(ACTIONS.len());
    for action in ACTIONS
        .into_iter()
        .filter(|action| action.available_for(topic))
    {
        let action_kind = action.reducer_value();
        let cooldown_id = canonical_cooldown_id(actor_id, target_id, topic, action_kind);
        if ctx
            .db
            .social_action_cooldown()
            .id()
            .find(&cooldown_id)
            .is_some_and(|cooldown| now < cooldown.available_at_minute)
        {
            continue;
        }
        let skill_check = adventuresim_world_schema::language_scaled_effect(
            crate::condition::mental_check(
                ctx,
                actor_id,
                social_action_skill(action, shares_concern),
            )?,
            language,
        );
        candidates.push((
            action,
            skill_check,
            automatic_personality_fit(&personality, action, topic),
        ));
    }
    Ok(choose_automatic_social_action(topic, candidates))
}

fn sensitivity(ctx: &ReducerContext, target_id: u64, topic: SocialTopic) -> f32 {
    let Some(p) = ctx
        .db
        .character_personality()
        .character_id()
        .find(target_id)
    else {
        return 0.5;
    };
    match topic {
        SocialTopic::Defeat => {
            if p.drive == crate::personality::Drive::Ambitious {
                1.0
            } else {
                0.45
            }
        }
        SocialTopic::Faith => {
            if p.conviction == crate::personality::Conviction::Zealous {
                1.0
            } else {
                0.5
            }
        }
        SocialTopic::Filth => {
            if p.hygiene == crate::personality::Hygiene::Cleanly {
                0.9
            } else {
                0.35
            }
        }
        SocialTopic::Injury => {
            if p.self_regard == crate::personality::SelfRegard::Proud {
                0.8
            } else {
                0.4
            }
        }
        _ => 0.4,
    }
}

fn shares_concern(ctx: &ReducerContext, character_id: u64, topic: SocialTopic) -> bool {
    ctx.db
        .character_morale_source()
        .character_id()
        .filter(character_id)
        .any(|source| {
            social_source_eligible(&source.kind, source.magnitude)
                && topic_for_source_kind(&source.kind) == Some(topic)
        })
}

fn personality_truth(ctx: &ReducerContext, target_id: u64, axis: PersonalityAxis) -> Option<i8> {
    let p = ctx
        .db
        .character_personality()
        .character_id()
        .find(target_id)?;
    Some(match axis {
        PersonalityAxis::Drive => match p.drive {
            crate::personality::Drive::Ambitious => 1,
            crate::personality::Drive::Content => -1,
            crate::personality::Drive::Neutral => 0,
        },
        PersonalityAxis::SelfRegard => match p.self_regard {
            crate::personality::SelfRegard::Proud => 1,
            crate::personality::SelfRegard::Humble => -1,
            crate::personality::SelfRegard::Neutral => 0,
        },
        PersonalityAxis::Conviction => match p.conviction {
            crate::personality::Conviction::Zealous => 1,
            crate::personality::Conviction::Irreverent => -1,
            crate::personality::Conviction::Neutral => 0,
        },
        PersonalityAxis::Hygiene => match p.hygiene {
            crate::personality::Hygiene::Cleanly => 1,
            crate::personality::Hygiene::Slovenly => -1,
            crate::personality::Hygiene::Neutral => 0,
        },
    })
}

fn validate_social_pair(
    ctx: &ReducerContext,
    actor: &crate::character::Character,
    target: &crate::character::Character,
    is_self: bool,
) -> Result<(), String> {
    if !actor.alive || !target.alive {
        return Err("Both characters must be living".into());
    }
    if !is_self && (actor.party_id.is_none() || actor.party_id != target.party_id) {
        return Err("Social actions require the same party".into());
    }
    if !is_self
        && (actor.current_settlement_id != target.current_settlement_id
            || crate::investigation::character_case_site_id(ctx, actor.id)
                != crate::investigation::character_case_site_id(ctx, target.id))
    {
        return Err("Characters must be co-located".into());
    }
    Ok(())
}

#[reducer]
pub fn perform_social_action(
    ctx: &ReducerContext,
    actor_id: u64,
    target_id: u64,
    source_id: String,
    action_kind: String,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    perform_social_action_authoritative(ctx, actor_id, target_id, source_id, action_kind)
}

fn perform_social_action_authoritative(
    ctx: &ReducerContext,
    actor_id: u64,
    target_id: u64,
    source_id: String,
    action_kind: String,
) -> Result<(), String> {
    let action = parse_action(&action_kind)?;
    let is_self = actor_id == target_id;
    if is_self != (action == SocialActionKind::Reflect) {
        return Err("Reflect is self-only; other social actions require a companion".into());
    }
    let actor = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .ok_or("Actor not found")?;
    let target = ctx
        .db
        .character()
        .id()
        .find(target_id)
        .ok_or("Target not found")?;
    validate_social_pair(ctx, &actor, &target, is_self)?;
    let source = ctx
        .db
        .character_morale_source()
        .id()
        .find(&source_id)
        .ok_or("Morale source is stale")?;
    if source.character_id != target_id {
        return Err("Morale source does not belong to target".into());
    }
    if !social_source_eligible(&source.kind, source.magnitude) {
        return Err("Only current, negative, recognized morale sources can be addressed".into());
    }
    let topic = topic_for_source_kind(&source.kind).ok_or("Morale source is not actionable")?;
    if !action.available_for(topic) {
        return Err("That social approach does not fit this concern".into());
    }
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(target_id)
        .map_or(0, |v| v.minutes);
    let cooldown_id = canonical_cooldown_id(actor_id, target_id, topic, &action_kind);
    if ctx
        .db
        .social_action_cooldown()
        .id()
        .find(&cooldown_id)
        .is_some_and(|v| now < v.available_at_minute)
    {
        return Err("That approach needs time before it can be tried again".into());
    }

    let familiarity = canonical_pair(actor_id, target_id)
        .and_then(|(l, h)| ctx.db.character_familiarity().id().find(&pair_id(l, h)))
        .map_or(0.0, |v| v.shared_minutes as f32 / 60.0);
    let affinity = if is_self {
        0.0
    } else {
        current_affinity(ctx, target_id, actor_id)
    };
    let actor_shares_concern = shares_concern(ctx, actor_id, topic);
    let skill = social_action_skill(action, actor_shares_concern);
    let mut skill_check = crate::condition::mental_check(ctx, actor_id, skill)?;
    if !is_self {
        skill_check = adventuresim_world_schema::language_scaled_effect(
            skill_check,
            crate::character::shared_language_coefficient(ctx, actor_id, target_id),
        );
    }
    let target_deception = if is_self {
        0.0
    } else {
        crate::condition::mental_check(ctx, target_id, Skill::Deception)?
    };
    let roll = (ctx.random::<u64>() as f64 / u64::MAX as f64) as f32;
    let relevant_axis = axis_for_topic(topic);
    let truth = relevant_axis.and_then(|axis| personality_truth(ctx, target_id, axis));
    let relevant_belief = relevant_axis.and_then(|axis| {
        ctx.db
            .social_belief()
            .id()
            .find(&format!("{actor_id}:{target_id}:{}", axis.slug()))
            .map(|belief| (axis, belief.perceived_value))
    });
    let diagnosis_correct = diagnosis_for_axis(
        relevant_axis,
        truth,
        &relevant_belief.into_iter().collect::<Vec<_>>(),
    );
    let outcome = resolve_social_attempt(SocialAttempt {
        action,
        topic,
        skill_check,
        affinity,
        familiarity_hours: familiarity,
        diagnosis_correct,
        sensitivity: sensitivity(ctx, target_id, topic),
        roll,
    });
    let before = ctx
        .db
        .character_strategic_condition()
        .character_id()
        .find(target_id)
        .map_or(0.0, |v| v.morale);
    if !is_self {
        let event_source = format!("social:{actor_id}:{target_id}:{source_id}:{action_kind}:{now}");
        crate::condition::record_morale_event(
            ctx,
            target_id,
            "social_interaction",
            outcome.morale_delta,
            Some(event_source),
        )?;
        // This is the first non-event mutation. Every fallible automatic call
        // propagates the error above so SpacetimeDB rolls the transaction back.
        settle_shared_party_time(ctx, actor_id);
    }
    let after = ctx
        .db
        .character_strategic_condition()
        .character_id()
        .find(target_id)
        .map_or(before + outcome.morale_delta, |v| v.morale);
    let realized_gain = if is_self {
        0.0
    } else {
        (after - before).max(0.0)
    };
    let affinity_delta = if outcome.succeeded {
        affinity_gain(affinity, realized_gain)
    } else {
        outcome.affinity_delta.min(0.0)
    };
    if !is_self {
        put_affinity(ctx, target_id, actor_id, affinity + affinity_delta);
    }
    ctx.db.social_interaction().insert(SocialInteraction {
        id: 0,
        actor_id,
        target_id,
        source_id: source_id.clone(),
        topic: format!("{topic:?}").to_ascii_lowercase(),
        action_kind: action_kind.clone(),
        succeeded: if is_self { true } else { outcome.succeeded },
        morale_delta: if is_self { 0.0 } else { outcome.morale_delta },
        occurred_at_minute: now,
    });
    if outcome.succeeded {
        let id = social_address_id(actor_id, target_id, &source_id);
        let address = SocialAddress {
            id: id.clone(),
            actor_id,
            target_id,
            source_id: source_id.clone(),
            addressed_at_minute: now,
        };
        if ctx.db.social_address().id().find(&id).is_some() {
            ctx.db.social_address().id().update(address);
        } else {
            ctx.db.social_address().insert(address);
        }
    }
    let cooldown = SocialActionCooldown {
        id: cooldown_id.clone(),
        actor_id,
        target_id,
        topic: format!("{topic:?}").to_ascii_lowercase(),
        action_kind,
        available_at_minute: now.saturating_add(SOCIAL_COOLDOWN_MINUTES),
    };
    if ctx
        .db
        .social_action_cooldown()
        .id()
        .find(&cooldown_id)
        .is_some()
    {
        ctx.db.social_action_cooldown().id().update(cooldown);
    } else {
        ctx.db.social_action_cooldown().insert(cooldown);
    }
    if outcome.revealed_belief
        && let (Some(axis), Some(truth)) = (relevant_axis, truth)
    {
        let (value, confidence) = diagnosed_axis(truth, skill_check, target_deception, roll);
        let axis = axis.slug().to_owned();
        let id = format!("{actor_id}:{target_id}:{axis}");
        let row = SocialBelief {
            id: id.clone(),
            observer_id: actor_id,
            subject_id: target_id,
            axis,
            perceived_value: value,
            confidence,
            observed_at_minute: now,
        };
        if ctx.db.social_belief().id().find(&id).is_some() {
            ctx.db.social_belief().id().update(row);
        } else {
            ctx.db.social_belief().insert(row);
        }
    }
    Ok(())
}

/// Run bounded, opt-in social care after real discretionary downtime. Targets
/// and source rows use stable ID ordering. Each pair receives at most one
/// personality- and skill-selected attempt per interval, with the ordinary
/// action reducer enforcing every co-location, life, source, skill, and outcome
/// rule.
pub(crate) fn apply_automatic_social_chats(
    ctx: &ReducerContext,
    actor_id: u64,
    discretionary_minutes: u64,
) -> Result<(), String> {
    let preferences: Vec<_> = ctx
        .db
        .automatic_social_chat()
        .actor_id()
        .filter(actor_id)
        .collect();
    let candidates: Vec<_> = preferences
        .iter()
        .map(|preference| {
            let pair_available = ctx
                .db
                .character()
                .id()
                .find(actor_id)
                .zip(ctx.db.character().id().find(preference.target_id))
                .is_some_and(|(actor, target)| {
                    validate_social_pair(ctx, &actor, &target, false).is_ok()
                });
            let mut sources: Vec<_> = ctx
                .db
                .character_morale_source()
                .character_id()
                .filter(preference.target_id)
                .filter(|source| social_source_eligible(&source.kind, source.magnitude))
                .filter(|source| !source_addressed(ctx, actor_id, preference.target_id, &source.id))
                .collect();
            sources.sort_by(|left, right| left.id.cmp(&right.id));
            (
                preference.target_id,
                preference.enabled && pair_available,
                sources.first().map(|source| source.id.clone()),
            )
        })
        .collect();
    let targets = adventuresim_core::social::automatic_social_targets(
        discretionary_minutes,
        candidates
            .iter()
            .map(|(target_id, enabled, source)| (*target_id, *enabled, source.is_some())),
        MAX_AUTOMATIC_SOCIAL_ATTEMPTS_PER_DOWNTIME,
    );

    for target_id in targets {
        let source_id = candidates
            .iter()
            .find_map(|(candidate_id, _, source)| {
                (*candidate_id == target_id)
                    .then(|| source.clone())
                    .flatten()
            })
            .expect("automatic target planner only returns actionable candidates");
        let topic = ctx
            .db
            .character_morale_source()
            .id()
            .find(&source_id)
            .and_then(|source| topic_for_source_kind(&source.kind))
            .expect("automatic target planner only returns actionable sources");
        let Some(action) = automatic_social_action(ctx, actor_id, target_id, topic)? else {
            continue;
        };
        perform_social_action_authoritative(
            ctx,
            actor_id,
            target_id,
            source_id,
            action.reducer_value().into(),
        )?;
    }
    Ok(())
}

/// Keep compact address state aligned with the refreshable source projection.
pub(crate) fn prune_social_addresses(ctx: &ReducerContext, target_id: u64) {
    for row in ctx
        .db
        .social_address()
        .target_id()
        .filter(target_id)
        .filter(|row| {
            ctx.db
                .character_morale_source()
                .id()
                .find(&row.source_id)
                .is_none()
        })
        .collect::<Vec<_>>()
    {
        ctx.db.social_address().id().delete(&row.id);
    }
}

pub fn cleanup_character_social(ctx: &ReducerContext, character_id: u64) {
    for row in ctx
        .db
        .character_affinity()
        .iter()
        .filter(|r| r.subject_id == character_id || r.actor_id == character_id)
        .collect::<Vec<_>>()
    {
        ctx.db.character_affinity().id().delete(&row.id);
    }
    for row in ctx
        .db
        .character_familiarity()
        .iter()
        .filter(|r| r.low_id == character_id || r.high_id == character_id)
        .collect::<Vec<_>>()
    {
        ctx.db.character_familiarity().id().delete(&row.id);
    }
    for row in ctx
        .db
        .social_belief()
        .iter()
        .filter(|r| r.observer_id == character_id || r.subject_id == character_id)
        .collect::<Vec<_>>()
    {
        ctx.db.social_belief().id().delete(&row.id);
    }
    for row in ctx
        .db
        .social_interaction()
        .iter()
        .filter(|r| r.actor_id == character_id || r.target_id == character_id)
        .collect::<Vec<_>>()
    {
        ctx.db.social_interaction().id().delete(row.id);
    }
    for row in ctx
        .db
        .social_action_cooldown()
        .iter()
        .filter(|r| r.actor_id == character_id || r.target_id == character_id)
        .collect::<Vec<_>>()
    {
        ctx.db.social_action_cooldown().id().delete(&row.id);
    }
    for row in ctx
        .db
        .social_address()
        .iter()
        .filter(|r| r.actor_id == character_id || r.target_id == character_id)
        .collect::<Vec<_>>()
    {
        ctx.db.social_address().id().delete(&row.id);
    }
    for row in ctx
        .db
        .automatic_social_chat()
        .iter()
        .filter(|r| r.actor_id == character_id || r.target_id == character_id)
        .collect::<Vec<_>>()
    {
        ctx.db.automatic_social_chat().id().delete(&row.id);
    }
}

/// Deterministic relationship fixture, reachable only through the guarded
/// isolated-development bootstrap.
pub(crate) fn seed_social_demo(ctx: &ReducerContext) -> Result<(), String> {
    const VIEWER: u64 = 9_999_999_999_999_977;
    const TARGET: u64 = 9_999_999_999_999_976;
    if ctx.db.character().id().find(VIEWER).is_none() {
        crate::character::insert_new_character(ctx, "Social Demo".into(), VIEWER, false)?;
    }
    if ctx.db.character().id().find(TARGET).is_none() {
        crate::character::insert_new_npc_character(ctx, "Greta the Guard".into(), TARGET, false)?;
    }
    crate::strategic::attach_seeded_party_member(ctx, VIEWER, TARGET, "Guard")?;
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(TARGET)
        .map_or(0, |v| v.minutes);
    let mut personality = crate::personality::CharacterPersonality::neutral(TARGET);
    personality.drive = crate::personality::Drive::Ambitious;
    personality.self_regard = crate::personality::SelfRegard::Proud;
    ctx.db
        .character_personality()
        .character_id()
        .update(personality);
    for row in ctx
        .db
        .morale_event()
        .character_id()
        .filter(TARGET)
        .collect::<Vec<_>>()
    {
        if row
            .source_id
            .as_deref()
            .is_some_and(|id| id.starts_with("social-demo:"))
        {
            ctx.db.morale_event().id().delete(row.id);
        }
    }
    crate::condition::record_morale_event(
        ctx,
        TARGET,
        "defeat",
        -8.0,
        Some("social-demo:defeat".into()),
    )?;
    crate::condition::record_morale_event(
        ctx,
        TARGET,
        "injury",
        -3.0,
        Some("social-demo:injury".into()),
    )?;
    put_affinity(ctx, TARGET, VIEWER, 18.0);
    let (low_id, high_id) = canonical_pair(VIEWER, TARGET).expect("distinct demo ids");
    let familiarity = CharacterFamiliarity {
        id: pair_id(low_id, high_id),
        low_id,
        high_id,
        shared_minutes: 18 * 60,
        joint_minute_anchor: now,
    };
    if ctx
        .db
        .character_familiarity()
        .id()
        .find(&familiarity.id)
        .is_some()
    {
        ctx.db.character_familiarity().id().update(familiarity);
    } else {
        ctx.db.character_familiarity().insert(familiarity);
    }
    // Deliberately wrong but plausible: truth remains authoritative for outcomes.
    let belief = SocialBelief {
        id: format!("{VIEWER}:{TARGET}:drive"),
        observer_id: VIEWER,
        subject_id: TARGET,
        axis: "drive".into(),
        perceived_value: -1,
        confidence: 0.64,
        observed_at_minute: now,
    };
    if ctx.db.social_belief().id().find(&belief.id).is_some() {
        ctx.db.social_belief().id().update(belief);
    } else {
        ctx.db.social_belief().insert(belief);
    }
    Ok(())
}

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn automatic_personality_fit_changes_the_preferred_style() {
        let mut sanguine = crate::personality::CharacterPersonality::neutral(1);
        sanguine.outlook = crate::personality::Outlook::Sanguine;
        sanguine.sociability = crate::personality::Sociability::Gregarious;
        assert!(
            automatic_personality_fit(
                &sanguine,
                SocialActionKind::LightenMood,
                SocialTopic::Defeat,
            ) > automatic_personality_fit(&sanguine, SocialActionKind::Listen, SocialTopic::Defeat,)
        );

        let mut ambitious = crate::personality::CharacterPersonality::neutral(2);
        ambitious.drive = crate::personality::Drive::Ambitious;
        ambitious.nerve = crate::personality::Nerve::Brave;
        assert!(
            automatic_personality_fit(&ambitious, SocialActionKind::Rally, SocialTopic::Defeat,)
                > automatic_personality_fit(
                    &ambitious,
                    SocialActionKind::Commiserate,
                    SocialTopic::Defeat,
                )
        );
    }

    #[test]
    fn automatic_selection_uses_the_same_target_clock_as_execution() {
        let source = include_str!("social.rs");
        let automatic = source
            .split("fn automatic_social_action")
            .nth(1)
            .expect("automatic selector")
            .split("fn sensitivity")
            .next()
            .expect("selector boundary");
        assert!(automatic.contains(".find(target_id)"));
        assert!(!automatic.contains(".find(actor_id)"));
    }

    #[test]
    fn disabled_automatic_preferences_are_not_retained_in_the_projection() {
        let source = include_str!("social.rs");
        let setter = source
            .split("pub fn set_automatic_social_chat")
            .nth(1)
            .expect("automatic preference setter")
            .split("pub fn current_affinity")
            .next()
            .expect("setter boundary");
        assert!(setter.contains("if !enabled"));
        assert!(setter.contains(".delete(&id)"));

        let view = source
            .split("pub fn backend_automatic_social_chats")
            .nth(1)
            .expect("automatic preference view")
            .split("pub struct SocialActionCooldown")
            .next()
            .expect("view boundary");
        assert!(view.contains(".filter(|row| row.enabled)"));
        assert!(view.contains("actor.party_id == target.party_id"));
        assert!(source.contains("pub(crate) fn prune_invalid_automatic_social_chats"));
    }

    #[test]
    fn automatic_failures_propagate_and_no_fallible_work_follows_first_auxiliary_write() {
        let source = include_str!("social.rs");
        let automatic = source
            .split("pub(crate) fn apply_automatic_social_chats")
            .nth(1)
            .and_then(|tail| tail.split("pub(crate) fn prune_social_addresses").next())
            .expect("automatic social implementation");
        assert!(!automatic.contains("let _ = perform_social_action_authoritative"));
        assert!(automatic.contains("perform_social_action_authoritative("));
        assert!(automatic.contains(")?;"));
        assert!(!automatic.contains("\"listen\".into()"));

        let action = source
            .split("fn perform_social_action_authoritative")
            .nth(1)
            .and_then(|tail| {
                tail.split("pub(crate) fn apply_automatic_social_chats")
                    .next()
            })
            .expect("shared authoritative social action");
        let event = action
            .find("record_morale_event")
            .expect("fallible morale write");
        let familiarity = action
            .find("settle_shared_party_time")
            .expect("familiarity mutation");
        assert!(event < familiarity);
        assert!(action[event..].contains(")?;"));
    }

    #[test]
    fn routine_gateway_projection_is_compact_current_address_state() {
        let source = include_str!("social.rs");
        let view = source
            .split("pub fn backend_social_addresses")
            .nth(1)
            .and_then(|tail| tail.split("pub struct AutomaticSocialChat").next())
            .expect("compact social address view");
        assert!(view.contains("social_address()"));
        assert!(view.contains("character_morale_source()"));
        assert!(!view.contains("social_interaction()"));
        assert!(source.contains("pub(crate) fn prune_social_addresses"));
    }
}
