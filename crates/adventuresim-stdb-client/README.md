# adventuresim-stdb-client

Auto-generated SpacetimeDB SDK client bindings for the strategic module.

## Generated Code

The `src/` directory contains auto-generated bindings. **Do not edit these files manually.**

Regenerate with:

```bash
just generate-db-client
```

This recipe formats the package immediately after successful generation, so
checked-in bindings remain compatible with the repository's pinned Rust
toolchain. If generation fails, formatting is not run and the recipe returns
the generator's failure.

Or manually:

```bash
spacetime generate \
  --lang rust \
  --out-dir crates/adventuresim-stdb-client/src \
  --project-path crates/adventuresim-stdb-module && \
cargo fmt --package adventuresim-stdb-client
```

Regenerate whenever the SpacetimeDB module schema changes (tables, reducers).
