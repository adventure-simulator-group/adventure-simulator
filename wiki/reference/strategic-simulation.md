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
(two through four non-neutral axes across thirteen behavioral axes), plus
always-assigned Sex, Presentation, and Inclination; correlated, bounded attributes; an explicit personality-by-attribute
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
other simulator-only setup. Policy observes only public condition and the narrow public
symptomatic/critical signal, buys a fixed concrete preparation, and invokes the generic administration
reducer without reading infection identity, crafting, diagnosing, or selecting an effect by disease.
It rests in bounded one-day steps until ready. While recovery is active it authoritatively replaces the saved
personality schedule with pure rest, then restores that profile schedule after recovery so labor or
thievery cannot interrupt convalescence with an incident. Quests remain suppressed while a member is unsafe. Reports audit
diagnosis attempts/results, crafting or purchases, medication equips, treatment gold and time,
recoveries, suppression, and terminal deaths.

Safety is intentionally strict. URLs are parsed structurally and must be an
exact credential-free HTTP loopback origin with no path, query, or fragment.
The command accepts only an `adventuresim-sim-*` database and refuses any
pre-existing run, character, or party state. Fixture mode also refuses any
settlement or import state. Full-world mode permits settlements only when a
completed `world_data_import` proves they came from the pinned compiled world,
and records its artifact ID and manifest digest in the report. It atomically
claims the database with an owner identity and nonce before creating simulation
characters. Bootstrap
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
just strategic-sim-core-loop target/sim-runs/fixture-001 42 8 20 30 2
just strategic-sim-core-loop-world target/sim-runs/world-001 42 8 20 30 2
```

## Quest evaluators

There are two gameplay evaluator surfaces and one offline content-analysis
surface, with deliberately different boundaries.

### Offline generated-content analyzer

`quest-analyze` projects deterministic generated investigations into an
observer-safe, in-memory `PlayerFrame`. It is useful for generator regression,
route diversity, policy fingerprints, dead ends, loops, correction persistence,
and separate public/developer audit artifacts. It does **not** call
SpacetimeDB reducers, exercise the browser, perform tactical combat, or prove
production gameplay behavior.

The default recipe is credential-free and the mock policy round-trips through
the same strict JSON response parser used by an OpenAI-compatible provider:

```powershell
just quest-analyze-mock 41 4
```

This creates `quest-analysis-public.json`, `quest-analysis-developer.json`, and
`quest-analysis-stories.md` as distinct artifacts and refuses to overwrite
them unless direct invocation passes `--overwrite`. The public report contains
only player-visible traces, bounded run provenance, classifications, and
aggregate behavior. Seeds, catalog revisions, canonical truth, structured
factor traces, bridge IDs, generator marginals, and truth-joined
classification/counterfactual audits remain in the developer report. Their
digest join is one-way.

All three artifacts are rendered in memory first. Per-artifact and combined
byte budgets are checked against the exact pretty JSON and Markdown bytes
before any output file is created. Existing-ancestor canonicalization also
prevents distinct-looking paths through symlinked or junction parents from
collapsing public and developer output onto the same file.

Each trace records pre/post observation digests, opaque legal choices, exact
dialogue, public discoveries and corrections, preparation, costs, exhaustion,
and termination. Classifications are bounded answers, never chain-of-thought.
Prepared/unprepared solve rates are descriptive quality slices, not causal
skill or equipment-benefit estimates.
Counterfactual comparisons are made only when different hidden cases naturally
share an identical player-visible initial prefix; absence of such a group is
reported as `not_measured`.

Contract completion, language benefit, tactical combat benefit, causal skill
benefit, and accidental perception discovery are also explicitly
`not_measured` because this projection does not implement those mechanics.
No proxy number should be treated as evidence for them. The privacy audit is a
structural type boundary plus canary scan, not a formal proof.

An observed non-solving run can be promoted into a reviewable deterministic
fixture candidate:

```powershell
just quest-analyze-promote 41 recurring-depredation
cargo run -p adventuresim-strategic-sim -- quest-analyze-replay `
  --fixture quest-analysis-replay-candidate.json
# Replay the reviewed checked-in regression:
just quest-analyze-replay-fixture
```

The versioned fixture joins catalog revision and generator manifest, records
opaque decisions, and checks stable outcome fields. Promotion never silently
commits or approves a fixture. Only step-limit and dead-end failures can be
promoted; provider failures, loop detection, and exhausted runtime budgets
cannot be reproduced from an opaque action list alone.

### Server-simulated NPC adventurers

The reducer-backed core loop is also the NPC-adventurer evaluator. These are the
same resident NPC companies that may intervene in old, escalating generated
cases during normal strategic settlement activity. Eligibility considers case
age, incident count, prior retry time, company availability, and recent player
activity. SpacetimeDB selects the company, applies the bounded result through
the existing case/objective/custody/local-problem authority, and persists an
idempotent intervention record.

