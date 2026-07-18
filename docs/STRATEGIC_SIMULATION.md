# Strategic NPC simulation

`adventuresim-strategic-sim` is a native, deterministic experiment harness for
balance exploration and regression reproduction. This first increment covers
only character settlement downtime: saved daily training and activity
allocations progress once per canonical in-game day. It is part of issue #66,
not a complete synthetic-player system.

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

## Backend seam and current limitations

`StrategicBackend` separates observations and player-like intents from state
mutation. `NativeSettlementBackend` is the only implementation today. A future
adapter should submit legal intents to the authoritative reducer interface and
observe the same data a player can see. It must not duplicate quest, travel,
trade, equipment, incident, or combat rules in a parallel simulator.

This increment deliberately has:

- no live SpacetimeDB/reducer adapter;
- no quest, travel, trade, equipment purchase, recovery, or provisioning actions;
- no native Raiding execution until an authoritative equipped-capability observation exists (generated schedules exclude it and custom schedules are rejected);
- no parity claim for live bulk multi-day rest, whose aggregate rounding and incident interruption semantics differ from repeated one-day actions;
- no shared-world population competition;
- no strategic-web dialogue or session end-to-end coverage;
- no tactical ticks and no persistent production NPC rows.

## Commands

```powershell
cargo run -p adventuresim-strategic-sim -- run --seed 42 --population 100 --days 1095 --output report.json
cargo run -p adventuresim-strategic-sim -- replay --report report.json
cargo run -p adventuresim-strategic-sim -- matched --seed 42 --days 365
```
