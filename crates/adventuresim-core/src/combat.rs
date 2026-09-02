use std::{f32, ops::Mul};

use serde::{Deserialize, Serialize};

mod armor;
mod capability;
mod config;
mod contact_geometry;
mod defense;
mod fatigue;
mod fatigue_config;
mod melee_resolution;
pub(crate) mod targeting;
mod wounds;

pub use armor::{
    ArmorCoverageSpan, ArmorImpact, ArmorImpactOutcome, AuthoredArmorCoverage,
    authored_armor_coverage, authored_armor_coverage_span, layered_armor_contact_index,
};
pub use capability::{MeleeAttackCapability, melee_attack_capability};
pub use config::*;
pub use contact_geometry::{
    HUMANOID_COLLISION_RADIUS_METRES, HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES,
    HUMANOID_REFERENCE_ARM_REACH_METRES, MeleeContactAtTime, MeleeContactAtTimeFacts,
    MeleeContactClassification, MeleeContactInvalidationCause, has_distal_striking_surface,
    preferred_melee_striking_measure, resolve_melee_contact_at_time,
};
pub use defense::{
    CommittedThreatChoice, CommittedThreatDecision, CommittedThreatFacts, WeaponDefenseAlignment,
    choose_committed_threat_response, reciprocal_intercept_response,
    resolve_weapon_defense_alignment, shield_aligned_response,
};
pub use fatigue::*;
pub use fatigue_config::CombatFatigueParameters;
pub use melee_resolution::resolve_melee_attack_by_parts;
pub use targeting::{
    AnatomicalSubregion, MeleeContactLocation, anatomical_subregion,
    melee_attack_accuracy_by_parts, melee_attack_value_by_parts, melee_contact_location,
    melee_measure_adjusted_precision, whole_body_armor_coverage,
};
pub use wounds::*;

use crate::{
    body::{BodyPart, BodySide, LimbWeights, PlayerBody},
    morale::{blood_loss_incapacitation, pain_incapacitation},
    prelude::{LimbAttribute, PlayerAttributes, PlayerEquipment, PlayerEssentials},
    skill::{PlayerSkills, Skill},
};

/// Empty-hand accuracy multipliers shared by tactical inventory and pure
/// matchup/autoresolve equipment.
pub const UNARMED_SWING_PRECISION: f32 = 0.2;
pub const UNARMED_STAB_PRECISION: f32 = 0.5;
/// Fraction of maximum balance recovered per second while combat continues.
pub const IMBALANCE_RECOVERY_PER_SECOND: f32 = 0.25;

#[must_use]
pub fn apply_clamped_limb_damage(health: &mut f32, damage: f32) -> f32 {
    let applied = damage.max(0.0).min(health.max(0.0));
    *health = (*health - applied).max(0.0);
    applied
}

#[must_use]
pub fn recover_combat_imbalance(imbalance: f32, seconds: f32) -> f32 {
    (imbalance - IMBALANCE_RECOVERY_PER_SECOND * seconds.max(0.0)).max(0.0)
}

#[must_use]
pub fn combat_incapacitation(
    starting_incapacitation: f32,
    starting_blood_fraction: f32,
    blood_loss_fraction: f32,
    total_limb_damage: f32,
    will_check: f32,
    imbalance: f32,
) -> f32 {
    let pain = pain_incapacitation(total_limb_damage, will_check);
    let remaining_blood = (starting_blood_fraction - blood_loss_fraction).clamp(0.0, 1.0);
    starting_incapacitation.max(0.0)
        + pain
        + blood_loss_incapacitation(remaining_blood, 1.0)
        + imbalance.max(0.0)
}

/// Derives tactical/autoresolve starting state from the authoritative strategic
/// snapshot without double-counting pain or blood loss, which combat recomputes.
#[must_use]
pub fn derive_combat_starting_condition(
    strategic_incapacitation: f32,
    strategic_pain: f32,
    strategic_blood_loss: f32,
    current_blood: f32,
    maximum_blood: f32,
) -> (f32, f32) {
    let starting_incapacitation =
        (strategic_incapacitation - strategic_pain - strategic_blood_loss).max(0.0);
    let starting_blood_fraction = if maximum_blood.is_finite() && maximum_blood > 0.0 {
        (current_blood / maximum_blood).clamp(0.0, 1.0)
    } else {
        1.0
    };
    (starting_incapacitation, starting_blood_fraction)
}

