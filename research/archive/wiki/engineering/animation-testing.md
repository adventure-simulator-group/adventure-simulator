# Animation testing

> **Status:** Deterministic viewer regression workflow
>
> **Scope:** Tactical character animation and procedural foot placement

The animation tests answer two different questions. First, did the animation
system violate a concrete contract, such as putting a supporting foot five
inches above the ground? Second, if several defects remain, which one should we
fix first? Binary acceptance gates answer the first question. A weighted quality
score answers the second. The score is a triage aid, not permission to ship a
failed gate.

<!-- toc -->

## Test layers

Animation testing has four layers. They become progressively more realistic and
expensive.

| Layer | What it runs | Best use |
| --- | --- | --- |
| Focused Rust tests | Individual planners, followers, owner transitions, and analyzers | Proving a local invariant while editing |
| Animation viewer | The real animation assets and procedural schedule at a deterministic 64 Hz | Reproducible route, pose, continuity, and rendering checks |
| Scripted native diagnostic | The real database, tactical server, transport, native client, input-send path, replication, terrain, assets, and variable rendering | Authoritative acceptance for gameplay defects |
| Manual animation mode | The same native game with unrestricted human input and bounded diagnostic logs | Finding behavior that the scripted scenarios don't yet cover |

The viewer is useful, but it isn't the authority when it disagrees with the
game. Its inputs, terrain, camera, and timing are controlled. The scripted
native diagnostic uses the production client/server path and is therefore the
acceptance test for a defect reported in live play.

The native scenarios are machine-readable command lists. A command such as
“raise guard, walk forward, stop, then dodge right” becomes the same
`PlayerInputRequest` that ordinary play sends during `FixedUpdate`. The server
processes it, the client receives replicated state, presentation predicts
between snapshots, and the normal animation and inverse-kinematics systems run
against the real rig. Logging happens after transform propagation, so the
record contains the final bone transforms that were actually presented.

The main commands are:

```powershell
just animation-test
just animation-preview <scenario> <output-directory>
just tactical-play animation
```

`just animation-test` first runs the viewer's focused Rust regression tests and
then captures the complete deterministic scenario matrix. Its `manifest.json`
is the machine-readable result; `quality_score.quality_percent` contains the
weighted score, `quality_score.categories` explains the deductions, and
`quality_score.acceptance_passed` remains the separate hard-gate result. The
rendered frames and `index.html` are for visual assessment. Pass `output=...`
to keep separate runs for comparison.

The native `tactical-play animation` mode remains the production-path check for
networking, replication, terrain, and rendering behavior. The viewer is the
fast deterministic check; it is not a replacement for that native run when a
defect depends on live gameplay.

Animation mode seeds five named combat targets: a passive bandit, an
omnidirectional shield blocker that doesn't turn toward the player, an
aiming directional dodger, a munition-plate dodger in half armor, and a passive
demi-lancer in three-quarter armor.
Their names, loadouts, and behavior roles come from
`assets/tactical-enemies/animation-demo.yaml`; changing that roster does not
require recompiling Rust. Enemy fixtures and `--scene-input` are independent,
so any enemy YAML can run in any environment. For example:

```sh
python scripts/dev_stack.py tactical-play animation \
  --scene-input flat-dry-grassland \
  --enemy-fixture animation-demo
```

The targets use ordinary combat statistics and equipment rules. Their behavior
is assembled from independent offense, guard, facing, reactive-defense, and
get-up packages, so a test target can defend without inheriting unrelated
movement or attack behavior. Directional dodges require a raised guard and a
nonzero travel direction, and recovered prone or supine targets use the same
authored get-up transition as players before returning upright.

The native logger records input, authoritative and presented locomotion,
semantic animation clocks, animation-layer weights, procedural owner state,
foot targets and derivatives, reach state, support state, terrain samples,
pelvis state, and final global bone transforms. Animation mode keeps two
rotating 32 MiB segments rather than writing an unbounded log.

## The iteration loop

The ordinary loop is intentionally narrow:

1. Reproduce the problem with the smallest applicable focused test or named
   native scenario.
2. Read the generated iteration summary and causal slice.
3. Identify the earliest failed contract.
4. Change only what is needed to correct that contract.
5. Run focused tests while editing.
6. Run the same authoritative scenario once.
7. Stop and assess its result before beginning another cycle.

The guard-footwork analyzer writes a compact
`guard-footwork-iteration-summary.json` and a bounded
`guard-footwork-causal-slice.json`. The causal slice contains only the frames
around the start and threshold crossing of the earliest failure. The full
animation JSONL is opened only when those files don't contain enough evidence.

