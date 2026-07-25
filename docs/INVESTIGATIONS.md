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

Runtime testimony generation persists the full proposition pipeline through a
gateway-authorized authority staging seam, then issues a private, one-use,
character-owned safe receipt. The browser never authors or receives the hidden
pipeline payload. Player actions can only consume an existing matching receipt;
they cannot submit statement text, confidence, sources, or coordinates.

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
markers. Recruitment indicators are a separate social feature.

## Threat inference

Player-facing ranking accepts only received descriptions, discovered evidence,
and visible regional priors. It delegates forward likelihoods and zero/rare
semantics to `bestiary::rank_candidates_in_region`; it has no inverse table and
cannot accept hidden `ThreatId` or case truth. Provenance names only inputs
already known to the observer.