const UPPER_MUSCLE_KG_PER_STRENGTH: f32 = 5.0;
const MUSCLE_KG_TO_JOULES: f32 = 2.0;
const UPPER_MUSCLE_KG_TO_PUNCH_KG: f32 = 0.1;
/// Empty-hand contacts move a whole body much more readily than they cause
/// disabling tissue injury. This resistance puts canonical John Fabelgeist's
/// ordinary connected punch into a 70 kg opponent at roughly 40% imbalance.
const UNARMED_STAGGER_RESISTANCE_JOULES_PER_KG: f32 = 0.875;
const UNARMED_BLUNT_INJURY_SCALE: f32 = 0.2;
/// Avoiding a committed swing can pull the attacker off balance, but this is
/// a bounded physical consequence of their own momentum rather than the raw
/// (and unbounded) margin between two skill checks.
const DODGE_OVEREXTENSION_SCALE: f32 = 0.25;
const WEAPON_DEFENSE_REBOUND_SCALE: f32 = 0.5;
const MAX_AVOIDED_ATTACK_BALANCE_DAMAGE: f32 = 0.5;

fn precision_damage_multiplier(excess_accuracy: f32, lore_cap: f32) -> f32 {
    excess_accuracy.max(0.0).min(lore_cap.max(2.0))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AttackResult {
    ToAttacker {
        balance_damage: f32,
        /// Physical force exchanged with a successful block or parry.
        contact_force: f32,
        /// False for a clean miss or dodge; true when equipment intercepted the attack.
        physical_contact: bool,
    },
    ToDefender {
        cut_damage: f32,
        blunt_damage: f32,
        balance_damage: f32,
        /// Physical force delivered at contact before armor absorption.
        contact_force: f32,
        /// Exact engaged authored surface and its physical energy accounting.
        /// `None` means the selected anatomical destination was a coverage gap.
        armor_impact: Option<ArmorImpact>,
    },
}

impl Mul<f32> for AttackResult {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        match self {
            Self::ToAttacker {
                balance_damage,
                contact_force,
                physical_contact,
            } => Self::ToAttacker {
                balance_damage: balance_damage * rhs,
                contact_force: contact_force * rhs,
                physical_contact,
            },
            Self::ToDefender {
                cut_damage,
                blunt_damage,
                balance_damage,
                contact_force,
                armor_impact,
            } => Self::ToDefender {
                cut_damage: cut_damage * rhs,
                blunt_damage: blunt_damage * rhs,
                balance_damage: balance_damage * rhs,
                contact_force: contact_force * rhs,
                armor_impact: armor_impact.map(|impact| ArmorImpact {
                    resisted_energy_joules: impact.resisted_energy_joules * rhs,
                    transmitted_energy_joules: impact.transmitted_energy_joules * rhs,
                    penetrated_energy_joules: impact.penetrated_energy_joules * rhs,
                    ..impact
                }),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum DefenderResponse {
    #[default]
    None,
    Block {
        effectiveness: f32,
    },
    Parry {
        input_reflex: f32,
        precision: f32,
    },
    Dodge {
        input_reflex: f32,
    },
}

pub fn flanking_from_dir(attacker_dir: (f32, f32), defender_dir: (f32, f32)) -> f32 {
    let dot = (attacker_dir.0 * defender_dir.0) + (attacker_dir.1 * defender_dir.1);

    let lower = -f32::consts::FRAC_1_SQRT_2; // dot of 135°
    let higher = f32::consts::FRAC_1_SQRT_2; // dot of 45°

    1.0 + (dot.clamp(lower, higher) + lower) / (higher - lower)
}

/// Resolve a melee attack between an attacker and a defender.
///
/// `hit_precision` is a value in [0.0, 1.0] where 0.0 is a complete miss
/// and 1.0 is a perfect hit.
///
/// Returns the outcome including damage values. Damage is not yet
/// applied to any body part — the caller is responsible for that.
fn avoided_attack_balance_damage(
    accuracy: f32,
    attacker_attr: &impl PlayerAttributes,
    attacker_body: &impl PlayerBody,
    attacker_equip: &impl PlayerEquipment,
    parameters: CombatResolutionParameters,
    defender_response: DefenderResponse,
) -> f32 {
    let response_scale = defender_response.rebound_scale();
    let resistance_per_kg = if attacker_equip.weapon_is_unarmed() {
        UNARMED_STAGGER_RESISTANCE_JOULES_PER_KG
    } else {
        parameters.stagger_resistance_joules_per_kg
    };
    let whole_body_mass = attacker_body.body_weight() + attacker_equip.inventory_weight();
    let resistance = resistance_per_kg * whole_body_mass.max(f32::EPSILON);
    let committed_impulse = attack_force(attacker_attr, attacker_body, attacker_equip, parameters)
        * accuracy.clamp(0.0, 1.0)
        * 0.5;
    (committed_impulse / resistance * response_scale).clamp(0.0, MAX_AVOIDED_ATTACK_BALANCE_DAMAGE)
}

/// Resolve a ranged attack using the same defense, armor, and damage model as
/// melee combat. Ranged accuracy uses the attacker's Ranged check and the
/// weapon's projectile energy rather than muscular striking force.
#[expect(
    clippy::too_many_arguments,
    reason = "combat resolution receives independent attacker and defender facets"
)]
pub fn resolve_ranged_attack_by_parts(
    attacker_skills: &impl PlayerSkills,
    attacker_attr: &impl PlayerAttributes,
    attacker_body: &impl PlayerBody,
    attacker_essentials: &impl PlayerEssentials,
    attacker_equip: &impl PlayerEquipment,
    parameters: CombatResolutionParameters,
    hit_precision: f32,
    precision_damage_multiplier_cap: f32,
    flanking: f32,
    defender_body_part: BodyPart,
    defender_response: DefenderResponse,
    defender_skills: &impl PlayerSkills,
    defender_attr: &impl PlayerAttributes,
    defender_body: &impl PlayerBody,
    defender_essentials: &impl PlayerEssentials,
    defender_equip: &impl PlayerEquipment,
) -> AttackResult {
    let accuracy = attacker_equip
        .weapon_skill_distribution()
        .weighted_check(|skill| {
            attacker_skills.skill_check_by_parts(
                skill,
                attacker_attr,
                attacker_body,
                attacker_essentials,
                attacker_equip,
                LimbWeights::both_arms(),
            )
        })
        * attacker_equip.weapon_accuracy()
        * hit_precision.clamp(0.0, 1.0);

    let defense = defense_by_parts(
        defender_response,
        defender_skills,
        defender_attr,
        defender_body,
        defender_essentials,
        defender_equip,
    ) * (1.0 - flanking).clamp(0.0, 1.0);
    let attack = accuracy - defense;

    if attack < 0.0 {
        return AttackResult::ToAttacker {
            // Missing with a projectile does not physically unbalance the
            // attacker. Keep the common result shape for callers.
            balance_damage: 0.0,
            contact_force: if defender_response.is_weapon_contact() {
                attacker_equip.weapon_ranged_force_joules() * accuracy.clamp(0.0, 1.0)
            } else {
                0.0
            },
            physical_contact: defender_response.is_weapon_contact(),
        };
    }

    if attack > 1.0 && attacker_equip.weapon_is_precise() {
        let critical = (attack - 1.0 - defender_equip.armor_coverage(defender_body_part)).max(0.0);
        if critical > 0.0 {
            return calculate_damage_from_force(
                1.0,
                attacker_equip.weapon_ranged_force_joules(),
                attacker_equip,
                defender_body_part,
                defender_body,
                defender_equip,
                None,
                parameters,
                MeleeContactAtTime::intended(0.0),
            ) * precision_damage_multiplier(critical, precision_damage_multiplier_cap);
        }
    }
    calculate_damage_from_force(
        attack.min(1.0),
        attacker_equip.weapon_ranged_force_joules(),
        attacker_equip,
        defender_body_part,
        defender_body,
        defender_equip,
        defender_equip.armor_surface(defender_body_part, 0.5),
        parameters,
        MeleeContactAtTime::intended(0.0),
    )
}

fn defense_by_parts(
    response: DefenderResponse,
    skills: &impl PlayerSkills,
    attr: &impl PlayerAttributes,
    body: &impl PlayerBody,
    essentials: &impl PlayerEssentials,
    equip: &impl PlayerEquipment,
) -> f32 {
    let base = match response {
        DefenderResponse::None => 0.0,
        DefenderResponse::Block { .. } | DefenderResponse::Parry { .. } => {
            let block = skills.skill_check_by_parts(
                Skill::Block,
                attr,
                body,
                essentials,
                equip,
                LimbWeights::all_equal(),
            );
            5.0 * (1.0 - (-(equip.shield_block_bonus() + block) / 2.0).exp())
        }
        DefenderResponse::Dodge { .. } => skills.skill_check_by_parts(
            Skill::Dodge,
            attr,
            body,
            essentials,
            equip,
            LimbWeights::both_legs(),
        ),
    };
    base * response.factor()
}

/// Calculate attack force in joules
fn attack_force(
    attr: &impl PlayerAttributes,
    body: &impl PlayerBody,
    equip: &impl PlayerEquipment,
    parameters: CombatResolutionParameters,
) -> f32 {
    let strength =
        attr.limb_attr_by_weight_by_parts(LimbAttribute::Strength, body, LimbWeights::both_arms());
    let upper_muscle_kg = strength * UPPER_MUSCLE_KG_PER_STRENGTH;
    let punch_kg = UPPER_MUSCLE_KG_TO_PUNCH_KG * upper_muscle_kg;
    let striking_mass_kg =
        punch_kg + equip.weapon_weight() * (1.0 + equip.weapon_balance() * equip.weapon_reach());
    let weapon_transfer = if equip.weapon_is_unarmed() {
        1.0
    } else {
        parameters.armed_attack_energy_transfer
    };
    upper_muscle_kg * MUSCLE_KG_TO_JOULES * striking_mass_kg * weapon_transfer
}

#[expect(
    clippy::too_many_arguments,
    reason = "damage resolution receives independent attack and defense facets"
)]
fn calculate_damage(
    attack: f32,
    attacker_attr: &impl PlayerAttributes,
    attacker_body: &impl PlayerBody,
    attacker_equip: &impl PlayerEquipment,
    defender_body_part: BodyPart,
    defender_body: &impl PlayerBody,
    defender_equip: &impl PlayerEquipment,
    armor_surface: Option<crate::equipment::ArmorSurface>,
    parameters: CombatResolutionParameters,
    contact_at_time: MeleeContactAtTime,
) -> AttackResult {
    if contact_at_time.classification == MeleeContactClassification::InvalidatedMiss {
        return AttackResult::ToAttacker {
            balance_damage: 0.0,
            contact_force: 0.0,
            physical_contact: false,
        };
    }
    calculate_damage_from_force(
        attack,
        attack_force(attacker_attr, attacker_body, attacker_equip, parameters)
            * contact_at_time.energy_fraction,
        attacker_equip,
        defender_body_part,
        defender_body,
        defender_equip,
        armor_surface,
        parameters,
        contact_at_time,
    )
}

