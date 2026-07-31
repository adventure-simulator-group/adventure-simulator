//! Romantic errantry quest frames and presenter-independent challenges.
//!
//! Errantry is deliberately a sibling of generated investigations: its
//! organizing principle is a chivalric purpose tested by ordered trials, not a
//! settlement case whose hidden cause must be discovered.

use serde::{Deserialize, Serialize};

pub const ORDERED_SIGIL_RULES_VERSION: u16 = 1;
pub const ORDERED_SIGIL_COUNT: usize = 5;
const MAX_GENERATION_CLUES: usize = 24;

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

        let mut pool = Vec::new();
        for position in 0..ORDERED_SIGIL_COUNT {
            pool.push(OrderedSigilClue::Exact {
                sigil: solution[position],
                position: position as u8,
            });
            for wrong in 0..ORDERED_SIGIL_COUNT {
                if wrong != position {
                    pool.push(OrderedSigilClue::NotAt {
                        sigil: solution[position],
                        position: wrong as u8,
                    });
                }
            }
        }
        for left in 0..ORDERED_SIGIL_COUNT {
            for right in (left + 1)..ORDERED_SIGIL_COUNT {
                pool.push(OrderedSigilClue::Before {
                    first: solution[left],
                    second: solution[right],
                });
                if right == left + 1 {
                    pool.push(OrderedSigilClue::Adjacent {
                        first: solution[left],
                        second: solution[right],
                    });
                }
            }
        }
        for end in (1..pool.len()).rev() {
            let selected = (rng.next() as usize) % (end + 1);
            pool.swap(end, selected);
        }

        let mut clues = Vec::new();
        for clue in pool.into_iter().take(MAX_GENERATION_CLUES) {
            clues.push(clue);
            if solutions(&clues, 2).len() == 1 {
                break;
            }
        }
        // Exact clues occur in the bounded pool and force convergence.
        if solutions(&clues, 2).len() != 1 {
            clues = solution
                .iter()
                .enumerate()
                .map(|(position, sigil)| OrderedSigilClue::Exact {
                    sigil: *sigil,
                    position: position as u8,
                })
                .collect();
        }
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
pub enum PresenterKind {
    FeySpoken,
    RuinContraption,
}

/// Flavor/presentation is bound after generation and cannot alter puzzle truth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChallengePresenter {
    pub kind: PresenterKind,
    pub name: String,
    pub title: String,
    pub introduction: Vec<String>,
    pub instruction: Vec<String>,
    pub wrong_feedback: Vec<String>,
    pub correct_feedback: Vec<String>,
}

pub fn presenter(kind: PresenterKind) -> ChallengePresenter {
    match kind {
        PresenterKind::FeySpoken => ChallengePresenter {
            kind,
            name: "The Lady Beneath the Thorn".into(),
            title: "Now Hear the Lady Crowned with Briar Thorn".into(),
            // Closed, authored supernatural speech. Each line is intended as
            // iambic pentameter in Shakespearean English, modern spelling.
            introduction: vec![
                "Good knight, five signs attend my moonlit gate.".into(),
                "Set each in place, and thereby prove thy wit.".into(),
            ],
            instruction: vec![
                "Mark well my words, then set the signs in rank.".into(),
                "Submit thy choice when all the five are set.".into(),
            ],
            wrong_feedback: vec![
                "Thy chosen rank hath missed the hidden truth.".into(),
                "Take heart, good knight, and read my words anew.".into(),
            ],
            correct_feedback: vec![
                "Thy wit hath won the passage through my wood.".into(),
                "Go forth with honor; none shall bar thy road.".into(),
            ],
        },
        PresenterKind::RuinContraption => ChallengePresenter {
            kind,
            name: "The Fivefold Gate".into(),
            title: "An ancient ordering mechanism".into(),
            introduction: vec!["Five weathered sockets wait beneath a sealed stone door.".into()],
            instruction: vec![
                "Arrange every sigil from left to right, then engage the bronze lever.".into(),
            ],
            wrong_feedback: vec![
                "The mechanism shudders, resets, and remains unlocked for another attempt.".into(),
            ],
            correct_feedback: vec!["Hidden counterweights descend. The stone door opens.".into()],
        },
    }
}

/// Adapts formal puzzle clues to presenter language without changing their
/// truth conditions. Supernatural speech comes only from the closed catalog
/// below; mechanisms retain concise non-spoken instruction prose.
pub fn presented_clue_text(kind: PresenterKind, clue: &OrderedSigilClue) -> &'static str {
    if kind == PresenterKind::RuinContraption {
        return ruin_clue_text(clue);
    }
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

fn ruin_clue_text(clue: &OrderedSigilClue) -> &'static str {
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
        } => RUIN_EXACT[sigil(value)][usize::from(position)],
        OrderedSigilClue::NotAt {
            sigil: value,
            position,
        } => RUIN_NOT_AT[sigil(value)][usize::from(position)],
        OrderedSigilClue::Before { first, second } => RUIN_BEFORE[sigil(first)][sigil(second)],
        OrderedSigilClue::Adjacent { first, second } => RUIN_ADJACENT[sigil(first)][sigil(second)],
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

// Closed mechanism inscriptions remain presenter-specific but are not speech.
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
    fn one_thousand_seeds_terminate_and_have_one_solution() {
        for seed in 0..1_000 {
            let puzzle = OrderedSigilPuzzle::generate(seed);
            assert_eq!(solutions(&puzzle.clues, 2), vec![puzzle.solution]);
            assert!(puzzle.clues.len() <= MAX_GENERATION_CLUES);
        }
    }

    #[test]
    fn replay_is_versioned_and_deterministic() {
        let first = OrderedSigilPuzzle::generate_versioned(1, 41).unwrap();
        assert_eq!(
            first,
            OrderedSigilPuzzle::generate_versioned(1, 41).unwrap()
        );
        assert!(OrderedSigilPuzzle::generate_versioned(2, 41).is_err());
    }

    #[test]
    fn presenters_share_truth_and_projection_is_safe() {
        let puzzle = OrderedSigilPuzzle::generate(9);
        let projection = puzzle.projection();
        assert_eq!(
            presenter(PresenterKind::FeySpoken).kind,
            PresenterKind::FeySpoken
        );
        assert_eq!(
            presenter(PresenterKind::RuinContraption).kind,
            PresenterKind::RuinContraption
        );
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
        let fey = presenter(PresenterKind::FeySpoken);
        let all = fey
            .introduction
            .iter()
            .chain(&fey.instruction)
            .chain(&fey.wrong_feedback)
            .chain(&fey.correct_feedback)
            .collect::<Vec<_>>();
        assert_eq!(all.len(), 8);
        assert!(
            all.iter()
                .all(|line| line.ends_with('.') && !line.contains('{'))
        );

        let mut clue_lines = Vec::new();
        for sigil in Sigil::ALL {
            for position in 0..ORDERED_SIGIL_COUNT as u8 {
                clue_lines.push(presented_clue_text(
                    PresenterKind::FeySpoken,
                    &OrderedSigilClue::Exact { sigil, position },
                ));
                clue_lines.push(presented_clue_text(
                    PresenterKind::FeySpoken,
                    &OrderedSigilClue::NotAt { sigil, position },
                ));
            }
            for other in Sigil::ALL {
                if sigil != other {
                    clue_lines.push(presented_clue_text(
                        PresenterKind::FeySpoken,
                        &OrderedSigilClue::Before {
                            first: sigil,
                            second: other,
                        },
                    ));
                    clue_lines.push(presented_clue_text(
                        PresenterKind::FeySpoken,
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
}
