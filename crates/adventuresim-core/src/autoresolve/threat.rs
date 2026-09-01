use super::*;
use crate::bestiary::{AttackStyle, Protection, RigTopology, ThreatId};

/// Build the same authored threat combatant used by strategic autoresolve and
/// observer-safe contract assessment.
pub fn authored_threat_combatant(
    id: u64,
    enemy_type: &str,
    difficulty: i32,
    combat_scale_bps: u32,
    countermeasure_multiplier_bps: u32,
) -> Result<Combatant, String> {
    let threat: ThreatId = enemy_type
        .parse()
        .map_err(|_| format!("Unknown threat ID: {enemy_type}"))?;
    let (physical_scale, training_scale) = crate::threat_escalation::combat_scaling_multipliers(
        combat_scale_bps,
        countermeasure_multiplier_bps,
    );
    let base_rating = 1.2 + difficulty.max(1) as f32 * 0.35;
    let physical_rating = base_rating * physical_scale;
    let threat_profile = threat.profile();
    let profile = threat_profile.combat;
    let mut combatant = Combatant::new(id);
    combatant.bestiary_categories = threat_profile.categories().collect();
    combatant.attributes = threat_attributes(physical_rating);
    let training = base_rating * 1_500.0 * profile.training_multiplier * training_scale;
    combatant.skills = threat_skills(training, profile.ranged, profile.protection, profile.morale);
    combatant.body.weight_kg = profile.weight_kg;
    let weapon = threat_weapon(profile);
    combatant.equipment.weapon = Some(weapon);
    equip_threat_weapon(&mut combatant, weapon, profile.ranged);
    let innate = profile.innate_protection;
    if innate.resistance_joules > 0.0 || innate.padding_joules > 0.0 {
        combatant.equipment.armor.fill(CombatArmor::innate(
            innate.resistance_joules,
            innate.padding_joules,
        ));
    }
    if matches!(profile.protection, Protection::Armored) {
        combatant.equipment.shield_block_bonus = 1.0;
        combatant.equipment.armor.fill(armored_threat_armor());
    }
    Ok(combatant)
}

fn threat_attributes(rating: f32) -> PlayerAttributeValues {
    PlayerAttributeValues {
        endurance: rating,
        immunity: rating,
        gut: rating,
        intelligence: rating * 0.7,
        instinct: rating,
        eyesight: rating,
        hearing: rating,
        left_arm_strength: rating,
        right_arm_strength: rating,
        left_leg_strength: rating,
        right_leg_strength: rating,
        left_arm_agility: rating,
        right_arm_agility: rating,
        left_leg_agility: rating,
        right_leg_agility: rating,
    }
}

fn threat_skills(training: f32, ranged: bool, protection: Protection, morale: u8) -> CombatSkills {
    CombatSkills {
        sword_hours: training,
        bow_hours: if ranged { training * 2.0 } else { 0.0 },
        dodge_hours: training,
        block_hours: if matches!(protection, Protection::Shielded | Protection::Armored) {
            training
        } else {
            training * 0.4
        },
        will_hours: training * (0.5 + f32::from(morale) / 50.0),
        balance_hours: training,
        ..CombatSkills::default()
    }
}

fn threat_weapon(profile: crate::bestiary::CombatProfile) -> CombatWeapon {
    let (blunt, slash, pierce) = match profile.attack {
        AttackStyle::Blunt => (true, false, false),
        AttackStyle::Blade => (false, true, false),
        AttackStyle::Knife
        | AttackStyle::Spear
        | AttackStyle::Bow
        | AttackStyle::Bite
        | AttackStyle::Claw => (false, false, true),
    };
    CombatWeapon {
        skills: if profile.ranged {
            crate::equipment::WeaponSkillDistribution {
                bow: 1.0,
                ..Default::default()
            }
        } else {
            crate::equipment::WeaponSkillDistribution {
                sword: 1.0,
                ..Default::default()
            }
        },
        melee: !profile.ranged,
        ranged: profile.ranged,
        blunt,
        slash,
        pierce,
        accuracy: 0.8 + profile.precision_bonus,
        swing_precision: if profile.ranged {
            0.0
        } else {
            0.8 + profile.precision_bonus
        },
        stab_precision: if profile.ranged {
            0.0
        } else {
            0.8 + profile.precision_bonus
        },
        preferred_melee_style: if pierce && !slash {
            crate::combat_style::MeleeAttackStyle::Stab
        } else {
            crate::combat_style::MeleeAttackStyle::Swing
        },
        weight: if profile.rig == RigTopology::Quadruped {
            1.0
        } else {
            1.5
        },
        penetration: if matches!(profile.attack, AttackStyle::Spear | AttackStyle::Claw) {
            1.5
        } else {
            0.8
        },
        melee_reach: if profile.ranged { 0.0 } else { 0.8 },
        grip_to_tip_m: if profile.ranged { 0.0 } else { 0.8 },
        total_length_m: if profile.ranged { 0.0 } else { 0.8 },
        ranged_range: if profile.ranged { 20.0 } else { 0.0 },
        attack_interval_seconds: if profile.ranged { 1.0 } else { 0.75 },
        precise: profile.precision_bonus > 0.0,
        balance: 0.3,
        ranged_force_joules: if profile.ranged { 40.0 } else { 0.0 },
        ..CombatWeapon::default()
    }
}

fn equip_threat_weapon(combatant: &mut Combatant, weapon: CombatWeapon, ranged: bool) {
    if ranged {
        combatant.equipment.ranged_weapon = Some(weapon);
        combatant.equipment.ranged_projectile_kind = Some(CombatProjectileKind::Arrowhead);
        combatant.equipment.melee_weapon = Some(CombatWeapon {
            melee: true,
            slash: true,
            pierce: true,
            accuracy: 1.0,
            weight: 0.5,
            penetration: 0.5,
            melee_reach: 0.5,
            attack_interval_seconds: 0.6,
            balance: 0.5,
            ..CombatWeapon::default()
        });
        combatant.equipment.ammunition = 12;
        combatant.initial_ammunition = 12;
    } else {
        combatant.equipment.melee_weapon = Some(weapon);
    }
}

fn armored_threat_armor() -> CombatArmor {
    CombatArmor {
        resistance: 25.0,
        padding: 15.0,
        flexibility: 0.8,
        range_of_motion: 0.9,
        coverage: 0.5,
        ..CombatArmor::default()
    }
}
