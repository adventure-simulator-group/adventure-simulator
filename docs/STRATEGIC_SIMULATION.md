# Strategic NPC simulation

Provision scenarios use multiple food definitions and aggregate useful calories
across independent lots. Drivers must not assume all food is `travel_ration`.

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

Profiles retain their seed and all inputs needed for inspection: a deterministic sparse personality
(two through four non-neutral axes); correlated, bounded attributes; an explicit personality-by-attribute
build role; initial leaf skills; activity allocations that produce training;
activity-versus-quest, risk, and recovery preferences; equipment style and
utility weights; and provisioning, reserve, and spending preferences. Some
fields are recorded for later slices and do not yet affect settlement choices. Build derivation keeps
skills, training, activity, quest risk/recovery, and equipment style coherent. Content leaders are
activity-only; ambition increases quest propensity. Bravery selects heavy front-line melee only when
endurance and both-arm strength make it viable, while fearful agents prefer ranged/light roles when
their perception supports one. Followers still defer to the current leader's quest/activity decision;
individual follower policy is applied to training, recovery, treatment, and equipment, which is a known
party-decision limitation.

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
completion and the subscribed state that follows it. It creates several
independent parties through ordinary join request/accept reducers. Each leader
then independently chooses settlement activity or questing from its generated
policy until its in-game duration or cycle bound is reached. Questing selects a
quest, travels through persisted camp stops, autoresolves, stores loot, returns,
turns in, liquidates party loot, withdraws the member's earned stake, purchases
from the merchant, and equips an upgrade. Followers travel and run their own
daily schedules. Defeat causes a retreat, bounded settlement convalescence, and
a bounded retry; an incapacitated party is never autoresolved repeatedly in
place.

Core-loop reports are explicitly tagged `spacetimedb_authoritative_core_loop`
and retain the server origin, disposable database, claimed run nonce, generated
profiles, semantic action trace, final equipment and capabilities, and metrics
for quest results, activity days, camps, loot value, proceeds, earned stake
withdrawals, purchases, upgrades, unexpected reducer failures/retries, stuck
detection, and duplicate semantic events. Final agent rows distinguish the
legacy character gold field, personal gold-coin stacks, party treasury, and
party stake. Generated preferences drive quest/activity choice, quest risk, and
weighted equipment utility (protection, mobility, price, and reach) for
unarmored, light, heavy, and ranged styles. An upgrade counts only after the
authoritative equipment row shows the purchased inventory item.

Live simulated NPCs inspect persistent equipment condition before choosing quests or settlement
activity. They submit repairable damaged equipment to the appropriate local smith, wait through the
ordinary rest reducer until the longest ETA, and retrieve every completed order before continuing.
Their replacement utility is discounted by current condition, so maintenance competes coherently
with buying a replacement. Reports include submissions, retrievals, repair wait time, worst final
condition, and outstanding orders; deterministic simulation setup seeds damage through a reducer
guarded to registered simulation characters.

Medical needs are evaluated before repairs, and repairs before equipment upgrades. The disposable
fixture seeds one deterministic influenza episode behind the same claimed-run capability boundary as
other simulator-only setup. Policy observes public condition and the narrow public symptomatic/critical
signal rather than private infection identity or vitals, uses the ordinary herbalist examination and
filters the trusted one-shot result by its simulator-owned patient ID, then either
crafts a matching course when skill and already-owned ingredients justify it or buys the prepared
course. It verifies the authoritative equipped-medication row, dismisses the examination, and rests in
bounded one-day steps until ready. While treatment is active it authoritatively replaces the saved
personality schedule with pure rest, then restores that profile schedule after recovery so labor or
thievery cannot interrupt convalescence with an incident. Quests remain suppressed while a member is unsafe. Reports audit
diagnosis attempts/results, crafting or purchases, medication equips, treatment gold and time,
recoveries, suppression, and terminal deaths.

Safety is intentionally strict. URLs are parsed structurally and must be an
exact credential-free HTTP loopback origin with no path, query, or fragment.
The command accepts only an `adventuresim-sim-*` database and refuses any
pre-existing run, character, party, or settlement state. It atomically claims
the fresh database with an owner identity and nonce before seeding. Bootstrap
configuration requires that claim and permanently marks each simulated
character by run and agent ID; simulated and ordinary characters cannot merge
parties. In addition, ordinary module builds compile with simulation claims
disabled. The recipe creates 32 random bytes in memory, exposes them to exactly
one module build and runner process through `ADVENTURESIM_SIM_BOOTSTRAP_TOKEN`,
and never accepts the capability as a CLI argument or writes it to a report.
The public claim reducer checks that build-time capability before inspecting
database freshness. The recipe creates a nonce-named database, exposes no
host/database override, and deletes it on exit. Population, duration, cycles,
action waits, camp continuation, defeat retries, and recovery loops are bounded.

There are two distinct random streams. The CLI seed deterministically controls
profiles and policy choices. Combat is authoritative: the autoresolver obtains
its seed from server RNG, and reports record that actual seed together with
rounds, summary, and log. Consequently the native backend supports exact replay,
while a core-loop rerun reproduces policy inputs but not necessarily combat
outcomes. Its trace is the debugging artifact. Reports identify the server,
database, and claimed run; the current SDK does not expose a deployed module
binary digest.

Current limitations are:

- no native Raiding execution until an authoritative equipped-capability observation exists (generated schedules exclude it and custom schedules are rejected);
- no parity claim for live bulk multi-day rest, whose aggregate rounding and incident interruption semantics differ from repeated one-day actions;
- the bounded bootstrap applies generated attributes, initial skills, and
  downtime schedules, while equipment starts from the normal character
  creator before policy-driven upgrades;
- party loot is liquidated through the shared party treasury; upgrades must be
  funded by withdrawing the character's earned stake before a personal trade;
- duplicate detection covers the simulator's semantic action stream, not the
  strategic-web rendered DOM;
- no tactical ticks and no persistent production NPC rows.

## Commands

```powershell
cargo run -p adventuresim-strategic-sim -- run --seed 42 --population 100 --days 1095 --output report.json
cargo run -p adventuresim-strategic-sim -- replay --report report.json
cargo run -p adventuresim-strategic-sim -- matched --seed 42 --days 365
# Safe disposable integration run (requires local SpacetimeDB 2.6.1):
just strategic-sim-core-loop 42 8 20 30 2
```

Direct `core-loop` invocation is intentionally an expert-only path: its process
must inherit the same `ADVENTURESIM_SIM_BOOTSTRAP_TOKEN` used to compile and
publish that disposable module. There is no token CLI option. Prefer the recipe,
which keeps the capability confined to one shell process and always cleans up.
