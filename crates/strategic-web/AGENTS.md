# Strategic world interface style guide

These rules apply to the in-world strategic interface: settlements, camps,
quest locations, travel, party views, and the panels and controls used within
those places. They do not apply to character selection or character creation,
even though those pages are implemented in this crate. They also do not apply
to the tactical interface.

## Design direction

- Make the interface feel like part of the physical game world. Prefer visual
  treatments that suggest a place, building, material, or constructed object
  over generic application chrome.
- A settlement service tab should read as the silhouette of the building that
  houses the service, rather than as an abstract symbol for the activity. For
  example, use an inn silhouette for the inn instead of a beer stein.
- The selected service establishes the architectural treatment for the
  surrounding interface. Style the left and right panels as parts of the same
  building represented by the active tab.
- Treat the time-of-day background and tint as environmental lighting. It may
  recolor scenery, architectural ornament, and other ambient surfaces, but it
  must not make controls or text difficult to read.
- Existing interface art is largely placeholder material. Introduce richer
  raster textures or detailed SVG artwork incrementally without changing
  layout or interaction behavior unless the task calls for it.

## Readability and accessibility

- The in-world strategic interface is textually dark-mode-first: normal text is
  light and the surface immediately behind text is dark, including during the
  daytime.
- Light scenic or ornamental backgrounds are allowed, including behind service
  tabs during the day. Place readable text on a sufficiently dark and opaque
  local surface rather than relying on the surrounding scene to provide
  contrast.
- Preserve accessible names and visible labels. Treat purely ornamental images
  as decorative, and do not communicate state through color or texture alone.
- Verify that time-of-day tinting, hover states, selected states, and disabled
  states retain readable contrast.

## Interface copy

- When creating or modifying an interface element, do not put explanatory copy
  inside the interface describing what the element does, why it exists, or what
  changed. The interface should communicate its purpose through its label,
  placement, controls, state, and visual design.
- When a short explanation is genuinely useful for discoverability, provide it
  as a tooltip where appropriate. Keep the element's accessible name or
  description equivalent, and do not use a tooltip as a substitute for a clear
  visible label.
- Functional content remains appropriate: show values, current state, action
  results, validation errors, and instructions required to complete an action.
  Do not present implementation commentary, release-note language, or a prose
  walkthrough inside the element.

## Component vocabulary and asset paths

Store architectural assets below `static/styles/` using this layout:

```text
styles/<architectural-family>/<component>/<variant>/<part>.<ext>
```

For example:

```text
styles/gothic/frame/relief/top-left.png
```

- `<architectural-family>` names a reusable architectural family such as
  `gothic`, `romanesque`, or `timber-framed`; it is not a service or building
  name.
- `<component>` names a reusable UI component such as `frame`, `surface`, or
  `vertical-support`.
- The component's **anatomy** is its standard set of named parts and the rules
  for how those parts attach, scale, and repeat.
- `<variant>` names a **component skin**: an interchangeable visual treatment
  that obeys the component's anatomy and geometry contract.
- `<part>` names one role in the anatomy, such as `top-left`, `middle`, `shaft`,
  or `bottom`.
- Use lowercase kebab-case for directory and file names.

A component anatomy is a compatibility contract, not merely a category. Every
skin of a component must use the same part roles, attachment geometry, scale,
alignment, and repeat behavior. Establish and document equivalent seam and
tiling rules before adding skins for a new component. Only introduce a new
component when the required geometry or assembly differs from an existing
component; a different appearance belongs in a new skin.

## Starter component anatomies

Begin with this small set. Add niche components only after a repeated interface
need demonstrates that these anatomies cannot express the required geometry.

### Surface

```text
surface/<variant>/tile.png
```

- `tile` must repeat seamlessly both horizontally and vertically.
- It must not contain a baked border or imply an outer edge.
- Use surfaces for stone, plaster, wood, cloth, and other arbitrary-area fills.
- Keep surfaces independent from frames so either can be changed without
  requiring a new combined asset.

### Frame

Use a conventional hollow 9-slice anatomy:

```text
frame/<variant>/top-left.png
frame/<variant>/top.png
frame/<variant>/top-right.png
frame/<variant>/left.png
frame/<variant>/right.png
frame/<variant>/bottom-left.png
frame/<variant>/bottom.png
frame/<variant>/bottom-right.png
```

- Corners remain fixed while edge pieces repeat along their corresponding axis.
- The center is normally transparent or omitted because a `surface` supplies
  the interior. Add a center part only when the artwork genuinely requires it.
- Use frames for side panels, dialogs, inventories, chat areas, menus, cards,
  and similar bounded regions.

### Horizontal band

Use a horizontal 3-slice anatomy:

```text
horizontal-band/<variant>/left.png
horizontal-band/<variant>/middle.png
horizontal-band/<variant>/right.png
```

- The caps remain fixed and `middle` repeats horizontally.
- Use horizontal bands for headers, footers, lintels, beams, and service-tab
  plinths. Do not name an asset after only one of those uses.

### Vertical support

Use a vertical 3-slice anatomy:

```text
vertical-support/<variant>/top.png
vertical-support/<variant>/shaft.png
vertical-support/<variant>/bottom.png
```

- `shaft` repeats vertically and must join seamlessly to itself and both caps.
- Use vertical supports for columns, pilasters, posts, poles, side rails, and
  other load-bearing or boundary-like treatments.

### Divider

Use a horizontal 3-slice anatomy:

```text
divider/<variant>/start.png
divider/<variant>/middle.png
divider/<variant>/end.png
```

- `middle` repeats horizontally between fixed terminals.
- Use dividers for thin separators within panels, lists, and grouped controls.
- If vertical dividers become a repeated need, introduce a separate
  `vertical-divider` anatomy. Do not rotate shaded raster artwork, because that
  also rotates its apparent lighting.

### Ornament

```text
ornament/<variant>/ornament.png
```

- An ornament has no seam contract. Document its anchor, intended scale range,
  overlap behavior, and whether mirroring is permitted.
- Use ornaments for crests, bosses, brackets, reliefs, flourishes, cracks, and
  other characterful details that do not define component geometry.
- Treat arches, windows, doors, scrollwork, chained borders, and irregular
  masonry as ornaments or compositions initially. Promote one to a component
  only when several interfaces require the same assembly contract.

## Composition

Build higher-level interface elements by composing the starter components
rather than creating a new asset anatomy for every use:

- Panel = surface + frame + optional ornament.
- Sidebar = surface + frame or vertical supports.
- Header = surface + horizontal band + optional ornament.
- Service tab = grayscale building background + separate service icon +
  horizontal-band plinth.
- Dialog = surface + frame + horizontal band.
- List section = surface + dividers.

These names describe implementation compositions, not additional asset
directories. Preserve the independence of their constituent skins so they can
be mixed and matched within an architectural family.

## Texture requirements

- Keep source textures grayscale so CSS can supply contrast, brightness, hue,
  and environmental tint at runtime.
- Every shaded texture must contain both pure black and pure white among its
  visible pixels, with useful tonal detail between them. Transparent pixels do
  not count toward this range requirement. Monochrome silhouette masks and the
  three-tone service-building backgrounds specified below are exempt from the
  full tonal-range requirement.
- Prefer lossless PNG for raster textures that require alpha and SVG for
  artwork that benefits from resolution-independent detail. Do not use JPEG
  for modular interface textures.
- Preserve transparent backgrounds where the component is intended to layer
  over another surface.
- Validate every required seam at its rendered size and verify repeatable
  components across more than one repetition. Do not hide a broken seam with a
  one-off offset that prevents the variant from being interchangeable.
- Apply color and contrast in CSS. Do not create separately colorized copies of
  the same source texture for times of day, services, hover states, or selected
  states.

## Building icons

### Art direction

Treat service buildings as precise cut-paper compositions rather than miniature
architectural illustrations. The intended result should look as though a small
number of sheets of colored paper were cut and aligned with machine precision:
flat, hard-edged, restrained, and slightly abstract. It must not read as a cozy
cartoon village, a textured painting, or a detailed model building.

- Construct each building from a small number of clean geometric shapes. Do
  not use gradients, bevels, cast shadows, highlights, material noise, paper
  fibers, weathering, or painterly marks.