This conserves tokens in several ways:

*   One agent owns the reproduce/fix/test loop. Parallel investigators and
    reviewers are reserved for an explicitly requested review or a candidate
    that has passed its authoritative test.
*   Deterministic analyzers reduce thousands of per-frame records to one first
    failure, contiguous evidence ranges, and a short causal slice.
*   The earliest failed contract takes precedence. Later symptoms aren't
    analyzed while an earlier defect is sufficient to explain them.
*   Focused tests run before the native stack, avoiding expensive launches for
    compile errors or local invariant failures.
*   Each iteration reruns the same scenario once. It doesn't fan out across
    every viewer route or repeatedly reread unchanged logs.
*   Manual play is exploratory. Once it reveals a gap, that input sequence and
    observable failure become a scripted production-path regression.

These rules reduce model context and execution cost without reducing test
realism. In particular, they favor a compact analysis of the real game over a
large analysis of a simplified fixture.

## Acceptance and scoring

The guard-footwork analyzer has two structural locomotion gates followed by five
power-of-two priorities:

| Priority | Score effect | Why it has this priority |
| --- | ---: | --- |
| Catastrophic horizontal foot displacement | Force quality to 0 | A foot held over 0.65 m horizontally from its hip for 0.1 s is a grossly broken pose. Smoothness cannot compensate for it. |
| Missing visible guard step | Force quality to 0 | Advancing gait phase and contact metadata do not constitute locomotion unless the final swing-foot transform travels, clears the floor, exchanges support, and replants. |
| Anatomically invalid joints | Weight 16 | A reversed knee or crossed leg is physically invalid and can look catastrophic even for one frame. Its weight exceeds all lower weighted priorities combined. |
| Contact foot airborne | Weight 8 | A foot claiming contact while visibly floating, penetrating terrain, or exceeding hard reach breaks the physical premise of foot placement. |
| Both feet behind the hips | Weight 4 | A sustained rearward stance makes locomotion appear detached from the body even when each foot remains reachable. |
| Foot dragging | Weight 2 | Sustained sliding or scuffing destroys the impression of planted weight, but is less severe than an invalid or floating leg. |
| Jitter and jerk | Weight 1 | Discontinuities are distracting, though a correctly placed but slightly jerky foot is preferable to a smoothly misplaced or floating one. |

Each weight is greater than the sum of all lower weights. The ordering therefore
survives aggregation: fixing one anatomical failure is always more valuable
than fixing every lower-priority category while leaving it in place.

Without a structural locomotion failure, the weighted defect score is the sum
of the failed weighted categories. A catastrophic displacement or missing
visible guard step assigns the full defect score of 31 by itself. The displayed
quality score is:

```text
quality = 100 * (1 - weighted_defect_score / 31)
```

A category contributes its weight once, regardless of how many frames failed.
Episode durations and evidence ranges remain in the manifest for diagnosis.
The animation passes only when the harness completed and every applicable hard
contract passed; a score of 97 is not a substitute for acceptance.

Jitter diagnostics are computed from the final presented pose at the capture
sample rate. Bone translations and rotations are converted to parent-relative
joint space, then finite differences produce velocity, acceleration, and jerk.
Each incident records its joint, derivative class, frame window, measured
value, threshold, and severity. Absolute limits, relative limits, and a noise
floor are all applied. This current-main port validates the final pose; it does
not yet retain a parallel authored-pose history for authored-versus-procedural
attribution, so that remains a follow-up gap rather than an implied pass.

## Guard-footwork metrics

The five priorities are computed from the following measurements. “Quarter
stride” means half the median measured half-step, clamped to 0.08–0.30 seconds.

