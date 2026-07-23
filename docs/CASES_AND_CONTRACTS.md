# Cases, objectives, and contracts

World problems are represented by a private `CaseAuthority`. A case references
the corresponding private investigation case, optionally references a local
problem, and owns a typed objective expression and final resolution. It exists
independently of whether anybody offers or accepts payment for resolving it.

Objective expressions use disjunctive normal form: any alternative path may
resolve the case, while every typed leaf in that path must be satisfied.
Supported leaves cover defeat, drive-off, capture, survival, rescue, escort,
retrieval and return, locating, identification and exposure, proof and
testimony, protection, negotiation, release, exchange, and reporting. The
shared-core evaluator returns `Pending`, `Satisfied`, or `Impossible` and
retains per-leaf partial progress. An impossible path does not invalidate a
still-viable alternative.

Tactical servers never complete cases or pay rewards. Trusted battle commits
produce source-idempotent `CaseOutcomeFact` rows attributed to a case, party,
mission outcome source, and hostile group. The case evaluator rejects facts
from unrelated cases, parties, and hostile groups. A satisfied expression
records one `CaseOutcome`; if linked, the local problem receives the same
idempotent outcome.

A `Contract` is a separate private agreement. Acceptance assigns only the
contract and may disclose already-existing case information; it does not
create, delete, or resolve the case. Resolution changes an accepted contract
to `ReadyToReport`. Reporting at its issuer pays once, records `paid_at`, and
changes it to `Paid`. Withdrawing a contract leaves the underlying case and
investigation intact.

The modular investigation generator creates no contract. Its cases enter play
through tavern rumors and NPC referrals, and their linked local problems
resolve and replenish without acceptance, tracking, reporting, or payment.
Legacy direct bounties may still create contracts; their presentation is
deliberately an issuer belief, and canonical threat identity and count remain
absent from the contract schema and gateway DTO.

Assets and subjects use one versioned custody row per stable object ID. A
transition must advance exactly one version and carry a stable source ID;
repeating the same source is idempotent, while stale, skipped, or cross-case
transitions are rejected.

Cases, objective graphs, outcome facts, custody, contracts, sites, and
investigation truth are private strategic authority. The web process
subscribes to a trusted `backend_contracts` projection and combines it with
observer-specific investigation knowledge. Browsers never subscribe directly
to objective or hidden-truth tables.

Noncombat objective facts have owning-subsystem producers. Dialogue producers
revalidate the selected character, party leadership, active session revision,
persistent NPC presence, intended recipient, and observer knowledge. Locate,
identify, expose, proof, testimony, and negotiation may advance a known case
without accepting a contract; report-to-issuer additionally requires the
session-bound active contract and exact issuer. Before exposing an eligible
response, the server derives one exact case from session-relevant observer
provenance and pre-issues a private session/case/objective binding. Effects
only revalidate and consume that binding after the owning producer succeeds;
they never search the character's other known cases. Each fact source includes the dialogue
session, stable action ID, and objective ID, so retries are idempotent and
distinct actions in the same minute cannot alias. There is no public generic
fact or complete-objective reducer.

## Objective producers

There is no generic reducer for applying arbitrary objective progress.
Objective facts are emitted only by the subsystem that can validate the
corresponding world event. Strategic investigation produces retrieve and
rescue custody transitions; dialogue produces return, release, and exchange;
case-site arrival produces escort; authenticated tactical completion produces
defeat, drive-off, capture, or capture-target-killed; and strategic continuity
guards produce survive and protect only after an uninterrupted deadline.

Each producer binds the expected case, party, target, hostile group, custody
version, and stable source identity as applicable. Replays are idempotent,
cross-case or stale-custody attempts fail, and terminal destruction or death
marks only affected objective leaves impossible. Alternative branches remain
available until the objective expression itself can no longer be satisfied.

Mission creation selects one eligible unresolved hostile approach rather than
choosing by objective precedence. The current kill-based tactical server and
autoresolver select `Defeated`. Investigation actions may prepare an ambush or
establish awareness, but they cannot emit `DrivenOff` or `Captured`; those
objectives remain pending until #207 adds an authoritative tactical producer.
Every shared hostile-result commit rechecks its selected resolution.
`CaptureTargetKilled` is never a successful result, and only `Defeated` may
produce battle loot.

Recurring generated cases bind `Defeat` and `DriveOff` alternatives to the same
hostile-group/site identity. Once an observer knows and enters the exact site,
the existing #217 mission seam materializes those leaves as weighted
`MissionOutcomeCandidate` rows; generation adds no parallel combat resolver.
