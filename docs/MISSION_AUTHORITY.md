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
`hostile_group_authority` rows own enemy composition and defeated state.
Tactical server requests copy only these explicit bindings and never carry a
quest ID.

Tactical servers keep positions, health, enemies, and per-tick simulation
transient. On victory, the owning server submits its bound mission through the
same strategic commit used by autoresolve. The commit validates party,
mission, and hostile-group attribution and inserts one private
`outcome_source_authority` receipt before writing the public battle result,
participants, and loot. Replaying the same source is a no-op, so it cannot
duplicate facts, morale, loot, or reward shares.

Defeat and stalemate retain only the bounded autoresolve diagnostic report and
condition consequences; they do not create a strategic victory outcome or
defeat a hostile group. A new attempt requires a new mission ID.

Legacy bounty completion is currently a downstream projection from a newly
defeated bound group to its case. It is not an input or fallback for mission,
battle, outcome, or loot authority. The generalized case/objective work
replaces that final projection.
