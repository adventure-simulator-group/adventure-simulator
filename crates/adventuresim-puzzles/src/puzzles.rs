use fabelgeist_determinism::SplitMix64;
use serde::{Deserialize, Serialize};

use super::{
    LogicGridProjection, LogicGridPuzzle, LogicGridSpec, OrderedSigilProjection,
    OrderedSigilPuzzle, OrderedSigilSpec, OrderedSigilSubmission, ProvisionId,
    ResourceAllocationProjection, ResourceAllocationPuzzle, ResourceAllocationSpec, Sigil,
};

pub const TRUTHFUL_WITNESS_RULES_VERSION: u16 = 2;
pub const RUNE_TRANSFORMATION_RULES_VERSION: u16 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PuzzleKind {
    OrderedSigils,
    TruthfulWitnesses,
    RuneTransformation,
    LogicGrid,
    ResourceAllocation,
}

impl PuzzleKind {
    pub const ALL: [Self; 5] = [
        Self::OrderedSigils,
        Self::TruthfulWitnesses,
        Self::RuneTransformation,
        Self::LogicGrid,
        Self::ResourceAllocation,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::OrderedSigils => "ordered-sigils",
            Self::TruthfulWitnesses => "truthful-witnesses",
            Self::RuneTransformation => "rune-transformation",
            Self::LogicGrid => "logic-grid",
            Self::ResourceAllocation => "resource-allocation",
        }
    }

    pub fn from_slug(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.slug() == value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationRequest {
    pub seed: u64,
    pub spec: PuzzleSpec,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "parameters", rename_all = "snake_case")]
pub enum PuzzleSpec {
    OrderedSigils(OrderedSigilSpec),
    TruthfulWitnesses(TruthfulWitnessSpec),
    RuneTransformation(RuneTransformationSpec),
    LogicGrid(LogicGridSpec),
    ResourceAllocation(ResourceAllocationSpec),
}

impl PuzzleSpec {
    pub const fn kind(&self) -> PuzzleKind {
        match self {
            Self::OrderedSigils(_) => PuzzleKind::OrderedSigils,
            Self::TruthfulWitnesses(_) => PuzzleKind::TruthfulWitnesses,
            Self::RuneTransformation(_) => PuzzleKind::RuneTransformation,
            Self::LogicGrid(_) => PuzzleKind::LogicGrid,
            Self::ResourceAllocation(_) => PuzzleKind::ResourceAllocation,
        }
    }