| Metric | Failure condition | Priority or role | Justification |
| --- | --- | --- | --- |
| Horizontal hip-foot offset | Either foot remains more than 0.65 m horizontally from its hip for over 0.1 s | Catastrophic | Detects the straight, side-pinned leg failure independent of rig-axis convention, even when the foot is perfectly still and therefore has excellent derivative metrics. |
| Visible guard half-step | Between consecutive contact edges, the final global swing-foot transform moves less than 0.05 m horizontally, gains less than 0.03 m of terrain-relative clearance, fails to exchange support, or fails to replant | Structural | Prevents a semantic gait clock from passing while both rendered feet remain in a rigid pose and slide with the controller. Every completed half-step must satisfy all four observables. |
| Knee flexion | More than 165 degrees | Anatomical | Rejects a nearly inverted two-bone solution while leaving a small numerical margin below perfectly straight. |
| Knee bend hemisphere | Tracked pole hemisphere dot product below zero | Anatomical | Detects the knee choosing the opposite bend solution even if its flexion angle alone looks plausible. |
| Foot crossover | Either foot crosses the pelvis centerline relative to its own hip | Anatomical | Detects crossed legs and side-identity failures. |
| Contact sole clearance | A foot with support weight above 0.5 is more than 0.127 m (five inches) above terrain | Contact airborne | Encodes the user-visible severe-airborne threshold, using the sole rather than the ankle joint. |
| Contact penetration | A supporting sole is more than 0.01 m below terrain | Contact airborne | Prevents grounding from being “fixed” by burying the foot. |
| Hard reach | The selected leg reaches the hard two-bone limit | Contact airborne | A hard clamp means the requested pose was not anatomically feasible and can introduce a visible snap. |
| Pelvis vertical step | More than 0.05 m in one semantic tick | Contact airborne | Catches the sinking-then-teleporting failure even when foot clearance happens to remain acceptable. |
| Both feet behind | Both feet remain more than 0.02 m behind the hips for over 0.3 s while moving | Both behind | Allows ordinary short stride overlap but rejects a human-perceptible trailing stance. |
| Stance width | Foot separation remains outside 0.5–4.0 times hip width for over 0.3 s | Both behind | Makes the metric scale with the rig instead of assuming one character size. |
| Planted drag | A supporting foot moves faster than 0.12 m/s for at least a quarter stride | Drag | Allows brief settling but rejects a foot visibly skating while it claims support. |
| Swing scuff | A non-supporting sole stays below 0.01 m while its ankle moves faster than 0.12 m/s for at least a quarter stride | Drag | Detects a swing foot scraping along the terrain. |
| Contact orientation | Sole normal differs from the terrain normal by more than 5 degrees for at least a quarter stride | Drag | Detects prolonged flat or twisted contacts on slopes. Terrain tilt used for the desired sole is capped at 28 degrees. |
| Acceleration ratio | Presented acceleration exceeds the selected owner's declared limit by more than 0.1% | Jitter | Verifies that a supposedly bounded motion owner actually honors its contract. |
| Jerk ratio | Presented jerk exceeds the selected owner's declared limit by more than 0.1% | Jitter | Detects derivative discontinuities even when positions look locally reasonable. |
| Cadence ratio | Longest moving half-step divided by shortest exceeds 1.5 | Jitter | Detects stalled or erratic step timing without prescribing a fixed gait speed. |

Several additional guard metrics are hard lifecycle gates rather than weighted
score categories:

| Metric | Failure condition | Why it is included |
| --- | --- | --- |
| Radial extension | An ankle is more than 0.90 m from the visual pelvis | Rejects conspicuously trailing legs even before an analytic hard-reach clamp. |
| Visible stuck episode | For at least 0.2 s, support is zero, cadence is awaiting an edge, a foot is elevated, and a foot is trailing or overextended | Captures the coherent “foot stuck in the air” syndrome rather than treating an isolated flag as proof. |
| Cadence wait | `awaiting_step_sequence` persists for at least 0.5 s | Finds owner lifecycles that have stopped making progress. |
| Stale raised release | Raised ownership survives at least 0.5 s after guard is lowered | Detects a dead handoff to ordinary locomotion. |
| Ground-safety slide | The fallback owner persists for at least 0.5 s | Prevents a safety mechanism from quietly replacing the gait with prolonged skating. |

Body-relative rearward extension, warning-reach frames, owner epochs, support
weights, and contiguous evidence ranges are also reported. They explain a
failure but don't independently change the weighted score unless they satisfy
one of the contracts above.

Before any result is accepted, the analyzer also proves that the harness ran:
it requires a schema header, at least 60 frames, guard raise and lower events, a
diagonal command, multiple authoritative and presentation source ticks, moving
raised locomotion, and at least 0.5 m of controller displacement. A missing or
truncated log is therefore an incomplete test, not a pass.

## Melee metrics

The melee scenario issues two attacks while stationary. These are binary gates,
not part of the guard-footwork score.

| Metric | Failure condition | Justification |
| --- | --- | --- |
| Input edges | Not exactly two attack presses | Proves that the scenario itself executed as intended. |
| Action lifetime | Either episode fails to reach phase 0.75 or lasts less than 0.25 s | Detects an attack that resets before presenting its contact pose. |
| Phase monotonicity | Action phase moves backward | Detects state resets and competing timeline owners. |
| Action weight | Maximum semantic action weight is below 0.999 | Requires the action layer itself to reach full weight. |
| Resolved clip dominance | More than two non-lower clips contribute at contact, or the strongest clip contributes less than 94% of their combined weight | Accounts for every competing upper/whole-body clip; a nominal action weight of one isn't enough if other clips dilute the rendered pose. |
| Hand extension | Neither hand travels at least 0.15 m from its pre-attack position | Verifies the rendered attack, rather than trusting only playback metadata. |
| Thrust elbow angle | The left shoulder–elbow–hand angle is below 160 degrees at contact | Compares the final global bones with the nearly straight authored thrust. |
| Stationary foot excursion | Either ankle moves more than 0.05 m during an attack | Prevents an upper-body attack from spuriously initiating a step. |

