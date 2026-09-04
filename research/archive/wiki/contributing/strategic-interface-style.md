# Strategic world interface style guide

These rules apply to the in-world strategic interface: settlements, camps,
quest locations, travel, party views, and the panels and controls used within
those places. They do not apply to character selection or character creation,
even though those pages are implemented in this crate. They also do not apply
to the tactical interface.

Unless a path says otherwise, asset paths on this page are relative to
`crates/strategic-web/`, where the interface and its static files live.

## Design direction

- Make the interface feel like part of the physical game world. Prefer visual
  treatments that suggest a place, building, material, or constructed object
  over generic application chrome.
- A settlement Place Facade should read as the silhouette of the building that
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

## Strategic UI pattern vocabulary

Use the following names consistently in interface discussions, specifications,
CSS, and implementation comments. They describe the strategic layer's spatial
and interaction patterns; they are not necessarily DOM class names.

### World Header

The **World Header** is the semi-diegetic navigation layer at the top of every
strategic page.

- Its upper-left **World Context** always states the current time and place.
- Its upper-right **Character Switcher** is the active character's portrait
  and provides character switching.
- Its central navigation is a row of **Place Facades**. A Place Facade is a
  physical representation of a place the player can use, not an abstract
  application tab. In settlements it normally depicts a building; in the
  wilderness it may depict a party tent or a destination's cave, hut, camp, or
  other point of interest.
- A Place Facade reserves a quiet, low-detail facade field for a standard
  purpose icon. The physical facade may otherwise vary substantially between
  locations. The icon communicates the mechanical purpose, while the facade
  communicates the specific place.

### Side Panels and Scene Stage

The **Scene Stage** is the central strategic canvas below the World Header. It
currently hosts maps and placeholder imagery, and will eventually host small
3D scenes and the characters in them. It is the least utility-critical layer:
overlays may cover it whenever needed.

The **Side Panels** sit below and visually behind the World Header on either
side of the Scene Stage.

- The left **Other Panel** generally presents the counterpart in an
  interaction: an NPC, merchant, or other outside entity.
- The right **Self Panel** generally presents the player-facing side of the
  interaction. In a merchant exchange, party inventory belongs here because it
  is closer to the player than the merchant. When interacting with a party
  member or party inventory, however, that entity belongs in the Other Panel.
- These are spatial defaults, not ownership rules. Inspection and comparison
  views may use both panels for a single person or subject.

### Stage overlays

**Stage overlays** occupy the Scene Stage rather than nesting inside a Side
Panel. They may be opaque or translucent as their purpose requires.

- The **Conversation Dock** is the bottom-centered, resizable chat overlay.
  It may expand upward into the Scene Stage and uses a partially transparent
  surface when that supports readability.
- The **Party Rail** is the top-centered portrait overlay for party members,
  open party slots, the invite control, and party inventory. Hover controls
  emerge below a Party Rail portrait.
- The **Scene Interactable Rail** sits just above the Conversation Dock. It
  contains people, environmental fixtures, evidence, and remains that can be
  acted upon at the current place. A shared card supplies focus, selection,
  label, and icon treatment; each subject keeps only its own actions. A person
  may talk or receive treatment, a fixture may open cooking, a service may open
  crafting, and evidence may expose inspection topics. Its hover controls
  emerge above the card so they do not collide with the Conversation Dock.
- A **Stage Modal** is a menu or dialog opened from a Side Panel. Do not place
  such menus inside the constrained panel that launched them; open them over
  the Scene Stage instead.

### Controls and information patterns

- **Segmented Meters** communicate numeric values primarily through bars, not
  adjacent numeric labels. A meter may stack multiple color-differentiated
  quantities in one track (for example, `x + y + z / total`) and must include
  regular tick marks so each contribution is estimable at a glance on an
  approximately zero-to-five scale. Reveal exact values to one decimal place
  through an Instant Tooltip.
- **Difficulty Bands** are the five stable colors used for skill-meter
  sections. Reuse those colors wherever an immediate skill-check difficulty
  cue is useful, including nested portions of a durability meter that show the
  skill required for repair. Do not use those colors to communicate damage.
- **Tactile Buttons** are raised, three-dimensional controls used for anything
  clickable. Their pressed and unpressed states must remain visually obvious.
- **Instant Tooltips** are styled, immediate hover and keyboard-focus
  descriptions. Do not use delayed, unstyled browser tooltips. Prefer icons
  over word labels for meters and controls where the icon can carry the name;
  the Instant Tooltip supplies that name.
- An **Exchange List** is the shared inventory-list pattern for every item
  exchange: merchant trade, transfers to allies, cooking-pot inputs, and loot.
  Preserve its URL-backed sorting, filtering, column visibility, nested rows,
  offer-acceptance redirects, and row actions across these contexts. In the
  Other Panel, place its scrollbar on the outer edge so it does not interfere
  with row controls.
- **Edge Actions** are the row actions in an Exchange List. They sit at the
  row's inner edge: on the left side of a Self Panel row and the right side of
  an Other Panel row. Activating them expands the row background outward and
  reveals each action as a Tactile Button. Shift applies the action up or down
  to the target quantity; Control applies it to the whole stack regardless of
  target quantity.

### Interaction philosophy

The strategic interface is **power-user-first, hover-revealed**. Optimize for
players who have learned the system, even when that leaves advanced behavior
less immediately discoverable for new players. Do not add persistent
explanatory copy merely to surface every feature: Instant Tooltips, clear
spatial placement, and efficient modifier interactions are the intended path
to learned fluency.

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
- Use frames for Side Panels, Stage Modals, Exchange Lists, Conversation Docks,
  menus, cards,
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
- Place Facade = grayscale building background + separate purpose icon +
  horizontal-band plinth.
- Stage Modal = surface + frame + horizontal band.
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
- Give the Place Facade row the full horizontal World Header width. Place the
  World Context at the upper-left and the Character Switcher at the upper-right
  as high-layer corner overlays; they must remain clickable when Place Facades
  pass underneath them and must not reserve flex space beside the row.
- Position service marks with tier-level custom properties, never per-building
  offsets. Every building in one village, town, or city set must share the same
  mark baseline and size; taller, more prosperous sets may move the whole mark
  row upward so centered-low doors remain clear beneath it.
- Use the original monochrome PNG masks in `static/icons/settlement-services/`
  for settlement services instead of generic inventory icon SVGs. Keep temple
  marks denomination-specific by resolving the same religion asset used by the
  skill menu. Travel architecture is always a freestanding watchtower: a
  modest wooden lookout in villages, a civic masonry tower in towns, and a
  prestigious urban tower in cities. Every tier needs an accessible roof-level
  gallery with open views on all four sides so a watchman can patrol and observe
  through 360 degrees. Keep the enclosed shaft solid and clear for the travel
  mark; do not use defensive wall towers, gates, or attached walls.
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
  quiet enough that the Place Facades remain dominant.
- Reserve the largest uninterrupted facade field for the overlaid purpose icon.
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
