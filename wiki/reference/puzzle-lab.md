# Puzzle laboratory

`adventuresim-puzzles` owns the presenter-independent puzzle engines used by
production errantry quests. It depends only on Serde and the CLI parser; it
does not depend on `adventuresim-core`, SpacetimeDB, or the web application.
`adventuresim-core::errantry` re-exports its public API for compatibility while
keeping fey speech, quest state, and tactical rewards outside the puzzle crate.

Run the focused test suite with:

```powershell
just puzzle-check
```

The `puzzle-lab` binary calls the same generators, validators, solvers,
projections, and submission checker as the game. Common workflows include:

```powershell
cargo run -q -p adventuresim-puzzles --bin puzzle-lab -- show rune-transformation --seed 42
cargo run -q -p adventuresim-puzzles --bin puzzle-lab -- play truthful-witnesses --seed 7
cargo run -q -p adventuresim-puzzles --bin puzzle-lab -- analyze rune-transformation --seed 42
cargo run -q -p adventuresim-puzzles --bin puzzle-lab -- sweep rune-transformation --count 100000
cargo run -q -p adventuresim-puzzles --bin puzzle-lab -- find rune-transformation --minimum-complexity 20 --maximum-complexity 24
cargo run -q -p adventuresim-puzzles --bin puzzle-lab -- validate ordered-sigils --count 10000
cargo run -q -p adventuresim-puzzles --bin puzzle-lab -- analyze logic-grid --grid-size 4 --seed 42
cargo run -q -p adventuresim-puzzles --bin puzzle-lab -- sweep resource-allocation --provisions 7 --hazards 4 --capacity 9 --count 10000
```

`show --reveal` includes private authority for debugging. `replay PATH` accepts
a serialized `PuzzleAuthority`, validates it, regenerates it from its retained
seed and specification, and rejects any deterministic mismatch. Commands also
accept `--request PATH` containing a serialized `GenerationRequest`; this is
the preferred way to retain an interesting parameter combination. One-off
commands retain the request's recorded seed unless `--seed` is supplied;
population commands use that seed as their starting point and vary it across
the requested range.

## Generation parameters

Ordering puzzles expose the enabled clue families (exact, before, adjacent,
and not-at) and the maximum clue budget. The generator still searches for a
globally minimum proof within the selected grammar and bound.

Witness puzzles expose path claims, witness-identity claims, whether the liar
must be uniquely proved, and whether every statement must be necessary to
prove the safe path.

Rune puzzles expose active gate count, route length, examples per gate, the
minimum number of laws consistent with each individual example, operation
reuse, and route-gate reuse. Invalid combinations fail before generation. For
example, one example cannot both remain ambiguous and uniquely identify its
gate law.

Logic-grid puzzles match each of three or four travelers to one token and one
road. They expose grid size, positive and negative clue polarities, and a clue
ceiling. Generation enumerates every pair of permutations, greedily selects
the clue with the greatest information gain, proves uniqueness, and then
removes every redundant clue. This is a documented bounded minimizer rather
than a claim of global minimum cardinality for the four-by-four case.

Resource-allocation puzzles expose the number of available provisions, number
of hazards, carrying capacity, and whether every hazard and the capacity must
change the optimum. They enumerate every provision subset. A legal pack must
fit the capacity and answer every hazard; the explicit objective maximizes
readiness, then minimizes weight, then minimizes item count. Generation rejects
instances without exactly one best pack.

## Analysis and calibration

Every generated puzzle can produce `PuzzleAnalysis`, including its initial and
remaining hypothesis counts, fact count, hypothesis trajectory, independent
rules inferred, application depth, working-memory width, possible final
answers, cumulative hypothesis load in presentation order, and two distinct
necessity checks. Population sweeps also summarize both necessity rates:

- whether every fact is needed to recover the complete hidden model;
- whether every fact is needed merely to determine the submitted answer.

The reported `structural_complexity` is an explicitly experimental comparison
score derived from those components. It is useful for population sweeps and
seed searches, but it is not yet a calibrated measure of human difficulty and
must not be mapped directly to character Intelligence. Difficulty presets and
an Intelligence mapping should be authored only after representative puzzles
and metric distributions have been reviewed.

Specifications are serialized into private puzzle authority. Consequently a
puzzle replays from its exact seed and parameters even after the CLI begins
testing nonstandard configurations. Observer-safe projections continue to
omit the specification, seed, operation assignments, and solution.
