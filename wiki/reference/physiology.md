# Physiology system

Physiology is the skill for preventing avoidable disease exposure, observing
health, administering prepared interventions, and improving wound recovery. It
produces a fallible differential of possible diseases, but never reveals the
authoritative disease identity or recommends a treatment.
[Herbalism](herbalism.md) now owns bounded biological preparation while
Alchemy #215 remains independent. Preparations act through versioned generic
meter profiles and never disease-keyed effects.

## Professional boundary

The College of Physicians is distinct from both the Fellowship of Herbalists
and the Surgeons' Guild. Physician training and advancement use Physiology
only: physicians observe patients, form fallible differentials, and administer
non-operative treatment. Herbalists prepare remedies through Herbalism, while
surgeons train Surgery for operative procedures.
Membership and advancement in one organization never confer a rank in another.

Physiology also supports a narrow bedside **Reassure** response to companions'
injury or pain, fatigue, and hunger or thirst morale concerns. This is honest,
period-facing attention to the patient's account and plainly observable
distress. It is a modest, low-risk morale intervention rather than medical
success: the action does not identify a condition, predict its course, triage
the patient, prescribe supportive care, endorse Humour theory, or alter any
physical state. Hunger or thirst reassurance is intentionally the weakest
authored case. See [Morale](../shared/morale.md) for the shared social-action,
relationship, cooldown, history, privacy, and automatic-care rules.

## Private authority

The authoritative strategic simulation derives ten bounded functional-loss
meters:

- oxygenation
- perfusion
- hydration
- temperature
- inflammation
- coagulation
- nutrition
- neurologic function
- renal clearance
- tissue integrity

Disease curves are deterministic and piecewise linear. A versioned, server
secret keyed phenotype changes the relative involvement of meters for each
episode while keeping the population mean neutral. Baselines, phenotype values,
raw meters, infection identifiers, and disease identity stay in private
SpacetimeDB state.

Interventions use generic, versioned profiles with meter deltas over time.
Administration records the patient, concrete preparation and profile version,
route, amount, optional body region, start and stop minutes, and private
sensitivity/adverse variation. No effect lookup accepts a disease key.
The personal inventory presents medication with the familiar checkbox gesture.
Checking it administers and consumes that concrete quantity-one item as one
standard 1,000-milliunit, whole-body course. The trusted server selects the
current profile version and the preparation's intrinsic route; the browser does
not choose medical parameters. The compact current-medication status retains a
Stop action and disappears when a course is stopped or reaches its authored
duration. Durable start and stop markers remain in the physician notebook.

All authoritative personal-time paths evaluate disease and intervention effects
together. The earliest integer-minute terminal crossing wins, with a stable
meter-order tie-break, so splitting an interval into smaller calls cannot change
the result.

Disease exposure is assembled by an explicit transmission route before the
private acquisition roll. Abstract settlement outbreaks use each disease's
authored primary community route; contaminated food and water, wounds, infected
blood, and within-party close contact retain their direct routes. Party
Physiology sharply reduces avoidable food/water and close-contact exposure,
reduces vermin exposure by a lower ceiling, and only modestly reduces blood
exposure. Wound risk is unchanged because Surgery and physical wound care own
that route. Every route retains residual risk, and available washing supplies,
wound closure, and other physical affordances continue to determine their own
stronger effects.

Each route splits its dose into an unavoidable/environmental component and a
preventable party-behavior component. Physiology reduces only the latter.
Route preventability and the current physical-affordance multiplier are named
inputs to that split. Infected blood illustrates the distinction: automatic
washing with actual soap removes blood before exposure, while bandaging or
stitching lowers the real cut route; if infectious blood remains, missing clean
handling and an open wound cap how much additional Physiology judgment can do.

Shared travel, camp rest, and treatment snapshot the explicitly co-advancing
participants and their capability-pinned Physiology coverage before changing
any clock. Every participant is previewed and committed against that immutable
plan, so character ID and reducer iteration order cannot change prevention or
transmission. Community, blood, and contact acquisitions resolve together in
absolute-minute order. Acquisitions within one minute are simultaneous and a
new infection can transmit onward starting on the following minute, making
secondary spread agree between one long interval and equivalent chunks.
Environmental sources retain stable later attempts, allowing exposure to
resume if an earlier episode resolves while the outbreak continues.

Solo catch-up uses recorded co-presence only. An open span may cover the
catching-up character through the lesser of their requested horizon and an
already-ahead peer's current clock, but never borrows time the peer has not
elapsed.

The authoritative database initializes private key material from runtime
randomness and persists one private key for the lifetime of the database.
Changing its version requires recreating the disposable pre-launch database;
older causal rows fail closed instead of being re-derived with new material.
Infection and administration causal rows pin the ruleset and key versions used to interpret
them. No secret is compiled into the module, placed in an environment-backed
WASM constant, or exposed through a public view.