- Do not depict brick courses, individual roof tiles, wood grain, timber or
  masonry patterns, or repeated surface linework. Architectural identity comes
  from the silhouette and a few large structural shapes.
- Give every building exactly three architectural tone roles: a light wall; a
  noticeably darker roof, column, chimney, steeple, or structural shape; and a
  near-black door, window, or narrow opening. The runtime tint supplies the hue.
- The secondary tone should read about 30–40% darker than the wall at the
  smallest supported tab size. Do not rely on a hue shift for separation.
- The pale service mark, notification badge, focus treatment, and selected
  underline are separate interface overlays and do not count against the
  building's three architectural tones.
- Keep the complete service mark inside the building silhouette, centered on
  the reserved facade field rather than floating above or obscuring the roof.
  Scale and space the buildings generously enough that the mark remains large
  and legible within that field at the final rendered tab size.
- Give the service navigation the full horizontal header width. Place the
  current location and time at the upper-left and the character profile control
  at the upper-right as high-layer corner overlays; they must remain clickable
  when service buildings pass underneath them and must not reserve flex space
  beside the building row.
- Position service marks with tier-level custom properties, never per-building
  offsets. Every building in one village, town, or city set must share the same
  mark baseline and size; taller, more prosperous sets may move the whole mark
  row upward so centered-low doors remain clear beneath it.
- Use the original monochrome PNG masks in `static/icons/settlement-services/`
  for settlement services instead of generic inventory icon SVGs. Keep temple
  marks denomination-specific by resolving the same religion asset used by the
  skill menu. Travel architecture is always a gatehouse: a fence-gate shack in
  villages, a civic gate in towns, and a fortified urban gate in cities.
- Apply the same time-of-day lighting value to both the tinted building raster
  and its service mark. Keep them as separate layers, but do not let the mark
  remain at full daytime brightness while the architecture darkens.
- Render the overlaid service SVG as one solid pale mask. Ignore any source SVG
  fills, strokes, or internal black-and-white treatment; time-of-day lighting
  may change the mask's brightness, but it must remain a single flat tone.
- Keep the rendered architecture materially darker than the pale service mark
  at every time of day. Environmental lighting affects both layers, but a
  separate fixed building darkening pass should preserve facade contrast.
- Prefer simple gable roofs: two pitched planes meeting at a ridge, like a
  precisely folded sheet of paper. Use hipped or pyramidal roofs sparingly for
  justified variation. A row should be predominantly gabled.
- Draw every service building as an orthographic front elevation, square to the
  camera. Center ordinary gable peaks over their facades and keep the two roof
  slopes visually balanced. Do not show side walls, receding ridges, or
  three-quarter perspective; silhouette variation must remain front-aligned.
- Keep settlement horizon art in a separate transparent layer behind the
  service buildings. Compose it from nearby settlement fabric in front of a
  more distant skyline so the service row reads as part of the place, not as a
  strip of buildings standing outside it. Confine roofs, lanes, quays, trees,
  and church silhouettes to the lower portion so the runtime sky remains
  visible, and apply the same time-of-day brightness variable to the horizon
  layer.
- Store horizon variants at
  `styles/timber-framed/background/<village|town|city>/<inland|coastal|river>.png`.
  Every horizon is a 2880-by-240 transparent RGBA panorama with subdued
  grayscale scenery and a shared bottom baseline. Meaningful settlement
  silhouettes must reach both horizontal edges above that baseline; do not
  place the town on a central island and bridge the sides with flat terrain or
  water filler. Render it proportionally with `cover`, centered at the bottom;
  never force it to `100% 100%`. Wider viewports may clip the sides, but must
  not stretch landmarks or expose visibly simpler edge bands.
- Inland village horizons may use fields and roads beyond nearby buildings;
  town and city horizons use rooflines, streets, and courtyards immediately
  behind the service row. River horizons use a lateral water band plus a
  tier-appropriate bridge, quay, or mill; coastal horizons use a Baltic
  shoreline plus tier-appropriate sheds, wharves, masts, or warehouses. In a
  city, water belongs behind a continuous built-up quay rather than between the
  viewer and an isolated distant skyline. Keep water shallow and the center
  quiet enough that the service tabs remain dominant.
