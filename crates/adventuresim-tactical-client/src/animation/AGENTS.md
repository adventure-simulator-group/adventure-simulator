# Animation development guide

## Validation authority

For defects reported in live tactical play, the native client/server diagnostic
is the acceptance authority. Synthetic ECS tests and animation-viewer captures
are focused regression tools, not substitutes for the production-path run.

Use the existing machine-readable diagnostic scenario and production logging.
Do not create another simplified fixture when the real-client script can express
the input sequence.

## Evidence order

Inspect artifacts in this order:

1. `guard-footwork-iteration-summary.json`
2. `guard-footwork-causal-slice.json`
3. The relevant source functions
4. The full animation JSONL only if the causal slice is demonstrably insufficient

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

## Agent and review policy

Use one implementation agent during ordinary iteration.

Do not keep a reviewer agent running continuously. Request independent review
only when:

- the native acceptance test passes;
- a proposed change alters ownership architecture or acceptance semantics; or
- the implementation agent has reached a concrete ambiguity that deterministic
  evidence cannot resolve.

Do not request a new review after every small remediation.

If a subagent is explicitly authorized, provide only the compact iteration
summary, causal slice, relevant source locations, and precise question. Do not
fork the full conversation history unless it is essential.

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

## Iteration report

After each native run, report only:

- pass or fail;
- wall-clock duration;
- first failed contract and frame;
- failed threshold values;
- paths to the compact summary and causal slice;
- files changed;
- agents used;
- whether another run was performed.

Do not narrate the full investigation history on every iteration.
