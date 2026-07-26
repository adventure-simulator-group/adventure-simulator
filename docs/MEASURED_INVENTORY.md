# Measured inventory architecture

Status: accepted architecture for issue #150; production schema rollout is
deliberately not part of this change.

This document defines how fractional consumables will coexist with ordinary
inventory stacks. Its authority is the strategic SpacetimeDB layer. Tactical
code may receive a snapshot or request a strategic result, but must not create
a second durable inventory authority.

## Decision

`quantity` and measured `amount` describe different dimensions and both exist:

- `quantity` is a count of independently ownable objects or unopened units.
- `amount` is the remaining magnitude of divisible material in one object or
  lot, stored as an unsigned integer in a definition-selected fixed unit.

An item definition opts into measurement through a profile/capability. The
inventory row continues to carry `quantity`; an optional, separate measured
state row carries `remaining_amount`. A row with measured state is always
quantity one. A definition without a measurement profile can never have
measured state.

Capabilities compose. Measurement, armor, weapon, food, alcohol, durability,
and similar profiles should be separate definition tables keyed by `item_id`,
not variants in one giant exclusive `ItemKind` union. A bottled medicine may
be both alcohol and food; clothing may also be armor. An exhaustive union would
make those legitimate combinations awkward or impossible.

The framework-independent arithmetic prototype is
`adventuresim_core::inventory_measurement`. It validates profiles and evaluates
full stacks and partial or empty singletons without changing a public database
schema.

## Alternatives considered

### Discrete uses

One stack unit represents one serving, wash, charge, or meal.

- Advantages: works with the current schema and integer trade; stacking,
  transfers, and targets are simple.
- Costs: opening a bottle cannot preserve what remains; using soap wastes a
  hidden remainder; meals and drinks have arbitrary serving boundaries.

This remains appropriate for genuinely discrete things such as arrows. It is
not the general model for divisible consumables.

### Fixed-point bulk

Every row stores an integer magnitude such as millilitres or milligrams, with no
physical container identity.

- Advantages: simple consumption, aggregation, and exact conservation; useful
  for fungible bulk stores.
- Costs: cannot account for tare mass/value, bottles, opened versus sealed
  goods, or reusable vessels without another model.

Bulk lots use the selected measured-state architecture with zero tare and a
quantity-one lot. They are not a replacement for containers.

### Per-container contents

Each opened container is a quantity-one row whose measured state records its
remaining contents.

- Advantages: preserves bottle identity, tare mass/value, empty containers,
  and exact transfer/loot ownership.
- Costs: opened containers no longer stack automatically; pouring and mixing
  require explicit rules.

This is the selected representation for containerized goods. Unopened identical
containers still stack and split only when one is opened.

### Rejected: count-or-amount

A sum such as `Count(u32) | Amount(u64)` makes the two values mutually
exclusive. A half-full bottle is simultaneously one bottle and some amount of
liquid, so the model loses information or needs container-specific exceptions.
Payload sum types also make relational filtering and arithmetic less direct
than a small profile table plus state table.

### Rejected: one unified numeric field

Treating everything as amount permits nonsensical durable states such as
`0.5 swords`. Enforcing integrality from the item definition recreates the
selected capability model with a weaker schema. Floating point is additionally
unsuitable for conserved durable inventory, nutrition, mass, and value because
split/recombine results would depend on operation order.

## Future clean schema

Names are illustrative, but the relationships and invariants are normative.
The production implementation should use the repository's clean reset/reseed
workflow. The project is pre-launch: there will be no migration, legacy
columns, dual reads, compatibility shim, or preservation of current characters.

