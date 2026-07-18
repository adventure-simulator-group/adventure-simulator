use std::collections::BTreeSet;

/// Retains only participants who are alive at the point a battle-related
/// benefit is committed. This is used both when creating participant records
/// and when distributing delayed loot.
pub fn living_participant_ids(recorded: &[u64], living: &[u64]) -> Vec<u64> {
    let living: BTreeSet<_> = living.iter().copied().collect();
    recorded
        .iter()
        .copied()
        .filter(|participant| living.contains(participant))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::living_participant_ids;

    #[test]
    fn dead_before_battle_is_not_eligible_for_participation_or_morale() {
        assert_eq!(living_participant_ids(&[10, 20], &[20]), vec![20]);
    }

    #[test]
    fn death_between_battle_and_loot_removes_the_recorded_participant() {
        assert_eq!(
            living_participant_ids(&[10, 20, 30], &[10, 30]),
            vec![10, 30]
        );
    }
}