The default policy is deterministic and credential-free. An optional LLM may
choose only one of the strategies advertised by a gateway-only candidate view:
investigate carefully, protect locals, confront directly, or defer. The model
does not receive canonical cause/site/reliability/weight data and cannot apply
an outcome; a simulation-capability-owned reducer validates its opaque choice.
The server then selects an evidence-supported physical, pattern, or social
route from the generated action graph. Retries rotate to a different route when
the case supplies one. The decisive action uses the same investigation resolver
as player actions before any strategic finale result is applied.

Every core-loop run writes `npc-adventurer-stories.md` by default. Each entry is
server-authored in the same transaction as its outcome and records the problem
being learned, timestamped witness interviews, exact spoken lines, the chosen
lead, route-specific preparation, the generated action chain, and the result.
Failed attempts state the concrete setback and the alternate route intended for
the retry instead of reporting a content-free failure. It contains no hidden
quest truth. The JSON report also carries the same Markdown for archival
purposes.

Because the production world clock is tied to elapsed wall time, a claimed
disposable simulation has one additional bounded reducer that advances that
same authoritative clock by a requested number of game minutes. The core loop
uses it once per active simulated day, then invokes ordinary settlement
activity so follow-up incidents, escalating penalties, eligibility, and NPC
interventions all occur through the production systems. The capability is
absent from normal module builds. Simulation characters receive a small
starting purse so an inn-only seed settlement cannot deadlock before its first
labor day; all accommodation and food costs still use ordinary currency rules.

Use the normal isolated recipe for the scripted policy:

```powershell
just strategic-sim-core-loop-world target/sim-runs/world-001 42 8 20 30 2
```

Direct expert invocation can opt into an OpenAI-compatible strategy policy:

```powershell
cargo run -p adventuresim-strategic-sim -- core-loop `
  --host http://127.0.0.1:3000 --database adventuresim-sim-UNIQUE `
  --run-nonce UNIQUE-NONCE --npc-strategy-policy openai `
  --imported-world --expected-world-manifest-digest PINNED-DIGEST `
  --output report.json `
  --npc-allow-network --npc-api-key-env OPENAI_API_KEY `
  --npc-stories-output npc-adventurer-stories.md
```

Provider mode requires explicit network consent and reads the credential only
from the named environment variable. HTTPS is required except for loopback test
fixtures. The candidate list and strategy override are gateway/simulation
capability surfaces, not general player APIs.

### End-to-end browser quest evaluator

The browser evaluator is deliberately separate from the strategic NPC
evaluator. It always uses an LLM and interacts with the running local game only
through visible web controls. The model receives the current screenshot,
visible page text, and opaque handles for visible enabled controls. It cannot
name a reducer, use a quest authority ID, invent an action, or navigate directly
to a guessed route.

Each run writes an immutable screenshot log: `index.html`, `manifest.json`, and
one viewport PNG for the initial state and every subsequent action. The log
therefore shows exactly what was on screen when the model made each decision.
Use a new output directory for every run:

```powershell
just quest-web-eval quest-browser-run-001 `
  http://127.0.0.1:24301 /characters OPENAI_API_KEY gpt-4.1-mini
```

Network use must be explicit, the game URL must be loopback, and provider
endpoints must use HTTPS unless they are loopback test fixtures. The command
fails closed when the named API-key variable is absent. CI exercises the strict
decision protocol and a loopback model fixture; it does not make paid model
requests.

Direct `core-loop` invocation is intentionally an expert-only path: its process
must inherit the same `ADVENTURESIM_SIM_BOOTSTRAP_TOKEN` used to compile and
publish that disposable module. There is no token CLI option. Prefer the recipe,
which keeps the capability confined to one shell process and always cleans up.

The full-world recipe is the authoritative core-loop workflow. It publishes a
nonce database with the simulation capability, loads exactly
`target/world-1544.json`, verifies the completed import through the simulator's
typed subscription, checks the file's size and SHA-256 against
`world-runtime-release.lock.json`, requires the observed import manifest to
match that verified file, then runs without calling `seed_simulation_world`. It
chooses the lexicographically first imported settlement ID so the loaded-world
start is deterministic. The explicit output directory must not exist. A
successful run contains `report.json`, `npc-adventurer-stories.md`, and
`launcher.json`; failed launches retain `launcher.json` with the failed stage.
The launcher attempts to delete the disposable database on every exit path and
reports `cleanup_failed` with a nonzero exit if deletion is not confirmed.

The reducer-backed core loop subscribes to strategic encounters and resolves
each through the same public reducer used by the Map/camp UI. Its report records
encounter frequency; sneak, detour, attack, run, and surrender choices; escape
eligibility; exact surrendered item/value losses; encounter defeats; and full
party wipes. Encounter events in the trace retain the canonical encounter ID,
chosen action, and authoritative outcome for replay diagnostics.
