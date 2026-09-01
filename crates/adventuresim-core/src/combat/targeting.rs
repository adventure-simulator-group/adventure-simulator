use crate::{
    body::{BodyPart, BodySide, LimbWeights, PlayerBody},
    combat::DefenderResponse,
    equipment::PlayerEquipment,
    prelude::{PlayerAttributes, PlayerEssentials},
    skill::{PlayerSkills, Skill},
};

const BODY_PART_CONTACT_WEIGHTS: [(BodyPart, f32); 7] = [
    (BodyPart::LeftArm, 0.12),
    (BodyPart::RightArm, 0.12),
    (BodyPart::LeftLeg, 0.12),
    (BodyPart::RightLeg, 0.12),
    (BodyPart::Chest, 0.22),
    (BodyPart::Stomach, 0.20),
    (BodyPart::Head, 0.10),
];

/// Front-facing torso surface-area/accessibility partition. Central chest
/// surfaces dominate; vulnerable openings remain named, bounded destinations.
const CHEST_STERNUM_END: f32 = 0.38;
const CHEST_LATERAL_RIBS_END: f32 = 0.72;
const CHEST_LOWER_EDGE_END: f32 = 0.85;
const CHEST_AXILLA_END: f32 = 0.92;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnatomicalSubregion {
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    ChestSternum,
    ChestLateralRibs,
    ChestAxilla,
    ChestNeckline,
    ChestLowerEdge,
    Abdomen,
    Head,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeleeContactLocation {
    pub body_part: BodyPart,
    pub anatomical_subregion: AnatomicalSubregion,
    pub surface_coordinate: f32,
    pub armor_surface: Option<crate::equipment::ArmorSurface>,
}

impl MeleeContactLocation {
    pub const fn new(
        body_part: BodyPart,
        anatomical_subregion: AnatomicalSubregion,
        surface_coordinate: f32,
        armor_surface: Option<crate::equipment::ArmorSurface>,
    ) -> Self {
        Self {
            body_part,
            anatomical_subregion,
            surface_coordinate,
            armor_surface,
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "combat accuracy receives independent attacker and defender facets"
)]
pub fn melee_attack_value_by_parts(
    attacker_skills: &impl PlayerSkills,
    attacker_attr: &impl PlayerAttributes,
    attacker_body: &impl PlayerBody,
    attacker_essentials: &impl PlayerEssentials,
    attacker_equip: &impl PlayerEquipment,
    attacker_side: BodySide,
    attack_style: crate::combat_style::MeleeAttackStyle,
    hit_precision: f32,
    flanking: f32,
    defender_response: DefenderResponse,
    defender_skills: &impl PlayerSkills,
    defender_attr: &impl PlayerAttributes,
    defender_body: &impl PlayerBody,
    defender_essentials: &impl PlayerEssentials,
    defender_equip: &impl PlayerEquipment,
) -> f32 {
    let accuracy = melee_attack_accuracy_by_parts(
        attacker_skills,
        attacker_attr,
        attacker_body,
        attacker_essentials,
        attacker_equip,
        attacker_side,
        attack_style,
        hit_precision,
    );
    let defense = match defender_response {
        DefenderResponse::None => 0.0,
        DefenderResponse::Block { .. } | DefenderResponse::Parry { .. } => {
            let block_skill = defender_skills.skill_check_by_parts(
                Skill::Block,
                defender_attr,
                defender_body,
                defender_essentials,
                defender_equip,
                LimbWeights::all_equal(),
            );
            let shield_bonus = defender_equip.shield_block_bonus();
            5.0 * (1.0 - (-(shield_bonus + block_skill) / 2.0).exp())
        }
        DefenderResponse::Dodge { .. } => {
            let dodge_skill = defender_skills.skill_check_by_parts(
                Skill::Dodge,
                defender_attr,
                defender_body,
                defender_essentials,
                defender_equip,
                LimbWeights {
                    left_arm: 1.0,
                    right_arm: 1.0,
                    left_leg: 0.4,
                    right_leg: 0.4,
                }
                .normalize(),
            );
            dodge_skill
                * defender_equip.armor_penalty(BodyPart::FULL_BODY)
                * defender_equip.encumbrance_penalty_by_parts(defender_attr, defender_body)
        }
    } * defender_response.factor()
        * (1.0 - flanking).clamp(0.0, 1.0);
    accuracy - defense
}

#[expect(
    clippy::too_many_arguments,
    reason = "combat accuracy receives independent character facets"
)]
pub fn melee_attack_accuracy_by_parts(
    attacker_skills: &impl PlayerSkills,
    attacker_attr: &impl PlayerAttributes,
    attacker_body: &impl PlayerBody,
    attacker_essentials: &impl PlayerEssentials,
    attacker_equip: &impl PlayerEquipment,
    attacker_side: BodySide,
    attack_style: crate::combat_style::MeleeAttackStyle,
    hit_precision: f32,
) -> f32 {
    let weights = LimbWeights::arm(attacker_side, attacker_body.primary_side());
    attacker_equip
        .weapon_skill_distribution()
        .weighted_check(|skill| {
            attacker_skills.skill_check_by_parts(
                skill,
                attacker_attr,
                attacker_body,
                attacker_essentials,
                attacker_equip,
                weights,
            )
        })
        * attacker_equip.weapon_melee_precision(attack_style)
        * hit_precision.clamp(0.0, 1.0)
}

