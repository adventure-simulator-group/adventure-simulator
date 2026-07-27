# Quest generation and investigation

This page is the canonical technical reference for authored quest content, deterministic case generation, observer-specific investigation knowledge, evidence, and discovery.

## Source-aware ambiguity

Generated testimony presentation is not a reliability oracle. Account wording
families are selected independently from hidden reliability, generated location
claims share the same route-segment grant shape, and journal confidence
describes provenance rather than sincerity. The same visible wording, source
shape, confidence band, and destination class must remain compatible with
truthful, mistaken, partial, evasive, and deceptive private states.

Hearing quest testimony automatically produces a fallible passive Insight
pressure cue. A cue may be present for a quest-bound concern or for a benign
concern, and may be absent in either case. It never declares a lie, changes a
proposition, or grants navigation.
Every primary witness volunteers the same reliability-neutral public pattern
account. A separate private pattern detail may or may not exist, sampled
independently from reliability; its presence never changes the initial
dialogue's text, cardinality, order, or source. Confrontation approaches become
available only after that observer has a possible-pressure cue or relevant
contradiction/evidence knowledge. Once bound testimony is released, later
Insight checks treat the witness as benign.

Persistent NPC personality and starting morale are sampled once from
server-private entropy and stored; public NPC IDs do not determine them. Named
morale-event rows preserve why morale changes. Current settled morale,
relationship affinity, familiarity, personality, and the chosen approach all
affect resolution, while browser projections remain qualitative.
Ordinary timed chat is available from the normal social menu rather than the
quest transcript. It may improve or strain morale and affinity and always
builds familiarity, but it never diagnoses pressure or releases testimony.
The passive Insight impression and quest-specific Listen, Reassure, Invoke
duty, and Bluff approaches remain available only in an active dialogue session
after their observer-specific prerequisites are met.

Field action skills are deliberately narrow. `FollowTracks` and
`ReacquireTracks` use the matching Terrain skill; other investigation actions
use observation, with ambushes combining observation and Stealth. Until mixed
terrain exists, Forest maps to Forest, Hills and Underground to Hills,
Settlement and Ruins to Urban, and Plains, Road, and Marsh to Plains.

Generated physical trails are immutable private manifest authority. A
`TrackTrail` owns an ordered chain of observer-scoped `TrackSegment` IDs; every
segment records its ordinal, explicit terrain, safe finding, and adjacent
predecessor/next links. Exactly one physical tracking action owns each segment.
Completing an early segment records only its safe finding and route-segment
knowledge, then activates the adjacent segment. Only the final segment may
produce an exact true-site destination. Inactive segment capabilities are not
projected, so the browser cannot infer the remaining segment count, destination,
fixed difficulty, or canonical cause. Attempt progress remains local to the
opaque segment capability.

## Developer quest editor

Settlement pages expose a complete quest-authoring dialog when the existing
browser-local developer mode is enabled. The top-right book button loads the
startup-compiled catalog, closed engine mechanics, and the current settlement's
persistent navigable NPCs. Its typed repeaters cover the template and canonical
cause, consequences and incident cadence, sites and areas, witnesses and
testimony/referrals, physical evidence and deterministic inspection topics,
routes and action outputs, DNF objectives/custody/hostiles, finales, dialogue
producers, canonical events, and causal bridges.

Submission derives the settlement from the session-selected character's
authoritative current location; the
browser never supplies a settlement ID. Structural diagnostics (references,
bounds, navigable NPCs, and generated-case invariants) always block creation.
Catalog compatibility/curation diagnostics block by default, but the explicitly
labeled developer override can suppress only that second tier. Invalid input is
transactional and writes nothing.

Debug and automatic generation call the same strategic materializer.
`DeveloperGenerationContext` persists the complete definition, compatibility
override, private observer entropy, ordinal, current witness candidates, and
catalog revision in private authority. Authority validation compiles that
context again and requires an exact manifest match.

Author-local IDs are validated before compilation, then every internal case,
site, area, event, proposition, witness, evidence, action, objective, custody
object, hostile group, finale, and bridge reference is deterministically
rewritten beneath the newly minted case scope. Persistent settlement NPC IDs
and catalog IDs are never rewritten. This permits the same definition to be
spawned repeatedly without colliding in globally keyed authority tables.
Starting custody is authored as exact object/site tuples; its asset-or-subject
kind is derived unambiguously from objective leaves rather than from the finale.

Creating a debug quest inserts only latent open world authority. It deliberately
does not grant a rumor receipt, referral, journal entry, destination pin, or any
observer knowledge. Players discover it through the same tavern/NPC rumor path
as automatically generated trouble.

> **Security limitation:** developer mode is UI hiding only. It is off by
> default and stored in the browser, but there is currently no developer
> credential or reducer authorization. A caller able to reach strategic-web or
> invoke `spawn_developer_quest` directly can use the tool. Add server-side
> authorization before exposing it to an untrusted deployment.

The editor is complete for the declarative surfaces represented by
`GeneratedCase`. Closed engine mechanics remain Rust-owned and are exposed as
schema options. Tactical encounter execution and tactical enemy authoring are
outside this editor; hostile identity, count, and site are configurable while
the existing tactical/autoresolve systems consume them.

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

