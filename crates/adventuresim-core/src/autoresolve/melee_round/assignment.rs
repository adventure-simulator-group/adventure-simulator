use super::*;

pub(in crate::autoresolve) fn melee_assignment(
    attacker_index: usize,
    attackers: &[Combatant],
    defenders: &[Combatant],
    parameters: crate::combat::AutoresolveParameters,
) -> (usize, f32) {
    let mut ordered = active_melee_indices(defenders);
    ordered.extend(active_ranged_indices(defenders));
    for index in active_indices(defenders) {
        if !ordered.contains(&index) {
            ordered.push(index);
        }
    }
    debug_assert!(!ordered.is_empty());
    let rank = attackers[..=attacker_index]
        .iter()
        .filter(|combatant| {
            !combatant.is_defeated()
                && combatant.can_attack_melee()
                && preferred_attack_mode(combatant) == AttackMode::Melee
        })
        .count()
        .saturating_sub(1);
    let target = ordered[rank % ordered.len()];
    (
        target,
        if rank >= ordered.len() {
            parameters.outnumbered_flanking
        } else {
            0.0
        },
    )
}

pub(in crate::autoresolve) fn active_indices(side: &[Combatant]) -> Vec<usize> {
    side.iter()
        .enumerate()
        .filter_map(|(index, combatant)| (!combatant.is_defeated()).then_some(index))
        .collect()
}

pub(in crate::autoresolve) fn active_melee_indices(side: &[Combatant]) -> Vec<usize> {
    side.iter()
        .enumerate()
        .filter_map(|(index, combatant)| {
            (!combatant.is_defeated()
                && combatant.can_attack_melee()
                && preferred_attack_mode(combatant) == AttackMode::Melee)
                .then_some(index)
        })
        .collect()
}

pub(in crate::autoresolve) fn active_ranged_indices(side: &[Combatant]) -> Vec<usize> {
    side.iter()
        .enumerate()
        .filter_map(|(index, combatant)| {
            (!combatant.is_defeated() && combatant.can_attack_ranged()).then_some(index)
        })
        .collect()
}

pub(in crate::autoresolve) fn prioritized_ranged_targets(side: &[Combatant]) -> Vec<usize> {
    let ranged = active_ranged_indices(side);
    if ranged.is_empty() {
        active_indices(side)
    } else {
        ranged
    }
}
