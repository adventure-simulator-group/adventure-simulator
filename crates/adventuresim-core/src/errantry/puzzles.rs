use serde::{Deserialize, Serialize};

use super::{OrderedSigilProjection, OrderedSigilPuzzle, OrderedSigilSubmission, Sigil};

pub const TRUTHFUL_WITNESS_RULES_VERSION: u16 = 1;
pub const RUNE_TRANSFORMATION_RULES_VERSION: u16 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PuzzleKind {
    OrderedSigils,
    TruthfulWitnesses,
    RuneTransformation,
}

impl PuzzleKind {
    pub const ALL: [Self; 3] = [
        Self::OrderedSigils,
        Self::TruthfulWitnesses,
        Self::RuneTransformation,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::OrderedSigils => "ordered-sigils",
            Self::TruthfulWitnesses => "truthful-witnesses",
            Self::RuneTransformation => "rune-transformation",
        }
    }

    pub fn from_slug(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.slug() == value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "puzzle", rename_all = "snake_case")]
pub enum PuzzleAuthority {
    OrderedSigils(OrderedSigilPuzzle),
    TruthfulWitnesses(TruthfulWitnessPuzzle),
    RuneTransformation(RuneTransformationPuzzle),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "puzzle", rename_all = "snake_case")]
pub enum PuzzleProjection {
    OrderedSigils(OrderedSigilProjection),
    TruthfulWitnesses(TruthfulWitnessProjection),
    RuneTransformation(RuneTransformationProjection),
}

