use crate::body::BodyPart;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeleeDodgeGeometry {
    pub closest_approach_metres: f32,
    pub contacted_body_part: Option<BodyPart>,
}

/// Physical opportunity available to a defender and the tracking still
/// available to the committed attack. Masses are kilograms, time is seconds,
/// reach is metres, and arc is radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeleeDodgeKinematics {
    pub defender_leg_agility: f32,
    pub defender_fatigue_performance: f32,
    pub defender_body_mass_kg: f32,
    pub defender_equipment_mass_kg: f32,
    pub displacement_time_seconds: f32,
    pub attacker_tracking: f32,
    pub weapon_reach_metres: f32,
    pub committed_arc_radians: f32,
}

impl MeleeDodgeKinematics {
    fn untracked_lateral_displacement(self, measured_displacement: f32) -> f32 {
        let total_mass = (self.defender_body_mass_kg + self.defender_equipment_mass_kg).max(1.0);
        let load_fraction = self.defender_body_mass_kg.max(1.0) / total_mass;
        let mobility = (self.defender_leg_agility.max(0.0) / 3.0)
            * self.defender_fatigue_performance.clamp(0.0, 1.0)
            * load_fraction;
        let time = self.displacement_time_seconds.max(0.0);
        // A planted lateral step begins from rest. Seven metres per second
        // squared is the reference whole-body acceleration for agility 3.
        let achievable = 0.5 * 7.0 * mobility * time * time;
        let displaced = measured_displacement.min(achievable);
        // The attacker can rotate a still-committed path through part of its
        // remaining arc. Tracking capacity grows linearly with time while the
        // defender's displacement grows quadratically from the planted step.
        let reference_tracking_rate_radians_per_second = 2.0;
        let trackable = self.weapon_reach_metres.max(0.0)
            * self.committed_arc_radians.max(0.0)
            * self.attacker_tracking.clamp(0.0, 1.0)
            * reference_tracking_rate_radians_per_second
            * time;
        // Tracking is allowed to consume the entire displacement. The former
        // ninety-percent cap guaranteed residual clearance even when a fast,
        // still-adjustable weapon path could physically follow the step.
        (displaced - trackable).max(0.0)
    }
}

