# Strategic NPC simulation

`adventuresim-strategic-sim` is a native, deterministic experiment harness for
balance exploration and regression reproduction. It has two deliberately
different backends: a fast native settlement-activity model for multi-year
population sweeps, and an opt-in reducer-backed core-loop driver for behavioral
and integration testing.

## Reproducibility contract

Configuration, manifests, profiles, traces, snapshots, and reports are
versioned JSON. Unknown configuration fields are rejected and population,
duration, decisions, trace events, and snapshots are bounded. The profile
generator uses the repository-owned SplitMix64 implementation and a stable
sub-seed per agent. Generation and simulation never depend on hash-map
iteration order or `DefaultHasher`. Reports use ascending agent ID within each
ascending day. Changing the random algorithm, field interpretation, or
canonical ordering requires a format-version bump.

The canonical BLAKE3 digest covers the manifest and all canonical report data;
the digest field itself is blanked before hashing. `replay` reruns the recorded
manifest and verifies both the stored report and reproduced digest. There is no
performance or wall-clock metadata in the canonical report. Reproducibility is
guaranteed for the same simulator format. Canonical hashing quantizes floating
values to four decimal places so harmless JSON and cross-platform subprecision
differences do not change the digest; stored balance metrics retain their full
precision. A future fixed-point game-state migration can strengthen guarantees
beyond that explicit tolerance. Config and report inputs are streamed with a 64 MiB limit,
and their vector bounds are revalidated before hashing or replay.

Daily scheduling is intentional and represents repeated one-day player actions.
It gives future incident policies a single canonical decision granularity. Pure
schedule training and settlement activity calculations live in
`adventuresim-core`; both the SpacetimeDB module and simulator call those same
functions. The live reducer's bulk rest currently trains and then evaluates one
aggregate activity interval; rounded income and single incident interruption
can therefore differ from repeated one-day actions. Bulk-rest strategy testing
is follow-up work rather than part of this first-slice contract. Gold from labor
and thievery is an intentional economic source. Future purchases, provisions, lodging, and losses will be explicit
sinks; the runner therefore does not assert naive currency conservation.

## Profiles and results

Profiles retain their seed and all inputs needed for inspection: correlated,
bounded attributes; initial skills; training and activity allocations;
activity-versus-quest, risk, and recovery preferences; equipment style and
utility weights; and provisioning, reserve, and spending preferences. Some
fields are recorded for later slices and do not yet affect settlement choices.

Reports include a bounded decision trace, bounded periodic snapshots, terminal
reason, wealth, final and gained skill hours, activity and leisure time,
notoriety, and cumulative risk exposure. Reports also contain the typed Pareto
frontier that maximizes wealth and skill-hour gain while minimizing notoriety
and risk exposure; the human summary prints its stable agent IDs. Risk exposure is a metric, not a fake
combat or incident outcome. Pareto utilities require typed maximize/minimize
objectives, preserve exact ties, and reject nonfinite values. The `matched`
command holds a profile and seed constant while changing only its declared
labor/thievery activity preference and allocation.

## Authoritative core-loop backend

`StrategicBackend` separates observations and player-like intents from native
state mutation. `NativeSettlementBackend` remains the fast deterministic
backend. The `core-loop` command instead connects through generated typed SDK
bindings and serializes ordinary reducer calls, waiting for both reducer
completion and the subscribed state that follows it. It forms solo parties,
merges them through ordinary join request/accept reducers, selects a quest,
provisions and travels through persisted camp stops, autoresolves, stores loot,
returns, turns in, liquidates party loot, and attempts a personal equipment
upgrade. Defeat causes a retreat, bounded settlement convalescence, and a
bounded retry; an incapacitated party is never autoresolved repeatedly in
place.

Core-loop reports are explicitly tagged `spacetimedb_authoritative_core_loop`
and retain generated profiles, a semantic action trace, final equipment and
capabilities, and metrics for quest results, camps, loot value, proceeds,
purchases, upgrades, reducer failures/retries, stuck detection, and duplicate
semantic events. Generated preferences drive quest risk selection and upgrade
style; the remaining preferences stay in the profile for later policy work.

Safety is intentionally strict. The command accepts only an explicit loopback
HTTP host and a database name beginning `adventuresim-sim-`. The recipe resets
only that caller-named disposable database, never the shared development
module. Population, cycles, action waits, camp continuation, defeat retries,
and recovery loops are bounded. The autoresolver obtains its seed only from the
authoritative reducer; the simulator has no combat-seed input.

Current limitations are:

- no native Raiding execution until an authoritative equipped-capability observation exists (generated schedules exclude it and custom schedules are rejected);
- no parity claim for live bulk multi-day rest, whose aggregate rounding and incident interruption semantics differ from repeated one-day actions;
- the bounded bootstrap applies generated attributes, initial skills, and
  downtime schedules, while equipment starts from the normal character
  creator before policy-driven upgrades;
- party loot is liquidated through the shared party treasury while personal
  upgrades use the character's personal merchant balance;
- duplicate detection covers the simulator's semantic action stream, not the
  strategic-web rendered DOM;
- no tactical ticks and no persistent production NPC rows.

## Commands

```powershell
cargo run -p adventuresim-strategic-sim -- run --seed 42 --population 100 --days 1095 --output report.json
cargo run -p adventuresim-strategic-sim -- replay --report report.json
cargo run -p adventuresim-strategic-sim -- matched --seed 42 --days 365
# Safe opt-in integration run (requires local SpacetimeDB 2.6.1):
just strategic-sim-core-loop adventuresim-sim-manual-42 42 2 1
```
