//! Durable strategic relationships and authoritative social actions.

use adventuresim_core::skill::Skill;
use adventuresim_core::social::{
    AFFINITY_MAX, AFFINITY_MIN, Courtship as CoreCourtship, Inclination as CoreInclination,
    Mirth as CoreMirth, PersonalityAxis, Presentation as CorePresentation, SOCIAL_COOLDOWN_MINUTES,
    SelfKnowledge as CoreSelfKnowledge, SocialActionKind, SocialAttempt, SocialTopic,
    Transparency as CoreTransparency, actor_allows_social_action, affinity_gain, axis_for_topic,
    canonical_cooldown_id, canonical_pair, choose_automatic_social_action,
    command_gravitas_modifier, diagnosed_axis, diagnosis_for_axis, discovery_training_split,
    flirt_charm_modifier, humor_charm_modifier, incompatible_flirt_outcome, resolve_social_attempt,
    self_knowledge_insight_modifier, settle_affinity, should_replace_belief,
    social_source_eligible, topic_for_source_kind,
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::character::{character, character__view};
use crate::condition::{character_morale_source__view, morale_event};
use crate::strategic::strategic_gateway_authority__view;
use crate::{
    character_attributes, character_capability, character_morale_source, character_personality,
    character_skills, character_strategic_condition, character_time,
};

pub const MAX_AUTOMATIC_SOCIAL_ATTEMPTS_PER_DOWNTIME: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum BeliefAxis {
    Nerve,
    Drive,
    Outlook,
    Sociability,
    Conscience,
    SelfRegard,
    Conviction,
    Hygiene,
    Temperance,
    Mirth,
    Courtship,
    Transparency,
    SelfKnowledge,
    Inclination,
    Presentation,
}

impl BeliefAxis {
    fn core(self) -> PersonalityAxis {
        match self {
            Self::Nerve => PersonalityAxis::Nerve,
            Self::Drive => PersonalityAxis::Drive,
            Self::Outlook => PersonalityAxis::Outlook,
            Self::Sociability => PersonalityAxis::Sociability,
            Self::Conscience => PersonalityAxis::Conscience,
            Self::SelfRegard => PersonalityAxis::SelfRegard,
            Self::Conviction => PersonalityAxis::Conviction,
            Self::Hygiene => PersonalityAxis::Hygiene,
            Self::Temperance => PersonalityAxis::Temperance,
            Self::Mirth => PersonalityAxis::Mirth,
            Self::Courtship => PersonalityAxis::Courtship,
            Self::Transparency => PersonalityAxis::Transparency,
            Self::SelfKnowledge => PersonalityAxis::SelfKnowledge,
            Self::Inclination => PersonalityAxis::Inclination,
            Self::Presentation => PersonalityAxis::Presentation,
        }
    }
}

impl From<PersonalityAxis> for BeliefAxis {
    fn from(value: PersonalityAxis) -> Self {
        match value {
            PersonalityAxis::Nerve => Self::Nerve,
            PersonalityAxis::Drive => Self::Drive,
            PersonalityAxis::Outlook => Self::Outlook,
            PersonalityAxis::Sociability => Self::Sociability,
            PersonalityAxis::Conscience => Self::Conscience,
            PersonalityAxis::SelfRegard => Self::SelfRegard,
            PersonalityAxis::Conviction => Self::Conviction,
            PersonalityAxis::Hygiene => Self::Hygiene,
            PersonalityAxis::Temperance => Self::Temperance,
            PersonalityAxis::Mirth => Self::Mirth,
            PersonalityAxis::Courtship => Self::Courtship,
            PersonalityAxis::Transparency => Self::Transparency,
            PersonalityAxis::SelfKnowledge => Self::SelfKnowledge,
            PersonalityAxis::Inclination => Self::Inclination,
            PersonalityAxis::Presentation => Self::Presentation,
        }
    }
}

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

/// Canonical pair-presence history. Unlike familiarity this retains every
/// join/rejoin span and the historical observation capability of both people.
#[derive(Clone, Debug)]
#[table(
    accessor = physiology_presence_span,
    index(accessor = presence_low_id, btree(columns = [low_id])),
    index(accessor = presence_high_id, btree(columns = [high_id]))
)]
pub struct PhysiologyPresenceSpan {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub low_id: u64,
    pub high_id: u64,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub low_observer_band: u8,
    pub high_observer_band: u8,
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
    pub axis: BeliefAxis,
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
        .filter(|belief| {
            belief
                .axis
                .core()
                .legal_values()
                .contains(&belief.perceived_value)
        })
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
        observe_presentation_on_contact(ctx, character_id, peer.id);
        observe_presentation_on_contact(ctx, peer.id, character_id);
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
        let already_open = ctx
            .db
            .physiology_presence_span()
            .presence_low_id()
            .filter(low_id)
            .any(|span| span.high_id == high_id && span.ended_at.is_none());
        if !already_open {
            let band = |id| {
                ctx.db
                    .character_capability()
                    .character_id()
                    .find(id)
                    .map_or(0, |capability| {
                        capability.physiology.round().clamp(0.0, 5.0) as u8
                    })
            };
            ctx.db
                .physiology_presence_span()
                .insert(PhysiologyPresenceSpan {
                    id: 0,
                    low_id,
                    high_id,
                    started_at: joint,
                    ended_at: None,
                    low_observer_band: band(low_id),
                    high_observer_band: band(high_id),
                });
        }
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

fn observe_presentation_on_contact(ctx: &ReducerContext, observer_id: u64, subject_id: u64) {
    let Some(personality) = ctx
        .db
        .character_personality()
        .character_id()
        .find(subject_id)
    else {
        return;
    };
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(observer_id)
        .map_or(0, |time| time.minutes);
    let truth = match personality.presentation {
        crate::personality::Presentation::Masculine => 0,
        crate::personality::Presentation::Ambiguous => 1,
        crate::personality::Presentation::Feminine => 2,
    };
    if personality.presentation != crate::personality::Presentation::Ambiguous {
        upsert_belief(
            ctx,
            observer_id,
            subject_id,
            PersonalityAxis::Presentation,
            truth,
            1.0,
            now,
        );
        return;
    }
    let (Ok(insight), Ok(base_deception)) = (
        crate::condition::mental_check(ctx, observer_id, Skill::Insight),
        crate::condition::mental_check(ctx, subject_id, Skill::Deception),
    ) else {
        return;
    };
    let deception = (base_deception
        + match personality.transparency {
            crate::personality::Transparency::Open => -1.0,
            crate::personality::Transparency::Neutral => 0.0,
            crate::personality::Transparency::Guarded => 1.0,
        })
    .clamp(0.0, 5.0);
    let roll = (ctx.random::<u64>() as f64 / u64::MAX as f64) as f32;
    let (value, confidence) = diagnosed_axis(
        PersonalityAxis::Presentation,
        truth,
        insight,
        deception,
        roll,
    );
    upsert_belief(
        ctx,
        observer_id,
        subject_id,
        PersonalityAxis::Presentation,
        value,
        confidence,
        now,
    );
    award_discovery_training(ctx, observer_id, subject_id, personality.transparency);
}

/// Close every open span before leave, party change, death or disband. Taking
/// the minimum personal clock preserves asymmetric-clock and chunk invariance.
pub fn close_physiology_presence(ctx: &ReducerContext, character_id: u64) {
    let clock = |id| {
        ctx.db
            .character_time()
            .character_id()
            .find(id)
            .map_or(0, |time| time.minutes)
    };
    let spans = ctx
        .db
        .physiology_presence_span()
        .iter()
        .filter(|span| {
            span.ended_at.is_none() && (span.low_id == character_id || span.high_id == character_id)
        })
        .collect::<Vec<_>>();
    for mut span in spans {
        span.ended_at = Some(
            clock(span.low_id)
                .min(clock(span.high_id))
                .max(span.started_at),
        );
        ctx.db.physiology_presence_span().id().update(span);
    }
}

fn parse_action(value: &str) -> Result<SocialActionKind, String> {
    match value {
        "reflect" => Ok(SocialActionKind::Reflect),
        "listen" => Ok(SocialActionKind::Listen),
        "commiserate" => Ok(SocialActionKind::Commiserate),
        "lighten_mood" => Ok(SocialActionKind::LightenMood),
        "command" => Ok(SocialActionKind::Rally),
        "deception" => Ok(SocialActionKind::Reframe),
        "flirt" => Ok(SocialActionKind::Flirt),
        _ => Err("Unknown social action".into()),
    }
}

fn social_action_skill(action: SocialActionKind, shares_concern: bool) -> Skill {
    match action {
        SocialActionKind::Reflect => Skill::Insight,
        SocialActionKind::Listen => Skill::Insight,
        SocialActionKind::Commiserate if shares_concern => Skill::Insight,
        SocialActionKind::Commiserate => Skill::Deception,
        SocialActionKind::LightenMood => Skill::Charm,
        SocialActionKind::Rally => Skill::Command,
        SocialActionKind::Reframe => Skill::Deception,
        SocialActionKind::Flirt => Skill::Charm,
    }
}

fn automatic_personality_fit(
    personality: &crate::personality::CharacterPersonality,
    action: SocialActionKind,
    topic: SocialTopic,
) -> f32 {
    use crate::personality::{
        Conscience, Conviction, Courtship, Drive, Mirth, Nerve, Outlook, SelfRegard, Sociability,
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
    match personality.mirth {
        Mirth::Merry if action == SocialActionKind::LightenMood => fit += 1.0,
        Mirth::Grave if action == SocialActionKind::LightenMood => fit -= 1.0,
        _ => {}
    }
    match personality.courtship {
        Courtship::Amorous if action == SocialActionKind::Flirt => fit += 1.25,
        Courtship::Proper if action == SocialActionKind::Flirt => fit -= 1.25,
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
    for action in ACTIONS.into_iter().filter(|action| {
        action.available_for(topic)
            && actor_allows_social_action(
                *action,
                core_mirth(personality.mirth),
                core_courtship(personality.courtship),
            )
    }) {
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
        let mut unscaled_skill_check = crate::condition::mental_check(
            ctx,
            actor_id,
            social_action_skill(action, shares_concern),
        )?;
        if action == SocialActionKind::Rally {
            unscaled_skill_check += command_gravitas_modifier(
                core_mirth(personality.mirth),
                core_courtship(personality.courtship),
            );
        }
        let skill_check =
            adventuresim_world_schema::language_scaled_effect(unscaled_skill_check, language);
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
        PersonalityAxis::Nerve => match p.nerve {
            crate::personality::Nerve::Neutral => 0,
            crate::personality::Nerve::Brave => 1,
            crate::personality::Nerve::Fearful => 2,
        },
        PersonalityAxis::Drive => match p.drive {
            crate::personality::Drive::Neutral => 0,
            crate::personality::Drive::Ambitious => 1,
            crate::personality::Drive::Content => 2,
        },
        PersonalityAxis::Outlook => match p.outlook {
            crate::personality::Outlook::Neutral => 0,
            crate::personality::Outlook::Sanguine => 1,
            crate::personality::Outlook::Brooding => 2,
        },
        PersonalityAxis::Sociability => match p.sociability {
            crate::personality::Sociability::Neutral => 0,
            crate::personality::Sociability::Gregarious => 1,
            crate::personality::Sociability::Solitary => 2,
        },
        PersonalityAxis::Conscience => match p.conscience {
            crate::personality::Conscience::Neutral => 0,
            crate::personality::Conscience::Compassionate => 1,
            crate::personality::Conscience::Callous => 2,
            crate::personality::Conscience::Cruel => 3,
        },
        PersonalityAxis::SelfRegard => match p.self_regard {
            crate::personality::SelfRegard::Neutral => 0,
            crate::personality::SelfRegard::Proud => 1,
            crate::personality::SelfRegard::Humble => 2,
        },
        PersonalityAxis::Conviction => match p.conviction {
            crate::personality::Conviction::Neutral => 0,
            crate::personality::Conviction::Zealous => 1,
            crate::personality::Conviction::Irreverent => 2,
        },
        PersonalityAxis::Hygiene => match p.hygiene {
            crate::personality::Hygiene::Neutral => 0,
            crate::personality::Hygiene::Slovenly => 1,
            crate::personality::Hygiene::Cleanly => 2,
        },
        PersonalityAxis::Temperance => match p.temperance {
            crate::personality::Temperance::Neutral => 0,
            crate::personality::Temperance::Temperate => 1,
            crate::personality::Temperance::Drunkard => 2,
        },
        PersonalityAxis::Mirth => match p.mirth {
            crate::personality::Mirth::Neutral => 0,
            crate::personality::Mirth::Merry => 1,
            crate::personality::Mirth::Grave => 2,
        },
        PersonalityAxis::Courtship => match p.courtship {
            crate::personality::Courtship::Neutral => 0,
            crate::personality::Courtship::Amorous => 1,
            crate::personality::Courtship::Proper => 2,
        },
        PersonalityAxis::Transparency => match p.transparency {
            crate::personality::Transparency::Neutral => 0,
            crate::personality::Transparency::Open => 1,
            crate::personality::Transparency::Guarded => 2,
        },
        PersonalityAxis::SelfKnowledge => match p.self_knowledge {
            crate::personality::SelfKnowledge::Neutral => 0,
            crate::personality::SelfKnowledge::Introspective => 1,
            crate::personality::SelfKnowledge::SelfDeceiving => 2,
        },
        PersonalityAxis::Inclination => match p.inclination {
            crate::personality::Inclination::Men => 0,
            crate::personality::Inclination::Either => 1,
            crate::personality::Inclination::Women => 2,
            crate::personality::Inclination::Neither => 3,
        },
        PersonalityAxis::Presentation => match p.presentation {
            crate::personality::Presentation::Masculine => 0,
            crate::personality::Presentation::Ambiguous => 1,
            crate::personality::Presentation::Feminine => 2,
        },
    })
}

fn core_mirth(value: crate::personality::Mirth) -> CoreMirth {
    match value {
        crate::personality::Mirth::Neutral => CoreMirth::Neutral,
        crate::personality::Mirth::Merry => CoreMirth::Merry,
        crate::personality::Mirth::Grave => CoreMirth::Grave,
    }
}

fn core_courtship(value: crate::personality::Courtship) -> CoreCourtship {
    match value {
        crate::personality::Courtship::Neutral => CoreCourtship::Neutral,
        crate::personality::Courtship::Amorous => CoreCourtship::Amorous,
        crate::personality::Courtship::Proper => CoreCourtship::Proper,
    }
}

fn core_inclination(value: crate::personality::Inclination) -> CoreInclination {
    match value {
        crate::personality::Inclination::Men => CoreInclination::Men,
        crate::personality::Inclination::Either => CoreInclination::Either,
        crate::personality::Inclination::Women => CoreInclination::Women,
        crate::personality::Inclination::Neither => CoreInclination::Neither,
    }
}

fn core_presentation(value: crate::personality::Presentation) -> CorePresentation {
    match value {
        crate::personality::Presentation::Masculine => CorePresentation::Masculine,
        crate::personality::Presentation::Ambiguous => CorePresentation::Ambiguous,
        crate::personality::Presentation::Feminine => CorePresentation::Feminine,
    }
}

fn discovery_axes(
    action: SocialActionKind,
    topic: SocialTopic,
    is_self: bool,
) -> Vec<PersonalityAxis> {
    if is_self {
        return vec![
            PersonalityAxis::SelfKnowledge,
            axis_for_topic(topic).unwrap_or(PersonalityAxis::Outlook),
        ];
    }
    match action {
        SocialActionKind::Listen => vec![
            axis_for_topic(topic).unwrap_or(match topic {
                SocialTopic::Fatigue => PersonalityAxis::Outlook,
                SocialTopic::Hunger => PersonalityAxis::Temperance,
                _ => PersonalityAxis::Transparency,
            }),
            PersonalityAxis::Transparency,
        ],
        SocialActionKind::Commiserate => {
            vec![PersonalityAxis::Conscience, PersonalityAxis::Sociability]
        }
        SocialActionKind::LightenMood => vec![PersonalityAxis::Mirth],
        SocialActionKind::Rally => vec![
            PersonalityAxis::Nerve,
            if topic == SocialTopic::Faith {
                PersonalityAxis::Conviction
            } else {
                PersonalityAxis::Drive
            },
        ],
        SocialActionKind::Reframe => {
            vec![PersonalityAxis::SelfRegard, PersonalityAxis::Outlook]
        }
        SocialActionKind::Flirt => {
            vec![PersonalityAxis::Courtship, PersonalityAxis::Inclination]
        }
        SocialActionKind::Reflect => unreachable!("self-only handled above"),
    }
}

fn award_discovery_training(
    ctx: &ReducerContext,
    observer_id: u64,
    subject_id: u64,
    transparency: crate::personality::Transparency,
) {
    let (observer_insight, subject_deception) = discovery_training_split(match transparency {
        crate::personality::Transparency::Open => CoreTransparency::Open,
        crate::personality::Transparency::Neutral => CoreTransparency::Neutral,
        crate::personality::Transparency::Guarded => CoreTransparency::Guarded,
    });
    if observer_id == subject_id {
        if let (Some(mut skills), Some(attributes)) = (
            ctx.db.character_skills().character_id().find(observer_id),
            ctx.db
                .character_attributes()
                .character_id()
                .find(observer_id),
        ) {
            adventuresim_core::skill::apply_direct_training(
                Skill::Insight,
                &mut skills.insight_hours,
                observer_insight,
                &attributes,
            );
            adventuresim_core::skill::apply_direct_training(
                Skill::Deception,
                &mut skills.deception_hours,
                subject_deception,
                &attributes,
            );
            ctx.db.character_skills().character_id().update(skills);
        }
        return;
    }
    if observer_insight > 0.0
        && let (Some(mut skills), Some(attributes)) = (
            ctx.db.character_skills().character_id().find(observer_id),
            ctx.db
                .character_attributes()
                .character_id()
                .find(observer_id),
        )
    {
        adventuresim_core::skill::apply_direct_training(
            Skill::Insight,
            &mut skills.insight_hours,
            observer_insight,
            &attributes,
        );
        ctx.db.character_skills().character_id().update(skills);
    }
    if subject_deception > 0.0
        && let (Some(mut skills), Some(attributes)) = (
            ctx.db.character_skills().character_id().find(subject_id),
            ctx.db
                .character_attributes()
                .character_id()
                .find(subject_id),
        )
    {
        adventuresim_core::skill::apply_direct_training(
            Skill::Deception,
            &mut skills.deception_hours,
            subject_deception,
            &attributes,
        );
        ctx.db.character_skills().character_id().update(skills);
    }
}

fn upsert_belief(
    ctx: &ReducerContext,
    observer_id: u64,
    subject_id: u64,
    axis: PersonalityAxis,
    perceived_value: i8,
    confidence: f32,
    now: u64,
) {
    if !axis.legal_values().contains(&perceived_value) {
        return;
    }
    let axis_slug = axis.slug().to_owned();
    let id = format!("{observer_id}:{subject_id}:{axis_slug}");
    if let Some(existing) = ctx.db.social_belief().id().find(&id)
        && !should_replace_belief(existing.confidence, confidence)
    {
        return;
    }
    let row = SocialBelief {
        id: id.clone(),
        observer_id,
        subject_id,
        axis: axis.into(),
        perceived_value,
        confidence,
        observed_at_minute: now,
    };
    if ctx.db.social_belief().id().find(&id).is_some() {
        ctx.db.social_belief().id().update(row);
    } else {
        ctx.db.social_belief().insert(row);
    }
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
    let actor_personality = crate::personality::personality_or_neutral(ctx, actor_id);
    if !actor_allows_social_action(
        action,
        core_mirth(actor_personality.mirth),
        core_courtship(actor_personality.courtship),
    ) {
        return Err("Your disposition does not permit that social approach".into());
    }
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
    if action == SocialActionKind::Rally {
        skill_check += command_gravitas_modifier(
            core_mirth(actor_personality.mirth),
            core_courtship(actor_personality.courtship),
        );
    }
    if !is_self {
        skill_check = adventuresim_world_schema::language_scaled_effect(
            skill_check,
            crate::character::shared_language_coefficient(ctx, actor_id, target_id),
        );
    }
    let target_personality = crate::personality::personality_or_neutral(ctx, target_id);
    let base_target_deception = crate::condition::mental_check(ctx, target_id, Skill::Deception)?;
    let obscuring_deception = (base_target_deception
        + match target_personality.transparency {
            crate::personality::Transparency::Open => -1.0,
            crate::personality::Transparency::Neutral => 0.0,
            crate::personality::Transparency::Guarded => 1.0,
        })
    .clamp(0.0, 5.0);
    let insight_check = crate::condition::mental_check(ctx, actor_id, Skill::Insight)?;
    let self_insight_modifier = if is_self {
        self_knowledge_insight_modifier(match target_personality.self_knowledge {
            crate::personality::SelfKnowledge::Neutral => CoreSelfKnowledge::Neutral,
            crate::personality::SelfKnowledge::Introspective => CoreSelfKnowledge::Introspective,
            crate::personality::SelfKnowledge::SelfDeceiving => CoreSelfKnowledge::SelfDeceiving,
        })
    } else {
        0.0
    };
    let social_roll = (ctx.random::<u64>() as f64 / u64::MAX as f64) as f32;
    let relevant_axis = axis_for_topic(topic);
    let truth = relevant_axis.and_then(|axis| personality_truth(ctx, target_id, axis));
    let relevant_belief = relevant_axis.and_then(|axis| {
        ctx.db
            .social_belief()
            .id()
            .find(&format!("{actor_id}:{target_id}:{}", axis.slug()))
            .and_then(|belief| {
                (belief.axis.core() == axis
                    && axis.legal_values().contains(&belief.perceived_value))
                .then_some((axis, belief.perceived_value))
            })
    });
    let diagnosis_correct = diagnosis_for_axis(
        relevant_axis,
        truth,
        &relevant_belief.into_iter().collect::<Vec<_>>(),
    );
    let flirt_modifier = if action == SocialActionKind::Flirt {
        flirt_charm_modifier(
            core_inclination(actor_personality.inclination),
            core_presentation(actor_personality.presentation),
            core_inclination(target_personality.inclination),
            core_presentation(target_personality.presentation),
            core_courtship(target_personality.courtship),
        )
    } else {
        Some(0.0)
    };
    if action == SocialActionKind::LightenMood {
        skill_check += humor_charm_modifier(
            core_mirth(actor_personality.mirth),
            core_mirth(target_personality.mirth),
        );
    }
    if let Some(modifier) = flirt_modifier {
        skill_check += modifier;
    }
    let outcome = if flirt_modifier.is_none() {
        // Incompatibility is a hard gate: it cannot leak the resolver's
        // minimum success chance or any positive outcome.
        incompatible_flirt_outcome()
    } else {
        resolve_social_attempt(SocialAttempt {
            action,
            topic,
            skill_check,
            affinity,
            familiarity_hours: familiarity,
            diagnosis_correct,
            sensitivity: sensitivity(ctx, target_id, topic),
            roll: social_roll,
        })
    };
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
    // Presentation is normally obvious on contact. Its explicit axis leaves
    // room for a future disguise override without exposing demographic sex.
    if !is_self
        && let Some(value) = personality_truth(ctx, target_id, PersonalityAxis::Presentation)
        && value != 1
    {
        upsert_belief(
            ctx,
            actor_id,
            target_id,
            PersonalityAxis::Presentation,
            value,
            1.0,
            now,
        );
    }
    for axis in discovery_axes(action, topic, is_self) {
        let Some(truth) = personality_truth(ctx, target_id, axis) else {
            continue;
        };
        let discovery_roll = (ctx.random::<u64>() as f64 / u64::MAX as f64) as f32;
        let deception = if axis == PersonalityAxis::Transparency {
            base_target_deception
        } else {
            obscuring_deception
        };
        let (value, confidence) = diagnosed_axis(
            axis,
            truth,
            (insight_check + self_insight_modifier).clamp(0.0, 5.0),
            deception,
            discovery_roll,
        );
        upsert_belief(ctx, actor_id, target_id, axis, value, confidence, now);
        award_discovery_training(ctx, actor_id, target_id, target_personality.transparency);
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
    let mut viewer_personality = crate::personality::personality_or_neutral(ctx, VIEWER);
    viewer_personality.sex = crate::personality::Sex::Male;
    viewer_personality.presentation = crate::personality::Presentation::Masculine;
    viewer_personality.inclination = crate::personality::Inclination::Women;
    ctx.db
        .character_personality()
        .character_id()
        .update(viewer_personality);
    let mut personality = crate::personality::CharacterPersonality::neutral(TARGET);
    personality.drive = crate::personality::Drive::Ambitious;
    personality.self_regard = crate::personality::SelfRegard::Proud;
    personality.conscience = crate::personality::Conscience::Cruel;
    personality.mirth = crate::personality::Mirth::Merry;
    personality.courtship = crate::personality::Courtship::Amorous;
    personality.sex = crate::personality::Sex::Female;
    personality.presentation = crate::personality::Presentation::Feminine;
    personality.inclination = crate::personality::Inclination::Men;
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
        axis: BeliefAxis::Drive,
        perceived_value: 2,
        confidence: 0.64,
        observed_at_minute: now,
    };
    if ctx.db.social_belief().id().find(&belief.id).is_some() {
        ctx.db.social_belief().id().update(belief);
    } else {
        ctx.db.social_belief().insert(belief);
    }
    upsert_belief(
        ctx,
        VIEWER,
        TARGET,
        PersonalityAxis::Conscience,
        3,
        0.82,
        now,
    );
    upsert_belief(
        ctx,
        VIEWER,
        TARGET,
        PersonalityAxis::Presentation,
        2,
        1.0,
        now,
    );
    Ok(())
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_personality_axis_has_a_reachable_observation_context() {
        let mut axes = HashSet::from([PersonalityAxis::Presentation]);
        for (action, topic, is_self) in [
            (SocialActionKind::Reflect, SocialTopic::Defeat, true),
            (SocialActionKind::Listen, SocialTopic::Defeat, false),
            (SocialActionKind::Listen, SocialTopic::Injury, false),
            (SocialActionKind::Listen, SocialTopic::Fatigue, false),
            (SocialActionKind::Listen, SocialTopic::Hunger, false),
            (SocialActionKind::Listen, SocialTopic::Faith, false),
            (SocialActionKind::Listen, SocialTopic::Filth, false),
            (SocialActionKind::Commiserate, SocialTopic::Defeat, false),
            (SocialActionKind::LightenMood, SocialTopic::Defeat, false),
            (SocialActionKind::Rally, SocialTopic::Defeat, false),
            (SocialActionKind::Rally, SocialTopic::Faith, false),
            (SocialActionKind::Reframe, SocialTopic::Injury, false),
            (SocialActionKind::Flirt, SocialTopic::Injury, false),
        ] {
            axes.extend(discovery_axes(action, topic, is_self));
        }
        for axis in [
            PersonalityAxis::Nerve,
            PersonalityAxis::Drive,
            PersonalityAxis::Outlook,
            PersonalityAxis::Sociability,
            PersonalityAxis::Conscience,
            PersonalityAxis::SelfRegard,
            PersonalityAxis::Conviction,
            PersonalityAxis::Hygiene,
            PersonalityAxis::Temperance,
            PersonalityAxis::Mirth,
            PersonalityAxis::Courtship,
            PersonalityAxis::Transparency,
            PersonalityAxis::SelfKnowledge,
            PersonalityAxis::Inclination,
            PersonalityAxis::Presentation,
        ] {
            assert!(axes.contains(&axis), "{axis:?} is unreachable");
        }
    }

    #[test]
    fn self_discovery_updates_one_skills_row_and_unsupported_contexts_do_not_check() {
        let source = include_str!("social.rs");
        let training = source
            .split("fn award_discovery_training")
            .nth(1)
            .and_then(|tail| tail.split("fn upsert_belief").next())
            .expect("training helper");
        let self_branch = training
            .split("if observer_id == subject_id")
            .nth(1)
            .and_then(|tail| tail.split("return;").next())
            .expect("self training branch");
        assert_eq!(
            self_branch
                .matches("character_skills().character_id().update(skills)")
                .count(),
            1
        );
        assert!(!adventuresim_core::social::discovery_supported(
            PersonalityAxis::Nerve,
            adventuresim_core::social::DiscoveryContext::Ordinary,
        ));
        assert!(!adventuresim_core::social::discovery_supported(
            PersonalityAxis::Courtship,
            adventuresim_core::social::DiscoveryContext::Stress,
        ));
    }

    #[test]
    fn contact_observes_obvious_presentation_but_checks_ambiguous_presentation() {
        let source = include_str!("social.rs");
        let contact = source
            .split("fn observe_presentation_on_contact")
            .nth(1)
            .and_then(|tail| tail.split("fn close_physiology_presence").next())
            .expect("contact presentation helper");
        assert!(contact.contains("presentation != crate::personality::Presentation::Ambiguous"));
        assert!(contact.contains("confidence"));
        assert!(contact.contains("diagnosed_axis("));
        assert!(contact.contains("award_discovery_training("));
        let join = source
            .split("pub fn reset_familiarity_after_join")
            .nth(1)
            .and_then(|tail| tail.split("fn observe_presentation_on_contact").next())
            .expect("join boundary");
        assert!(join.contains("observe_presentation_on_contact(ctx, character_id, peer.id)"));
        assert!(join.contains("observe_presentation_on_contact(ctx, peer.id, character_id)"));
    }

    #[test]
    fn persisted_beliefs_are_typed_and_invalid_values_fail_closed() {
        let source = include_str!("social.rs");
        assert!(source.contains("pub axis: BeliefAxis"));
        assert!(source.contains("if !axis.legal_values().contains(&perceived_value)"));
        assert_eq!(
            PersonalityAxis::Inclination.value_label(-1),
            None,
            "invalid typed value must not decode as a normal belief"
        );
    }

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
    fn manual_and_automatic_actions_share_actor_trait_gates_and_rally_bonus() {
        let source = include_str!("social.rs");
        let automatic = source
            .split("fn automatic_social_action")
            .nth(1)
            .and_then(|tail| tail.split("fn sensitivity").next())
            .expect("automatic selector");
        assert!(automatic.contains("actor_allows_social_action("));
        assert!(automatic.contains("command_gravitas_modifier("));
        assert!(
            automatic.find("command_gravitas_modifier(")
                < automatic.find("language_scaled_effect(")
        );

        let authoritative = source
            .split("fn perform_social_action_authoritative")
            .nth(1)
            .and_then(|tail| {
                tail.split("pub(crate) fn apply_automatic_social_chats")
                    .next()
            })
            .expect("authoritative action");
        assert!(authoritative.contains("actor_allows_social_action("));
        assert!(authoritative.contains("command_gravitas_modifier("));
        assert!(
            authoritative.find("command_gravitas_modifier(")
                < authoritative.find("language_scaled_effect(")
        );
        assert!(authoritative.contains("Your disposition does not permit"));
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
