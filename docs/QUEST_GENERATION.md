# Quest generation

## Authoring catalog

Modular quest and bestiary content is authored in the strict JSON-compatible
subset of YAML under `content/quests/`. Files are read in sorted path order,
validated during the `adventuresim-core` build, embedded in the executable,
and parsed into an immutable catalog once at process startup. Production does
not read loose YAML. The SHA-256 digest of filenames and exact source bytes is
the generated-case catalog revision, so changing authored content creates a
new deterministic replay boundary.

`bestiary.yaml` owns current monster names, aliases, combat values,
loot/loadout identifier, innate resistance and padding, behavior, habitats,
descriptions, clues, ambiguity links, and preparation text.
`investigation.yaml` owns evidence portraits, inspection topics and
creation-time DC ranges, witness demographics and circumstances, descriptions,
sites, and rare bridges. `generation.yaml` owns templates and separate
plausibility/curation weights, including explicit hard zeros.

Run `cargo run -p adventuresim-core --bin questgen-check -- validate` after an
edit. Build-time embedding, process startup, and the authoring checker use the
same exhaustive validator. It rejects unknown fields, duplicate or overlong
IDs, dangling references, incomplete relation coverage, ambiguous witness
rules, invalid closed-mechanic names, malformed evidence DCs and weights, and
unsupported template graphs. Diagnostics include the source file and
structural path. Open catalog IDs are limited to 63 ASCII identifier bytes.

### Current typed adapter boundary

Threat, site, witness-demographic, circumstance, ambiguous-description,
physical-evidence, bestiary-trace, and causal-bridge identities are bounded
open string IDs. A new value using existing mechanics therefore requires YAML
only. Closed Rust enums remain only
where the engine must execute a finite mechanic: rig topology, attack style,
protection, temperament, activity period, terrain, evidence check attribute,
reliability behavior, route/action and objective operation, settlement symptom,
and encounter archetype. Adding a new value to one of those finite mechanic
vocabularies requires Rust.
`shattering_blow` is a preparation hypothesis backed by the existing physical
resistance/padding model; it does not introduce a damage multiplier.

Typed investigation lists, aliases, regional priors and loadout IDs are
embedded. Tactical materialization currently consumes only one loot item ID;
it does not support multi-item authored loadouts, ability scripts, bespoke AI
state machines, or new rig/animation topologies. Behavior currently reaches
combat through temperament, perception, stealth, morale, movement, attack,
ranged precision, encounter scale, and physically based innate resistance and
padding. These are the factual incomplete bestiary surfaces for this change.

The first catalog boundary does not yet interpret arbitrary authored graph
programs. Reliability semantics (truthful, mistaken, evasive, deceptive),
account-style behavior, route/action execution, finale/objective execution,
symptom-to-settlement-effect calculation, and incident construction remain
closed engine mechanics. Their available IDs and primary selection weights are
declared in YAML, but adding a new executable semantic requires Rust. Likewise,
template declarations select the route/objective set, cause-to-finale mapping,
consequence profile, incident interval, and incident ceiling persisted in each
generated manifest. The two supported route/objective graph shapes still have
typed Rust assemblers, and startup rejects any other shape. This prevents
content from becoming executable server code and is the principal incomplete
quest surface of this PR.

Witness demographic selection is also authored. A match rule may constrain
the NPC facts `age_band`, `sex`, `profession`, and `local_role`; empty lists are
wildcards. Age bands are `child`, `adolescent`, `adult`, and `elder`, and sex is
`female` or `male`. Profession and local-role selectors are lowercase
identifiers matched against either the complete generated NPC fact or one
whole alphanumeric token in that fact; arbitrary substrings never match.
Selectors must match the finite NPC fact vocabulary known at startup. Higher
priority wins. Exactly one selector-free fallback is required, and
equal-priority rules belonging to different demographics may not overlap under
the same matching function used at runtime. Fallback priority is ignored: the
fallback is consulted only when no non-fallback rule matches.

Generated quests are deterministic typed case manifests assembled from shared
modules rather than scripts with substituted nouns. The initial catalog has
two families:

- `RecurringDepredation` investigates repeated attacks, locates one bound
  hostile group, and resolves it through the strategic mission outcome seam.
- `DisappearanceOrLoss` uses physical and social routes to locate the same
  person, asset, or false claim, then rescues, retrieves and returns, or
  exposes it according to the canonical cause.

