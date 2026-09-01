use serde::{Deserialize, Serialize};

use crate::body::BodyPart;

/// A persistent source of blood loss created by tissue trauma. The flow rate
/// is a fraction of normal full blood volume per second.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CombatWound {
    pub body_part: BodyPart,
    pub kind: CombatWoundKind,
    pub blood_fraction_per_second: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombatWoundKind {
    Open,
    Internal,
}

#[must_use]
pub fn advance_combat_bleeding(
    blood_loss_fraction: f32,
    wounds: &[CombatWound],
    seconds: f32,
) -> f32 {
    let flow = wounds
        .iter()
        .map(|wound| wound.blood_fraction_per_second.max(0.0))
        .sum::<f32>();
    (blood_loss_fraction + flow * seconds.max(0.0)).clamp(0.0, 1.0)
}

#[must_use]
pub fn wounds_from_applied_health_damage(
    part: BodyPart,
    applied_cut: f32,
    applied_blunt: f32,
    blunt_energy_joules: f32,
) -> Vec<CombatWound> {
    let blunt_coefficient = match part {
        BodyPart::Head => 0.015,
        BodyPart::Stomach => 0.01,
        BodyPart::Chest => 0.0075,
        BodyPart::LeftArm | BodyPart::RightArm | BodyPart::LeftLeg | BodyPart::RightLeg => 0.005,
    };
    let open_flow = applied_cut.max(0.0) * 0.5 / 60.0;
    let internal_bleeding_threshold_joules = match part {
        BodyPart::Head => 4.0,
        BodyPart::Chest | BodyPart::Stomach => 8.0,
        BodyPart::LeftArm | BodyPart::RightArm | BodyPart::LeftLeg | BodyPart::RightLeg => 6.0,
    };
    let internal_flow = if blunt_energy_joules >= internal_bleeding_threshold_joules {
        applied_blunt.max(0.0) * blunt_coefficient / 60.0
    } else {
        0.0
    };
    [
        (CombatWoundKind::Open, open_flow),
        (CombatWoundKind::Internal, internal_flow),
    ]
    .into_iter()
    .filter_map(|(kind, blood_fraction_per_second)| {
        (blood_fraction_per_second > 0.0).then_some(CombatWound {
            body_part: part,
            kind,
            blood_fraction_per_second,
        })
    })
    .collect()
}

#[must_use]
pub fn acute_trauma_incapacitation(part: BodyPart, applied_damage: f32) -> f32 {
    let vital_scale = match part {
        BodyPart::Head => 1.25,
        BodyPart::Chest => 0.45,
        BodyPart::Stomach => 0.30,
        BodyPart::LeftArm | BodyPart::RightArm | BodyPart::LeftLeg | BodyPart::RightLeg => 0.10,
    };
    applied_damage.max(0.0) * vital_scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_creates_flow_without_instant_blood_loss() {
        let wounds = wounds_from_applied_health_damage(BodyPart::LeftArm, 0.4, 0.0, 0.0);
        assert_eq!(wounds.len(), 1);
        assert_eq!(wounds[0].kind, CombatWoundKind::Open);
        assert_eq!(advance_combat_bleeding(0.0, &wounds, 0.0), 0.0);
        assert!(advance_combat_bleeding(0.0, &wounds, 10.0) > 0.0);
    }

    #[test]
    fn protected_sub_joule_blunt_contact_is_not_an_internal_bleed() {
        assert!(wounds_from_applied_health_damage(BodyPart::Head, 0.0, 0.1, 0.67).is_empty());
    }

    #[test]
    fn identical_wounds_add_flow_over_time() {
        let wound = CombatWound {
            body_part: BodyPart::Chest,
            kind: CombatWoundKind::Open,
            blood_fraction_per_second: 0.001,
        };
        let one = advance_combat_bleeding(0.0, &[wound], 20.0);
        let two = advance_combat_bleeding(0.0, &[wound, wound], 20.0);
        assert!((two - one * 2.0).abs() < f32::EPSILON);
    }
}