Quest files may also declare `dialogue_variants`: bounded inert templates for
generated `referral` or `testimony` prose. Variants reuse dialogue's typed
condition tree and highest-priority selection semantics. They are presentation
only: no variant can change canonical facts, reliability, recipient, route,
or eligibility. Templates are rendered only from server-supplied values. The
quest compiler records the exact template scalar source span, so generated
referrals and testimony use the existing developer-mode edit link to the
selected quest YAML. Contract and finale exchanges remain ordinary compiled
dialogue content; use `content/dialogue/` for those surfaces.

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

---

## Investigations

Investigations are observer-specific knowledge, not a projection of canonical
quest truth. `adventuresim-core::investigation` models atomic propositions
through distinct perception, recollection, disclosure, transmission, belief,
revision, evidence, and lead stages. A witness can be sincerely mistaken,
omit one proposition, distort another, or later correct an account; there is no
witness-wide `is_lying` flag.

### Privacy boundary

The SpacetimeDB module keeps case truth, canonical events, stage records,
evidence authority, generation explanations, beliefs, revisions, leads, action
receipts, and sharing receipts in private tables. Only
`backend_investigation_journal`, `backend_investigation_leads`, and
`backend_case_site_pins` are public views. They fail closed for identities
other than the registered strategic gateway and omit hidden threats, causes,
sincerity, undiscovered evidence, private NPC identifiers, weights, bridges,
and hidden coordinates. The trusted SSR gateway additionally filters every
view to the selected session character. A case-site pin is joined to private
physical authority only when that observer has an unrevised exact-believed or
visited lead; textual and approximate leads cannot reveal coordinates.

The local-problem integration consumes a character-owned private rumor receipt
and derives an observer-facing case ID from the public problem ID. It never
projects `opaque_case_ref`, consults the problem cause, or silently grants a
contact's current location.

Generated cases privately map canonical, investigation, and public journal
identities. Dialogue eligibility accepts only those exact aliases plus
session-relevant rumor, testimony, belief, evidence, or custody provenance; it
never searches arbitrary cases the character happens to know.

Runtime testimony generation is a private production pipeline. Server-authored
perception, memory, disclosure, and transmission stages are persisted as a
private bundle, then issue a private, one-use, character-owned safe receipt.
Receiving that receipt records exact observer-owned provenance for the claim
and witness. The browser never authors or receives the hidden pipeline payload.
Player actions can only consume an existing matching receipt; they cannot
submit statement text, confidence, sources, or coordinates.

Compiled testimony patterns operate per proposition: an account may be
truthful, mistaken, evasive, deceptive, or partly truthful, including an
accurate event description paired with an omitted reason for being present.
Follow-up eligibility uses only observer-visible claims, contradictions,
familiarity, language/social checks, prior questioning, and possessed
evidence. The reliability pattern, motive, and canonical event remain private.

Evidence authority explicitly classifies presentation as physical or
informational. Physical evidence requires a custody row currently held by the
presenting party or character; a missing row fails closed. Informational
evidence requires a private, source-attributed
`investigation_evidence_knowledge` receipt. The mere existence of hidden
evidence authority—or a belief concerning the same proposition—does not grant
knowledge or physical possession of proof.

#### Physical inspection and Bestiary knowledge

Physical evidence exposes only safe topic IDs and labels. Its base attribute
thresholds, Bestiary thresholds, hidden case truth, and failed Bestiary checks
remain private. Once the physical inspection succeeds, the reducer evaluates
each authored atomic category implication against the inspecting character's
effective Bestiary knowledge. It never substitutes the party's best expert and
never consults the hidden threat.

Successful checks persist only observer-owned diagnostic-kind receipts and
safe interpretations. A failed physical observation creates no diagnostic
receipt. Generated testimony similarly creates an observer-owned report
receipt only after the private manifest is revalidated and the testimony is
actually received. Neither receipt can be supplied by the browser.

Whenever either input changes, the server calls the shared `infer_threats` and
`qualitative_deductions` functions using only that observer's received reports,
learned diagnostics, and the public regional context. It persists a deduplicated
set of possible monster kinds with `strong`, `plausible`, or `weak` support and
safe provenance. The journal and evidence conversation show those qualitative
candidates and their sources. Raw scores, basis points, percentages, hidden
thresholds, failed checks, canonical cause, and hidden evidence IDs never cross
the gateway. Identical visible report and diagnostic inputs therefore produce
identical deductions regardless of the hidden cause.

The SpacetimeDB module currently sets `test = false`, so ordinary Cargo tests
cannot instantiate a reducer database harness. Narrow pure tests cover action
receipt scope, canonical-row augmentation, duplicate suppression, stable
physical failure, success-only category filtering, and observer-safe
serialization. End-to-end reducer transaction behavior remains a live
database verification responsibility.

### Sharing and navigation

Knowledge belongs to a character. Sharing a selected lead or belief is an
explicit, idempotent action. The recipient must be living, in the same party,
and at the same strategic location at that moment. Joining or rejoining grants
no historical knowledge.