The family does not determine the answer. Threat, site, witness demographic,
circumstance, report description, reliability, evidence, and route are
separate variables constrained by typed relations. Descriptions reuse the
shared bestiary and deliberately fit several threats.

## Weighted constraint model

Every candidate relation has separate positive integer plausibility and
curation weights. Authoritative selection uses bounded integer arithmetic,
stable candidate IDs, and deterministic domain-separated entropy. Zero means
impossible. Low positive weights remain possible, but sufficiently unusual
combinations must name a typed causal bridge. A selected bridge materializes a
canonical event, discoverable evidence, and a playable lead. Each bridge
authors the existing action that emits its evidence separately for both
supported template families; startup rejects missing families or action names
that their typed assembler does not emit. A selected bridge from any
generation relation is carried through to this materialization step. Evidence
relations are the one explicit exception: startup rejects bridges there
because the follow-up evidence selector has no case-graph materialization
context.

The solver uses deterministic weighted candidate order, forward rejection,
and backtracking under a hard node budget. Its private trace records factors,
hard-zero reasons, required bridges, forward rejections, accepted choices, and
backtracks. Static typed catalogs are intentional: do not add runtime closures,
floating-point authoritative draws, duplicated inverse tables, or unbounded
dynamic rules.

## Manifest invariants

`adventuresim-core::quest_generation::GeneratedCase` contains canonical and
public identities, cause and events, consequences, sites and areas, persistent
NPC witness bindings, proposition testimony, evidence, typed investigation
actions and outputs, DNF objectives, custody, hostile groups, finales, dialogue
producers, bridges, and the private replay trace.

Validation fails unless:

- there is exactly one true finale site;
- recurring cases begin with one exact referred-contact action, which unlocks
  inactive approach and watch routes only after that contact succeeds;
- disappearance/loss cases retain independent physical and witness roots;
- every route can disclose the same site with a typed exact output;
- targets name materialized sites, areas, or persistent NPC authority;
- unusual selected relations have their event/evidence/lead bridge;
- every selected objective leaf has a concrete owning producer;
- disappearance objectives match the canonical cause and both terminal routes
  converge on the same person or asset;
- recurring combat alternatives name the same hostile group;
- every rare bridge names the exact reachable action that emits its exact
  evidence authority.

Unsupported cause/finale combinations are not selectable. Voluntary
disappearance is excluded until locate/testimony/report producers exist, and
generated templates do not select negotiation or capture without an owner.

## Persistence and privacy

Settlement generation writes the case, local problem, investigation authority,
evidence, action graph, custody, hostiles, finales, and private
`quest_generation_authority` in one reducer transaction. The latter stores the
seed, catalog revision, input snapshot, full manifest, and trace for replay. It
has no public table accessor. Gateway projections expose only symptoms and
observer-owned knowledge; browsers never receive canonical causes, traces,
undiscovered evidence, true/decoy status, or hidden coordinates.

The initial manifest remains immutable, but an unresolved generated problem
may acquire append-only follow-up incidents as authoritative world time
advances. At the template's authored interval, settlement activity materializes the next due
incident with a stable `(case, ordinal)` identity, occurrence time, persistent
NPC witness and victim binding, circumstance, existing case site, canonical
event, and new physical-evidence authority. Delayed refreshes deterministically
catch up every missed incident. Fresh evidence is selected through the same
cause-and-site likelihood table and hard zeros as initial evidence rather than
an incident-only inverse lookup. The template's authored maximum incident
count is the current safety ceiling (five in both initial templates). Before
that ceiling can leave a neglected case
permanently stalled, an available resident NPC adventuring company may
intervene after the case has aged and accumulated incidents. Recent player
investigation or physical presence at a case site grants a grace period so the
server does not resolve a case out from under an active party.

Incident authority is private. A character who already knows the problem can
receive a dry local report when rumor circulation next reaches them; an
uninformed character receives no incident history, witness identity, evidence,
or location. Follow-up evidence remains undiscovered merely because its hidden
authority exists.

Generated physical evidence has its own observer-facing presentation rather
than speaking through an NPC. At an exact, occupied case site, each visible
object is presented as a portrait with an italic initial observation and
clickable inspection topics for particular parts of the object. A topic may be
irrelevant, or it may test eyesight, intelligence, or instinct and reveal the
object's clue.

