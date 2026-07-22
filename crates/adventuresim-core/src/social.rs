//! Deterministic strategic relationship and social-action rules.
//!
//! Persistence stores directional affinity separately from canonical, symmetric
//! familiarity. Presentation code must use the closed topic/action catalogue;
//! free-form morale labels are never parsed into actions.

pub const AFFINITY_MIN: f32 = -100.0;
pub const AFFINITY_MAX: f32 = 100.0;
pub const AFFINITY_HALF_LIFE_MINUTES: u64 = 30 * 24 * 60;
pub const SOCIAL_COOLDOWN_MINUTES: u64 = 24 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SocialTopic {
    General,
    Defeat,
    Injury,
    Fatigue,
    Hunger,
    Faith,
    Filth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SocialActionKind {
    Listen,
    Commiserate,
    LightenMood,
    Rally,
    Reframe,
    Flirt,
}

impl SocialActionKind {
    pub const fn skill_name(self) -> &'static str {
        match self {
            Self::Listen => "Insight",
            Self::Commiserate => "Self-awareness",
            Self::LightenMood => "Humor",
            Self::Rally => "Command",
            Self::Reframe => "Deception",
            Self::Flirt => "Seduction",
        }
    }

    pub const fn description(self, topic: SocialTopic) -> &'static str {
        match (self, topic) {
            (Self::Listen, _) => "Ask how they are feeling",
            (Self::Commiserate, SocialTopic::Defeat) => "Commiserate about recent defeat",
            (Self::Commiserate, _) => "Acknowledge what is troubling them",
            (Self::LightenMood, SocialTopic::Defeat) => "Make light of the setback",
            (Self::LightenMood, _) => "Try to lighten the mood",
            (Self::Rally, SocialTopic::Defeat) => "Rally them after the setback",
            (Self::Rally, _) => "Offer firm encouragement",
            (Self::Reframe, _) => "Offer a reassuring interpretation",
            (Self::Flirt, _) => "Offer personal encouragement",
        }
    }

    pub const fn risk(self) -> f32 {
        match self {
            Self::Listen => 0.05,
            Self::Commiserate => 0.2,
            Self::LightenMood => 0.45,
            Self::Rally => 0.55,
            Self::Reframe => 0.65,
            Self::Flirt => 0.85,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SocialAttempt {
    pub action: SocialActionKind,
    pub topic: SocialTopic,
    pub skill_check: f32,
    pub affinity: f32,
    pub familiarity_hours: f32,
    pub diagnosis_correct: bool,
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
    let diagnosis = if attempt.diagnosis_correct {
        0.15
    } else {
        -0.22 - risk * 0.18
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
        revealed_belief: attempt.action == SocialActionKind::Listen,
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
            diagnosis_correct: true,
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
            diagnosis_correct: true,
            sensitivity: 0.0,
            roll: 0.4,
        };
        assert!(resolve_social_attempt(base).succeeded);
        assert!(
            !resolve_social_attempt(SocialAttempt {
                diagnosis_correct: false,
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
}
