//! Framework-neutral corpse custody, decomposition, and autopsy observation rules.
//!
//! Corpse evidence is derived from committed strategic combat outcomes. It never
//! stores a canonical killer or cause-of-death answer: players must interpret
//! bounded physical findings through Surgery, Physiology, and learned Bestiary lore.

use crate::autoresolve::{
    BattleLogEntry, BattleOpening, BattleOutcome, Combatant, CombatantOutcome, resolve_battle,
};
use crate::prelude::BodyPart;
use adventuresim_world_schema::BASIS_POINTS_PER_WHOLE;
use serde::{Deserialize, Serialize};

pub const SCENE_MINUTES: u64 = 90;
pub const LOCAL_CUSTODY_MINUTES: u64 = 24 * 60;
pub const MAX_BODY_INJURIES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpseLocation {
    Scene,
    LocalCustody,
    Interred,
    Exhumed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecompositionBand {
    Fresh,
    Early,
    Advanced,
    Skeletal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyInjury {
    pub sequence: u32,
    pub region: BodyPart,
    pub cut_damage: f32,
    pub blunt_damage: f32,
    pub projectile: bool,
    pub contact_stress: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PostCombatBody {
    pub combatant_id: u64,
    pub health: [f32; 7],
    pub blood_loss_fraction: f32,
    pub injuries: Vec<BodyInjury>,
}

/// Durable, bounded systemic state captured at death. It contains no disease,
/// source, attacker, or canonical cause identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemicPathologySnapshot {
    pub respiratory_bps: u16,
    pub circulatory_bps: u16,
    pub homeostatic_bps: u16,
    pub neurologic_bps: u16,
    pub feverish: bool,
    pub air_hunger: bool,
    pub wasting: bool,
}

pub fn corpse_location(
    discovered_minute: u64,
    now_minute: u64,
    buried: bool,
    exhumed: bool,
) -> CorpseLocation {
    if exhumed {
        return CorpseLocation::Exhumed;
    }
    if buried {
        return CorpseLocation::Interred;
    }
    match now_minute.saturating_sub(discovered_minute) {
        0..SCENE_MINUTES => CorpseLocation::Scene,
        SCENE_MINUTES..LOCAL_CUSTODY_MINUTES => CorpseLocation::LocalCustody,
        _ => CorpseLocation::Interred,
    }
}

pub fn decomposition_band(
    death_minute: u64,
    now_minute: u64,
    handling_damage_bps: u16,
) -> DecompositionBand {
    let effective_age = now_minute
        .saturating_sub(death_minute)
        .saturating_add(u64::from(handling_damage_bps) * 2);
    match effective_age {
        0..720 => DecompositionBand::Fresh,
        720..2_880 => DecompositionBand::Early,
        2_880..20_160 => DecompositionBand::Advanced,
        _ => DecompositionBand::Skeletal,
    }
}

/// Persist only ordinary combat output for one defeated combatant. Attacker
/// identity and weapon instance are deliberately discarded at this boundary.
pub fn post_combat_body(combatant: &CombatantOutcome, log: &[BattleLogEntry]) -> PostCombatBody {
    let mut injuries = log
        .iter()
        .filter(|entry| {
            entry.defender_id == combatant.id
                && (entry.cut_damage > 0.0 || entry.blunt_damage > 0.0)
        })
        .map(|entry| BodyInjury {
            sequence: entry.sequence,
            region: entry.body_part,
            cut_damage: entry.cut_damage,
            blunt_damage: entry.blunt_damage,
            projectile: entry.projectile_kind.is_some(),
            contact_stress: entry.contact_stress,
        })
        .collect::<Vec<_>>();
    injuries.sort_by_key(|injury| injury.sequence);
    injuries.truncate(MAX_BODY_INJURIES);
    PostCombatBody {
        combatant_id: combatant.id,
        health: combatant.body.health,
        blood_loss_fraction: combatant.blood_loss_fraction.clamp(0.0, 1.0),
        injuries,
    }
}

/// A victim is a corpse only when the ordinary simulated body has a lethal
/// result. Incapacitation alone deliberately remains a surviving casualty.
pub fn is_lethal_body(combatant: &CombatantOutcome) -> bool {
    combatant.incapacitated
        && (combatant.body.health[4] <= 0.0
            || combatant.body.health[5] <= 0.0
            || combatant.body.health[6] <= 0.0
            || combatant.blood_loss_fraction >= 0.8)
}

/// Reusable seam for death-required incidents. It runs ordinary autoresolve
/// with a bounded deterministic sequence of seeds and fails cleanly if the
/// selected enemy survives every result. No wound or clue is authored directly.
pub fn resolve_death_required_incident(
    allies: &[Combatant],
    enemies: &[Combatant],
    victim_enemy_id: u64,
    base_seed: u64,
    max_attempts: u16,
) -> Option<BattleOutcome> {
    (0..max_attempts).find_map(|attempt| {
        let seed = base_seed.wrapping_add(u64::from(attempt));
        let outcome = resolve_battle(
            allies.to_vec(),
            enemies.to_vec(),
            seed,
            BattleOpening::Normal,
        );
        outcome
            .enemies
            .iter()
            .find(|enemy| enemy.id == victim_enemy_id)
            .is_some_and(is_lethal_body)
            .then_some(outcome)
    })
}

/// Internal examination precision. Low Surgery does not invent information;
/// it raises bounded iatrogenic obscuration that both medical windows must obey.
pub fn opening_quality_bps(surgery_check: f32, entropy_bps: u16) -> (u16, u16) {
    let skill =
        (surgery_check.clamp(0.0, 5.0) / 5.0 * f32::from(BASIS_POINTS_PER_WHOLE)).round() as u16;
    let obscuration = (u32::from(BASIS_POINTS_PER_WHOLE)
        .saturating_sub(u32::from(skill))
        .saturating_mul(3)
        / 5)
    .saturating_add(u32::from(entropy_bps.min(2_000)) / 4)
    .min(u32::from(BASIS_POINTS_PER_WHOLE)) as u16;
    (skill, obscuration)
}

#[derive(Clone, Copy, Debug)]
pub struct AutopsyEvidenceContext {
    pub decomposition: DecompositionBand,
    pub at_scene: bool,
    pub opening_obscuration_bps: u16,
}

fn evidence_quality_bps(skill_check: f32, context: AutopsyEvidenceContext, internal: bool) -> u16 {
    let skill = (skill_check.clamp(0.0, 5.0) * 2_000.0).round() as i32;
    let decomposition_penalty = match context.decomposition {
        DecompositionBand::Fresh => 0,
        DecompositionBand::Early => 1_500,
        DecompositionBand::Advanced => 4_500,
        DecompositionBand::Skeletal => 8_000,
    };
    let opening_penalty = if internal {
        i32::from(context.opening_obscuration_bps)
    } else {
        0
    };
    (skill - decomposition_penalty - opening_penalty)
        .clamp(0, i32::from(BASIS_POINTS_PER_WHOLE))
        .try_into()
        .unwrap_or(0)
}

fn region_label(region: BodyPart) -> &'static str {
    match region {
        BodyPart::LeftArm => "left arm",
        BodyPart::RightArm => "right arm",
        BodyPart::LeftLeg => "left leg",
        BodyPart::RightLeg => "right leg",
        BodyPart::Chest => "chest",
        BodyPart::Stomach => "abdomen",
        BodyPart::Head => "head",
    }
}

fn strongest_injury(injuries: &[BodyInjury]) -> Option<&BodyInjury> {
    injuries.iter().max_by(|left, right| {
        (left.cut_damage + left.blunt_damage).total_cmp(&(right.cut_damage + right.blunt_damage))
    })
}

/// Surgery reports bounded physical morphology, not its physiological effect.
pub fn surgery_finding(
    injuries: &[BodyInjury],
    skill_check: f32,
    context: AutopsyEvidenceContext,
    internal: bool,
) -> Option<String> {
    let quality = evidence_quality_bps(skill_check, context, internal);
    if quality < 1_500 {
        return None;
    }
    let injury = strongest_injury(injuries)?;
    let region = region_label(injury.region);
    let morphology = if injury.projectile {
        "a narrow penetrating track consistent with a hard projectile"
    } else if injury.cut_damage > injury.blunt_damage * 1.5 {
        if injury.contact_stress < 25.0 {
            "a relatively narrow edged wound with little surrounding crushing"
        } else {
            "a deep cutting wound with substantial compressed margins"
        }
    } else if injury.blunt_damage > injury.cut_damage * 1.5 {
        "broad crushing trauma from a heavy impact"
    } else {
        "mixed tearing and crushing trauma"
    };
    let depth = if internal {
        "Internal examination follows"
    } else {
        "External examination finds"
    };
    let scene = if context.at_scene && !internal {
        " At the undisturbed scene, blood distribution supports that the wound occurred here."
    } else {
        ""
    };
    let caveat = if quality < 5_000 {
        " Fine wound margins remain uncertain."
    } else {
        ""
    };
    Some(format!(
        "{depth} {morphology} at the {region}.{scene}{caveat}"
    ))
}

/// Physiology reports bounded systemic consequences from body state and never
/// assigns an instrument, attacker, or canonical cause of death.
pub fn physiology_finding(
    body: &PostCombatBody,
    skill_check: f32,
    context: AutopsyEvidenceContext,
    internal: bool,
) -> Option<String> {
    let quality = evidence_quality_bps(skill_check, context, internal);
    if quality < 1_500 {
        return None;
    }
    let (worst_index, worst_health) = body
        .health
        .iter()
        .copied()
        .enumerate()
        .min_by(|left, right| left.1.total_cmp(&right.1))?;
    let region = region_label(
        [
            BodyPart::LeftArm,
            BodyPart::RightArm,
            BodyPart::LeftLeg,
            BodyPart::RightLeg,
            BodyPart::Chest,
            BodyPart::Stomach,
            BodyPart::Head,
        ][worst_index],
    );
    let blood = body.blood_loss_fraction;
    let systemic = if blood >= 0.65 {
        "The remaining tissues show changes compatible with profound circulatory depletion"
    } else if worst_health <= 0.2 {
        "The regional damage is severe enough to have disrupted ordinary bodily function"
    } else {
        "The visible damage imposed substantial physiological stress"
    };
    let detail = if quality >= 5_000 {
        format!(
            "; the {region} retained roughly {:.0}% function and estimated blood loss is about {:.0}%",
            worst_health * 100.0,
            blood * 100.0
        )
    } else {
        format!("; the clearest dysfunction is around the {region}")
    };
    Some(format!("{systemic}{detail}."))
}

pub fn physiology_pathology_finding(
    pathology: &SystemicPathologySnapshot,
    skill_check: f32,
    context: AutopsyEvidenceContext,
    internal: bool,
) -> Option<String> {
    let quality = evidence_quality_bps(skill_check, context, internal);
    if quality < 2_500 {
        return None;
    }
    let systems = [
        (pathology.respiratory_bps, "respiratory"),
        (pathology.circulatory_bps, "circulatory"),
        (pathology.homeostatic_bps, "whole-body regulatory"),
        (pathology.neurologic_bps, "neurologic"),
    ];
    let (severity, system) = systems.into_iter().max_by_key(|(severity, _)| *severity)?;
    if severity < 1_000 {
        return None;
    }
    let accompanying = if pathology.air_hunger {
        " Signs of sustained air hunger accompany it."
    } else if pathology.feverish {
        " General heat-related tissue stress accompanies it."
    } else if pathology.wasting {
        " Long-running loss of tissue condition accompanies it."
    } else {
        ""
    };
    let caveat = if quality < 5_500 {
        " Decomposition prevents a narrower interpretation."
    } else {
        " This pattern supports several different illnesses or exposures."
    };
    Some(format!(
        "Systemic examination finds a pronounced {system} failure pattern.{accompanying}{caveat}"
    ))
}

/// Bestiary interprets an already-observed signature into broad candidates.
/// It deliberately receives no subject species or attacker identity.
pub fn bestiary_finding(
    injuries: &[BodyInjury],
    lore_check: f32,
    context: AutopsyEvidenceContext,
    internal: bool,
) -> Option<String> {
    let quality = evidence_quality_bps(lore_check, context, internal);
    if quality < 2_500 {
        return None;
    }
    let injury = strongest_injury(injuries)?;
    let candidates = if injury.projectile {
        "ranged weapon users or creatures capable of launching hard projectiles"
    } else if injury.blunt_damage > injury.cut_damage * 1.5 {
        "large, heavy striking threats or wielders of blunt weapons"
    } else if injury.cut_damage > injury.blunt_damage * 1.5 {
        "edged-weapon users or creatures with narrow sharp claws"
    } else {
        "threats capable of both tearing and forceful impact"
    };
    let support = if quality >= 7_500 {
        "strong"
    } else if quality >= 5_000 {
        "moderate"
    } else {
        "weak"
    };
    Some(format!(
        "Learned lore gives {support} support to {candidates}; the physical signs do not identify one culprit."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoresolve::{CombatBody, CombatProjectileKind};

    #[test]
    fn custody_is_dynamic_from_discovery_while_decomposition_uses_death() {
        assert_eq!(
            corpse_location(100, 150, false, false),
            CorpseLocation::Scene
        );
        assert_eq!(
            corpse_location(100, 300, false, false),
            CorpseLocation::LocalCustody
        );
        assert_eq!(
            corpse_location(100, 2_000, false, false),
            CorpseLocation::Interred
        );
        assert_eq!(
            corpse_location(100, 150, true, false),
            CorpseLocation::Interred
        );
        assert_eq!(
            corpse_location(100, 2_000, true, true),
            CorpseLocation::Exhumed
        );
        assert_eq!(decomposition_band(0, 1_000, 0), DecompositionBand::Early);
    }

    #[test]
    fn body_derivation_discards_attacker_and_weapon_identity() {
        let outcome = CombatantOutcome {
            id: 9,
            body: CombatBody::default(),
            blood_loss_fraction: 0.4,
            cut_damage: 0.2,
            incapacitated: true,
            yielded: false,
            incapacitation: 1.0,
            imbalance: 0.0,
            acute_trauma: 0.0,
            pain_incapacitation: 0.0,
            fatigue: 0.0,
            encumbrance: 0.0,
            wound_count: 0,
            open_wound_count: 0,
            internal_wound_count: 0,
            wound_flow_fraction_per_second: 0.0,
            ammunition_used: 0,
            terminal_cause: None,
        };
        let entry = BattleLogEntry {
            sequence: 3,
            phase: "melee".into(),
            round: 1,
            attacker_id: 77,
            defender_id: 9,
            attack_kind: crate::autoresolve::BattleAttackKind::Melee,
            weapon_inventory_item_id: Some(1234),
            defender_contact_item_id: None,
            defender_response: crate::autoresolve::MeleeResponseChoice::None,
            body_part: BodyPart::Head,
            outcome: crate::autoresolve::BattleAttackOutcome::HitHealth,
            health_damage: 0.3,
            cut_damage: 0.2,
            blunt_damage: 0.1,
            projectile_kind: Some(CombatProjectileKind::Arrowhead),
            contact_stress: 42.0,
            armor_impact: None,
            melee_telemetry: None,
        };
        let body = post_combat_body(&outcome, &[entry]);
        assert_eq!(body.combatant_id, 9);
        assert_eq!(body.injuries.len(), 1);
        assert!(body.injuries[0].projectile);
    }

    #[test]
    fn poor_opening_obscures_more_information() {
        assert!(opening_quality_bps(0.0, 0).1 > opening_quality_bps(5.0, 0).1);
    }

    #[test]
    fn incapacitation_without_lethal_anatomy_is_not_a_corpse() {
        let casualty = CombatantOutcome {
            id: 1,
            body: CombatBody::default(),
            blood_loss_fraction: 0.2,
            cut_damage: 0.0,
            incapacitated: true,
            yielded: false,
            incapacitation: 1.0,
            imbalance: 0.0,
            acute_trauma: 0.0,
            pain_incapacitation: 0.0,
            fatigue: 0.0,
            encumbrance: 0.0,
            wound_count: 0,
            open_wound_count: 0,
            internal_wound_count: 0,
            wound_flow_fraction_per_second: 0.0,
            ammunition_used: 0,
            terminal_cause: None,
        };
        assert!(!is_lethal_body(&casualty));
    }

    #[test]
    fn physical_signatures_produce_contrasting_bounded_findings() {
        let fresh = AutopsyEvidenceContext {
            decomposition: DecompositionBand::Fresh,
            at_scene: true,
            opening_obscuration_bps: 0,
        };
        let cut = BodyInjury {
            sequence: 0,
            region: BodyPart::Head,
            cut_damage: 0.5,
            blunt_damage: 0.05,
            projectile: false,
            contact_stress: 10.0,
        };
        let projectile = BodyInjury {
            sequence: 1,
            region: BodyPart::Chest,
            cut_damage: 0.2,
            blunt_damage: 0.05,
            projectile: true,
            contact_stress: 80.0,
        };
        assert!(
            surgery_finding(&[cut], 4.0, fresh, false)
                .unwrap()
                .contains("edged wound")
        );
        assert!(
            surgery_finding(&[projectile], 4.0, fresh, false)
                .unwrap()
                .contains("projectile")
        );
        assert!(
            bestiary_finding(&[cut], 4.0, fresh, false)
                .unwrap()
                .contains("sharp claws")
        );
        assert!(
            bestiary_finding(&[projectile], 4.0, fresh, false)
                .unwrap()
                .contains("ranged weapon users")
        );
    }

    #[test]
    fn decomposition_and_bad_opening_suppress_findings_without_inventing_answers() {
        let injury = BodyInjury {
            sequence: 0,
            region: BodyPart::Stomach,
            cut_damage: 0.1,
            blunt_damage: 0.6,
            projectile: false,
            contact_stress: 90.0,
        };
        let old = AutopsyEvidenceContext {
            decomposition: DecompositionBand::Skeletal,
            at_scene: false,
            opening_obscuration_bps: 0,
        };
        let obscured = AutopsyEvidenceContext {
            decomposition: DecompositionBand::Advanced,
            at_scene: false,
            opening_obscuration_bps: 5_000,
        };
        assert!(surgery_finding(&[injury], 3.0, old, false).is_none());
        assert!(surgery_finding(&[injury], 5.0, obscured, true).is_none());
        assert!(
            surgery_finding(
                &[injury],
                5.0,
                AutopsyEvidenceContext {
                    decomposition: DecompositionBand::Fresh,
                    at_scene: true,
                    opening_obscuration_bps: 0,
                },
                false
            )
            .unwrap()
            .contains("undisturbed scene")
        );
    }

    #[test]
    fn systemic_pathology_is_useful_without_becoming_a_diagnosis() {
        let finding = physiology_pathology_finding(
            &SystemicPathologySnapshot {
                respiratory_bps: 7_500,
                circulatory_bps: 1_000,
                homeostatic_bps: 2_000,
                neurologic_bps: 500,
                feverish: true,
                air_hunger: true,
                wasting: false,
            },
            4.0,
            AutopsyEvidenceContext {
                decomposition: DecompositionBand::Fresh,
                at_scene: true,
                opening_obscuration_bps: 0,
            },
            true,
        )
        .unwrap();
        assert!(finding.contains("respiratory"));
        assert!(finding.contains("several different illnesses or exposures"));
        for forbidden in ["influenza", "shroud", "source", "elemental"] {
            assert!(!finding.to_ascii_lowercase().contains(forbidden));
        }
    }
}