#[expect(clippy::too_many_arguments, reason = "independent combat facets")]
fn calculate_damage_from_force(
    attack: f32,
    full_force: f32,
    attacker_equip: &impl PlayerEquipment,
    _defender_body_part: BodyPart,
    defender_body: &impl PlayerBody,
    defender_equip: &impl PlayerEquipment,
    armor_surface: Option<crate::equipment::ArmorSurface>,
    parameters: CombatResolutionParameters,
    contact_at_time: MeleeContactAtTime,
) -> AttackResult {
    let attack = attack.clamp(0.0, 1.0);

    let unarmed = attacker_equip.weapon_is_unarmed();
    let shortened_contact = matches!(
        contact_at_time.classification,
        MeleeContactClassification::Haft | MeleeContactClassification::Pommel
    );
    let has_edge = !shortened_contact
        && (attacker_equip.weapon_does_slash() || attacker_equip.weapon_does_pierce());
    let has_blunt = shortened_contact || unarmed || attacker_equip.weapon_does_blunt();
    let stagger_resistance_per_kg = if unarmed {
        UNARMED_STAGGER_RESISTANCE_JOULES_PER_KG
    } else {
        parameters.stagger_resistance_joules_per_kg
    };
    let defender_stagger_resistance = stagger_resistance_per_kg
        * (defender_equip.inventory_weight() + defender_body.body_weight());

    let attack_force = full_force.max(0.0) * attack;
    if attack_force <= f32::EPSILON {
        return AttackResult::ToAttacker {
            balance_damage: 0.0,
            contact_force: 0.0,
            physical_contact: false,
        };
    }
    let energy = armor::resolve_contact_energy(
        armor_surface,
        attack,
        attack_force,
        has_edge,
        has_blunt,
        attacker_equip.weapon_penetration().max(0.0),
    );
    let cut_damage = energy.cut_energy_joules;
    let blunt_damage = energy.blunt_energy_joules
        * if unarmed {
            UNARMED_BLUNT_INJURY_SCALE
        } else {
            1.0
        };
    // A pure blunt impact still transfers momentum when there is no edge for
    // resistance to absorb. Edge and mixed contacts retain the absorbed-force
    // impulse used by the existing model.
    let stagger_impulse = if has_blunt && !has_edge {
        attack_force * 0.5
    } else {
        energy.armor_impact.map_or(attack_force * 0.5, |impact| {
            impact.resisted_energy_joules * 0.5
        })
    };
    let balance_damage = stagger_impulse / defender_stagger_resistance;
    AttackResult::ToDefender {
        cut_damage,
        blunt_damage,
        balance_damage,
        contact_force: attack_force,
        armor_impact: energy.armor_impact,
    }
}

