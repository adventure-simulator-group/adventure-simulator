use super::{AuthorizedRangedShot, CombatDuration, CombatInstant, ValidatedRangedShot};
use bevy::prelude::Component;

#[derive(Debug, Clone)]
struct ObservedRangedWindup {
    ready_at: CombatInstant,
    expires_at: CombatInstant,
}

#[derive(Component, Debug, Default, Clone)]
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

    pub(in crate::combat) fn authorize_shot(
        &mut self,
        shot: ValidatedRangedShot,
        now: CombatInstant,
        cooldown: CombatDuration,
    ) -> Option<AuthorizedRangedShot> {
        self.authorize(now, cooldown)
            .then_some(AuthorizedRangedShot(shot))
    }
}
