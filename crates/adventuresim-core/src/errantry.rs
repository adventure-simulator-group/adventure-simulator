//! Romantic errantry quest frames and presenter-independent challenges.
//!
//! Errantry is deliberately a sibling of generated investigations: its
//! organizing principle is a chivalric purpose tested by ordered trials, not a
//! settlement case whose hidden cause must be discovered.

use serde::{Deserialize, Serialize};

pub const ORDERED_SIGIL_RULES_VERSION: u16 = 2;
pub const ORDERED_SIGIL_COUNT: usize = 5;
pub const MAX_MINIMIZATION_SUBSETS: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrantryPurpose {
    KeepVow,
    SeekBoon,
    Rescue,
    Pilgrimage,
    ProveWorth,
    Reconcile,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrantryFrame {
    pub id: String,
    pub purpose: ErrantryPurpose,
    pub charge: String,
    pub trials: Vec<TrialBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialBinding {
    pub order: u16,
    pub trial_id: String,
    pub challenge_id: Option<String>,
    pub site_id: String,
    pub kind: TrialKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrialKind {
    Combat,
    Social,
    Puzzle,
    Temptation,
    Ordeal,
}

/// A camp's stable identity within one journey. Elapsed time is deliberately
/// absent: resting advances elapsed time without moving the camp.
pub fn journey_camp_identity_matches(
    journey_departure_minute: u64,
    completed_movement_minute: u64,
    camp_stop_minutes: &[u64],
    bound_departure_minute: u64,
    bound_movement_minute: u64,
) -> bool {
    journey_departure_minute == bound_departure_minute
        && completed_movement_minute == bound_movement_minute
        && camp_stop_minutes.contains(&bound_movement_minute)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Sigil {
    Crown,
    Hart,
    Moon,
    Rose,
    Sword,
}

impl Sigil {
    pub const ALL: [Self; ORDERED_SIGIL_COUNT] =
        [Self::Crown, Self::Hart, Self::Moon, Self::Rose, Self::Sword];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Crown => "Crown",
            Self::Hart => "Hart",
            Self::Moon => "Moon",
            Self::Rose => "Rose",
            Self::Sword => "Sword",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderedSigilClue {
    Exact { sigil: Sigil, position: u8 },
    Before { first: Sigil, second: Sigil },
    Adjacent { first: Sigil, second: Sigil },
    NotAt { sigil: Sigil, position: u8 },
}

impl OrderedSigilClue {
    pub fn text(&self) -> String {
        match self {
            Self::Exact { sigil, position } => {
                format!("The {} belongs in place {}.", sigil.label(), position + 1)
            }
            Self::Before { first, second } => format!(
                "The {} stands somewhere before the {}.",
                first.label(),
                second.label()
            ),
            Self::Adjacent { first, second } => format!(
                "The {} and {} stand beside one another.",
                first.label(),
                second.label()
            ),
            Self::NotAt { sigil, position } => {
                format!(
                    "The {} does not belong in place {}.",
                    sigil.label(),
                    position + 1
                )
            }
        }
    }
}

/// Private deterministic replay authority. Never project this type to clients.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderedSigilPuzzle {
    pub rules_version: u16,
    pub seed: u64,
    pub solution: [Sigil; ORDERED_SIGIL_COUNT],
    pub clues: Vec<OrderedSigilClue>,
}

/// Observer-safe challenge truth. It intentionally lacks seed and solution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderedSigilProjection {
    pub rules_version: u16,
    pub sigils: [Sigil; ORDERED_SIGIL_COUNT],
    pub clues: Vec<OrderedSigilClue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderedSigilSubmission {
    pub expected_revision: u32,
    pub ordering: [Sigil; ORDERED_SIGIL_COUNT],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmissionError {
    DuplicateSigil,
}

impl OrderedSigilPuzzle {
    pub fn generate(seed: u64) -> Self {
        Self::generate_versioned(ORDERED_SIGIL_RULES_VERSION, seed)
            .expect("current ordered-sigil rules are supported")
    }

    pub fn generate_versioned(rules_version: u16, seed: u64) -> Result<Self, &'static str> {
        if rules_version != ORDERED_SIGIL_RULES_VERSION {
            return Err("unsupported ordered-sigil rules version");
        }
        let mut rng = SplitMix64(seed ^ 0x6572_7261_6e74_7279);
        let mut solution = Sigil::ALL;
        for end in (1..solution.len()).rev() {
            let selected = (rng.next() as usize) % (end + 1);
            solution.swap(end, selected);
        }

        let (clues, _) = minimum_clues(&solution)?;
        let puzzle = Self {
            rules_version,
            seed,
            solution,
            clues,
        };
        puzzle.validate()?;
        Ok(puzzle)
    }

    pub fn projection(&self) -> OrderedSigilProjection {
        OrderedSigilProjection {
            rules_version: self.rules_version,
            sigils: Sigil::ALL,
            clues: self.clues.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.rules_version != ORDERED_SIGIL_RULES_VERSION {
            return Err("unsupported ordered-sigil rules version");
        }
        if !is_permutation(&self.solution) {
            return Err("ordered-sigil solution is malformed");
        }
        if self.clues.iter().any(|clue| match clue {
            OrderedSigilClue::Exact { position, .. } | OrderedSigilClue::NotAt { position, .. } => {
                usize::from(*position) >= ORDERED_SIGIL_COUNT
            }
            OrderedSigilClue::Before { first, second }
            | OrderedSigilClue::Adjacent { first, second } => first == second,
        }) {
            return Err("ordered-sigil clue coordinates are malformed");
        }
        let found = solutions(&self.clues, 2);
        if found.len() != 1 || found[0] != self.solution {
            return Err("ordered-sigil clues do not prove the canonical solution");
        }
        Ok(())
    }

    pub fn check(&self, submission: &OrderedSigilSubmission) -> Result<bool, SubmissionError> {
        if !is_permutation(&submission.ordering) {
            return Err(SubmissionError::DuplicateSigil);
        }
        Ok(submission.ordering == self.solution)
    }
}

fn true_clue_pool(solution: &[Sigil; ORDERED_SIGIL_COUNT]) -> Vec<OrderedSigilClue> {
    let mut pool = Vec::with_capacity(39);
    // Canonical relational-first order is also the final deterministic
    // tie-break after cardinality, Exact count, and NotAt count.
    for left in 0..ORDERED_SIGIL_COUNT {
        for right in (left + 1)..ORDERED_SIGIL_COUNT {
            pool.push(OrderedSigilClue::Before {
                first: solution[left],
                second: solution[right],
            });
        }
    }
    for left in 0..(ORDERED_SIGIL_COUNT - 1) {
        pool.push(OrderedSigilClue::Adjacent {
            first: solution[left],
            second: solution[left + 1],
        });
    }
    for (position, sigil) in solution.iter().copied().enumerate() {
        for wrong in 0..ORDERED_SIGIL_COUNT {
            if wrong != position {
                pool.push(OrderedSigilClue::NotAt {
                    sigil,
                    position: wrong as u8,
                });
            }
        }
    }
    for (position, sigil) in solution.iter().copied().enumerate() {
        pool.push(OrderedSigilClue::Exact {
            sigil,
            position: position as u8,
        });
    }
    pool
}

fn all_orderings() -> Vec<[Sigil; ORDERED_SIGIL_COUNT]> {
    solutions(&[], usize::MAX)
}

fn minimum_clues(
    solution: &[Sigil; ORDERED_SIGIL_COUNT],
) -> Result<(Vec<OrderedSigilClue>, usize), &'static str> {
    let pool = true_clue_pool(solution);
    let orderings = all_orderings();
    let solution_index = orderings
        .iter()
        .position(|candidate| candidate == solution)
        .ok_or("ordered-sigil solution is absent from permutation space")?;
    let target = 1_u128 << solution_index;
    let full = (1_u128 << orderings.len()) - 1;
    let masks = pool
        .iter()
        .map(|clue| {
            orderings
                .iter()
                .enumerate()
                .fold(0_u128, |mask, (index, candidate)| {
                    mask | (u128::from(clue_holds(clue, candidate)) << index)
                })
        })
        .collect::<Vec<_>>();
    let mut evaluated = 0;
    for cardinality in 1..=4 {
        let mut chosen = Vec::with_capacity(cardinality);
        let mut best: Option<(usize, usize, Vec<usize>)> = None;
        search_minimum_subsets(
            0,
            cardinality,
            full,
            target,
            &pool,
            &masks,
            &mut chosen,
            &mut best,
            &mut evaluated,
        )?;
        if let Some((_, _, indices)) = best {
            return Ok((
                indices
                    .into_iter()
                    .map(|index| pool[index].clone())
                    .collect(),
                evaluated,
            ));
        }
    }
    Err("ordered-sigil four-clue relational bound failed")
}

#[allow(clippy::too_many_arguments)]
fn search_minimum_subsets(
    start: usize,
    remaining: usize,
    intersection: u128,
    target: u128,
    pool: &[OrderedSigilClue],
    masks: &[u128],
    chosen: &mut Vec<usize>,
    best: &mut Option<(usize, usize, Vec<usize>)>,
    evaluated: &mut usize,
) -> Result<(), &'static str> {
    if remaining == 0 {
        *evaluated += 1;
        if *evaluated > MAX_MINIMIZATION_SUBSETS {
            return Err("ordered-sigil minimization exceeded its subset bound");
        }
        if intersection == target {
            let exact = chosen
                .iter()
                .filter(|index| matches!(pool[**index], OrderedSigilClue::Exact { .. }))
                .count();
            let not_at = chosen
                .iter()
                .filter(|index| matches!(pool[**index], OrderedSigilClue::NotAt { .. }))
                .count();
            let candidate = (exact, not_at, chosen.clone());
            if best.as_ref().is_none_or(|current| candidate < *current) {
                *best = Some(candidate);
            }
        }
        return Ok(());
    }
    let last_start = pool.len().saturating_sub(remaining);
    for index in start..=last_start {
        chosen.push(index);
        search_minimum_subsets(
            index + 1,
            remaining - 1,
            intersection & masks[index],
            target,
            pool,
            masks,
            chosen,
            best,
            evaluated,
        )?;
        chosen.pop();
    }
    Ok(())
}

pub fn solutions(clues: &[OrderedSigilClue], limit: usize) -> Vec<[Sigil; ORDERED_SIGIL_COUNT]> {
    fn visit(
        at: usize,
        current: &mut [Sigil; ORDERED_SIGIL_COUNT],
        clues: &[OrderedSigilClue],
        limit: usize,
        found: &mut Vec<[Sigil; ORDERED_SIGIL_COUNT]>,
    ) {
        if found.len() >= limit {
            return;
        }
        if at == current.len() {
            if clues.iter().all(|clue| clue_holds(clue, current)) {
                found.push(*current);
            }
            return;
        }
        for next in at..current.len() {
            current.swap(at, next);
            visit(at + 1, current, clues, limit, found);
            current.swap(at, next);
        }
    }
    let mut found = Vec::new();
    let mut current = Sigil::ALL;
    visit(0, &mut current, clues, limit, &mut found);
    found
}

fn clue_holds(clue: &OrderedSigilClue, candidate: &[Sigil; ORDERED_SIGIL_COUNT]) -> bool {
    let position = |wanted| candidate.iter().position(|sigil| *sigil == wanted).unwrap();
    match *clue {
        OrderedSigilClue::Exact { sigil, position: p } => position(sigil) == usize::from(p),
        OrderedSigilClue::Before { first, second } => position(first) < position(second),
        OrderedSigilClue::Adjacent { first, second } => {
            position(first).abs_diff(position(second)) == 1
        }
        OrderedSigilClue::NotAt { sigil, position: p } => position(sigil) != usize::from(p),
    }
}

fn is_permutation(ordering: &[Sigil; ORDERED_SIGIL_COUNT]) -> bool {
    Sigil::ALL.iter().all(|sigil| {
        ordering
            .iter()
            .filter(|candidate| *candidate == sigil)
            .count()
            == 1
    })
}

struct SplitMix64(u64);
impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeyPresenterCatalogId {
    LadyBeneathThornV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeySpeechPart {
    Introduction,
    Instruction,
    Wrong,
    Correct,
}

pub const FEY_PRESENTER_NAME: &str = "The Lady Beneath the Thorn";
pub const FEY_PRESENTER_TITLE: &str = "Now Hear the Lady Crowned with Briar Thorn";

/// Closed server-owned speech selected by typed catalog identity. No persisted
/// prose can become a supernatural utterance.
pub fn fey_speech(catalog: FeyPresenterCatalogId, part: FeySpeechPart) -> &'static [&'static str] {
    match (catalog, part) {
        (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Introduction) => &[
            "Good knight, five signs attend my moonlit gate.",
            "Set each in place, and thereby prove thy wit.",
        ],
        (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Instruction) => &[
            "Mark well my words, then set the signs in rank.",
            "Submit thy choice when all the five are set.",
        ],
        (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Wrong) => &[
            "Thy chosen rank hath missed the hidden truth.",
            "Take heart, good knight, and read my words anew.",
        ],
        (FeyPresenterCatalogId::LadyBeneathThornV1, FeySpeechPart::Correct) => &[
            "Thy wit hath won the passage through my wood.",
            "Go forth with honor; none shall bar thy road.",
        ],
    }
}

pub fn fey_clue_text(_catalog: FeyPresenterCatalogId, clue: &OrderedSigilClue) -> &'static str {
    let sigil = |value: Sigil| match value {
        Sigil::Crown => 0,
        Sigil::Hart => 1,
        Sigil::Moon => 2,
        Sigil::Rose => 3,
        Sigil::Sword => 4,
    };
    match *clue {
        OrderedSigilClue::Exact {
            sigil: value,
            position,
        } => FEY_EXACT[sigil(value)][usize::from(position)],
        OrderedSigilClue::NotAt {
            sigil: value,
            position,
        } => FEY_NOT_AT[sigil(value)][usize::from(position)],
        OrderedSigilClue::Before { first, second } => FEY_BEFORE[sigil(first)][sigil(second)],
        OrderedSigilClue::Adjacent { first, second } => FEY_ADJACENT[sigil(first)][sigil(second)],
    }
}

// Closed, reviewed supernatural clue speech. Every reachable entry is an
// authored modern-spelling Shakespearean line intended as iambic pentameter.
const FEY_EXACT: [[&str; 5]; 5] = [
    [
        "The Crown shall now in first position stand.",
        "The Crown shall in the second station stand.",
        "The Crown shall now in third position stand.",
        "The Crown shall now in fourth position stand.",
        "The Crown shall now in fifth position stand.",
    ],
    [
        "The Hart shall now in first position stand.",
        "The Hart shall in the second station stand.",
        "The Hart shall now in third position stand.",
        "The Hart shall now in fourth position stand.",
        "The Hart shall now in fifth position stand.",
    ],
    [
        "The Moon shall now in first position stand.",
        "The Moon shall in the second station stand.",
        "The Moon shall now in third position stand.",
        "The Moon shall now in fourth position stand.",
        "The Moon shall now in fifth position stand.",
    ],
    [
        "The Rose shall now in first position stand.",
        "The Rose shall in the second station stand.",
        "The Rose shall now in third position stand.",
        "The Rose shall now in fourth position stand.",
        "The Rose shall now in fifth position stand.",
    ],
    [
        "The Sword shall now in first position stand.",
        "The Sword shall in the second station stand.",
        "The Sword shall now in third position stand.",
        "The Sword shall now in fourth position stand.",
        "The Sword shall now in fifth position stand.",
    ],
];

const FEY_NOT_AT: [[&str; 5]; 5] = [
    [
        "The Crown shall never hold the first estate.",
        "The Crown shall not hold the second estate.",
        "The Crown shall never hold the third estate.",
        "The Crown shall never hold the fourth estate.",
        "The Crown shall never hold the fifth estate.",
    ],
    [
        "The Hart shall never hold the first estate.",
        "The Hart shall not hold the second estate.",
        "The Hart shall never hold the third estate.",
        "The Hart shall never hold the fourth estate.",
        "The Hart shall never hold the fifth estate.",
    ],
    [
        "The Moon shall never hold the first estate.",
        "The Moon shall not hold the second estate.",
        "The Moon shall never hold the third estate.",
        "The Moon shall never hold the fourth estate.",
        "The Moon shall never hold the fifth estate.",
    ],
    [
        "The Rose shall never hold the first estate.",
        "The Rose shall not hold the second estate.",
        "The Rose shall never hold the third estate.",
        "The Rose shall never hold the fourth estate.",
        "The Rose shall never hold the fifth estate.",
    ],
    [
        "The Sword shall never hold the first estate.",
        "The Sword shall not hold the second estate.",
        "The Sword shall never hold the third estate.",
        "The Sword shall never hold the fourth estate.",
        "The Sword shall never hold the fifth estate.",
    ],
];

const FEY_BEFORE: [[&str; 5]; 5] = [
    [
        "",
        "The Crown must take its place before the Hart.",
        "The Crown must take its place before the Moon.",
        "The Crown must take its place before the Rose.",
        "The Crown must take its place before the Sword.",
    ],
    [
        "The Hart must take its place before the Crown.",
        "",
        "The Hart must take its place before the Moon.",
        "The Hart must take its place before the Rose.",
        "The Hart must take its place before the Sword.",
    ],
    [
        "The Moon must take its place before the Crown.",
        "The Moon must take its place before the Hart.",
        "",
        "The Moon must take its place before the Rose.",
        "The Moon must take its place before the Sword.",
    ],
    [
        "The Rose must take its place before the Crown.",
        "The Rose must take its place before the Hart.",
        "The Rose must take its place before the Moon.",
        "",
        "The Rose must take its place before the Sword.",
    ],
    [
        "The Sword must take its place before the Crown.",
        "The Sword must take its place before the Hart.",
        "The Sword must take its place before the Moon.",
        "The Sword must take its place before the Rose.",
        "",
    ],
];

const FEY_ADJACENT: [[&str; 5]; 5] = [
    [
        "",
        "Let Crown and Hart stand ever side by side.",
        "Let Crown and Moon stand ever side by side.",
        "Let Crown and Rose stand ever side by side.",
        "Let Crown and Sword stand ever side by side.",
    ],
    [
        "Let Hart and Crown stand ever side by side.",
        "",
        "Let Hart and Moon stand ever side by side.",
        "Let Hart and Rose stand ever side by side.",
        "Let Hart and Sword stand ever side by side.",
    ],
    [
        "Let Moon and Crown stand ever side by side.",
        "Let Moon and Hart stand ever side by side.",
        "",
        "Let Moon and Rose stand ever side by side.",
        "Let Moon and Sword stand ever side by side.",
    ],
    [
        "Let Rose and Crown stand ever side by side.",
        "Let Rose and Hart stand ever side by side.",
        "Let Rose and Moon stand ever side by side.",
        "",
        "Let Rose and Sword stand ever side by side.",
    ],
    [
        "Let Sword and Crown stand ever side by side.",
        "Let Sword and Hart stand ever side by side.",
        "Let Sword and Moon stand ever side by side.",
        "Let Sword and Rose stand ever side by side.",
        "",
    ],
];

// Historical physical-presenter text is compile-disabled while the initial
// errantry slice is chat-only.
#[cfg(any())]
const RUIN_EXACT: [[&str; 5]; 5] = [
    [
        "Crown: I.",
        "Crown: II.",
        "Crown: III.",
        "Crown: IV.",
        "Crown: V.",
    ],
    [
        "Hart: I.",
        "Hart: II.",
        "Hart: III.",
        "Hart: IV.",
        "Hart: V.",
    ],
    [
        "Moon: I.",
        "Moon: II.",
        "Moon: III.",
        "Moon: IV.",
        "Moon: V.",
    ],
    [
        "Rose: I.",
        "Rose: II.",
        "Rose: III.",
        "Rose: IV.",
        "Rose: V.",
    ],
    [
        "Sword: I.",
        "Sword: II.",
        "Sword: III.",
        "Sword: IV.",
        "Sword: V.",
    ],
];
#[cfg(any())]
const RUIN_NOT_AT: [[&str; 5]; 5] = [
    [
        "Crown ≠ I.",
        "Crown ≠ II.",
        "Crown ≠ III.",
        "Crown ≠ IV.",
        "Crown ≠ V.",
    ],
    [
        "Hart ≠ I.",
        "Hart ≠ II.",
        "Hart ≠ III.",
        "Hart ≠ IV.",
        "Hart ≠ V.",
    ],
    [
        "Moon ≠ I.",
        "Moon ≠ II.",
        "Moon ≠ III.",
        "Moon ≠ IV.",
        "Moon ≠ V.",
    ],
    [
        "Rose ≠ I.",
        "Rose ≠ II.",
        "Rose ≠ III.",
        "Rose ≠ IV.",
        "Rose ≠ V.",
    ],
    [
        "Sword ≠ I.",
        "Sword ≠ II.",
        "Sword ≠ III.",
        "Sword ≠ IV.",
        "Sword ≠ V.",
    ],
];
#[cfg(any())]
const RUIN_BEFORE: [[&str; 5]; 5] = [
    [
        "",
        "Crown < Hart.",
        "Crown < Moon.",
        "Crown < Rose.",
        "Crown < Sword.",
    ],
    [
        "Hart < Crown.",
        "",
        "Hart < Moon.",
        "Hart < Rose.",
        "Hart < Sword.",
    ],
    [
        "Moon < Crown.",
        "Moon < Hart.",
        "",
        "Moon < Rose.",
        "Moon < Sword.",
    ],
    [
        "Rose < Crown.",
        "Rose < Hart.",
        "Rose < Moon.",
        "",
        "Rose < Sword.",
    ],
    [
        "Sword < Crown.",
        "Sword < Hart.",
        "Sword < Moon.",
        "Sword < Rose.",
        "",
    ],
];
#[cfg(any())]
const RUIN_ADJACENT: [[&str; 5]; 5] = [
    [
        "",
        "Crown—Hart.",
        "Crown—Moon.",
        "Crown—Rose.",
        "Crown—Sword.",
    ],
    ["Hart—Crown.", "", "Hart—Moon.", "Hart—Rose.", "Hart—Sword."],
    ["Moon—Crown.", "Moon—Hart.", "", "Moon—Rose.", "Moon—Sword."],
    ["Rose—Crown.", "Rose—Hart.", "Rose—Moon.", "", "Rose—Sword."],
    [
        "Sword—Crown.",
        "Sword—Hart.",
        "Sword—Moon.",
        "Sword—Rose.",
        "",
    ],
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camp_identity_survives_rest_but_not_movement_or_a_new_journey() {
        let stops = [60, 120];
        assert!(journey_camp_identity_matches(40, 60, &stops, 40, 60));
        // Rest is not an input: only stable journey and movement coordinates
        // identify the persisted camp.
        assert!(!journey_camp_identity_matches(40, 120, &stops, 40, 60));
        assert!(!journey_camp_identity_matches(41, 60, &stops, 40, 60));
        assert!(!journey_camp_identity_matches(40, 60, &[120], 40, 60));
    }

    #[test]
    fn every_solution_gets_a_globally_minimum_bounded_necessary_clue_set() {
        for solution in all_orderings() {
            let (clues, evaluated) = minimum_clues(&solution).unwrap();
            assert!(clues.len() <= 4);
            assert!(evaluated <= MAX_MINIMIZATION_SUBSETS);
            assert_eq!(solutions(&clues, 2), vec![solution]);

            for removed in 0..clues.len() {
                let mut reduced = clues.clone();
                reduced.remove(removed);
                assert_ne!(solutions(&reduced, 2), vec![solution]);
            }

            let pool = true_clue_pool(&solution);
            for cardinality in 1..clues.len() {
                assert!(
                    best_unique_subset(&pool, &solution, cardinality).is_none(),
                    "found a smaller clue set for {solution:?}"
                );
            }
        }
    }

    #[test]
    fn replay_is_versioned_and_deterministic() {
        let first =
            OrderedSigilPuzzle::generate_versioned(ORDERED_SIGIL_RULES_VERSION, 41).unwrap();
        assert_eq!(
            first.solution,
            [
                Sigil::Hart,
                Sigil::Crown,
                Sigil::Sword,
                Sigil::Moon,
                Sigil::Rose
            ]
        );
        assert_eq!(
            first.clues,
            vec![
                OrderedSigilClue::Before {
                    first: Sigil::Hart,
                    second: Sigil::Crown,
                },
                OrderedSigilClue::Before {
                    first: Sigil::Crown,
                    second: Sigil::Sword,
                },
                OrderedSigilClue::Before {
                    first: Sigil::Sword,
                    second: Sigil::Moon,
                },
                OrderedSigilClue::Before {
                    first: Sigil::Moon,
                    second: Sigil::Rose,
                },
            ]
        );
        assert_eq!(
            first,
            OrderedSigilPuzzle::generate_versioned(ORDERED_SIGIL_RULES_VERSION, 41).unwrap()
        );
        assert!(OrderedSigilPuzzle::generate_versioned(1, 41).is_err());
    }

    #[test]
    fn tie_break_prefers_relations_then_not_at_then_exact() {
        for solution in all_orderings() {
            let (chosen, _) = minimum_clues(&solution).unwrap();
            let pool = true_clue_pool(&solution);
            let chosen_indices = chosen
                .iter()
                .map(|clue| pool.iter().position(|candidate| candidate == clue).unwrap())
                .collect::<Vec<_>>();
            let chosen_score = subset_style_score(&pool, &chosen_indices);
            assert_eq!(
                Some(chosen_score),
                best_unique_subset(&pool, &solution, chosen.len())
            );
        }
    }

    #[test]
    fn projection_is_safe() {
        let puzzle = OrderedSigilPuzzle::generate(9);
        let projection = puzzle.projection();
        let json = serde_json::to_string(&projection).unwrap();
        assert!(!json.contains("solution"));
        assert!(!json.contains("seed"));
        assert_eq!(projection.clues, puzzle.clues);
    }

    #[test]
    fn only_the_solution_is_accepted() {
        let puzzle = OrderedSigilPuzzle::generate(77);
        for candidate in solutions(&[], usize::MAX) {
            let submission = OrderedSigilSubmission {
                expected_revision: 0,
                ordering: candidate,
            };
            assert_eq!(
                puzzle.check(&submission).unwrap(),
                candidate == puzzle.solution
            );
        }
        let malformed = OrderedSigilSubmission {
            expected_revision: 0,
            ordering: [Sigil::Crown; ORDERED_SIGIL_COUNT],
        };
        assert_eq!(
            puzzle.check(&malformed),
            Err(SubmissionError::DuplicateSigil)
        );
    }

    #[test]
    fn supernatural_speech_comes_only_from_the_closed_verse_catalog() {
        let catalog = FeyPresenterCatalogId::LadyBeneathThornV1;
        let all = [
            FeySpeechPart::Introduction,
            FeySpeechPart::Instruction,
            FeySpeechPart::Wrong,
            FeySpeechPart::Correct,
        ]
        .into_iter()
        .flat_map(|part| fey_speech(catalog, part))
        .collect::<Vec<_>>();
        assert_eq!(all.len(), 8);
        assert!(
            all.iter()
                .all(|line| line.ends_with('.') && !line.contains('{'))
        );

        let mut clue_lines = Vec::new();
        for sigil in Sigil::ALL {
            for position in 0..ORDERED_SIGIL_COUNT as u8 {
                clue_lines.push(fey_clue_text(
                    catalog,
                    &OrderedSigilClue::Exact { sigil, position },
                ));
                clue_lines.push(fey_clue_text(
                    catalog,
                    &OrderedSigilClue::NotAt { sigil, position },
                ));
            }
            for other in Sigil::ALL {
                if sigil != other {
                    clue_lines.push(fey_clue_text(
                        catalog,
                        &OrderedSigilClue::Before {
                            first: sigil,
                            second: other,
                        },
                    ));
                    clue_lines.push(fey_clue_text(
                        catalog,
                        &OrderedSigilClue::Adjacent {
                            first: sigil,
                            second: other,
                        },
                    ));
                }
            }
        }
        assert_eq!(clue_lines.len(), 90);
        assert!(clue_lines.iter().all(|line| {
            !line.is_empty()
                && line.ends_with('.')
                && !line.contains('{')
                && !line.contains("position {}")
        }));
    }

    fn subset_style_score(
        pool: &[OrderedSigilClue],
        indices: &[usize],
    ) -> (usize, usize, Vec<usize>) {
        let exact = indices
            .iter()
            .filter(|index| matches!(pool[**index], OrderedSigilClue::Exact { .. }))
            .count();
        let not_at = indices
            .iter()
            .filter(|index| matches!(pool[**index], OrderedSigilClue::NotAt { .. }))
            .count();
        (exact, not_at, indices.to_vec())
    }

    fn best_unique_subset(
        pool: &[OrderedSigilClue],
        solution: &[Sigil; ORDERED_SIGIL_COUNT],
        cardinality: usize,
    ) -> Option<(usize, usize, Vec<usize>)> {
        let orderings = all_orderings();
        let target = 1_u128
            << orderings
                .iter()
                .position(|candidate| candidate == solution)
                .unwrap();
        let masks = pool
            .iter()
            .map(|clue| {
                orderings
                    .iter()
                    .enumerate()
                    .fold(0_u128, |mask, (index, ordering)| {
                        mask | (u128::from(clue_holds(clue, ordering)) << index)
                    })
            })
            .collect::<Vec<_>>();
        let mut best = None;
        visit_mask_subsets(
            pool,
            &masks,
            cardinality,
            0,
            (1_u128 << orderings.len()) - 1,
            target,
            &mut Vec::new(),
            &mut best,
        );
        best
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_mask_subsets(
        pool: &[OrderedSigilClue],
        masks: &[u128],
        remaining: usize,
        start: usize,
        intersection: u128,
        target: u128,
        chosen: &mut Vec<usize>,
        best: &mut Option<(usize, usize, Vec<usize>)>,
    ) {
        if remaining == 0 {
            if intersection == target {
                let candidate = subset_style_score(pool, chosen);
                if best.as_ref().is_none_or(|current| candidate < *current) {
                    *best = Some(candidate);
                }
            }
            return;
        }
        for index in start..=pool.len() - remaining {
            chosen.push(index);
            visit_mask_subsets(
                pool,
                masks,
                remaining - 1,
                index + 1,
                intersection & masks[index],
                target,
                chosen,
                best,
            );
            chosen.pop();
        }
    }
}
