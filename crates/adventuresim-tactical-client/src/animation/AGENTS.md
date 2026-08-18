# Animation development guide

## Validation authority

For defects reported in live tactical play, the native client/server diagnostic
is the acceptance authority. Synthetic ECS tests and animation-viewer captures
are focused regression tools, not substitutes for the production-path run.

Run the existing production-path scenario with `just tactical-play diagnostic`.
Its supervised run directory, reported by `just tactical-status`, contains the
machine-readable animation JSONL and process logs. Do not create another
simplified fixture when the real-client script can express the input sequence.

## Evidence order

Inspect a compact analyzer summary or bounded projection first, then the
relevant source functions. Read the full animation JSONL only when that evidence
is demonstrably insufficient.

Do not paste or emit full JSONL records, cumulative incident lists, or large
frame arrays into model context. Prefer contiguous ranges, counts, first
occurrence, and bounded projections.

A normal causal slice should contain no more than:

- the first failed contract;
- the owner transition that began its episode;
- approximately 8-20 projected frames;
- only the relevant controller, cadence, pelvis, support, target, reach, and
  rendered-ankle fields.

If more evidence is required, extract another bounded projection
programmatically instead of reading the full log.

## Iteration loop

For each iteration:

1. Run or inspect the authoritative native scenario.
2. Select the earliest failed acceptance contract.
3. Produce one causal hypothesis from its bounded slice.
4. Make the smallest correction that restores the violated ownership or
   continuity invariant.
5. Run the narrowest directly affected unit test.
6. Run one native acceptance test.
7. Stop and report the result.

Do not fix later incidents in the same iteration unless they have the same
demonstrated cause.

Do not retry a failed native run without either a code change or evidence that
the harness itself was incomplete.

## Test ladder

During iteration:

1. Analyzer unit test
2. One filtered Rust regression in the relevant binary or binaries
3. One native diagnostic run

Run the complete procedural-animation suite, both complete binaries, wiki
checks, and other broad validation only after native acceptance passes or
immediately before commit.

Compilation warnings already known to be unrelated should be summarized rather
than repeatedly analyzed.

## Acceptance integrity

Never make a run pass by:

- raising visual or biomechanical thresholds;
- shortening the scenario;
- removing difficult directions, stops, or cadence edges;
- classifying procedural output as authored;
- accepting smooth but visibly stale world-space targets;
- weakening missing-owner, hard-reach, support, or contact checks.

Analyzer or telemetry changes must preserve or strengthen the existing
acceptance contract.

## Locomotion invariants

- Give each leg and pelvis exactly one presentation owner per semantic tick.
  Handoffs must transfer the visible position and retained derivatives
  atomically; a wait flag or diagnostic label is not an owner.
- Derive reach, stride, clearance, and dynamics from the measured rig and
  cadence rather than fixed humanoid distances. Validate representative limb
  scales, speeds, and render rates when changing locomotion.
- Treat reachable geometry as a solve-boundary invariant. A downstream IK
  projection must never change the visible ankle while retaining derivatives
  from an unreachable target.
- Prefer a simple body-relative gait with bounded terrain contacts over
  overlapping world-space plant, release, recovery, and fallback owners.

## Iteration report

After each native run, report only:

- pass or fail;
- wall-clock duration;
- first failed contract and frame;
- failed threshold values;
- paths to the compact summary or bounded projection;
- files changed;
- agents used;
- whether another run was performed.

Do not narrate the full investigation history on every iteration.
