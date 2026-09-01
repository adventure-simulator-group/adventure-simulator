# Building-editor comparison: The Sims 4 and Fabelgeist

Research date: 20 August 2026

## Decision in brief

Adopt the **interaction language** of The Sims 4 Build Mode—direct selection,
clear tool modes, a focused storey, wall cutaways, immediate previews, undo,
and a small contextual inspector—and retain its freedom to make unusual,
awkward, or unfinished buildings. The semantic, audited 1544 building recipe
should remain a powerful **procedural-generation and analysis system**, not a
requirement placed on player construction. The outcome is a freeform historical
construction workbench, not a modern suburban catalogue.

The current editor is a good starting point for the procedural-authoring path:
it edits a versioned `BuildingDocument`, regenerates derived geometry, and
accepts an edit only after the full structural audit succeeds. The player path
must be separate. It needs direct spatial construction and must save a valid
player document even when no equivalent semantic graph exists. The missing
piece is therefore both an understandable spatial UI and this second authority.

## Sources and scope

This report treats official EA material as useful confirmation of the mode's
existence, but its current page exposes little readable instructional text.
The detailed interaction observations therefore use maintained community
tutorials. The 2014 SimsVIP lesson is particularly useful for the original
Build Mode controls; the Sims Society guide documents later Build Mode
categories including platforms, terrain, ponds, and outdoor plants.

