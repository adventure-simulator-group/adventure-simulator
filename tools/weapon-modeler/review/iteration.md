# Equipment model iteration

Use one implementation agent and a separate artistic reviewer. The reviewer
owns the visual decision and must inspect the images, rather than infer quality
from a passing mesh validator. The starting rubric and period references are in
[artistic-criteria.md](artistic-criteria.md).

1. Capture defaults, a fixed seed and the adverse fixtures before changing
   geometry. Include front/oblique views, rear shield views and low/high LODs.
2. Give the reviewer the capture paths and the rubric. Ask for specific visible
   failures, their affected specimens, priority and acceptance conditions. Keep
   period evidence separate from modeling judgments and generator stress cases.
3. Fix the earliest construction failure with one implementation agent. Add
   behavioral coverage for the failed contact, crease, aperture or silhouette.
   Do not hide rejected slider combinations: captures record rejected proposals
   alongside the exact accepted definition.
4. Replay the saved `fixtures.json` into a new output directory. Each screenshot
   carries its seed, view and LOD; `manifest.json` records definitions, slider
   changes, dimensions, triangle counts, validation results and source hashes.
   The source snapshot permits reconstruction even before changes are committed.
5. Have the independent reviewer compare the replay against the previous pass.
   Fix remaining blockers and repeat steps 3–5. A fresh seed supplements the
   replay; it must never replace an unfavorable existing specimen.
6. Stop when the targeted findings are independently accepted and `npm test`
   passes. Record remaining historical or rendering limitations explicitly.

From `tools/weapon-modeler`, start the server with `npm start`. The capture
runner uses an optional local installation of `playwright` and installed Chrome:

```powershell
node capture-review.mjs --output ../../output/playwright/equipment/before --seed 1544
node capture-review.mjs --output ../../output/playwright/equipment/adverse --adversarial true
node capture-review.mjs --output ../../output/playwright/equipment/after --fixtures ../../output/playwright/equipment/before/fixtures.json
npm test
```

If Playwright is supplied by a shared runtime, pass `--playwright-module` with
the absolute path to its `index.mjs`. Other options: `--url`, comma-separated
`--ids`, `--lods low,medium,high`, and
`--views front-whole,oblique-detail,rear-whole`. The interactive gallery at
`/review.html` accepts `seed`, `ids`, `batch`, `lod`, `pose` and `focus` query
parameters. Numeric and choice-control randomization is deterministic per
preset, independent of gallery ordering. Captures fail on invalid geometry or
browser exceptions and retain the failing image and manifest for diagnosis.

LOD camera framing comes from the high-detail version of the same specimen.
Details select actual hilt/head parts rather than a percentage of weapon length,
so an unusually long grip does not get cropped down to only its pommel.

The modeler is an asset-authoring experiment. These changes and captures do not
change the separate authoritative Rust weapon model or tactical equipment LOD
selection. A selected LOD can be exported as its own skinned GLB.
