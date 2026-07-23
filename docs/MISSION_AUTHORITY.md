# Mission and battle authority

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
