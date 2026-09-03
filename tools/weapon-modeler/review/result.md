# Accepted equipment iteration

Branch: `codex/weapon-artistic-iteration`, based on `badaf222`.
Implementation: one agent. Independent artistic review: `artistic_review`.

The reviewer requested changes after the baseline screenshots and again after
the first implementation pass. The second review caught a mirrored halberd
fluke overlapping its axe and a shield plate obstructing the hollow buckler
boss. Both were fixed before independent acceptance. Full findings, historical
sources and the final acceptance decision are in [artistic-criteria.md](artistic-criteria.md).

The final changes cover indexed surface normals and preserved hard edges,
fitted pommel/grip junctions, rounded flattened caps, variable quillon taper and
terminals, alternative pommel profiles, tapered/cusped axe heads, coupled rear
head orientation, diamond-section spear points, hollow sockets and bosses,
buckler apertures, shield construction/proportions and shared LOD budgets.
The editor and animator GLB export both use the selected LOD.

Validation completed:

- All 67 Node tests passed, including the preset endpoint/seed suite with
  17,960 pair, all-endpoint and seeded cases, plus composer cases.
- All 28 defaults validated at low, medium and high LOD. Tests check stable
  frames, bounded extent/volume drift, smoothing boundaries, grip seats,
  fluke orientation, real hand apertures and retained GLB indices/normals.
- The first reviewed specimen definitions were replayed unchanged across
  three LODs and three views. Six adverse cases received low/high independent
  visual review. A fresh seed captured all 28 presets at low/high detail without
  geometry or browser errors.
- Browser interaction verified pommel selection, breadth-slider limits,
  low/high detail changes and complete hilt framing. Real low-detail longsword
  and high-detail buckler GLBs exported successfully using the 130-joint rig.
- `git diff --check` passed.

Evidence is under `output/playwright/weapon-iteration/` relative to this
worktree: `before-*.png`, `cycle1/`, `replay/`, `adversarial-final/`,
`fresh-seed-1545/`, `editor-pear-high.png`, `tests-final.txt`,
`longsword-low.glb` and `buckler-high.glb`. Capture directories contain exact
`fixtures.json` and camera/geometry `manifest.json` records; the fresh seed
also archives generator source files and hashes.

This acceptance covers the visible construction and LOD changes. It does not
claim exhaustive historical style coverage, calibrated museum-object mass,
texture-space tangents or changes to the separate authoritative Rust model.
