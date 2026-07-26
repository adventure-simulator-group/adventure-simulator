# Organizations

Organizations replace the former universal profession record. A character may
hold any number of memberships, while `organization_presentation` records the
single organization (or none) whose identity and privileges the character is
currently asserting.

## Content

`content/organizations/*.yaml` is compiled into `adventuresim-core`. Definitions
declare stable IDs, names, chapters, recognition, admission requirements and
fees, arbitrary ordered ranks, recurring dues, activity training and rewards,
and privileges such as bearing arms, wearing armor, or licensed foraging.
Organization-level privileges are inherited at every rank; rank-level
privileges are additive.

Requirements and training targets are tagged data. No code path decides that an
organization is a guild because it teaches Smithing, or a church because it
requires a religion. Mixed requirements are intentional and supported.

`content/settlement-policies.yaml` declares settlement arms and armor
restrictions. Only the currently presented, recognized, active, dues-current
membership supplies an exemption. Ownership is unaffected: when a character
loses an exemption, prohibited equipment is unequipped.

Foraging licenses use a separate global presented-privilege evaluation. They
still require persisted presentation plus a matching active, dues-current
membership and its current rank, but do not require the current settlement to
recognize the organization. A valid persisted presentation therefore survives
travel and entry into an unrecognizing settlement. This does not weaken the
locally recognized equipment privilege rules. The three forester organizations
grant Low Game, Fish, and Plants throughout their ranks; each has a terrain-4.0
Master rank that adds High Game.

The first catalog includes migrated trade and religious bodies, denomination-
specific witch-hunter and knightly organizations, three regional forester
organizations, and a deliberately eccentric Catholic cooks test organization.
These are historically informed fictional institutions rather than claims that
each exact organization existed in every listed settlement.

The character sheet exposes the presented organization as a compact profession
picker. Its large label combines the member's rank with the service profession
where one exists (for example, `Apprentice Weaponsmith`), while the smaller
label names the organization. Crests are stable heraldic marks derived from the
organization ID and service using the locally vendored Game Icons charges, so
every catalog organization has a stable heraldic identity without
presentation-only persistence fields.

## Persistence and authority

SpacetimeDB owns membership, rank, dues, presentation, payment, promotion, and
equipment-law enforcement. Reducers verify control of the character and local
chapter presence. Joining is idempotent; the joining fee is charged once.
Crossing a paid-through boundary suspends membership and clears its
presentation. Paying at a chapter reactivates it without retroactive arrears.

Organization training and activity require a current membership and local
chapter. Their skill mix is read from the catalog, including fixed skills,
denomination-specific Religion, Bestiary and Terrain leaves, and equipped
weapon skills.

### MVP privacy and uniqueness limitation

`organization_membership` rows remain public in the MVP because strategic-web
subscribes to them to render membership and schedule management. This is not an
authorization boundary: only the effective presented organization governs
privileges or public dialogue identity. An owner-scoped projection can replace
the raw subscription later. The join reducer enforces one procedural membership
per character and organization by checking the pair before insertion; the
pre-launch schema does not yet add a composite unique index.

## Validation

Build validation rejects unknown fields and invalid or duplicate IDs,
requirements, ranks, weights, organization- and rank-level privileges,
religions, skill leaves, and
settlement policies. A canonical check against a compiled Viabundus world is
also required:

```powershell
python scripts/validate_organization_world.py --world path\to\compiled-world.json
```

The cross-world check is separate because the catalog can be compiled without
the large Viabundus dataset.
