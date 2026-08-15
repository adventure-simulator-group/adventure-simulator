# Tactical presentation module restructuring

## Purpose

`crates/adventuresim-tactical-client/src/presentation.rs` currently combines the
top-level tactical presentation plugin with several independently evolving
rendering systems. At committed `HEAD` it is already about 1,730 lines. The
current tree-rendering work expands it to roughly 2,700 lines, with the tree
geometry and impostor baker accounting for about half of the file.

Restructure this code into focused modules before further grass work lands. The
goal is a behavior-preserving extraction that reduces future merge conflicts,
especially between procedural grass and procedural tree development.

This task is architectural cleanup only. Do not redesign rendering behavior,
change visual output, retune constants, or change scene-authority boundaries as
part of the move.

## Target layout

Replace `presentation.rs` with a directory module:

```text
crates/adventuresim-tactical-client/src/presentation/
  mod.rs
  environment.rs
  terrain.rs
  foliage.rs
  weather.rs
  vista.rs
  procedural.rs
  obstacles/
    mod.rs
    rock.rs
    tree/
      mod.rs
      geometry.rs
      impostor.rs
```

The exact visibility of internal items may vary, but preserve the existing
external interface through `presentation/mod.rs`.

## Module responsibilities

### `presentation/mod.rs`

Keep only the presentation facade and orchestration here:

- `TacticalPresentationPlugin` and its `Default` implementation.
- `TacticalGraphicsSettings`.
- Registration of materials, resources, startup systems, observers, and update
  systems.
- Marker types that genuinely describe the overall presentation layer, if any.
- `pub(crate) use` re-exports required by current callers.

The following names are currently consumed outside the module and must remain
available at their existing path unless all callers are deliberately updated in
the same change:

```rust
presentation::FoliageLayer
presentation::GrassInteractor
presentation::ProceduralRockVisual
presentation::TacticalPresentationPlugin
presentation::TacticalSunlight
presentation::TerrainMaterialPresentation
presentation::TreeImpostorProvenance
presentation::TreeLod
presentation::VistaTerrain
presentation::WeatherParticle
```

Prefer private definitions in their owning submodules followed by narrowly
scoped re-exports from `mod.rs`. Do not make implementation details workspace-
public merely to make the extraction easier.

### `environment.rs`

Own global visual-environment setup and response:

- Tactical camera creation and graphics-preset-controlled components.
- Sunlight and atmosphere setup.
- `TacticalSunlight`.
- Sunlight illuminance calculation.
- Distance fog calculation and updates.

Camera ownership should remain compatible with the existing tactical camera
plugin. This extraction must not alter camera projection, exposure, view range,
MSAA, bloom, SSAO, atmosphere, or environment-map behavior.

### `terrain.rs`

Own playable terrain presentation:

- `TacticalTerrainExtension` and `TacticalTerrainMaterial`.
- Terrain shader selection.
- `TerrainMaterialPresentation` and the source-scene marker used to associate a
  rendered terrain mesh with its authoritative scene.
- Terrain mesh/material spawning for `SceneId` and `SceneTerrain`.
- Terrain-material refresh when `SceneEnvironment` arrives.
- `terrain_material`, `scene_ground_color`, and the legacy environment fallback.

Keep the existing fallback behavior intact. Removing the legacy fallback, if
desired, is a separate behavior change.

### `foliage.rs`

Own grass and ground-cover presentation end to end:

- `TacticalFoliageMaterial` and foliage shader selection/specialization.
- `FoliageLayer`, `FoliageOf`, `GrassInteractor`, and
  `GrassInteractionState`.
- Grass-interaction material updates.
- Environment-driven grass and understory placement.
- `grass_patch_mesh`, `foliage_clump_mesh`, and `foliage_patch_mesh`.
- Grass/foliage tests.

Preserve all current constants and contracts, including:

