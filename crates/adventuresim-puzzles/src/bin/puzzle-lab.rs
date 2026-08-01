use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

use adventuresim_puzzles::{
    GenerationRequest, OrderedSigilSpec, PuzzleAuthority, PuzzleKind, PuzzleProjection, PuzzleSpec,
    PuzzleSubmission, RuneTransformationSpec, Sigil, TruthfulWitnessSpec, WitnessPath,
};
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "puzzle-lab",
    about = "Generate, play, and measure Adventure Simulator puzzles"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Show {
        #[command(flatten)]
        generation: GenerationArgs,
        #[arg(long)]
        reveal: bool,
    },
    Play {
        #[command(flatten)]
        generation: GenerationArgs,
    },
    Analyze {
        #[command(flatten)]
        generation: GenerationArgs,
        #[arg(long)]
        json: bool,
    },
    Sweep {
        #[command(flatten)]
        generation: GenerationArgs,
        #[arg(long, default_value_t = 1_000)]
        count: u64,
        #[arg(long)]
        jsonl: bool,
    },
    Find {
        #[command(flatten)]
        generation: GenerationArgs,
        #[arg(long)]
        minimum_complexity: f32,
        #[arg(long)]
        maximum_complexity: f32,
        #[arg(long, default_value_t = 10_000)]
        search: u64,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    Validate {
        #[command(flatten)]
        generation: GenerationArgs,
        #[arg(long, default_value_t = 10_000)]
        count: u64,
    },
    Replay {
        path: PathBuf,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum KindArg {
    OrderedSigils,
    TruthfulWitnesses,
    RuneTransformation,
}

impl From<KindArg> for PuzzleKind {
    fn from(value: KindArg) -> Self {
        match value {
            KindArg::OrderedSigils => Self::OrderedSigils,
            KindArg::TruthfulWitnesses => Self::TruthfulWitnesses,
            KindArg::RuneTransformation => Self::RuneTransformation,
        }
    }
}

#[derive(Args)]
struct GenerationArgs {
    #[arg(value_enum)]
    kind: Option<KindArg>,
    #[arg(long)]
    seed: Option<u64>,
    /// Read a GenerationRequest from JSON. Its spec is used; --seed can override its seed.
    #[arg(long)]
    request: Option<PathBuf>,

    #[arg(long, default_value_t = 4)]
    max_clues: u8,
    #[arg(long)]
    no_exact: bool,
    #[arg(long)]
    no_before: bool,
    #[arg(long)]
    no_adjacent: bool,
    #[arg(long)]
    no_not_at: bool,

    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    path_claims: bool,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    witness_claims: bool,
    #[arg(long)]
    unique_liar: bool,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    irredundant: bool,

    #[arg(long, default_value_t = 3)]
    gates: u8,
    #[arg(long, default_value_t = 3)]
    route_length: u8,
    #[arg(long, default_value_t = 2)]
    examples_per_gate: u8,
    #[arg(long, default_value_t = 2)]
    minimum_example_candidates: u8,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    repeated_operations: bool,
    #[arg(long)]
    repeated_route_gates: bool,
}

impl GenerationArgs {
    fn request(&self, seed: Option<u64>) -> Result<GenerationRequest, String> {
        if let Some(path) = &self.request {
            let mut request: GenerationRequest =
                serde_json::from_str(&fs::read_to_string(path).map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?;
            if let Some(seed) = seed {
                request.seed = seed;
            }
            return Ok(request);
        }
        let kind = self
            .kind
            .ok_or_else(|| "provide a puzzle kind or --request PATH".to_owned())?;
        let spec = match PuzzleKind::from(kind) {
            PuzzleKind::OrderedSigils => PuzzleSpec::OrderedSigils(OrderedSigilSpec {
                allow_exact: !self.no_exact,
                allow_before: !self.no_before,
                allow_adjacent: !self.no_adjacent,
                allow_not_at: !self.no_not_at,
                max_clues: self.max_clues,
            }),
            PuzzleKind::TruthfulWitnesses => PuzzleSpec::TruthfulWitnesses(TruthfulWitnessSpec {
                allow_path_claims: self.path_claims,
                allow_witness_claims: self.witness_claims,
                require_unique_liar: self.unique_liar,
                require_irredundant: self.irredundant,
            }),
            PuzzleKind::RuneTransformation => {
                PuzzleSpec::RuneTransformation(RuneTransformationSpec {
                    gate_count: self.gates,
                    route_length: self.route_length,
                    examples_per_gate: self.examples_per_gate,
                    minimum_single_example_candidates: self.minimum_example_candidates,
                    allow_repeated_operations: self.repeated_operations,
                    allow_repeated_route_gates: self.repeated_route_gates,
                })
            }
        };
        Ok(GenerationRequest {
            seed: seed.unwrap_or(0),
            spec,
        })
    }

    fn starting_seed(&self) -> Result<u64, String> {
        Ok(self.request(self.seed)?.seed)
    }

    fn generate(&self, seed: Option<u64>) -> Result<PuzzleAuthority, String> {
        PuzzleAuthority::generate_request(self.request(seed)?).map_err(str::to_owned)
    }
}

fn main() -> Result<(), String> {
    let cli = Cli::parse();
    match cli.command {
        Command::Show { generation, reveal } => {
            let puzzle = generation.generate(generation.seed)?;
            print!("{}", render(&puzzle.projection()));
            if reveal {
                println!("\nPrivate authority:\n{}", pretty(&puzzle)?);
            }
        }
        Command::Play { generation } => play(generation.generate(generation.seed)?)?,
        Command::Analyze { generation, json } => {
            let puzzle = generation.generate(generation.seed)?;
            let analysis = puzzle.analysis();
            if json {
                println!("{}", pretty(&analysis)?);
            } else {
                print!("{}", render(&puzzle.projection()));
                println!("\nAnalysis:\n{}", pretty(&analysis)?);
            }
        }
        Command::Sweep {
            generation,
            count,
            jsonl,
        } => {
            let mut scores = Vec::new();
            let mut full_model_necessary = 0_u64;
            let mut answer_necessary = 0_u64;
            let starting_seed = generation.starting_seed()?;
            for offset in 0..count {
                let seed = starting_seed.wrapping_add(offset);
                let puzzle = generation.generate(Some(seed)).map_err(|error| {
                    format!(
                        "seed {seed}: {error}\nReplay with the same arguments and --seed {seed}"
                    )
                })?;
                let analysis = puzzle.analysis();
                if jsonl {
                    println!(
                        "{{\"seed\":{seed},\"analysis\":{}}}",
                        serde_json::to_string(&analysis).unwrap()
                    );
                }
                full_model_necessary += u64::from(analysis.all_facts_necessary_for_full_model);
                answer_necessary += u64::from(analysis.all_facts_necessary_for_answer);
                scores.push(analysis.structural_complexity);
            }
            if !jsonl {
                scores.sort_by(f32::total_cmp);
                let mean = scores.iter().sum::<f32>() / scores.len().max(1) as f32;
                println!(
                    "generated={} min={:.2} mean={mean:.2} median={:.2} max={:.2}",
                    scores.len(),
                    scores.first().copied().unwrap_or(0.0),
                    scores.get(scores.len() / 2).copied().unwrap_or(0.0),
                    scores.last().copied().unwrap_or(0.0)
                );
                let denominator = scores.len().max(1) as f32;
                println!(
                    "all-facts-necessary: full-model={:.1}% final-answer={:.1}%",
                    full_model_necessary as f32 * 100.0 / denominator,
                    answer_necessary as f32 * 100.0 / denominator,
                );
            }
        }
        Command::Find {
            generation,
            minimum_complexity,
            maximum_complexity,
            search,
            limit,
        } => {
            let mut found = 0;
            let starting_seed = generation.starting_seed()?;
            for offset in 0..search {
                let seed = starting_seed.wrapping_add(offset);
                let puzzle = match generation.generate(Some(seed)) {
                    Ok(puzzle) => puzzle,
                    Err(_) => continue,
                };
                let score = puzzle.analysis().structural_complexity;
                if (minimum_complexity..=maximum_complexity).contains(&score) {
                    println!("seed={seed} complexity={score:.2}");
                    found += 1;
                    if found == limit {
                        break;
                    }
                }
            }
            if found == 0 {
                return Err("no matching seeds found".into());
            }
        }
        Command::Validate { generation, count } => {
            let starting_seed = generation.starting_seed()?;
            for offset in 0..count {
                let seed = starting_seed.wrapping_add(offset);
                let puzzle = generation
                    .generate(Some(seed))
                    .map_err(|error| format!("seed {seed}: {error}"))?;
                puzzle
                    .validate()
                    .map_err(|error| format!("seed {seed}: {error}"))?;
                if puzzle.replay().map_err(str::to_owned)? != puzzle {
                    return Err(format!("seed {seed}: deterministic replay differs"));
                }
            }
            println!("validated {count} seeds");
        }
        Command::Replay { path } => {
            let puzzle: PuzzleAuthority =
                serde_json::from_str(&fs::read_to_string(path).map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?;
            puzzle.validate().map_err(str::to_owned)?;
            if puzzle.replay().map_err(str::to_owned)? != puzzle {
                return Err("deterministic replay differs".into());
            }
            print!("{}", render(&puzzle.projection()));
            println!("\nAnalysis:\n{}", pretty(&puzzle.analysis())?);
        }
    }
    Ok(())
}

fn pretty<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|error| error.to_string())
}

fn render(projection: &PuzzleProjection) -> String {
    match projection {
        PuzzleProjection::OrderedSigils(puzzle) => format!(
            "Arrange: {}\n{}\n",
            puzzle
                .sigils
                .iter()
                .map(|sigil| sigil.label())
                .collect::<Vec<_>>()
                .join(", "),
            puzzle
                .clues
                .iter()
                .map(|clue| format!("- {}", clue.text()))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        PuzzleProjection::TruthfulWitnesses(puzzle) => format!(
            "Exactly one witness lies; exactly one path is safe.\n{}\n",
            puzzle
                .statements
                .iter()
                .map(|statement| format!("- {}", statement.text()))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        PuzzleProjection::RuneTransformation(puzzle) => format!(
            "Candidate rules:\n{}\nExamples:\n{}\nQuestion: {} through {}\n",
            puzzle
                .candidate_rules
                .iter()
                .map(|rule| format!("- {}", rule.rule_text()))
                .collect::<Vec<_>>()
                .join("\n"),
            puzzle
                .examples
                .iter()
                .map(|example| format!("- {}", example.text()))
                .collect::<Vec<_>>()
                .join("\n"),
            puzzle.query.label(),
            puzzle
                .route
                .iter()
                .map(|gate| gate.label())
                .collect::<Vec<_>>()
                .join(" -> "),
        ),
    }
}

fn play(puzzle: PuzzleAuthority) -> Result<(), String> {
    print!("{}\nAnswer: ", render(&puzzle.projection()));
    io::stdout().flush().map_err(|error| error.to_string())?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|error| error.to_string())?;
    let answer = answer.trim();
    let submission = match &puzzle {
        PuzzleAuthority::OrderedSigils(_) => {
            let values = answer
                .split(',')
                .map(str::trim)
                .map(parse_sigil)
                .collect::<Result<Vec<_>, _>>()?;
            let ordering: [Sigil; 5] = values
                .try_into()
                .map_err(|_| "enter five comma-separated sigils")?;
            PuzzleSubmission::OrderedSigils { ordering }
        }
        PuzzleAuthority::TruthfulWitnesses(_) => PuzzleSubmission::TruthfulWitnesses {
            safe_path: parse_path(answer)?,
        },
        PuzzleAuthority::RuneTransformation(_) => PuzzleSubmission::RuneTransformation {
            result: parse_sigil(answer)?,
        },
    };
    println!(
        "{}",
        if puzzle
            .check(&submission)
            .map_err(|error| format!("{error:?}"))?
        {
            "Correct."
        } else {
            "Incorrect."
        }
    );
    Ok(())
}

fn parse_sigil(value: &str) -> Result<Sigil, String> {
    Sigil::ALL
        .into_iter()
        .find(|sigil| sigil.label().eq_ignore_ascii_case(value))
        .ok_or_else(|| format!("unknown sigil: {value}"))
}

fn parse_path(value: &str) -> Result<WitnessPath, String> {
    WitnessPath::ALL
        .into_iter()
        .find(|path| {
            path.label().eq_ignore_ascii_case(value)
                || path
                    .label()
                    .trim_end_matches(" path")
                    .eq_ignore_ascii_case(value)
        })
        .ok_or_else(|| format!("unknown path: {value}"))
}
