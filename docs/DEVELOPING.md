# Development Workflow

Local workflow for the Axum + Maud + SQLx rewrite.

## Requirements

- Rust toolchain
- `just` for recipes
- `wasm-bindgen` for browser tactical builds

## Quick Start

```bash
just web
```

Open `http://localhost:8080`.

`strategic-web` runs migrations and seeds the default world on startup. It uses
`sqlite://adventuresim.db` unless `DATABASE_URL` is set.

## Full Browser Tactical Flow

```bash
just build-wasm
just web
```

Then in the browser:

1. Create or select a character.
2. Create or join a party.
3. Accept a quest as party leader.
4. Use **Enter Mission** from the party page.
5. Wait for mission status to become ready.
6. Use **Connect to Mission**.

The tactical browser client connects directly to the spawned tactical server.

## Useful Commands

```bash
just web              # Run strategic-web and spawn tactical servers from mission requests
just build-tactical   # Build adventuresim-tactical-server
just build-wasm       # Build browser WASM client into strategic-web/static/tactical/wasm
just tactical         # Run one standalone tactical server for manual testing
just client           # Run native tactical client against just tactical
just check            # cargo fmt --check and cargo check --workspace
```

## Ports

| Service | Default | Description |
| --- | --- | --- |
| strategic-web | 8080 | SSR UI and internal tactical callbacks |
| tactical server | chosen dynamically | One process per mission |
| standalone tactical test | 6000 | Used by `just tactical` |

## Troubleshooting

- **Mission stuck preparing**: ensure `adventuresim-tactical-server` is built or set `TACTICAL_SERVER_BIN`.
- **Browser cannot connect**: restart `just web` so the local tactical server address is regenerated.
- **Internal callbacks fail**: set `STRATEGIC_INTERNAL_URL` to the URL tactical server processes can reach.
- **Unexpected seed data**: restart `just web`; it resets the local SQLite database before startup.