```rust
MeasurementProfile {
    item_id: String,                 // primary key; FK-equivalent to Item
    kind: MeasurementKind,          // Depletable | Containerized | BulkLot
    unit: MeasurementUnit,          // Milligram | Millilitre | ...
    capacity: u64,                  // amount when full, greater than zero
    full_contents_mass_mg: u64,
    full_contents_value_subunits: u64,
    tare_mass_mg: u64,
    tare_value_subunits: u64,
}

PersonalMeasuredState {
    inventory_item_id: u64,         // primary key, owns exactly one personal row
    remaining_amount: u64,          // inclusive range 0..=capacity
}

PartyMeasuredState {
    party_inventory_item_id: u64,   // primary key, owns exactly one party row
    remaining_amount: u64,          // inclusive range 0..=capacity
}
```

`MeasurementKind` is a payload-free discriminator if persisted. Profile
columns remain ordinary scalar columns so reducers and operational queries can
inspect them directly. Food, alcohol, and soap profiles remain separate tables
linked by `item_id`; their existence is independently validated against the
measurement profile.

Definitions use one canonical fixed unit per profile. Initial units are
milligrams for mass-like solids, millilitres for volume-like liquids,
milligrams for effective mass, nutrition subunits selected by the food schema,
ABV basis points, and the smallest supported currency subunit. Conversions for
display occur only at the API/UI boundary.

### Definition invariants

- `capacity > 0`.
- Full contents mass/value and tare mass/value use integers.
- Depletable and bulk-lot definitions have zero tare. Containerized definitions
  may have nonzero tare; zero is legal for a disposable wrapper.
- All checked full-unit additions fit `u64`.
- Food, alcohol, and soap capabilities must reference a compatible measurement
  profile and unit.
- A discrete definition has no measurement profile and cannot acquire measured
  state.

Seed validation rejects the whole definition before inventory mutation. These
are authoring errors, not values to clamp.

### Personal and party row invariants

The same rules apply to `InventoryItem` and `PartyInventoryItem`:

1. `quantity` is positive. Zero-quantity rows do not persist.
2. No measured-state row means a full, unopened row. It may have any positive
   quantity allowed by the inventory limit.
3. A measured-state row requires a measurement profile and `quantity == 1`.
4. `0 <= remaining_amount <= capacity`; reducers reject values outside it.
5. A partial row is `0 < remaining_amount < capacity`.
6. An opened, still-full row may be represented by `remaining_amount ==
   capacity` when opened identity matters. It does not merge with sealed stock.
7. A depletable/bulk row reaching zero is deleted atomically with its measured
   state. A containerized row reaching zero remains as the empty container with
   amount zero. Discarding or consuming that container explicitly deletes both.
8. A measured state has exactly the same owner as its parent by construction:
   it stores only the parent row ID. Orphan state and state pointing across
   personal/party tables are invalid.
9. Deleting a parent deletes its state in the same reducer. No reducer may
   publish an intermediate orphan.

The absence of measured state is the final schema's representation of unopened
full stacks, not a transitional fallback for legacy data.

### Opening, splitting, transferring, and combining

- Opening one unit from an unopened stack decrements that stack. If it was the
  last unit, the row may be reused. The opened unit becomes a quantity-one row
  with state at `capacity`. This happens before the first consumption.
- Reducers preflight definition, ownership, bounds, row limits, and all checked
  arithmetic before mutation.
- A complete partial personal row transfers as one indivisible custody unit to
  a new party row, or vice versa. Parent and measured state move atomically;
  amount, definition, and container identity do not change.
- Quantity transfer of unopened stock retains current stack behavior.
- "Transfer part of this partial row" means pouring and is deferred. The first
  rollout transfers the complete row only.
- Unopened rows merge by the existing stack compatibility rules.
- Partial depletable/bulk rows may combine only through an explicit reducer,
  with the same item definition, unit, relevant food/preparation/provenance and
  contamination profile, and total amount at most capacity. The reducer keeps
  the lowest parent row ID, adds exact conserved integer fields, and deletes
  the other parent/state atomically.
- Containerized rows do not combine merely because their contents match.
  Pouring between vessels is deferred.
