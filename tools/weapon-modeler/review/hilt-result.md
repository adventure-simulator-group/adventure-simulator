# Accepted hilt construction iteration

Branch: `codex/weapon-artistic-iteration`, following equipment iteration
`336700f0`. Implementation agent: `hilt_implementation`. Independent artistic
reviewer: `hilt_artistic_review`.

The implementation adds lathed, plate/wheel, faceted, writhen, outline and
composite pommels; swept guard sections and independently shaped quillons;
seven terminal choices; connected guard graphs with grip-dependent bindings;
and optional curved, dished, pierced shell plates. Composite sockets accept
built-in ornaments or closed indexed authored meshes. Conditional editor
controls expose the relevant dimensions, and semantic pommel/guard views keep
small fittings visible during editing and screenshot review. See
[hilt-construction.md](hilt-construction.md) for the schema and authoring limits.

The reviewer initially rejected detached crown points, buried relief,
writhen surface discontinuities, an overly rectangular shell and insufficient
evidence of section twist. Exact-input replay verified their fixes. A fresh
seed then exposed shelves in the default Zweihander's fluted profile; the
final continuous profile and helical tessellation resolved those as well.
[hilt-artistic-criteria.md](hilt-artistic-criteria.md) records the independent
findings and final acceptance.

Writhen tessellation now follows surface error rather than multiplying dense
fixed subdivisions. The extreme pommel uses 1,248 / 1,456 / 3,360 triangles at
low / medium / high detail. Its whole sword uses 6,332 triangles at high detail,
down from 50,246 during remediation. The default high-detail Zweihander uses
6,400 triangles, down from 50,650.

## Verification

- Full Node suite: 77/77 passed, exit 0, 387.2 seconds. The endpoint-pair sweep
  covers 29,884 numeric combinations, including hidden controls.
- Subsequent schema/control cleanup: artistic and hilt tests 19/19 passed;
  final independent-quillon visibility cleanup: hilt tests 11/11 passed.
- Final visual evidence: 48 matched low/high close-up sheets for 24 fixed
  adverse cases, plus 24 sheets for eight sword defaults and eight seeded
  variants. The seed-1546 replay explicitly records seed 1546.
- Browser editing verified composite base controls, visible ornaments and
  persistent pommel focus across LOD changes.
- Skinned CLI exports succeeded using the local character rig: high-detail
  Zweihander (6,400 triangles) and low-detail Reitschwert (1,054 triangles),
  each retaining the 130-joint skin and `r_weapon` attachment.

Local evidence is under `output/playwright/weapon-iteration/`:

- `hilt-final-fixed/` and `hilt-final-seed1546/`: final screenshots, exact
  definitions, manifests and source snapshots/hashes. Their Git revision
  names the parent commit because captures preceded this implementation's
  commit; source snapshots identify the actual reviewed code.
- `hilt-editor-composite.png`: editor interaction evidence.
- `hilt-tests-final-optimized.txt` and `hilt-controls-final.txt`: passing logs.
- `hilt-zweihander-high.glb` and `hilt-reitschwert-low.glb`: export samples.

## Bounded limitations

Low-detail extreme writhen close-ups retain angular flute facets and small
notches; medium/high is appropriate for close inspection. Each shell plate
supports one matched cutout, with separate plates or members for additional
openings. Later shell studies remain optional and labeled, outside the default
1544 equipment set. Built-in ornaments demonstrate construction; acceptance
does not establish museum-level ornament fidelity or validate every possible
user-authored graph.
