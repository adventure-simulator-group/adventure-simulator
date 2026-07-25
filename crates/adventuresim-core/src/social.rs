//! Deterministic strategic relationship and social-action rules.
//!
//! Persistence stores directional affinity separately from canonical, symmetric
//! familiarity. Presentation code must use the closed topic/action catalogue;
//! free-form morale labels are never parsed into actions.

use std::collections::HashSet;

pub const AFFINITY_MIN: f32 = -100.0;
pub const AFFINITY_MAX: f32 = 100.0;
pub const AFFINITY_HALF_LIFE_MINUTES: u64 = 30 * 24 * 60;
pub const SOCIAL_COOLDOWN_MINUTES: u64 = 24 * 60;

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
    Drive,
    SelfRegard,
    Conviction,
    Hygiene,
}

impl PersonalityAxis {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Drive => "drive",
            Self::SelfRegard => "self_regard",
            Self::Conviction => "conviction",
            Self::Hygiene => "hygiene",
        }
    }
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
            Self::LightenMood => "humor",
            Self::Rally => "command",
            Self::Reframe => "deception",
            Self::Flirt => "seduction",
        }
    }

    pub const fn skill_name(self, shares_concern: bool) -> &'static str {
        match self {
            Self::Reflect => "Self-awareness",
            Self::Listen => "Insight",
            Self::Commiserate if shares_concern => "Insight",
            Self::Commiserate => "Deception",
            Self::LightenMood => "Humor",
            Self::Rally => "Command",
            Self::Reframe => "Deception",
            Self::Flirt => "Seduction",
        }
    }

    pub const fn available_for(self, topic: SocialTopic) -> bool {
        match self {
            Self::Reflect | Self::Listen | Self::Commiserate => true,
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
            Self::LightenMood => 0.45,
            Self::Rally => 0.55,
            Self::Reframe => 0.65,
            Self::Flirt => 0.85,
        }
    }
}

/// Select the available automatic approach which best combines the actor's
/// effective skill with their personality fit. Risk only breaks exact ties,
/// so automation does not collapse to the universally low-risk action.
pub fn choose_automatic_social_action(
    topic: SocialTopic,
    candidates: impl IntoIterator<Item = (SocialActionKind, f32, f32)>,
) -> Option<SocialActionKind> {
    candidates
        .into_iter()
        .filter(|(action, skill_check, personality_fit)| {
            *action != SocialActionKind::Reflect
                && action.available_for(topic)
                && skill_check.is_finite()
                && personality_fit.is_finite()
        })
        .max_by(|left, right| {
            let left_score = left.1.clamp(0.0, 5.0) + left.2.clamp(-2.0, 2.0);
            let right_score = right.1.clamp(0.0, 5.0) + right.2.clamp(-2.0, 2.0);
            left_score
                .total_cmp(&right_score)
                .then_with(|| left.2.total_cmp(&right.2))
                .then_with(|| left.1.total_cmp(&right.1))
                .then_with(|| left.0.risk().total_cmp(&right.0.risk()))
        })
        .map(|(action, _, _)| action)
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
    let risk = attempt.action.risk();
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
    let chance = (0.38 + attempt.skill_check.clamp(0.0, 5.0) * 0.08 + relationship + diagnosis
        - presumptuousness)
        .clamp(0.05, 0.95);
    let succeeded = attempt.roll.clamp(0.0, 1.0) < chance;
    let magnitude = 1.0 + 5.0 * risk;
    let morale_delta = if succeeded {
        magnitude
    } else {
        -(0.5 + 5.5 * risk)
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

/// Produces a deterministic, plausible diagnosis which may be wrong.
pub fn diagnosed_axis(true_axis: i8, insight: f32, deception: f32, roll: f32) -> (i8, f32) {
    let chance =
        (0.45 + 0.1 * insight.clamp(0.0, 5.0) - 0.08 * deception.clamp(0.0, 5.0)).clamp(0.1, 0.95);
    let correct = roll.clamp(0.0, 1.0) < chance;
    let belief = if correct {
        true_axis.clamp(-1, 1)
    } else if true_axis == 0 {
        if roll < 0.5 { -1 } else { 1 }
    } else {
        -true_axis
    };
    (belief, if correct { chance } else { 1.0 - chance })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn misdiagnosis_is_deterministic_and_actionable() {
        assert_eq!(diagnosed_axis(1, 0.0, 5.0, 0.9), (-1, 0.9));
        assert_eq!(
            diagnosed_axis(1, 0.0, 5.0, 0.9),
            diagnosed_axis(1, 0.0, 5.0, 0.9)
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
                (SocialActionKind::Listen, 3.0, 0.0),
                (SocialActionKind::LightenMood, 3.5, 1.0),
                (SocialActionKind::Rally, 4.0, 0.0),
                (SocialActionKind::Flirt, 1.0, 0.0),
            ],
        );
        assert_eq!(action, Some(SocialActionKind::LightenMood));

        let skilled = choose_automatic_social_action(
            SocialTopic::Defeat,
            [
                (SocialActionKind::Listen, 1.0, 1.0),
                (SocialActionKind::Rally, 4.5, 0.0),
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
                    (SocialActionKind::Flirt, 5.0, 2.0),
                    (SocialActionKind::Listen, 1.0, 0.0),
                ],
            ),
            Some(SocialActionKind::Listen)
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
        assert_eq!(
            SocialActionKind::Flirt.description(SocialTopic::Injury, false),
            "Tell them the scar makes them look striking"
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
                &[(PersonalityAxis::Drive, -1)]
            ),
            Some(false)
        );
    }
}
