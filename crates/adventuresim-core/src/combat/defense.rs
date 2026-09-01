use super::{
    DODGE_OVEREXTENSION_SCALE, DefenderResponse, MeleeContactLocation, WEAPON_DEFENSE_REBOUND_SCALE,
};
use crate::body::{BodyPart, BodySide};

/// A one kilogram-square-metre implement is the boundary between ordinary
/// one-handed rotational commitment and high-inertia pole/impact weapons.
const REFERENCE_HAND_WEAPON_INERTIA_KG_M2: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommittedThreatChoice {
    FinishTrade,
    WeaponIntercept,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CommittedThreatFacts {
    /// How long after the incoming contact the defender's committed weapon
    /// would contact. Non-positive values mean the defender lands first or in
    /// the same simultaneous contact batch.
    pub own_contact_after_incoming_seconds: f32,
    pub own_windup_seconds: f32,
    /// Expected geometric engagement of the attempted intercept, after skill
    /// and available reaction time, on the same zero-to-one scale used by the
    /// weapon-alignment resolver.
    pub expected_intercept_engagement: f32,
    /// Current medical/structural vulnerability to another contact.
    pub incapacitation: f32,
    pub weapon_moment_of_inertia_kg_m2: f32,
    pub weapon_recovery_seconds: f32,
    /// Number of immediately preceding committed attacks consumed by defense.
    /// This is short-term observation of a repeated phase relationship, not a
    /// permanent combatant trait.
    pub consecutive_intercepts: u8,
    /// Seeded behavioral variation. It selects against the continuous ratio
    /// of sunk attack work to expected defensive benefit; it is not an outcome
    /// sample or foreknowledge of the incoming contact.
    pub decision_sample: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CommittedThreatDecision {
    pub choice: CommittedThreatChoice,
    pub finish_trade_probability: f32,
    pub completed_work_fraction: f32,
    pub expected_intercept_benefit: f32,
}

/// Chooses whether an already-committed strike is redirected into a weapon
/// intercept or allowed to finish as a trade.
///
/// Both alternatives are compared as dimensionless fractions of the current
/// action. Near-contact work and rotational inertia raise the cost of
/// redirecting the weapon. Intercept alignment and current vulnerability raise
/// the benefit of defense, while long recovery raises the exposure of trading.
/// Repeatedly observed interceptions add the completed work that the fighter
/// has learned will otherwise be discarded, breaking a fixed phase cycle.
#[must_use]
pub fn choose_committed_threat_response(facts: CommittedThreatFacts) -> CommittedThreatDecision {
    let windup = facts.own_windup_seconds.max(f32::EPSILON);
    let contact_lag = facts.own_contact_after_incoming_seconds.max(0.0);
    let completed_work_fraction = (1.0 - contact_lag / windup).clamp(0.0, 1.0);
    if facts.own_contact_after_incoming_seconds <= 0.0 {
        return CommittedThreatDecision {
            choice: CommittedThreatChoice::FinishTrade,
            finish_trade_probability: 1.0,
            completed_work_fraction,
            expected_intercept_benefit: 0.0,
        };
    }

    let inertia_commitment = facts.weapon_moment_of_inertia_kg_m2.max(0.0)
        / (facts.weapon_moment_of_inertia_kg_m2.max(0.0) + REFERENCE_HAND_WEAPON_INERTIA_KG_M2);
    let learned_phase_commitment = completed_work_fraction
        * f32::from(facts.consecutive_intercepts)
        / (f32::from(facts.consecutive_intercepts) + 1.0);
    let sunk_attack_work =
        completed_work_fraction * (1.0 + inertia_commitment) + learned_phase_commitment;
    let recovery_exposure =
        facts.weapon_recovery_seconds.max(0.0) / (facts.weapon_recovery_seconds.max(0.0) + windup);
    let contact_vulnerability = 1.0 + facts.incapacitation.clamp(0.0, 1.0) + recovery_exposure;
    let expected_intercept_benefit =
        facts.expected_intercept_engagement.clamp(0.0, 1.0) * contact_vulnerability;
    let total = sunk_attack_work + expected_intercept_benefit;
    let finish_trade_probability = if total <= f32::EPSILON {
        0.0
    } else {
        sunk_attack_work / total
    };
    CommittedThreatDecision {
        choice: if facts.decision_sample.clamp(0.0, 1.0) < finish_trade_probability {
            CommittedThreatChoice::FinishTrade
        } else {
            CommittedThreatChoice::WeaponIntercept
        },
        finish_trade_probability,
        completed_work_fraction,
        expected_intercept_benefit,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponDefenseAlignment {
    pub attempted: DefenderResponse,
    pub effective: DefenderResponse,
    pub success_probability: f32,
    pub alignment_sample: f32,
    /// Continuous fraction of the attempted intercept that actually engages
    /// the incoming weapon. This is distinct from the work/posture cost of
    /// attempting the defense with the defender's own implement.
    pub engagement: f32,
}

/// Resolves the unavoidable line and timing variation of a weapon intercept.
/// The attack/defense margin is measured on the shared practical skill scale;
/// one scale point is the characteristic width of the logistic alignment
/// distribution. Timing, leverage, fatigue, and skill have already shaped
/// both the response and `attack_value` before this final physical alignment.
#[must_use]
pub fn resolve_weapon_defense_alignment(
    attempted: DefenderResponse,
    attack_value: f32,
    alignment_sample: f32,
) -> WeaponDefenseAlignment {
    if !attempted.is_weapon_contact() {
        return WeaponDefenseAlignment {
            attempted,
            effective: attempted,
            success_probability: 1.0,
            alignment_sample,
            engagement: 1.0,
        };
    }
    let success_probability = 1.0 / (1.0 + attack_value.exp());
    let sample = alignment_sample.clamp(f32::EPSILON, 1.0 - f32::EPSILON);
    // A uniform sample transformed by the inverse logistic CDF is a physical
    // alignment error on the same practical-skill scale as `attack_value`.
    // The remaining intercept margin then produces a continuous engagement:
    // zero is a clean failure, one is a deeply aligned bind, and the nominal
    // success boundary is half engagement.
    let alignment_error = (sample / (1.0 - sample)).ln();
    let intercept_margin = -attack_value - alignment_error;
    let engagement = 1.0 / (1.0 + (-intercept_margin).exp());
    let effective = attempted.scaled_for_alignment(engagement);
    WeaponDefenseAlignment {
        attempted,
        effective,
        success_probability,
        alignment_sample,
        engagement,
    }
}

/// Applies the frontal-plane geometry of a hand-held shield to a block choice.
///
/// Coordinates are normalized to half body width. A buckler or shield can move
/// about 0.45 body half-widths from its resting hand and covers about another
/// 0.35. Contacts beyond that combined traverse cannot engage the implement;
/// contacts near the edge retain less alignment and therefore less leverage.
#[must_use]
pub fn shield_aligned_response(
    response: DefenderResponse,
    shield_side: Option<BodySide>,
    contact: MeleeContactLocation,
) -> DefenderResponse {
    let DefenderResponse::Block { effectiveness } = response else {
        return response;
    };
    let Some(resting_coordinate) = shield_side.and_then(shield_resting_coordinate) else {
        return response;
    };
    const SHIELD_HALF_WIDTH: f32 = 0.35;
    const ARM_TRAVERSE: f32 = 0.45;
    const MAX_ENGAGEMENT_DISTANCE: f32 = SHIELD_HALF_WIDTH + ARM_TRAVERSE;
    let distance = (contact_lateral_coordinate(contact) - resting_coordinate).abs();
    if distance >= MAX_ENGAGEMENT_DISTANCE {
        return DefenderResponse::None;
    }
    DefenderResponse::Block {
        effectiveness: effectiveness * (1.0 - distance / MAX_ENGAGEMENT_DISTANCE),
    }
}

#[must_use]
pub fn reciprocal_intercept_response(
    input_reflex: f32,
    precision: f32,
    shield_block_bonus: f32,
) -> DefenderResponse {
    if shield_block_bonus > 0.0 {
        let shield_leverage = shield_block_bonus / (shield_block_bonus + 0.5);
        DefenderResponse::Block {
            effectiveness: ((0.5 + input_reflex * 0.5) * shield_leverage).clamp(0.0, 1.0),
        }
    } else {
        DefenderResponse::Parry {
            input_reflex,
            precision,
        }
    }
}

fn shield_resting_coordinate(side: BodySide) -> Option<f32> {
    match side {
        BodySide::Left => Some(-0.55),
        BodySide::Right => Some(0.55),
        BodySide::Both => None,
    }
}

fn contact_lateral_coordinate(contact: MeleeContactLocation) -> f32 {
    let local = contact.surface_coordinate.clamp(0.0, 1.0);
    match contact.body_part {
        BodyPart::LeftArm => -1.0 + 0.2 * local,
        BodyPart::RightArm => 0.8 + 0.2 * local,
        BodyPart::LeftLeg => -0.5 + 0.2 * local,
        BodyPart::RightLeg => 0.3 + 0.2 * local,
        BodyPart::Chest | BodyPart::Stomach => (local - 0.5) * 1.1,
        BodyPart::Head => (local - 0.5) * 0.7,
    }
}

impl DefenderResponse {
    fn scaled_for_alignment(self, engagement: f32) -> Self {
        let engagement = engagement.clamp(0.0, 1.0);
        match self {
            Self::None => Self::None,
            Self::Block { effectiveness } => Self::Block {
                effectiveness: effectiveness * engagement,
            },
            Self::Parry {
                input_reflex,
                precision,
            } => Self::Parry {
                input_reflex,
                precision: precision * engagement,
            },
            Self::Dodge { input_reflex } => Self::Dodge { input_reflex },
        }
    }

    pub fn scaled_for_performance(self, performance: f32) -> Self {
        let performance = performance.clamp(0.0, 1.0);
        match self {
            Self::None => Self::None,
            Self::Block { effectiveness } => Self::Block {
                effectiveness: effectiveness * performance,
            },
            Self::Parry {
                input_reflex,
                precision,
            } => Self::Parry {
                input_reflex,
                precision: precision * performance,
            },
            Self::Dodge { input_reflex } => Self::Dodge {
                input_reflex: input_reflex * performance,
            },
        }
    }

    pub fn factor(&self) -> f32 {
        match self {
            Self::None => 1.0,
            &Self::Block { effectiveness } => effectiveness.clamp(0.0, 1.0),
            &Self::Parry {
                input_reflex,
                precision,
            } => 2.0 * input_reflex * precision.clamp(0.0, 1.0),
            &Self::Dodge { input_reflex } => 1.5 * input_reflex,
        }
    }

    pub fn is_weapon_contact(self) -> bool {
        matches!(self, Self::Block { .. } | Self::Parry { .. })
    }

    pub(super) fn rebound_scale(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Block { .. } | Self::Parry { .. } => WEAPON_DEFENSE_REBOUND_SCALE,
            Self::Dodge { .. } => DODGE_OVEREXTENSION_SCALE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::anatomical_subregion;

    fn contact(part: BodyPart, surface_coordinate: f32) -> MeleeContactLocation {
        MeleeContactLocation::new(
            part,
            anatomical_subregion(part, surface_coordinate),
            surface_coordinate,
            None,
        )
    }

    #[test]
    fn shield_alignment_is_lateral_and_mirror_symmetric() {
        let block = DefenderResponse::Block { effectiveness: 0.8 };
        let left =
            shield_aligned_response(block, Some(BodySide::Left), contact(BodyPart::LeftArm, 0.5));
        let right = shield_aligned_response(
            block,
            Some(BodySide::Right),
            contact(BodyPart::RightArm, 0.5),
        );
        let (
            DefenderResponse::Block {
                effectiveness: left,
            },
            DefenderResponse::Block {
                effectiveness: right,
            },
        ) = (left, right)
        else {
            panic!("mirrored shield contacts should both engage");
        };
        assert!((left - right).abs() < 1.0e-6);
        assert_eq!(
            shield_aligned_response(
                block,
                Some(BodySide::Left),
                contact(BodyPart::RightArm, 0.5),
            ),
            DefenderResponse::None
        );
    }

    #[test]
    fn central_contact_can_be_caught_but_loses_edge_alignment() {
        let block = DefenderResponse::Block { effectiveness: 1.0 };
        let DefenderResponse::Block { effectiveness } =
            shield_aligned_response(block, Some(BodySide::Left), contact(BodyPart::Chest, 0.5))
        else {
            panic!("central chest should remain inside shield traverse");
        };
        assert!(effectiveness > 0.0 && effectiveness < 1.0);
    }

    #[test]
    fn weapon_defense_alignment_is_continuous_without_becoming_an_outcome_oracle() {
        let parry = DefenderResponse::Parry {
            input_reflex: 0.8,
            precision: 0.8,
        };
        let favorable = resolve_weapon_defense_alignment(parry, -1.0, 0.1);
        let difficult = resolve_weapon_defense_alignment(parry, 1.0, 0.9);
        assert!(favorable.success_probability > difficult.success_probability);
        assert!(favorable.engagement > 0.5);
        assert!(difficult.engagement < 0.5);

        let credible_failure = resolve_weapon_defense_alignment(parry, -1.0, 0.9);
        assert_eq!(credible_failure.attempted, parry);
        assert!(credible_failure.engagement < 0.5);
    }

    #[test]
    fn alignment_margin_is_monotonic_across_physical_inputs() {
        let parry = DefenderResponse::Parry {
            input_reflex: 0.8,
            precision: 0.8,
        };
        let sample = 0.61;
        let strong_defender = resolve_weapon_defense_alignment(parry, -2.0, sample);
        let even_exchange = resolve_weapon_defense_alignment(parry, 0.0, sample);
        let strong_attacker = resolve_weapon_defense_alignment(parry, 2.0, sample);
        assert!(strong_defender.engagement > even_exchange.engagement);
        assert!(even_exchange.engagement > strong_attacker.engagement);

        // Timing, leverage, fatigue, and skill all enter through this signed
        // margin, so moving the combined margin by one practical-skill point
        // has the same monotonic effect regardless of its source.
        let timing_or_fatigue_penalty = resolve_weapon_defense_alignment(parry, 1.0, sample);
        assert!(timing_or_fatigue_penalty.engagement < even_exchange.engagement);
    }

    fn threat_facts(contact_lag: f32, sample: f32) -> CommittedThreatFacts {
        CommittedThreatFacts {
            own_contact_after_incoming_seconds: contact_lag,
            own_windup_seconds: 0.65,
            expected_intercept_engagement: 0.7,
            incapacitation: 0.0,
            weapon_moment_of_inertia_kg_m2: 0.4,
            weapon_recovery_seconds: 0.5,
            consecutive_intercepts: 0,
            decision_sample: sample,
        }
    }

    #[test]
    fn near_complete_committed_strike_is_more_likely_to_finish_than_early_windup() {
        let near = choose_committed_threat_response(threat_facts(0.05, 0.5));
        let early = choose_committed_threat_response(threat_facts(0.55, 0.5));
        assert!(near.finish_trade_probability > early.finish_trade_probability);
        assert_eq!(near.choice, CommittedThreatChoice::FinishTrade);
        assert_eq!(early.choice, CommittedThreatChoice::WeaponIntercept);
    }

    #[test]
    fn first_or_simultaneous_contact_finishes_without_erasing_either_attack() {
        for contact_lag in [-0.1, 0.0] {
            let decision = choose_committed_threat_response(threat_facts(contact_lag, 1.0));
            assert_eq!(decision.choice, CommittedThreatChoice::FinishTrade);
            assert_eq!(decision.finish_trade_probability, 1.0);
        }
    }

    #[test]
    fn injury_and_intercept_geometry_monotonically_favor_defense() {
        let baseline = choose_committed_threat_response(threat_facts(0.2, 0.5));
        let mut safer_intercept = threat_facts(0.2, 0.5);
        safer_intercept.expected_intercept_engagement = 1.0;
        safer_intercept.incapacitation = 0.8;
        let safer_intercept = choose_committed_threat_response(safer_intercept);
        assert!(safer_intercept.finish_trade_probability < baseline.finish_trade_probability);
    }

    #[test]
    fn repeated_phase_intercepts_shift_a_borderline_choice_toward_finishing() {
        let first = choose_committed_threat_response(threat_facts(0.2, 0.52));
        let mut adapted = threat_facts(0.2, 0.52);
        adapted.consecutive_intercepts = 2;
        let adapted = choose_committed_threat_response(adapted);
        assert!(adapted.finish_trade_probability > first.finish_trade_probability);
        assert_eq!(first.choice, CommittedThreatChoice::WeaponIntercept);
        assert_eq!(adapted.choice, CommittedThreatChoice::FinishTrade);
    }
}