- Reserve the largest uninterrupted facade field for the overlaid service mark.
  Place doors beside that field, normally at a lower outer corner, rather than
  centered beneath it; this keeps the building low and the mark large.
- Treat town and city service buildings as restrained backlit silhouettes.
  Below the roofline, keep the facade as one uninterrupted wall plane: do not
  add floor beams, half-timber grids, pilasters, moldings, or clipped support
  fragments. Reserve secondary shading for continuous roof and outer-contour
  shapes, and reserve the darkest tone for doors and windows. This prevents
  structural lines from appearing to stop abruptly around the service mark.
- Keep openings sparse: normally one doorway and at most one additional window.
  Market stalls may use open bays and supports instead of a door.
- Vary silhouettes with a few historically grounded cues appropriate to circa
  1544: an open market roof, smithy chimney, broad inn, or church bell-cote.
  Avoid fantasy towers and later monumental forms. Historical grounding should
  affect the large shapes, not introduce surface detail.

Settlement scale and means change proportions and construction type, not the
fundamental graphic vocabulary:

- A small, relatively poor village uses low cottages, sheds, open stalls, squat
  workshops, a broad but modest inn, and a small chapel. Most facades read as
  one story. Show limited means through scale and simpler construction, not
  dirt, damage, broken roofs, or comic destitution.
- A medium town may use compact two- or three-story guildhouses, workshops,
  market halls, and a modest late-Gothic church. Keep the strip compact enough
  for the existing location header; it is not a town panorama.
- A city may use taller, denser merchant houses, masonry civic buildings,
  larger halls, and a more prominent church, while retaining the same sparse
  geometry and tab-scale legibility. Monumentality comes from massing, not
  extra surface marks.
- Within one set, keep facade detail, service-mark scale, baseline, edge weight,
  and tone contrast consistent. Buildings may vary in width and roofline, but
  one service should not appear to belong to a different art system.

### Asset and color contract

Store transparent building backgrounds under their architectural family, tier,
and stable service identifier:

```text
styles/timber-framed/building/village/inn.png
styles/timber-framed/building/town/inn.png
styles/timber-framed/building/city/inn.png
```

- Keep the service filename stable across families and tiers. Use a consistent
  512-by-512 transparent RGBA canvas, bottom baseline, padding, visual scale,
  and silhouette weight across a set.
- Source building backgrounds contain no service symbol. The existing local
  Game Icons SVG is a separate semantic layer superimposed by the interface;
  this also lets religion select its faith-specific SVG dynamically.
- Visible source pixels are grayscale and use exactly three RGB values for the
  architectural tone roles. Preserve alpha antialiasing at edges. CSS combines
  that luminance with `--building-tint`; do not commit separately colorized
  copies or bake settlement, time, selection, or notification state into PNGs.
- `Unknown`, `Hamlet`, and `Village` use the village tier; `Town` uses town;
  `City` and `Capital` use city. Define the village URL as each service's CSS
  baseline. Add town or city overrides one service at a time only when the
  corresponding asset exists, so an incomplete higher-tier set falls back to
  village without requesting a missing file.
- The horizon tier follows the same category mapping. Until imported hydrology
  is available, the server emits a stable settlement-ID-derived inland,
  coastal, or river variant. Keep that temporary selector centralized so the
  imported dataset can replace it without changing markup or CSS contracts.
- These three-tone service backgrounds are exempt from the general texture
  rule requiring pure black, pure white, and intermediate detail.
- Continue to use the locally vendored Game Icons collection in
  `static/icons/game/` for appropriate non-building interface symbols, as
  directed by the repository-level `AGENTS.md`.

## Implementation expectations

- Keep architectural family, component skin, service, interaction state, and
  time-of-day lighting as separate inputs. Do not bake one family, service, or
  time of day into otherwise reusable markup.
- Prefer CSS custom properties and composable classes for selecting assets and
  applying color treatment.
- Ensure ornamental layers do not intercept pointer input or obscure focus
  indicators.
- Check the result at supported desktop and narrow viewport layouts. Cropping,
  repeating, or hiding ornament must not move or cover functional controls.
- Record the origin and license of third-party assets in the repository's
  applicable attribution or third-party notice file.
