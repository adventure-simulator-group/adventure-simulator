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
session-bound active contract and exact issuer. Each fact source includes the
dialogue session, stable action ID, and objective ID, so retries are
idempotent and distinct actions in the same minute cannot alias. There is no
public generic fact or complete-objective reducer.
