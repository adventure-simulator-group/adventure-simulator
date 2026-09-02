use super::*;

mod timeline;
use timeline::autoresolve_timeline_evidence;
mod armor;
use armor::{forced_armor_contacts, mirrored_vambrace_contacts};

#[derive(Clone, Debug, serde::Serialize)]
pub struct MeleeIterationAcceptanceEvidence {
    pub armor_contacts: Vec<ForcedArmorContactEvidence>,
    pub mirrored_vambrace_contacts: Vec<MirroredArmorContactEvidence>,
    pub defense_matrix: Vec<DefenseEvidence>,
    pub disabled_weapon_arm: Vec<DisabledWeaponArmEvidence>,
    pub fatigue_cadence: FatigueCadenceEvidence,
    pub autoresolve_timeline: AutoresolveTimelineEvidence,
    pub polearm_contact_revalidation: Vec<PolearmContactRevalidationEvidence>,
    pub all_weapon_contact_bands: Vec<WeaponContactBandEvidence>,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct WeaponContactBandEvidence {
    pub weapon: &'static str,
    pub surface_measure_metres: f32,
    pub center_separation_metres: f32,
    pub classification: MeleeContactClassification,
    pub lever_arm_metres: f32,
    pub contact_material: Option<crate::item_catalog_schema::EquipmentMaterial>,
    pub incident_energy_joules: f32,
    pub transformed_energy_joules: f32,
    pub energy_fraction: f32,
    pub invalidation_cause: Option<MeleeContactInvalidationCause>,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct PolearmContactRevalidationEvidence {
    pub scheduled_measure_metres: f32,
    pub actual_measure_metres: f32,
    pub classification: MeleeContactClassification,
    pub lever_arm_metres: f32,
    pub contact_material: Option<crate::item_catalog_schema::EquipmentMaterial>,
    pub incident_energy_joules: f32,
    pub transformed_energy_joules: f32,
    pub edge_contact: bool,
    pub invalidation_cause: Option<MeleeContactInvalidationCause>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct AutoresolveTimelineEvidence {
    pub side_swap_forward: Vec<MeleeTimelineEvent>,
    pub side_swap_reversed: Vec<MeleeTimelineEvent>,
    pub normalized_nonterminal_sequences_equal: bool,
    pub polearm_opening_measure: Vec<MeleeTimelineEvent>,
    pub simultaneous_contacts: Vec<MeleeTimelineEvent>,
    pub cancellation_sequence: Vec<MeleeTimelineEvent>,
    pub canceled_attack_ids: Vec<u64>,
    pub canceled_attack_ids_that_contacted: Vec<u64>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ForcedArmorContactEvidence {
    pub armor: &'static str,
    pub coverage_contact: &'static str,
    pub body_part: BodyPart,
    pub anatomical_subregion: AnatomicalSubregion,
    pub contact_surface_coordinate: f32,
    pub armor_layer_chain: Vec<ArmorLayerEvidence>,
    pub result: AttackResult,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ArmorLayerEvidence {
    pub item: &'static str,
    pub material: Option<crate::item_catalog_schema::EquipmentMaterial>,
    pub geometry: AuthoredArmorCoverage,
    pub intersected: bool,
    pub selected: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct MirroredArmorContactEvidence {
    pub side: BodySide,
    pub body_part: BodyPart,
    pub contact_surface_coordinate: f32,
    pub layer: ArmorLayerEvidence,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct DefenseEvidence {
    pub defender: &'static str,
    pub situation: &'static str,
    pub attack_value: f32,
    pub defended: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct DisabledWeaponArmEvidence {
    pub combatant: &'static str,
    pub disabled_arm: BodyPart,
    pub capability: &'static str,
    pub attack_available: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct FatigueCadenceEvidence {
    pub combatant: &'static str,
    pub completed_attacks: u32,
    pub completed_weapon_defenses: u32,
    pub completed_explosive_dodges: u32,
    pub fatigue: f32,
    pub fresh_recovery_seconds: f32,
    pub fatigued_recovery_seconds: f32,
}

pub fn melee_iteration_acceptance_evidence() -> Result<MeleeIterationAcceptanceEvidence, String> {
    let (john, opponents) = melee_iteration_roster()?;
    let opponent = |key| {
        opponents
            .iter()
            .find(|build| build.key == key)
            .ok_or_else(|| format!("missing {key}"))
    };
    let hammer = opponent("hammer_brute")?;
    let novice = opponent("knife_novice")?;
    let veteran = opponent("polearm_veteran")?;
    let militia = opponent("militia")?;
    let demi_lancer = opponent("demi_lancer")?;
    let armor_contacts = forced_armor_contacts(&john.combatant, &hammer.combatant)?;
    let defense_matrix = vec![
        defense_evidence(
            &john.combatant,
            &veteran.combatant,
            "Polearm Veteran",
            "late low-leverage parry",
            1.0,
            DefenderResponse::Parry {
                input_reflex: 0.05,
                precision: 0.05,
            },
        ),
        defense_evidence(
            &novice.combatant,
            &novice.combatant,
            "Knife Novice",
            "early aligned weapon block",
            0.05,
            DefenderResponse::Block { effectiveness: 1.0 },
        ),
    ];
    Ok(MeleeIterationAcceptanceEvidence {
        armor_contacts,
        mirrored_vambrace_contacts: mirrored_vambrace_contacts()?,
        defense_matrix,
        disabled_weapon_arm: [militia, demi_lancer]
            .map(disabled_weapon_arm_evidence)
            .into_iter()
            .collect(),
        fatigue_cadence: fatigue_cadence_evidence(&john)?,
        autoresolve_timeline: autoresolve_timeline_evidence(&john, veteran, hammer)?,
        polearm_contact_revalidation: polearm_contact_revalidation_evidence(veteran),
        all_weapon_contact_bands: all_weapon_contact_band_evidence(&john, hammer, veteran),
    })
}

fn all_weapon_contact_band_evidence(
    john: &MeleeIterationBuild,
    hammer: &MeleeIterationBuild,
    veteran: &MeleeIterationBuild,
) -> Vec<WeaponContactBandEvidence> {
    [
        (john, "longsword", 69.5),
        (hammer, "war_hammer", 76.788),
        (veteran, "halberd", 101.4),
    ]
    .into_iter()
    .flat_map(|(build, weapon, incident_energy_joules)| {
        let equipment = build.combatant.equipment.for_melee();
        let reach = melee_effective_reach(&build.combatant);
        let grip = equipment.weapon_grip_to_tip();
        let grip_origin = (reach - grip).max(0.0);
        let butt = (equipment.weapon_total_length() - grip).max(0.0);
        let head_boundary = grip_origin + (grip - equipment.weapon_striking_head_length()).max(0.0);
        [
            reach,
            (head_boundary + 0.000_1).min(reach),
            (head_boundary - 0.000_1).max(0.0),
            (grip_origin - butt * 0.5).max(0.0),
            0.0,
        ]
        .map(move |surface_measure_metres| {
            let ideal_measure_metres = preferred_melee_striking_measure(
                reach,
                grip,
                equipment.weapon_striking_head_length(),
                equipment.weapon.is_some_and(|weapon| weapon.distal_headed),
                EMBEDDED_AUTORESOLVE_PARAMETERS.melee_measure_reach_fraction,
            );
            let contact = resolve_melee_contact_at_time(MeleeContactAtTimeFacts {
                scheduled_measure_metres: reach,
                actual_measure_metres: surface_measure_metres,
                ideal_measure_metres,
                effective_reach_metres: reach,
                grip_to_tip_metres: grip,
                total_length_metres: equipment.weapon_total_length(),
                striking_head_length_metres: equipment.weapon_striking_head_length(),
                distal_headed: equipment.weapon.is_some_and(|weapon| weapon.distal_headed),
                attack_style: equipment.weapon_preferred_melee_style(),
                body_material: equipment.weapon_body_material(),
                striking_material: equipment.weapon_striking_material(),
            });
            WeaponContactBandEvidence {
                weapon,
                surface_measure_metres,
                center_separation_metres: surface_measure_metres
                    + HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES,
                classification: contact.classification,
                lever_arm_metres: contact.lever_arm_metres,
                contact_material: contact.contact_material,
                incident_energy_joules,
                transformed_energy_joules: incident_energy_joules * contact.energy_fraction,
                energy_fraction: contact.energy_fraction,
                invalidation_cause: contact.invalidation_cause,
            }
        })
    })
    .collect()
}

fn polearm_contact_revalidation_evidence(
    veteran: &MeleeIterationBuild,
) -> Vec<PolearmContactRevalidationEvidence> {
    const REVIEWED_INCIDENT_ENERGY_JOULES: f32 = 101.4;
    let equipment = veteran.combatant.equipment.for_melee();
    let effective_reach = melee_effective_reach(&veteran.combatant);
    let grip_to_tip = equipment.weapon_grip_to_tip();
    let grip_origin = (effective_reach - grip_to_tip).max(0.0);
    let head_boundary =
        grip_origin + (grip_to_tip - equipment.weapon_striking_head_length()).max(0.0);
    let ideal_measure_metres = preferred_melee_striking_measure(
        effective_reach,
        grip_to_tip,
        equipment.weapon_striking_head_length(),
        equipment.weapon.is_some_and(|weapon| weapon.distal_headed),
        EMBEDDED_AUTORESOLVE_PARAMETERS.melee_measure_reach_fraction,
    );
    [
        effective_reach,
        head_boundary,
        head_boundary - 0.000_01,
        1.25,
        (grip_origin - 0.05).max(0.0),
        effective_reach + 0.05,
    ]
    .map(|actual_measure_metres| {
        let contact = resolve_melee_contact_at_time(MeleeContactAtTimeFacts {
            scheduled_measure_metres: 2.0,
            actual_measure_metres,
            ideal_measure_metres,
            effective_reach_metres: effective_reach,
            grip_to_tip_metres: equipment.weapon_grip_to_tip(),
            total_length_metres: equipment.weapon_total_length(),
            striking_head_length_metres: equipment.weapon_striking_head_length(),
            distal_headed: equipment.weapon.is_some_and(|weapon| weapon.distal_headed),
            attack_style: equipment.weapon_preferred_melee_style(),
            body_material: equipment.weapon_body_material(),
            striking_material: equipment.weapon_striking_material(),
        });
        PolearmContactRevalidationEvidence {
            scheduled_measure_metres: contact.scheduled_measure_metres,
            actual_measure_metres: contact.actual_measure_metres,
            classification: contact.classification,
            lever_arm_metres: contact.lever_arm_metres,
            contact_material: contact.contact_material,
            incident_energy_joules: REVIEWED_INCIDENT_ENERGY_JOULES,
            transformed_energy_joules: REVIEWED_INCIDENT_ENERGY_JOULES * contact.energy_fraction,
            edge_contact: contact.classification == MeleeContactClassification::IntendedSurface,
            invalidation_cause: contact.invalidation_cause,
        }
    })
    .into_iter()
    .collect()
}

fn disabled_weapon_arm_evidence(build: &MeleeIterationBuild) -> DisabledWeaponArmEvidence {
    let mut body = build.combatant.body.clone();
    let disabled_arm = match build.combatant.equipment.melee_holding_side {
        BodySide::Left => BodyPart::LeftArm,
        BodySide::Right | BodySide::Both => BodyPart::RightArm,
    };
    body.health[body_part_index(disabled_arm)] = 0.0;
    let capability = melee_attack_capability(&body, &build.combatant.equipment);
    DisabledWeaponArmEvidence {
        combatant: build.name,
        disabled_arm,
        capability: match capability {
            MeleeAttackCapability::Available => "available",
            MeleeAttackCapability::DisabledWeaponArm { .. } => "disabled_weapon_arm",
            MeleeAttackCapability::NoStrikingSide => "no_striking_side",
        },
        attack_available: capability.is_available(),
    }
}

fn fatigue_cadence_evidence(build: &MeleeIterationBuild) -> Result<FatigueCadenceEvidence, String> {
    const ATTACKS: u32 = 15;
    const WEAPON_DEFENSES: u32 = 15;
    const DODGES: u32 = 1;
    let weapon = build
        .combatant
        .equipment
        .melee_weapon
        .ok_or("John has no melee weapon")?;
    let endurance = build.combatant.attributes.endurance;
    let workload = |work, seconds, mass, inertia| {
        combat_action_workload(
            work,
            seconds,
            mass,
            inertia,
            build.combatant.equipment.inventory_weight,
            build.combatant.body.weight_kg,
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS.fatigue,
        )
    };
    let attack = workload(
        CombatActionWork::Attack,
        0.75,
        weapon.weight,
        weapon.moment_of_inertia_kg_m2,
    );
    let defense = workload(
        CombatActionWork::WeaponDefense,
        0.5,
        weapon.weight,
        weapon.moment_of_inertia_kg_m2,
    );
    let dodge = workload(CombatActionWork::ExplosiveDodge, 0.5, 0.0, 0.0);
    let mut fatigue = 0.0;
    for workload in std::iter::repeat_n(attack, ATTACKS as usize)
        .chain(std::iter::repeat_n(defense, WEAPON_DEFENSES as usize))
        .chain(std::iter::repeat_n(dodge, DODGES as usize))
    {
        apply_combat_workload(
            &mut fatigue,
            workload,
            endurance,
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS.fatigue,
        );
    }
    let performance = combat_fatigue_performance(fatigue);
    let fresh_recovery_seconds = weapon.attack_interval_seconds;
    Ok(FatigueCadenceEvidence {
        combatant: build.name,
        completed_attacks: ATTACKS,
        completed_weapon_defenses: WEAPON_DEFENSES,
        completed_explosive_dodges: DODGES,
        fatigue,
        fresh_recovery_seconds,
        fatigued_recovery_seconds: fatigue_adjusted_recovery_seconds(
            fresh_recovery_seconds,
            performance,
        ),
    })
}

fn forced_melee_contact(
    attacker: &Combatant,
    defender: &Combatant,
    contact_surface_coordinate: f32,
    armor_surface: Option<crate::equipment::ArmorSurface>,
) -> AttackResult {
    let attacker_equipment = attacker.equipment.for_melee();
    let defender_equipment = defender.equipment.for_melee();
    resolve_melee_attack_by_parts(
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
            anatomical_subregion: anatomical_subregion(BodyPart::Chest, contact_surface_coordinate),
            surface_coordinate: contact_surface_coordinate,
            armor_surface,
        },
        MeleeContactAtTime::intended(0.0),
        DefenderResponse::None,
        &defender.skills,
        &defender.attributes,
        &defender.body,
        &defender.essentials,
        &defender_equipment,
    )
}

fn defense_evidence(
    attacker: &Combatant,
    defender: &Combatant,
    defender_name: &'static str,
    situation: &'static str,
    hit_precision: f32,
    response: DefenderResponse,
) -> DefenseEvidence {
    let attacker_equipment = attacker.equipment.for_melee();
    let attack_value = melee_attack_value_by_parts(
        &attacker.skills,
        &attacker.attributes,
        &attacker.body,
        &attacker.essentials,
        &attacker_equipment,
        attacker_equipment.holding_side,
        attacker_equipment.weapon_preferred_melee_style(),
        hit_precision,
        0.0,
        response,
        &defender.skills,
        &defender.attributes,
        &defender.body,
        &defender.essentials,
        &defender.equipment,
    );
    DefenseEvidence {
        defender: defender_name,
        situation,
        attack_value,
        defended: attack_value < 0.0,
    }
}