/// Convert physical damage energy into the strategic 0-1 body-part health
/// scale. The calibration follows combat.md: roughly 20 J to an unarmored arm
/// is disabling, while larger body regions absorb proportionally more energy.
pub fn health_damage_from_attack(result: AttackResult, part: BodyPart) -> f32 {
    let AttackResult::ToDefender {
        cut_damage,
        blunt_damage,
        ..
    } = result
    else {
        return 0.0;
    };
    let capacity_joules = match part {
        BodyPart::LeftArm | BodyPart::RightArm => 20.0,
        BodyPart::LeftLeg | BodyPart::RightLeg => 35.0,
        BodyPart::Head => 20.0,
        BodyPart::Chest => 80.0,
        BodyPart::Stomach => 55.0,
    };
    ((cut_damage + blunt_damage * 0.75) / capacity_joules).max(0.0)
}

/// Partitions clamped limb-health damage according to each attack channel's
/// contribution to the health calculation.
#[must_use]
pub fn apportion_attack_health_damage(result: AttackResult, applied: f32) -> (f32, f32) {
    let AttackResult::ToDefender {
        cut_damage,
        blunt_damage,
        ..
    } = result
    else {
        return (0.0, 0.0);
    };
    let cut_weight = cut_damage.max(0.0);
    let blunt_weight = blunt_damage.max(0.0) * 0.75;
    let total_weight = cut_weight + blunt_weight;
    if applied <= 0.0 || total_weight <= 0.0 {
        return (0.0, 0.0);
    }
    let applied_cut = applied * cut_weight / total_weight;
    (applied_cut, applied - applied_cut)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoresolve::{CombatArmor, CombatEquipment, CombatWeapon};
    use crate::stub::{StubAttributes, StubBody, StubEquipment, StubEssentials, StubSkills};

    #[derive(Debug)]
    struct MatchupCombatant<'a> {
        name: &'a str,
        weight_kg: f32,
        will_check: f32,
    }

    impl PlayerBody for MatchupCombatant<'_> {
        fn body_part_health(&self, _part: BodyPart) -> f32 {
            1.0
        }

        fn body_weight(&self) -> f32 {
            self.weight_kg
        }

        fn primary_side(&self) -> BodySide {
            BodySide::Right
        }
    }

    fn assert_in_window(label: &str, actual: f32, expected: (f32, f32)) {
        assert!(
            (expected.0..=expected.1).contains(&actual),
            "{label}: {actual:.4} was outside {:.4}..={:.4}",
            expected.0,
            expected.1,
        );
    }

    #[test]
    fn no_defender_response_has_zero_defense() {
        let none = defense_by_parts(
            DefenderResponse::None,
            &StubSkills,
            &StubAttributes,
            &StubBody,
            &StubEssentials,
            &StubEquipment,
        );
        let parry = defense_by_parts(
            DefenderResponse::Parry {
                input_reflex: 1.0,
                precision: 1.0,
            },
            &StubSkills,
            &StubAttributes,
            &StubBody,
            &StubEssentials,
            &StubEquipment,
        );
        assert_eq!(none, 0.0);
        assert!(parry > 0.0);
    }

    #[test]
    fn parry_precision_scales_defense_independently_of_reflex() {
        let high_precision = defense_by_parts(
            DefenderResponse::Parry {
                input_reflex: 0.8,
                precision: 1.0,
            },
            &StubSkills,
            &StubAttributes,
            &StubBody,
            &StubEssentials,
            &StubEquipment,
        );
        let low_precision = defense_by_parts(
            DefenderResponse::Parry {
                input_reflex: 0.8,
                precision: 0.25,
            },
            &StubSkills,
            &StubAttributes,
            &StubBody,
            &StubEssentials,
            &StubEquipment,
        );
        assert!((low_precision - high_precision * 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn blunt_force_bypasses_edge_resistance_but_not_padding() {
        let attacker = CombatEquipment {
            weapon: Some(CombatWeapon {
                blunt: true,
                penetration: 0.5,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut defender = CombatEquipment::default();
        defender.armor.fill(CombatArmor {
            resistance: 1_000.0,
            padding: 20.0,
            flexibility: 0.5,
            range_of_motion: 1.0,
            coverage: 1.0,
            ..Default::default()
        });

        let result = calculate_damage_from_force(
            1.0,
            100.0,
            &attacker,
            BodyPart::Chest,
            &StubBody,
            &defender,
            defender.armor_surface(BodyPart::Chest, 0.0),
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
            MeleeContactAtTime::intended(0.0),
        );
        let AttackResult::ToDefender {
            cut_damage,
            blunt_damage,
            balance_damage,
            ..
        } = result
        else {
            panic!("an undefended hit must damage the defender");
        };
        assert_eq!(cut_damage, 0.0);
        assert_eq!(blunt_damage, 80.0);
        assert_eq!(
            balance_damage,
            50.0 / (70.0 * EMBEDDED_COMBAT_RESOLUTION_PARAMETERS.stagger_resistance_joules_per_kg)
        );
    }

    #[test]
    fn johns_longsword_glances_off_munition_plate() {
        let john = crate::starting_character::default_character("combat-matchups");
        let john_body = MatchupCombatant {
            name: &john.name,
            weight_kg: 70.0,
            will_check: john.skills.will,
        };
        let longsword = CombatEquipment {
            weapon: Some(CombatWeapon {
                weight: 1.5,
                penetration: 1.0,
                melee_reach: 1.25,
                balance: 0.45,
                slash: true,
                pierce: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut munition_armor = CombatEquipment {
            inventory_weight: 15.95,
            ..Default::default()
        };
        munition_armor.armor[crate::autoresolve::body_part_index(BodyPart::Head)] = CombatArmor {
            resistance: 115.0,
            padding: 60.0,
            flexibility: 27.0 / 115.0,
            range_of_motion: 0.78,
            coverage: 0.7475,
            ..Default::default()
        };

        let force = attack_force(
            &john.attributes,
            &john_body,
            &longsword,
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
        );
        assert_in_window("John longsword energy", force, (69.0, 70.0));
        let result = calculate_damage_from_force(
            1.0,
            force,
            &longsword,
            BodyPart::Head,
            &john_body,
            &munition_armor,
            munition_armor.armor_surface(BodyPart::Head, 0.5),
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
            MeleeContactAtTime::intended(0.0),
        );
        let AttackResult::ToDefender {
            cut_damage,
            blunt_damage,
            balance_damage,
            ..
        } = result
        else {
            panic!("a plate contact must land on the defender");
        };
        assert_eq!(cut_damage, 0.0);
        assert_eq!(blunt_damage, 0.0);
        assert_in_window("munition plate imbalance", balance_damage, (0.09, 0.11));
        assert_eq!(health_damage_from_attack(result, BodyPart::Head), 0.0);
    }

    #[test]
    fn unarmed_matchups_land_in_realistic_outcome_windows() {
        struct Matchup<'a, 'b> {
            defender: &'a MatchupCombatant<'b>,
            target: BodyPart,
            contact_energy: (f32, f32),
            imbalance: (f32, f32),
            health_damage: (f32, f32),
            total_incapacitation: (f32, f32),
        }

        let john = crate::starting_character::default_character("combat-matchups");
        assert_eq!(john.name, crate::starting_character::DEFAULT_CHARACTER_NAME);
        let john_body = MatchupCombatant {
            name: &john.name,
            weight_kg: 70.0,
            will_check: john.skills.will,
        };
        let light_bandit = MatchupCombatant {
            name: "light bandit",
            weight_kg: 55.0,
            will_check: 1.5,
        };
        let average_bandit = MatchupCombatant {
            name: "average bandit",
            weight_kg: 70.0,
            will_check: 1.5,
        };
        let heavy_bandit = MatchupCombatant {
            name: "heavy bandit",
            weight_kg: 95.0,
            will_check: 1.5,
        };
        let unarmed = CombatEquipment::default();

        let matchups = [
            Matchup {
                defender: &average_bandit,
                target: BodyPart::Head,
                contact_energy: (48.0, 51.0),
                imbalance: (0.38, 0.45),
                health_damage: (0.25, 0.40),
                total_incapacitation: (0.55, 0.75),
            },
            Matchup {
                defender: &light_bandit,
                target: BodyPart::Chest,
                contact_energy: (48.0, 51.0),
                imbalance: (0.45, 0.60),
                health_damage: (0.06, 0.11),
                total_incapacitation: (0.50, 0.70),
            },
            Matchup {
                defender: &heavy_bandit,
                target: BodyPart::Stomach,
                contact_energy: (48.0, 51.0),
                imbalance: (0.25, 0.35),
                health_damage: (0.09, 0.15),
                total_incapacitation: (0.34, 0.48),
            },
        ];

        for matchup in matchups {
            let label = format!(
                "{} -> {} ({})",
                john_body.name, matchup.defender.name, matchup.target
            );
            let result = resolve_melee_attack_by_parts(
                &john.skills,
                &john.attributes,
                &john_body,
                &StubEssentials,
                &unarmed,
                EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
                BodySide::Right,
                crate::combat_style::MeleeAttackStyle::Swing,
                1.0,
                2.0,
                0.0,
                MeleeContactLocation {
                    body_part: matchup.target,
                    anatomical_subregion: anatomical_subregion(matchup.target, 0.5),
                    surface_coordinate: 0.5,
                    armor_surface: None,
                },
                MeleeContactAtTime::intended(0.0),
                DefenderResponse::None,
                &StubSkills,
                &StubAttributes,
                matchup.defender,
                &StubEssentials,
                &unarmed,
            );
            let AttackResult::ToDefender {
                balance_damage,
                contact_force,
                ..
            } = result
            else {
                panic!("{label}: undefended punch did not reach defender");
            };
            let health_damage = health_damage_from_attack(result, matchup.target);
            let (cut, blunt) = apportion_attack_health_damage(result, health_damage);
            let blunt_energy = match result {
                AttackResult::ToDefender { blunt_damage, .. } => blunt_damage,
                AttackResult::ToAttacker { .. } => 0.0,
            };
            let wounds =
                wounds_from_applied_health_damage(matchup.target, cut, blunt, blunt_energy);
            let blood_loss = advance_combat_bleeding(0.0, &wounds, 60.0);
            let total_incapacitation = combat_incapacitation(
                0.0,
                1.0,
                blood_loss,
                health_damage,
                matchup.defender.will_check,
                balance_damage,
            );

            assert_in_window(
                &format!("{label} contact energy"),
                contact_force,
                matchup.contact_energy,
            );
            assert_in_window(
                &format!("{label} imbalance"),
                balance_damage,
                matchup.imbalance,
            );
            assert_in_window(
                &format!("{label} health damage"),
                health_damage,
                matchup.health_damage,
            );
            assert_in_window(
                &format!("{label} total incapacitation"),
                total_incapacitation,
                matchup.total_incapacitation,
            );
            assert!(
                blood_loss < 0.01,
                "{label}: blunt punch caused {blood_loss:.4} immediate blood loss"
            );
        }

        struct AvoidedMatchup {
            label: &'static str,
            response: DefenderResponse,
            defender_equipment: CombatEquipment,
            expected_imbalance: (f32, f32),
            expected_contact: bool,
        }
        let avoided_matchups = [
            AvoidedMatchup {
                label: "John punch cleanly dodged",
                response: DefenderResponse::Dodge { input_reflex: 1.0 },
                defender_equipment: CombatEquipment::default(),
                expected_imbalance: (0.08, 0.13),
                expected_contact: false,
            },
            AvoidedMatchup {
                label: "John punch caught by shield parry",
                response: DefenderResponse::Parry {
                    input_reflex: 1.0,
                    precision: 1.0,
                },
                defender_equipment: CombatEquipment {
                    shield_block_bonus: 5.0,
                    ..Default::default()
                },
                expected_imbalance: (0.17, 0.24),
                expected_contact: true,
            },
        ];

        for matchup in avoided_matchups {
            let result = resolve_melee_attack_by_parts(
                &john.skills,
                &john.attributes,
                &john_body,
                &StubEssentials,
                &unarmed,
                EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
                BodySide::Right,
                crate::combat_style::MeleeAttackStyle::Swing,
                1.0,
                2.0,
                0.0,
                MeleeContactLocation {
                    body_part: BodyPart::Chest,
                    anatomical_subregion: AnatomicalSubregion::ChestAxilla,
                    surface_coordinate: 0.9,
                    armor_surface: None,
                },
                MeleeContactAtTime::intended(0.0),
                matchup.response,
                &john.skills,
                &john.attributes,
                &john_body,
                &StubEssentials,
                &matchup.defender_equipment,
            );
            let AttackResult::ToAttacker {
                balance_damage,
                physical_contact,
                ..
            } = result
            else {
                panic!("{}: defense did not avoid the punch", matchup.label);
            };
            assert_in_window(
                &format!("{} attacker imbalance", matchup.label),
                balance_damage,
                matchup.expected_imbalance,
            );
            assert_eq!(
                physical_contact, matchup.expected_contact,
                "{}",
                matchup.label
            );
        }
    }

    #[test]
    fn penetration_can_cross_resistance_at_armor_limited_force() {
        let cutting = |penetration| CombatEquipment {
            weapon: Some(CombatWeapon {
                slash: true,
                penetration,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut defender = CombatEquipment::default();
        defender.armor.fill(CombatArmor::innate(150.0, 0.0));

        let lower = calculate_damage_from_force(
            1.0,
            100.0,
            &cutting(0.5),
            BodyPart::Chest,
            &StubBody,
            &defender,
            defender.armor_surface(BodyPart::Chest, 0.0),
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
            MeleeContactAtTime::intended(0.0),
        );
        let stronger = calculate_damage_from_force(
            1.0,
            100.0,
            &cutting(1.0),
            BodyPart::Chest,
            &StubBody,
            &defender,
            defender.armor_surface(BodyPart::Chest, 0.0),
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
            MeleeContactAtTime::intended(0.0),
        );

        assert!(matches!(
            lower,
            AttackResult::ToDefender {
                cut_damage: 0.0,
                ..
            }
        ));
        assert!(matches!(
            stronger,
            AttackResult::ToDefender {
                cut_damage: 25.0,
                ..
            }
        ));
    }

    #[test]
    fn hammer_gap_contact_partitions_seventy_six_joules_without_duplication() {
        let attacker = CombatEquipment {
            weapon: Some(CombatWeapon {
                blunt: true,
                slash: true,
                penetration: 1.0,
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = calculate_damage_from_force(
            1.0,
            76.5,
            &attacker,
            BodyPart::Chest,
            &StubBody,
            &CombatEquipment::default(),
            None,
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
            MeleeContactAtTime::intended(0.0),
        );
        assert!(matches!(
            result,
            AttackResult::ToDefender {
                cut_damage: 38.25,
                blunt_damage: 38.25,
                ..
            }
        ));
    }

    #[test]
    fn shortened_halberd_contact_conserves_energy_and_cannot_cut() {
        let halberd = CombatEquipment {
            weapon: Some(CombatWeapon {
                blunt: true,
                slash: true,
                pierce: true,
                penetration: 2.0,
                ..Default::default()
            }),
            ..Default::default()
        };
        let contact = resolve_melee_contact_at_time(MeleeContactAtTimeFacts {
            scheduled_measure_metres: 2.0,
            actual_measure_metres: 1.25,
            ideal_measure_metres: 1.92,
            effective_reach_metres: 2.0,
            grip_to_tip_metres: 1.9,
            total_length_metres: 2.1,
            striking_head_length_metres: 0.16,
            distal_headed: true,
            attack_style: crate::combat_style::MeleeAttackStyle::Swing,
            body_material: Some(crate::item_catalog_schema::EquipmentMaterial::Hardwood),
            striking_material: Some(crate::item_catalog_schema::EquipmentMaterial::RoughSteel),
        });
        let result = calculate_damage_from_force(
            1.0,
            101.4 * contact.energy_fraction,
            &halberd,
            BodyPart::Chest,
            &StubBody,
            &CombatEquipment::default(),
            None,
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
            contact,
        );
        let AttackResult::ToDefender {
            cut_damage,
            blunt_damage,
            contact_force,
            ..
        } = result
        else {
            panic!("shortened contact must transfer shaft energy");
        };
        assert_eq!(cut_damage, 0.0);
        assert!((blunt_damage - contact_force).abs() < 0.001);
        assert!((contact_force - 101.4 * contact.energy_fraction).abs() < 0.001);
    }

    #[test]
    fn zero_energy_is_no_effective_contact() {
        let result = calculate_damage_from_force(
            1.0,
            0.0,
            &StubEquipment,
            BodyPart::Chest,
            &StubBody,
            &StubEquipment,
            None,
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
            MeleeContactAtTime::intended(0.0),
        );
        assert!(matches!(
            result,
            AttackResult::ToAttacker {
                contact_force: 0.0,
                physical_contact: false,
                ..
            }
        ));
    }

    #[test]
    fn anatomical_destination_then_coverage_distinguishes_surface_from_gap() {
        let mut defender = CombatEquipment::default();
        defender.armor[crate::autoresolve::body_part_index(BodyPart::Chest)] = CombatArmor {
            resistance: 120.0,
            padding: 50.0,
            flexibility: 0.08,
            range_of_motion: 0.65,
            coverage: 1.0,
            ..Default::default()
        };

        assert!((whole_body_armor_coverage(&defender) - 0.22).abs() < 0.0001);
        let contact = melee_contact_location(&defender, 0.5, 0.0);
        assert_eq!(contact.body_part, BodyPart::Chest);
        assert!(contact.armor_surface.is_some());
    }

    #[test]
    fn partial_body_part_coverage_has_forced_surface_and_gap_samples() {
        let mut defender = CombatEquipment::default();
        defender.armor[crate::autoresolve::body_part_index(BodyPart::Chest)] = CombatArmor {
            resistance: 120.0,
            padding: 50.0,
            flexibility: 0.08,
            range_of_motion: 0.65,
            coverage: 0.7,
            inventory_item_id: Some(42),
            material: Some(crate::item_catalog_schema::EquipmentMaterial::RoughSteel),
            coverage_span: None,
            coverage_geometry: None,
        };
        let surface = melee_contact_location(&defender, 0.524, 0.0);
        let gap = melee_contact_location(&defender, 0.689, 0.0);
        assert_eq!(surface.body_part, BodyPart::Chest);
        assert_eq!(surface.armor_surface.unwrap().inventory_item_id, Some(42));
        assert_eq!(gap.body_part, BodyPart::Chest);
        assert!(gap.armor_surface.is_none());
        let precisely_placed = melee_contact_location(&defender, 0.524, 1.0);
        assert!(
            precisely_placed.armor_surface.is_none(),
            "precision biases the coverage sample toward a real gap before contact"
        );
    }

    #[test]
    fn anatomical_lore_clamps_excess_accuracy_with_a_two_x_floor() {
        assert_eq!(precision_damage_multiplier(6.0, 0.0), 2.0);
        assert_eq!(precision_damage_multiplier(6.0, 3.5), 3.5);
        assert_eq!(precision_damage_multiplier(1.25, 7.0), 1.25);
    }

    #[test]
    fn depleted_strategic_snapshot_excludes_recomputed_sources() {
        let (starting, blood_fraction) =
            derive_combat_starting_condition(1.35, 0.25, 0.5, 2_500.0, 5_000.0);
        assert!((starting - 0.6).abs() < 0.0001);
        assert!((blood_fraction - 0.5).abs() < 0.0001);
        assert_eq!(
            derive_combat_starting_condition(0.2, 0.3, 0.4, 1.0, 0.0),
            (0.0, 1.0)
        );
    }

    #[test]
    fn severe_trauma_can_incapacitate_before_blood_loss() {
        let immediate = combat_incapacitation(0.0, 1.0, 0.0, 5.0, 0.0, 0.0);
        assert!(immediate >= 1.0);
    }
}
