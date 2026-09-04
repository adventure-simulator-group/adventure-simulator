# Physical objects and operational custody

Strategic physical objects have one identity: the existing auto-incremented
`InventoryObject.id`. `adventuresim_core::physical_object::PhysicalObjectId`
is a nonzero typed view of that value, not another identifier and not another
table. Identity remains stable when the object moves between a character, an
exact party, a container, or an exact strategic place or fixture.

Operational custody answers only where an object is held now. The closed core
vocabulary distinguishes:

- an exact character inventory;
- an exact party inventory;
- direct containment by another physical object;
- an exact canonical strategic place; and
- an exact canonical strategic fixture.

A fixture and the place containing it are different custody locations. A pan
over an inn fireplace is in custody of that exact fireplace fixture, not merely
the inn, settlement, character, or party. A contained item's direct custody is
its parent object; following the bounded, acyclic containment chain yields its
root character, party, place, or fixture custody.

The SpacetimeDB module keeps the current `InventoryObject` location columns and
containment table as its persistence representation. One centralized adapter
strictly converts those rows, reducer inventory scopes, containment edges, and
canonical place/fixture strings to the core vocabulary. It rejects zero object
or character IDs, non-canonical party or character custody, unknown location
kinds, missing or contradictory backing inventory rows, self-containment,
cycles, excessive nesting, conflicting carried custodians, and fixture/place
mismatches. A malformed row cannot silently become personal or current-party
custody.

Fireplace stations and dishes persist the same closed operational-custody
transport for their immutable return destination. A vessel's physical object
itself moves to exact fireplace-fixture custody while installed; retrieval
returns that same `InventoryObject.id` to the recorded character or exact
party. The caller's current party or requested selector cannot redirect it.

## Boundaries

Custody is not legal ownership, title, classification, access policy, or
permission. Those rights remain a separate authority layer and must not be
inferred from a custody variant or field name.

Likewise, physical-object identity is not measured quantity or material-lot
identity. Future measured-material work may attach amount state and a distinct
lot/batch referent to a physical object, but it must key the physical carrier by
`InventoryObject.id` rather than create another measured-object identity.
Splitting or transforming material may create new lot identity according to
its own conservation laws; moving the carrier changes only custody.

## Core laws

- A custody transfer preserves `PhysicalObjectId`.
- A physical object cannot contain itself, and the containment graph is
  bounded, acyclic, and rejects duplicate object IDs.
- Character and party custody match only the exact recorded identity.
- Place custody never aliases fixture custody.
- Custody APIs and persistence fields do not claim legal ownership.