    pub fn standard(kind: PuzzleKind) -> Self {
        match kind {
            PuzzleKind::OrderedSigils => Self::OrderedSigils(OrderedSigilSpec::default()),
            PuzzleKind::TruthfulWitnesses => {
                Self::TruthfulWitnesses(TruthfulWitnessSpec::default())
            }
            PuzzleKind::RuneTransformation => {
                Self::RuneTransformation(RuneTransformationSpec::default())
            }
            PuzzleKind::LogicGrid => Self::LogicGrid(LogicGridSpec::default()),
            PuzzleKind::ResourceAllocation => {
                Self::ResourceAllocation(ResourceAllocationSpec::default())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "puzzle", rename_all = "snake_case")]
pub enum PuzzleAuthority {
    OrderedSigils(OrderedSigilPuzzle),
    TruthfulWitnesses(TruthfulWitnessPuzzle),
    RuneTransformation(RuneTransformationPuzzle),
    LogicGrid(LogicGridPuzzle),
    ResourceAllocation(ResourceAllocationPuzzle),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "puzzle", rename_all = "snake_case")]
pub enum PuzzleProjection {
    OrderedSigils(OrderedSigilProjection),
    TruthfulWitnesses(TruthfulWitnessProjection),
    RuneTransformation(RuneTransformationProjection),
    LogicGrid(LogicGridProjection),
    ResourceAllocation(ResourceAllocationProjection),
}

impl PuzzleProjection {
    pub const fn kind(&self) -> PuzzleKind {
        match self {
            Self::OrderedSigils(_) => PuzzleKind::OrderedSigils,
            Self::TruthfulWitnesses(_) => PuzzleKind::TruthfulWitnesses,
            Self::RuneTransformation(_) => PuzzleKind::RuneTransformation,
            Self::LogicGrid(_) => PuzzleKind::LogicGrid,
            Self::ResourceAllocation(_) => PuzzleKind::ResourceAllocation,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "answer", rename_all = "snake_case")]
pub enum PuzzleSubmission {
    OrderedSigils {
        ordering: [Sigil; 5],
    },
    TruthfulWitnesses {
        safe_path: WitnessPath,
    },
    RuneTransformation {
        result: Sigil,
    },
    LogicGrid {
        assignments: Vec<crate::LogicGridAssignment>,
    },
    ResourceAllocation {
        provisions: Vec<ProvisionId>,
    },
}

impl PuzzleSubmission {
    pub const fn kind(&self) -> PuzzleKind {
        match self {
            Self::OrderedSigils { .. } => PuzzleKind::OrderedSigils,
            Self::TruthfulWitnesses { .. } => PuzzleKind::TruthfulWitnesses,
            Self::RuneTransformation { .. } => PuzzleKind::RuneTransformation,
            Self::LogicGrid { .. } => PuzzleKind::LogicGrid,
            Self::ResourceAllocation { .. } => PuzzleKind::ResourceAllocation,
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
        Self::generate_request(GenerationRequest {
            seed,
            spec: PuzzleSpec::standard(kind),
        })
        .expect("standard puzzle specification is valid")
    }

    pub fn generate_request(request: GenerationRequest) -> Result<Self, &'static str> {
        let seed = request.seed;
        Ok(match request.spec {
            PuzzleSpec::OrderedSigils(spec) => {
                Self::OrderedSigils(OrderedSigilPuzzle::generate_with_spec(seed, spec)?)
            }
            PuzzleSpec::TruthfulWitnesses(spec) => {
                Self::TruthfulWitnesses(TruthfulWitnessPuzzle::generate_with_spec(seed, spec)?)
            }
            PuzzleSpec::RuneTransformation(spec) => {
                Self::RuneTransformation(RuneTransformationPuzzle::generate_with_spec(seed, spec)?)
            }
            PuzzleSpec::LogicGrid(spec) => {
                Self::LogicGrid(LogicGridPuzzle::generate_with_spec(seed, spec)?)
            }
            PuzzleSpec::ResourceAllocation(spec) => {
                Self::ResourceAllocation(ResourceAllocationPuzzle::generate_with_spec(seed, spec)?)
            }
        })
    }

    pub fn standard_request(kind: PuzzleKind, seed: u64) -> GenerationRequest {
        GenerationRequest {
            seed,
            spec: PuzzleSpec::standard(kind),
        }
    }

    pub fn generation_request(&self) -> GenerationRequest {
        match self {
            Self::OrderedSigils(puzzle) => GenerationRequest {
                seed: puzzle.seed,
                spec: PuzzleSpec::OrderedSigils(puzzle.spec),
            },
            Self::TruthfulWitnesses(puzzle) => GenerationRequest {
                seed: puzzle.seed,
                spec: PuzzleSpec::TruthfulWitnesses(puzzle.spec),
            },
            Self::RuneTransformation(puzzle) => GenerationRequest {
                seed: puzzle.seed,
                spec: PuzzleSpec::RuneTransformation(puzzle.spec),
            },
            Self::LogicGrid(puzzle) => GenerationRequest {
                seed: puzzle.seed,
                spec: PuzzleSpec::LogicGrid(puzzle.spec),
            },
            Self::ResourceAllocation(puzzle) => GenerationRequest {
                seed: puzzle.seed,
                spec: PuzzleSpec::ResourceAllocation(puzzle.spec),
            },
        }
    }

    pub const fn kind(&self) -> PuzzleKind {
        match self {
            Self::OrderedSigils(_) => PuzzleKind::OrderedSigils,
            Self::TruthfulWitnesses(_) => PuzzleKind::TruthfulWitnesses,
            Self::RuneTransformation(_) => PuzzleKind::RuneTransformation,
            Self::LogicGrid(_) => PuzzleKind::LogicGrid,
            Self::ResourceAllocation(_) => PuzzleKind::ResourceAllocation,
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
            Self::LogicGrid(puzzle) => PuzzleProjection::LogicGrid(puzzle.projection()),
            Self::ResourceAllocation(puzzle) => {
                PuzzleProjection::ResourceAllocation(puzzle.projection())
            }
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::OrderedSigils(puzzle) => puzzle.validate(),
            Self::TruthfulWitnesses(puzzle) => puzzle.validate(),
            Self::RuneTransformation(puzzle) => puzzle.validate(),
            Self::LogicGrid(puzzle) => puzzle.validate(),
            Self::ResourceAllocation(puzzle) => puzzle.validate(),
        }
    }