## Quickstep metrics

The quickstep scenario performs four directional dodges. It also uses hard
binary gates rather than the weighted guard score.

| Metric | Failure condition | Justification |
| --- | --- | --- |
| Input edges | Not exactly four dodge presses | Proves complete scenario execution. |
| Anatomical validity | Any frame reports an invalid joint | Applies the highest-priority invariant during airborne motion and landing. |
| Contact airborne | A supporting ankle is more than 0.127 m above terrain | Rejects a dodge foot that claims support while floating. |
| Grounded support | A grounded frame has no foot with meaningful support | Detects a landing with no physical contact owner. |
| Hard reach | Either leg reports hard reach | Rejects a clamped or infeasible dodge pose. |
| Supported radial extension | A supporting ankle is more than 0.90 m from the visual pelvis | Allows an intentionally airborne swing to extend, but not a planted leg. |

Raised-guard footwork owns a client-local alternating support/swing state. A
landing is constructed on the swinging foot's anatomical side of the support
contact, while intermediate airborne samples are exempt from planted-pair
separation checks. Landing completion advances the contact state independently
of analytic IK success, so a reach failure can be reported but cannot freeze
the gait at the end of a swing or reset both feet to authored FK.
| Impact contact | On every `Dodge` to ordinary transition, both soles must be within 0.01 m of terrain | Requires both feet to have arrived when impact and movement end. |

## Interpreting and changing metrics

The thresholds encode either an anatomical invariant, a physical presentation
contract, or a duration that has been judged visibly objectionable. They must
not be relaxed merely to make a run pass. If visual testing shows that a
threshold is poorly calibrated, change the analyzer and its documentation
together, preserve the old evidence for comparison, and explain the physical
or perceptual reason for the new value.

The score also can't measure every aesthetic defect. Manual animation mode is
still useful because a person can notice an odd silhouette, rhythm, or gesture
that no current metric names. The correct response is to add a reproducible
command sequence and measurable contract, not to replace the native test with
a smaller synthetic simulation.

## Directional raised-guard benchmark

Use `just animation-direction-benchmark` to record production-client bone
transforms while the character holds guard and walks forward, backward, left,
and right at identical input speed. The script gives each direction four
seconds, separated by one second at rest. An initial unguarded forward walk is
an unscored warm-up so first-use streaming does not systematically contaminate
the first scored direction. The script deliberately disables OBS and PresentMon
because the JSONL animation trace is the benchmark artifact.

Pass the reported `animation-state-*.jsonl` file to
`scripts/analyze_animation_direction_trace.py`. The analyzer discards the first
second and final half-second of every direction, converts the same 17 tracked
bones used by the viewer to parent-local transforms, and applies the viewer's
angular/local-position acceleration and jerk thresholds. It also measures the
longest-to-shortest half-step ratio against the viewer's 1.5 limit. A sustained
horizontal hip-foot offset above 0.65 m or a completed contact interval without
at least 0.05 m of swing travel and 0.03 m of clearance gain forces the
direction's quality score to zero. Otherwise its continuous motion score is
`100 / (1 + worst normalized threshold ratio)`:
50 means either the 95th percentile of the noisiest bone and derivative class,
or the cadence deviation from an even 1.0 ratio, reaches its allowed threshold.
A higher number is smoother. The analyzer reports the derivative-only value as
`motion_smoothness_score` so a static invalid pose cannot hide behind it. This
comparative score does not replace the binary acceptance gates or the viewer's
weighted quality score.

Catastrophic placement is also evaluated over the entire raised-guard interval,
including the rests between directional commands. If a foot becomes stuck when
movement stops, `benchmark_quality_score` is therefore zero even though the
steady-state derivative scores remain useful for diagnosing the preceding gait.

The analyzer marks the whole benchmark invalid if any sampled direction has a
render delta above 50 ms or an authoritative locomotion-sample gap above four
ticks. It reports those frames separately under `timing`; their derivative and
cadence spikes are transport or scheduling evidence, not directional animation
evidence. An invalid benchmark exits with status 1 and must be recorded again.
