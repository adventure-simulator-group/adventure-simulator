# Official religion in 1544

Settlement religion is reconstructed from the Institute of European History
(IEG) maps of the legally recognized religion of European territories in 1500
and 1555. This models public law and institutions, not personal belief.

- IEG map collection: <https://www.ieg-maps.uni-mainz.de/mapsp/mapconfession.htm>
- Source rights statement: © IEG Mainz / Andreas Kunz. The source page does
  not state an open-data license; only the project's coarse derived
  intermediate is checked in.
- Reference maps: `IEG_Europe_1500_religion.gif` and
  `IEG_Europe_1555_religion.pdf`.
- Game year: 1544.

The published maps are illustrative rasters without a machine-readable
geographic boundary layer. The checked-in
`assets/world-data/ieg-religion-1544.csv` is therefore a deliberately coarse,
human-curated intermediate between the two maps. Each row is a named bounding
region with an explicit unique priority; lower numbers are evaluated first, so
small territories override broad ones even if the file is reordered. The
shapes are gameplay priors and do not claim to reproduce historical borders
exactly.

Settlements outside the curated regions receive a complete plausible fallback:
Roman Catholic in the general Viabundus coverage and Eastern Orthodox in the
far-eastern Ruthenian portion. No canonical record stores an unknown religion.
The compiler refuses to use this fixed intermediate for a year other than 1544.

## Imported model

Canonical data distinguishes:

- an established official religion;
- parity, where two recognized western confessions have equal legal status;
- multi-confessional status, where multiple confessions are legally present;
- religion determined at the municipal level.

The supported denominations are Roman Catholic, Lutheran (the IEG Wittenberg
Reformation category), Reformed, Anglican, Eastern Orthodox, and Islamic. Pair
arrangements carry a pair-specific church enum, so
the settlement's current single church cannot name a denomination outside the
legal arrangement. The existing church/priest gameplay identifier is derived
from that typed denomination during database import rather than supplied as a
second potentially contradictory field.

The compiler reports the number of curated regions, settlement samples, and
fallback samples. `--religion-regions` can point to another intermediate using
the same checked CSV boundary.

The checked intermediate was audited against all 6,041 Viabundus settlements
active in 1544. Its 13 explicit regions omit places for which the historical
source identifies only generic Protestantism, allowing denomination inference
to resolve those settlements without creating a catch-all canonical religion.
Remaining unmatched settlements use the Roman Catholic fallback. The Upper
Rhine multi-confessional approximation retains its specific priority over the
broader Hessian region.

`just verify-religion` validates the committed file's fixed size, SHA-256,
exact column order, ascending unique priorities, coordinate bounds, statuses,
and 13-row revision. `plan-religion` reports that identity.
`init-religion` always refuses so the rights-reserved GIF/PDF source images are
never downloaded or mirrored.
