# World bounds

The offline world compiler accepts an optional `--world-bounds PATH` JSON file
that defines one inclusive WGS84 rectangle:

```json
{
  "south_west": { "latitude": 47.0, "longitude": -5.0 },
  "north_east": { "latitude": 61.0, "longitude": 24.0 }
}
```

The values above are an example only. Choose and commit the actual playable
extent deliberately. Latitude must be between -90 and 90, longitude between
-180 and 180, and the southwest point must be strictly south and west of the
northeast point. Antimeridian-crossing extents are intentionally rejected until
their routing and tile-selection behavior is explicitly designed.

The compiler records the selected bounds in `WorldMetadata`; changing them is a
full rebuild boundary. When bounds are supplied, the Viabundus importer retains
only active settlements and land/ferry edges whose endpoints are inside the
rectangle, then retains the required endpoint and parent nodes. Alias and
description records follow the selected settlements. An empty settlement set is
rejected before environmental enrichment.

All source acquisition and preparation must start from this canonical extent.
For a source using WGS84 one-degree tiles, `WorldBounds` exposes the southwest
origin of every intersecting tile. Sources using another CRS transform the
same rectangle before selecting their rasters or vector features. A source may
apply a documented context margin—for example, hydrology needs nearby features
to classify road crossings—but that margin is part of that source's inventory
and provenance, not a second playable-world boundary.

For local smoke testing, `world-bounds.hamburg-test.json` covers approximately
50 km around Hamburg's city centre. It is deliberately named as a temporary
test extent and must not be treated as the final playable-world boundary.
