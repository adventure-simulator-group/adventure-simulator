//! Framework-neutral corpse custody, decomposition, and autopsy observation rules.
//!
//! Corpse evidence is derived from committed strategic combat outcomes. It never
//! stores a canonical killer or cause-of-death answer: players must interpret
//! bounded physical findings through Surgery, Physiology, and learned Bestiary lore.

use crate::autoresolve::{
    BattleLogEntry, BattleOpening, BattleOutcome, Combatant, CombatantOutcome, resolve_battle,
};
use crate::prelude::BodyPart;

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

pub fn corpse_location(discovered_minute: u64, now_minute: u64, exhumed: bool) -> CorpseLocation {
    if exhumed {
        return CorpseLocation::Exhumed;
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
    let skill = (surgery_check.clamp(0.0, 5.0) / 5.0 * 10_000.0).round() as u16;
    let obscuration = (10_000_u32
        .saturating_sub(u32::from(skill))
        .saturating_mul(3)
        / 5)
    .saturating_add(u32::from(entropy_bps.min(2_000)) / 4)
    .min(10_000) as u16;
    (skill, obscuration)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoresolve::{CombatBody, CombatProjectileKind};

    #[test]
    fn custody_is_dynamic_from_discovery_while_decomposition_uses_death() {
        assert_eq!(corpse_location(100, 150, false), CorpseLocation::Scene);
        assert_eq!(
            corpse_location(100, 300, false),
            CorpseLocation::LocalCustody
        );
        assert_eq!(corpse_location(100, 2_000, false), CorpseLocation::Interred);
        assert_eq!(corpse_location(100, 2_000, true), CorpseLocation::Exhumed);
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
            ammunition_used: 0,
        };
        let entry = BattleLogEntry {
            sequence: 3,
            phase: "melee".into(),
            round: 1,
            attacker_id: 77,
            defender_id: 9,
            attack_kind: "melee".into(),
            weapon_inventory_item_id: Some(1234),
            defender_contact_item_id: None,
            body_part: BodyPart::Head,
            outcome: "hit".into(),
            health_damage: 0.3,
            cut_damage: 0.2,
            blunt_damage: 0.1,
            projectile_kind: Some(CombatProjectileKind::Arrowhead),
            contact_stress: 42.0,
            armor_contact: false,
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
            ammunition_used: 0,
        };
        assert!(!is_lethal_body(&casualty));
    }
}
