use crate::{
    body::{BodyPart, BodySide, PlayerBody},
    equipment::PlayerEquipment,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeleeAttackCapability {
    Available,
    DisabledWeaponArm { arm: BodyPart },
    NoStrikingSide,
}

impl MeleeAttackCapability {
    #[must_use]
    pub const fn is_available(self) -> bool {
        matches!(self, Self::Available)
    }
}

/// Determines whether the selected held weapon can be accelerated by its
/// controlling arm. Off-hand transfer is a distinct authored action and is
/// therefore not inferred here.
#[must_use]
pub fn melee_attack_capability(
    body: &impl PlayerBody,
    equipment: &impl PlayerEquipment,
) -> MeleeAttackCapability {
    let Some(side) = equipment.weapon_holding_side() else {
        return MeleeAttackCapability::NoStrikingSide;
    };
    let arms: &[BodyPart] = match side {
        BodySide::Left => &[BodyPart::LeftArm],
        BodySide::Right => &[BodyPart::RightArm],
        BodySide::Both => &[BodyPart::LeftArm, BodyPart::RightArm],
    };
    if let Some(arm) = arms
        .iter()
        .copied()
        .find(|arm| body.body_part_health(*arm) <= f32::EPSILON)
    {
        MeleeAttackCapability::DisabledWeaponArm { arm }
    } else {
        MeleeAttackCapability::Available
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoresolve::{CombatBody, CombatEquipment, CombatWeapon, body_part_index};

    #[test]
    fn militia_and_demi_profiles_cannot_attack_with_disabled_weapon_arm() {
        for profile in ["militia", "demi_lancer"] {
            let mut body = CombatBody::default();
            body.health[body_part_index(BodyPart::RightArm)] = 0.0;
            let equipment = CombatEquipment {
                weapon: Some(CombatWeapon::default()),
                melee_weapon: Some(CombatWeapon::default()),
                holding_side: BodySide::Right,
                ..CombatEquipment::default()
            };
            assert_eq!(
                melee_attack_capability(&body, &equipment),
                MeleeAttackCapability::DisabledWeaponArm {
                    arm: BodyPart::RightArm
                },
                "{profile}"
            );
        }
    }
}
