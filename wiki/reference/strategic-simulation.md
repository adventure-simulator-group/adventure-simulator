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
policy until its in-game duration or cycle bound is reached. Before an outbound
case-site leg, the evaluator derives a conservative public provisioning horizon
from disclosed distance and walking schedule. Four times the ordinary daylight
elapsed projection covers fatigue-expanded outbound travel plus the return leg;
a further one-day reserve covers delays and encounters. It forecasts food and
water for the living party from public needs and inventory and buys an
affordable shortfall through the ordinary merchant reducer. A co-located living
party member may pay using the shared treasury plus their purse, while retaining
their observable medical reserve. Pricing fails closed without that payer's
public local-price effect and exactly one public default merchant present at
that payer's own public character time. If
either staple is unavailable or unaffordable, the party
remains at the settlement and performs sustainable activity instead of
knowingly departing empty. Questing then travels through persisted camp stops,
autoresolves, stores loot, returns,
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

Each autonomous party choice records an observer-safe `quest_decision` before
the selected action. It includes the deterministic policy selector and quest
propensity, current settlement, count of player-visible offered contracts,
the leader's count of open generated cases and projected investigation actions,
and the selected direct-contract, generated-discovery, generated-case, or
activity path. An owned open generated case takes precedence over the random
activity selector. Repeated daily decisions
are intentionally exempt from semantic-duplicate alarms. Because this evaluator
does not open tactical crime incidents, authored Thievery and Raiding schedule
minutes are explicitly reassigned to legal Labor in the effective schedule
instead of disappearing into leisure; the authored preference remains intact.
When a non-earning schedule such as Prayer meets low-food reserve pressure, its
preferred allocation temporarily becomes legal Labor with
`subsistence_reserve_to_labor`; the authored preference is restored when the
pressure clears. The chosen effective schedule is installed and verified
before the ordinary rest reducer runs. Temple viability requires one visible
day of food only because settlement water is authoritative and free. Otherwise
the policy selects a full-board Inn only when the purse covers its public cost
plus observable medical commitments and as much of the profile's visible cash
reserve target as is currently attainable. A Temple remains the player-visible
last resort while initial Labor builds that reserve. Activity events retain
the preferred and effective activity, installed schedule, venue, committed
reserve, fallback reason, and public pre/post purse, strategic condition,
hunger, thirst, visible food and water, character minutes, and signed deltas,
with `outcome=completed`. A rejected settlement-rest attempt
instead emits one `outcome=failed` activity event before the run stops,
containing its public pre-action state, effective plan and venue, stable stage,
and safe error category without raw reducer text. Camp handling subscribes only
to the public `party_journey` and `party_journey_itinerary` projections. At
each stop it applies the same remaining-interval overlap as the web UI to
`completed_elapsed_minutes..total_elapsed_minutes`, rests exactly to the end of
the active forecast camp through `rest_at_camp`, logs bounded pre/post public
camp state, and re-reads the journey, itinerary, party, health, and leader
before `continue_camp_travel`. A missing, overlapping, or non-advancing
projection fails closed instead of guessing a fixed rest duration.
Successful final-agent rows carry the same public needs, visible food and water,
settlement services, herbalist quote, and inn full-board cost used by failure
diagnostics. Failure artifacts use schema version 4. In addition to the strict
event vocabulary and activity-detail semantics, they retain only an allowlisted
operation name and stable reason code for expected investigation and camp
failures. `rest_at_camp` and `continue_camp_travel` are allowlisted operations
with stable daylight-window and journey-projection reason codes; raw reducer
text is never copied into the artifact. These fields make poverty,
starvation, unavailable-rest, and stale temporal-action deadlocks diagnosable
without exposing hidden case truth.

In full-world runs, leaders without an offered direct contract discover local
generated problems through the same ordinary player dialogue used by the web
client. The runner uses only public settlement-NPC facts and current public
presence. Discovery is discriminated by the public settlement location and
time, not by the source NPC. Each decision performs exactly one ordinary
dialogue with the stably first public representative at the inn when one is
present. The settlement overview is used only when no inn representative is
present, matching the same public discovery rule enforced by the server. An
unproductive action falls back to an ordinary settlement activity day before
another discovery decision.

The runner records a bounded public attempt and result for that action:
visible candidate count, public candidate identity/name/location, dialogue and
owner-session success, and owner-scoped open-case counts before and after. A
new owner-visible open case is the postcondition for an observer-safe
`rumor_delivered=true`; the evaluator does not read private delivery receipts
or generation eligibility. Stable result reasons distinguish
`rumor_delivered`, `no_public_rumor_available`, and `no_visible_contacts`.
Reports separately count attempted and fruitful discovery actions plus
unproductive discovery decisions, including decisions with no visible
discovery location. The pre-action quest-decision event distinguishes policy
intent from selection with `quest_intended` and `quest_selected`; the later
discovery-result event alone reports the postcondition and whether the ordinary
activity fallback follows.