Inspection is an authoritative fixed-threshold comparison. The generator
assigns each checked topic a hidden, deterministic difficulty when it creates
the evidence; inspection never makes a random roll. A character therefore
gets the same result on every retry unless their relevant attribute changes.
The browser receives topic IDs, labels, and observed narration, but never the
difficulty or the character's compared value. Repeated attempts are allowed,
and only the first successful discovery records the clue in the journal.

Each accumulated incident increases the unresolved problem's trade,
encounter, and disease consequences by 25 percent of their initial values,
before the existing global safety caps and mitigation are applied. At the
currently authored five-incident ceiling, consequences are twice their initial
severity. Resolving or fully mitigating the linked problem still suppresses all
of those effects.

NPC interventions are deterministic strategic outcomes; they do not create
tactical tick state. A company chooses a route supported by the case's generated
testimony and investigation-action graph, records the quoted lead, prepares for
that route, and resolves its decisive action with the ordinary investigation
mechanics. A route can fail for a concrete reason, such as an unreadable trail
or an empty ambush; the next attempt rotates to another supported route when one
exists. After a successful investigation route, a company applies the generated
objective through the same hostile, custody, outcome-fact, case, and
local-problem authority used by player results. Partial success mitigates the
settlement penalty, while failure or deferral schedules a bounded retry.
Intervention IDs, attempt numbers, party availability, and retry times make
repeated settlement refreshes idempotent. Characters who already know the
problem receive only a dry result notice in their journal.

The private authority also stores a domain-separated SHA-256 commitment to the
exact serialized generation context, including observer-ID entropy. Every
authoritative consumer validates that commitment, row identities, seed,
catalog revision, factor trace, settlement scope, and the core manifest
invariants, then deterministically regenerates the complete manifest from the
stored context and requires exact equality. Observer IDs and generated state
are derived only after this validation; malformed or stale authority fails
closed instead of becoming manual behavior.

Canonical case IDs are used by objective authority. Journals use a separate
public case ID. The reducer samples a separate 128-bit observer-ID secret and
persists it only in generation authority. SHA-256 domain-separated IDs for
actions, witnesses, propositions, capabilities, leads, and outcomes are minted
from that secret; none embeds or is reproducible from canonical or other
browser-visible identifiers. Action resolution seeds are independently sampled
and persisted when capability authority is issued or revised. Exact retries are
idempotent, while a new version receives fresh entropy and a version-separated
roll.

Every private investigation capability also persists immutable provenance:
manual, or generated with its canonical generated-case identity. Generated
capabilities never fall back to manual behavior when authority or output rows
are missing. Projection, recovery, and execution reconstruct the opaque
capability ID from the private generation context and require its method,
target, terrain, prerequisite, alternate, summary, consequence, and typed
outputs to match the immutable manifest. Canonical and public case aliases are
resolved only through private indexed authority, with collisions rejected.
Private case/objective authority carries the same explicit manual-or-generated
provenance. Dialogue eligibility and execution share one validator: only an
explicitly manual case may use its current-NPC fallback, while a generated case
requires its immutable manifest, context, objective expression, and authored
dialogue producer to remain intact.

## Discovery and resolution

Entering an inn guarantees symptom discovery when an available unknown
**validated generated quest problem** exists. This records the rumor in the
player's dry journal and grants a private referral authority; it does not
accept a contract or disclose testimony. Legacy/manual
seeded `LocalProblemAuthority` rows currently drive settlement simulation
modifiers and effects only and are intentionally non-discoverable; retiring
that producer is separate work. Any local can repeat a validated generated
rumor and name the referred persistent NPC by visible description, profession,
and expected location tab. Testimony is issued only when the addressed NPC is the bound
witness. Corrections reuse the proposition they revise, preserving an earlier
false believed pin until the correction is learned.

Witness discovery is an explicit authored graph, not permission inferred from
private manifest membership. The initial rumor grants an observer-bound
referral to the primary witness. Individual testimony drafts may refer to exact
subsequent witnesses; only processing that account grants the next private
referral. The journal intentionally does not project that referral, its
location, or any suggested next action; those details remain in the dialogue
the player actually heard. Referral execution
revalidates observer, canonical/public case authority, NPC, settlement,
location, catalog revision, dialogue session, and live presence. Private
referral authority records whether it came from the exact initial rumor receipt
or an exact source witness and testimony draft; every use regenerates the
manifest and revalidates that provenance and its one authored edge. Missing,
cyclic, duplicate, or unreachable route-required witness edges fail generation
validation. Secondary testimony with no authored contact action changes no
route, while the primary contact still uniquely unlocks its successors.