/// Tests the displaced defender against the attack ray aimed at their
/// pre-dodge location. Coordinates are horizontal metres in any consistent
/// basis.
#[must_use]
pub fn resolve_melee_dodge_geometry(
    attack_origin: (f32, f32),
    intended_target: (f32, f32),
    displaced_target: (f32, f32),
    intended_body_part: BodyPart,
    kinematics: MeleeDodgeKinematics,
) -> MeleeDodgeGeometry {
    let attack = (
        intended_target.0 - attack_origin.0,
        intended_target.1 - attack_origin.1,
    );
    let length = (attack.0 * attack.0 + attack.1 * attack.1).sqrt();
    let direction = if length > f32::EPSILON {
        (attack.0 / length, attack.1 / length)
    } else {
        (1.0, 0.0)
    };
    let displaced = (
        displaced_target.0 - attack_origin.0,
        displaced_target.1 - attack_origin.1,
    );
    let signed_lateral = direction.0 * displaced.1 - direction.1 * displaced.0;
    let closest = kinematics.untracked_lateral_displacement(signed_lateral.abs());
    let intended_region_radius = match intended_body_part {
        BodyPart::Chest | BodyPart::Stomach => 0.30,
        BodyPart::Head => 0.16,
        BodyPart::LeftArm | BodyPart::RightArm => 0.18,
        BodyPart::LeftLeg | BodyPart::RightLeg => 0.20,
    };
    // The attack can miss its named destination yet still cross the displaced
    // body's adjacent silhouette. Torso/head lines include the trailing arm;
    // limb lines include the torso or pelvis behind that limb. These are body
    // envelopes, not extra defense rolls.
    let swept_body_radius = match intended_body_part {
        BodyPart::Chest | BodyPart::Stomach | BodyPart::Head => 0.48,
        BodyPart::LeftArm | BodyPart::RightArm => 0.32,
        BodyPart::LeftLeg | BodyPart::RightLeg => 0.28,
    };
    let contacted_body_part = if closest > swept_body_radius {
        None
    } else if closest > intended_region_radius {
        Some(match intended_body_part {
            BodyPart::Chest | BodyPart::Stomach | BodyPart::Head => {
                if signed_lateral >= 0.0 {
                    BodyPart::RightArm
                } else {
                    BodyPart::LeftArm
                }
            }
            BodyPart::LeftArm | BodyPart::RightArm => BodyPart::Chest,
            BodyPart::LeftLeg | BodyPart::RightLeg => BodyPart::Stomach,
        })
    } else {
        Some(intended_body_part)
    };
    MeleeDodgeGeometry {
        closest_approach_metres: closest,
        contacted_body_part,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirects_a_graze_and_allows_a_clean_miss() {
        let kinematics = MeleeDodgeKinematics {
            defender_leg_agility: 3.0,
            defender_fatigue_performance: 1.0,
            defender_body_mass_kg: 75.0,
            defender_equipment_mass_kg: 5.0,
            displacement_time_seconds: 0.4,
            attacker_tracking: 0.1,
            weapon_reach_metres: 0.8,
            committed_arc_radians: 0.5,
        };
        let graze = resolve_melee_dodge_geometry(
            (0.0, 0.0),
            (1.0, 0.0),
            (1.0, 0.42),
            BodyPart::Chest,
            kinematics,
        );
        assert_eq!(graze.contacted_body_part, Some(BodyPart::RightArm));
        let miss = resolve_melee_dodge_geometry(
            (0.0, 0.0),
            (1.0, 0.0),
            (1.0, 0.6),
            BodyPart::Chest,
            kinematics,
        );
        assert_eq!(miss.contacted_body_part, None);
    }

    #[test]
    fn load_fatigue_and_late_displacement_constrain_dodge_clearance() {
        let base = MeleeDodgeKinematics {
            defender_leg_agility: 4.0,
            defender_fatigue_performance: 1.0,
            defender_body_mass_kg: 75.0,
            defender_equipment_mass_kg: 5.0,
            displacement_time_seconds: 0.35,
            attacker_tracking: 0.35,
            weapon_reach_metres: 1.0,
            committed_arc_radians: 0.6,
        };
        let clearance = |kinematics| {
            resolve_melee_dodge_geometry(
                (0.0, 0.0),
                (1.0, 0.0),
                (1.0, 0.8),
                BodyPart::Chest,
                kinematics,
            )
            .closest_approach_metres
        };
        assert!(
            clearance(base)
                > clearance(MeleeDodgeKinematics {
                    defender_equipment_mass_kg: 45.0,
                    ..base
                })
        );
        assert!(
            clearance(base)
                > clearance(MeleeDodgeKinematics {
                    defender_fatigue_performance: 0.5,
                    ..base
                })
        );
        assert!(
            clearance(base)
                > clearance(MeleeDodgeKinematics {
                    displacement_time_seconds: 0.15,
                    ..base
                })
        );
        assert!(
            clearance(base)
                > clearance(MeleeDodgeKinematics {
                    attacker_tracking: 0.8,
                    ..base
                })
        );
    }

    #[test]
    fn sufficient_remaining_tracking_can_follow_the_whole_step() {
        let geometry = resolve_melee_dodge_geometry(
            (0.0, 0.0),
            (1.0, 0.0),
            (1.0, 0.35),
            BodyPart::Chest,
            MeleeDodgeKinematics {
                defender_leg_agility: 3.0,
                defender_fatigue_performance: 1.0,
                defender_body_mass_kg: 75.0,
                defender_equipment_mass_kg: 5.0,
                displacement_time_seconds: 0.4,
                attacker_tracking: 1.0,
                weapon_reach_metres: 1.0,
                committed_arc_radians: 0.8,
            },
        );
        assert_eq!(geometry.closest_approach_metres, 0.0);
        assert_eq!(geometry.contacted_body_part, Some(BodyPart::Chest));
    }
}
