# Development Workflow

Local workflow for running the Adventure Simulator demo.

## Architecture Overview

The game uses a Mount & Blade style architecture:

1. **Strategic Layer** (SpacetimeDB) - Character progression, inventory, mission tracking
2. **Tactical Spawner** - Watches for pending missions, spawns tactical servers
3. **Tactical Server** (Lightyear) - Real-time 3D gameplay server
4. **Browser Client** (WASM/Bevy) - Connects to tactical server via WebTransport

**Flow:**
1. Player clicks a location in the browser → SpacetimeDB creates a "pending" mission
2. Tactical spawner sees pending mission → starts a adventuresim-tactical-server process
3. Tactical server writes connection info to SpacetimeDB → status becomes "ready"
4. Browser polls and sees "ready" → loads WASM game and connects to tactical server
5. Mission ends → tactical server commits results and exits

## Quick Start

```bash
just dev
```

Open http://localhost:8080

## Full Development (with Tactical Servers)

To run the complete stack with automatic tactical server spawning:

**Terminal 1:** Start SpacetimeDB, the strategic web server, and tactical spawner
```bash
just dev
```

**Terminal 2:** (Optional) Rebuild the WASM client independently
```bash
just build-wasm
```

Now when you click a location in the browser, a tactical server will automatically spawn.

## Requirements

- Rustup (the repository's `rust-toolchain.toml` automatically selects the
  pinned nightly toolchain and required components)
- just (`cargo install just`)
- SpacetimeDB CLI 2.6.1:
  `curl -sSf https://install.spacetimedb.com | bash`, then
  `spacetime version install 2.6.1` and `spacetime version use 2.6.1`
- Python 3
- wasm-bindgen (`cargo install wasm-bindgen-cli`) - for WASM builds
- Caddy (for the HTTPS HTTP/2 development entry point)

## Services and Ports

| Service | Port | Description |
|---------|------|-------------|
| SpacetimeDB | 3000 | Strategic database |
| Strategic web | 8080 | Axum server-rendered browser UI |
| Strategic web HTTPS | 8443 | Caddy HTTP/2 and HTTP/3 entry point |
| Tactical Server | 6000+ | Game server (one per mission) |

## Core Commands

```bash
# Development
just dev              # Start the complete browser stack
just web-secure       # Start strategic-web at https://localhost:8443
just secure-web-trust # Trust Caddy's local development CA (normally once)
just web-damaged      # Start a fresh stack with an injured demo character
just spawner          # Run tactical server spawner
just build-wasm       # Build WASM client

# Testing
just tactical         # Run a single tactical server (for testing)
just status           # Check service status
just stop             # Stop all services

# Workspace verification
just fmt              # Format all Rust workspace packages
just check            # Check all Rust workspace packages
just test             # Test all Rust workspace packages
just lint             # Run Clippy with warnings denied

# Building
just build-tactical   # Build adventuresim-tactical-server and adventuresim-tactical-server-dispatcher
just build-all        # Build everything

# Database
just publish          # Publish SpacetimeDB module
just publish-reset    # Publish and clear database
just generate-db-client # Regenerate and format the Rust client bindings

# World-data source
just init-viabundus   # Download Viabundus v2 CSV data into viabundus/
just normalise-viabundus # Write the 1544 strategic graph to target/
just load-viabundus-world # Load it into a published local SpacetimeDB module
```

`just test` runs the native test suites across the workspace. The
SpacetimeDB module itself targets the SpacetimeDB host ABI, so validate that
crate with `spacetime build`; its pure strategic calculations live in
`adventuresim-core` and are covered by native unit tests. Reducer integration
tests require a running SpacetimeDB environment.

The workspace pins both the module crate and Rust SDK to SpacetimeDB 2.6.1.
Before building or generating bindings, verify the active CLI with
`spacetime --version`. Binding generation uses `spacetime generate
--module-path` and intentionally excludes private tables.

