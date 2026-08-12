# Physical materials foundation

`adventuresim_core::material` is a pure vocabulary for the physical substrate
shared by existing food, liquid, medicinal, contamination, preparation, and
passive-process mechanics. It adds no persistent schema, reducer, generated
binding, client payload, recipe language, or general chemistry system.

Every material lot has its own stable `MaterialLotId` and is connected to one
stable `PhysicalObjectId` with checked `ObjectCustody`. Direct containment is
therefore ordinary object custody in a stable vessel object, not a parallel
cooking or tincture identity. `MaterialVessel` adds only an authored liquid
capacity snapshot; reducers remain responsible for exact custody, nesting,
locks, and capacity authority.
Invariant-bearing snapshots, remainders, process clocks, and produced-material
values have private fields and checked constructors; adapters cannot fabricate
zero outputs or bypass the full-consumption `None` remainder.

Material measure uses integer milligrams and microliters. Solids may have zero
known volume; liquids still carry mass. Partial selection rounds the taken
portion down once and assigns the exact remainder to the source, so partitioning
cannot manufacture material; consuming the whole returns no remainder value, so
an empty lot cannot be persisted. The same split law applies to private
medicinal component magnitudes and contaminant loads. Combination and
transformation use a checked conservation receipt: output plus explicit
physical loss must equal input within a rounding tolerance capped at one integer
subunit. A closed reducer-owned process policy must name legitimate evaporation,
rendered fat, component activation/destruction, microbial growth/kill, washing,
or other existing gains and losses rather than letting ordinary mutation choose
an arbitrary allowance.

Shared preparation and process enums cover Raw, Cut, Ground, cooked lanes,
tincture, combine, pour, wash, administer, and consume, with closed typed domain
extensions. They do not encode recipes or arbitrary property bags. Cooking and
tincture share stable vessel identity and checked process timing. Cooking
distinguishes early, exact-ready, and late retrieval. Passive processes propose
materialization once; a recorded materialization remains mature even if an
observer clock later regresses, and a recorded materialization before the ready
boundary is rejected.

Medicinal components and microbial or other contamination loads are private
extensive values. The core projection does not inspect or copy them into a
public material view. The authoritative adapter is still responsible for
supplying observer-safe presentation and must not derive it from unknown private
truth. Existing observer knowledge continues to govern any safe hazard or
medicine summary.

Transformation provenance binds a request and stable process. Every source
contribution is derived from an authoritative private snapshot and carries its
lot identity, physical object and exact custody, expected revision, full
measure, consumed measure, and proportional private truth. Receipt construction
derives the canonical input digest internally from those snapshots, verifies
bulk and private extensive accounting against full typed output snapshots, and
rejects duplicate lot IDs or physical object IDs even when custody differs. An
exact retry additionally compares the full typed server-side source values,
including preparation and private consumed truth; those low-entropy facts are
never placed in the digest. Exact equality returns the prior
receipt as a no-apply decision. A different provenance colliding with a stored
request is rejected, so adapters cannot rerun effects and duplicate, lose, or
manufacture material.

Before applying a non-replay receipt, reducers must call the pure commit
validator in the same transaction with fresh authoritative snapshots. It
requires the exact lot, physical object, custody, revision, full measure,
preparation, and private truth captured by planning, and separately reports
missing or ambiguous sources, identity mismatch, stale revision, overconsumption,
and other snapshot mismatch.

## Planned adapters

- Cut and grind will preserve measures, components, contamination, and source
  lot identity while changing only the shared preparation state.
- Fireplace combine/cook will aggregate direct vessel contents, water, authored
  nutrition/flavor/value, private microbial load, explicit losses, and source
  provenance into one output lot in that same stable vessel.
- Tincture will bind the existing passive process to its bottle or jar object,
  conserve herb/spirit inputs, and materialize the private medicinal outcome
  once at the existing duration boundary.
- Pour, wash, administer, and consume will use exact partial measures and
  receipts while preserving their current contamination, route, dose, and
  early/late behavior.

Those adapters remain later stacked work. This foundation intentionally has no
brewing, fermentation, preservation, temperature spoilage, poisons, dyeing,
tanning, smithing, or new process family.