- If multiple eligible rows exist, selection is stable: requested owner scope,
  then family-specific priority, then oldest/open partial first, then item ID,
  then inventory row ID. Reducers must sort explicitly rather than rely on table
  iteration order.

## Effective mass and value

Let:

- `C` be positive full capacity;
- `A` be remaining amount, `0 <= A <= C`;
- `Mc` and `Vc` be full contents mass and intrinsic value;
- `Mt` and `Vt` be container tare mass and recoverable intrinsic value.

For a measured singleton:

```text
contents_mass(A)  = floor(Mc * A / C)
contents_value(A) = floor(Vc * A / C)
effective_mass    = Mt + contents_mass(A)
effective_value   = Vt + contents_value(A)
```

For `Q` unopened units:

```text
effective_mass  = Q * (Mt + Mc)
effective_value = Q * (Vt + Vc)
```

Depletable and bulk profiles set `Mt = Vt = 0`. Containerized amount zero
therefore retains its tare totals. Products are evaluated in `u128`, divided
once, converted to `u64`, then added/multiplied with checked operations. Any
invalid bound or overflow rejects the command before mutation; authoritative
code must not saturate, wrap, or use floats. Aggregators use checked integer
addition and fail the whole operation on overflow.

Flooring is canonical and monotonic. For any partition of one amount, the sum
of independently floored contents values cannot exceed the unsplit value.
Combining restores at most the canonical value of the combined amount.
Intrinsic conserved value is distinct from a merchant quote.

### Currency and trade

Before arbitrary amount sales, currency gains an integer subunit fine enough
for ordinary fractional goods (for example 100 subunits per displayed base
coin). Denominations remain presentation/exchange records over that conserved
integer.

The safe initial policy is:

- merchants buy or sell only complete inventory rows;
- unopened stacks may still transact an integer quantity of full units;
- a measured singleton is quoted once from its canonical effective contents
  value plus recoverable tare value;
- player-to-merchant proceeds round down once at the complete-line aggregate;
- merchant-to-player prices round up once at the complete-line aggregate;
- party stake credit, liquidation, surrender valuation, and trade all call the
  same effective-value function;
- no quote is computed for artificial child portions, and splitting is never
  an accepted way to obtain multiple independently rounded payouts.

Later arbitrary pouring/sales must carry a deterministic residual/remainder
ledger or allocate the line's remainder to one stable child. Repeated split,
sale, and recombine sequences must never increase total currency plus intrinsic
inventory value.

## Item-family semantics

### Alcohol

Alcohol definitions retain serving volume, ABV basis points, emergency
hydration, and disinfectant effectiveness, but serving volume becomes measured
capacity rather than a consumed stack unit.

- Opening a bottle splits one sealed unit. Drinking subtracts the exact integer
  volume requested or the bounded volume needed by the action.
- Alcohol dose, hydration, and disinfectant use are prorated from consumed
  volume with widened integer arithmetic. Hydration remains capped by physical
  non-alcohol water volume.
- Surgery previews the exact container, volume, soap amount, resulting
  remainder, and effectiveness before confirmation.
- Reserve targets are expressed in total potable volume or servings converted
  to volume, not row count. Protected strong disinfectant stock remains a
  separate policy constraint.
- Automatic selection preserves current intent: shared before personal where
  allowed, ordinary drinks before strong disinfectant, then open partial before
  sealed, effectiveness/ABV policy, item ID, row ID.
- Drinking from and pouring between arbitrary reusable cups, bottles, and
  waterskins is deferred. The first implementation consumes directly from the
  owned source row and transfers only complete partial containers.

### Food

The existing `food_lot` is a useful behavioral prototype but currently stores
conserved mass, nutrition, and value with floating-point fields. The clean
rollout converts durable mass, useful calories/nutrition, ingredient
provenance shares, and value to integer units.

- Every heterogeneous or prepared lot remains quantity one with measured state.
  Homogeneous sealed units may stack until opened.