Generated local-problem authority uses the official world clock for its active
window. Dialogue discovery therefore checks problem `starts_at`, `ends_at`, and
resolution against official world time, while journal `learned_at` and
`recorded_at` remain on the observing character's elapsed timeline. Travel,
treatment, and bulk settlement activity can move that observer timeline
independently and must not make a world problem appear prematurely active or
expired.

The runner then follows only owner-scoped
case, journal, lead, dialogue-topic, action, action-outcome, and exact-site-pin
projections. Referred witnesses are never inferred from generated truth:
same-named public candidates are tried in stable order and only the candidate
whose projected session exposes `referred-testimony` for the selected public
case is selected. Investigation actions and their outcomes are likewise
filtered by exact owner and public case before reducers receive the projected
action ID, method, and version. Exact site travel selects only the pin matching
the party's current case-site occupancy. Combat requires an owner-scoped,
public-case-scoped binding matching party, battle, mission, and site. The state
machine rechecks the current leader and every member's public strategic
condition after each time-advancing action, travel leg, and combat. A completed
case returns any surviving party from its occupied site before the case leaves
the active loop. When a projected action advertises `night_window` with a
bounded `wait_minutes`, the runner uses the ordinary settlement-rest action
when an affordable service is available, otherwise ordinary field rest, then
re-reads the projection, expected version, leader, and party health before
acting. It never parses action prose or reducer errors to decide to wait. The
runner records bounded pre-call evidence for each generated investigation
attempt: immutable public case subject, public case/action IDs, projected
method and summary, expected version, availability reason and wait, and the
actor/party public clocks. A narrowly allowlisted victim-cohort
moved/changed/unavailable reducer result triggers one observer-safe projection
refresh and then defers to the next cycle without aborting, even if the local
subscription cache has not applied a new row yet. The authoritative gateway
projection independently checks the current private cohort binding and exposes
only generic `target_changed` unavailability; it never reveals which target
predicate changed. On the next cycle the runner can choose another available
public action instead of deterministically retrying the stale one. Reports
count replans, and neither events nor failure artifacts copy the raw authority
error.

The state machine is bounded per cycle and falls back to sustainable settlement
activity when no legal projected step is available.

Reports separately count generated cases discovered, completed by the
simulated party's immediate dialogue/action/autoresolve transition, and closed
externally by background resident NPCs. They also count projected investigation
actions, temporal waits and wait minutes, observer-safe replans, and witness
dialogues. Generated-case trace events contain only public
case IDs/subjects, NPC names and locations already visible in the selected
dialogue, projected action summaries, and public outcome wording; they never
read generation manifests, canonical causes/sites, reliability, hostile
authority, custody authority, or outcome authority.
The same report exposes unique owner-party discoveries, exact-site-ready cases,
finance-blocked cycles, case-site journeys, provision purchases, and actual
public gold spent. Identical affordability signatures enter backoff until the
required budget or observable funds change. Direct contracts preflight against
the greatest public distance among their case destinations before acceptance,
then select the disclosed owner-scoped pin by minimum-distance, stable-site
ordering and re-run observer-safe provisioning for that exact pin before
travel. Thus a temporary shortfall does not withdraw the offer or fund travel
to a different destination. An explicit post-defeat
cannot-reprovision abandonment is
reported as abandonment rather than deferral.

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
The policy reproduces the player-visible herbalist quote from the public item definition, visible
storefront stock, and the gateway-projected local-problem trade modifier. Affordability includes the
visible cost of the required one-day rest venue, preferring a free temple to a paid inn. An affordable symptomatic character buys a course;
an unaffordable character, a settlement without an herbalist, or a nonsymptomatic convalescent
instead takes bounded one-day natural recovery. Equipment maintenance retains one locally quoted
course as an emergency reserve rather than consuming every coin before a later symptom becomes
visible. It rests in bounded one-day steps until ready. Before each choice, the
trace records public condition, symptomatic status, settlement, purse, quote,
affordability, action, and reason. Recovery completion records
`recovery_context=public_symptoms` and keeps the pre-rest symptomatic
observation separate from the newly read post-rest observation; it does not
claim a private physiological cause.
While recovery is active it authoritatively replaces the saved
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
lead, route-specific preparation, an observer-safe chronological projection of
the generated action chain, and the result.
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