Destination knowledge advances from unknown through textual directions or a
landmark, an approximate area or route segment, exact believed location, and
observer-specific visited location. Only the last two stages may carry a pin.
An incorrect exact account remains the observer's destination until a sourced
correction revises it.

Generated corrections reuse the proposition ID they revise. A later witness or
evidence receipt creates the revision and marks the earlier lead corrected;
private materialization alone does not remove the observer's false pin.

Strategic destinations have stable case-site IDs independent of quests and
contracts. Character and party location, journey endpoints, camp continuation,
terrain planning, map links, and tactical scene selection all use the case-site
ID. A site's `case_id` may currently point to a legacy direct bounty, but
`Quest` contains no location, scene, coordinate, distance, or tracking fields.

Tracking is a private per-party presentation choice over an exact site already
known by the leader. It does not disclose the site, accept or abandon a
contract, move the party, satisfy an objective, start combat, or grant a
reward. Travel independently revalidates the leader's observer-safe exact
knowledge on every attempt and retry. Direct bounties seed a private site and
explicitly disclose it when the issuer accepts the contract, preserving the
prototype flow without making quest state the location authority.

Available-quest, quest-giver/service, issuer-route, and turn-in exclamation
markers are absent. Exact case-site pins are knowledge projections, not quest
markers. Reported exact locations and visited sites are labeled separately;
an active-contract badge appears only when the party's active contract
explicitly matches that case. Recruitment indicators are a separate social
feature.

### Threat inference

Player-facing ranking accepts only received descriptions, discovered evidence,
and visible regional priors. It delegates forward likelihoods and zero/rare
semantics to `bestiary::rank_candidates_in_region`; it has no inverse table and
cannot accept hidden `ThreatId` or case truth. Provenance names only inputs
already known to the observer.

### Strategic investigation actions

Investigation opportunities are private, versioned capabilities issued by the
strategic authority. The browser receives only an opaque action ID, method,
version, safe description, prerequisites, costs, uncertainty, and contribution
labels. Hidden case truth, target IDs, exact coordinates, deterministic seeds,
success thresholds, and weights never enter browser state.

The initial action vocabulary is inspect site, search area, follow or reacquire
tracks, locate a contact, watch, patrol, lay an ambush, and approach a lead.
Rumors materialize all nine as two linked routes: witness-led search and
observation-led interception. A recurring case begins with the exact referred
contact; succeeding there unlocks inactive approach and watch branches.
Disappearance/loss cases instead retain independent physical and witness roots.
Successful actions unlock their successors, and failed actions reactivate a
validated same-owner, same-case alternate. Failure text reports whether any
other currently live-supported case route remains, including a patrol already
supported by its exact clue. Approximate areas are private strategic geometry,
not client-authored destinations.
Resolution uses authoritative terrain, time of day, evidence age, relevant
skills, bounded party assistance, and observer familiarity. Weather is
explicitly unavailable until the strategic layer owns authoritative weather;
clients must not infer or invent it.

Every attempt is idempotent and consumes strenuous strategic time, including a
failed attempt. Failure may increase risk or uncertainty, but it does not
silently invalidate alternate investigation routes. Approximate discoveries
remain directions or areas. An exact map pin is disclosed only when an
authoritative result supports exact observer knowledge. Journal and map-pin
text use the site's safe generated name; opaque site IDs remain navigation
authority and are not shown as destination labels. Watches, patrols, and
ambush preparation remain strategic actions; they do not persist tactical tick
state and cannot fabricate a combat result.
Before spending time the reducer revalidates party readiness, co-location,
journey and camp state, unresolved encounters, predecessor knowledge,
position, and typed prerequisites. Party clocks synchronize first, with night
defined as before 06:00 or from 20:00 onward. Browser estimates are broad
method-derived duration ranges; exact terrain, needs, fatigue, success, and
risk remain authoritative.

Location is revalidated at execution, not merely at issuance. Contact actions
use the referred NPC's current settlement and presence window (or the same
settlement as a bound ask-around action). Track actions remain bound to the
materialized predecessor area until they disclose the site; occupying another
site from the same case counts only when its valid coordinates fall
within that area's meter radius. Areas bind the origin settlement's coordinate
mode: imported geographic worlds use great-circle meters, while abstract maps
use the strategic-travel convention of Euclidean coordinate units as
kilometers. Site and area modes must agree. Later
site-targeting actions require actual site occupancy. Traveling elsewhere
invalidates the attempt before time is spent or a lead is written.

Retrieve and rescue consequences re-read current custody and require the case objective,
object kind, site holder, occupied site, and next version to agree. A purely
stale version reissues the capability without spending time; a holder, site,
or case mismatch fails closed. Investigation can discover, track, position, and
prepare an ambush, but it never creates a mission, battle receipt, hostile
disposition, drive-off fact, or capture fact. Authoritative non-kill tactical
resolution begins only after authenticated combat succeeds. Strategic mission
authority privately snapshots exact observer-authorized pending objective
approaches and deterministically selects among compatible defeat, drive-off,
and capture consequences at commit time. Investigation actions cannot select
or invoke that result.