The 1.x to 2.6.1 project upgrade is intentionally a clean reset: the game is
pre-launch, so existing local and deployment data is not retained. Stop the
old server, take an operator backup only if the old data may still be useful,
and move its data directory aside or configure a new empty data directory.
Then install/select SpacetimeDB 2.6.1, start the 2.6.1 server, and run
`just publish-reset` followed by `just _seed-world`. This recreates the schema
and seed data and permanently discards the database's previous contents. Keep
the moved directory until the reset has been validated, then retire it under
the operator's normal backup-retention policy.

After this one-time reset, routine publishes should use `just publish` so data
is preserved. Do not use `publish-reset` on a future public or player-bearing
database unless data loss is explicitly approved and a verified recovery copy
exists.

## Viabundus source data

The Viabundus v2 CSV download is a local development input for the strategic
world importer. It is intentionally ignored by Git. Initialise or restore it
from the official Zenodo record with:

```bash
just init-viabundus
```

Use `python3 scripts/init_viabundus.py --force` only when replacing an existing
local download. The command records the source URLs and SHA-256 checksums in
`viabundus/.viabundus-source.json`.

`just normalise-viabundus` retains active 1544 land and ferry segments, all
nodes needed to connect those segments, and active settlements. It writes a
deterministic, generated artifact to `target/viabundus-v2-1544.json` and emits
a validation report. `just load-viabundus-world` sends that same normalized
data in bounded batches to a published local module. Run it after
`just publish-reset`, without `_seed-world`, when using the historical world.

The loader claims a one-time import identity before sending batches. For a
production deployment, the operator must make that first call before allowing
untrusted clients to connect.

## Strategic UI

The strategic UI is server-rendered by `crates/strategic-web`. Browser clients
receive live state through the web server rather than connecting directly to
SpacetimeDB. The tactical WASM page remains under
`crates/adventuresim-stdb-module/static/tactical.html` and is served by
`strategic-web` at `/tactical/tactical.html`.

Test the server-rendered strategic browser through `https://localhost:8443`
using `just web-secure`. Caddy terminates TLS and negotiates HTTP/2 or HTTP/3
with the browser, then proxies to `strategic-web` on `127.0.0.1:8080`. Run
`just secure-web-trust` once if the browser does not yet trust Caddy's local
certificate authority. Port 8080 remains available for backend diagnostics but
does not exercise multiplexed browser transport.

The certificate-trust recipe runs with PowerShell on Windows even though most
of the development stack currently expects Bash. If Caddy is not on `PATH`, set
`CADDY_BIN` to the executable's full path before invoking it, for example:

```powershell
$env:CADDY_BIN = "C:\tools\caddy.exe"
just secure-web-trust
```

## Tactical Spawner

The spawner polls SpacetimeDB for "pending" missions and starts adventuresim-tactical-server processes:

```bash
just spawner
```

Each tactical server:
1. Starts on an available port
2. Calls `tactical_server_ready` reducer with connection info
3. Runs for 2 minutes (or until mission ends)
4. Calls `commit_mission` reducer with results
5. Exits

## Testing a Single Server

For testing without the spawner:

```bash
just tactical mission_id="test-123" scene_key="town_a"
```

## Troubleshooting

- **SpacetimeDB not running:** `just status`, then `just spacetime-start`
- **SpacetimeDB failed to start:** check `/tmp/adventure-simulator-1/spacetime.log`
- **Tactical spawner can't find binary:** run `just build-tactical` first
- **Mission stuck on "pending":** spawner not running or binary not found
- **Cargo cannot create a temporary `target` directory:** ensure the parent of
  `CARGO_TARGET_DIR` is writable. On Windows or in a restricted sandbox, use a
  workspace-local directory, for example:

  ```powershell
  $env:CARGO_TARGET_DIR = "$PWD\target\verification"
  just test
  ```

`strategic-web` logs every HTTP request at `info` level with a request ID,
method, URI, response status, and elapsed milliseconds. The same request ID is
returned in the `X-Request-Id` response header for correlation with the browser
network panel. Requests abandoned by a navigating browser are logged as
canceled rather than silently disappearing. Set
`RUST_LOG=strategic_web=info` if a shell-level log filter suppresses these
diagnostics.