Only an authoritative typed exact destination output can create an exact pin.
The raw site ID remains navigation authority; the matching map pin uses the
site's generated safe name. Witness-described sites use a neutral attribution
such as `Place Anna Weber described`; labels never comment on whether the
account is plausible or confirmed. The journal does not interpret or restate
the pin as a destination.
Discovery actions execute at a known contact, settlement, area, or predecessor
route and may reveal travel-capable knowledge. They never also resolve custody.
After travel establishes authoritative occupancy, a separate `InspectSite` or
`LayAmbush` action may resolve custody or prepare the finale. Retrieve and
rescue outputs use the versioned custody producer. Return and
expose use compiled generic dialogue responses, pre-issued bindings, the exact
generated recipient, and one-use consumption. Cases and linked local problems
may resolve without a contract.

Every generated investigation route makes bounded persistent progress without
replacing its native skill check. This includes physical searches and tracking,
contact-finding and approach actions, observation, patrol, and ambush routes.
Each attempt retains its ordinary skill-based chance, time, supplies, fatigue,
and risk, while contiguous failures on the exact same observer capability raise
the deterministic threshold until attempt six is guaranteed. Capability,
observer, method, or version gaps reset that progress; manual investigation
actions remain unbounded. Attempt authority and journal wording snapshot the
threshold, uncertainty, and six-attempt bound at resolution time.
When exact destination knowledge is corrected, dependent progress is matched
through validated private canonical/public case aliases. Unsupported routes
receive a fresh version and seed; an exact replacement that still supports the
same route preserves its contiguous work.

Attack patterns are playable modules rather than private flavor. Initial
surveillance remains pattern-neutral. Success earns one exact corroborated
pattern proposition; only then does the dependent action disclose and enforce
its nighttime window, roadside route, authored victim profile, or broad
schedule-free search. Unreliable testimony may contradict that proposition
until the evidence is learned, and the dependent capability requires knowledge
of the exact evidence ID and successful completion of its authored predecessor,
rather than inferring from the canonical event. Projection, failed-route
recovery, and execution all apply those same live-support requirements.
Victim-specific patterns bind an opaque cohort reference to one persistent
settlement NPC in private authority, including their authored demographic and a
versioned presence fingerprint. The learned clue exposes only legitimate
demographic, physical, and referral details; patrol and ambush execution
revalidate the NPC's identity, profile, location, and availability immediately
before resolving the action.

Generated cases create no `Contract` rows. Tavern discovery and NPC referrals
are their entry points. Settlement activity counts open generated cases
directly and immediately replenishes resolved generated problems independently
of contract acceptance, tracking, reporting, or payment.

Recurring finales use the existing strategic mission authority from #217.
Pending `Defeat` and `DriveOff` leaves for the generated hostile group become
weighted `MissionOutcomeCandidate` rows after exact observer-authorized site
entry. Investigation never fabricates a battle result and no tactical tick
state is persisted.

An observer can open a generated case site without a contract only after the
observer-owned exact pin and authoritative party occupancy agree on that site.
The location page uses the validated public problem summary and site label; it
does not synthesize a contract or expose the private manifest. Available
site-bound investigation actions continue through the normal authorized
investigation reducer. Strategic autoresolve is offered only when the
validated generated finale, a pending objective path, and an active hostile
group all bind the exact occupied site. Battle results are joined back to the
page by typed case-site authority rather than canonical/public case aliases.
The observer-safe case-site projection also exposes whether the generated case
has resolved, including noncombat finales that produce no battle result. A
resolved site shows an explicit completion notice and no longer offers
pre-finale rest controls.
Manual bounty pages retain their accepted-contract and active-contract gates.

## Developer tools

```text
cargo run -p adventuresim-core --bin questgen-check -- validate
cargo run -p adventuresim-core --bin questgen-check -- explain 42 0
cargo run -p adventuresim-core --bin questgen-check -- audit 1000
cargo run -p adventuresim-core --bin questgen-check -- counterfactual 42 43
```

`validate` exercises both initial template ordinals, `explain` prints the
private manifest for development, `audit` reports family marginals, and
`counterfactual` compares stable high-level selections. Candidate domains,
persistent-witness input bytes, visited nodes, trace records, and trace bytes
all have production bounds before ordering, cloning, or serialization.
