# Quest generation

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
canonical event, discoverable evidence, and a playable lead.

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
**validated generated quest problem** exists. This records a markerless journal
referral; it does not accept a contract or disclose testimony. Legacy/manual
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
referral and projects the persistent NPC's safe name, physical description,
profession, and location-tab label into the journal. Referral execution
revalidates observer, canonical/public case authority, NPC, settlement,
location, catalog revision, dialogue session, and live presence. Private
referral authority records whether it came from the exact initial rumor receipt
or an exact source witness and testimony draft; every use regenerates the
manifest and revalidates that provenance and its one authored edge. Missing,
cyclic, duplicate, or unreachable route-required witness edges fail generation
validation. Secondary testimony with no authored contact action changes no
route, while the primary contact still uniquely unlocks its successors.

Only an authoritative typed exact destination output can create an exact pin.
The raw site ID remains navigation authority; player-facing journal text and
the matching map pin use the site's generated safe name.
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
