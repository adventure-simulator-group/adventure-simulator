# strategic-db-client

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
  --out-dir crates/strategic-db-client/src \
  --project-path crates/strategic-db
```

Regenerate whenever the SpacetimeDB module schema changes (tables, reducers).