- Forty-nine grass blades per shared patch.
- Two crossed planes per blade.
- Five vertices and three triangles per plane.
- Full density through 24 metres, continuous thinning to 18 percent at 120
  metres, and the existing visibility ranges.
- Stable per-blade thresholds, width compensation, wind weights, and local
  interaction behavior.

This boundary is intentionally strict so later grass work normally edits only
`foliage.rs` and `assets/shaders/tactical_foliage.wgsl`.

### `weather.rs`

Own precipitation presentation:

- `WeatherParticle`.
- Rain and snow mesh/material creation.
- Weather-particle replacement when the scene environment changes.
- Particle advancement and wrapping.
- Deterministic fixture-coordinate generation if it is not shared elsewhere.

Do not move weather into foliage merely because both react to wind.

### `vista.rs`

Own distant terrain rings:

- `VistaTerrain` and `VISTA_CHUNK_CELLS`.
- `SceneVistaBundle` handling.
- Vista LOD ring/chunk mesh generation.
- Vista color derivation.
- Vista tests.

Vista meshes remain presentation-only and must not acquire tactical colliders.

### `procedural.rs`

Own small deterministic helpers shared by more than one presentation subsystem:

- `splitmix64`.
- `unit_hash`.
- Text/digest seed conversion when genuinely shared.
- Basis-point conversion and color packing when genuinely shared.

Do not turn this into a miscellaneous dumping ground. A helper used by only one
subsystem should remain private to that subsystem.

### `obstacles/mod.rs`

Keep obstacle presentation coordination thin. Either:

1. retain one `SceneObstacle` observer that dispatches to `tree::present` or
   `rock::present`, or
2. register separate tree and rock observers that immediately return for the
   other variant.

Prefer the option that yields smaller Bevy system parameter lists without
duplicating scene queries or changing observer behavior.

### `obstacles/rock.rs`

Own:

- `ProceduralRockVisual`.
- Rock seed derivation if it is rock-specific.
- Procedural rock mesh generation.
- Rock presentation material/spawning.
- Rock containment tests.

### `obstacles/tree/mod.rs`

Own the runtime tree facade:

- `TreeLod`.
- `TreePresentationCache` and cached presentation handles.
- Tree material asset types and shader integration where appropriate.
- Tree variant selection.
- Tree asset construction and entity/child spawning.
- LOD visibility ranges and names.
- Public diagnostic/provenance re-exports needed by the scene viewer.

Keep the cache at this layer. Geometry generation and impostor baking should be
pure inputs used to populate it, not Bevy systems themselves.

### `obstacles/tree/geometry.rs`

Own procedural botanical geometry:

- `TreeBranchSegment`.
- Tree skeleton generation.
- `TreeLeaf`, oak leaf outlines, and leaf mesh generation.
- Branch tube/mesh generation.
- Crown bounds and related geometric helpers.
- Tree hierarchy and geometry tests.

This module should not know about Bevy commands, observers, material assets, or
visibility ranges.

### `obstacles/tree/impostor.rs`

Own the complete tree-impostor baking pipeline:

- Bake version and provenance records.
- Bake-card selection and fitting.
- Source-geometry hashing.
- CPU projection and rasterization of branches and leaves.
- Pixel writes and point-in-polygon tests.
- Baked image and card-mesh construction.
- Tree impostor material creation if keeping it here results in cleaner
  ownership than `tree/mod.rs`.
- Impostor and LOD-transition tests.

This is the highest-priority extraction. It is a cohesive software renderer and
does not belong beside grass interaction or weather particles.

## Observer decomposition

The current environment observer performs several unrelated mutations in one
large Bevy system: terrain material refresh, initial foliage creation, sunlight
and fog updates, and weather-particle replacement.

Split this behavior by owner when practical:

- `terrain::on_environment_added`
- `foliage::on_environment_added`
- `environment::on_environment_added`
- `weather::on_environment_added`

