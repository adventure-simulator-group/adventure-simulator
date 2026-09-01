//! Melee-only iteration roster assembled from canonical character and item data.

use super::*;
use crate::{
    item_catalog_schema::{DamageType, ItemKind},
    starting_character::{StartingAttributes, StartingCharacterSpec, StartingSkills},
};

mod evidence;

pub use evidence::*;

#[derive(Clone, Debug)]
pub struct MeleeIterationBuild {
    pub key: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub equipment_description: &'static str,
    pub weapon_id: &'static str,
    pub armor_ids: Vec<&'static str>,
    pub shield_id: Option<&'static str>,
    pub combatant: Combatant,
}

pub fn melee_iteration_roster() -> Result<(MeleeIterationBuild, Vec<MeleeIterationBuild>), String> {
    let john_spec = crate::starting_character::default_character("melee-iteration");
    let john = build_from_spec(
        "john",
        "John Fabelgeist",
        "Combat-trained adventurer; strong and agile, with broadly advanced melee training.",
        "Longsword, morion, breastplate, paired steel vambraces, ordinary clothing and boots.",
        &john_spec,
        "longsword",
        &["morion", "breastplate", "vambrace", "vambrace"],
        None,
    )?;
    let mut hammer_brute = purpose_build(
        "hammer_brute",
        "Hammer Brute",
        4.5,
        2.25,
        "war_hammer",
        &["brigandine", "arming_cap"],
        None,
        "Exceptionally strong but only moderately agile fighter with competent hammer training.",
        "War hammer, brigandine, and arming cap.",
    )?;
    hammer_brute.combatant.attributes.instinct = 3.0;
    hammer_brute.combatant.attributes.left_arm_agility = 3.0;
    hammer_brute.combatant.attributes.right_arm_agility = 3.0;
    hammer_brute.combatant.attributes.left_leg_agility = 3.0;
    hammer_brute.combatant.attributes.right_leg_agility = 3.0;
    let opponents = vec![
        purpose_build(
            "militia",
            "Shield Militiaman",
            3.0,
            2.5,
            "arming_sword",
            &["arming_doublet", "arming_cap"],
            Some("buckler"),
            "Healthy adult militia fighter with competent sword, shield, dodge, and balance training.",
            "Arming sword and buckler over an arming doublet and cap.",
        )?,
        purpose_build(
            "demi_lancer",
            "Demi-lancer",
            3.5,
            3.25,
            "arming_sword",
            &["morion", "cuirass", "tassets", "vambrace", "vambrace"],
            None,
            "Fit professional mounted soldier fighting on foot, with advanced sword, defense, and balance training.",
            "Arming sword and demi-lancer steel plate armor.",
        )?,
        purpose_build(
            "polearm_veteran",
            "Polearm Veteran",
            3.5,
            4.0,
            "halberd",
            &["jack_of_plates", "sallet"],
            None,
            "Seasoned professional with expert polearm skill, strong balance, and advanced defensive training.",
            "Halberd, jack of plates, and sallet.",
        )?,
        hammer_brute,
        purpose_build(
            "knife_novice",
            "Knife Novice",
            2.5,
            1.0,
            "baselard",
            &["arming_doublet"],
            None,
            "Below-average novice with rudimentary knife, dodge, block, will, and balance training.",
            "Baselard and an arming doublet.",
        )?,
    ];
    Ok((john, opponents))
}

