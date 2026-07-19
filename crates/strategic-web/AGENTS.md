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
- Service tab = building icon + horizontal-band plinth.
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
  not count toward this range requirement. Monochrome silhouette masks, such as
  building icons, are exempt from the full tonal-range requirement.
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

Store architectural building icons directly under their architectural family
and name them with stable service identifiers:

```text
styles/gothic/church.svg
styles/gothic/inn.svg
styles/gothic/market.svg
```

- Keep the service filename stable across architectural families so the active
  family can be changed without changing service semantics.
- Use a recognizable building silhouette and a consistent view box, baseline,
  visual scale, and stroke or silhouette weight across a family.
- Building icons are monochrome masks colored by CSS. They do not need both
  black and white source values.
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
