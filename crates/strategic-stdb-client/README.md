# strategic-stdb-client

Auto-generated SpacetimeDB SDK client bindings for the strategic module.

## Generated Code

The `src/` directory contains auto-generated bindings. **Do not edit these files manually.**

Regenerate with:

```bash
just generate-stdb-client
```

Or manually:

```bash
spacetime generate \
  --lang rust \
  --out-dir crates/strategic-stdb-client/src \
  --project-path crates/strategic-server/strategic-stdb-module
```

Regenerate whenever the SpacetimeDB module schema changes (tables, reducers).
