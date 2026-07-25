# Investigations

Investigations are observer-specific knowledge, not a projection of canonical
quest truth. `adventuresim-core::investigation` models atomic propositions
through distinct perception, recollection, disclosure, transmission, belief,
revision, evidence, and lead stages. A witness can be sincerely mistaken,
omit one proposition, distort another, or later correct an account; there is no
witness-wide `is_lying` flag.

## Privacy boundary

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

### Physical inspection and Bestiary knowledge

Physical evidence exposes only safe topic IDs and labels. Its base attribute
thresholds, Bestiary thresholds, hidden case truth, and failed Bestiary checks
remain private. Once the physical inspection succeeds, the reducer evaluates
each authored atomic category implication against the inspecting character's
effective Bestiary knowledge. It never substitutes the party's best expert and
never consults the hidden threat.

Successful category results persist on one canonical observer-owned inspection
record: category, support basis points, and interpretation. A repeated reducer
action ID is an idempotent no-op. A later inspection keeps the original
physical pass/failure, narration, and timestamp, then rechecks current Bestiary
knowledge and monotonically adds newly successful categories. It never removes
knowledge or creates another conversation row. A failed physical observation
can never gain Bestiary results.

The evidence conversation labels these results `Bestiary check(s) succeeded:`.
Support colors interpolate continuously from pure red at 0%, through pure
yellow at 50%, to pure green at 100%. Text labels and percentages accompany
the color. The same structured successes are copied into the single durable
investigation-journal notice and remain visible after leaving the evidence
site. Each result is keyboard focusable. Pointer hover and keyboard focus use
the document-level strategic tooltip layer, with viewport collision handling
and `aria-describedby`, to list the current enemy types covered by that
category. Clicking a result pins the tooltip. Hovering or focusing an enemy
type within it reveals only strengths and weaknesses derived from mechanics
consumed by current combat; category-level generalizations and unimplemented
folklore are omitted.

The SpacetimeDB module currently sets `test = false`, so ordinary Cargo tests
cannot instantiate a reducer database harness. Narrow pure tests cover action
receipt scope, canonical-row augmentation, duplicate suppression, stable
physical failure, success-only category filtering, and observer-safe
serialization. End-to-end reducer transaction behavior remains a live
database verification responsibility.

## Sharing and navigation

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

## Threat inference

Player-facing ranking accepts only received descriptions, discovered evidence,
and visible regional priors. It delegates forward likelihoods and zero/rare
semantics to `bestiary::rank_candidates_in_region`; it has no inverse table and
cannot accept hidden `ThreatId` or case truth. Provenance names only inputs
already known to the observer.

## Strategic investigation actions

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