#[expect(
    clippy::too_many_arguments,
    reason = "fixture construction keeps reviewer context with its build"
)]
fn purpose_build(
    key: &'static str,
    name: &'static str,
    physical: f32,
    skill_rank: f32,
    weapon: &'static str,
    armor: &[&'static str],
    shield: Option<&'static str>,
    description: &'static str,
    equipment_description: &'static str,
) -> Result<MeleeIterationBuild, String> {
    let hours = Skill::Sword.hours_for_rank(skill_rank);
    let attributes = StartingAttributes {
        endurance: physical,
        immunity: physical,
        gut: physical,
        intelligence: 3.0,
        instinct: physical,
        eyesight: 3.0,
        hearing: 3.0,
        strength: physical,
        agility: physical,
    };
    let skills = StartingSkills {
        sword: hours,
        polearm: hours,
        bludgeon: hours,
        knife: hours,
        dodge: hours,
        block: hours,
        will: hours,
        balance: hours,
        ..StartingSkills::default()
    };
    let spec = StartingCharacterSpec {
        id: stable_id(key),
        name: name.into(),
        age_years: 28,
        background: description.into(),
        personality: crate::starting_character::default_character(key).personality,
        attributes,
        skills,
        currency: 0,
        settlement_selector: 0,
        inventory: Vec::new(),
        age_tier: crate::starting_character::StartingAgeTier::Adult,
        profession: None,
        organization: None,
        religion_id: None,
    };
    build_from_spec(
        key,
        name,
        description,
        equipment_description,
        &spec,
        weapon,
        armor,
        shield,
    )
}

fn stable_id(value: &str) -> u64 {
    value.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "fixture construction names combat and reviewer context"
)]
fn build_from_spec(
    key: &'static str,
    name: &'static str,
    description: &'static str,
    equipment_description: &'static str,
    spec: &StartingCharacterSpec,
    weapon_id: &'static str,
    armor_ids: &[&'static str],
    shield_id: Option<&'static str>,
) -> Result<MeleeIterationBuild, String> {
    let a = &spec.attributes;
    let mut combatant = Combatant::new(spec.id);
    combatant.attributes = PlayerAttributeValues {
        endurance: a.endurance,
        immunity: a.immunity,
        gut: a.gut,
        intelligence: a.intelligence,
        instinct: a.instinct,
        eyesight: a.eyesight,
        hearing: a.hearing,
        left_arm_strength: a.strength,
        right_arm_strength: a.strength,
        left_leg_strength: a.strength,
        right_leg_strength: a.strength,
        left_arm_agility: a.agility,
        right_arm_agility: a.agility,
        left_leg_agility: a.agility,
        right_leg_agility: a.agility,
    };
    combatant.skills = combat_skills(&spec.skills);
    combatant.body.weight_kg = 75.0;
    combatant.equipment = authored_equipment(weapon_id, armor_ids, shield_id)?;
    Ok(MeleeIterationBuild {
        key,
        name,
        description,
        equipment_description,
        weapon_id,
        armor_ids: armor_ids.to_vec(),
        shield_id,
        combatant,
    })
}

fn combat_skills(s: &StartingSkills) -> CombatSkills {
    CombatSkills {
        polearm_hours: s.polearm,
        axe_hours: s.axe,
        bludgeon_hours: s.bludgeon,
        sword_hours: s.sword,
        knife_hours: s.knife,
        dodge_hours: s.dodge,
        block_hours: s.block,
        bow_hours: 0.0,
        crossbow_hours: 0.0,
        firearm_hours: 0.0,
        throw_hours: s.throw,
        will_hours: s.will,
        insight_hours: s.insight,
        charm_hours: s.charm,
        command_hours: s.command,
        deception_hours: s.deception,
        physiology_hours: s.physiology,
        religion_hours: s.religion.total_direct(),
        stealth_hours: s.stealth,
        balance_hours: s.balance,
        bestiary_hours: s.bestiary,
        surgery_hours: s.surgery,
        tailoring_hours: s.tailoring,
        smithing_hours: s.smithing,
    }
}

fn authored_equipment(
    weapon_id: &str,
    armor_ids: &[&str],
    shield_id: Option<&str>,
) -> Result<CombatEquipment, String> {
    let definition = crate::item_catalog::definition(weapon_id)
        .ok_or_else(|| format!("unknown weapon {weapon_id}"))?;
    let ItemKind::Weapon {
        preferred_attack,
        swing_precision,
        stab_precision,
        accuracy,
        reach_m,
        penetration,
        precise,
        melee,
        ranged,
        damage_types,
        skills,
        ..
    } = &definition.kind
    else {
        return Err(format!("{weapon_id} is not a weapon"));
    };
    if !melee || *ranged {
        return Err(format!("{weapon_id} is not melee-only"));
    }
    let moment_of_inertia_kg_m2 = match definition.kind {
        ItemKind::Weapon {
            moment_of_inertia_kg_m2,
            ..
        } => moment_of_inertia_kg_m2,
        _ => unreachable!(),
    };
    let timing =
        crate::equipment::melee_attack_timing(*preferred_attack, moment_of_inertia_kg_m2, false);
    let grip_to_tip_m = definition
        .equipment
        .as_ref()
        .map_or(0.0, |equipment| equipment.physical.grip_to_tip_m);
    let dimensions_m = definition
        .equipment
        .as_ref()
        .map_or([0.0; 3], |equipment| equipment.physical.dimensions_m);
    let total_length_m = dimensions_m[1];
    let striking_head_length_m = dimensions_m[0].max(dimensions_m[2]);
    let skill_distribution: crate::equipment::WeaponSkillDistribution = (*skills).into();
    let weapon = CombatWeapon {
        skills: skill_distribution,
        melee: true,
        ranged: false,
        blunt: damage_types.contains(&DamageType::Blunt),
        slash: damage_types.contains(&DamageType::Slash),
        pierce: damage_types.contains(&DamageType::Pierce),
        accuracy: *accuracy,
        swing_precision: *swing_precision,
        stab_precision: *stab_precision,
        preferred_melee_style: *preferred_attack,
        weight: definition.weight_kg,
        moment_of_inertia_kg_m2,
        penetration: *penetration,
        melee_reach: *reach_m,
        grip_to_tip_m,
        total_length_m,
        striking_head_length_m,
        distal_headed: crate::combat::has_distal_striking_surface(
            grip_to_tip_m,
            striking_head_length_m,
            definition
                .equipment
                .as_ref()
                .and_then(|equipment| equipment.material),
            definition
                .equipment
                .as_ref()
                .and_then(|equipment| equipment.striking_material),
        ),
        body_material: definition
            .equipment
            .as_ref()
            .and_then(|equipment| equipment.material),
        striking_material: definition
            .equipment
            .as_ref()
            .and_then(|equipment| equipment.striking_material),
        ranged_range: 0.0,
        // Autoresolve advances the same complete attack cycle as the tactical
        // bot: the authored server windup to contact plus inertia/style-based
        // recovery before another fresh attack may begin.
        attack_interval_seconds: EMBEDDED_AUTORESOLVE_PARAMETERS.melee_windup_seconds
            + timing.recovery_secs,
        precise: *precise,
        balance: crate::equipment::weapon_balance_from_moment(
            moment_of_inertia_kg_m2,
            definition.weight_kg,
            grip_to_tip_m,
        ),
        ranged_force_joules: 0.0,
    };
    let mut equipment = CombatEquipment {
        weapon: Some(weapon),
        melee_weapon: Some(weapon),
        melee_weapon_id: Some(stable_id(weapon_id)),
        inventory_weight: definition.weight_kg,
        ..CombatEquipment::default()
    };
    if let Some(shield_id) = shield_id {
        let shield = crate::item_catalog::definition(shield_id)
            .ok_or_else(|| format!("unknown shield {shield_id}"))?;
        let ItemKind::Shield { block, .. } = shield.kind else {
            return Err(format!("{shield_id} is not a shield"));
        };
        equipment.shield_block_bonus = block;
        equipment.shield_side = Some(match equipment.melee_holding_side {
            BodySide::Left => BodySide::Right,
            BodySide::Right => BodySide::Left,
            BodySide::Both => BodySide::Left,
        });
        equipment.defense_item_id = Some(stable_id(shield_id));
        equipment.inventory_weight += shield.weight_kg;
    }
    apply_authored_armor(&mut equipment, armor_ids)?;
    Ok(equipment)
}

fn apply_authored_armor(equipment: &mut CombatEquipment, armor_ids: &[&str]) -> Result<(), String> {
    for (index, armor_id) in armor_ids.iter().enumerate() {
        let armor = crate::item_catalog::definition(armor_id)
            .ok_or_else(|| format!("unknown armor {armor_id}"))?;
        let ItemKind::Armor {
            coverage,
            resistance,
            padding,
            flexibility,
            range_of_motion,
            ..
        } = armor.kind
        else {
            continue;
        };
        let authored = armor
            .equipment
            .as_ref()
            .ok_or_else(|| format!("armor {armor_id} has no equipment projection"))?;
        let occurrence = armor_ids[..index]
            .iter()
            .filter(|prior| *prior == armor_id)
            .count();
        let placement = authored
            .placements
            .get(occurrence % authored.placements.len().max(1))
            .ok_or_else(|| format!("armor {armor_id} has no authored placement"))?;
        if placement.protection.is_empty() {
            return Err(format!("armor {armor_id} placement protects no body part"));
        }
        for authored_part in &placement.protection {
            let part = crate::equipment::equipment_body_part(*authored_part);
            let coverage_geometry =
                crate::combat::authored_armor_coverage(placement, part, coverage);
            equipment.armor[body_part_index(part)] = CombatArmor {
                inventory_item_id: Some(stable_id(&format!("{armor_id}:{occurrence}"))),
                material: authored.material,
                resistance,
                padding,
                flexibility,
                range_of_motion,
                coverage,
                coverage_span: Some(coverage_geometry.span),
                coverage_geometry: Some(coverage_geometry),
            };
        }
        equipment.inventory_weight += armor.weight_kg;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_is_varied_and_melee_only() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        assert_eq!(opponents.len(), 5);
        assert!(std::iter::once(&john).chain(&opponents).all(|build| {
            build.combatant.equipment.melee_weapon.is_some()
                && build.combatant.equipment.ranged_weapon.is_none()
        }));
        assert!(opponents.iter().any(|build| build.shield_id.is_some()));
        let hammer = opponents
            .iter()
            .find(|build| build.key == "hammer_brute")
            .unwrap()
            .combatant
            .equipment
            .melee_weapon
            .unwrap();
        let knife = opponents
            .iter()
            .find(|build| build.key == "knife_novice")
            .unwrap()
            .combatant
            .equipment
            .melee_weapon
            .unwrap();
        assert!(hammer.moment_of_inertia_kg_m2 > knife.moment_of_inertia_kg_m2);
        assert!(hammer.attack_interval_seconds > knife.attack_interval_seconds);
        assert_ne!(hammer.balance, knife.balance);
    }

    #[test]
    fn every_roster_weapon_uses_authored_physics_for_autoresolve_cadence_and_balance() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        for build in std::iter::once(john).chain(opponents) {
            let definition = crate::item_catalog::definition(build.weapon_id).unwrap();
            let ItemKind::Weapon {
                preferred_attack,
                moment_of_inertia_kg_m2,
                ..
            } = definition.kind
            else {
                panic!("{} is not an authored weapon", build.weapon_id);
            };
            let grip_to_tip_m = definition
                .equipment
                .as_ref()
                .unwrap()
                .physical
                .grip_to_tip_m;
            let projected = build.combatant.equipment.melee_weapon.unwrap();
            let expected_timing = crate::equipment::melee_attack_timing(
                preferred_attack,
                moment_of_inertia_kg_m2,
                false,
            );
            let expected_interval = EMBEDDED_AUTORESOLVE_PARAMETERS.melee_windup_seconds
                + expected_timing.recovery_secs;
            let expected_balance = crate::equipment::weapon_balance_from_moment(
                moment_of_inertia_kg_m2,
                definition.weight_kg,
                grip_to_tip_m,
            );
            assert!((projected.attack_interval_seconds - expected_interval).abs() < 1.0e-6);
            assert!((projected.balance - expected_balance).abs() < 1.0e-6);
        }
    }

    #[test]
    fn shared_dodge_projection_preserves_polearm_contact_time_and_tracking_advantage() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        let veteran = opponents
            .iter()
            .find(|build| build.key == "polearm_veteran")
            .unwrap();
        let dodge = DefenderResponse::Dodge { input_reflex: 0.2 };
        let john_against_halberd = melee_exchange(
            &veteran.combatant,
            &john.combatant,
            1.0,
            0.0,
            0.4,
            dodge,
            0.5,
            0.42,
        );
        let veteran_against_longsword = melee_exchange(
            &john.combatant,
            &veteran.combatant,
            1.0,
            0.0,
            0.4,
            dodge,
            0.5,
            0.42,
        );
        let john_clearance = john_against_halberd
            .dodge_geometry
            .unwrap()
            .closest_approach_metres;
        let veteran_clearance = veteran_against_longsword
            .dodge_geometry
            .unwrap()
            .closest_approach_metres;
        assert!(john_clearance > veteran_clearance);
        assert!(matches!(
            john_against_halberd.result,
            AttackResult::ToAttacker {
                physical_contact: false,
                ..
            }
        ));
        assert!(
            veteran_against_longsword
                .dodge_geometry
                .unwrap()
                .contacted_body_part
                .is_some()
        );
    }

    #[test]
    fn iteration_duels_make_terminal_progress() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        for opponent in opponents {
            let outcomes = || {
                (1..=32)
                    .map(|seed| {
                        resolve_battle(
                            vec![john.combatant.clone()],
                            vec![opponent.combatant.clone()],
                            seed,
                            BattleOpening::Normal,
                        )
                        .resolution
                    })
                    .collect::<Vec<_>>()
            };
            let first = outcomes();
            assert_eq!(first, outcomes(), "{} was not reproducible", opponent.name);
            assert!(
                first
                    .iter()
                    .all(|resolution| *resolution != BattleResolution::Timeout),
                "{} did not reach a victor",
                opponent.name
            );
        }
    }

    #[test]
    fn shorter_reach_combatants_land_contacts_across_bounded_property_seeds() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        for opponent in opponents {
            let john_reach = john.combatant.equipment.weapon_reach();
            let opponent_reach = opponent.combatant.equipment.weapon_reach();
            if (john_reach - opponent_reach).abs() <= f32::EPSILON {
                continue;
            }
            let shorter_id = if john_reach < opponent_reach {
                john.combatant.id
            } else {
                opponent.combatant.id
            };
            let landed = (1..=64).any(|seed| {
                resolve_battle(
                    vec![john.combatant.clone()],
                    vec![opponent.combatant.clone()],
                    seed,
                    BattleOpening::Normal,
                )
                .log
                .iter()
                .any(|entry| entry.attacker_id == shorter_id && entry.outcome.starts_with("hit"))
            });
            assert!(landed, "{} shorter-reach side never landed", opponent.name);
        }
    }

    #[test]
    fn polearm_contact_uses_only_movement_elapsed_before_contact() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        let veteran = opponents
            .iter()
            .find(|build| build.key == "polearm_veteran")
            .unwrap();
        let outcome = resolve_battle(
            vec![john.combatant],
            vec![veteran.combatant.clone()],
            1,
            BattleOpening::Normal,
        );
        for movement in outcome
            .timeline
            .iter()
            .filter(|event| event.kind == MeleeTimelineKind::Movement)
        {
            let elapsed = movement.movement_elapsed_seconds.unwrap();
            assert!(elapsed <= 1.0 / 64.0 + 1.0e-6);
            let displacement = movement.movement_displacement_metres.unwrap().abs();
            let velocity = movement
                .movement_velocity_before_metres_per_second
                .unwrap()
                .abs()
                .max(
                    movement
                        .movement_velocity_after_metres_per_second
                        .unwrap()
                        .abs(),
                );
            assert!(displacement <= velocity * elapsed + 1.0e-6);
        }
        let first_contact = outcome
            .timeline
            .iter()
            .find(|event| event.kind == MeleeTimelineKind::Contact)
            .unwrap();
        assert!(
            outcome.timeline[..first_contact.sequence as usize]
                .iter()
                .all(|event| event.time_seconds <= first_contact.time_seconds)
        );
        let first_log = outcome.log.first().unwrap();
        assert_eq!(
            first_log
                .melee_telemetry
                .as_ref()
                .unwrap()
                .actual_contact_measure_metres,
            first_contact.engagement_distance_before_metres.unwrap()
        );
    }

    #[test]
    fn authored_brigandine_and_jack_plate_surfaces_stop_longsword_edges() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        let attacker = &john.combatant;
        let attacker_equipment = attacker.equipment.for_melee();
        for key in ["hammer_brute", "polearm_veteran"] {
            let defender = &opponents
                .iter()
                .find(|build| build.key == key)
                .unwrap()
                .combatant;
            let defender_equipment = defender.equipment.for_melee();
            let surface = defender_equipment
                .armor_surface(BodyPart::Chest, 0.4)
                .expect("authored torso armor must expose its engaged surface");
            assert!(surface.material.is_some_and(|material| material.is_metal()));

            let result = resolve_melee_attack_by_parts(
                &attacker.skills,
                &attacker.attributes,
                &attacker.body,
                &attacker.essentials,
                &attacker_equipment,
                EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
                attacker_equipment.holding_side,
                attacker_equipment.weapon_preferred_melee_style(),
                1.0,
                0.0,
                1.0,
                MeleeContactLocation {
                    body_part: BodyPart::Chest,
                    anatomical_subregion: AnatomicalSubregion::ChestSternum,
                    surface_coordinate: 0.2,
                    armor_surface: Some(surface),
                },
                MeleeContactAtTime::intended(0.0),
                DefenderResponse::None,
                &defender.skills,
                &defender.attributes,
                &defender.body,
                &defender.essentials,
                &defender_equipment,
            );
            assert!(matches!(
                result,
                AttackResult::ToDefender {
                    cut_damage,
                    armor_impact: Some(ArmorImpact {
                        outcome: ArmorImpactOutcome::Stopped | ArmorImpactOutcome::Deflected,
                        ..
                    }),
                    ..
                } if cut_damage == 0.0
            ));
        }
    }

    #[test]
    fn acceptance_evidence_exercises_surface_gap_redirection_and_nonbinary_defense() {
        let evidence = melee_iteration_acceptance_evidence().unwrap();
        assert_eq!(evidence.armor_contacts.len(), 2);
        assert!(evidence.armor_contacts[0].armor_layer_chain[0].intersected);
        assert!(!evidence.armor_contacts[1].armor_layer_chain[0].intersected);
        assert!(matches!(
            evidence.armor_contacts[0].result,
            AttackResult::ToDefender {
                armor_impact: Some(_),
                ..
            }
        ));
        assert_eq!(evidence.mirrored_vambrace_contacts.len(), 4);
        let left_surface = &evidence.mirrored_vambrace_contacts[0];
        let right_surface = &evidence.mirrored_vambrace_contacts[1];
        let left_gap = &evidence.mirrored_vambrace_contacts[2];
        let right_gap = &evidence.mirrored_vambrace_contacts[3];
        assert_eq!(
            left_surface.layer.geometry.span,
            right_surface.layer.geometry.span
        );
        assert!(left_surface.layer.intersected && right_surface.layer.intersected);
        assert!(!left_gap.layer.intersected && !right_gap.layer.intersected);
        assert!(matches!(
            evidence.armor_contacts[1].result,
            AttackResult::ToDefender {
                armor_impact: None,
                ..
            }
        ));
        assert_ne!(
            evidence.partial_dodge.contacted_body_part,
            Some(evidence.partial_dodge.intended_body_part)
        );
        assert!(!evidence.defense_matrix[0].defended);
        assert!(evidence.defense_matrix[1].defended);
        assert_eq!(evidence.disabled_weapon_arm.len(), 2);
        assert!(
            evidence
                .disabled_weapon_arm
                .iter()
                .all(|entry| entry.capability == "disabled_weapon_arm" && !entry.attack_available)
        );
        assert_eq!(evidence.fatigue_cadence.medical_incapacitation, 0.0);
        assert!(
            evidence.fatigue_cadence.fatigued_recovery_seconds
                > evidence.fatigue_cadence.fresh_recovery_seconds
        );
        assert!(
            evidence
                .autoresolve_timeline
                .normalized_nonterminal_sequences_equal
        );
        assert_eq!(evidence.autoresolve_timeline.simultaneous_contacts.len(), 2);
        assert_eq!(
            evidence.autoresolve_timeline.simultaneous_contacts[0].simultaneous_batch_id,
            evidence.autoresolve_timeline.simultaneous_contacts[1].simultaneous_batch_id
        );
        assert!(
            evidence
                .autoresolve_timeline
                .polearm_opening_measure
                .iter()
                .any(|event| event.kind == MeleeTimelineKind::Movement)
        );
        assert!(!evidence.autoresolve_timeline.canceled_attack_ids.is_empty());
        assert!(
            evidence
                .autoresolve_timeline
                .canceled_attack_ids_that_contacted
                .is_empty()
        );
        assert!(evidence.all_weapon_contact_bands.iter().all(|contact| {
            contact.center_separation_metres >= HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES
                && (contact.transformed_energy_joules
                    - contact.incident_energy_joules * contact.energy_fraction)
                    .abs()
                    < 1.0e-4
                && contact.transformed_energy_joules <= contact.incident_energy_joules + 1.0e-4
        }));
        assert!(evidence.all_weapon_contact_bands.iter().any(|contact| {
            contact.weapon == "war_hammer"
                && contact.classification == MeleeContactClassification::Pommel
                && contact.energy_fraction < 0.1
        }));
        assert!(evidence.all_weapon_contact_bands.iter().any(|contact| {
            contact.weapon == "longsword"
                && contact.surface_measure_metres <= 0.000_1
                && contact.energy_fraction < 1.0
        }));
    }
}