## Observer notebooks

Physiology exposes four period-facing Humours: Sanguine, Phlegmatic, Choleric,
and Melancholic. Each is a documented weighted sum of several private meters.
The map is intentionally many-to-one, so a humour reading cannot reconstruct a
private meter or disease identity.

Notebook authorization is based on persisted pair-presence spans. Joining or
rejoining a party opens a fresh span at the lesser of the two personal clocks;
departure and death close it by the same rule. Each direction stores the
observer's Physiology capability band at that boundary, preventing later
training from sharpening historical observations. Every notebook records at
most one examination per day. Higher bands produce more finely quantized
readings and a better-calibrated differential, but do not add examinations.

The same private spans bound passive prevention. Each span pins both members'
Physiology bands and is clamped to their lesser personal clock, so joining,
leaving, training across a band boundary, lazy catch-up, and split time advances
cannot rewrite who supplied knowledge during an elapsed minute. A character
without any pair-presence history receives no interval-wide solo fallback,
because applying a current skill retroactively would make catch-up chunking
change outcomes. Point-in-time actions such as consuming a food lot may use the
actor's current capability safely.

Close-contact transmission reads source infections and pair presence only
inside private authority. Browsers learn neither who exposed whom nor whether a
Physiology reduction changed a particular roll; ordinary symptom notices remain
the first public illness signal.

The trusted strategic gateway derives a bounded, pre-quantized chart on demand
from causal infection, administration, presence, capability, ruleset, and key
boundaries. Periodic meter or Humour snapshots are never persisted. A chart
contains:

- timestamped, signed humour deviations for each body region
- a fallible differential of possible diseases
- interventions the observer could know about
- explicit gaps for absence

Externally visible findings are never listed directly. Coughing, fever, rash,
and every other finding are forced through the deliberately unhelpful Humour
lens and contribute to the relevant regions. Healthy is the zero baseline;
both positive and negative deviations are shown, and the absolute deviation
contributes to the Humour-colored impairment in the corresponding regional
health bar.

The physician notebook presents seven tall graphs side by side, one for each
body region. Time runs vertically. Hovering, focusing, or selecting an
observation opens its quantized snapshot. Medication starts and stops are
horizontal event markers, while party absence is drawn as a hatched interval
that also breaks the Humour lines. Stable keyed observer noise varies the
readings from day to day without changing the authoritative body state, making
early treatment response deliberately ambiguous.

Possible diseases are ordered and colored from red through yellow to green.
Physiology skill and time spent observing make those colors better calibrated.
Known interventions remain visible as timeline markers so the player can
interpret subsequent improvement or deterioration; they are not a hidden
disease-identity input to the differential. Fantastic diseases add strongly
coordinated, authored meter sequences. Once an observer has enough uninterrupted
authorized history, a pure scorer compares only the actual quantized visible
readings and their timestamps with public definitions. Absence resets that
sequence, self charts do not synthesize prior readings, early readings remain
ambiguous, and ordinary diseases do not receive the added pattern weight. Their
air/sanguine, water/phlegmatic, fire/choleric, and earth/melancholic signatures
are consequences of physical meter bundles, not evidence that Humours cause
disease. See
[Fantastic diseases](fantastic-diseases.md).

It contains no authoritative diagnosis, recommendation, infection row,
phenotype, or raw private meter. Browser subscriptions never include the
private tables.

## Presentation contract

This document and the [Health wiki page](../shared/health.md) are the
canonical explanation of meters, Humour weights, chart limitations, and
intervention scope. The game does not expose a standalone authoritative
reference page: players currently encounter the system through the physician
notebook. Future NPC dialogue should keep period claims separate from
authoritative explanations so presentation does not blur in-world belief with
system truth.
Humour information remains textual as well as colored, keyboard focusable, and
available through pinned mouse/touch tooltips.

## Strategic exposure

Wetness and thermal strain are ordinary visible survival state, not private
disease meters. Wetness is independent from filth even though the status rail
draws the thinner filth bar inset over the thicker water bar. Thermal strain is
signed around a comfortable center: cold is negative and heat is positive.
The initial cold/hot warning bands do not reduce readiness; incapacitation
begins progressively only on entering the very cold/hot bands and reaches its
cap at the extreme. Sustained severe cold accumulates deterministic frostbite
damage on one least-protected arm or leg per threshold, with canonical event
times breaking equal-protection ties independently of elapsed-time chunking.
Damage is committed through the normal injury table. Tents stop rain and
buffet wind but supply no warmth of their own.