#[must_use]
pub fn whole_body_armor_coverage(equipment: &impl PlayerEquipment) -> f32 {
    BODY_PART_CONTACT_WEIGHTS
        .iter()
        .map(|(part, weight)| weight * equipment.armor_coverage(*part).clamp(0.0, 1.0))
        .sum::<f32>()
        .clamp(0.0, 1.0)
}

#[must_use]
pub fn body_part_from_contact_sample(sample: f32) -> BodyPart {
    weighted_body_part_and_local(sample.clamp(0.0, 1.0 - f32::EPSILON), |_| 1.0).0
}

#[must_use]
pub fn melee_contact_location(
    defender: &impl PlayerEquipment,
    contact_sample: f32,
    gap_targeting: f32,
) -> MeleeContactLocation {
    let (body_part, surface_coordinate) = melee_contact_coordinates(contact_sample, gap_targeting);
    let armor_surface = defender.armor_surface(body_part, surface_coordinate);
    MeleeContactLocation {
        body_part,
        anatomical_subregion: anatomical_subregion(body_part, surface_coordinate),
        surface_coordinate,
        armor_surface,
    }
}

fn melee_contact_coordinates(contact_sample: f32, gap_targeting: f32) -> (BodyPart, f32) {
    let (body_part, local_coordinate) =
        weighted_body_part_and_local(contact_sample.clamp(0.0, 1.0 - f32::EPSILON), |_| 1.0);
    let gap_targeting = gap_targeting.clamp(0.0, 1.0);
    let targeted_coordinate =
        local_coordinate + (1.0 - f32::EPSILON - local_coordinate) * gap_targeting;
    (body_part, targeted_coordinate)
}

#[must_use]
pub fn anatomical_subregion(body_part: BodyPart, area_sample: f32) -> AnatomicalSubregion {
    let area_sample = area_sample.clamp(0.0, 1.0 - f32::EPSILON);
    match body_part {
        BodyPart::LeftArm => AnatomicalSubregion::LeftArm,
        BodyPart::RightArm => AnatomicalSubregion::RightArm,
        BodyPart::LeftLeg => AnatomicalSubregion::LeftLeg,
        BodyPart::RightLeg => AnatomicalSubregion::RightLeg,
        BodyPart::Stomach => AnatomicalSubregion::Abdomen,
        BodyPart::Head => AnatomicalSubregion::Head,
        BodyPart::Chest if area_sample < CHEST_STERNUM_END => AnatomicalSubregion::ChestSternum,
        BodyPart::Chest if area_sample < CHEST_LATERAL_RIBS_END => {
            AnatomicalSubregion::ChestLateralRibs
        }
        BodyPart::Chest if area_sample < CHEST_LOWER_EDGE_END => {
            AnatomicalSubregion::ChestLowerEdge
        }
        BodyPart::Chest if area_sample < CHEST_AXILLA_END => AnatomicalSubregion::ChestAxilla,
        BodyPart::Chest => AnatomicalSubregion::ChestNeckline,
    }
}