Register all observers from `TacticalPresentationPlugin`. Each observer should
respond to the same authoritative `SceneEnvironment` addition and remain
idempotent under the same conditions as the existing combined observer.

If Bevy observer ordering would create an observable behavioral change, retain
a thin coordinator in `mod.rs` for now and delegate to ordinary functions in
the owning modules. Do not introduce ordering assumptions casually.

## Tests

Move each existing test beside the implementation it verifies:

- Weather sunlight/fog/particle tests to `environment.rs` or `weather.rs`.
- Rock containment tests to `obstacles/rock.rs`.
- Grass mesh, LOD, material, and interaction tests to `foliage.rs`.
- Tree hierarchy tests to `obstacles/tree/geometry.rs`.
- Tree bake/LOD tests to `obstacles/tree/impostor.rs` or `tree/mod.rs` according
  to the behavior under test.
- Vista ring tests to `vista.rs`.

Do not weaken assertions merely because private items move. Prefer unit tests in
the owning module over widening production visibility for tests.

## Extraction order

Use small, mechanically reviewable steps where possible:

1. Create `presentation/mod.rs`, move the plugin/configuration facade, and
   preserve existing re-exports.
2. Extract the tree subtree first, because the current uncommitted work is
   concentrated there and it is the largest future conflict surface.
3. Extract foliage so future grass changes have an isolated ownership boundary.
4. Extract vista and rock.
5. Extract terrain, environment, and weather, then split the combined observer
   only after the moved code compiles.
6. Move genuinely shared deterministic helpers last, after actual usage makes
   their correct home evident.
7. Move tests with their implementations throughout rather than leaving all
   tests for a final bulk edit.

`presentation.rs` and `presentation/mod.rs` cannot coexist as definitions of
the same Rust module. Once the new directory facade is ready, remove
`presentation.rs` in the same change that establishes `presentation/mod.rs`.

## Conflict and scope rules

- Preserve all active tree work currently present in this worktree.
- Do not reset, discard, or reconstruct uncommitted changes from committed
  `HEAD`.
- Make the restructuring behavior-preserving.
- Do not modify the grass algorithm or foliage shader as part of this task.
- Do not modify tactical authority, persistence, physics, colliders, or scene
  generation contracts.
- Preserve existing component names because the native tactical scene viewer
  uses them for capture validation and diagnostics.
- Avoid opportunistic formatting or renaming outside the moved code.
- Treat generated SpacetimeDB client bindings as out of scope.

## Documentation and project map

Because this restructuring removes `presentation.rs` and adds several source
files, regenerate the project map after the final layout is stable:

```powershell
python scripts/update_project_map.py
python scripts/update_project_map.py --check
```

No gameplay or architecture documentation should require semantic changes if
the extraction is truly behavior-preserving. Update documentation only where a
page names the old source path directly.

## Verification

At minimum, run:

```powershell
just fmt
just check
just test
python scripts/update_project_map.py --check
```

If the full workspace commands are blocked by an existing baseline failure,
also run the narrowest tactical-client equivalents and report the distinction
clearly. The tactical scene viewer should still compile because it exercises
the presentation marker and provenance interface.

For confidence beyond compilation, run the existing tactical scene capture or
viewer validation used by the tree work if available. This restructuring must
not change rendered output, entity counts, LOD ranges, foliage density, or
validation manifests.

## Acceptance criteria

- No monolithic `presentation.rs` remains.
- Grass changes are localized primarily to `presentation/foliage.rs`.
- Tree geometry and tree impostor baking are separate modules.
- The plugin remains the single public facade for tactical presentation.
- Existing callers continue to resolve the presentation types they use.
- Existing tests retain equivalent or stronger assertions and pass to the same
  baseline as before the move.
- The tactical scene viewer compiles and retains its capture/diagnostic access.
- The generated project map is current.
- No intended visual, gameplay, authority, persistence, or collider behavior
  changes are introduced.
