# Strategic interface agent guide

These rules apply only to the in-world strategic interface: settlements, camps,
quest locations, travel, party views, and their controls. They do not apply to
character creation, character selection, or the tactical interface.

Before changing this interface or its art, read
`../../wiki/contributing/strategic-interface-style.md`; it is the canonical
source for the project's pattern vocabulary, asset anatomy, and art direction.

- Make controls feel like physical parts of the game world rather than generic
  application chrome. A Place Facade depicts the place itself; its separate icon
  communicates purpose.
- Keep text on dark, opaque local surfaces even when scenic backgrounds are
  light. Verify contrast and controls at desktop and narrow viewport widths.
- Use Tactile Buttons for actions and Instant Tooltips for immediate hover and
  keyboard-focus descriptions. Preserve accessible names, visible labels, focus
  indicators, and non-color state cues.
- Do not add explanatory or release-note copy inside an interface element.
  Present only labels, state, results, validation errors, and instructions
  needed to complete the action.
- Keep architectural family, component skin, service, interaction state, and
  time-of-day lighting as separate inputs; do not bake reusable state into
  markup or assets.
- Assume the in-world interface runs in one game tab and may spend 10--20
  seconds on its initial renderer and asset load. Preserve one fullscreen Bevy
  canvas, Wasm instance, WebGPU device, and asset cache for the lifetime of the
  strategic document; scene changes must not recreate them.
- Bevy owns continuous 3D presentation and spatial interaction. HTML owns
  document-like panels, forms, dialogue, and accessibility-critical controls.
  Egui may be used for canvas-native HUD and spatial interactions, but must not
  introduce a second strategic authority or duplicate an HTML workflow without
  a specific interaction need.
- Synchronize canvas and HTML interactions through typed semantic commands and
  stable domain IDs. Strategic state remains authoritative in the existing
  server and SpacetimeDB flows, not in either presentation layer.
- Store third-party asset provenance in the applicable attribution file and
  `../../THIRD_PARTY_NOTICES.md`.
