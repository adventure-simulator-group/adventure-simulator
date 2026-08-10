# Timber-framed strategic scene assets

The `building/{village,town,city}/{public-square,residences,keep}.png`
facades are project-native assets created for Fabelgeist in July
2026. They were generated deterministically from simple rectangles, polygons,
arches, and scanline fills; no external artwork or generative image model was
used.

Each facade is a 512 × 512 RGBA tint mask using only grayscale RGB values 24,
112, and 220 on visible pixels. The transparent canvas, shared baseline at row
487, and light 125 × 125 semantic-icon field beginning at `(194, 254)` are part
of the UI asset contract. The three settlement tiers use progressively larger
but intentionally restrained silhouettes.

The other assets in this directory predate this project-native facade set and
retain their existing provenance records elsewhere in the repository.
