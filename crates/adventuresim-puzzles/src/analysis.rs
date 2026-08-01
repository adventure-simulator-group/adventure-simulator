use serde::{Deserialize, Serialize};

use crate::{
    PuzzleAuthority, PuzzleKind, RuneExample, RuneGate, RuneGateLaw, RuneOperation, WitnessPath,
    all_grid_solutions, allocation_legal_packs, allocation_optimal_packs, apply_rune_route,
    grid_solutions, rune_operations_consistent_with, solutions, witness_solutions,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PuzzleAnalysis {
    pub kind: PuzzleKind,
    pub fact_count: u16,
    pub initial_hypotheses: u32,
    pub final_hypotheses: u32,
    pub hypotheses_after_each_fact: Vec<u32>,
    pub independently_inferred_rules: u16,
    pub application_depth: u16,
    pub working_memory_items: u16,
    pub possible_answers: u16,
    pub all_facts_necessary_for_full_model: bool,
    pub all_facts_necessary_for_answer: bool,
    /// Cumulative base-two hypothesis space encountered while incorporating
    /// facts in their presented order, including the initial state.
    pub hypothesis_load: f32,
    /// Experimental structural score for comparing generated populations. It
    /// is not yet a calibrated measure of human or character difficulty.
    pub structural_complexity: f32,
}

impl PuzzleAnalysis {
    fn finish(mut self) -> Self {
        self.hypothesis_load = (self.initial_hypotheses.max(1) as f32).log2()
            + self
                .hypotheses_after_each_fact
                .iter()
                .map(|count| ((*count).max(1) as f32).log2())
                .sum::<f32>();
        self.structural_complexity = self.hypothesis_load
            + f32::from(self.fact_count)
            + f32::from(self.independently_inferred_rules) * 2.0
            + f32::from(self.application_depth) * 1.5
            + f32::from(self.working_memory_items) * 0.5;
        self
    }
}

impl PuzzleAuthority {
    pub fn analysis(&self) -> PuzzleAnalysis {
        match self {
            Self::OrderedSigils(puzzle) => {
                let counts = (1..=puzzle.clues.len())
                    .map(|end| solutions(&puzzle.clues[..end], usize::MAX).len() as u32)
                    .collect::<Vec<_>>();
                let necessary = (0..puzzle.clues.len()).all(|removed| {
                    let reduced = puzzle
                        .clues
                        .iter()
                        .cloned()
                        .enumerate()
                        .filter_map(|(index, clue)| (index != removed).then_some(clue))
                        .collect::<Vec<_>>();
                    solutions(&reduced, 2).len() != 1
                });
                PuzzleAnalysis {
                    kind: PuzzleKind::OrderedSigils,
                    fact_count: puzzle.clues.len() as u16,
                    initial_hypotheses: 120,
                    final_hypotheses: counts.last().copied().unwrap_or(120),
                    hypotheses_after_each_fact: counts,
                    independently_inferred_rules: 0,
                    application_depth: puzzle.clues.len() as u16,
                    working_memory_items: 5,
                    possible_answers: 1,
                    all_facts_necessary_for_full_model: necessary,
                    all_facts_necessary_for_answer: necessary,
                    hypothesis_load: 0.0,
                    structural_complexity: 0.0,
                }
                .finish()
            }
            Self::TruthfulWitnesses(puzzle) => {
                let counts = (1..=puzzle.statements.len())
                    .map(|end| witness_solutions(&puzzle.statements[..end]).len() as u32)
                    .collect::<Vec<_>>();
                let conclusions = |statements: &[crate::WitnessStatement]| {
                    let mut paths = witness_solutions(statements)
                        .into_iter()
                        .map(|(path, _)| path)
                        .collect::<Vec<WitnessPath>>();
                    paths.sort();
                    paths.dedup();
                    paths
                };
                let answer_necessary = (0..puzzle.statements.len()).all(|removed| {
                    let reduced = puzzle
                        .statements
                        .iter()
                        .copied()
                        .enumerate()
                        .filter_map(|(index, statement)| (index != removed).then_some(statement))
                        .collect::<Vec<_>>();
                    conclusions(&reduced).len() != 1
                });
                let final_hypotheses = witness_solutions(&puzzle.statements).len() as u32;
                PuzzleAnalysis {
                    kind: PuzzleKind::TruthfulWitnesses,
                    fact_count: puzzle.statements.len() as u16,
                    initial_hypotheses: 9,
                    final_hypotheses,
                    hypotheses_after_each_fact: counts,
                    independently_inferred_rules: u16::from(final_hypotheses == 1),
                    application_depth: puzzle.statements.len() as u16,
                    working_memory_items: puzzle.statements.len() as u16,
                    possible_answers: conclusions(&puzzle.statements).len() as u16,
                    all_facts_necessary_for_full_model: puzzle.spec.require_unique_liar
                        && final_hypotheses == 1
                        && answer_necessary,
                    all_facts_necessary_for_answer: answer_necessary,
                    hypothesis_load: 0.0,
                    structural_complexity: 0.0,
                }
                .finish()
            }
            Self::RuneTransformation(puzzle) => {
                let hypotheses = |examples: &[RuneExample]| {
                    puzzle
                        .gate_laws
                        .iter()
                        .map(|law| {
                            rune_operations_consistent_with(
                                &examples
                                    .iter()
                                    .copied()
                                    .filter(|example| example.gate == law.gate)
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .collect::<Vec<_>>()
                };
                let hypothesis_count = |sets: &[Vec<RuneOperation>]| {
                    sets.iter()
                        .fold(1_u32, |total, set| total.saturating_mul(set.len() as u32))
                };
                let counts = (1..=puzzle.examples.len())
                    .map(|end| hypothesis_count(&hypotheses(&puzzle.examples[..end])))
                    .collect::<Vec<_>>();
                let full_sets = hypotheses(&puzzle.examples);
                let possible_answers = rune_answers(
                    &puzzle
                        .gate_laws
                        .iter()
                        .map(|law| law.gate)
                        .collect::<Vec<_>>(),
                    &full_sets,
                    &puzzle.route,
                    puzzle.query,
                );
                let full_model_necessary = (0..puzzle.examples.len()).all(|removed| {
                    let reduced = puzzle
                        .examples
                        .iter()
                        .copied()
                        .enumerate()
                        .filter_map(|(index, example)| (index != removed).then_some(example))
                        .collect::<Vec<_>>();
                    hypothesis_count(&hypotheses(&reduced)) > 1
                });
                let answer_necessary = (0..puzzle.examples.len()).all(|removed| {
                    let reduced = puzzle
                        .examples
                        .iter()
                        .copied()
                        .enumerate()
                        .filter_map(|(index, example)| (index != removed).then_some(example))
                        .collect::<Vec<_>>();
                    let sets = hypotheses(&reduced);
                    rune_answers(
                        &puzzle
                            .gate_laws
                            .iter()
                            .map(|law| law.gate)
                            .collect::<Vec<_>>(),
                        &sets,
                        &puzzle.route,
                        puzzle.query,
                    )
                    .len()
                        > 1
                });
                PuzzleAnalysis {
                    kind: PuzzleKind::RuneTransformation,
                    fact_count: puzzle.examples.len() as u16,
                    initial_hypotheses: 5_u32.pow(puzzle.gate_laws.len() as u32),
                    final_hypotheses: hypothesis_count(&full_sets),
                    hypotheses_after_each_fact: counts,
                    independently_inferred_rules: full_sets
                        .iter()
                        .filter(|set| set.len() == 1)
                        .count() as u16,
                    application_depth: puzzle.route.len() as u16,
                    working_memory_items: (puzzle.gate_laws.len() + 1) as u16,
                    possible_answers: possible_answers.len() as u16,
                    all_facts_necessary_for_full_model: full_model_necessary,
                    all_facts_necessary_for_answer: answer_necessary,
                    hypothesis_load: 0.0,
                    structural_complexity: 0.0,
                }
                .finish()
            }
            Self::LogicGrid(puzzle) => {
                let counts = (1..=puzzle.clues.len())
                    .map(|end| {
                        grid_solutions(puzzle.spec.size, &puzzle.clues[..end], usize::MAX).len()
                            as u32
                    })
                    .collect::<Vec<_>>();
                let necessary = (0..puzzle.clues.len()).all(|removed| {
                    let reduced = puzzle
                        .clues
                        .iter()
                        .copied()
                        .enumerate()
                        .filter_map(|(index, clue)| (index != removed).then_some(clue))
                        .collect::<Vec<_>>();
                    grid_solutions(puzzle.spec.size, &reduced, 2).len() != 1
                });
                PuzzleAnalysis {
                    kind: PuzzleKind::LogicGrid,
                    fact_count: puzzle.clues.len() as u16,
                    initial_hypotheses: all_grid_solutions(puzzle.spec.size).len() as u32,
                    final_hypotheses: counts.last().copied().unwrap_or(0),
                    hypotheses_after_each_fact: counts,
                    independently_inferred_rules: 0,
                    application_depth: puzzle.clues.len() as u16,
                    working_memory_items: u16::from(puzzle.spec.size) * 3,
                    possible_answers: 1,
                    all_facts_necessary_for_full_model: necessary,
                    all_facts_necessary_for_answer: necessary,
                    hypothesis_load: 0.0,
                    structural_complexity: 0.0,
                }
                .finish()
            }
            Self::ResourceAllocation(puzzle) => {
                let capacity_count =
                    allocation_legal_packs(&puzzle.provisions, &[], puzzle.spec.capacity).len();
                let mut counts = vec![capacity_count as u32];
                counts.extend((1..=puzzle.hazards.len()).map(|end| {
                    allocation_legal_packs(
                        &puzzle.provisions,
                        &puzzle.hazards[..end],
                        puzzle.spec.capacity,
                    )
                    .len() as u32
                }));
                let optimal = allocation_optimal_packs(
                    &puzzle.provisions,
                    &puzzle.hazards,
                    puzzle.spec.capacity,
                );
                counts.push(optimal.len() as u32);
                let hazards_necessary = (0..puzzle.hazards.len()).all(|removed| {
                    let reduced = puzzle
                        .hazards
                        .iter()
                        .copied()
                        .enumerate()
                        .filter_map(|(index, hazard)| (index != removed).then_some(hazard))
                        .collect::<Vec<_>>();
                    allocation_optimal_packs(&puzzle.provisions, &reduced, puzzle.spec.capacity)
                        != optimal
                });
                let capacity_necessary =
                    allocation_optimal_packs(&puzzle.provisions, &puzzle.hazards, u8::MAX)
                        != optimal;
                let objective_necessary = allocation_legal_packs(
                    &puzzle.provisions,
                    &puzzle.hazards,
                    puzzle.spec.capacity,
                )
                .len()
                    > 1;
                let necessary = hazards_necessary && capacity_necessary && objective_necessary;
                PuzzleAnalysis {
                    kind: PuzzleKind::ResourceAllocation,
                    fact_count: (puzzle.hazards.len() + 2) as u16,
                    initial_hypotheses: 1_u32 << puzzle.provisions.len(),
                    final_hypotheses: optimal.len() as u32,
                    hypotheses_after_each_fact: counts,
                    independently_inferred_rules: puzzle.hazards.len() as u16,
                    application_depth: (puzzle.hazards.len() + 2) as u16,
                    working_memory_items: puzzle.provisions.len() as u16,
                    possible_answers: optimal.len() as u16,
                    all_facts_necessary_for_full_model: necessary,
                    all_facts_necessary_for_answer: necessary,
                    hypothesis_load: 0.0,
                    structural_complexity: 0.0,
                }
                .finish()
            }
        }
    }
}

fn rune_answers(
    gates: &[RuneGate],
    candidates: &[Vec<RuneOperation>],
    route: &[RuneGate],
    query: crate::Sigil,
) -> Vec<crate::Sigil> {
    fn visit(
        index: usize,
        gates: &[RuneGate],
        candidates: &[Vec<RuneOperation>],
        laws: &mut Vec<RuneGateLaw>,
        route: &[RuneGate],
        query: crate::Sigil,
        answers: &mut Vec<crate::Sigil>,
    ) {
        if index == gates.len() {
            if let Some(answer) = apply_rune_route(laws, route, query) {
                answers.push(answer);
            }
            return;
        }
        for operation in &candidates[index] {
            laws.push(RuneGateLaw {
                gate: gates[index],
                operation: *operation,
            });
            visit(index + 1, gates, candidates, laws, route, query, answers);
            laws.pop();
        }
    }
    let mut answers = Vec::new();
    visit(
        0,
        gates,
        candidates,
        &mut Vec::new(),
        route,
        query,
        &mut answers,
    );
    answers.sort();
    answers.dedup();
    answers
}
