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

Private `mission_authority` rows bind a mission to its party, optional exact
case site, optional hostile group, and scene. Private
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
stored in public rows or command-line arguments.

Tactical servers keep positions, health, enemies, and per-tick simulation
transient. On victory, the owning server submits its bound mission through the
same strategic commit used by autoresolve. The commit validates party,
mission, and hostile-group attribution and inserts one private
`outcome_source_authority` receipt before writing the public battle result,
participants, and loot. Tactical drops come from the immutable hostile-group
manifest, not temporary enemy equipment. Replaying the same source is a no-op, so it cannot
duplicate facts, morale, loot, or reward shares.

Defeat and stalemate retain only the bounded autoresolve diagnostic report and
condition consequences; they do not create a strategic victory outcome or
defeat a hostile group. A new attempt requires a new mission ID.

Legacy bounty completion is currently a downstream projection from a newly
defeated bound group to its case. It is not an input or fallback for mission,
battle, outcome, or loot authority. The generalized case/objective work
replaces that final projection.
