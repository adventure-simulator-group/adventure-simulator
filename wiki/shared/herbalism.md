# Herbalism and ingredient preparation

Herbalism is the Intelligence-governed strategic skill that recognizes,
activates, and detoxifies medicinal substances. It is no longer a separate
crafting menu. Food and medicinal ingredients use the same measured `FoodLot`
structure; an ingredient may have nutrition, flavor, medicinal components, or
any combination of those properties.

## Physical preparation

Concrete personal ingredient rows expose **Cut** and **Grind** as Exchange List
Edge Actions. Preparation states are exclusive: Raw may become Cut or Ground,
and Cut may become Ground. Ground cannot become Cut and never counts as Cut.
Neither action changes mass, nutrition, flavor, value, or microbial load.

Cutting takes 10 minutes before skill reduction and requires a carried edged
weapon whose current, condition-adjusted accuracy is at least 0.5. Equipped
weapons and weapons in nested personal or current-party containers qualify;
fireplace custody does not. Cutting causes no weapon wear, trains Knife, and
Knife rank removes 6% of time per rank.

Grinding takes 20 minutes, needs no tool, trains Bludgeon, and receives the
same 6%-per-rank reduction. A carried mortar and pestle halves the result.
A clipped terminal-time action changes and trains nothing.

Each measured lot has a stable inventory-object identity and a nonzero material
revision from creation. The strategic gateway publishes the exact Cut/Grind
request tuple and calculated duration used by the contextual inventory edge
action; the browser does not derive a lot ID, object ID, revision, or request
ID. At commit, the reducer rebuilds the typed plan and revalidates actor, place,
object custody, revision, transition, tool, duration, rights, mass, private
medicinal components, and contamination. The transformation conserves all
extensive and private material exactly.

The request identity binds only public authority: actor, submitted inventory
scope and row, lot, stable object, revision, action, attempt generation,
canonical place, and immediate resolved custody. Private medicinal and
microbial truth is excluded from the public request ID. It remains bound to the
private plan and receipt and is revalidated transactionally after the wait;
microbial truth is evaluated from its anchor and growth rate at the actor's
current minute. The gateway publishes fully resolved personal- and party-held
actions in their contextual inventory surfaces.

Preparation attempts have private durable receipts. An exact retry is a
no-op, reuse of a request ID for different inputs is rejected, and stale
revisions fail closed. A terminal-time interruption records the attempt;
preparation state, material, revision, and training remain unchanged, and an
exact retry is resolved from that receipt.
The server advances a private attempt generation after such a clipped attempt,
so the old request remains an exact no-op while the gateway issues a distinct
next request at the unchanged material revision. Terminal boundaries are part
of the typed plan and therefore produce a wait-only plan with no preparation
commit effect. After a successful wait, the reducer reloads place, custody,
lot, tool, action, and private source rows in the same transaction before
material or training commit; a mismatch fails and rolls back the interval.

Cut lots use 75% and ground lots 50% of their authored cooking safety time.
Prepared food remains edible. Medicinal-only substances may contribute zero
nutrition and flavor; food capability never removes authored toxicity.

## Medicinal transformations

Medicine is a private, versioned component on a stable carrier rather than a
disease-keyed catalogue identity. Herbalism is required when a transformation
activates useful constituents or removes toxic parts. Heating can combine a
compatible medicine with ordinary food, conserving both nutrition and
medicinal components. Under- or overheating scales potency; authored
heat-sensitive substances may be destroyed or gain harmful components.
Components stack in stable order. Public views show only safety knowledge the
observer's Herbalism supports; exact hazard profiles remain private.

Oral medicinal food doses by the fraction actually eaten, so a partial meal
cannot apply a whole dose or be consumed twice. Topical carriers retain their
topical administration route. Intervention application pins the generic
`InterventionProfile` version for disease-independent replay.

## Tinctures

Tincturing is passive container work. Put exactly 50 g of ground poppy and
150 ml of authored tincture spirit directly in a bottle or jar, then start the
process. Nested hidden ingredients do not count. Poppy uses a six-week
(60,480-minute) baseline. The actor is free while it macerates.

The process is keyed to the stable bottle object. Its contents are locked, but
the whole bottle and subtree may transfer custody. Completion materializes a
versioned medicinal component once and sets a durable matured flag; later
clock regression cannot make it unfinished. Typed container liquid counts
against capacity exactly once and cannot silently mix with water.

Gateway, living-character, tactical/unresolved-encounter, exact custody,
capacity, arithmetic, and atomic-preflight checks remain authoritative.
Herbalism stays visible as an informational skill row and professional field.
