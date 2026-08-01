//! Deterministic strategic random-encounter selection.
//!
//! Rolls live on canonical movement-minute boundaries, so reducer chunking and
//! retries cannot change which journey/seed encounters a party. Mount support
//! is intentionally represented as a speed input; absent mounts are neutral.

use crate::bestiary::{ActivityTime, Habitat, ThreatId, select_habitat_relation};

pub const ENCOUNTER_ROLL_INTERVAL_MINUTES: u64 = 180;
pub const BASE_ENCOUNTER_BASIS_POINTS: u32 = 180;
pub const QUEST_PROXIMITY_MINUTES: u64 = 120;
pub const QUEST_ENCOUNTER_BONUS_BASIS_POINTS: u32 = 180;
pub const QUEST_ARCHETYPE_WEIGHT_BONUS: u32 = 240;
/// Added by the strategic layer to enemy awareness for encounters at night.
pub const NIGHT_ENEMY_AWARENESS_BONUS: u16 = 150;
pub const SURRENDER_MINIMUM_ITEM_VALUE: u32 = 20;
pub const BANDIT_SURRENDER_THRESHOLD_PERCENT: u8 = 45;
pub const PARTY_WALKING_SPEED_M_PER_MINUTE: u32 = 83;
pub const MIN_ENCUMBRANCE_SPEED_BASIS_POINTS: u32 = 1_000;
pub const PARTY_MEMBER_LOGISTICS_PENALTY_BASIS_POINTS: u32 = 250;
pub const NARRATIVE_TRAVEL_INTERVAL_MINUTES: u64 = 240;
pub const NARRATIVE_REST_INTERVAL_MINUTES: u64 = 180;
pub const NARRATIVE_TRAVEL_CHANCE_BPS: u16 = 900;
pub const NARRATIVE_REST_CHANCE_BPS: u16 = 1_200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncounterTerrain {
    Road,
    Open,
    SparseWoods,
    DeepWoods,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncounterArchetype {
    Bandits,
    Goblins,
    Undead,
}

/// Influence from the party's accepted active quest at an encounter boundary.
///
/// `distance_minutes` is the remaining travel time to that quest's destination;
/// influence decays linearly to zero at [`QUEST_PROXIMITY_MINUTES`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcceptedQuestInfluence {
    pub archetype: EncounterArchetype,
    pub distance_minutes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalProblemInfluence {
    pub frequency_bonus_basis_points: u16,
    pub archetype: Option<EncounterArchetype>,
}

impl EncounterArchetype {
    pub const fn threat_id(self) -> ThreatId {
        match self {
            Self::Bandits => ThreatId::Bandit,
            Self::Goblins => ThreatId::Goblin,
            Self::Undead => ThreatId::Skeleton,
        }
    }

    pub fn enemy_speed_m_per_minute(self) -> u32 {
        self.threat_id().profile().combat.speed_m_per_minute
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Awareness {
    Neither,
    PartyOnly,
    EnemyOnly,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncounterChoice {
    Sneak,
    Detour,
    Attack,
    Run,
    Surrender,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncounterContext {
    pub terrain: EncounterTerrain,
    pub night: bool,
    /// Only the party's accepted active quest may contribute this influence.
    pub accepted_active_quest: Option<AcceptedQuestInfluence>,
    pub combat_capable_members: u16,
    pub party_awareness: u16,
    pub enemy_awareness: u16,
    pub party_speed_m_per_minute: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncounterSelection {
    pub boundary_minute: u64,
    pub roll_index: u64,
    pub archetype: EncounterArchetype,
    pub count: u16,
    pub awareness: Awareness,
    pub party_roll: u16,
    pub enemy_roll: u16,
}

pub const fn penalty_minutes(terrain: EncounterTerrain, choice: EncounterChoice) -> u64 {
    match (terrain, choice) {
        (EncounterTerrain::Road, EncounterChoice::Detour) => 30,
        (EncounterTerrain::Open, EncounterChoice::Detour) => 45,
        (EncounterTerrain::SparseWoods, EncounterChoice::Detour) => 60,
        (EncounterTerrain::DeepWoods, EncounterChoice::Detour) => 90,
        (EncounterTerrain::Road, EncounterChoice::Run) => 20,
        (EncounterTerrain::Open, EncounterChoice::Run) => 30,
        (EncounterTerrain::SparseWoods, EncounterChoice::Run) => 45,
        (EncounterTerrain::DeepWoods, EncounterChoice::Run) => 60,
        _ => 0,
    }
}

pub fn run_is_eligible(party_speed: u32, archetype: EncounterArchetype) -> bool {
    party_speed > archetype.enemy_speed_m_per_minute()
}

/// Sustainable overland speed. Mounts are deliberately neutral until the
/// strategic layer models them; fatigue, aggregate encumbrance, party size,
/// and the interruption terrain are all authoritative inputs.
pub fn sustainable_speed_m_per_minute(
    fatigue_percent: u8,
    encumbrance_remaining_basis_points: u32,
    party_size: u16,
    terrain: EncounterTerrain,
) -> u32 {
    let fatigue = 10_000_u32.saturating_sub(u32::from(fatigue_percent) * 50);
    let encumbrance = if encumbrance_remaining_basis_points < MIN_ENCUMBRANCE_SPEED_BASIS_POINTS {
        MIN_ENCUMBRANCE_SPEED_BASIS_POINTS
    } else if encumbrance_remaining_basis_points > 10_000 {
        10_000
    } else {
        encumbrance_remaining_basis_points
    };
    let logistics = 10_000_u32
        .saturating_sub(
            u32::from(party_size.saturating_sub(1)) * PARTY_MEMBER_LOGISTICS_PENALTY_BASIS_POINTS,
        )
        .max(7_000);
    let terrain: u32 = match terrain {
        EncounterTerrain::Road => 10_000,
        EncounterTerrain::Open => 9_000,
        EncounterTerrain::SparseWoods => 8_000,
        EncounterTerrain::DeepWoods => 6_500,
    };
    let scaled = u64::from(PARTY_WALKING_SPEED_M_PER_MINUTE)
        * u64::from(fatigue)
        * u64::from(encumbrance)
        * u64::from(logistics)
        * u64::from(terrain);
    (scaled / 10_000_u64.pow(4)).max(1) as u32
}

pub fn available_choices(
    awareness: Awareness,
    archetype: EncounterArchetype,
    party_speed: u32,
) -> Vec<EncounterChoice> {
    let mut choices = match awareness {
        Awareness::Neither => Vec::new(),
        Awareness::PartyOnly => vec![
            EncounterChoice::Sneak,
            EncounterChoice::Detour,
            EncounterChoice::Attack,
        ],
        Awareness::EnemyOnly | Awareness::Both => vec![EncounterChoice::Attack],
    };
    if awareness == Awareness::Both && run_is_eligible(party_speed, archetype) {
        choices.push(EncounterChoice::Run);
    }
    if matches!(awareness, Awareness::EnemyOnly | Awareness::Both)
        && archetype == EncounterArchetype::Bandits
    {
        choices.push(EncounterChoice::Surrender);
    }
    choices
}

/// Returns the first encounter in `(completed, completed + requested]`.
pub fn first_encounter(
    seed: u64,
    completed: u64,
    requested: u64,
    context_at: impl FnMut(u64) -> EncounterContext,
) -> Option<EncounterSelection> {
    first_encounter_with_problem(seed, completed, requested, context_at, |_| None)
}

/// As [`first_encounter`], with a private influence sampled at the same
/// canonical boundary. Existing entropy domains and boundary traversal remain
/// unchanged, preserving retry and chunk invariance.
pub fn first_encounter_with_problem(
    seed: u64,
    completed: u64,
    requested: u64,
    mut context_at: impl FnMut(u64) -> EncounterContext,
    mut problem_at: impl FnMut(u64) -> Option<LocalProblemInfluence>,
) -> Option<EncounterSelection> {
    let end = completed.saturating_add(requested);
    let first_index = completed / ENCOUNTER_ROLL_INTERVAL_MINUTES + 1;
    let last_index = end / ENCOUNTER_ROLL_INTERVAL_MINUTES;
    (first_index..=last_index).find_map(|index| {
        let minute = index * ENCOUNTER_ROLL_INTERVAL_MINUTES;
        select_at_with_problem(seed, index, minute, context_at(minute), problem_at(minute))
    })
}

pub fn select_at(
    seed: u64,
    index: u64,
    minute: u64,
    context: EncounterContext,
) -> Option<EncounterSelection> {
    select_at_with_problem(seed, index, minute, context, None)
}

fn select_at_with_problem(
    seed: u64,
    index: u64,
    minute: u64,
    context: EncounterContext,
    problem: Option<LocalProblemInfluence>,
) -> Option<EncounterSelection> {
    let quest_strength = context.accepted_active_quest.map_or(0, |quest| {
        QUEST_PROXIMITY_MINUTES.saturating_sub(quest.distance_minutes.min(QUEST_PROXIMITY_MINUTES))
    });
    let quest_frequency_bonus =
        u64::from(QUEST_ENCOUNTER_BONUS_BASIS_POINTS) * quest_strength / QUEST_PROXIMITY_MINUTES;
    let frequency = BASE_ENCOUNTER_BASIS_POINTS
        + match context.terrain {
            EncounterTerrain::Road => 0,
            EncounterTerrain::Open => 35,
            EncounterTerrain::SparseWoods => 75,
            EncounterTerrain::DeepWoods => 130,
        }
        + if context.night { 90 } else { 0 }
        + quest_frequency_bonus as u32
        + problem.map_or(0, |value| u32::from(value.frequency_bonus_basis_points));
    if domain_roll(seed, index, 0) % 10_000 >= u64::from(frequency) {
        return None;
    }

    let habitat = match context.terrain {
        EncounterTerrain::Road => Habitat::Road,
        EncounterTerrain::Open => Habitat::Open,
        EncounterTerrain::SparseWoods => Habitat::SparseWoods,
        EncounterTerrain::DeepWoods => Habitat::DeepWoods,
    };
    // Profiles own the forward context likelihood and keep ecological prior
    // separate from curation pressure. The roll domains below are unchanged.
    let mut weights = [
        encounter_weight(ThreatId::Bandit, habitat, context.night),
        encounter_weight(ThreatId::Goblin, habitat, context.night),
        encounter_weight(ThreatId::Skeleton, habitat, context.night),
    ];
    if let Some(quest) = context.accepted_active_quest {
        let bonus = (u64::from(QUEST_ARCHETYPE_WEIGHT_BONUS) * quest_strength
            / QUEST_PROXIMITY_MINUTES) as u32;
        let slot = match quest.archetype {
            EncounterArchetype::Bandits => 0,
            EncounterArchetype::Goblins => 1,
            EncounterArchetype::Undead => 2,
        };
        if weights[slot] > 0 {
            weights[slot] = weights[slot].saturating_add(bonus);
        }
    }
    if let Some(problem) = problem
        && let Some(archetype) = problem.archetype
    {
        let slot = match archetype {
            EncounterArchetype::Bandits => 0,
            EncounterArchetype::Goblins => 1,
            EncounterArchetype::Undead => 2,
        };
        if weights[slot] > 0 {
            weights[slot] =
                weights[slot].saturating_add(u32::from(problem.frequency_bonus_basis_points));
        }
    }
    let archetype = select_archetype_from_weights(domain_roll(seed, index, 1), weights)?;
    let count = scale_enemy_count(
        enemy_count(seed, index, context.combat_capable_members),
        archetype,
    );
    let party_roll = (domain_roll(seed, index, 3) % 1000) as u16;
    let enemy_roll = (domain_roll(seed, index, 4) % 1000) as u16;
    let awareness = awareness_from_rolls(
        party_roll,
        enemy_roll,
        EncounterContext {
            enemy_awareness: context
                .enemy_awareness
                .saturating_add(u16::from(archetype.threat_id().profile().combat.perception)),
            ..context
        },
    );
    (awareness != Awareness::Neither).then_some(EncounterSelection {
        boundary_minute: minute,
        roll_index: index,
        archetype,
        count,
        awareness,
        party_roll,
        enemy_roll,
    })
}

fn select_archetype_from_weights(roll: u64, weights: [u32; 3]) -> Option<EncounterArchetype> {
    let total = weights.iter().copied().fold(0_u32, u32::saturating_add);
    if total == 0 {
        return None;
    }
    let pick = roll % u64::from(total);
    Some(if pick < u64::from(weights[0]) {
        EncounterArchetype::Bandits
    } else if pick < u64::from(weights[0] + weights[1]) {
        EncounterArchetype::Goblins
    } else {
        EncounterArchetype::Undead
    })
}

fn encounter_weight(id: ThreatId, habitat: Habitat, night: bool) -> u32 {
    let profile = id.profile();
    let Ok(relation) = select_habitat_relation(id, habitat, None) else {
        return 0;
    };
    let habitat = u32::from(relation.weight);
    let activity = match (profile.investigation.activity, night) {
        (ActivityTime::Night, true) | (ActivityTime::Day, false) | (ActivityTime::Any, _) => 100,
        _ => 20,
    };
    (u32::from(profile.base_weight) * habitat * activity / 10_000)
        .saturating_add(u32::from(profile.curation_weight) / 10)
}

/// Returns the deterministic enemy count for an encounter roll.
///
/// This is exposed separately so a strategic interruption can recompute the
/// count from the authoritative party membership at the encounter boundary.
pub fn enemy_count(seed: u64, index: u64, combat_capable_members: u16) -> u16 {
    let capable = combat_capable_members.max(1);
    let spread = (capable / 2).max(1);
    capable.saturating_add((domain_roll(seed, index, 2) % u64::from(spread + 1)) as u16)
}

pub fn scale_enemy_count(count: u16, archetype: EncounterArchetype) -> u16 {
    let basis_points = u32::from(
        archetype
            .threat_id()
            .profile()
            .combat
            .encounter_scale_basis_points,
    );
    ((u32::from(count) * basis_points).div_ceil(10_000))
        .max(1)
        .min(u32::from(u16::MAX)) as u16
}

pub const fn awareness_from_rolls(
    party_roll: u16,
    enemy_roll: u16,
    context: EncounterContext,
) -> Awareness {
    match (
        party_roll.saturating_add(context.party_awareness) >= 500,
        enemy_roll.saturating_add(context.enemy_awareness) >= 500,
    ) {
        (false, false) => Awareness::Neither,
        (true, false) => Awareness::PartyOnly,
        (false, true) => Awareness::EnemyOnly,
        (true, true) => Awareness::Both,
    }
}

/// A separate domain ensures the second whole-party sneak check cannot perturb
/// selection, enemy awareness, or later journey rolls.
pub fn sneak_succeeds(
    seed: u64,
    roll_index: u64,
    party_stealth: u16,
    enemy_awareness: u16,
) -> bool {
    (domain_roll(seed, roll_index, 5) % 1000) as u16 + party_stealth
        > (domain_roll(seed, roll_index, 6) % 1000) as u16 + enemy_awareness
}

fn domain_roll(seed: u64, index: u64, domain: u64) -> u64 {
    let mut value = seed
        ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ domain.wrapping_mul(0xd6e8_feb8_6659_fd93);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Durable context for a goal-neutral narrative interruption roll. This uses
/// entropy domains disjoint from tactical/combat encounter selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NarrativeBoundaryKind {
    Travel,
    Rest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NarrativeContext {
    pub kind: NarrativeBoundaryKind,
    pub in_settlement: bool,
    pub another_interruption_pending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NarrativeSelection {
    pub boundary_minute: u64,
    pub roll_index: u64,
    pub catalog_id: String,
}

pub const fn next_combat_roll_after_reached_boundary(boundary_minute: u64) -> u64 {
    boundary_minute / ENCOUNTER_ROLL_INTERVAL_MINUTES + 1
}

/// Returns the first narrative boundary in `(completed, completed + requested]`.
/// `completed` is the journey movement cursor for travel and the durable
/// accumulated camp-rest cursor for rest, so splitting a reducer call cannot
/// introduce another roll.
pub fn first_narrative_encounter(
    seed: u64,
    completed: u64,
    requested: u64,
    context: NarrativeContext,
) -> Option<NarrativeSelection> {
    if context.in_settlement || context.another_interruption_pending {
        return None;
    }
    let interval = match context.kind {
        NarrativeBoundaryKind::Travel => NARRATIVE_TRAVEL_INTERVAL_MINUTES,
        NarrativeBoundaryKind::Rest => NARRATIVE_REST_INTERVAL_MINUTES,
    };
    let first = completed / interval + 1;
    let last = completed.saturating_add(requested) / interval;
    (first..=last).find_map(|index| narrative_selection_at(seed, index, index * interval, context))
}

pub fn narrative_selection_at(
    seed: u64,
    index: u64,
    boundary_minute: u64,
    context: NarrativeContext,
) -> Option<NarrativeSelection> {
    if context.in_settlement || context.another_interruption_pending {
        return None;
    }
    let chance = match context.kind {
        NarrativeBoundaryKind::Travel => NARRATIVE_TRAVEL_CHANCE_BPS,
        NarrativeBoundaryKind::Rest => NARRATIVE_REST_CHANCE_BPS,
    };
    // Domains 100+ are reserved for narrative encounters and cannot perturb
    // the combat selector's domains 0..=6.
    if domain_roll(seed, index, 100 + context.kind as u64) % 10_000 >= u64::from(chance) {
        return None;
    }
    let candidates: Vec<_> = crate::road_encounter_catalog::definitions()
        .iter()
        .filter(|definition| match context.kind {
            NarrativeBoundaryKind::Travel => definition.triggers.travel,
            NarrativeBoundaryKind::Rest => definition.triggers.rest,
        })
        .collect();
    let total = candidates
        .iter()
        .map(|definition| u64::from(definition.weight))
        .sum::<u64>();
    if total == 0 {
        return None;
    }
    let mut pick = domain_roll(seed, index, 110 + context.kind as u64) % total;
    for definition in candidates {
        if pick < u64::from(definition.weight) {
            return Some(NarrativeSelection {
                boundary_minute,
                roll_index: index,
                catalog_id: definition.id.clone(),
            });
        }
        pick -= u64::from(definition.weight);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> EncounterContext {
        EncounterContext {
            terrain: EncounterTerrain::Road,
            night: false,
            accepted_active_quest: None,
            combat_capable_members: 4,
            party_awareness: 500,
            enemy_awareness: 500,
            party_speed_m_per_minute: PARTY_WALKING_SPEED_M_PER_MINUTE,
        }
    }

    fn narrative(kind: NarrativeBoundaryKind) -> NarrativeContext {
        NarrativeContext {
            kind,
            in_settlement: false,
            another_interruption_pending: false,
        }
    }

    #[test]
    fn narrative_travel_and_rest_are_chunk_and_retry_invariant() {
        for kind in [NarrativeBoundaryKind::Travel, NarrativeBoundaryKind::Rest] {
            let whole = first_narrative_encounter(88, 0, 7_200, narrative(kind));
            assert_eq!(
                whole,
                first_narrative_encounter(88, 0, 7_200, narrative(kind))
            );
            let interval = match kind {
                NarrativeBoundaryKind::Travel => NARRATIVE_TRAVEL_INTERVAL_MINUTES,
                NarrativeBoundaryKind::Rest => NARRATIVE_REST_INTERVAL_MINUTES,
            };
            let by_boundaries = (1..=7_200 / interval).find_map(|index| {
                narrative_selection_at(88, index, index * interval, narrative(kind))
            });
            assert_eq!(whole, by_boundaries);
        }
    }

    #[test]
    fn narrative_interruption_advances_only_past_its_reached_combat_boundary() {
        let requested_end = ENCOUNTER_ROLL_INTERVAL_MINUTES * 20;
        let narrative_boundary = ENCOUNTER_ROLL_INTERVAL_MINUTES * 3;
        assert_eq!(
            next_combat_roll_after_reached_boundary(narrative_boundary),
            4
        );
        assert_ne!(
            next_combat_roll_after_reached_boundary(narrative_boundary),
            next_combat_roll_after_reached_boundary(requested_end),
        );
    }

    #[test]
    fn narrative_never_selects_in_settlement_or_over_an_interruption() {
        for seed in 0..10_000 {
            assert!(
                narrative_selection_at(
                    seed,
                    1,
                    240,
                    NarrativeContext {
                        in_settlement: true,
                        ..narrative(NarrativeBoundaryKind::Travel)
                    }
                )
                .is_none()
            );
            assert!(
                narrative_selection_at(
                    seed,
                    1,
                    240,
                    NarrativeContext {
                        another_interruption_pending: true,
                        ..narrative(NarrativeBoundaryKind::Travel)
                    }
                )
                .is_none()
            );
        }
    }

    #[test]
    fn chunks_and_retries_are_invariant() {
        let all: Vec<_> = (1..=20)
            .filter_map(|i| select_at(91, i, i * ENCOUNTER_ROLL_INTERVAL_MINUTES, context()))
            .collect();
        let chunked: Vec<_> = [0, 720, 1440, 2160, 2880]
            .windows(2)
            .flat_map(|w| {
                let mut found = Vec::new();
                let mut at = w[0];
                while let Some(e) = first_encounter(91, at, w[1] - at, |_| context()) {
                    at = e.boundary_minute;
                    found.push(e);
                }
                found
            })
            .collect();
        assert_eq!(
            all.into_iter()
                .filter(|e| e.boundary_minute <= 2880)
                .collect::<Vec<_>>(),
            chunked
        );
        assert_eq!(
            first_encounter(91, 0, 2880, |_| context()),
            first_encounter(91, 0, 2880, |_| context())
        );
    }

    #[test]
    fn baseline_is_low_and_modifiers_raise_frequency() {
        let count = |mut c: EncounterContext| {
            (0..10_000)
                .filter(|i| {
                    c.combat_capable_members = 4;
                    select_at(*i, 1, 180, c).is_some()
                })
                .count()
        };
        let base = count(context());
        let mut risky = context();
        risky.terrain = EncounterTerrain::DeepWoods;
        risky.night = true;
        risky.accepted_active_quest = Some(AcceptedQuestInfluence {
            archetype: EncounterArchetype::Bandits,
            distance_minutes: 0,
        });
        assert!(base < 300);
        assert!(count(risky) > base);
    }

    #[test]
    fn accepted_quest_frequency_influence_decays_with_distance() {
        let count = |accepted_active_quest| {
            let c = EncounterContext {
                accepted_active_quest,
                ..context()
            };
            (0..100_000)
                .filter(|seed| select_at(*seed, 1, 180, c).is_some())
                .count()
        };
        let quest_at = |distance_minutes| {
            Some(AcceptedQuestInfluence {
                archetype: EncounterArchetype::Undead,
                distance_minutes,
            })
        };

        let none = count(None);
        let far = count(quest_at(QUEST_PROXIMITY_MINUTES / 2));
        let near = count(quest_at(0));
        assert!(near > far, "near={near}, far={far}");
        assert!(far > none, "far={far}, none={none}");
        assert_eq!(count(quest_at(QUEST_PROXIMITY_MINUTES)), none);
    }

    #[test]
    fn accepted_quest_strongly_favors_its_matching_archetype() {
        let archetype_counts = |accepted_active_quest| {
            let c = EncounterContext {
                accepted_active_quest,
                terrain: EncounterTerrain::DeepWoods,
                ..context()
            };
            let mut counts = [0_usize; 3];
            for seed in 0..100_000 {
                if let Some(selection) = select_at(seed, 1, 180, c) {
                    counts[selection.archetype as usize] += 1;
                }
            }
            counts
        };
        let baseline = archetype_counts(None);
        let undead_quest = archetype_counts(Some(AcceptedQuestInfluence {
            archetype: EncounterArchetype::Undead,
            distance_minutes: 0,
        }));
        let baseline_total: usize = baseline.iter().sum();
        let quest_total: usize = undead_quest.iter().sum();

        assert!(
            undead_quest[EncounterArchetype::Undead as usize] * baseline_total
                > baseline[EncounterArchetype::Undead as usize] * quest_total * 2,
            "baseline={baseline:?}, undead quest={undead_quest:?}"
        );
    }

    #[test]
    fn enemy_count_is_stable_and_scales_from_boundary_membership() {
        assert_eq!(enemy_count(42, 7, 4), enemy_count(42, 7, 4));
        assert!((4..=6).contains(&enemy_count(42, 7, 4)));
        assert!((8..=12).contains(&enemy_count(42, 7, 8)));
    }

    #[test]
    fn all_awareness_states_and_independent_rolls_exist() {
        let mut seen = [false; 4];
        for seed in 0..100_000 {
            if let Some(e) = select_at(
                seed,
                1,
                180,
                EncounterContext {
                    party_awareness: 0,
                    enemy_awareness: 0,
                    ..context()
                },
            ) {
                seen[e.awareness as usize] = true;
            }
        }
        // Neither is intentionally filtered because it does not interrupt.
        assert!(!seen[0]);
        assert!(seen[1..].iter().all(|v| *v));
    }

    #[test]
    fn speed_and_terrain_penalties_are_explicit() {
        assert!(run_is_eligible(83, EncounterArchetype::Bandits));
        assert!(!run_is_eligible(83, EncounterArchetype::Goblins));
        assert!(
            penalty_minutes(EncounterTerrain::DeepWoods, EncounterChoice::Detour)
                > penalty_minutes(EncounterTerrain::Road, EncounterChoice::Detour)
        );
    }

    #[test]
    fn awareness_and_choice_contract_covers_all_four_outcomes() {
        let c = EncounterContext {
            party_awareness: 0,
            enemy_awareness: 0,
            ..context()
        };
        assert_eq!(awareness_from_rolls(0, 0, c), Awareness::Neither);
        assert_eq!(awareness_from_rolls(500, 0, c), Awareness::PartyOnly);
        assert_eq!(awareness_from_rolls(0, 500, c), Awareness::EnemyOnly);
        assert_eq!(awareness_from_rolls(500, 500, c), Awareness::Both);
        assert!(available_choices(Awareness::Neither, EncounterArchetype::Bandits, 100).is_empty());
        assert_eq!(
            available_choices(Awareness::PartyOnly, EncounterArchetype::Bandits, 100),
            vec![
                EncounterChoice::Sneak,
                EncounterChoice::Detour,
                EncounterChoice::Attack
            ]
        );
        assert_eq!(
            available_choices(Awareness::EnemyOnly, EncounterArchetype::Bandits, 1),
            vec![EncounterChoice::Attack, EncounterChoice::Surrender]
        );
        assert_eq!(
            available_choices(Awareness::Both, EncounterArchetype::Goblins, 100),
            vec![EncounterChoice::Attack, EncounterChoice::Run]
        );
    }

    #[test]
    fn sustainable_speed_uses_every_modeled_strategic_penalty() {
        let baseline = sustainable_speed_m_per_minute(0, 10_000, 1, EncounterTerrain::Road);
        assert_eq!(baseline, PARTY_WALKING_SPEED_M_PER_MINUTE);
        assert!(sustainable_speed_m_per_minute(50, 10_000, 1, EncounterTerrain::Road) < baseline);
        assert!(sustainable_speed_m_per_minute(0, 5_000, 1, EncounterTerrain::Road) < baseline);
        assert!(sustainable_speed_m_per_minute(0, 10_000, 6, EncounterTerrain::Road) < baseline);
        assert!(
            sustainable_speed_m_per_minute(0, 10_000, 1, EncounterTerrain::DeepWoods) < baseline
        );
    }

    #[test]
    fn impossible_habitats_remain_hard_zero_despite_curation() {
        assert_eq!(encounter_weight(ThreatId::Skeleton, Habitat::Road, true), 0);
        assert_eq!(
            encounter_weight(ThreatId::Bandit, Habitat::DeepWoods, false),
            0
        );
        let occupied_total = [ThreatId::Bandit, ThreatId::Goblin, ThreatId::Skeleton]
            .into_iter()
            .map(|id| encounter_weight(id, Habitat::OccupiedHouse, true))
            .sum::<u32>();
        // Skeletons need a causal bridge here, which random encounter selection cannot supply.
        assert_eq!(occupied_total, 0);
        assert_eq!(select_archetype_from_weights(7, [0, 0, 0]), None);
        let road_with_undead_quest = EncounterContext {
            accepted_active_quest: Some(AcceptedQuestInfluence {
                archetype: EncounterArchetype::Undead,
                distance_minutes: 0,
            }),
            ..context()
        };
        assert!(
            (0..50_000)
                .filter_map(|seed| select_at(seed, 1, 180, road_with_undead_quest))
                .all(|selection| selection.archetype != EncounterArchetype::Undead)
        );
    }

    #[test]
    fn absent_problem_preserves_roll_domains_and_problem_scan_is_chunk_invariant() {
        for seed in 0..2_000 {
            assert_eq!(
                select_at(seed, 1, 180, context()),
                select_at_with_problem(seed, 1, 180, context(), None)
            );
        }
        let influence = |_| {
            Some(LocalProblemInfluence {
                frequency_bonus_basis_points: 2_000,
                archetype: Some(EncounterArchetype::Bandits),
            })
        };
        let whole = first_encounter_with_problem(42, 0, 720, |_| context(), influence);
        let first = first_encounter_with_problem(42, 0, 360, |_| context(), influence);
        let split =
            first.or_else(|| first_encounter_with_problem(42, 360, 360, |_| context(), influence));
        assert_eq!(whole, split);
    }

    #[test]
    fn problem_influence_cannot_revive_an_impossible_archetype() {
        let influence = Some(LocalProblemInfluence {
            frequency_bonus_basis_points: 2_000,
            archetype: Some(EncounterArchetype::Undead),
        });
        assert!(
            (0..20_000)
                .filter_map(|seed| select_at_with_problem(seed, 1, 180, context(), influence))
                .all(|selection| selection.archetype != EncounterArchetype::Undead)
        );
    }
}