- Eating subtracts integer mass and proportional nutrition/value from the same
  lot. The last operation consumes all residual conserved fields so rounding
  dust cannot strand or multiply nutrition.
- Recipes consume complete selected portions and create one derived
  quantity-one output lot whose integer mass, nutrition, value, provenance, and
  contamination are computed once. No terminal meal may be repeatedly cooked
  to compound value.
- Partial meals retain preparation, age, provenance, nutrition, and private
  contamination anchors. Compatible combine requires all relevant metadata to
  match; heterogeneous leftovers otherwise remain independent.
- Spoilage continues to be evaluated lazily from lot identity and time. No
  floating conserved inventory field is justified by continuous microbial
  calculations; derived concentration may use numeric simulation internally
  while persisted conserved dose/mass inputs remain integers.
- Automatic eating selects oldest/most perishable eligible open lots before
  sealed lots, then item ID and row ID, subject to shared-before-personal rules.

### Soap

Soap becomes a depletable mass/capacity profile rather than a whole-use stack.

- A wash or surgery subtracts the bounded amount actually used. Cleansing and
  infection-control effectiveness are prorated from that amount and may retain
  diminishing-return logic outside the inventory arithmetic.
- Remaining soap mass and value fall with remaining amount; zero deletes the
  row because there is no container tare.
- Rest and surgery previews show exact mass/percent remaining, planned
  consumption, source owner, expected cleansing/effectiveness, and remainder.
- Automatic selection uses open personal soap before sealed personal soap,
  then open shared soap before sealed shared soap, with the existing
  health-risk ordering for assignment and item ID/row ID ties.

## Impact map

| Surface | Required behavior |
| --- | --- |
| Inventory targets | Definition-aware targets use full units for discrete/unopened goods and total amount for measured goods. Reserve calculations include partial rows without pretending they are fractional object counts. |
| Personal/party transfer | Full stack quantities retain current behavior; partial rows move whole with state. Both owner schemas enforce identical invariants. |
| Loot | Tactical results describe full units or explicit strategic measured lots. Finalization creates validated profile/state rows; autoloot uses effective value/mass. |
| Trade | Complete-row interim policy and aggregate directional rounding apply. Quotes, affordability, and settlement stock use currency subunits. |
| Encumbrance/travel | Replace every direct `item.weight * quantity` path with one shared checked effective-mass evaluator for personal plus party inventory. |
| Party stakes | Deposits, withdrawals, loot shares, reserve value, and member settlement use the same effective value. Partial row custody never loses its stake valuation. |
| Liquidation/surrender | Deterministically value and select complete rows with effective mass/value; delete parent and measured state atomically. Currency remains excluded from ordinary sale. |
| Automation | Resupply, ration planning, autoloot, autosell, washing, eating, drinking, and surgery compare amount totals and effective value/mass, and use stable selection. |
| Tactical handoff | Include remaining amount and units only for relevant carried/equipped snapshots. Tactical state never becomes durable authority. |
| UI | See below; staged controls operate on row IDs and amounts, never display fractional swords. |

Every inventory consumer must go through central helpers. Direct multiplication
of base `weight` or base value by `quantity` is invalid once profiles ship.

## UI and API contract

The collapsed inventory row continues to show `#` for discrete or sealed
quantity. Measured singletons show a localized amount and unit, optionally
`remaining / capacity` and a progress bar. Container rows show contents and
container separately in expanded details; empty containers say "Empty" rather
than "0 bottles." Search and sort use effective total mass/value.

Opening is normally implicit on first use but can be explicit where the player
must choose a bottle. Transfer, discard, drink/eat/wash, cooking, surgery, trade,
and liquidation dialogs preview the exact row, amount, effective mass/value
change, and post-action remainder. Controls use integer step sizes declared by
the family profile and clamp only in the UI; reducers independently reject
invalid input. The initial UI offers complete-row transfer/sale and bounded
consumption, not pouring/mixing.