    pub fn replay(&self) -> Result<Self, &'static str> {
        match self {
            Self::OrderedSigils(puzzle) => Ok(Self::OrderedSigils(
                OrderedSigilPuzzle::generate_with_spec(puzzle.seed, puzzle.spec)?,
            )),
            Self::TruthfulWitnesses(puzzle) => Ok(Self::TruthfulWitnesses(
                TruthfulWitnessPuzzle::generate_with_spec(puzzle.seed, puzzle.spec)?,
            )),
            Self::RuneTransformation(puzzle) => Ok(Self::RuneTransformation(
                RuneTransformationPuzzle::generate_with_spec(puzzle.seed, puzzle.spec)?,
            )),
            Self::LogicGrid(puzzle) => Ok(Self::LogicGrid(LogicGridPuzzle::generate_with_spec(
                puzzle.seed,
                puzzle.spec,
            )?)),
            Self::ResourceAllocation(puzzle) => Ok(Self::ResourceAllocation(
                ResourceAllocationPuzzle::generate_with_spec(puzzle.seed, puzzle.spec)?,
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
            (Self::LogicGrid(puzzle), PuzzleSubmission::LogicGrid { assignments }) => puzzle
                .check(assignments)
                .map_err(|_| PuzzleSubmissionError::Malformed),
            (
                Self::ResourceAllocation(puzzle),
                PuzzleSubmission::ResourceAllocation { provisions },
            ) => puzzle
                .check(provisions)
                .map_err(|_| PuzzleSubmissionError::Malformed),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TruthfulWitnessSpec {
    pub allow_path_claims: bool,
    pub allow_witness_claims: bool,
    pub require_unique_liar: bool,
    pub require_irredundant: bool,
}

impl Default for TruthfulWitnessSpec {
    fn default() -> Self {
        Self {
            allow_path_claims: true,
            allow_witness_claims: true,
            require_unique_liar: false,
            require_irredundant: true,
        }
    }
}

impl TruthfulWitnessSpec {
    pub fn validate(self) -> Result<Self, &'static str> {
        if !(self.allow_path_claims || self.allow_witness_claims) {
            return Err("truthful-witness spec must enable a claim family");
        }
        Ok(self)
    }

    fn allows(self, claim: WitnessClaim) -> bool {
        match claim {
            WitnessClaim::PathIsSafe(_) | WitnessClaim::PathIsUnsafe(_) => self.allow_path_claims,
            WitnessClaim::WitnessLies(_) | WitnessClaim::WitnessSpeaksTruth(_) => {
                self.allow_witness_claims
            }
        }
    }
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
    pub spec: TruthfulWitnessSpec,
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
        Self::generate_with_spec(seed, TruthfulWitnessSpec::default())
            .expect("current truthful-witness rules are supported")
    }

    pub fn generate_versioned(rules_version: u16, seed: u64) -> Result<Self, &'static str> {
        if rules_version != TRUTHFUL_WITNESS_RULES_VERSION {
            return Err("unsupported truthful-witness rules version");
        }
        Self::generate_with_spec(seed, TruthfulWitnessSpec::default())
    }

    pub fn generate_with_spec(seed: u64, spec: TruthfulWitnessSpec) -> Result<Self, &'static str> {
        let spec = spec.validate()?;
        const TRUTHFUL_WITNESS_GENERATION_DOMAIN: u64 = 0x7472_7574_685f_7769;
        let mut rng = SplitMix64::new(seed ^ TRUTHFUL_WITNESS_GENERATION_DOMAIN);
        let solution_path = WitnessPath::ALL[rng.index(WitnessPath::ALL.len())];
        let liar = Witness::ALL[rng.index(Witness::ALL.len())];
        let choices = Witness::ALL.map(|speaker| {
            let mut claims = witness_claims()
                .into_iter()
                .filter(|claim| spec.allows(*claim))
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
                        && (!spec.require_unique_liar
                            || legal.iter().all(|(_, candidate)| *candidate == liar))
                        && (!spec.require_irredundant || statement_set_is_irredundant(&statements))
                    {
                        selected = Some(statements);
                        break 'outer;
                    }
                }
            }
        }
        let puzzle = Self {
            rules_version: TRUTHFUL_WITNESS_RULES_VERSION,
            seed,
            spec,
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
        self.spec.validate()?;
        if self
            .statements
            .iter()
            .any(|statement| !self.spec.allows(statement.claim))
        {
            return Err("truthful-witness statements violate their generation spec");
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
        if self.spec.require_unique_liar && !legal.iter().all(|(_, liar)| *liar == self.liar) {
            return Err("truthful-witness statements do not prove one liar");
        }
        if self.spec.require_irredundant && !statement_set_is_irredundant(&self.statements) {
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

pub fn witness_solutions(statements: &[WitnessStatement]) -> Vec<(WitnessPath, Witness)> {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuneTransformationSpec {
    pub gate_count: u8,
    pub route_length: u8,
    pub examples_per_gate: u8,
    pub minimum_single_example_candidates: u8,
    pub allow_repeated_operations: bool,
    pub allow_repeated_route_gates: bool,
}

impl Default for RuneTransformationSpec {
    fn default() -> Self {
        Self {
            gate_count: 3,
            route_length: 3,
            examples_per_gate: 2,
            minimum_single_example_candidates: 2,
            allow_repeated_operations: true,
            allow_repeated_route_gates: false,
        }
    }
}

impl RuneTransformationSpec {
    pub fn validate(self) -> Result<Self, &'static str> {
        if !(1..=RuneGate::ALL.len() as u8).contains(&self.gate_count) {
            return Err("rune gate count must be between one and three");
        }
        if !(1..=8).contains(&self.route_length) {
            return Err("rune route length must be between one and eight");
        }
        if !self.allow_repeated_route_gates && self.route_length > self.gate_count {
            return Err("non-repeating rune route cannot exceed the gate count");
        }
        if !(1..=2).contains(&self.examples_per_gate) {
            return Err("rune examples per gate must be one or two");
        }
        if !(1..=RuneOperation::ALL.len() as u8).contains(&self.minimum_single_example_candidates) {
            return Err("rune single-example candidate minimum is out of bounds");
        }
        if self.examples_per_gate == 1 && self.minimum_single_example_candidates > 1 {
            return Err("one rune example cannot both stay ambiguous and prove its gate law");
        }
        Ok(self)
    }
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

    pub fn apply(self, input: Sigil) -> Sigil {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuneGateLaw {
    pub gate: RuneGate,
    pub operation: RuneOperation,
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
    pub spec: RuneTransformationSpec,
    pub gate_laws: Vec<RuneGateLaw>,
    pub examples: Vec<RuneExample>,
    pub route: Vec<RuneGate>,
    pub query: Sigil,
    pub solution: Sigil,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuneTransformationProjection {
    pub rules_version: u16,
    pub sigils: [Sigil; 5],
    pub candidate_rules: [RuneOperation; 5],
    pub examples: Vec<RuneExample>,
    pub route: Vec<RuneGate>,
    pub query: Sigil,
}

impl RuneTransformationPuzzle {
    pub fn generate(seed: u64) -> Self {
        Self::generate_with_spec(seed, RuneTransformationSpec::default())
            .expect("current rune-transformation rules are supported")
    }

    pub fn generate_versioned(rules_version: u16, seed: u64) -> Result<Self, &'static str> {
        if rules_version != RUNE_TRANSFORMATION_RULES_VERSION {
            return Err("unsupported rune-transformation rules version");
        }
        Self::generate_with_spec(seed, RuneTransformationSpec::default())
    }

    pub fn generate_with_spec(
        seed: u64,
        spec: RuneTransformationSpec,
    ) -> Result<Self, &'static str> {
        let spec = spec.validate()?;
        const RUNE_TRANSFORMATION_GENERATION_DOMAIN: u64 = 0x7275_6e65_5f74_7261;
        let mut rng = SplitMix64::new(seed ^ RUNE_TRANSFORMATION_GENERATION_DOMAIN);
        let active_gates = RuneGate::ALL[..usize::from(spec.gate_count)].to_vec();
        let mut available_operations = RuneOperation::ALL.to_vec();
        shuffle(&mut available_operations, &mut rng);
        let gate_laws = active_gates
            .iter()
            .copied()
            .enumerate()
            .map(|(index, gate)| RuneGateLaw {
                gate,
                operation: if spec.allow_repeated_operations {
                    RuneOperation::ALL[rng.index(RuneOperation::ALL.len())]
                } else {
                    available_operations[index]
                },
            })
            .collect::<Vec<_>>();
        let query = Sigil::ALL[rng.index(Sigil::ALL.len())];
        let mut route = Vec::with_capacity(usize::from(spec.route_length));
        if spec.allow_repeated_route_gates {
            for _ in 0..spec.route_length {
                route.push(active_gates[rng.index(active_gates.len())]);
            }
        } else {
            let mut shuffled = active_gates.clone();
            shuffle(&mut shuffled, &mut rng);
            route.extend(shuffled.into_iter().take(usize::from(spec.route_length)));
        }
        let mut examples =
            Vec::with_capacity(usize::from(spec.gate_count) * usize::from(spec.examples_per_gate));
        for law in &gate_laws {
            let gate = law.gate;
            let operation = law.operation;
            let mut inputs = Sigil::ALL;
            shuffle(&mut inputs, &mut rng);
            let chosen = rune_gate_examples(
                gate,
                operation,
                &inputs,
                spec.examples_per_gate,
                spec.minimum_single_example_candidates,
            )
            .ok_or("could not generate rune examples for this specification")?;
            examples.extend(chosen);
        }
        shuffle(&mut examples, &mut rng);
        let solution = apply_rune_route(&gate_laws, &route, query)
            .ok_or("rune route references an unknown gate")?;
        let puzzle = Self {
            rules_version: RUNE_TRANSFORMATION_RULES_VERSION,
            seed,
            spec,
            gate_laws,
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
            route: self.route.clone(),
            query: self.query,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.rules_version != RUNE_TRANSFORMATION_RULES_VERSION {
            return Err("unsupported rune-transformation rules version");
        }
        self.spec.validate()?;
        if self.gate_laws.len() != usize::from(self.spec.gate_count)
            || self.examples.len()
                != usize::from(self.spec.gate_count) * usize::from(self.spec.examples_per_gate)
            || self.route.len() != usize::from(self.spec.route_length)
        {
            return Err("rune transformation dimensions violate their generation spec");
        }
        if Some(self.solution) != apply_rune_route(&self.gate_laws, &self.route, self.query)
            || self.examples.iter().any(|example| {
                gate_operation(&self.gate_laws, example.gate)
                    .is_none_or(|operation| example.output != operation.apply(example.input))
            })
            || self
                .route
                .iter()
                .any(|gate| gate_operation(&self.gate_laws, *gate).is_none())
        {
            return Err("rune transformation contradicts private authority");
        }
        for law in &self.gate_laws {
            let gate = law.gate;
            let gate_examples = self
                .examples
                .iter()
                .copied()
                .filter(|example| example.gate == gate)
                .collect::<Vec<_>>();
            if gate_examples.len() != usize::from(self.spec.examples_per_gate)
                || rune_operations_consistent_with(&gate_examples) != vec![law.operation]
            {
                return Err("rune examples do not prove every gate law");
            }
            for example in &gate_examples {
                if rune_operations_consistent_with(&[*example]).len()
                    < usize::from(self.spec.minimum_single_example_candidates)
                {
                    return Err("rune transformation contains a redundant example");
                }
            }
        }
        Ok(())
    }
}

pub fn rune_operations_consistent_with(examples: &[RuneExample]) -> Vec<RuneOperation> {
    RuneOperation::ALL
        .into_iter()
        .filter(|operation| {
            examples
                .iter()
                .all(|example| operation.apply(example.input) == example.output)
        })
        .collect()
}

pub fn apply_rune_route(laws: &[RuneGateLaw], route: &[RuneGate], input: Sigil) -> Option<Sigil> {
    route.iter().try_fold(input, |current, gate| {
        Some(gate_operation(laws, *gate)?.apply(current))
    })
}

fn gate_operation(laws: &[RuneGateLaw], gate: RuneGate) -> Option<RuneOperation> {
    laws.iter()
        .find(|law| law.gate == gate)
        .map(|law| law.operation)
}

fn rune_gate_examples(
    gate: RuneGate,
    operation: RuneOperation,
    inputs: &[Sigil],
    count: u8,
    minimum_single_candidates: u8,
) -> Option<Vec<RuneExample>> {
    let example = |input| RuneExample {
        gate,
        input,
        output: operation.apply(input),
    };
    if count == 1 {
        return inputs.iter().copied().map(example).find_map(|example| {
            (rune_operations_consistent_with(&[example]) == vec![operation])
                .then_some(vec![example])
        });
    }
    inputs.iter().enumerate().find_map(|(first, left)| {
        inputs.iter().skip(first + 1).find_map(|right| {
            let pair = vec![example(*left), example(*right)];
            (pair.iter().all(|item| {
                rune_operations_consistent_with(&[*item]).len()
                    >= usize::from(minimum_single_candidates)
            }) && rune_operations_consistent_with(&pair) == vec![operation])
            .then_some(pair)
        })
    })
}

pub(crate) fn shuffle<T>(values: &mut [T], rng: &mut SplitMix64) {
    for end in (1..values.len()).rev() {
        let selected = rng.index(end + 1);
        values.swap(end, selected);
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
            assert!(!projection.contains("gate_laws"));
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
                PuzzleAuthority::LogicGrid(puzzle) => PuzzleSubmission::LogicGrid {
                    assignments: crate::grid_solutions(puzzle.spec.size, &puzzle.clues, 1)
                        .into_iter()
                        .next()
                        .unwrap(),
                },
                PuzzleAuthority::ResourceAllocation(puzzle) => {
                    PuzzleSubmission::ResourceAllocation {
                        provisions: crate::allocation_optimal_packs(
                            &puzzle.provisions,
                            &puzzle.hazards,
                            puzzle.spec.capacity,
                        )
                        .into_iter()
                        .next()
                        .unwrap(),
                    }
                }
            };
            assert_eq!(puzzle.check(&answer), Ok(true));
        }
    }

    #[test]
    fn generation_specs_are_serializable_validated_and_replayed_exactly() {
        let requests = [
            GenerationRequest {
                seed: 70,
                spec: PuzzleSpec::OrderedSigils(OrderedSigilSpec {
                    allow_exact: false,
                    allow_before: true,
                    allow_adjacent: true,
                    allow_not_at: false,
                    max_clues: 5,
                }),
            },
            GenerationRequest {
                seed: 71,
                spec: PuzzleSpec::TruthfulWitnesses(TruthfulWitnessSpec {
                    require_unique_liar: true,
                    ..TruthfulWitnessSpec::default()
                }),
            },
            GenerationRequest {
                seed: 72,
                spec: PuzzleSpec::RuneTransformation(RuneTransformationSpec {
                    gate_count: 2,
                    route_length: 4,
                    examples_per_gate: 2,
                    minimum_single_example_candidates: 2,
                    allow_repeated_operations: false,
                    allow_repeated_route_gates: true,
                }),
            },
        ];
        for request in requests {
            let json = serde_json::to_string(&request).unwrap();
            let decoded: GenerationRequest = serde_json::from_str(&json).unwrap();
            let puzzle = PuzzleAuthority::generate_request(decoded.clone()).unwrap();
            assert_eq!(puzzle.generation_request(), decoded);
            assert_eq!(puzzle.replay().unwrap(), puzzle);
            assert_eq!(puzzle.analysis().possible_answers, 1);
        }
    }

    #[test]
    fn rune_complexity_parameters_change_the_measured_structure() {
        let easy = PuzzleAuthority::generate_request(GenerationRequest {
            seed: 99,
            spec: PuzzleSpec::RuneTransformation(RuneTransformationSpec {
                gate_count: 1,
                route_length: 1,
                examples_per_gate: 1,
                minimum_single_example_candidates: 1,
                allow_repeated_operations: true,
                allow_repeated_route_gates: false,
            }),
        })
        .unwrap();
        let hard = PuzzleAuthority::generate(PuzzleKind::RuneTransformation, 99);
        let easy_analysis = easy.analysis();
        let hard_analysis = hard.analysis();
        assert!(hard_analysis.fact_count > easy_analysis.fact_count);
        assert!(hard_analysis.application_depth > easy_analysis.application_depth);
        assert!(hard_analysis.initial_hypotheses > easy_analysis.initial_hypotheses);
        assert!(hard_analysis.structural_complexity > easy_analysis.structural_complexity);
    }

    #[test]
    fn invalid_complexity_combinations_fail_before_generation() {
        let result = PuzzleAuthority::generate_request(GenerationRequest {
            seed: 1,
            spec: PuzzleSpec::RuneTransformation(RuneTransformationSpec {
                gate_count: 1,
                route_length: 2,
                examples_per_gate: 2,
                minimum_single_example_candidates: 2,
                allow_repeated_operations: true,
                allow_repeated_route_gates: false,
            }),
        });
        assert_eq!(
            result,
            Err("non-repeating rune route cannot exceed the gate count")
        );
    }
}
