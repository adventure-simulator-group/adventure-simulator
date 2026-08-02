# Quest authority

Generated outbreaks add private outbreak and patient authority beneath this
boundary. Public projections never expose disease, source, culpability,
chronology, carrier, or exact remediation. A remediation fact is accepted only
when its source and ID match that authority. See
[Outbreak investigations](outbreaks.md).

This page is the canonical technical reference for durable cases, objectives, contracts, local problems, mission and battle outcomes, NPC recruitment, and strategic incidents. These systems may interact, but none uses legacy quest identity as its authority boundary.

## Cases, objectives, and contracts

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

### Objective producers

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

---

## Local problems

Local problems are persistent strategic conditions which affect settlements
and roads before a character knows their cause. They are independent of legacy
quests, contracts, rewards, and tactical tick state.

### Authority and privacy

`adventuresim-core::local_problem` owns deterministic weighted generation,
absolute-time lifecycle evaluation, stable aggregation, caps, and checked price
adjustment. Relations have separate plausibility and curation weights. Zero is
impossible; rare relations may require a causal bridge, whose key is emitted for
later evidence authoring.

Generated quests use `adventuresim-core::quest_generation` to select canonical
truth and atomically link the case to this symptom authority. The local problem
stores the consequence mechanism, not a parallel monster answer. See
[Quest generation and investigation](quest-generation-and-investigation.md).

Cause, disease identity, encounter archetype, opaque case reference, weights,
bridges, generation entropy, explanations, and per-problem consequences are
private. Public rows contain only observable symptoms. The authenticated
strategic gateway receives character-scoped aggregate trade pressure and
private rumor deliveries through gateway-filtered views; browsers cannot
subscribe to either authority rows or per-problem consequence fingerprints.

### Lifecycle and consequences

A problem has an absolute interval, monotonic mitigation, and an optional
earliest resolution minute. Generation uses private entropy, and expired or
resolved history does not prevent a later replacement. At most three active
rows contribute to a scope, in stable ID order, with aggregate caps. Resolution
also closes the public symptom interval.

- Trade pressure is applied after base and language pricing by the same checked
  integer basis-point function used for UI quotes and reducer settlement. Food
  lots are included; merchant catalog stock remains infinite.
- Route problems use a canonical sorted endpoint pair and affect only existing
  canonical encounter boundaries. Entropy domains, retries, chunking, and
  impossible habitat weights are preserved.
- Disease pressure reuses `first_eligible_presence_exposure_minute` with the
  stable problem ID as exposure source. Disease identity remains private until
  diagnosed.

The internal outcome seam accepts an idempotent source ID, authoritative minute,
monotonic mitigation, or resolution. It is not a public reducer and cannot
accept contracts, complete objectives, or pay rewards.

### Markerless discovery

Dialogue with an available inn NPC surfaces one unknown unresolved problem.
Overview dialogue does so only where no inn NPC is available. Safe rumors give
the symptom and refer to a persistent NPC by name, visible description,
occupation, and expected location tab. A private per-character receipt records
the source and opaque case reference. Rumor text is delivered through a private
session-scoped row and is merged into the authenticated SSR response; it is
never written to the public dialogue-event table.

Later conversations with ordinary locals give a short summary and referral
instead of replaying discovery. Discovery does not accept or mutate a quest,
reveal a cause or destination, or create a map marker.

The first inn conversation records discovery only. Even if the innkeeper is
also a generated witness, it does not deliver testimony in that session. The
character must later address the bound referred NPC; selecting another local
cannot reveal that witness's account.

---

## Mission and battle authority

Combat authority is independent from contracts and legacy quest identity.

- `MissionId` identifies one requested tactical or autoresolved combat
  opportunity.
- `HostileGroupId` identifies the particular persistent group occupying a case
  site. A random encounter is explicitly unbound; matching its species and
  headcount cannot defeat a case site's group.
- `BattleId` identifies one finished combat attempt.
- `OutcomeSourceId` is the authenticated idempotency key for the strategic
  consequences of a victorious battle.

Private `mission_authority` rows bind a mission to its party, observer, exact
case site, case, hostile group, and scene. Case missions snapshot a private,
immutable set of exact `mission_outcome_candidate` rows derived from
observer-authorized `mission_approach_capability` rows. Each candidate names a
pending path and objective, compatible resolution, weight, and, for capture,
the exact subject and custody version. Capabilities require exact believed or
visited site knowledge; their IDs and weights have no public projection. Private
`hostile_group_authority` rows are materialized when a case site is created
and own enemy composition, immutable drop manifest, and defeated state.
Mission creation reads only these authorities, never a quest. Public tactical
views contain party-safe presentation data and never expose the case-site or
hostile-group binding.

