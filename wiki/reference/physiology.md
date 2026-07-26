# Physiology system

Physiology is the skill for observing health, administering prepared
interventions, and improving wound recovery. It produces a fallible differential
of possible diseases, but never reveals the authoritative disease identity or
recommends a treatment. Preparation crafting and chemistry remain deliberately
out of scope for issues #214 and #215.

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

All authoritative personal-time paths evaluate disease and intervention effects
together. The earliest integer-minute terminal crossing wins, with a stable
meter-order tie-break, so splitting an interval into smaller calls cannot change
the result.

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
The differential also compares the expected action of known treatment with the
patient's subsequent regional burden, so improvement or deterioration can
strengthen or weaken a candidate without exposing private disease identity.

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
