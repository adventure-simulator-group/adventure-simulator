use std::{ops::Add, time::Duration};

use adventuresim_tactical_core::prelude::{BodyPart, melee_interaction_range};
use bevy::prelude::*;

use super::{
    MELEE_RANGE_LATENCY_TOLERANCE, MeleeIntentFacts, MeleeIntentRejection,
    RANGED_RANGE_LATENCY_TOLERANCE, RangedIntentFacts, RangedIntentRejection, TacticalCombatSide,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CombatInstant(Duration);

impl CombatInstant {
    pub(crate) fn from_elapsed(time: &Time<()>) -> Self {
        Self(time.elapsed())
    }

    pub(crate) fn elapsed_since(self, earlier: Self) -> Duration {
        self.0.saturating_sub(earlier.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CombatDuration(Duration);

impl CombatDuration {
    pub(crate) const fn from_duration(duration: Duration) -> Self {
        Self(duration)
    }

    pub(crate) fn as_secs_f32(self) -> f32 {
        self.0.as_secs_f32()
    }
}

impl Add<CombatDuration> for CombatInstant {
    type Output = Self;

    fn add(self, rhs: CombatDuration) -> Self::Output {
        Self(self.0.saturating_add(rhs.0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ReportedPrecision(f32);

impl ReportedPrecision {
    /// Accepts every finite client report exactly as supplied. Precision is a
    /// deliberately trusted animation boundary and is not geometrically
    /// reconstructed or range-clamped by the headless server.
    pub(crate) fn new(value: f32) -> Option<Self> {
        value.is_finite().then_some(Self(value))
    }

    pub(crate) fn get(self) -> f32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ValidatedMeleeAttack {
    attacker: Entity,
    target: Entity,
    body_part: BodyPart,
    reported_precision: ReportedPrecision,
    attacker_position: Vec3,
    target_position: Vec3,
    attacker_yaw: f32,
    target_yaw: f32,
}

impl ValidatedMeleeAttack {
    pub(super) fn attacker(&self) -> Entity {
        self.attacker
    }
    pub(super) fn target(&self) -> Entity {
        self.target
    }
    pub(super) fn attacker_position(&self) -> Vec3 {
        self.attacker_position
    }
    pub(super) fn target_position(&self) -> Vec3 {
        self.target_position
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AuthorizedMeleeAttack(ValidatedMeleeAttack);

impl AuthorizedMeleeAttack {
    pub(super) fn attacker(&self) -> Entity {
        self.0.attacker
    }
    pub(super) fn target(&self) -> Entity {
        self.0.target
    }
    pub(super) fn body_part(&self) -> BodyPart {
        self.0.body_part
    }
    pub(super) fn reported_precision(&self) -> ReportedPrecision {
        self.0.reported_precision
    }
    pub(super) fn attacker_yaw(&self) -> f32 {
        self.0.attacker_yaw
    }
    pub(super) fn target_yaw(&self) -> f32 {
        self.0.target_yaw
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ValidatedRangedImpact {
    Miss,
    Hit {
        target: Entity,
        body_part: BodyPart,
        target_position: Vec3,
        target_yaw: f32,
    },
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ValidatedRangedShot {
    attacker: Entity,
    attacker_side: TacticalCombatSide,
    attacker_position: Vec3,
    attacker_yaw: f32,
    reported_precision: ReportedPrecision,
    impact: ValidatedRangedImpact,
}

impl ValidatedRangedShot {
    pub(super) fn attacker(&self) -> Entity {
        self.attacker
    }
    pub(super) fn attacker_position(&self) -> Vec3 {
        self.attacker_position
    }
    pub(super) fn impact(&self) -> ValidatedRangedImpact {
        self.impact
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AuthorizedRangedShot(ValidatedRangedShot);

impl AuthorizedRangedShot {
    pub(super) fn attacker(&self) -> Entity {
        self.0.attacker
    }
    pub(super) fn attacker_side(&self) -> TacticalCombatSide {
        self.0.attacker_side
    }
    pub(super) fn attacker_yaw(&self) -> f32 {
        self.0.attacker_yaw
    }
    pub(super) fn reported_precision(&self) -> ReportedPrecision {
        self.0.reported_precision
    }
    pub(super) fn impact(&self) -> ValidatedRangedImpact {
        self.0.impact
    }
}

#[derive(Debug)]
struct ObservedMeleeWindup {
    target: Option<Entity>,
    ready_at: CombatInstant,
    expires_at: CombatInstant,
}

#[derive(Component, Debug, Default)]
pub(crate) struct MeleeAttackAuthority {
    windup: Option<ObservedMeleeWindup>,
    cooldown_until: CombatInstant,
}

impl MeleeAttackAuthority {
    pub(crate) fn observe(
        &mut self,
        target: Option<Entity>,
        now: CombatInstant,
        windup: CombatDuration,
        network_allowance: CombatDuration,
    ) {
        let ready_at = now + windup;
        self.windup = Some(ObservedMeleeWindup {
            target,
            ready_at,
            expires_at: ready_at + network_allowance,
        });
    }

    fn authorize(&mut self, target: Entity, now: CombatInstant, cooldown: CombatDuration) -> bool {
        let valid = self.windup.as_ref().is_some_and(|windup| {
            windup.target.map_or(true, |observed| observed == target)
                && now >= windup.ready_at
                && now <= windup.expires_at
                && now >= self.cooldown_until
        });
        if valid {
            self.windup = None;
            self.cooldown_until = now + cooldown;
        }
        valid
    }

    pub(super) fn authorize_attack(
        &mut self,
        attack: ValidatedMeleeAttack,
        now: CombatInstant,
        cooldown: CombatDuration,
    ) -> Option<AuthorizedMeleeAttack> {
        self.authorize(attack.target, now, cooldown)
            .then_some(AuthorizedMeleeAttack(attack))
    }

    pub(crate) fn permits(&self, target: Entity, now: CombatInstant) -> bool {
        self.windup.as_ref().is_some_and(|windup| {
            windup.target.map_or(true, |observed| observed == target)
                && now >= windup.ready_at
                && now <= windup.expires_at
                && now >= self.cooldown_until
        })
    }
}

#[derive(Debug)]
struct ObservedRangedWindup {
    ready_at: CombatInstant,
    expires_at: CombatInstant,
}

#[derive(Component, Debug, Default)]
pub(crate) struct RangedAttackAuthority {
    windup: Option<ObservedRangedWindup>,
    cooldown_until: CombatInstant,
}

impl RangedAttackAuthority {
    pub(crate) fn observe(
        &mut self,
        now: CombatInstant,
        windup: CombatDuration,
        network_allowance: CombatDuration,
    ) {
        let ready_at = now + windup;
        self.windup = Some(ObservedRangedWindup {
            ready_at,
            expires_at: ready_at + network_allowance,
        });
    }

    pub(crate) fn permits(&self, now: CombatInstant) -> bool {
        self.windup.as_ref().is_some_and(|windup| {
            now >= windup.ready_at && now <= windup.expires_at && now >= self.cooldown_until
        })
    }

    fn authorize(&mut self, now: CombatInstant, cooldown: CombatDuration) -> bool {
        let valid = self.permits(now);
        if valid {
            self.windup = None;
            self.cooldown_until = now + cooldown;
        }
        valid
    }

    pub(super) fn authorize_shot(
        &mut self,
        shot: ValidatedRangedShot,
        now: CombatInstant,
        cooldown: CombatDuration,
    ) -> Option<AuthorizedRangedShot> {
        self.authorize(now, cooldown)
            .then_some(AuthorizedRangedShot(shot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: CombatDuration = CombatDuration::from_duration(Duration::from_secs(1));

    #[test]
    fn precision_accepts_all_finite_reports_without_clamping() {
        assert_eq!(ReportedPrecision::new(-1.0).unwrap().get(), -1.0);
        assert_eq!(ReportedPrecision::new(99.0).unwrap().get(), 99.0);
        assert!(ReportedPrecision::new(f32::NAN).is_none());
        assert!(ReportedPrecision::new(f32::INFINITY).is_none());
    }

    #[test]
    fn authority_boundaries_are_inclusive_and_duration_typed() {
        let target = Entity::from_bits(7);
        let start = CombatInstant(Duration::ZERO);
        let mut authority = MeleeAttackAuthority::default();
        authority.observe(Some(target), start, SECOND, SECOND);
        assert!(!authority.permits(target, start));
        assert!(authority.permits(target, start + SECOND));
        assert!(authority.permits(target, start + SECOND + SECOND));
    }
}

pub(super) fn validate_ranged_intent(
    facts: RangedIntentFacts,
) -> Result<ValidatedRangedShot, RangedIntentRejection> {
    if facts.target == Some(facts.attacker) {
        return Err(RangedIntentRejection::SelfTarget);
    }
    let Some(attacker_side) = facts.attacker_side else {
        return Err(RangedIntentRejection::MissingSide);
    };
    let Some(attacker_incapacitated) = facts.attacker_incapacitated else {
        return Err(RangedIntentRejection::MissingCombatState);
    };
    if attacker_incapacitated {
        return Err(RangedIntentRejection::Incapacitated);
    }
    if !facts.weapon_is_ranged || !facts.weapon_range.is_finite() || facts.weapon_range <= 0.0 {
        return Err(RangedIntentRejection::NotRanged);
    }
    if let Some(_) = facts.target {
        let Some(target_side) = facts.target_side else {
            return Err(RangedIntentRejection::MissingSide);
        };
        let Some(target_incapacitated) = facts.target_incapacitated else {
            return Err(RangedIntentRejection::MissingCombatState);
        };
        if attacker_side == target_side {
            return Err(RangedIntentRejection::FriendlyTarget);
        }
        if target_incapacitated {
            return Err(RangedIntentRejection::Incapacitated);
        }
        if !facts.separation.is_some_and(|distance| {
            distance.is_finite() && distance <= facts.weapon_range + RANGED_RANGE_LATENCY_TOLERANCE
        }) {
            return Err(RangedIntentRejection::OutOfRange);
        }
        if facts.target_in_aim_cone != Some(true) {
            return Err(RangedIntentRejection::OutsideAimCone);
        }
    }
    if !facts.authority_permits {
        return Err(RangedIntentRejection::Windup);
    }
    let impact = match facts.target {
        None => ValidatedRangedImpact::Miss,
        Some(target) => ValidatedRangedImpact::Hit {
            target,
            body_part: facts.body_part,
            target_position: facts.target_position.expect("validated target position"),
            target_yaw: facts.target_yaw.expect("validated target yaw"),
        },
    };
    Ok(ValidatedRangedShot {
        attacker: facts.attacker,
        attacker_side,
        attacker_position: facts.attacker_position,
        attacker_yaw: facts.attacker_yaw,
        reported_precision: facts.reported_precision,
        impact,
    })
}

pub(super) fn validate_melee_intent_cheap(
    facts: MeleeIntentFacts,
) -> Result<ValidatedMeleeAttack, MeleeIntentRejection> {
    if facts.attacker == facts.target {
        return Err(MeleeIntentRejection::SelfTarget);
    }
    let (Some(attacker_side), Some(target_side)) = (facts.attacker_side, facts.target_side) else {
        return Err(MeleeIntentRejection::MissingSide);
    };
    if attacker_side == target_side {
        return Err(MeleeIntentRejection::FriendlyTarget);
    }
    let (Some(attacker_incapacitated), Some(target_incapacitated)) =
        (facts.attacker_incapacitated, facts.target_incapacitated)
    else {
        return Err(MeleeIntentRejection::MissingCombatState);
    };
    if attacker_incapacitated || target_incapacitated {
        return Err(MeleeIntentRejection::Incapacitated);
    }
    if !facts.weapon_reach.is_finite() || facts.weapon_reach <= 0.0 {
        return Err(MeleeIntentRejection::Unarmed);
    }
    if !facts.separation.is_finite()
        || facts.separation
            > melee_interaction_range(facts.weapon_reach) + MELEE_RANGE_LATENCY_TOLERANCE
    {
        return Err(MeleeIntentRejection::OutOfRange);
    }
    if !facts.authority_permits {
        return Err(MeleeIntentRejection::Windup);
    }
    Ok(ValidatedMeleeAttack {
        attacker: facts.attacker,
        target: facts.target,
        body_part: facts.body_part,
        reported_precision: facts.reported_precision,
        attacker_position: facts.attacker_position,
        target_position: facts.target_position,
        attacker_yaw: facts.attacker_yaw,
        target_yaw: facts.target_yaw,
    })
}
