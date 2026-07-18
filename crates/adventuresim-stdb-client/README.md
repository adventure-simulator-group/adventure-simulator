# adventuresim-stdb-client

SpacetimeDB SDK client bindings for the strategic module.

## Generated Code

The generated files in `src/` are exposed through the handwritten `src/lib.rs`
facade. **Do not edit generated files manually.**

Regenerate with:

```bash
just generate-db-client
```

`just verify-db-client` performs the same generation in a temporary directory,
formats it, and fails on any difference. Native tactical and WASM builds depend
on this non-mutating freshness guard.

This recipe formats the package immediately after successful generation, so
checked-in bindings remain compatible with the repository's pinned Rust
toolchain. If generation fails, formatting is not run and the recipe returns
the generator's failure.

Or manually:

```bash
spacetime generate \
  --lang rust \
  --out-dir crates/adventuresim-stdb-client/src \
  --module-path crates/adventuresim-stdb-module && \
cargo fmt --package adventuresim-stdb-client
```

Use the pinned SpacetimeDB CLI 2.6.1 and regenerate whenever the module schema
changes (tables, views, reducers). Private tables are intentionally omitted.