The authenticated dispatcher uses the registered strategic-gateway identity.
For each request it generates a 256-bit claim, stores only its SHA-256 digest
through a gateway-only reducer, and waits for reducer success before spawning.
The raw claim is passed only in the tactical child's environment; the full
gateway token is explicitly removed. The child consumes the matching private
claim exactly once when registering its server identity. Claims are never
stored in public rows or command-line arguments. If process creation fails,
the dispatcher revokes the still-pending claim so the request can be retried.
A child that exits after process creation but before registration still
requires cancellation or dispatcher restart; durable child supervision is a
follow-up operational improvement.

Tactical servers keep positions, health, enemies, and per-tick simulation
transient. Their completion enum is only a compatibility transport: `Failed`
means failure, `CaptureTargetKilled` is explicit contradictory terminal
evidence that also fails without sampling, and the other values are the same
opaque authenticated success signal. Tactical requests and servers contain no strategic approach,
objective, subject, weight, or expected-result field. On success, strategic
authority revalidates the prebound candidates, canonically sorts them, and
performs a deterministic SHA-256-derived weighted draw from private
server-generated mission entropy. Caller-selected mission IDs therefore cannot
grind outcomes, while retries reuse the persisted entropy and select the same
result. Stale capture custody removes that
candidate; if none remains, the attempt fails without fabrication. Allied
autoresolve victory uses the same sampler.

The strategic commit validates party, mission, site, hostile-group, objective,
candidate, and capture custody attribution and inserts one private
`outcome_source_authority` receipt before writing the public battle result,
participants, and loot. Tactical drops come from the immutable hostile-group
manifest, not temporary enemy equipment. Only `Defeated` can mint group drops
or random gold. `DrivenOff` emits its typed fact without loot. `Captured`
atomically transfers the exact subject from the bound site and custody version
to the party and emits `SubjectCaptured`, also without loot. Success revokes
sibling approaches for that group and site. Replaying a source is a no-op, so
it cannot duplicate facts, morale, custody, loot, or reward shares.

Failed, cancelled, stale, defeated, and stalemated attempts are terminal under
their mission ID. Defeat and stalemate retain only the bounded autoresolve
diagnostic report and condition consequences; they do not create a strategic
victory outcome or resolve a hostile group. A retry requires a new mission ID.
Pending or active sessions for the same group remain mutually exclusive.
Random encounters have no case manifest and remain defeat-only.

Legacy bounty completion is currently a downstream projection from a newly
defeated bound group to its case. It is not an input or fallback for mission,
battle, outcome, or loot authority. The generalized case/objective work
replaces that final projection.

---

## Recruitment and incident authority

Recruitment and strategic incidents are independent world systems. Neither is
a contract, objective, or legacy `Quest`.

### NPC recruitment

An NPC company is advertised by a stable `RecruitmentOfferId`. Its
`RecruitmentSourceId` is the deduplication identity for the settlement/company
source. The offer owns the recruiting party, leader, settlement, creation and
expiry minutes, and an `Open`, `Closed`, or `Expired` lifecycle.

Settlement activity population creates recruiting companies and their ordinary
party recruitment roles without accepting or creating a quest. Requests and
acceptance revalidate the open offer, expiry, leader, role capacity, and
co-location inside one reducer transaction. Repeating the same pending request
is a successful no-op. Player-authored general recruitment roles remain
independent and need no NPC offer.

Each generated company is anchored to a persistent settlement NPC and that
NPC's scheduled observable presence. The offer source derives from the stable
NPC identity; stale party, leader, settlement, or presence bindings close the
offer. The service API returns recruitment companies separately from quest
contracts, so an inn can surface a company when no quest posting exists.

The public offer contains only social presentation identity. It has no
investigation case, witness evidence, hidden cause, or threat data.

### Strategic incidents

`IncidentId`, `IncidentSourceId`, `IncidentKind`, and `IncidentStatus` form a
private strategic authority. The source ID is the retry/deduplication key. Each
incident owns its party, instigator, settlement, case-site binding, hostile
group binding, creation time, and lifecycle.

An incident uses the normal case-site location authority and mission/hostile
group authority, but it does not create a quest or contract. Starting one moves
the party to its incident site without changing `Party.active_contract_id`.
Leaving that site marks the incident avoided. A victorious tactical or
autoresolved mission matches the exact hostile-group ID and marks it resolved
before any legacy quest projection is considered.

Departure synchronization permits retreat only when the sole pending incident
is the incident at the party's exact departing site. Incidents created at a
different location, or multiple inconsistent pending incidents, invalidate the
stale journey request. Activity incidents derive their source identity from the
persisted activity occurrence minute rather than the random occurrence roll.

Consequently, an unrelated battle or incident can never complete a quest, and
resolving or avoiding an incident cannot mutate quest/objective state.
Tactical positions, health, damage, and tick state remain transient.

Each hostile group owns one exact stable Character roster through contextual
membership. Mission binding snapshots those IDs; autoresolve and tactical
servers consume the snapshot instead of fabricating anonymous combatants.
Surrender and other nonviolent Character-system outcomes now enter cases only
through trusted typed fact adapters. See [Systemic Character interactions](systemic-character-interactions.md).