impl PuzzleProjection {
    pub const fn kind(&self) -> PuzzleKind {
        match self {
            Self::OrderedSigils(_) => PuzzleKind::OrderedSigils,
            Self::TruthfulWitnesses(_) => PuzzleKind::TruthfulWitnesses,
            Self::RuneTransformation(_) => PuzzleKind::RuneTransformation,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "answer", rename_all = "snake_case")]
pub enum PuzzleSubmission {
    OrderedSigils { ordering: [Sigil; 5] },
    TruthfulWitnesses { safe_path: WitnessPath },
    RuneTransformation { result: Sigil },
}

impl PuzzleSubmission {
    pub const fn kind(&self) -> PuzzleKind {
        match self {
            Self::OrderedSigils { .. } => PuzzleKind::OrderedSigils,
            Self::TruthfulWitnesses { .. } => PuzzleKind::TruthfulWitnesses,
            Self::RuneTransformation { .. } => PuzzleKind::RuneTransformation,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PuzzleSubmissionError {
    WrongKind,
    Malformed,
}

impl PuzzleAuthority {
    pub fn generate(kind: PuzzleKind, seed: u64) -> Self {
        match kind {
            PuzzleKind::OrderedSigils => Self::OrderedSigils(OrderedSigilPuzzle::generate(seed)),
            PuzzleKind::TruthfulWitnesses => {
                Self::TruthfulWitnesses(TruthfulWitnessPuzzle::generate(seed))
            }
            PuzzleKind::RuneTransformation => {
                Self::RuneTransformation(RuneTransformationPuzzle::generate(seed))
            }
        }
    }

    pub const fn kind(&self) -> PuzzleKind {
        match self {
            Self::OrderedSigils(_) => PuzzleKind::OrderedSigils,
            Self::TruthfulWitnesses(_) => PuzzleKind::TruthfulWitnesses,
            Self::RuneTransformation(_) => PuzzleKind::RuneTransformation,
        }
    }

    pub fn projection(&self) -> PuzzleProjection {
        match self {
            Self::OrderedSigils(puzzle) => PuzzleProjection::OrderedSigils(puzzle.projection()),
            Self::TruthfulWitnesses(puzzle) => {
                PuzzleProjection::TruthfulWitnesses(puzzle.projection())
            }
            Self::RuneTransformation(puzzle) => {
                PuzzleProjection::RuneTransformation(puzzle.projection())
            }
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::OrderedSigils(puzzle) => puzzle.validate(),
            Self::TruthfulWitnesses(puzzle) => puzzle.validate(),
            Self::RuneTransformation(puzzle) => puzzle.validate(),
        }
    }

    pub fn replay(&self) -> Result<Self, &'static str> {
        match self {
            Self::OrderedSigils(puzzle) => Ok(Self::OrderedSigils(
                OrderedSigilPuzzle::generate_versioned(puzzle.rules_version, puzzle.seed)?,
            )),
            Self::TruthfulWitnesses(puzzle) => Ok(Self::TruthfulWitnesses(
                TruthfulWitnessPuzzle::generate_versioned(puzzle.rules_version, puzzle.seed)?,
            )),
            Self::RuneTransformation(puzzle) => Ok(Self::RuneTransformation(
                RuneTransformationPuzzle::generate_versioned(puzzle.rules_version, puzzle.seed)?,
            )),
        }
    }

    pub fn check(&self, submission: &PuzzleSubmission) -> Result<bool, PuzzleSubmissionError> {
        match (self, submission) {
            (Self::OrderedSigils(puzzle), PuzzleSubmission::OrderedSigils { ordering }) => puzzle
                .check(&OrderedSigilSubmission {
                    expected_revision: 0,
                    ordering: *ordering,
                })
                .map_err(|_| PuzzleSubmissionError::Malformed),
            (
                Self::TruthfulWitnesses(puzzle),
                PuzzleSubmission::TruthfulWitnesses { safe_path },
            ) => Ok(*safe_path == puzzle.solution_path),
            (Self::RuneTransformation(puzzle), PuzzleSubmission::RuneTransformation { result }) => {
                Ok(*result == puzzle.solution)
            }
            _ => Err(PuzzleSubmissionError::WrongKind),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Witness {
    Crown,
    Hart,
    Moon,
}

impl Witness {
    pub const ALL: [Self; 3] = [Self::Crown, Self::Hart, Self::Moon];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Crown => "Crown",
            Self::Hart => "Hart",
            Self::Moon => "Moon",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WitnessPath {
    Ash,
    Moon,
    Thorn,
}

impl WitnessPath {
    pub const ALL: [Self; 3] = [Self::Ash, Self::Moon, Self::Thorn];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ash => "Ash path",
            Self::Moon => "Moon path",
            Self::Thorn => "Thorn path",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WitnessClaim {
    PathIsSafe(WitnessPath),
    PathIsUnsafe(WitnessPath),
    WitnessLies(Witness),
    WitnessSpeaksTruth(Witness),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessStatement {
    pub speaker: Witness,
    pub claim: WitnessClaim,
}

impl WitnessStatement {
    pub fn text(self) -> String {
        let claim = match self.claim {
            WitnessClaim::PathIsSafe(path) => format!("the {} is safe", path.label()),
            WitnessClaim::PathIsUnsafe(path) => format!("the {} is unsafe", path.label()),
            WitnessClaim::WitnessLies(witness) => {
                format!("the {} witness is the liar", witness.label())
            }
            WitnessClaim::WitnessSpeaksTruth(witness) => {
                format!("the {} witness speaks truth", witness.label())
            }
        };
        format!("The {} witness says that {}.", self.speaker.label(), claim)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TruthfulWitnessPuzzle {
    pub rules_version: u16,
    pub seed: u64,
    pub solution_path: WitnessPath,
    pub liar: Witness,
    pub statements: [WitnessStatement; 3],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TruthfulWitnessProjection {
    pub rules_version: u16,
    pub witnesses: [Witness; 3],
    pub paths: [WitnessPath; 3],
    pub statements: [WitnessStatement; 3],
}

impl TruthfulWitnessPuzzle {
    pub fn generate(seed: u64) -> Self {
        Self::generate_versioned(TRUTHFUL_WITNESS_RULES_VERSION, seed)
            .expect("current truthful-witness rules are supported")
    }

    pub fn generate_versioned(rules_version: u16, seed: u64) -> Result<Self, &'static str> {
        if rules_version != TRUTHFUL_WITNESS_RULES_VERSION {
            return Err("unsupported truthful-witness rules version");
        }
        let mut rng = PuzzleRng(seed ^ 0x7472_7574_685f_7769);
        let solution_path = WitnessPath::ALL[(rng.next() as usize) % WitnessPath::ALL.len()];
        let liar = Witness::ALL[(rng.next() as usize) % Witness::ALL.len()];
        let choices = Witness::ALL.map(|speaker| {
            let mut claims = witness_claims()
                .into_iter()
                .filter(|claim| claim_is_true(*claim, solution_path, liar) == (speaker != liar))
                .filter(|claim| !matches!(claim, WitnessClaim::WitnessLies(w) | WitnessClaim::WitnessSpeaksTruth(w) if *w == speaker))
                .collect::<Vec<_>>();
            shuffle(&mut claims, &mut rng);
            claims
        });
        let mut selected = None;
        'outer: for &first in &choices[0] {
            for &second in &choices[1] {
                for &third in &choices[2] {
                    let statements = [
                        WitnessStatement {
                            speaker: Witness::Crown,
                            claim: first,
                        },
                        WitnessStatement {
                            speaker: Witness::Hart,
                            claim: second,
                        },
                        WitnessStatement {
                            speaker: Witness::Moon,
                            claim: third,
                        },
                    ];
                    let legal = witness_solutions(&statements);
                    if !legal.is_empty()
                        && legal.iter().all(|(path, _)| *path == solution_path)
                        && statement_set_is_irredundant(&statements)
                    {
                        selected = Some(statements);
                        break 'outer;
                    }
                }
            }
        }
        let puzzle = Self {
            rules_version,
            seed,
            solution_path,
            liar,
            statements: selected.ok_or("could not generate a unique truthful-witness puzzle")?,
        };
        puzzle.validate()?;
        Ok(puzzle)
    }

    pub fn projection(&self) -> TruthfulWitnessProjection {
        TruthfulWitnessProjection {
            rules_version: self.rules_version,
            witnesses: Witness::ALL,
            paths: WitnessPath::ALL,
            statements: self.statements,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.rules_version != TRUTHFUL_WITNESS_RULES_VERSION {
            return Err("unsupported truthful-witness rules version");
        }
        if self.statements.map(|statement| statement.speaker) != Witness::ALL {
            return Err("truthful-witness speakers are malformed");
        }
        for statement in self.statements {
            if claim_is_true(statement.claim, self.solution_path, self.liar)
                != (statement.speaker != self.liar)
            {
                return Err("truthful-witness statement contradicts private authority");
            }
        }
        let legal = witness_solutions(&self.statements);
        if legal.is_empty() || !legal.iter().all(|(path, _)| *path == self.solution_path) {
            return Err("truthful-witness statements do not prove one safe path");
        }
        if !statement_set_is_irredundant(&self.statements) {
            return Err("truthful-witness puzzle contains a redundant statement");
        }
        Ok(())
    }
}

fn witness_claims() -> Vec<WitnessClaim> {
    let mut claims = Vec::new();
    for path in WitnessPath::ALL {
        claims.push(WitnessClaim::PathIsSafe(path));
        claims.push(WitnessClaim::PathIsUnsafe(path));
    }
    for witness in Witness::ALL {
        claims.push(WitnessClaim::WitnessLies(witness));
        claims.push(WitnessClaim::WitnessSpeaksTruth(witness));
    }
    claims
}

fn claim_is_true(claim: WitnessClaim, safe_path: WitnessPath, liar: Witness) -> bool {
    match claim {
        WitnessClaim::PathIsSafe(path) => path == safe_path,
        WitnessClaim::PathIsUnsafe(path) => path != safe_path,
        WitnessClaim::WitnessLies(witness) => witness == liar,
        WitnessClaim::WitnessSpeaksTruth(witness) => witness != liar,
    }
}

fn witness_solutions(statements: &[WitnessStatement]) -> Vec<(WitnessPath, Witness)> {
    let mut solutions = Vec::new();
    for path in WitnessPath::ALL {
        for liar in Witness::ALL {
            if statements.iter().all(|statement| {
                claim_is_true(statement.claim, path, liar) == (statement.speaker != liar)
            }) {
                solutions.push((path, liar));
            }
        }
    }
    solutions
}

fn statement_set_is_irredundant(statements: &[WitnessStatement; 3]) -> bool {
    (0..statements.len()).all(|removed| {
        let reduced = statements
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, statement)| (index != removed).then_some(statement))
            .collect::<Vec<_>>();
        let solutions = witness_solutions(&reduced);
        let mut paths = solutions.iter().map(|(path, _)| *path).collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        paths.len() != 1
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuneOperation {
    ExchangeCrownHart,
    ExchangeHartMoon,
    ExchangeMoonRose,
    ExchangeRoseSword,
    ExchangeSwordCrown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RuneGate {
    Ash,
    Briar,
    Glass,
}

impl RuneGate {
    pub const ALL: [Self; 3] = [Self::Ash, Self::Briar, Self::Glass];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ash => "Gate of Ash",
            Self::Briar => "Gate of Briar",
            Self::Glass => "Gate of Glass",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Ash => 0,
            Self::Briar => 1,
            Self::Glass => 2,
        }
    }
}

impl RuneOperation {
    pub const ALL: [Self; 5] = [
        Self::ExchangeCrownHart,
        Self::ExchangeHartMoon,
        Self::ExchangeMoonRose,
        Self::ExchangeRoseSword,
        Self::ExchangeSwordCrown,
    ];

    pub const fn rule_text(self) -> &'static str {
        match self {
            Self::ExchangeCrownHart => {
                "Exchange Crown with Hart; leave every other sigil unchanged."
            }
            Self::ExchangeHartMoon => "Exchange Hart with Moon; leave every other sigil unchanged.",
            Self::ExchangeMoonRose => "Exchange Moon with Rose; leave every other sigil unchanged.",
            Self::ExchangeRoseSword => {
                "Exchange Rose with Sword; leave every other sigil unchanged."
            }
            Self::ExchangeSwordCrown => {
                "Exchange Sword with Crown; leave every other sigil unchanged."
            }
        }
    }

    fn apply(self, input: Sigil) -> Sigil {
        let pair = match self {
            Self::ExchangeCrownHart => (Sigil::Crown, Sigil::Hart),
            Self::ExchangeHartMoon => (Sigil::Hart, Sigil::Moon),
            Self::ExchangeMoonRose => (Sigil::Moon, Sigil::Rose),
            Self::ExchangeRoseSword => (Sigil::Rose, Sigil::Sword),
            Self::ExchangeSwordCrown => (Sigil::Sword, Sigil::Crown),
        };
        if input == pair.0 {
            pair.1
        } else if input == pair.1 {
            pair.0
        } else {
            input
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuneExample {
    pub gate: RuneGate,
    pub input: Sigil,
    pub output: Sigil,
}

impl RuneExample {
    pub fn text(self) -> String {
        format!(
            "At the {}, when the {} enters, the {} emerges.",
            self.gate.label(),
            self.input.label(),
            self.output.label()
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuneTransformationPuzzle {
    pub rules_version: u16,
    pub seed: u64,
    pub gate_operations: [RuneOperation; 3],
    pub examples: Vec<RuneExample>,
    pub route: [RuneGate; 3],
    pub query: Sigil,
    pub solution: Sigil,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuneTransformationProjection {
    pub rules_version: u16,
    pub sigils: [Sigil; 5],
    pub candidate_rules: [RuneOperation; 5],
    pub examples: Vec<RuneExample>,
    pub route: [RuneGate; 3],
    pub query: Sigil,
}

impl RuneTransformationPuzzle {
    pub fn generate(seed: u64) -> Self {
        Self::generate_versioned(RUNE_TRANSFORMATION_RULES_VERSION, seed)
            .expect("current rune-transformation rules are supported")
    }

    pub fn generate_versioned(rules_version: u16, seed: u64) -> Result<Self, &'static str> {
        if rules_version != RUNE_TRANSFORMATION_RULES_VERSION {
            return Err("unsupported rune-transformation rules version");
        }
        let mut rng = PuzzleRng(seed ^ 0x7275_6e65_5f74_7261);
        let gate_operations = RuneGate::ALL
            .map(|_| RuneOperation::ALL[(rng.next() as usize) % RuneOperation::ALL.len()]);
        let query = Sigil::ALL[(rng.next() as usize) % Sigil::ALL.len()];
        let mut route = RuneGate::ALL;
        shuffle(&mut route, &mut rng);
        let mut examples = Vec::with_capacity(6);
        for gate in RuneGate::ALL {
            let operation = gate_operations[gate.index()];
            let mut inputs = Sigil::ALL;
            shuffle(&mut inputs, &mut rng);
            let pair = inputs
                .iter()
                .enumerate()
                .flat_map(|(first, left)| {
                    inputs
                        .iter()
                        .skip(first + 1)
                        .map(move |right| (*left, *right))
                })
                .find(|(left, right)| {
                    rune_operations_consistent_with(&[RuneExample {
                        gate,
                        input: *left,
                        output: operation.apply(*left),
                    }])
                    .len()
                        > 1
                        && rune_operations_consistent_with(&[RuneExample {
                            gate,
                            input: *right,
                            output: operation.apply(*right),
                        }])
                        .len()
                            > 1
                        && rune_operations_consistent_with(&[
                            RuneExample {
                                gate,
                                input: *left,
                                output: operation.apply(*left),
                            },
                            RuneExample {
                                gate,
                                input: *right,
                                output: operation.apply(*right),
                            },
                        ]) == vec![operation]
                })
                .ok_or("could not generate an irredundant rune gate")?;
            examples.extend([pair.0, pair.1].map(|input| RuneExample {
                gate,
                input,
                output: operation.apply(input),
            }));
        }
        shuffle(&mut examples, &mut rng);
        let solution = apply_rune_route(&gate_operations, route, query);
        let puzzle = Self {
            rules_version,
            seed,
            gate_operations,
            examples,
            route,
            query,
            solution,
        };
        puzzle.validate()?;
        Ok(puzzle)
    }

    pub fn projection(&self) -> RuneTransformationProjection {
        RuneTransformationProjection {
            rules_version: self.rules_version,
            sigils: Sigil::ALL,
            candidate_rules: RuneOperation::ALL,
            examples: self.examples.clone(),
            route: self.route,
            query: self.query,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.rules_version != RUNE_TRANSFORMATION_RULES_VERSION {
            return Err("unsupported rune-transformation rules version");
        }
        if self.examples.len() != 6 {
            return Err("rune transformation must contain two examples per gate");
        }
        if self.solution != apply_rune_route(&self.gate_operations, self.route, self.query)
            || self.examples.iter().any(|example| {
                example.output != self.gate_operations[example.gate.index()].apply(example.input)
            })
            || !RuneGate::ALL.iter().all(|gate| {
                self.route
                    .iter()
                    .filter(|candidate| *candidate == gate)
                    .count()
                    == 1
            })
        {
            return Err("rune transformation contradicts private authority");
        }
        for gate in RuneGate::ALL {
            let gate_examples = self
                .examples
                .iter()
                .copied()
                .filter(|example| example.gate == gate)
                .collect::<Vec<_>>();
            if gate_examples.len() != 2
                || rune_operations_consistent_with(&gate_examples)
                    != vec![self.gate_operations[gate.index()]]
            {
                return Err("rune examples do not prove every gate law");
            }
            for example in &gate_examples {
                if rune_operations_consistent_with(&[*example]).len() <= 1 {
                    return Err("rune transformation contains a redundant example");
                }
            }
        }
        Ok(())
    }
}

fn rune_operations_consistent_with(examples: &[RuneExample]) -> Vec<RuneOperation> {
    RuneOperation::ALL
        .into_iter()
        .filter(|operation| {
            examples
                .iter()
                .all(|example| operation.apply(example.input) == example.output)
        })
        .collect()
}

fn apply_rune_route(operations: &[RuneOperation; 3], route: [RuneGate; 3], input: Sigil) -> Sigil {
    route.into_iter().fold(input, |current, gate| {
        operations[gate.index()].apply(current)
    })
}

fn shuffle<T>(values: &mut [T], rng: &mut PuzzleRng) {
    for end in (1..values.len()).rev() {
        let selected = (rng.next() as usize) % (end + 1);
        values.swap(end, selected);
    }
}

struct PuzzleRng(u64);

impl PuzzleRng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_puzzle_kinds_round_trip_their_slugs() {
        for kind in PuzzleKind::ALL {
            assert_eq!(PuzzleKind::from_slug(kind.slug()), Some(kind));
        }
    }

    #[test]
    fn generated_witness_puzzles_prove_one_safe_path_without_redundant_statements() {
        for seed in 0..1_000 {
            let puzzle = TruthfulWitnessPuzzle::generate(seed);
            puzzle.validate().unwrap();
            assert!(
                !serde_json::to_string(&puzzle.projection())
                    .unwrap()
                    .contains("solution_path")
            );
            assert!(
                !serde_json::to_string(&puzzle.projection())
                    .unwrap()
                    .contains("liar")
            );
        }
    }

    #[test]
    fn generated_rune_puzzles_prove_one_result_without_exposing_the_chosen_rule() {
        for seed in 0..1_000 {
            let puzzle = RuneTransformationPuzzle::generate(seed);
            puzzle.validate().unwrap();
            let projection = serde_json::to_string(&puzzle.projection()).unwrap();
            assert!(!projection.contains("gate_operations"));
            assert!(!projection.contains("solution"));
            assert_eq!(puzzle.projection().candidate_rules, RuneOperation::ALL);
            assert_eq!(puzzle.examples.len(), 6);
            for gate in RuneGate::ALL {
                let examples = puzzle
                    .examples
                    .iter()
                    .copied()
                    .filter(|example| example.gate == gate)
                    .collect::<Vec<_>>();
                assert_eq!(examples.len(), 2);
                assert_eq!(rune_operations_consistent_with(&examples).len(), 1);
                assert!(
                    examples
                        .iter()
                        .all(|example| rune_operations_consistent_with(&[*example]).len() > 1)
                );
            }
        }
    }

    #[test]
    fn puzzle_envelope_replays_and_checks_every_engine() {
        for (ordinal, kind) in PuzzleKind::ALL.into_iter().enumerate() {
            let puzzle = PuzzleAuthority::generate(kind, ordinal as u64 + 40);
            puzzle.validate().unwrap();
            assert_eq!(puzzle.replay().unwrap(), puzzle);
            let answer = match &puzzle {
                PuzzleAuthority::OrderedSigils(puzzle) => PuzzleSubmission::OrderedSigils {
                    ordering: puzzle.solution,
                },
                PuzzleAuthority::TruthfulWitnesses(puzzle) => PuzzleSubmission::TruthfulWitnesses {
                    safe_path: puzzle.solution_path,
                },
                PuzzleAuthority::RuneTransformation(puzzle) => {
                    PuzzleSubmission::RuneTransformation {
                        result: puzzle.solution,
                    }
                }
            };
            assert_eq!(puzzle.check(&answer), Ok(true));
        }
    }
}
