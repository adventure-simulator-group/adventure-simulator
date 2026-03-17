# adventuresim-stdb-client

Auto-generated SpacetimeDB SDK client bindings for the strategic module.

## Generated Code

The `src/` directory contains auto-generated bindings. **Do not edit these files manually.**

Regenerate with:

```bash
just generate-db-client
```

Or manually:

```bash
spacetime generate \
  --lang rust \
  --out-dir crates/adventuresim-stdb-client/src \
  --project-path crates/adventuresim-stdb-module
```

Regenerate whenever the SpacetimeDB module schema changes (tables, reducers).
