# Procedural texture agent guide

- Keep texture recipes renderer-independent. A recipe may create Bevy `Image`
  values or handles, but must not construct tactical materials or scene entities.
- Give each independently iterated texture its own source file. Shared sampling,
  mip, packing, and image helpers belong in narrowly named shared modules.
- Preserve stable `TextureRecipeId` slugs. Add real generation before changing a
  catalogue entry from `Planned`; do not add placeholder checkerboards.
- Every recipe needs deterministic tests for repeatability, edge tiling when
  applicable, physical feature scale, channel packing, and mip completeness.
- Review through `procedural-texture-lab`. Export only the recipe being changed
  and keep generated PNGs under `target/`, never in source control.
- A texture-focused change should not tune another recipe. If shared helpers
  must change, prove unchanged outputs or coordinate that change separately.

## Review commands

```text
cargo run -p adventuresim-procedural-textures --bin procedural-texture-lab -- list
cargo run -p adventuresim-procedural-textures --bin procedural-texture-lab -- export oak-bark
cargo test -p adventuresim-procedural-textures
```