fn weighted_body_part_and_local(sample: f32, factor: impl Fn(BodyPart) -> f32) -> (BodyPart, f32) {
    let total = BODY_PART_CONTACT_WEIGHTS
        .iter()
        .map(|(part, weight)| weight * factor(*part).max(0.0))
        .sum::<f32>();
    let mut cursor = sample * total;
    for (part, weight) in BODY_PART_CONTACT_WEIGHTS {
        let candidate_weight = weight * factor(part).max(0.0);
        if candidate_weight <= f32::EPSILON {
            continue;
        }
        if cursor < candidate_weight {
            return (
                part,
                (cursor / candidate_weight).clamp(0.0, 1.0 - f32::EPSILON),
            );
        }
        cursor -= candidate_weight;
    }
    (BodyPart::Head, 1.0 - f32::EPSILON)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn torso_subregions_follow_named_area_and_accessibility_weights() {
        let samples = (0..10_000).map(|index| (index as f32 + 0.5) / 10_000.0);
        let mut counts = [0_usize; 5];
        for sample in samples {
            let index = match anatomical_subregion(BodyPart::Chest, sample) {
                AnatomicalSubregion::ChestSternum => 0,
                AnatomicalSubregion::ChestLateralRibs => 1,
                AnatomicalSubregion::ChestLowerEdge => 2,
                AnatomicalSubregion::ChestAxilla => 3,
                AnatomicalSubregion::ChestNeckline => 4,
                region => panic!("unexpected chest region {region:?}"),
            };
            counts[index] += 1;
        }
        assert_eq!(counts, [3_800, 3_400, 1_300, 700, 800]);
        assert!(counts[0] + counts[1] > counts[2] + counts[3] + counts[4]);
    }

    #[test]
    fn breastplate_sized_surface_cannot_turn_into_a_neckline_contact() {
        for index in 0..8_500 {
            let sample = (index as f32 + 0.5) / 10_000.0;
            assert_ne!(
                anatomical_subregion(BodyPart::Chest, sample),
                AnatomicalSubregion::ChestNeckline
            );
        }
    }

    #[test]
    fn chest_local_area_coordinate_is_independent_of_global_body_part_interval() {
        let chest_start = 0.48;
        let chest_weight = 0.22;
        let mut counts = [0_usize; 5];
        for index in 0..10_000 {
            let global_sample = chest_start + chest_weight * (index as f32 + 0.5) / 10_000.0;
            let (part, local_sample) = weighted_body_part_and_local(global_sample, |_| 1.0);
            assert_eq!(part, BodyPart::Chest);
            let slot = match anatomical_subregion(part, local_sample) {
                AnatomicalSubregion::ChestSternum => 0,
                AnatomicalSubregion::ChestLateralRibs => 1,
                AnatomicalSubregion::ChestLowerEdge => 2,
                AnatomicalSubregion::ChestAxilla => 3,
                AnatomicalSubregion::ChestNeckline => 4,
                region => panic!("unexpected region {region:?}"),
            };
            counts[slot] += 1;
        }
        assert_eq!(counts, [3_800, 3_400, 1_300, 700, 800]);
    }

    #[test]
    fn one_surface_coordinate_keeps_chest_anatomy_and_plate_intersection_coherent() {
        let mut sternum_gaps = 0;
        let mut opening_surfaces = 0;
        for index in 0..10_000 {
            let sample = 0.48 + 0.22 * (index as f32 + 0.5) / 10_000.0;
            let (part, coordinate) = melee_contact_coordinates(sample, 0.0);
            assert_eq!(part, BodyPart::Chest);
            let region = anatomical_subregion(part, coordinate);
            let plate_surface = coordinate < 0.85;
            if region == AnatomicalSubregion::ChestSternum && !plate_surface {
                sternum_gaps += 1;
            }
            if matches!(
                region,
                AnatomicalSubregion::ChestAxilla | AnatomicalSubregion::ChestNeckline
            ) && plate_surface
            {
                opening_surfaces += 1;
            }
        }
        assert_eq!(sternum_gaps, 0);
        assert_eq!(opening_surfaces, 0);
    }

    #[test]
    fn precision_moves_the_contact_toward_named_openings() {
        let chest_sample = 0.48 + 0.22 * 0.2;
        let (ordinary_part, ordinary_coordinate) = melee_contact_coordinates(chest_sample, 0.0);
        let (precise_part, precise_coordinate) = melee_contact_coordinates(chest_sample, 0.9);
        assert_eq!(ordinary_part, precise_part);
        assert!(precise_coordinate > ordinary_coordinate);
        assert_eq!(
            anatomical_subregion(ordinary_part, ordinary_coordinate),
            AnatomicalSubregion::ChestSternum
        );
        assert_eq!(
            anatomical_subregion(precise_part, precise_coordinate),
            AnatomicalSubregion::ChestAxilla
        );
    }

    #[test]
    fn symmetric_arm_coordinates_have_identical_surface_intersections() {
        for index in 0..10_000 {
            let local = (index as f32 + 0.5) / 10_000.0;
            let (left, left_coordinate) = melee_contact_coordinates(0.12 * local, 0.35);
            let (right, right_coordinate) = melee_contact_coordinates(0.12 + 0.12 * local, 0.35);
            assert_eq!(left, BodyPart::LeftArm);
            assert_eq!(right, BodyPart::RightArm);
            assert!((left_coordinate - right_coordinate).abs() < 1.0e-5);
            assert_eq!(left_coordinate < 0.85, right_coordinate < 0.85);
        }
    }
}
