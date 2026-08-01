use serde::{Deserialize, Serialize};

use crate::{PuzzleRng, shuffle};

pub const LOGIC_GRID_RULES_VERSION: u16 = 1;
pub const MAX_GRID_SIZE: usize = 4;

const TRAVELERS: [&str; MAX_GRID_SIZE] = ["Aldren", "Beatrice", "Cuthbert", "Dorothea"];
const TOKENS: [&str; MAX_GRID_SIZE] = ["Bell", "Key", "Lantern", "Mirror"];
const ROADS: [&str; MAX_GRID_SIZE] = ["Ash road", "Moon road", "Thorn road", "Yew road"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicGridSpec {
    pub size: u8,
    pub allow_positive_clues: bool,
    pub allow_negative_clues: bool,
    pub max_clues: u8,
}

impl Default for LogicGridSpec {
    fn default() -> Self {
        Self {
            size: 3,
            allow_positive_clues: true,
            allow_negative_clues: true,
            max_clues: 6,
        }
    }
}

impl LogicGridSpec {
    pub fn validate(self) -> Result<Self, &'static str> {
        if !(3..=MAX_GRID_SIZE as u8).contains(&self.size) {
            return Err("logic-grid size must be three or four");
        }
        if !self.allow_positive_clues && !self.allow_negative_clues {
            return Err("logic grid must enable at least one clue polarity");
        }
        if !(1..=12).contains(&self.max_clues) {
            return Err("logic-grid clue limit must be between one and twelve");
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GridRelation {
    TravelerToken,
    TravelerRoad,
    TokenRoad,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicGridClue {
    pub relation: GridRelation,
    pub left: u8,
    pub right: u8,
    pub matches: bool,
}

impl LogicGridClue {
    pub fn text(self) -> String {
        let relation = match self.relation {
            GridRelation::TravelerToken => {
                format!("{} carried the {}", traveler(self.left), token(self.right))
            }
            GridRelation::TravelerRoad => {
                format!("{} took the {}", traveler(self.left), road(self.right))
            }
            GridRelation::TokenRoad => format!(
                "the traveler bearing the {} took the {}",
                token(self.left),
                road(self.right)
            ),
        };
        if self.matches {
            format!("{relation}.")
        } else {
            format!("It is not true that {relation}.")
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicGridAssignment {
    pub traveler: u8,
    pub token: u8,
    pub road: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicGridPuzzle {
    pub rules_version: u16,
    pub seed: u64,
    pub spec: LogicGridSpec,
    solution: Vec<LogicGridAssignment>,
    pub clues: Vec<LogicGridClue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicGridProjection {
    pub rules_version: u16,
    pub travelers: Vec<String>,
    pub tokens: Vec<String>,
    pub roads: Vec<String>,
    pub clues: Vec<LogicGridClue>,
}

impl LogicGridPuzzle {
    pub fn generate(seed: u64) -> Self {
        Self::generate_with_spec(seed, LogicGridSpec::default())
            .expect("standard logic-grid specification is valid")
    }

    pub fn generate_with_spec(seed: u64, spec: LogicGridSpec) -> Result<Self, &'static str> {
        let spec = spec.validate()?;
        let size = usize::from(spec.size);
        let mut rng = PuzzleRng(seed ^ 0x6c6f_6769_635f_6772);
        let mut token_order = (0..spec.size).collect::<Vec<_>>();
        let mut road_order = (0..spec.size).collect::<Vec<_>>();
        shuffle(&mut token_order, &mut rng);
        shuffle(&mut road_order, &mut rng);
        let solution = (0..spec.size)
            .map(|traveler| LogicGridAssignment {
                traveler,
                token: token_order[usize::from(traveler)],
                road: road_order[usize::from(traveler)],
            })
            .collect::<Vec<_>>();

        let mut pool = true_grid_clues(&solution, spec);
        shuffle(&mut pool, &mut rng);
        let candidates = all_grid_solutions(spec.size);
        let mut retained = Vec::new();
        let mut legal = candidates.clone();
        while legal.len() > 1 && retained.len() < usize::from(spec.max_clues) {
            let best = pool
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, clue)| !retained.contains(clue))
                .map(|(index, clue)| {
                    let remaining = legal
                        .iter()
                        .filter(|candidate| grid_clue_holds(&clue, candidate))
                        .count();
                    (legal.len().saturating_sub(remaining), index, clue)
                })
                .filter(|(eliminated, _, _)| *eliminated > 0)
                .max_by_key(|(eliminated, index, _)| (*eliminated, std::cmp::Reverse(*index)))
                .map(|(_, _, clue)| clue)
                .ok_or("logic-grid clue grammar cannot isolate the solution")?;
            retained.push(best);
            legal.retain(|candidate| grid_clue_holds(&best, candidate));
        }
        if legal.len() != 1 {
            return Err("logic-grid clue limit cannot prove one solution");
        }
        let mut index = retained.len();
        while index > 0 {
            index -= 1;
            let mut reduced = retained.clone();
            reduced.remove(index);
            if grid_solutions(spec.size, &reduced, 2).len() == 1 {
                retained = reduced;
            }
        }
        let puzzle = Self {
            rules_version: LOGIC_GRID_RULES_VERSION,
            seed,
            spec,
            solution,
            clues: retained,
        };
        puzzle.validate()?;
        debug_assert_eq!(size, puzzle.solution.len());
        Ok(puzzle)
    }

    pub fn projection(&self) -> LogicGridProjection {
        let size = usize::from(self.spec.size);
        LogicGridProjection {
            rules_version: self.rules_version,
            travelers: TRAVELERS[..size]
                .iter()
                .map(|value| (*value).into())
                .collect(),
            tokens: TOKENS[..size].iter().map(|value| (*value).into()).collect(),
            roads: ROADS[..size].iter().map(|value| (*value).into()).collect(),
            clues: self.clues.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        self.spec.validate()?;
        if self.rules_version != LOGIC_GRID_RULES_VERSION {
            return Err("unsupported logic-grid rules version");
        }
        validate_grid_assignments(self.spec.size, &self.solution)?;
        if self.clues.is_empty() || self.clues.len() > usize::from(self.spec.max_clues) {
            return Err("logic-grid clue count violates its generation spec");
        }
        if self.clues.iter().any(|clue| {
            clue.left >= self.spec.size
                || clue.right >= self.spec.size
                || (clue.matches && !self.spec.allow_positive_clues)
                || (!clue.matches && !self.spec.allow_negative_clues)
                || !grid_clue_holds(clue, &self.solution)
        }) {
            return Err("logic-grid clue contradicts private authority");
        }
        let solutions = grid_solutions(self.spec.size, &self.clues, 2);
        if solutions != vec![self.solution.clone()] {
            return Err("logic-grid clues do not prove the private solution");
        }
        if (0..self.clues.len()).any(|removed| {
            let reduced = self
                .clues
                .iter()
                .copied()
                .enumerate()
                .filter_map(|(index, clue)| (index != removed).then_some(clue))
                .collect::<Vec<_>>();
            grid_solutions(self.spec.size, &reduced, 2).len() == 1
        }) {
            return Err("logic-grid puzzle contains a redundant clue");
        }
        Ok(())
    }

    pub fn check(&self, assignments: &[LogicGridAssignment]) -> Result<bool, &'static str> {
        validate_grid_assignments(self.spec.size, assignments)?;
        Ok(assignments == self.solution)
    }
}

pub fn grid_solutions(
    size: u8,
    clues: &[LogicGridClue],
    limit: usize,
) -> Vec<Vec<LogicGridAssignment>> {
    all_grid_solutions(size)
        .into_iter()
        .filter(|candidate| clues.iter().all(|clue| grid_clue_holds(clue, candidate)))
        .take(limit)
        .collect()
}

pub fn all_grid_solutions(size: u8) -> Vec<Vec<LogicGridAssignment>> {
    let permutations = permutations(size);
    let mut solutions = Vec::with_capacity(permutations.len() * permutations.len());
    for tokens in &permutations {
        for roads in &permutations {
            solutions.push(
                (0..size)
                    .map(|traveler| LogicGridAssignment {
                        traveler,
                        token: tokens[usize::from(traveler)],
                        road: roads[usize::from(traveler)],
                    })
                    .collect(),
            );
        }
    }
    solutions
}

pub fn grid_clue_holds(clue: &LogicGridClue, candidate: &[LogicGridAssignment]) -> bool {
    let assignment = |traveler: u8| candidate.iter().find(|item| item.traveler == traveler);
    let actual = match clue.relation {
        GridRelation::TravelerToken => {
            assignment(clue.left).is_some_and(|item| item.token == clue.right)
        }
        GridRelation::TravelerRoad => {
            assignment(clue.left).is_some_and(|item| item.road == clue.right)
        }
        GridRelation::TokenRoad => candidate
            .iter()
            .find(|item| item.token == clue.left)
            .is_some_and(|item| item.road == clue.right),
    };
    actual == clue.matches
}

fn true_grid_clues(solution: &[LogicGridAssignment], spec: LogicGridSpec) -> Vec<LogicGridClue> {
    let mut clues = Vec::new();
    for relation in [
        GridRelation::TravelerToken,
        GridRelation::TravelerRoad,
        GridRelation::TokenRoad,
    ] {
        for left in 0..spec.size {
            for right in 0..spec.size {
                let positive = LogicGridClue {
                    relation,
                    left,
                    right,
                    matches: true,
                };
                let matches = grid_clue_holds(&positive, solution);
                if matches && spec.allow_positive_clues {
                    clues.push(positive);
                } else if !matches && spec.allow_negative_clues {
                    clues.push(LogicGridClue {
                        matches: false,
                        ..positive
                    });
                }
            }
        }
    }
    clues
}

fn validate_grid_assignments(
    size: u8,
    assignments: &[LogicGridAssignment],
) -> Result<(), &'static str> {
    if assignments.len() != usize::from(size) {
        return Err("logic-grid answer has the wrong number of travelers");
    }
    for field in [
        assignments
            .iter()
            .map(|item| item.traveler)
            .collect::<Vec<_>>(),
        assignments
            .iter()
            .map(|item| item.token)
            .collect::<Vec<_>>(),
        assignments.iter().map(|item| item.road).collect::<Vec<_>>(),
    ] {
        let mut sorted = field;
        sorted.sort_unstable();
        if sorted != (0..size).collect::<Vec<_>>() {
            return Err("logic-grid answer must use every value exactly once");
        }
    }
    if assignments
        .iter()
        .enumerate()
        .any(|(index, item)| usize::from(item.traveler) != index)
    {
        return Err("logic-grid assignments must be ordered by traveler");
    }
    Ok(())
}

fn permutations(size: u8) -> Vec<Vec<u8>> {
    fn visit(prefix: &mut Vec<u8>, remaining: &mut Vec<u8>, output: &mut Vec<Vec<u8>>) {
        if remaining.is_empty() {
            output.push(prefix.clone());
            return;
        }
        for index in 0..remaining.len() {
            let value = remaining.remove(index);
            prefix.push(value);
            visit(prefix, remaining, output);
            prefix.pop();
            remaining.insert(index, value);
        }
    }
    let mut output = Vec::new();
    visit(&mut Vec::new(), &mut (0..size).collect(), &mut output);
    output
}

fn traveler(index: u8) -> &'static str {
    TRAVELERS[usize::from(index)]
}

fn token(index: u8) -> &'static str {
    TOKENS[usize::from(index)]
}

fn road(index: u8) -> &'static str {
    ROADS[usize::from(index)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_grids_are_unique_irredundant_and_observer_safe() {
        for size in 3..=4 {
            for seed in 0..250 {
                let puzzle = LogicGridPuzzle::generate_with_spec(
                    seed,
                    LogicGridSpec {
                        size,
                        max_clues: 10,
                        ..LogicGridSpec::default()
                    },
                )
                .unwrap();
                puzzle.validate().unwrap();
                let projection = serde_json::to_string(&puzzle.projection()).unwrap();
                assert!(!projection.contains("solution"));
            }
        }
    }
}
