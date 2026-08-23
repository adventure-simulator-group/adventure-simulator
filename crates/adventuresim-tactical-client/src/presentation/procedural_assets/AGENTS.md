# Procedural environment art

Environment assets use a realistic-structure, molded-material art direction.

- Keep meshes, silhouettes, normal maps, height maps, and ambient-occlusion maps
  as detailed as the subject and performance budget justify.
- Ground surfaces are the deliberate exception: render them from solid palette
  colors without sampled albedo or stored normal maps. Terrain geometry supplies
  the base normal; the shared packed height/AO texture may add the bounded
  near-field relief defined by the tactical architecture. Gameplay data masks
  may still select hard-edged substrate, cover, water, and snow regions.
- Treat albedo as the color of the material itself, not paint or accumulated
  surface history. Use a small palette of solid color regions with hard
  boundaries. Do not bake gradients, dirt, grime, edge wear, stains, or
  lighting into albedo.
- Give specular or roughness inputs the same low-frequency, low-palette
  treatment as albedo. Fine surface detail belongs in height/normal and AO,
  not in specular noise.
- Prefer deterministic, parameterized recipes over one-off bitmaps. New species
  and substrates should be presets over shared generators whenever their
  structure is related.
- Keep recipe parameters in physical or plainly interpretable terms and test
  that every supported preset is deterministic, bounded, and
  palette-constrained.
- Real reference imagery may be retained when geographic identity matters,
  notably the Moon. Filter or quantize its color response while preserving the
  real landmark layout.
- Lighting, weather, wetness, and canopy shadow may change rendered appearance,
  but they must not be baked into base color.
