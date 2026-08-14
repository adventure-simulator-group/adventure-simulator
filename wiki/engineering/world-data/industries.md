# Strategic industries and commodities

World schema v21 and inference rules v6 attach a nonempty, bounded
`InferredIndustryProfile` to every imported settlement. The profile describes
plausible local production in 1544; it is strategic world data, not a tactical
simulation or a market inventory.

Industry inference runs after route-terrain finalization. It consumes only the
canonical HYDE 3.5 land-use reconstruction, historical vegetation, finalized
SoilGrids/EGDI soil and lithology, OWDA moisture, EU-Hydro access, settlement
population, and incident finalized routes. Scores and thresholds use integers
and basis points. Outputs are sorted, unique, and limited to 24 per settlement.

Routes can downgrade `Regional` production to `Local` or `Marginal`; they never
invent a crop, fishery, deposit, fuel, or construction material. Mining
currently contains coal only and requires explicit coal-bearing sedimentary
geology. Crystalline rocks never imply metals.

Derived outputs cover agriculture (grain, flax, wool, dairy, hides), freshwater,
estuarine, and marine fishing, exact mapped quarry stone, coal mining, clay and
earthenware pottery, convergent peat cutting, woodland products and charcoal,
evaporite/saline/coastal-fuel saltmaking, and evidence-backed construction
inputs. Peat requires an organic/Histosol/peat parent plus wet convergence;
topsoil carbon alone is insufficient. Coastal brine requires open-coast access
and woodland or peat fuel, so Baltic access alone does not imply solar salt.

If no derived output clears its threshold, exactly one marginal fallback is
chosen in stable precedence: freshwater fish, grazing dairy, cropland grain,
woodland fuelwood, then common aggregate. A fallback cannot claim regional
scale or an arbitrary resource.

Offline validation and the SpacetimeDB reducer recheck profile bounds, canonical
ordering, route scale limits, and resource evidence. Build-report counters
reconcile settlements, derived/fallback outputs, and every industry category.
Required Markdown provenance fails closed when the source-note bound cannot
hold it.

The complete official-world audit remains blocked until all upstream
distributions are available locally. The synthetic rules matrix is deterministic
and does not close issue #62 by itself.
