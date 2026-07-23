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
`backend_investigation_journal` and `backend_investigation_leads` are public
views. They fail closed for identities other than the registered strategic
gateway and omit hidden threats, causes, sincerity, undiscovered evidence,
private NPC identifiers, weights, bridges, and hidden coordinates. The trusted
SSR gateway additionally filters both views to the selected session character.

The local-problem integration consumes a character-owned private rumor receipt
and derives an observer-facing case ID from the public problem ID. It never
projects `opaque_case_ref`, consults the problem cause, or silently grants a
contact's current location.

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

Available-quest, quest-giver/service, issuer-route, and turn-in exclamation
markers are absent. The legacy accepted quest flow still reveals an exact
destination, so its active destination marker remains. Recruitment indicators
are a separate social feature.

## Threat inference

Player-facing ranking accepts only received descriptions, discovered evidence,
and visible regional priors. It delegates forward likelihoods and zero/rare
semantics to `bestiary::rank_candidates_in_region`; it has no inverse table and
cannot accept hidden `ThreatId` or case truth. Provenance names only inputs
already known to the observer.