- [EA: The Sims 4 Build Mode](https://www.ea.com/games/the-sims/the-sims-4/new-player-hub/build-mode)
- [SimsVIP: Build Mode Lessons](https://simsvip.com/2014/08/06/the-sims-4-build-mode-lessons/1000/)
- [Sims Society: Build Mode](https://simssociety.wordpress.com/sims4-guides/build-mode/)
- [SheriGR: Wall and Basement Tools](https://sherigr.com/sims-4-building-tutorials/wall-and-basement-tools/)

“Current” below means the native prototype in
`crates/adventuresim-building-generator`, not a promised tactical or strategic
runtime feature.

## Two complementary authoring paths

| Path | Audience and purpose | Source of truth | Validation behavior |
| --- | --- | --- | --- |
| **Freeform Build Mode** | Players building a house, shop, fortress, or delightful architectural nonsense. This is the Sims-like primary UI. | A player-build document containing placed building parts, transforms, materials, attachments, storeys, and site objects. It need not reduce to `BuildingProgram`. | Never reject or discard a player edit merely because it is structurally implausible or not representable by the procedural grammar. Basic engine safety checks still apply. The analyzer may show non-blocking warnings such as “no exterior door” or “upper floor has no support.” |
| **Procedural programme editor** | Developers, world authors, and players who want a historically informed starting point or a generated building family. | The existing `BuildingDocument` / `BuildingProgram` plus ordered semantic edit log. | Keep the present regenerate-and-audit transaction. A failure blocks only this programme edit; it must never invalidate a freeform building. |
| **Bridge, optional in both directions** | “Generate a base, then decorate/rebuild it,” and “analyse this player build.” | An imported snapshot/derived report, not forced conversion. | Export from a programme to freeform is always allowed. Import/recognition from freeform is best-effort and may produce a partial programme plus a list of unrecognised details. |

The essential rule is that **the validator is an advisor for players and a
gatekeeper only for semantic programme generation**. A crooked tower, a complex
roof, or a building with no available graph representation is still a valid
player creation.

## What exists today

The current editor launches as `building-viewer --editor`. It has ten curated
archetypes: civilian houses and civic buildings, a cathedral, four castle
forms, and a 1544 artillery rondel castle. Middle mouse orbits; Shift+middle
mouse pans; the wheel zooms; `F` frames the selected item. Hovered semantic
items receive a grey outline and selected items a white outline.

The only editable targets are walls, openings, and timber members. A selected
wall can receive a window with width, sill, and height values; a selected
opening can be removed. Civilian fixtures can switch the building-wide wall
finish between timber/plaster, plaster, brick, and stone. Timber selection can
switch the building-wide Fachwerk pattern. The Fixtures menu changes the
complete curated program. Undo/redo, JSON save/load, and a failed-edit error
are present.

It does **not** currently offer a plan drawing tool, room resize/move/rotate,
doors or gates as user operations, per-storey editing, floor or ceiling
operations, roof editing, visibility modes, terrain modification, foliage or
prop placement, a furnishing catalogue, fences, or free placement. This is a
substantial current limitation, not a player-facing design constraint: today’s
geometry is derived from an audited semantic program, whereas Build Mode needs
its own freeform saved representation.

The prototype is already ahead of the Sims 4 in important medieval structural
vocabulary. It has grid-anchored **round towers** with integral-cell diameters,
round shells and chord interfaces, spiral/helical vertical connectors, tower
portals, wall walks, defensive circuits, gatehouses, bartizans, hoardings,
machicolations, bretèches, arrow loops, dry ditches, and the artillery rondel
retrofit. The Sims tutorial describes straight, L-shaped, and U-shaped stairs,
but not these tower- and fortification-specific structures. [Sims Society:
Stairs/Ladders](https://simssociety.wordpress.com/sims4-guides/build-mode/#stairs-ladders-railings)

## Fabelgeist-specific structures: “what would Maxis do?”

The Sims pattern is not merely a menu of parts. It is: choose a legible
category, place an intelligible object or footprint, manipulate it with a few
obvious handles, and receive immediate visual feedback. Fabelgeist should use
that pattern for its genuinely different medieval structures. Historical and
structural analysis should enrich this flow with suggestions and warnings—not
replace it with permission checks.

| Fabelgeist capability absent from the Sims tutorial vocabulary | Sims-like freeform interaction | Optional procedural/analysis assistance |
| --- | --- | --- |
| **Round towers** with grid-anchored integral diameters and curved shell/chord connections | `Construct → Tower`: choose Round, Square, or Rondel; drag a circular footprint from its centre. A perimeter ring previews diameter, wall thickness, entrance point, and join points. Selecting it reveals radial handles for diameter, height, roof/crown, and doorway. Freeform mode permits off-grid or unusual diameters. | Offer snap points and a “make procedural” badge when the tower happens to fit the integral-cell recipe. Otherwise retain it exactly and, at most, warn that a curtain-wall join or interior floor is unresolved. |
| **Spiral/helical tower stairs** and tower portals | Selecting a tower shows a vertical “circulation spine.” Click a floor badge to add a spiral segment; drag a landing badge to choose an exit direction. The visual metaphor is the Sims stair handle, but the tower is the host. | Show rise, headroom, landing width, and destination warnings. A visually possible but impractical stair is placeable; a conversion to semantic circulation is available only when it meets the generator's requirements. |
| **Gatehouses, portcullises, draw/deniable bridges, and paired towers** | `Construct → Defence → Gatehouse` offers a compact prebuilt assembly, much like a styled room, which may be stamped onto a wall and then freely adjusted. Players may also build the same gatehouse piece by piece. | The preset creates a coherent semantic assembly. The analyser can identify a hand-built approximation, but it must preserve unrecognised additions rather than flatten or reject them. |
| **Curtain walls, wall walks, battlements, defensive circuits, and tower-top decks** | Drag a curtain wall like a Sims fence; the default preview is a wall-and-walk ribbon. Selecting it offers `Crown`, `Walk`, `Tower access`, and `Gate` chips. A highlighted circuit traces the route across connected walls and decks; an “inside/outside” toggle makes the protected walk side unmistakable. | Flag gaps, inaccessible walks, or disconnected decks for players who care about functional defences. Keep decorative crenellation, broken walks, and experimental layouts placeable. |
| **Hoardings, machicolations, bretèches, bartizans, arrow loops, and artillery gun-loop parapets** | Put these in a `Defence details` subpalette, not the generic wall-decoration catalogue. The pointer preview includes its host face, outward direction, service access, and coverage cone; the inspector offers historically coherent presets plus an advanced freeform transform. | Mark anachronistic, unsupported, or inaccessible choices as historically/structurally questionable; never hide a purchased part because it fails the procedural grammar. Legacy types are available when their renderer exists, even if analysis cannot certify them. |
| **Cathedral bays, apses, bell stages, buttresses, and roof systems** | `Construct → Sacred/Civic` has bay-oriented stamps for nave, aisle, choir, transept, apse, tower, chapel, and sacristy, but each may be pulled apart and combined with ordinary wall/roof parts. | A “recognise as cathedral programme” command can produce a best-effort programme proposal and report ambiguities. It does not replace a bespoke player cathedral. |
| **Timber-frame systems**, jetties, braces, and window-bearing framed bays | The existing building-scope Fachwerk choice becomes a `Frame` swatch strip. Clicking a façade offers `Apply façade`, `Apply storey`, and `Apply building`; individual posts, rails, braces, and jetties remain placeable and transformable. | The programme renderer can generate a structurally coherent frame; the analyser may report unsupported beams or obstructed openings on player work without deleting it. |
| **1544 artillery rondel retrofit**, earth backing, dry ditch, and deniable bridge | Present this as a `Fortification upgrade` card with before/after preview and a small number of era-specific presets—rondel placement, earth backing, ditch, and bridge. All parts are also available from the construction palette for nonstandard layouts. | Context cards explain period fit and recommend clearance/site changes. A player may ignore them; only the optional procedural conversion requires a compatible programme. |

This approach makes the editor feel approachable in the same way Sims does:
direct manipulation first, a small contextual inspector second, and complex
rules expressed as previews. It does **not** reduce a round tower to an
octagonal room or a defensive circuit to a decorative fence.

## Feature assessment

| Sims behavior | Fabelgeist current behavior | Recommendation |
| --- | --- | --- |
| Walls can be drawn individually; the Room Tool draws rectangles, enclosed walls create floors, and selection exposes resize/move/rotate. Ctrl removes a wall and Shift draws a rectangle. [SimsVIP](https://simsvip.com/2014/08/06/the-sims-4-build-mode-lessons/1000/) | Room cells and canonical walls are generated from a `BuildingProgram`; no direct plan tools. | **Implement (P0).** Add freeform Wall, Room, Move, Rotate, Resize, and Remove tools. Grid snapping is the default, not a restriction. Offer “analyse/generate semantic programme” separately for builders who want it. |
| Each floor has a single selectable wall height; half walls have several heights. [SheriGR](https://sherigr.com/sims-4-building-tutorials/wall-and-basement-tools/) | Storey height is program-derived; no partial walls. | **Implement (P1).** Add short masonry/parapet, timber screen, and rail/low wall types with historically grounded presets plus manual height/length controls. Use them for gallery rails, stair wells, market enclosures, battlement parapets, and courtyard divisions. |
| A selected room can build/remove its floor or ceiling; Sims distinguishes storeys. [SheriGR](https://sherigr.com/sims-4-building-tutorials/wall-and-basement-tools/) | Floors and roofs are derived; no room-level control. | **Implement (P1).** Provide freeform floor surface, `open to below`, ceiling, and roof-opening operations. Structural analysis can identify unsupported openings, but may not block the player from making them. |
| Doors/windows are placed on wall segments; windows can be placed by room and adjusted on the wall. [SimsVIP](https://simsvip.com/2014/08/06/the-sims-4-build-mode-lessons/1000/) | User may add/remove windows only; doors, gates, loops, and existing opening profiles are generator-owned. | **Implement (P0).** An Opening tool should choose door, gate, window, shuttered window, arrow loop, shop hatch, and arch. The inspector exposes authentic profiles, closure, sill/head height, and a **By façade / By room** rhythm option. Show circulation, host-strength, and defence concerns as analysis, not a refusal to place. |
| Roof pieces are selected then pushed/pulled for pitch, curvature, and overhang. [Sims Society](https://simssociety.wordpress.com/sims4-guides/build-mode/#roofs) | Typed roof assemblies, dormers, valleys, ridges, and roof recipes exist but are not editable. | **Implement (P1).** Select a roof field and expose eave, ridge, pitch, material, dormer, and gable controls as bounded presets/handles. Do not expose arbitrary cartoon curvature where no 1544 construction system supports it. |
| Stairs snap to rooms, can be moved/rotated/resized, and can receive railings. [SimsVIP](https://simsvip.com/2014/08/06/the-sims-4-build-mode-lessons/1000/) | Stairs are generated, including spiral/helical tower circulation; not editable. | **Implement (P1).** Use a freeform stair tool with straight flight, quarter-turn, half-turn, spiral tower stair, ladder, and exterior steps. Show rise/run, headroom, landing, and destination analysis. This is one place where Fabelgeist should exceed Sims rather than emulate it. |
| Foundations/platforms create split levels and use a height slider. [Sims Society](https://simssociety.wordpress.com/sims4-guides/build-mode/#foundations-platforms) | No editor controls for foundations or grade. | **Implement selectively (P2).** Support cellar, plinth, raised hall, earth-backed rampart, bridge abutment, and terrace as discrete construction types—not unrestricted modern platforms or stilts. |
| Build Mode has fences/gates, columns/spandrels, trims, and wall decoration. [Sims Society](https://simssociety.wordpress.com/sims4-guides/build-mode/) | Gates are derived for fortress programs; no user placement of fences, columns, or trims. | **Implement (P2).** A `Boundaries & supports` palette: wattle fence, palisade, masonry precinct wall, gate, cloister arcade, post, buttress, hoarding support, and scaffold. Offer historically plausible exterior trim only where the construction family permits it. |
| Terrain paint/manipulation and water tools support ponds; a final category places trees, shrubs, flowers, and rocks. [Sims Society](https://simssociety.wordpress.com/sims4-guides/build-mode/#terrain-tools-outdoor-plants) | Building editor has no exterior layer. The wider project has terrain and vegetation data/presentation work, but no building-editor placement authority. | **Implement foliage and site dressing (P1); defer terrain sculpting (P3).** Add a site palette with local tree species, orchard rows, kitchen herbs, vegetable beds, hedges, brush, logs, fieldstone, reeds, and refuse/wood piles. Snap rows and borders, allow modest jitter, and recommend species from the settlement profile without forbidding creative choices. Treat major grade/water changes as world-authoring, not lot decoration. |
| Pools, fountains, water effects, and swimming-oriented decorations are construction categories. [Sims Society](https://simssociety.wordpress.com/sims4-guides/build-mode/#pools-fountains) | No equivalent. | **Do not implement pools.** A well, cistern, trough, millrace, fishpond, moat, fountain, or drainage channel may belong in the site palette, but only when tied to hydrology, construction, or gameplay—never as a suburban leisure pool system. |
| Styled, pre-furnished rooms can be searched, bought, placed, moved, and recolored. [SimsVIP](https://simsvip.com/2014/08/06/the-sims-4-build-mode-lessons/1000/) | Curated fixtures are complete semantic programs; no furniture catalogue. | **Implement as presets, not Buy Mode (P2).** Offer semantic additions such as `bakehouse`, `smithy`, `merchant shop`, `guardroom`, `chapel`, and `stable`, each with required circulation, openings, construction, and occupation hooks. Separate interior furnishing from the building envelope until the tactical/strategic authority is ready. |

## Intentionally out of scope

The following Sims features are poor matches for a 1544 construction editor or
for this prototype's authority model:

- Modern residential catalogue categories: garages, driveways for cars,
  televisions/appliances, plumbing suites, electrical lights as placeable
  objects, carpet, linoleum, modern metal siding, and contemporary balcony
  accessories.
- Leisure pools and swim behavior; use wells, ponds, moats, and waterworks
  only where the settlement and hydrology justify them.
- The Sims' fully modern object catalogue and its simulation-specific cheats.
  Fabelgeist should support free placement and overlap where it is useful to
  player expression, while maintaining engine-safe rendering/collision data and
  offering analysis warnings instead of a blanket construction prohibition.
- A DLC-sized object catalogue, household inventory, currency refund loop,
  Gallery sharing, and Sim-specific door locks. They answer a life-simulation
  use case, not an authored historical settlement.
- Generic “styled rooms” full of furniture. Retain semantic construction
  presets instead.

## Recommended UI

### Screen structure

Use a Sims-like layout, with terms and safeguards that fit Fabelgeist:

```text
┌ File  Undo  Redo │ Select  Construct  Openings  Roof  Site  Finish │ [Search] ┐
│                                                                            │
│  storey / visibility rail              freeform 3D construction canvas    │
│  [▲]  Roof / upper / 2 / 1 / ground    hover = grey, select = white        │
│  [▼]  Walls: Up | Cutaway | Down       advice / warning preview = amber     │
│       Roof: Show | Ghost | Hide                                                │
│       Grid · room labels · structure · foliage · terrain                      │
│                                                                            │
│                              contextual inspector                           │
│                              selected wall / room / stair / roof / site    │
│                              type, material, dimensions, optional analysis  │
└────────────────────────────────────────────────────────────────────────────┘
```

The mode strip replaces a flat property window. Each mode shows a small,
searchable historic palette and a preview attached to the pointer. The
inspector never becomes a catalogue dump: it explains the selected item's
semantic role, available edits, and any reason an operation is unavailable.

### Controls

Retain the existing camera controls and add familiar, discoverable bindings:

| Action | Control | UI feedback |
| --- | --- | --- |
| Select / inspect | `1`, or Select tool; primary click | White outline, semantic name, properties. |
| Construct wall / room / boundary | `2`, then drag; `Shift` constrains rectangle; `Ctrl` changes to removal | Placement preview plus optional amber advice; an unusual but renderable construction still places. |
| Opening tool | `3`, choose opening then click a host | Ghost opening snaps to valid wall field; profile and sill preview. |
| Roof / stair / floor tools | `4`; use visible handles or inspector values | Handles show only legally editable dimensions; rise/run and roof support are visible. |
| Site / foliage tool | `5`; click, drag a row, or paint density | Species, season, clearance, and terrain-suitability preview. |
| Finish/material tool | `6`; click element, Shift applies matching eligible region | Material swatch plus construction-family compatibility. |
| Next/previous storey | `Page Up` / `Page Down`, or storey rail | Active storey bright; lower storeys remain contextual; upper storeys hide. |
| Wall visibility | `Home` cycles Up → Cutaway → Down | Persistent labelled pill, never an unexplained key state. |
| Roof visibility | `R` cycles Show → Ghost → Hide | Ghost roofs remain selectable but do not occlude. |
| Top-down plan view | `T` | Orthographic, active-storey plan with grid and room labels. |
| Frame selected | `F` | Keep the present behavior. |
| Cancel / return to select | `Esc` | Drops preview and returns to Select. |
| Undo / redo | `Ctrl+Z` / `Ctrl+Y` | Shows action label and works for both freeform and programme documents. |

The proposed `Home`, `Page Up`, and `Page Down` parallels are deliberate: Sims
uses floor buttons/`PgUp`/`PgDwn` and a three-state wall view, and its cutaway
hides foreground walls so an interior can be worked on. [SimsVIP: Build
Camera](https://simsvip.com/2014/08/06/the-sims-4-build-mode-lessons/1000/)

### Visibility model

This is the highest-value Sims convention to adopt first. Make it independent
of editing mode and keep it sticky per document:

- **Active storey:** only this storey's plan accepts construction edits by
  default. Storeys above it are hidden; lower storeys are visible but can be
  unlocked if the player wants to make a cross-storey construction.
- **Walls Up:** exterior review and façade work.
- **Walls Cutaway:** hides only view-facing occluders, preserving far walls,
  floors, stairs, and selected items. This is the everyday interior mode.
- **Walls Down:** plan/layout work; render wall footprints/structural lines at
  low opacity rather than making walls vanish without trace.
- **Roof Show/Ghost/Hide:** needed because Fabelgeist roof assemblies and
  tower decks are more structurally meaningful than Sims roof shells.
- **Overlays:** grid and cell coordinates; room/use labels; circulation and
  headroom; load-bearing/support path; openings; defensive circuit; site
  foliage; terrain/hydrology. These are opt-in analytic visual aids, not
  constraints or a second geometry authority.

## Delivery order

1. **P0 — establish player freedom:** introduce a freeform player-build
   document and save/load/undo for direct wall, room, opening, move, resize,
   rotate, and material operations. It must be possible to save a building that
   has no semantic-programme equivalent.
2. **P0 — make it usable:** storey rail, wall/roof visibility, top-down mode,
   named Select/Construct/Opening tools, `Esc`, keyboard undo, and a
   non-blocking analysis panel. Add door/gate/arch parts alongside windows.
3. **P1 — broaden freeform construction:** floors/voids, stairs including
   spiral towers, roof handles, low walls/screens, defensive structures, and
   the ability to disassemble presets into editable parts.
4. **P1 — give a building a site:** foliage, orchard/garden/path/rock dressing,
   and waterwork objects with settlement-derived recommendations. Do not yet
   mutate terrain.
5. **P2 — preserve procedural value:** one-click generated starting buildings,
   best-effort recognition/import, and a separate programme editor that keeps
   its strict audit for users who choose that path.
6. **P3 — only after the site authority is designed:** limited terrain and
   hydrology editing, with explicit handoff to world data rather than a Sims
   lot-sculpting clone.

Freeform operations should commit whenever they are representable by the player
build renderer and save system. Run structural, historical, circulation, and
semantic-recognition analysis asynchronously or on demand, presenting findings
as readable overlays and suggestions. Reserve the existing edit → regenerate →
audit → reject transaction for the explicit procedural-programme editor. This
keeps Sims-like immediacy and player authorship while preserving the project's
distinctive generator as a genuinely useful second tool.
