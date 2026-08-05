# Observer knowledge provenance

`adventuresim_core::knowledge` provides pure typed envelopes for information an
observer has learned. It is not a universal knowledge graph, inference engine,
rumor network, or disclosure authority, and it has no schema, reducer, generated
binding, client transport, or free-form proposition format.

An envelope always names a stable record, exact observer, typed subject and
domain proposition, typed source, source and learning minutes, bounded private
confidence, visibility, and checked revision lineage. Construction requires the source
to precede learning and learning to be no later than the observer's personal
time. Missing provenance therefore cannot produce an observer record.

`AuthoritativeTruth` is a separate private type. There is deliberately no
conversion from canonical truth to observer belief or public presentation.
Observer records retain their domain-owned belief payload even when it is false,
incomplete, or contradicted. The core projection exposes only learning minute, a
bounded Weak/Plausible/Strong confidence band, and an adapter-authored
observer-safe presentation; it never copies source, subject, proposition, belief,
rolls, disease identity, or canonical truth. Only the exact observer also receives
an observer-owned record/revision reference for later dialogue or referral.

Private and shareable records project directly only to their exact observer.
Projection requires an opaque grant that only trusted core server code can mint
after authenticating the viewer, personal-time frontier, and typed public
disclosure scope. Visibility is checked before chronology, so an unauthorized
viewer receives only `NotVisible`, including when a record lies beyond that
viewer's time. Public disclosures never receive observer-owned record handles.
Each later cross-crate adapter requires an intentional authenticated bridge
inside `adventuresim-core`; adapters can consume grants publicly but cannot mint
them themselves.

Shareable visibility does not make a record transferable: a checked sharing
receipt binds the exact source record/revision, typed subject and proposition,
sharer, recipient, rule, and chronology. Its only recipient-construction method
creates a new envelope for that exact recipient and fact, with source minute equal
to the sharing minute and visibility fixed to observer-private. Opaque shared
provenance is revalidated by the envelope, so copying it cannot retarget another
recipient, fact, source chronology, or disclosure scope. Any later re-share or
public disclosure requires separate future reducer-owned authorization.

Revision one has no predecessor; every later revision names exactly one record.
Supersession requires the same observer, subject, and proposition, the exact
predecessor, the next consecutive revision, and non-regressing chronology. It
does not mutate the earlier record. Contradictions likewise retain two immutable
record/revision references for the same observer plus typed domain rationale;
they neither reveal truth nor silently replace either belief.

All later record, share, and contradiction reducers can use the same typed
mutation receipt. It binds a nonzero request identity to the exact typed server
input and committed typed outcome. A new request returns `Apply`; an exact replay
returns the committed outcome as a top-level no-apply decision; reusing the
request identity with different input returns `Collision`.
Multiple prior receipts for one request identity return
`AmbiguousPriorReceipt` instead of selecting an order-dependent outcome.

## Planned adapters

- Investigation observations, evidence, claims, beliefs, leads, and testimony
  will retain their domain payloads while sharing observer/source chronology.
- Insight presentation will keep private checks and sincerity separate from the
  provenance confidence band.
- Bestiary inspection/deduction and physiology notebooks will use their own
  closed subjects, propositions, belief payloads, and eligibility rules.
- Rumors, topic knowledge, referrals, and public-threat notices will use checked
  sharing or public-disclosure provenance without automatic propagation.

Adapters remain responsible for minting projection grants only after server-side
authentication, current eligibility, observer-safe presentation and attribution,
bounded response content, and transactional persistence. This foundation adds no
new deductions, dialogue mechanics, NPC decisions, or future-fact disclosure.