Public personal and party projections add the measurement profile fields needed
for display and optional measured state. Mutation APIs accept inventory row IDs
plus integer amount where consumption supports it. Generated
`adventuresim-stdb-client` bindings, strategic-web view models, tactical
snapshots, simulator actions, and reducer call sites must update in the same
schema commit. No bindings are generated for this architecture/prototype-only
change because no public table changed.

## Rollout plan

1. Introduce currency subunits and shared checked mass/value arithmetic. Convert
   definition constants that participate in conservation.
2. Add measurement-profile and separate personal/party state tables; update all
   read projections and generated bindings in one clean-schema change.
3. Reset and reseed the isolated development database. Convert alcohol and soap
   definitions and reducers; keep complete-row transfer/trade.
4. Replace floating conserved `food_lot` fields with integer state and route
   cooking, eating, contamination dose inputs, and merchant quotes through it.
5. Update loot, targets, party stakes, liquidation, surrender, automation,
   strategic simulation, tactical handoff, and every encumbrance path.
6. Ship measured UI display, amount controls, previews, accessibility labels,
   and stable live-refresh behavior.

There is intentionally no live-data migration. Deployment of the schema change
requires an isolated database reset/reseed and regenerated client bindings.

## Verification requirements

- Profile validation: zero capacity, incompatible tare, invalid family/unit,
  and full-unit overflow.
- Row validation for both owner types: zero quantity, state on discrete item,
  non-singleton measured state, amount over capacity, orphan state.
- Full, partial, opened-full, and empty mass/value examples for depletable,
  bulk, and containerized profiles.
- Monotonicity over the complete amount range and checked aggregation overflow.
- Opening a stack, complete partial transfer in both directions, deterministic
  selection, compatible combine, zero deletion, and empty-container retention.
- Property tests that partition/recombine preserves conserved amount and cannot
  increase intrinsic value; trade sequence tests that cannot create currency.
- Family tests for alcohol dose/hydration/disinfection/reserves, food
  nutrition/recipe/leftover/spoilage identity, and soap cleansing/surgery
  previews.
- Integration tests for loot, trade, encumbrance, targets, autoloot/autosell,
  stakes, liquidation, surrender, and generated API round trips.
- UI tests for units, empty labels, amount bounds, complete-row-only controls,
  previews, sorting, keyboard operation, and SSE refresh deferral while a draft
  is active.

## Deliberately deferred

- Arbitrary pouring, partial-row transfer, and player-selected splitting.
- Mixing liquids or heterogeneous food lots with different profiles,
  provenance, age, contamination, or preparation.
- Refilling reusable bottles, cups, waterskins, and arbitrary vessels.
- Merchant purchase/sale of arbitrary sub-row amounts.
- Container damage, leakage, evaporation, and nested containers.

These omissions constrain the first rollout; they do not change the durable
model.

## Issue #150 acceptance map

- Discrete-use, fixed-point bulk, and per-container approaches and tradeoffs:
  [Alternatives considered](#alternatives-considered).
- Integer durable state and personal/party invariants:
  [Decision](#decision), [Future clean schema](#future-clean-schema).
- Contents plus container mass/value:
  [Effective mass and value](#effective-mass-and-value).
- Currency precision and split/recombine rounding:
  [Currency and trade](#currency-and-trade).
- Alcohol, food, and soap partial semantics:
  [Item-family semantics](#item-family-semantics).
- Targets, transfers, loot, trade, encumbrance, and automation:
  [Impact map](#impact-map).
- UI and reset/migration:
  [UI and API contract](#ui-and-api-contract), [Rollout plan](#rollout-plan).
- Gameplay and architecture documentation:
  this document plus links from `ARCHITECTURE.md`,
  `FOOD_AND_COOKING.md`, and `wiki/shared/Inventory.md`.
