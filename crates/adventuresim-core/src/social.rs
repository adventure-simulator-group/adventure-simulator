//! Deterministic strategic relationship and social-action rules.
//!
//! Persistence stores directional affinity separately from canonical, symmetric
//! familiarity. Presentation code must use the closed topic/action catalogue;
//! free-form morale labels are never parsed into actions.

use std::collections::HashSet;

use adventuresim_world_schema::OfficialReligion;

pub const AFFINITY_MIN: f32 = -100.0;
pub const AFFINITY_MAX: f32 = 100.0;
pub const AFFINITY_HALF_LIFE_MINUTES: u64 = 30 * 24 * 60;
pub const SOCIAL_COOLDOWN_MINUTES: u64 = 24 * 60;
pub const SOCIAL_RESPONSE_MINUTES: u64 = 5;
pub const DISCOVERY_TRAINING_HOURS: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionPetitionApproach {
    PersonalAppeal,
    Command,
    ProfessionalOpinion,
    ReligiousPetition,
    GuildPetition,
}

#[derive(Debug, Clone, Copy)]
pub struct PermissionPetitionInput {
    pub approach: PermissionPetitionApproach,
    pub skill_check: f32,
    pub language_coefficient: f32,
    pub affinity: f32,
    pub familiarity_hours: f32,
    pub reputation_modifier: i16,
    pub professional_fit: bool,
    pub authority_fit: bool,
    pub difficulty: f32,
    pub roll: f32,
}

/// Generic permission petition used by legal, medical, religious, and guild
/// dialogue. Domain authority remains the caller's responsibility.
pub fn resolve_permission_petition(input: PermissionPetitionInput) -> bool {
    if !input.skill_check.is_finite()
        || !input.language_coefficient.is_finite()
        || !input.affinity.is_finite()
        || !input.familiarity_hours.is_finite()
        || !input.difficulty.is_finite()
        || !input.roll.is_finite()
    {
        return false;
    }
    let fit = match input.approach {
        PermissionPetitionApproach::PersonalAppeal => 0.0,
        PermissionPetitionApproach::Command => {
            if input.authority_fit {
                0.65
            } else {
                -0.8
            }
        }
        PermissionPetitionApproach::ProfessionalOpinion => {
            if input.professional_fit {
                0.75
            } else {
                -0.9
            }
        }
        PermissionPetitionApproach::ReligiousPetition => {
            if input.authority_fit {
                0.8
            } else {
                -0.7
            }
        }
        PermissionPetitionApproach::GuildPetition => {
            if input.authority_fit {
                0.55
            } else {
                -0.8
            }
        }
    };
    let score = input.skill_check.clamp(0.0, 5.0) * input.language_coefficient.clamp(0.0, 1.0)
        + input.affinity.clamp(-25.0, 25.0) / 25.0
        + (input.familiarity_hours / 100.0).clamp(0.0, 1.0)
        + f32::from(input.reputation_modifier) / 25.0
        + fit
        + (0.5 - input.roll.clamp(0.0, 1.0));
    score >= input.difficulty
}

#[cfg(test)]
mod permission_petition_tests {
    use super::*;

    #[test]
    fn language_and_typed_fit_materially_govern_permission() {
        let input = PermissionPetitionInput {
            approach: PermissionPetitionApproach::ReligiousPetition,
            skill_check: 4.0,
            language_coefficient: 1.0,
            affinity: 10.0,
            familiarity_hours: 10.0,
            reputation_modifier: 5,
            professional_fit: false,
            authority_fit: true,
            difficulty: 3.0,
            roll: 0.5,
        };
        assert!(resolve_permission_petition(input));
        assert!(!resolve_permission_petition(PermissionPetitionInput {
            language_coefficient: 0.0,
            authority_fit: false,
            ..input
        }));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SocialTopic {
    Defeat,
    Injury,
    Fatigue,
    Hunger,
    Faith,
    Filth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PersonalityAxis {
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

impl PersonalityAxis {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Nerve => "Nerve",
            Self::Drive => "Drive",
            Self::Outlook => "Outlook",
            Self::Sociability => "Sociability",
            Self::Conscience => "Conscience",
            Self::SelfRegard => "Self-regard",
            Self::Conviction => "Conviction",
            Self::Hygiene => "Hygiene",
            Self::Temperance => "Temperance",
            Self::Mirth => "Mirth",
            Self::Courtship => "Courtship",
            Self::Transparency => "Transparency",
            Self::SelfKnowledge => "Self-knowledge",
            Self::Inclination => "Inclination",
            Self::Presentation => "Presentation",
        }
    }

    pub const fn value_label(self, value: i8) -> Option<&'static str> {
        match (self, value) {
            (Self::Nerve, 0)
            | (Self::Drive, 0)
            | (Self::Outlook, 0)
            | (Self::Sociability, 0)
            | (Self::Conscience, 0)
            | (Self::SelfRegard, 0)
            | (Self::Conviction, 0)
            | (Self::Hygiene, 0)
            | (Self::Temperance, 0)
            | (Self::Mirth, 0)
            | (Self::Courtship, 0)
            | (Self::Transparency, 0)
            | (Self::SelfKnowledge, 0) => Some("Neutral"),
            (Self::Nerve, 1) => Some("Brave"),
            (Self::Nerve, 2) => Some("Fearful"),
            (Self::Drive, 1) => Some("Ambitious"),
            (Self::Drive, 2) => Some("Content"),
            (Self::Outlook, 1) => Some("Sanguine"),
            (Self::Outlook, 2) => Some("Brooding"),
            (Self::Sociability, 1) => Some("Gregarious"),
            (Self::Sociability, 2) => Some("Solitary"),
            (Self::Conscience, 1) => Some("Compassionate"),
            (Self::Conscience, 2) => Some("Callous"),
            (Self::Conscience, 3) => Some("Cruel"),
            (Self::SelfRegard, 1) => Some("Proud"),
            (Self::SelfRegard, 2) => Some("Humble"),
            (Self::Conviction, 1) => Some("Zealous"),
            (Self::Conviction, 2) => Some("Irreverent"),
            (Self::Hygiene, 1) => Some("Slovenly"),
            (Self::Hygiene, 2) => Some("Cleanly"),
            (Self::Temperance, 1) => Some("Temperate"),
            (Self::Temperance, 2) => Some("Drunkard"),
            (Self::Mirth, 1) => Some("Merry"),
            (Self::Mirth, 2) => Some("Grave"),
            (Self::Courtship, 1) => Some("Amorous"),
            (Self::Courtship, 2) => Some("Proper"),
            (Self::Transparency, 1) => Some("Open"),
            (Self::Transparency, 2) => Some("Guarded"),
            (Self::SelfKnowledge, 1) => Some("Introspective"),
            (Self::SelfKnowledge, 2) => Some("Self-deceiving"),
            (Self::Inclination, 0) => Some("Attracted to men"),
            (Self::Inclination, 1) => Some("Attracted to men and women"),
            (Self::Inclination, 2) => Some("Attracted to women"),
            (Self::Inclination, 3) => Some("Attracted to neither"),
            (Self::Presentation, 0) => Some("Man"),
            (Self::Presentation, 1) => Some("Ambiguous"),
            (Self::Presentation, 2) => Some("Woman"),
            _ => None,
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Nerve => "nerve",
            Self::Drive => "drive",
            Self::Outlook => "outlook",
            Self::Sociability => "sociability",
            Self::Conscience => "conscience",
            Self::SelfRegard => "self_regard",
            Self::Conviction => "conviction",
            Self::Hygiene => "hygiene",
            Self::Temperance => "temperance",
            Self::Mirth => "mirth",
            Self::Courtship => "courtship",
            Self::Transparency => "transparency",
            Self::SelfKnowledge => "self_knowledge",
            Self::Inclination => "inclination",
            Self::Presentation => "presentation",
        }
    }

    /// Stable legal value codes. Codes are axis-local and must never be
    /// interpreted by sign.
    pub const fn legal_values(self) -> &'static [i8] {
        match self {
            Self::Conscience | Self::Inclination => &[0, 1, 2, 3],
            _ => &[0, 1, 2],
        }
    }

    pub const fn base_obscurity(self) -> u8 {
        match self {
            Self::Presentation => 0,
            Self::Hygiene
            | Self::Mirth
            | Self::Outlook
            | Self::Sociability
            | Self::Transparency => 1,
            Self::Temperance | Self::Drive | Self::SelfRegard | Self::Conviction | Self::Nerve => 2,
            Self::Conscience | Self::SelfKnowledge | Self::Courtship => 3,
            Self::Inclination => 4,
        }
    }

    pub const fn is_neutral_code(self, value: i8) -> bool {
        match self {
            Self::Inclination => false,
            Self::Presentation => value == 1,
            _ => value == 0,
        }
    }

    pub fn parse(slug: &str) -> Option<Self> {
        [
            Self::Nerve,
            Self::Drive,
            Self::Outlook,
            Self::Sociability,
            Self::Conscience,
            Self::SelfRegard,
            Self::Conviction,
            Self::Hygiene,
            Self::Temperance,
            Self::Mirth,
            Self::Courtship,
            Self::Transparency,
            Self::SelfKnowledge,
            Self::Inclination,
            Self::Presentation,
        ]
        .into_iter()
        .find(|axis| axis.slug() == slug)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryContext {
    Ordinary,
    Stress,
    Romantic,
    Reflection,
}

pub const fn discovery_supported(axis: PersonalityAxis, context: DiscoveryContext) -> bool {
    match axis {
        PersonalityAxis::Nerve => matches!(context, DiscoveryContext::Stress),
        PersonalityAxis::Courtship | PersonalityAxis::Inclination => {
            matches!(context, DiscoveryContext::Romantic)
        }
        PersonalityAxis::SelfKnowledge => matches!(context, DiscoveryContext::Reflection),
        _ => true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirth {
    Neutral,
    Merry,
    Grave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Courtship {
    Neutral,
    Amorous,
    Proper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transparency {
    Neutral,
    Open,
    Guarded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfKnowledge {
    Neutral,
    Introspective,
    SelfDeceiving,
}

pub const fn self_knowledge_insight_modifier(value: SelfKnowledge) -> f32 {
    match value {
        SelfKnowledge::Neutral => 0.0,
        SelfKnowledge::Introspective => 1.0,
        SelfKnowledge::SelfDeceiving => -1.0,
    }
}

/// Conserved real-hour split for one actual discovery check.
pub const fn discovery_training_split(transparency: Transparency) -> (f32, f32) {
    match transparency {
        Transparency::Open => (DISCOVERY_TRAINING_HOURS, 0.0),
        Transparency::Neutral => (
            DISCOVERY_TRAINING_HOURS * 0.5,
            DISCOVERY_TRAINING_HOURS * 0.5,
        ),
        Transparency::Guarded => (0.0, DISCOVERY_TRAINING_HOURS),
    }
}

pub fn should_replace_belief(existing_confidence: f32, new_confidence: f32) -> bool {
    new_confidence.is_finite()
        && (!existing_confidence.is_finite() || new_confidence >= existing_confidence)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inclination {
    Men,
    Either,
    Women,
    Neither,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presentation {
    Man,
    Ambiguous,
    Woman,
}

/// Ambiguous presentation is compatible only with `Either`; a directional
/// preference requires an unambiguous signal.
pub const fn inclination_accepts(inclination: Inclination, presentation: Presentation) -> bool {
    match inclination {
        Inclination::Men => matches!(presentation, Presentation::Man),
        Inclination::Women => matches!(presentation, Presentation::Woman),
        Inclination::Either => true,
        Inclination::Neither => false,
    }
}

pub const fn mutually_attracted(
    actor_inclination: Inclination,
    actor_presentation: Presentation,
    target_inclination: Inclination,
    target_presentation: Presentation,
) -> bool {
    inclination_accepts(actor_inclination, target_presentation)
        && inclination_accepts(target_inclination, actor_presentation)
}

pub const fn humor_charm_modifier(actor: Mirth, target: Mirth) -> f32 {
    match (actor, target) {
        (Mirth::Merry, Mirth::Merry) => 0.75,
        (Mirth::Merry, Mirth::Neutral) => 0.35,
        (Mirth::Merry, Mirth::Grave) => -1.25,
        (_, Mirth::Grave) => -0.4,
        _ => 0.0,
    }
}

/// Grave and Proper characters give up their matching expressive Charm
/// approaches, but their reserve makes direct Command somewhat more credible.
/// The two modest bonuses stack without replacing trained Command.
pub const fn command_gravitas_modifier(mirth: Mirth, courtship: Courtship) -> f32 {
    let grave = if matches!(mirth, Mirth::Grave) {
        0.35
    } else {
        0.0
    };
    let proper = if matches!(courtship, Courtship::Proper) {
        0.35
    } else {
        0.0
    };
    grave + proper
}

/// Returns no modifier when flirting cannot succeed. Same-presentation mutual
/// attraction receives a recognition bonus on top of Courtship's stronger
/// response.
pub fn flirt_charm_modifier(
    actor_inclination: Inclination,
    actor_presentation: Presentation,
    target_inclination: Inclination,
    target_presentation: Presentation,
    target_courtship: Courtship,
) -> Option<f32> {
    if !mutually_attracted(
        actor_inclination,
        actor_presentation,
        target_inclination,
        target_presentation,
    ) {
        return None;
    }
    let courtship = match target_courtship {
        Courtship::Amorous => 1.5,
        Courtship::Neutral => 0.0,
        Courtship::Proper => -1.75,
    };
    let recognition = if actor_presentation == target_presentation {
        0.75
    } else {
        0.0
    };
    Some(courtship + recognition)
}

pub fn topic_for_source_kind(kind: &str) -> Option<SocialTopic> {
    match kind {
        "defeat" => Some(SocialTopic::Defeat),
        "injury" | "pain" => Some(SocialTopic::Injury),
        "fatigue" => Some(SocialTopic::Fatigue),
        "hunger" | "thirst" => Some(SocialTopic::Hunger),
        "religion" | "faith" | "holy_day" | "religious_discord" | "prayer" => {
            Some(SocialTopic::Faith)
        }
        "filth" | "cleanliness" => Some(SocialTopic::Filth),
        _ => None,
    }
}

pub fn social_source_eligible(kind: &str, magnitude: f32) -> bool {
    magnitude.is_finite() && magnitude < 0.0 && topic_for_source_kind(kind).is_some()
}

/// Counts projected source rows, rather than topics, which the exact observer
/// has not successfully addressed for the exact target and durable source ID.
pub fn unaddressed_social_source_count<'a>(
    actor_id: u64,
    target_id: u64,
    sources: impl IntoIterator<Item = (&'a str, &'a str, f32)>,
    interactions: impl IntoIterator<Item = (u64, u64, &'a str, bool)>,
) -> usize {
    let addressed: HashSet<&str> = interactions
        .into_iter()
        .filter_map(|(actor, target, source_id, succeeded)| {
            (succeeded && actor == actor_id && target == target_id).then_some(source_id)
        })
        .collect();
    sources
        .into_iter()
        .filter(|(source_id, kind, magnitude)| {
            social_source_eligible(kind, *magnitude) && !addressed.contains(source_id)
        })
        .count()
}

/// Current ranked projection inputs are already personality- and Will-adjusted.
/// Social restoration is visually capped by the gross actionable loss.
pub fn resolved_social_morale<'a>(sources: impl IntoIterator<Item = (&'a str, f32)>) -> f32 {
    let sources: Vec<_> = sources.into_iter().collect();
    let gross_actionable = sources
        .iter()
        .filter(|(kind, magnitude)| social_source_eligible(kind, *magnitude))
        .map(|(_, magnitude)| -*magnitude)
        .sum::<f32>()
        .max(0.0);
    let restoration = sources
        .iter()
        .filter(|(kind, magnitude)| *kind == "social_interaction" && *magnitude > 0.0)
        .map(|(_, magnitude)| *magnitude)
        .sum::<f32>();
    restoration.min(gross_actionable)
}

/// Deterministic bounded target plan for an opt-in automatic downtime pass.
pub fn automatic_social_targets(
    discretionary_minutes: u64,
    preferences: impl IntoIterator<Item = (u64, bool, bool)>,
    maximum_attempts: usize,
) -> Vec<u64> {
    if discretionary_minutes == 0 || maximum_attempts == 0 {
        return Vec::new();
    }
    let mut targets: Vec<_> = preferences
        .into_iter()
        .filter_map(|(target_id, enabled, actionable)| (enabled && actionable).then_some(target_id))
        .collect();
    targets.sort_unstable();
    targets.dedup();
    targets.truncate(maximum_attempts);
    targets
}

pub const fn axis_for_topic(topic: SocialTopic) -> Option<PersonalityAxis> {
    match topic {
        SocialTopic::Defeat => Some(PersonalityAxis::Drive),
        SocialTopic::Injury => Some(PersonalityAxis::SelfRegard),
        SocialTopic::Faith => Some(PersonalityAxis::Conviction),
        SocialTopic::Filth => Some(PersonalityAxis::Hygiene),
        SocialTopic::Fatigue | SocialTopic::Hunger => None,
    }
}

pub fn diagnosis_for_axis(
    axis: Option<PersonalityAxis>,
    truth: Option<i8>,
    beliefs: &[(PersonalityAxis, i8)],
) -> Option<bool> {
    let axis = axis?;
    let truth = truth?;
    beliefs
        .iter()
        .find_map(|(belief_axis, value)| (*belief_axis == axis).then_some(*value == truth))
}

pub fn canonical_cooldown_id(
    actor_id: u64,
    target_id: u64,
    topic: SocialTopic,
    action_kind: &str,
) -> String {
    format!("{actor_id}:{target_id}:{topic:?}:{action_kind}").to_ascii_lowercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SocialActionKind {
    Reflect,
    Listen,
    Commiserate,
    Pray,
    Reassure,
    LightenMood,
    Rally,
    Reframe,
    Flirt,
}

impl SocialActionKind {
    pub const fn reducer_value(self) -> &'static str {
        match self {
            Self::Reflect => "reflect",
            Self::Listen => "listen",
            Self::Commiserate => "commiserate",
            Self::Pray => "pray",
            Self::Reassure => "reassure",
            Self::LightenMood => "lighten_mood",
            Self::Rally => "command",
            Self::Reframe => "deception",
            Self::Flirt => "flirt",
        }
    }

    pub const fn skill_name(self, shares_concern: bool) -> &'static str {
        match self {
            Self::Reflect => "Insight",
            Self::Listen => "Insight",
            Self::Commiserate if shares_concern => "Insight",
            Self::Commiserate => "Deception",
            Self::Pray => "Religion",
            Self::Reassure => "Physiology",
            Self::LightenMood => "Charm",
            Self::Rally => "Command",
            Self::Reframe => "Deception",
            Self::Flirt => "Charm",
        }
    }

    pub const fn available_for(self, topic: SocialTopic) -> bool {
        match self {
            Self::Reflect | Self::Listen | Self::Commiserate => true,
            Self::Pray => !matches!(topic, SocialTopic::Filth),
            Self::Reassure => matches!(
                topic,
                SocialTopic::Injury | SocialTopic::Fatigue | SocialTopic::Hunger
            ),
            Self::LightenMood => !matches!(topic, SocialTopic::Faith),
            Self::Rally => matches!(
                topic,
                SocialTopic::Defeat | SocialTopic::Fatigue | SocialTopic::Faith
            ),
            Self::Reframe => matches!(
                topic,
                SocialTopic::Defeat | SocialTopic::Injury | SocialTopic::Faith
            ),
            Self::Flirt => matches!(topic, SocialTopic::Defeat | SocialTopic::Injury),
        }
    }

    pub const fn description(self, topic: SocialTopic, shares_concern: bool) -> &'static str {
        match (self, topic, shares_concern) {
            (Self::Reflect, _, _) => "Reflect on why this affects you",
            (Self::Listen, SocialTopic::Defeat, _) => "Ask how they feel about the defeat",
            (Self::Listen, SocialTopic::Injury, _) => "Ask how the injury is affecting them",
            (Self::Listen, SocialTopic::Fatigue, _) => "Ask how exhaustion is wearing on them",
            (Self::Listen, SocialTopic::Hunger, _) => "Ask how hunger is affecting them",
            (Self::Listen, SocialTopic::Faith, _) => "Ask what is troubling their conscience",
            (Self::Listen, SocialTopic::Filth, _) => "Ask why the grime is bothering them",
            (Self::Commiserate, SocialTopic::Defeat, true) => "Commiserate about the defeat",
            (Self::Commiserate, SocialTopic::Injury, true) => "Commiserate about being injured",
            (Self::Commiserate, SocialTopic::Fatigue, true) => "Commiserate about the exhaustion",
            (Self::Commiserate, SocialTopic::Hunger, true) => "Commiserate about going hungry",
            (Self::Commiserate, SocialTopic::Faith, true) => "Commiserate about the moral setback",
            (Self::Commiserate, SocialTopic::Filth, true) => "Commiserate about being filthy",
            (Self::Commiserate, SocialTopic::Defeat, false) => "Feign sympathy about the defeat",
            (Self::Commiserate, SocialTopic::Injury, false) => "Feign sympathy about the injury",
            (Self::Commiserate, SocialTopic::Fatigue, false) => {
                "Feign sympathy about the exhaustion"
            }
            (Self::Commiserate, SocialTopic::Hunger, false) => "Feign sympathy about going hungry",
            (Self::Commiserate, SocialTopic::Faith, false) => {
                "Feign sympathy about the moral setback"
            }
            (Self::Commiserate, SocialTopic::Filth, false) => "Feign sympathy about being filthy",
            (Self::Pray, _, _) => "Offer a prayer in their tradition",
            (Self::Reassure, SocialTopic::Injury, _) => {
                "Sit with them and speak calmly about what can be plainly observed"
            }
            (Self::Reassure, SocialTopic::Fatigue, _) => {
                "Attend to their weariness and acknowledge what they are feeling"
            }
            (Self::Reassure, SocialTopic::Hunger, _) => {
                "Stay with them and acknowledge the bodily distress of hunger"
            }
            (Self::LightenMood, SocialTopic::Defeat, _) => "Joke about bouncing back from defeat",
            (Self::LightenMood, SocialTopic::Injury, _) => "Joke to distract them from the pain",
            (Self::LightenMood, SocialTopic::Fatigue, _) => "Joke to help keep them awake",
            (Self::LightenMood, SocialTopic::Hunger, _) => "Joke about the empty provisions",
            (Self::LightenMood, SocialTopic::Filth, _) => "Joke about the mess they are in",
            (Self::Rally, SocialTopic::Defeat, _) => "Rally them after the defeat",
            (Self::Rally, SocialTopic::Fatigue, _) => "Urge them to keep going despite exhaustion",
            (Self::Rally, SocialTopic::Faith, _) => "Call on them to stand by their convictions",
            (Self::Reframe, SocialTopic::Defeat, _) => "Cast the defeat as a lesson",
            (Self::Reframe, SocialTopic::Injury, _) => {
                "Claim the injury is less serious than it looks"
            }
            (Self::Reframe, SocialTopic::Faith, _) => {
                "Offer a reassuring interpretation of the moral setback"
            }
            (Self::Flirt, SocialTopic::Defeat, _) => {
                "Tell them they remain impressive despite the defeat"
            }
            (Self::Flirt, SocialTopic::Injury, _) => "Tell them the scar makes them look striking",
            // Callers must check `available_for`; this keeps malformed clients
            // from eliciting invented topic-specific copy.
            _ => "This approach does not fit the concern",
        }
    }

    pub const fn risk(self) -> f32 {
        match self {
            Self::Reflect => 0.05,
            Self::Listen => 0.05,
            Self::Commiserate => 0.2,
            Self::Pray => 0.45,
            Self::Reassure => 0.12,
            Self::LightenMood => 0.45,
            Self::Rally => 0.55,
            Self::Reframe => 0.65,
            Self::Flirt => 0.85,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BedsideReassuranceApproach {
    pub counsel: &'static str,
    pub effectiveness: f32,
    pub risk: f32,
}

/// Closed catalog for honest bedside attention. It is deliberately limited to
/// bodily concerns that a physician can acknowledge without diagnosing,
/// predicting, triaging, or prescribing treatment.
pub const fn bedside_reassurance_approach(
    topic: SocialTopic,
) -> Option<BedsideReassuranceApproach> {
    let (counsel, effectiveness, risk) = match topic {
        SocialTopic::Injury => (
            "Sit at their bedside, attend to what they describe, and speak calmly about what is plainly visible",
            0.55,
            0.12,
        ),
        SocialTopic::Fatigue => (
            "Attend to their weariness and acknowledge the strain they describe",
            0.42,
            0.08,
        ),
        SocialTopic::Hunger => (
            "Stay with them and acknowledge the bodily distress they describe",
            0.25,
            0.06,
        ),
        SocialTopic::Defeat | SocialTopic::Faith | SocialTopic::Filth => return None,
    };
    Some(BedsideReassuranceApproach {
        counsel,
        effectiveness,
        risk,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrayerApproach {
    pub devotion: &'static str,
    pub intention: &'static str,
    pub effectiveness: f32,
    pub risk: f32,
}

/// Closed, exhaustive social-prayer catalog. Topic stems carry the mechanical
/// profile while each tradition supplies grounded, concise devotional copy.
pub const fn prayer_approach(
    religion: OfficialReligion,
    topic: SocialTopic,
) -> Option<PrayerApproach> {
    let (base_effectiveness, risk) = match topic {
        SocialTopic::Defeat => (1.0, 0.45),
        SocialTopic::Injury => (1.0, 0.40),
        SocialTopic::Fatigue => (0.80, 0.30),
        SocialTopic::Hunger => (0.65, 0.30),
        SocialTopic::Faith => (1.15, 0.55),
        SocialTopic::Filth => return None,
    };
    let bonus = match (religion, topic) {
        (OfficialReligion::Lutheran | OfficialReligion::Reformed, SocialTopic::Defeat)
        | (
            OfficialReligion::RomanCatholic
            | OfficialReligion::EasternOrthodox
            | OfficialReligion::Judaism,
            SocialTopic::Injury,
        )
        | (OfficialReligion::Islamic, SocialTopic::Fatigue) => 0.10,
        _ => 0.0,
    };
    let devotion = match religion {
        OfficialReligion::RomanCatholic => "Pray a psalm and the Pater Noster",
        OfficialReligion::Lutheran => "Pray a psalm and recall Christ's promise",
        OfficialReligion::Reformed => "Pray from Scripture and trust in providence",
        OfficialReligion::Anglican => "Offer a psalm, litany, and supplication",
        OfficialReligion::EasternOrthodox => "Pray a psalm and the Jesus Prayer",
        OfficialReligion::Islamic => "Make du'a",
        OfficialReligion::Judaism => "Recite Tehillim and pray",
    };
    let intention = match topic {
        SocialTopic::Defeat => "for courage after defeat",
        SocialTopic::Injury => "for healing",
        SocialTopic::Fatigue => "for patience and renewed strength",
        SocialTopic::Hunger => "for daily provision",
        SocialTopic::Faith => "for guidance and steadfast faith",
        SocialTopic::Filth => return None,
    };
    Some(PrayerApproach {
        devotion,
        intention,
        effectiveness: base_effectiveness + bonus,
        risk,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SocialResolutionProfile {
    pub risk: f32,
    pub effectiveness: f32,
    pub chance_modifier: f32,
    pub failure_multiplier: f32,
}

impl SocialResolutionProfile {
    pub const fn ordinary(action: SocialActionKind) -> Self {
        Self {
            risk: action.risk(),
            effectiveness: 1.0,
            chance_modifier: 0.0,
            failure_multiplier: 1.0,
        }
    }
}

pub const fn bedside_reassurance_resolution_profile(
    approach: BedsideReassuranceApproach,
) -> SocialResolutionProfile {
    SocialResolutionProfile {
        risk: approach.risk,
        effectiveness: approach.effectiveness,
        chance_modifier: 0.0,
        failure_multiplier: 1.0,
    }
}

/// Conviction codes are canonical axis-local values: 1 Zealous, 2
/// Irreverent, and 0 neutral. The bounded modifiers affect prayer without
/// leaking the target's true personality into presentation.
pub fn prayer_resolution_profile(
    approach: PrayerApproach,
    target_conviction: i8,
) -> SocialResolutionProfile {
    let (risk_delta, effectiveness, chance_modifier, failure_multiplier) = match target_conviction {
        1 => (0.05, 1.10, 0.04, 1.15),
        2 => (0.15, 0.85, -0.12, 1.10),
        _ => (0.0, 1.0, 0.0, 1.0),
    };
    SocialResolutionProfile {
        risk: (approach.risk + risk_delta).clamp(0.0, 0.95),
        effectiveness: (approach.effectiveness * effectiveness).clamp(0.0, 1.5),
        chance_modifier,
        failure_multiplier,
    }
}

/// Actor personality is authoritative availability, separate from whether an
/// action fits the current morale topic. Neutral and positive values retain
/// access; only the explicitly contrary value closes its expressive action.
pub const fn actor_allows_social_action(
    action: SocialActionKind,
    mirth: Mirth,
    courtship: Courtship,
) -> bool {
    match action {
        SocialActionKind::LightenMood => !matches!(mirth, Mirth::Grave),
        SocialActionKind::Flirt => !matches!(courtship, Courtship::Proper),
        _ => true,
    }
}

pub const fn actor_allows_social_prayer(conviction_code: i8) -> bool {
    conviction_code != 1
}

/// Select the available automatic approach which best combines the actor's
/// effective skill with their personality fit. Risk only breaks exact ties,
/// so automation does not collapse to the universally low-risk action.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutomaticSocialCandidate {
    pub action: SocialActionKind,
    pub skill_check: f32,
    pub personality_fit: f32,
    /// Resolved risk for this actor, target, and topic.
    pub risk: f32,
}

impl AutomaticSocialCandidate {
    pub const fn ordinary(
        action: SocialActionKind,
        skill_check: f32,
        personality_fit: f32,
    ) -> Self {
        Self {
            action,
            skill_check,
            personality_fit,
            risk: action.risk(),
        }
    }

    pub const fn with_resolved_risk(
        action: SocialActionKind,
        skill_check: f32,
        personality_fit: f32,
        risk: f32,
    ) -> Self {
        Self {
            action,
            skill_check,
            personality_fit,
            risk,
        }
    }
}

pub fn choose_automatic_social_action(
    topic: SocialTopic,
    candidates: impl IntoIterator<Item = AutomaticSocialCandidate>,
) -> Option<SocialActionKind> {
    candidates
        .into_iter()
        .filter(|candidate| {
            candidate.action != SocialActionKind::Reflect
                && candidate.action.available_for(topic)
                && candidate.skill_check.is_finite()
                && candidate.personality_fit.is_finite()
                && candidate.risk.is_finite()
        })
        .max_by(|left, right| {
            let left_score =
                left.skill_check.clamp(0.0, 5.0) + left.personality_fit.clamp(-2.0, 2.0);
            let right_score =
                right.skill_check.clamp(0.0, 5.0) + right.personality_fit.clamp(-2.0, 2.0);
            left_score
                .total_cmp(&right_score)
                .then_with(|| left.personality_fit.total_cmp(&right.personality_fit))
                .then_with(|| left.skill_check.total_cmp(&right.skill_check))
                .then_with(|| right.risk.total_cmp(&left.risk))
        })
        .map(|candidate| candidate.action)
}

#[derive(Debug, Clone, Copy)]
pub struct SocialAttempt {
    pub action: SocialActionKind,
    pub topic: SocialTopic,
    pub skill_check: f32,
    pub affinity: f32,
    pub familiarity_hours: f32,
    pub diagnosis_correct: Option<bool>,
    /// How touchy the true personality makes this topic, from 0 to 1.
    pub sensitivity: f32,
    /// Injected deterministic roll, from 0 to 1.
    pub roll: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SocialOutcome {
    pub succeeded: bool,
    pub morale_delta: f32,
    pub affinity_delta: f32,
    pub revealed_belief: bool,
}

/// The small personality surface used by an ordinary conversation.
///
/// Codes use the canonical personality values: neutral is zero, while one and
/// two are the opposed authored traits for Sociability and Outlook. Keeping
/// this input explicit lets settlement NPCs use their authored Mirth and
/// Transparency while treating personality axes they do not own as neutral.
#[derive(Debug, Clone, Copy)]
pub struct CasualChatDisposition {
    pub mirth: Mirth,
    pub transparency: Transparency,
    pub sociability: i8,
    pub outlook: i8,
}

#[derive(Debug, Clone, Copy)]
pub struct CasualChatInput {
    pub charm_check: f32,
    pub insight_check: f32,
    pub affinity: f32,
    pub familiarity_hours: f32,
    pub actor: CasualChatDisposition,
    pub target: CasualChatDisposition,
    /// Injected deterministic roll, from zero to one, for one quarter hour.
    pub roll: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CasualChatOutcome {
    pub positive: bool,
    pub morale_delta: f32,
    pub affinity_delta: f32,
}

/// Mutual personality fit for an unstructured conversation. Matching strong
/// traits help, opposed traits hinder, and neutral traits reveal nothing about
/// the outcome on their own.
pub fn casual_chat_personality_fit(
    actor: CasualChatDisposition,
    target: CasualChatDisposition,
) -> f32 {
    let paired = |left: i8, right: i8, same: f32, opposed: f32| {
        if left == 0 || right == 0 {
            0.0
        } else if left == right {
            same
        } else {
            opposed
        }
    };
    let mirth = match (actor.mirth, target.mirth) {
        (Mirth::Neutral, _) | (_, Mirth::Neutral) => 0.0,
        (left, right) if left == right => 0.45,
        _ => -0.5,
    };
    let transparency = match (actor.transparency, target.transparency) {
        (Transparency::Neutral, _) | (_, Transparency::Neutral) => 0.0,
        (left, right) if left == right => 0.25,
        _ => -0.2,
    };
    (mirth
        + transparency
        + paired(actor.sociability, target.sociability, 0.35, -0.4)
        + paired(actor.outlook, target.outlook, 0.2, -0.25))
    .clamp(-1.5, 1.5)
}

/// Resolve one quarter hour of ordinary conversation. Even a skilled,
/// compatible pair can have an awkward stretch, while poor matches can still
/// occasionally connect. The result deliberately exposes no chance or roll.
pub fn resolve_casual_chat(input: CasualChatInput) -> CasualChatOutcome {
    let personality_fit = casual_chat_personality_fit(input.actor, input.target);
    let relationship = (input.affinity / 100.0).clamp(-1.0, 1.0) * 0.12
        + (input.familiarity_hours / 100.0).min(1.0) * 0.08;
    let chance = (0.38
        + input.charm_check.clamp(0.0, 5.0) * 0.065
        + input.insight_check.clamp(0.0, 5.0) * 0.035
        + personality_fit * 0.12
        + relationship)
        .clamp(0.08, 0.92);
    let positive = input.roll.clamp(0.0, 1.0) < chance;
    let morale_delta = if positive {
        0.35 + input.charm_check.clamp(0.0, 5.0) * 0.05
    } else {
        -(0.3 + (-personality_fit).max(0.0) * 0.2)
    };
    let affinity_delta = if positive {
        affinity_gain(input.affinity, morale_delta)
    } else {
        morale_delta * 0.65
    };
    CasualChatOutcome {
        positive,
        morale_delta,
        affinity_delta,
    }
}

pub fn settle_affinity(anchor: f32, elapsed_minutes: u64) -> f32 {
    if !anchor.is_finite() {
        return 0.0;
    }
    let factor = 0.5_f32.powf(elapsed_minutes as f32 / AFFINITY_HALF_LIFE_MINUTES as f32);
    let value = anchor.clamp(AFFINITY_MIN, AFFINITY_MAX) * factor;
    if value.abs() < 0.000_1 { 0.0 } else { value }
}

/// A realized morale improvement grants less affinity near the positive cap.
pub fn affinity_gain(current: f32, realized_morale_gain: f32) -> f32 {
    if !realized_morale_gain.is_finite() || realized_morale_gain <= 0.0 {
        return 0.0;
    }
    let headroom =
        (AFFINITY_MAX - current.clamp(AFFINITY_MIN, AFFINITY_MAX)) / (AFFINITY_MAX - AFFINITY_MIN);
    (realized_morale_gain * 0.8 * headroom).max(0.0)
}

pub fn canonical_pair(left: u64, right: u64) -> Option<(u64, u64)> {
    (left != right).then(|| (left.min(right), left.max(right)))
}

pub fn effective_familiarity_hours(
    shared_minutes: u64,
    current_party_size: usize,
    together: bool,
) -> f32 {
    let divisor = if together {
        current_party_size.max(1)
    } else {
        1
    };
    shared_minutes as f32 / 60.0 / divisor as f32
}

pub fn resolve_social_attempt(attempt: SocialAttempt) -> SocialOutcome {
    resolve_social_attempt_with_profile(attempt, SocialResolutionProfile::ordinary(attempt.action))
}

pub fn resolve_social_attempt_with_profile(
    attempt: SocialAttempt,
    profile: SocialResolutionProfile,
) -> SocialOutcome {
    let risk = profile.risk.clamp(0.0, 0.95);
    let relationship = (attempt.affinity / 100.0).clamp(-1.0, 1.0) * 0.18
        + (attempt.familiarity_hours / 100.0).min(1.0) * 0.12;
    let diagnosis = match attempt.diagnosis_correct {
        Some(true) => 0.15,
        Some(false) => -0.22 - risk * 0.18,
        None => 0.0,
    };
    let presumptuousness = risk
        * attempt.sensitivity.clamp(0.0, 1.0)
        * (1.0 - ((attempt.affinity + 100.0) / 200.0).clamp(0.0, 1.0));
    let chance = (0.38
        + attempt.skill_check.clamp(0.0, 5.0) * 0.08
        + relationship
        + diagnosis
        + profile.chance_modifier.clamp(-0.25, 0.25)
        - presumptuousness)
        .clamp(0.05, 0.95);
    let succeeded = attempt.roll.clamp(0.0, 1.0) < chance;
    let magnitude = 1.0 + 5.0 * risk;
    let morale_delta = if succeeded {
        magnitude * profile.effectiveness.clamp(0.0, 2.0)
    } else {
        -(0.5 + 5.5 * risk) * profile.failure_multiplier.clamp(0.5, 1.5)
    };
    let affinity_delta = if morale_delta > 0.0 {
        affinity_gain(attempt.affinity, morale_delta)
    } else {
        morale_delta * (0.3 + risk * 0.7)
    };
    SocialOutcome {
        succeeded,
        morale_delta,
        affinity_delta,
        revealed_belief: matches!(
            attempt.action,
            SocialActionKind::Listen | SocialActionKind::Reflect
        ),
    }
}

/// Hard failure used by a flirt which fails mutual compatibility. It has no
/// random minimum-success floor and therefore cannot produce positive morale
/// or affinity.
pub fn incompatible_flirt_outcome() -> SocialOutcome {
    let risk = SocialActionKind::Flirt.risk();
    let morale_delta = -(0.5 + 5.5 * risk);
    SocialOutcome {
        succeeded: false,
        morale_delta,
        affinity_delta: morale_delta * (0.3 + risk * 0.7),
        revealed_belief: false,
    }
}

/// Produces a deterministic, legal, axis-aware diagnosis which may be wrong.
pub fn diagnosed_axis(
    axis: PersonalityAxis,
    true_value: i8,
    insight: f32,
    deception: f32,
    roll: f32,
) -> (i8, f32) {
    let legal = axis.legal_values();
    debug_assert!(legal.contains(&true_value));
    let obscurity = axis.base_obscurity() as f32
        + if axis.is_neutral_code(true_value) {
            1.0
        } else {
            0.0
        };
    let chance = (0.72 + 0.09 * insight.clamp(0.0, 5.0)
        - 0.07 * deception.clamp(0.0, 5.0)
        - 0.08 * obscurity)
        .clamp(0.05, 0.95);
    let correct = roll.clamp(0.0, 1.0) < chance;
    let belief = if correct {
        true_value
    } else {
        let alternatives: Vec<_> = legal
            .iter()
            .copied()
            .filter(|value| *value != true_value)
            .collect();
        // Conditional entropy within the failed interval remains uniform,
        // instead of reusing the upper tail of the success roll directly.
        let failed_entropy =
            ((roll.clamp(0.0, 0.999_999) - chance) / (1.0 - chance)).clamp(0.0, 0.999_999);
        alternatives
            [((failed_entropy * alternatives.len() as f32) as usize).min(alternatives.len() - 1)]
    };
    let confidence = if correct {
        chance
    } else {
        (1.0 - chance) / (legal.len() - 1) as f32
    };
    (belief, confidence)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimAssessmentDirection {
    Unknown,
    LikelyFalse,
    LikelyTrue,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClaimAssessment {
    pub direction: ClaimAssessmentDirection,
    /// Bounded presentation strength. This is a noisy demeanor perception, not
    /// confidence in factual accuracy.
    pub strength: f32,
}

/// Produce a fallible observer perception for one atomic claim.
///
/// Noise deliberately dominates weak checks, so any demeanor can produce any
/// colored direction. Better Insight strengthens a non-ambiguous private
/// demeanor signal without ever making it certain.
pub fn assess_testimony_claim(
    demeanor_truth_signal: f32,
    insight_check: f32,
    roll: f32,
) -> ClaimAssessment {
    let insight = insight_check.clamp(0.0, 5.0);
    let noise = (roll.clamp(0.0, 1.0) * 2.0 - 1.0) * 0.75;
    let signal = demeanor_truth_signal.clamp(-1.0, 1.0) * (0.1 + insight * 0.06) + noise;
    let absolute = signal.abs();
    let direction = if absolute < 0.18 {
        ClaimAssessmentDirection::Unknown
    } else if signal < 0.0 {
        ClaimAssessmentDirection::LikelyFalse
    } else {
        ClaimAssessmentDirection::LikelyTrue
    };
    ClaimAssessment {
        direction,
        strength: ((absolute - 0.18).max(0.0) / 0.82).clamp(0.0, 1.0),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimChallengeApproach {
    Charm,
    Command,
    Bluff,
}

impl ClaimChallengeApproach {
    pub const fn skill_name(self) -> &'static str {
        match self {
            Self::Charm => "Charm",
            Self::Command => "Command",
            Self::Bluff => "Deception",
        }
    }

    const fn social_action(self) -> SocialActionKind {
        match self {
            Self::Charm => SocialActionKind::LightenMood,
            Self::Command => SocialActionKind::Rally,
            Self::Bluff => SocialActionKind::Reframe,
        }
    }

    pub const fn leverage(self) -> f32 {
        match self {
            Self::Charm => 0.0,
            Self::Command => 0.45,
            Self::Bluff => 0.9,
        }
    }

    pub const fn failure_affinity_loss(self) -> f32 {
        match self {
            Self::Charm => -0.8,
            Self::Command => -1.4,
            Self::Bluff => -2.5,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ClaimChallengeInput {
    pub approach: ClaimChallengeApproach,
    pub claim_is_factually_accurate: bool,
    pub skill_check: f32,
    pub affinity: f32,
    pub familiarity_hours: f32,
    /// Current settled NPC morale in the ordinary -100..=100 strategic range.
    /// It contributes at most +/-0.12 chance before the common clamp.
    pub current_morale: f32,
    pub target_transparency: Transparency,
    pub target_mirth: Mirth,
    pub roll: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClaimChallengeOutcome {
    pub succeeded: bool,
    pub morale_delta: f32,
    pub affinity_delta: f32,
}

/// Resolve a response to one atomic claim. Truthful claims always use the same
/// safe failed-challenge result as an insufficient check against an untrue
/// claim, so callers cannot infer why a response failed.
pub fn resolve_claim_challenge(input: ClaimChallengeInput) -> ClaimChallengeOutcome {
    let personality_fit = match (
        input.approach,
        input.target_transparency,
        input.target_mirth,
    ) {
        (ClaimChallengeApproach::Charm, _, Mirth::Merry) => 0.45,
        (ClaimChallengeApproach::Charm, _, Mirth::Grave) => -0.45,
        (ClaimChallengeApproach::Command, Transparency::Open, _) => -0.3,
        (ClaimChallengeApproach::Command, Transparency::Guarded, _) => 0.25,
        (ClaimChallengeApproach::Bluff, Transparency::Open, _) => 0.25,
        (ClaimChallengeApproach::Bluff, Transparency::Guarded, _) => -0.35,
        _ => 0.0,
    };
    let sensitivity = match input.target_transparency {
        Transparency::Open => 0.2,
        Transparency::Neutral => 0.5,
        Transparency::Guarded => 0.85,
    };
    let morale_fit = input.current_morale.clamp(-100.0, 100.0) / 100.0 * 1.5;
    let outcome = resolve_social_attempt(SocialAttempt {
        action: input.approach.social_action(),
        topic: SocialTopic::Defeat,
        skill_check: input.skill_check + personality_fit + morale_fit + input.approach.leverage(),
        affinity: input.affinity,
        familiarity_hours: input.familiarity_hours,
        diagnosis_correct: None,
        sensitivity,
        roll: input.roll,
    });
    let succeeded = !input.claim_is_factually_accurate && outcome.succeeded;
    let affinity_delta = if succeeded {
        if input.approach == ClaimChallengeApproach::Command {
            -0.4
        } else {
            0.0
        }
    } else {
        input.approach.failure_affinity_loss()
    };
    ClaimChallengeOutcome {
        succeeded,
        morale_delta: if succeeded {
            outcome.morale_delta
        } else {
            outcome.morale_delta.min(0.0)
        },
        affinity_delta,
    }
}

pub fn realized_affinity_delta(current: f32, requested_delta: f32) -> f32 {
    (current + requested_delta).clamp(AFFINITY_MIN, AFFINITY_MAX)
        - current.clamp(AFFINITY_MIN, AFFINITY_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casual_chat_uses_social_skills_and_mutual_personality_without_certainty() {
        let compatible = CasualChatDisposition {
            mirth: Mirth::Merry,
            transparency: Transparency::Open,
            sociability: 1,
            outlook: 1,
        };
        let incompatible = CasualChatDisposition {
            mirth: Mirth::Grave,
            transparency: Transparency::Guarded,
            sociability: 2,
            outlook: 2,
        };
        let skilled = resolve_casual_chat(CasualChatInput {
            charm_check: 5.0,
            insight_check: 5.0,
            affinity: 0.0,
            familiarity_hours: 0.0,
            actor: compatible,
            target: compatible,
            roll: 0.5,
        });
        let poor_match = resolve_casual_chat(CasualChatInput {
            charm_check: 0.0,
            insight_check: 0.0,
            affinity: 0.0,
            familiarity_hours: 0.0,
            actor: compatible,
            target: incompatible,
            roll: 0.5,
        });
        assert!(skilled.positive);
        assert!(skilled.affinity_delta > 0.0);
        assert!(!poor_match.positive);
        assert!(poor_match.affinity_delta < 0.0);

        assert!(
            !resolve_casual_chat(CasualChatInput {
                roll: 0.99,
                ..CasualChatInput {
                    charm_check: 5.0,
                    insight_check: 5.0,
                    affinity: 100.0,
                    familiarity_hours: 100.0,
                    actor: compatible,
                    target: compatible,
                    roll: 0.0,
                }
            })
            .positive
        );
        assert!(
            resolve_casual_chat(CasualChatInput {
                roll: 0.0,
                ..CasualChatInput {
                    charm_check: 0.0,
                    insight_check: 0.0,
                    affinity: -100.0,
                    familiarity_hours: 0.0,
                    actor: compatible,
                    target: incompatible,
                    roll: 1.0,
                }
            })
            .positive
        );
    }

    #[test]
    fn decay_is_partition_independent_and_never_crosses_neutral() {
        for start in [-80.0, 80.0] {
            let once = settle_affinity(start, 40_000);
            let split = settle_affinity(settle_affinity(start, 10_000), 30_000);
            assert!((once - split).abs() < 0.0001);
            assert_eq!(once.signum(), start.signum());
            assert!(once.abs() < start.abs());
        }
    }

    #[test]
    fn claim_assessments_are_bounded_and_overlap_demeanor_states() {
        for truth_signal in [-1.0, 0.0, 1.0] {
            let low = assess_testimony_claim(truth_signal, 3.0, 0.0);
            let high = assess_testimony_claim(truth_signal, 3.0, 1.0);
            assert_ne!(low.direction, high.direction);
            assert!((0.0..=1.0).contains(&low.strength));
            assert!((0.0..=1.0).contains(&high.strength));
        }
        assert_eq!(
            assess_testimony_claim(1.0, 0.0, 0.43).direction,
            ClaimAssessmentDirection::Unknown
        );
        let correct = |insight| {
            (0..1_000)
                .filter(|index| {
                    assess_testimony_claim(1.0, insight, *index as f32 / 999.0).direction
                        == ClaimAssessmentDirection::LikelyTrue
                })
                .count()
        };
        assert!(correct(5.0) > correct(0.0));
    }

    #[test]
    fn challenge_risk_and_leverage_increase_by_approach() {
        assert!(
            ClaimChallengeApproach::Charm.leverage() < ClaimChallengeApproach::Command.leverage()
        );
        assert!(
            ClaimChallengeApproach::Command.leverage() < ClaimChallengeApproach::Bluff.leverage()
        );
        assert!(
            ClaimChallengeApproach::Charm.failure_affinity_loss().abs()
                < ClaimChallengeApproach::Command
                    .failure_affinity_loss()
                    .abs()
        );
        assert!(
            ClaimChallengeApproach::Command
                .failure_affinity_loss()
                .abs()
                < ClaimChallengeApproach::Bluff.failure_affinity_loss().abs()
        );
    }

    #[test]
    fn truthful_and_insufficient_challenges_share_safe_failure() {
        let attempt = |claim_is_factually_accurate, roll| {
            resolve_claim_challenge(ClaimChallengeInput {
                approach: ClaimChallengeApproach::Charm,
                claim_is_factually_accurate,
                skill_check: 2.5,
                affinity: 0.0,
                familiarity_hours: 0.0,
                current_morale: 0.0,
                target_transparency: Transparency::Neutral,
                target_mirth: Mirth::Neutral,
                roll,
            })
        };
        let truthful = attempt(true, 0.0);
        let insufficient = attempt(false, 1.0);
        assert!(!truthful.succeeded);
        assert!(!insufficient.succeeded);
        assert_eq!(truthful.affinity_delta, insufficient.affinity_delta);
    }

    #[test]
    fn command_always_takes_an_affinity_toll() {
        let outcome = resolve_claim_challenge(ClaimChallengeInput {
            approach: ClaimChallengeApproach::Command,
            claim_is_factually_accurate: false,
            skill_check: 5.0,
            affinity: 100.0,
            familiarity_hours: 100.0,
            current_morale: 0.0,
            target_transparency: Transparency::Open,
            target_mirth: Mirth::Neutral,
            roll: 0.0,
        });
        assert!(outcome.succeeded);
        assert!(outcome.affinity_delta < 0.0);
    }

    #[test]
    fn realized_affinity_delta_reports_clamping() {
        assert!((realized_affinity_delta(-99.5, -2.5) + 0.5).abs() < 0.0001);
        assert_eq!(realized_affinity_delta(0.0, 0.0), 0.0);
    }

    #[test]
    fn positive_gain_requires_realized_improvement_and_diminishes() {
        assert_eq!(affinity_gain(0.0, 0.0), 0.0);
        assert_eq!(affinity_gain(0.0, -1.0), 0.0);
        assert!(affinity_gain(0.0, 5.0) > affinity_gain(90.0, 5.0));
    }

    #[test]
    fn familiarity_is_symmetric_and_party_size_adjusted() {
        assert_eq!(canonical_pair(9, 2), Some((2, 9)));
        assert_eq!(canonical_pair(2, 2), None);
        assert_eq!(effective_familiarity_hours(600, 5, true), 2.0);
        assert_eq!(effective_familiarity_hours(600, 0, false), 10.0);
    }

    #[test]
    fn risky_actions_have_stronger_upside_and_downside() {
        let base = SocialAttempt {
            action: SocialActionKind::Listen,
            topic: SocialTopic::Defeat,
            skill_check: 3.0,
            affinity: 0.0,
            familiarity_hours: 0.0,
            diagnosis_correct: Some(true),
            sensitivity: 0.0,
            roll: 0.0,
        };
        let safe = resolve_social_attempt(base);
        let risky = resolve_social_attempt(SocialAttempt {
            action: SocialActionKind::Flirt,
            ..base
        });
        assert!(risky.morale_delta > safe.morale_delta);
        let failed_safe = resolve_social_attempt(SocialAttempt { roll: 1.0, ..base });
        let failed_risky = resolve_social_attempt(SocialAttempt {
            action: SocialActionKind::Flirt,
            roll: 1.0,
            ..base
        });
        assert!(failed_risky.morale_delta < failed_safe.morale_delta);
    }

    #[test]
    fn sensitivity_and_wrong_diagnosis_can_turn_success_into_failure() {
        let base = SocialAttempt {
            action: SocialActionKind::Rally,
            topic: SocialTopic::Defeat,
            skill_check: 2.0,
            affinity: -20.0,
            familiarity_hours: 0.0,
            diagnosis_correct: Some(true),
            sensitivity: 0.0,
            roll: 0.4,
        };
        assert!(resolve_social_attempt(base).succeeded);
        assert!(
            !resolve_social_attempt(SocialAttempt {
                diagnosis_correct: Some(false),
                sensitivity: 1.0,
                ..base
            })
            .succeeded
        );
    }

    #[test]
    fn misdiagnosis_is_deterministic_and_axis_legal() {
        let first = diagnosed_axis(PersonalityAxis::Inclination, 1, 0.0, 5.0, 0.9);
        assert!(
            PersonalityAxis::Inclination
                .legal_values()
                .contains(&first.0)
        );
        assert_ne!(first.0, 1);
        assert_eq!(
            first,
            diagnosed_axis(PersonalityAxis::Inclination, 1, 0.0, 5.0, 0.9)
        );
        let conscience = diagnosed_axis(PersonalityAxis::Conscience, 3, 0.0, 5.0, 0.9);
        assert!(
            PersonalityAxis::Conscience
                .legal_values()
                .contains(&conscience.0)
        );
        let chance = 0.72 - 0.07 * 5.0 - 0.08 * 4.0;
        let mut alternatives = std::collections::HashSet::new();
        for segment in 0..3 {
            let entropy = (segment as f32 + 0.5) / 3.0;
            let roll = chance + entropy * (1.0 - chance);
            alternatives.insert(diagnosed_axis(PersonalityAxis::Inclination, 1, 0.0, 5.0, roll).0);
        }
        assert_eq!(alternatives, std::collections::HashSet::from([0, 2, 3]));
        let wrong = diagnosed_axis(PersonalityAxis::Inclination, 1, 0.0, 5.0, 0.99);
        assert!(wrong.1 < 0.5);
        assert!(should_replace_belief(wrong.1, 0.7));
        let obvious = diagnosed_axis(PersonalityAxis::Presentation, 0, 2.0, 2.0, 0.0);
        let ambiguous = diagnosed_axis(PersonalityAxis::Presentation, 1, 2.0, 2.0, 0.0);
        assert!(
            obvious.1 > ambiguous.1,
            "ambiguous presentation retains the neutral-value obscurity penalty"
        );
    }

    #[test]
    fn attraction_gate_and_rarity_bonus_are_fail_closed() {
        assert!(!mutually_attracted(
            Inclination::Neither,
            Presentation::Man,
            Inclination::Women,
            Presentation::Woman,
        ));
        assert_eq!(
            flirt_charm_modifier(
                Inclination::Neither,
                Presentation::Man,
                Inclination::Women,
                Presentation::Woman,
                Courtship::Amorous,
            ),
            None
        );
        let same = flirt_charm_modifier(
            Inclination::Men,
            Presentation::Man,
            Inclination::Men,
            Presentation::Man,
            Courtship::Neutral,
        )
        .unwrap();
        let other = flirt_charm_modifier(
            Inclination::Women,
            Presentation::Man,
            Inclination::Men,
            Presentation::Woman,
            Courtship::Neutral,
        )
        .unwrap();
        assert!(same > other);
        assert!(inclination_accepts(
            Inclination::Either,
            Presentation::Ambiguous
        ));
        assert!(!inclination_accepts(
            Inclination::Men,
            Presentation::Ambiguous
        ));
        let failed = incompatible_flirt_outcome();
        assert!(!failed.succeeded);
        assert!(failed.morale_delta < 0.0);
        assert!(failed.affinity_delta < 0.0);
    }

    #[test]
    fn courtship_is_stronger_than_mirth_and_contexts_are_gated() {
        assert!(
            flirt_charm_modifier(
                Inclination::Women,
                Presentation::Man,
                Inclination::Men,
                Presentation::Woman,
                Courtship::Amorous,
            )
            .unwrap()
                > humor_charm_modifier(Mirth::Merry, Mirth::Merry)
        );
        assert!(discovery_supported(
            PersonalityAxis::Inclination,
            DiscoveryContext::Romantic
        ));
        assert!(!discovery_supported(
            PersonalityAxis::Inclination,
            DiscoveryContext::Ordinary
        ));
        for transparency in [
            Transparency::Open,
            Transparency::Neutral,
            Transparency::Guarded,
        ] {
            let (insight, deception) = discovery_training_split(transparency);
            assert!((insight + deception - DISCOVERY_TRAINING_HOURS).abs() < f32::EPSILON);
        }
        assert_eq!(
            discovery_training_split(Transparency::Neutral),
            (0.125, 0.125)
        );
        assert!(should_replace_belief(0.6, 0.6));
        assert!(!should_replace_belief(0.8, 0.6));
        let introspective = diagnosed_axis(
            PersonalityAxis::SelfKnowledge,
            1,
            2.0 + self_knowledge_insight_modifier(SelfKnowledge::Introspective),
            2.0,
            0.5,
        );
        let neutral = diagnosed_axis(
            PersonalityAxis::SelfKnowledge,
            1,
            2.0 + self_knowledge_insight_modifier(SelfKnowledge::Neutral),
            2.0,
            0.5,
        );
        let self_deceiving = diagnosed_axis(
            PersonalityAxis::SelfKnowledge,
            1,
            2.0 + self_knowledge_insight_modifier(SelfKnowledge::SelfDeceiving),
            2.0,
            0.5,
        );
        assert!(introspective.1 > neutral.1);
        assert!(neutral.1 > self_deceiving.1);
    }

    #[test]
    fn reserved_traits_trade_expressive_actions_for_command_gravitas() {
        assert!(actor_allows_social_action(
            SocialActionKind::LightenMood,
            Mirth::Merry,
            Courtship::Neutral,
        ));
        assert!(!actor_allows_social_action(
            SocialActionKind::LightenMood,
            Mirth::Grave,
            Courtship::Neutral,
        ));
        assert!(actor_allows_social_action(
            SocialActionKind::Flirt,
            Mirth::Neutral,
            Courtship::Amorous,
        ));
        assert!(!actor_allows_social_action(
            SocialActionKind::Flirt,
            Mirth::Neutral,
            Courtship::Proper,
        ));
        assert_eq!(
            command_gravitas_modifier(Mirth::Neutral, Courtship::Neutral),
            0.0
        );
        assert_eq!(
            command_gravitas_modifier(Mirth::Grave, Courtship::Neutral),
            0.35
        );
        assert_eq!(
            command_gravitas_modifier(Mirth::Neutral, Courtship::Proper),
            0.35
        );
        assert_eq!(
            command_gravitas_modifier(Mirth::Grave, Courtship::Proper),
            0.7
        );
    }

    #[test]
    fn source_topics_are_closed_and_negative_only() {
        assert_eq!(topic_for_source_kind("defeat"), Some(SocialTopic::Defeat));
        assert_eq!(
            topic_for_source_kind("religious_discord"),
            Some(SocialTopic::Faith)
        );
        assert_eq!(topic_for_source_kind("prayer"), Some(SocialTopic::Faith));
        assert_eq!(topic_for_source_kind("social_interaction"), None);
        assert_eq!(topic_for_source_kind("made_up"), None);
        assert!(social_source_eligible("defeat", -1.0));
        assert!(!social_source_eligible("defeat", 1.0));
        assert!(!social_source_eligible("social_interaction", -1.0));
    }

    #[test]
    fn notification_count_is_per_source_actor_target_and_success() {
        let sources = [
            ("loss-a", "defeat", -3.0),
            ("loss-b", "defeat", -1.0),
            ("good", "victory", 2.0),
        ];
        let interactions = [
            (7, 9, "loss-a", false),
            (8, 9, "loss-a", true),
            (7, 10, "loss-b", true),
        ];
        assert_eq!(
            unaddressed_social_source_count(7, 9, sources, interactions),
            2
        );
        assert_eq!(
            unaddressed_social_source_count(
                7,
                9,
                sources,
                [(7, 9, "loss-a", true), (7, 9, "loss-b", true)]
            ),
            0
        );
    }

    #[test]
    fn resolved_segment_uses_projected_values_and_caps_at_actionable_loss() {
        assert_eq!(
            resolved_social_morale([
                ("defeat", -4.0),
                ("injury", -2.0),
                ("social_interaction", 3.0),
            ]),
            3.0
        );
        assert_eq!(
            resolved_social_morale([("defeat", -2.0), ("social_interaction", 8.0)]),
            2.0
        );
        assert_eq!(
            resolved_social_morale([("made_up", -9.0), ("social_interaction", 4.0)]),
            0.0
        );
    }

    #[test]
    fn automatic_target_plan_requires_downtime_and_is_enabled_stable_and_bounded() {
        let preferences = [
            (9, true, true),
            (4, false, true),
            (7, true, true),
            (3, true, true),
            (7, true, true),
        ];
        assert!(automatic_social_targets(0, preferences, 3).is_empty());
        assert_eq!(automatic_social_targets(15, preferences, 2), vec![3, 7]);
        assert!(automatic_social_targets(15, preferences, 0).is_empty());
    }

    #[test]
    fn quiet_low_ids_do_not_starve_an_actionable_higher_target() {
        let preferences = [
            (1, true, false),
            (2, true, false),
            (3, true, false),
            (4, true, true),
        ];
        assert_eq!(automatic_social_targets(60, preferences, 3), vec![4]);
    }

    #[test]
    fn automatic_action_combines_personality_and_skill_instead_of_forcing_listen() {
        let action = choose_automatic_social_action(
            SocialTopic::Defeat,
            [
                AutomaticSocialCandidate::ordinary(SocialActionKind::Listen, 3.0, 0.0),
                AutomaticSocialCandidate::ordinary(SocialActionKind::LightenMood, 3.5, 1.0),
                AutomaticSocialCandidate::ordinary(SocialActionKind::Rally, 4.0, 0.0),
                AutomaticSocialCandidate::ordinary(SocialActionKind::Flirt, 1.0, 0.0),
            ],
        );
        assert_eq!(action, Some(SocialActionKind::LightenMood));

        let skilled = choose_automatic_social_action(
            SocialTopic::Defeat,
            [
                AutomaticSocialCandidate::ordinary(SocialActionKind::Listen, 1.0, 1.0),
                AutomaticSocialCandidate::ordinary(SocialActionKind::Rally, 4.5, 0.0),
            ],
        );
        assert_eq!(skilled, Some(SocialActionKind::Rally));
    }

    #[test]
    fn automatic_action_rejects_actions_that_do_not_fit_the_topic() {
        assert_eq!(
            choose_automatic_social_action(
                SocialTopic::Hunger,
                [
                    AutomaticSocialCandidate::ordinary(SocialActionKind::Flirt, 5.0, 2.0),
                    AutomaticSocialCandidate::ordinary(SocialActionKind::Listen, 1.0, 0.0),
                ],
            ),
            Some(SocialActionKind::Listen)
        );
    }

    #[test]
    fn automatic_action_exact_ties_prefer_lower_risk() {
        assert_eq!(
            choose_automatic_social_action(
                SocialTopic::Defeat,
                [
                    AutomaticSocialCandidate::ordinary(SocialActionKind::Rally, 3.0, 0.0),
                    AutomaticSocialCandidate::ordinary(SocialActionKind::Listen, 3.0, 0.0),
                ],
            ),
            Some(SocialActionKind::Listen)
        );
        assert_eq!(
            choose_automatic_social_action(
                SocialTopic::Fatigue,
                [
                    AutomaticSocialCandidate::with_resolved_risk(
                        SocialActionKind::Pray,
                        3.0,
                        0.0,
                        0.30,
                    ),
                    AutomaticSocialCandidate::ordinary(SocialActionKind::LightenMood, 3.0, 0.0,),
                ],
            ),
            Some(SocialActionKind::Pray)
        );
        assert_eq!(
            choose_automatic_social_action(
                SocialTopic::Faith,
                [
                    AutomaticSocialCandidate::with_resolved_risk(
                        SocialActionKind::Pray,
                        3.0,
                        0.0,
                        0.60,
                    ),
                    AutomaticSocialCandidate::ordinary(SocialActionKind::Rally, 3.0, 0.0),
                ],
            ),
            Some(SocialActionKind::Rally)
        );
    }

    #[test]
    fn cooldown_identity_does_not_depend_on_source_row() {
        assert_eq!(
            canonical_cooldown_id(1, 2, SocialTopic::Defeat, "listen"),
            "1:2:defeat:listen"
        );
    }

    #[test]
    fn social_catalog_keeps_one_commiseration_path_and_filters_bad_fits() {
        for topic in [
            SocialTopic::Defeat,
            SocialTopic::Injury,
            SocialTopic::Fatigue,
            SocialTopic::Hunger,
            SocialTopic::Faith,
            SocialTopic::Filth,
        ] {
            assert!(SocialActionKind::Commiserate.available_for(topic));
        }
        assert_eq!(SocialActionKind::Commiserate.skill_name(true), "Insight");
        assert_eq!(SocialActionKind::Commiserate.skill_name(false), "Deception");
        assert!(!SocialActionKind::Flirt.available_for(SocialTopic::Hunger));
        assert!(!SocialActionKind::Rally.available_for(SocialTopic::Filth));
        assert!(!SocialActionKind::LightenMood.available_for(SocialTopic::Faith));
        assert!(SocialActionKind::Pray.available_for(SocialTopic::Faith));
        assert!(!SocialActionKind::Pray.available_for(SocialTopic::Filth));
        assert_eq!(
            SocialActionKind::Flirt.description(SocialTopic::Injury, false),
            "Tell them the scar makes them look striking"
        );
    }

    #[test]
    fn prayer_catalog_is_exhaustive_and_keeps_authored_relative_profiles() {
        for religion in OfficialReligion::ALL {
            for topic in [
                SocialTopic::Defeat,
                SocialTopic::Injury,
                SocialTopic::Fatigue,
                SocialTopic::Hunger,
                SocialTopic::Faith,
            ] {
                let approach = prayer_approach(religion, topic).expect("catalog entry");
                assert!(!approach.devotion.is_empty());
                assert!(!approach.intention.is_empty());
                assert!(approach.effectiveness > 0.0);
                assert!((0.0..1.0).contains(&approach.risk));
            }
            assert_eq!(prayer_approach(religion, SocialTopic::Filth), None);
        }
        assert!(
            prayer_approach(OfficialReligion::Lutheran, SocialTopic::Defeat)
                .unwrap()
                .effectiveness
                > prayer_approach(OfficialReligion::Anglican, SocialTopic::Defeat)
                    .unwrap()
                    .effectiveness
        );
        assert!(
            prayer_approach(OfficialReligion::Islamic, SocialTopic::Fatigue)
                .unwrap()
                .effectiveness
                > prayer_approach(OfficialReligion::Anglican, SocialTopic::Fatigue)
                    .unwrap()
                    .effectiveness
        );
    }

    #[test]
    fn bedside_reassurance_is_authored_only_for_bodily_concerns() {
        let injury = bedside_reassurance_approach(SocialTopic::Injury).expect("injury reassurance");
        let fatigue =
            bedside_reassurance_approach(SocialTopic::Fatigue).expect("fatigue reassurance");
        let hunger = bedside_reassurance_approach(SocialTopic::Hunger).expect("hunger reassurance");

        assert_eq!(injury.effectiveness, 0.55);
        assert_eq!(injury.risk, 0.12);
        assert_eq!(fatigue.effectiveness, 0.42);
        assert_eq!(fatigue.risk, 0.08);
        assert_eq!(hunger.effectiveness, 0.25);
        assert_eq!(hunger.risk, 0.06);
        assert!(hunger.effectiveness < fatigue.effectiveness);
        assert!(fatigue.effectiveness < injury.effectiveness);
        for topic in [SocialTopic::Defeat, SocialTopic::Faith, SocialTopic::Filth] {
            assert_eq!(bedside_reassurance_approach(topic), None);
            assert!(!SocialActionKind::Reassure.available_for(topic));
        }
        for approach in [injury, fatigue, hunger] {
            let copy = approach.counsel.to_ascii_lowercase();
            for unsupported_claim in [
                "humour", "diagnos", "prognos", "recover", "treat", "cure", "prescri",
            ] {
                assert!(
                    !copy.contains(unsupported_claim),
                    "bedside copy made unsupported claim: {copy}"
                );
            }
        }
    }

    #[test]
    fn bedside_reassurance_is_conservative_beside_riskier_approaches() {
        let attempt = |topic, action| SocialAttempt {
            action,
            topic,
            skill_check: 5.0,
            affinity: 0.0,
            familiarity_hours: 0.0,
            diagnosis_correct: None,
            sensitivity: 0.5,
            roll: 0.0,
        };
        for topic in [
            SocialTopic::Injury,
            SocialTopic::Fatigue,
            SocialTopic::Hunger,
        ] {
            let approach = bedside_reassurance_approach(topic).unwrap();
            let profile = bedside_reassurance_resolution_profile(approach);
            let reassurance = resolve_social_attempt_with_profile(
                attempt(topic, SocialActionKind::Reassure),
                profile,
            );
            let prayer_approach =
                prayer_approach(OfficialReligion::Anglican, topic).expect("prayer topic");
            let prayer_profile = prayer_resolution_profile(prayer_approach, 0);
            let prayer = resolve_social_attempt_with_profile(
                attempt(topic, SocialActionKind::Pray),
                prayer_profile,
            );

            assert!(profile.risk < prayer_profile.risk);
            assert!(reassurance.morale_delta < prayer.morale_delta);
        }
        assert!(
            bedside_reassurance_approach(SocialTopic::Hunger)
                .unwrap()
                .effectiveness
                < bedside_reassurance_approach(SocialTopic::Injury)
                    .unwrap()
                    .effectiveness
        );
    }

    #[test]
    fn target_tradition_study_gates_correlated_religion_knowledge() {
        let mut hours = adventuresim_world_schema::ReligionHours {
            roman_catholic: 1_000.0,
            ..Default::default()
        };
        assert_eq!(hours.effective(OfficialReligion::Lutheran), 0.0);
        hours.lutheran = 1.0;
        assert!(hours.effective(OfficialReligion::Lutheran) > 800.0);
    }

    #[test]
    fn conviction_orders_prayer_success_and_backfire() {
        let approach = prayer_approach(OfficialReligion::Anglican, SocialTopic::Injury).unwrap();
        let neutral = prayer_resolution_profile(approach, 0);
        let zealous = prayer_resolution_profile(approach, 1);
        let irreverent = prayer_resolution_profile(approach, 2);
        assert!(zealous.effectiveness > neutral.effectiveness);
        assert!(irreverent.chance_modifier < neutral.chance_modifier);
        assert!(zealous.failure_multiplier > neutral.failure_multiplier);
        assert!(irreverent.risk > neutral.risk);

        let attempt = SocialAttempt {
            action: SocialActionKind::Pray,
            topic: SocialTopic::Injury,
            skill_check: 5.0,
            affinity: 0.0,
            familiarity_hours: 0.0,
            diagnosis_correct: None,
            sensitivity: 0.5,
            roll: 0.0,
        };
        let zealous_success = resolve_social_attempt_with_profile(attempt, zealous);
        let neutral_success = resolve_social_attempt_with_profile(attempt, neutral);
        assert!(zealous_success.morale_delta > neutral_success.morale_delta);
        let failed = SocialAttempt {
            roll: 1.0,
            ..attempt
        };
        assert!(
            resolve_social_attempt_with_profile(failed, zealous).morale_delta
                < resolve_social_attempt_with_profile(failed, neutral).morale_delta
        );
        assert!(!actor_allows_social_prayer(1));
        assert!(actor_allows_social_prayer(0));
        assert!(actor_allows_social_prayer(2));
    }

    #[test]
    fn automatic_care_can_select_prayer_and_reassurance_only_where_authored() {
        assert_eq!(
            choose_automatic_social_action(
                SocialTopic::Injury,
                [
                    AutomaticSocialCandidate::ordinary(SocialActionKind::Listen, 2.0, 0.0),
                    AutomaticSocialCandidate::with_resolved_risk(
                        SocialActionKind::Pray,
                        4.0,
                        0.25,
                        0.4,
                    ),
                ],
            ),
            Some(SocialActionKind::Pray)
        );
        assert_eq!(
            choose_automatic_social_action(
                SocialTopic::Filth,
                [AutomaticSocialCandidate::with_resolved_risk(
                    SocialActionKind::Pray,
                    5.0,
                    2.0,
                    0.3,
                )],
            ),
            None
        );
        assert_eq!(
            choose_automatic_social_action(
                SocialTopic::Fatigue,
                [
                    AutomaticSocialCandidate::ordinary(SocialActionKind::Listen, 2.0, 0.0),
                    AutomaticSocialCandidate::with_resolved_risk(
                        SocialActionKind::Reassure,
                        4.0,
                        0.0,
                        0.08,
                    ),
                ],
            ),
            Some(SocialActionKind::Reassure)
        );
        assert_eq!(
            choose_automatic_social_action(
                SocialTopic::Defeat,
                [AutomaticSocialCandidate::with_resolved_risk(
                    SocialActionKind::Reassure,
                    5.0,
                    2.0,
                    0.06,
                )],
            ),
            None
        );
    }

    #[test]
    fn only_relevant_axis_can_be_a_correct_diagnosis() {
        assert_eq!(
            axis_for_topic(SocialTopic::Defeat),
            Some(PersonalityAxis::Drive)
        );
        assert_eq!(axis_for_topic(SocialTopic::Fatigue), None);
        assert_eq!(
            diagnosis_for_axis(Some(PersonalityAxis::Drive), Some(1), &[]),
            None
        );
        assert_eq!(
            diagnosis_for_axis(
                Some(PersonalityAxis::Drive),
                Some(1),
                &[(PersonalityAxis::Conviction, 1)]
            ),
            None
        );
        assert_eq!(
            diagnosis_for_axis(
                Some(PersonalityAxis::Drive),
                Some(1),
                &[(PersonalityAxis::Drive, 1)]
            ),
            Some(true)
        );
        assert_eq!(
            diagnosis_for_axis(
                Some(PersonalityAxis::Drive),
                Some(1),
                &[(PersonalityAxis::Drive, 2)]
            ),
            Some(false)
        );
    }
}
