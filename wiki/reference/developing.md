# Development Workflow

For fast puzzle generation, interactive play, structural difficulty sweeps,
and deterministic regression replay without starting SpacetimeDB or the web
stack, use the dependency-light [`puzzle-lab`](puzzle-lab.md) CLI.

## Puzzle demo

Start a disposable strategic-only stack with `just puzzle-demo`. Create or
select an adventurer in a settlement, enable browser-local developer mode, and
choose **Sigil puzzle**, **Witness puzzle**, **Rune puzzle**,
**Logic-grid puzzle**, or **Provision puzzle**. It creates or
reuses a deterministic Order-sourced quest, active journey, persisted road
camp, finale hostile, and selected puzzle trial, then redirects immediately to
the playable chat challenge. This skips
ordinary dialogue acceptance and travel setup. The no-JavaScript form uses
POST/redirect/GET and preserves safe wrong/correct feedback. Background world
updates do not replace the challenge page, and the solved transcript remains
until **Return to camp** is selected. Solving reveals a combat-model-derived
weakness and preparation recommendation without changing enemy statistics;
the optional trial never
blocks **Continue travel**. Rest for at least one hour at that bound camp to
exercise the optional wounded-courier road trial. Aiding him adds his captured
dispatch to party inventory; leaving him or continuing the journey remains
valid.

The loader is idempotent while its puzzle remains open and is disabled in
ordinary module builds. The redirect is read from the safe
challenge projection rather than reconstructed by the HTTP adapter. See
[Errantry and modular challenges](errantry-and-challenges.md).

## Outbreak demo

Start a disposable strategic-only stack with `just outbreak-demo`. Create or
select an adventurer, enable browser-local developer mode, and choose
**Outbreak demo** in the settlement top bar. The loader creates a deterministic
generated outbreak with private progressing patients, an optional exact-course
disease victim or carrier-autoresolve victim, and an ordinary remediation path.
It prepares the selected character
with Physiology, Surgery, Bestiary, social and urban-investigation skills plus a
surgery kit.

The loader intentionally does not reveal the case or write a journal entry.
Ask local NPCs for rumors, then investigate through the normal quest,
physiology, surgery, bestiary, and dialogue surfaces. Repeated loading is
idempotent for the same character and settlement. The reducer is available only
in a development-bootstrap module.

## Autopsy demo

Start a disposable strategic-only stack:

```powershell
just autopsy-demo
```

Create or select an adventurer, enable browser-local developer mode, return to
their settlement overview, and choose **Autopsy demo** in the top bar. The
one-shot loader raises that character's Surgery, Physiology, and Bestiary
skills, supplies a surgery kit, and stages three bodies in the current
settlement: a recent victim, an unidentified interred victim, and an enemy
killed by the party. All physical injuries come from ordinary strategic
autoresolve. The loader adjusts only custody and discovery times to make each
state immediately accessible.

The recent and buried victims share an explicitly bound local family member so
dialogue permission and unauthorized consequences can be exercised. Burning
either victim demonstrates the severe social and reputation penalty; burning
the fallen enemy demonstrates the penalty exemption. Loading is intentionally
one-shot for the selected character. Restart the isolated profile to obtain a
clean run after irreversible actions.

The loader reducer is disabled in normal module builds. Like the existing
developer quest UI, browser-local developer mode only hides the control; do not
deploy a development-bootstrap module to an untrusted environment.

## Herbalism demo

Bootstrap the isolated profile with visual demos, select **Herbalism Demo**,
and open the raised **Herbalism** skill action. Willow demonstrates public
grade and potency, comfrey demonstrates dry/grind and excessive-heat waste,
and poppy tincture demonstrates a strong benefit with a physiological hazard.
The demo also carries tincture spirit so the bounded solvent requirement is
visible in the preview and success path.
The result uses the normal inventory transfer, merchant exchange, and medical
administration paths.

## Item content

Item YAML uses the production build validator. Run `just content-check` for all
compiled core catalogs plus dialogue, or `just content-check items` while iterating on
`content/items/*.yaml`; see
[Item definition authoring](item-authoring.md).

Road/rest encounter YAML is also compiled and validated at build time. Run
`cargo run -p adventuresim-core --bin content-check -- encounters` for its
focused provenance, digest, and semantic checks.

## Organization content

Organization YAML is validated during `adventuresim-core` builds. Validate its
settlement references against the canonical or another exported compiled
Viabundus world with:

```powershell
python scripts/validate_organization_world.py --world path\to\compiled-world.json
```

See [organizations.md](organizations.md) for the schema and authority boundary.

## Developer quest spawning

The existing browser-local developer mode is off by default. On settlement
pages it reveals a top-right quest-authoring button; it never appears at camp
or case sites. The resulting quest remains undiscovered until ordinary
tavern/NPC rumor delivery.

This is not an authorization boundary. The HTTP endpoint and
`spawn_developer_quest` reducer intentionally have no developer credential yet,
so they must not be exposed as an administrative tool on an untrusted
deployment.

The editor, authorization limitation, generated authority, and discovery model
are documented in
[Quest generation and investigation](quest-generation-and-investigation.md).

## Quest content

Quest and bestiary content lives in `content/quests/*.yaml` using the same
strict JSON-compatible YAML convention as dialogue. Validate it with:

```powershell
cargo run -p adventuresim-core --bin questgen-check -- validate
```

The complete authoring and validation contract is documented in
[Quest generation and investigation](quest-generation-and-investigation.md).

## Simulation and quest evaluation

For deterministic multi-year NPC balance experiments and replay commands, see
[`strategic-simulation.md`](strategic-simulation.md), `just strategic-sim`, and
`just test-strategic-sim`. The isolated `just strategic-sim-core-loop <new-output-dir>` command
also evaluates the authoritative strategic incident, escalation, recruitment,
and quest systems. See
[`strategic-simulation.md`](strategic-simulation.md#quest-evaluators).
The separate end-to-end web evaluator is LLM-only and drives the same visible
controls as a player. With a local strategic server running, invoke
`just quest-web-eval quest-browser-run-001`. It saves `index.html`,
`manifest.json`, and a chronological PNG after every action. It requires
Playwright's Chromium browser (`npx playwright install chromium`) and reads the
model credential from `OPENAI_API_KEY` by default.
The opt-in authoritative integration driver is
`just strategic-sim-core-loop-world <new-output-dir>` loads the pinned
`target/world-1544.json` rather than sample/renderer data and is preferred for
gameplay evaluation. Both recipes create, claim, and delete their own
  nonce-named loopback database, compile a one-run bootstrap capability in
  memory, and accepts no host, database, or capability override.

For a fast, credential-free lifecycle rule-composition check, run
`just strategic-sim-lifecycle <new-output-dir> <seed>`. It refuses an existing
directory and writes immutable whole-cadence, daily-cadence, and comparison
reports. This is an offline pure-rule tier; see
[`strategic-simulation.md`](strategic-simulation.md#lifecycle-acceptance-tier)
for its exact coverage and limits.

The current strategic/tactical boundaries and tactical lifecycle are documented
in [Architecture](architecture.md). This page is the canonical home for local
commands, prerequisites, and operator-safe development workflows.

## Quick Start

```bash
just dev
```

Open http://localhost:8080

Ordinary `just dev` / `just web` startup publishes without deleting database
data. Publication failures stop startup before the tactical spawner or web
process starts and print the server/database identity plus recovery choices.
Canonical reset recipes are intentionally disabled.

For a disposable demo or worktree, use an explicit isolated profile:

```bash
just web-isolated renderer-demo 23100
```

The profile name is restricted to lowercase letters, digits, and hyphens, and
the base port must leave room for the web and tactical ports. The recipe derives
a stable fingerprint from the resolved worktree path and includes it in the
database name and profile directory. State lives below the current user's local
runtime/cache directory rather than shared `/tmp`; directories and metadata are
owner-only and symlinks/path escapes are rejected. Thus the same human-readable
profile in two worktrees still has distinct database, data, logs, and process
identities. The three loopback ports remain explicit, and startup fails if any
is already occupied.

The Python lifecycle process holds an exclusive profile lock from the first
port check through web-server exit. It records each child process's resolved
executable and OS creation token, checks that identity throughout readiness and
immediately before reset-publish, and uses the same identity for cleanup. It
will not treat an unrelated listener as its SpacetimeDB or signal a reused PID.
Only this guarded workflow may pass
`--delete-data=always`; it rejects remote servers, non-loopback binds, mismatched
database names, and unsafe profile strings. It stops its own SpacetimeDB and
spawner when the foreground web process exits. The isolated database files are
retained under the fingerprinted profile directory for inspection and are reset
the next time that exact worktree/profile is run.

The public `dev_stack.py publish` command is always non-destructive. Reset
publication is not a CLI option: it is an internal lifecycle operation that
requires the held profile lock and re-verifies the captured standalone listener
identity immediately before invoking SpacetimeDB.

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

## Strategic-Only Development

For strategic-layer work, start SpacetimeDB and the server-rendered browser UI
without building the tactical WASM client or tactical server binaries and
without running the tactical dispatcher:

```bash
just dev-strategic
```

This preserves the canonical local database just like `just dev`. It also
stops a canonical dispatcher left by an earlier full-stack run. Tactical
missions can still enter the pending state, but they will not start until the
full stack is running again.

For a disposable, worktree-safe strategic-only database, use:

```bash
just web-isolated-strategic renderer-demo 23100
```

The isolated lifecycle retains the same guarded reset, ownership checks, and
cleanup as `web-isolated`, but it neither reserves the tactical port nor starts
a dispatcher.

For native tactical testing from WSL on Windows, the equivalent of running
`just dev`, `just tactical`, and `just client 0` in separate Linux terminals is:

```bash
just win-dev
```

This runs the strategic stack in WSL, cross-compiles and stages the tactical
executables in `E:\adventure-sim-dev`, then starts one native Windows tactical
server and client 0. Press Ctrl+C to stop the web process and Windows tactical
processes; the detached SpacetimeDB and tactical dispatcher follow the normal
`just dev` lifecycle and can be stopped with `just stop`. The recipe installs
the pinned toolchain's `x86_64-pc-windows-gnu` Rust target when needed; the WSL
package `gcc-mingw-w64-x86-64` must already be installed.

## Requirements

- Rustup (the repository's `rust-toolchain.toml` automatically selects the
  pinned nightly toolchain and required components)
- just (`cargo install just`)
- SpacetimeDB CLI 2.6.1:
  `curl -sSf https://install.spacetimedb.com | bash`, then
  `spacetime version install 2.6.1` and `spacetime version use 2.6.1`
- Python 3
- Node.js 20 or newer (strategic browser behavior tests)
- wasm-bindgen (`cargo install wasm-bindgen-cli`) - for WASM builds
- Caddy (for the HTTPS HTTP/2 development entry point)

Run `spacetime login` before starting the strategic web stack. Canonical local
startup reads the authenticated token without printing it and passes it only to
the trusted strategic-web child process.

The `justfile` and repository automation do not require Bash. Stateful or
compound recipes are implemented in Python, and simple recipes are compatible
with the host shell. On Windows the default interpreter is `python`; on other
platforms it is `python3`. Set `PYTHON_BIN` when the interpreter has a different
name or lives outside `PATH`.

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
just dev-strategic    # Start only SpacetimeDB and the strategic browser UI
just web-isolated     # Reset and start an explicitly isolated local profile
just web-isolated-strategic # Reset and start an isolated strategic-only profile
just web-secure       # Start strategic-web at https://localhost:8443
just secure-web-trust # Trust Caddy's local development CA (normally once)
just spawner          # Run tactical server spawner
just tactical-isolated # Start a disposable tactical database and request
just client           # Run a native tactical client
just build-wasm       # Build WASM client

# Testing
just test             # Run native Rust/browser tests and validate the SpacetimeDB module ABI
just test-chat        # Run only the strategic chat behavior tests
just test-schedule    # Run only the training-schedule editor tests
just test-dev-stack   # Test local workflow policy without writing bytecode
just tactical         # Run a single tactical server (for testing)
just status           # Check service status
just stop             # Stop all services

# Workspace verification
just fmt              # Format all Rust workspace packages
just check            # Check all Rust workspace packages
just test             # Test native Rust packages and build the SpacetimeDB module
just lint             # Run Clippy with warnings denied

# Building
just build-strategic  # Build the SpacetimeDB module
just build-tactical   # Build adventuresim-tactical-server and adventuresim-tactical-server-dispatcher
just build-wasm       # Build the browser tactical client
just build-all        # Build everything

# Database
just publish          # Publish SpacetimeDB module
just generate-db-client # Regenerate and format the Rust client bindings
just verify-db-client   # Fail if committed bindings differ from the module ABI

# World-data source
just init-world-data  # Install the pinned full input bundle, including Viabundus and HYDE
just init-world-runtime # Install the small compiled world/map runtime bundle
just verify-world-data-bundle /path/to/archive.zip /path/to/archive.release.json <published-descriptor-sha256> # Verify a reviewed input collection
just install-world-data /path/to/archive.zip /path/to/archive.release.json <published-descriptor-sha256> # Install it without source-by-source downloads
just build-base-terrain # Build documented-road-only inference terrain
just compile-world      # Build base terrain, then compile the 1544 world
just build-strategic-map # Build base, world, and final map/terrain artifacts
just normalise-viabundus # Compatibility alias for compile-world
just load-world         # Recreate the canonical local database and load it
just load-world http://127.0.0.1:24610 adventuresim-dev-example # Recreate and load an isolated profile database
```

`just test` runs the strategic browser tests and the native Rust test suites,
excluding `adventuresim-stdb-module`. It also runs `spacetime build` to validate
that module against the SpacetimeDB host ABI. Native linking cannot provide that
host ABI, and including the module would enable its shared schema feature for
the entire workspace. Its pure strategic calculations live in
`adventuresim-core` and are covered by native unit tests. Reducer integration
tests require a running SpacetimeDB environment.

The workspace pins both the module crate and Rust SDK to SpacetimeDB 2.6.1.
Before building or generating bindings, verify the active CLI with
`spacetime --version`. Binding generation uses `spacetime generate
--module-path` and intentionally excludes private tables. Both native tactical
and WASM builds run `just verify-db-client`, which generates and formats into a
temporary directory and compares the result without changing the checkout.

Schema changes are clean pre-launch changes. Regenerate client bindings and
recreate the development database rather than adding a migration or
compatibility path. Routine `just dev`, `just web`, and `just publish` preserve
data. `just load-world` is the explicit destructive exception: it accepts only
a bare loopback server and a lowercase `adventuresim-*` database, reset-publishes
the current module, and discards all existing data before importing the pinned
world. `web-reset` and `publish-reset` remain unavailable; never pass
destructive publish flags manually against a public or player-bearing database
without explicit approval and a verified recovery copy.

`web-isolated` owns its loopback server and database, reset-publishes, reseeds
the normal world and visual test fixtures, and discards only that profile's
contents. Its data remains available in the profile directory for inspection
until the next run resets the same profile.

`bootstrap_development_world` is itself idempotent: it inserts only missing demo
rows. The isolated profile seed workflow then resets `Sick Demo` and its party
of staggered patients plus a high-Physiology physician so symptoms, diagnosis,
and treatment can be tested immediately. It propagates every reducer failure
instead of treating arbitrary errors as evidence that seeding already happened.

Individual fixture reducers are not published. The isolated profile launcher
creates a 256-bit token, compiles it into that disposable module build,
publishes, invokes the single development-bootstrap reducer, and removes the
token from child-process environments. For a manual disposable publish, set
`ADVENTURESIM_DEV_BOOTSTRAP_TOKEN` to a 64-character hexadecimal value while
publishing, then pass the same value to `scripts/dev_stack.py seed --token`.

Spawner metadata contains the resolved repository, profile, server/database,
bind/port configuration, hashes of both tactical binaries, actual executable,
PID, and OS process-creation token. Start, reuse, and stop are serialized under
the profile lifecycle lock. A live process with missing or different metadata
is rejected, so a worktree cannot silently reuse another checkout's dispatcher,
an out-of-date build, or a recycled PID. Confirmed-dead metadata is safely
replaced.

## World data workflow

Most developers should use the pinned compiled runtime rather than download and
rebuild the full geospatial source collection:

```powershell
just init-world-runtime
just load-world
```

`just load-world` installs the runtime bundle when absent, destructively
reset-publishes the current module into the selected loopback
`adventuresim-*` database, and then loads `target/world-1544.json`. This
deliberately discards characters and every other existing row so the database
schema and compiled world always match the checkout. Stop any web process using
the database before loading, then restart it afterward. Pass an isolated
profile's server and generated database name explicitly when targeting that
profile.

### Rebuilding world artifacts

Install the reviewed source-separated input bundle only when changing or
auditing world generation:

```powershell
just init-world-data
python scripts/init_viabundus.py --force
just build-strategic-map
```

The explicit Viabundus initializer adds supplementary upstream inputs, including
`water-1500.csv`, that are not part of the bounded five-CSV bundle component.
Individual source initializers and verifiers remain available through
`just --list` for focused work.

The build chain is:

1. `just build-base-terrain` creates the documented-road-only inference pack.
2. `just compile-world` compiles and validates `target/world-1544.json`.
3. `just build-strategic-map` produces the schema-5 map manifest, AVIF tile
   pack, and final schema-6 terrain-routing pack.

The base terrain pack is an inference input and must not be served. The final
map and terrain artifacts must be distributed with their generated data-license
and source notices.

Source preparation, verification, licensing, and canonical model details live
in the World Data references:

- [World-data bundles](world-data-bundles.md) and
  [Source manifests](source-manifests.md) define release and identity rules.
- [Viabundus](viabundus.md), [Elevation](elevation.md),
  [Historical land use](historical-land-use.md), and
  [Forest cover](forest-cover.md) cover the base geographic inputs.
- [Potential vegetation](potential-vegetation.md),
  [Tree species](tree-species.md), [Soil](soil.md),
  [Geology](geology.md), [Religion](religion.md),
  [Drought](drought.md), and [Hydrology](hydrology.md) cover enrichment stages.
- [Strategic route terrain](route-terrain.md),
  [Industries](industries.md), and [Canonical spatial grid](spatial-grid.md)
  cover derived gameplay facts and shared build identity.

## Strategic UI
The issue #63 cache slice is documented in
[`strategic-read-cache.md`](strategic-read-cache.md), including the explicit
mutable subscription inventory, static/on-demand exclusions, route read
classification, and a deterministic measurement procedure. The procedure
reports unavailable values rather than inventing latency or subscription-byte
measurements when the disposable database fixture is not running.

The strategic UI is server-rendered by `crates/strategic-web`. Browser clients
receive live state through the web server rather than connecting directly to
SpacetimeDB. Current rendering and transport boundaries are documented in
[Architecture](architecture.md).

The strategic UI uses a pseudonymous browser owner, not an account. Set
`STRATEGIC_SESSION_SECRET` to exactly 32 cryptographically random bytes encoded
as unpadded base64url before startup; a missing or malformed secret fails
closed. The signed opaque cookie contains no character IDs. Set
`STRATEGIC_SESSION_COOKIE_SECURE=true` whenever the browser reaches the gateway
over HTTPS. The default `127.0.0.1:8080` bind remains intentional; a
non-loopback development bind must set
`ALLOW_INSECURE_NON_LOOPBACK_BIND=true` and remain on an isolated network.

Test the server-rendered strategic browser through `https://localhost:8443`
using `just web-secure`. Caddy terminates TLS and negotiates HTTP/2 or HTTP/3
with the browser, then proxies to `strategic-web` on `127.0.0.1:8080`. Run
`just secure-web-trust` once if the browser does not yet trust Caddy's local
certificate authority. Port 8080 remains available for backend diagnostics but
does not exercise multiplexed browser transport.

The certificate-trust recipe uses the host's configured command environment.
If Caddy is not on `PATH`, set `CADDY_BIN` to the executable's full path before
invoking it, for example:

```powershell
$env:CADDY_BIN = "C:\tools\caddy.exe"
just secure-web-trust
```

## Tactical Spawner

The dispatcher subscribes to pending SpacetimeDB tactical requests and starts
`adventuresim-tactical-server` processes:

```bash
just spawner
```

Each tactical server:

1. Starts on an available port
2. Consumes its one-use dispatcher claim and registers its server identity
3. Opens the Aeronet WebSocket endpoint and runs until completion or timeout
4. Calls `end_tactical_server` with its terminal resolution
5. Exits after strategic authority validates and commits the durable outcome

## Testing a Single Server

For testing without the spawner:

```bash
just tactical mission_id="test-123" scene_key="hills"
```

For a self-contained tactical database and request, prefer
`just tactical-isolated`; it writes `.env.tactical` so a subsequent bare
`just tactical` and `just client` target the same isolated instance.

## Troubleshooting

- **SpacetimeDB not running:** `just status`, then `just spacetime-start`
- **SpacetimeDB failed to start:** check
  `adventure-simulator-1/spacetime.log` below the platform temporary directory
  (`%TEMP%` on Windows and usually `$TMPDIR` or `/tmp` elsewhere).
- **Tactical spawner can't find binary:** run `just build-tactical` first
- **Mission stuck on "pending":** spawner not running or binary not found
- **Cargo cannot create a temporary `target` directory:** ensure the parent of
  `CARGO_TARGET_DIR` is writable. On Windows or in a restricted sandbox, use a
  workspace-local directory, for example:

  ```powershell
  $env:CARGO_TARGET_DIR = "$PWD\target\verification"
  just test
  ```

  Isolated profiles started with `scripts/dev_stack.py run-profile` also honor
  `CARGO_TARGET_DIR`, which lets multiple worktrees reuse an existing build
  directory instead of compiling the same stack into each worktree.

`strategic-web` logs every HTTP request at `info` level with a request ID,
method, URI, response status, and elapsed milliseconds. The same request ID is
returned in the `X-Request-Id` response header for correlation with the browser
network panel. Requests abandoned by a navigating browser are logged as
canceled rather than silently disappearing. Set
`RUST_LOG=strategic_web=info` if a shell-level log filter suppresses these
diagnostics.

## Physiology key material

The strategic database initializes versioned private Physiology key material
from authoritative runtime randomness. There is no build-time or environment
fallback to configure. Pre-launch schema recreation creates a new population;
causal infection and administration rows pin the versions needed for replay.
See [`physiology.md`](physiology.md) for the privacy contract.
## Social panel demo

Start the isolated strategic stack with the guarded visual fixtures:

```powershell
just web-isolated-strategic social-demo 23100
```

Select **Social Demo**, open **Greta the Guard**, and press the raised Social
icon beside the Morale meter.
The fixture includes defeat and injury penalties, established Familiarity,
positive Affinity, exact multi-valued observer beliefs, presentation, and one
deliberately incorrect perceived sensitivity. Greta professes Lutheranism, and
Social Demo has direct Lutheran study plus correlated Catholic knowledge, so
the themed Prayer response is immediately usable. The Social rail shows
Insight, Charm, Command, Deception, and target-specific Religion; Lighten Mood
and Flirt are distinct Charm
approaches, and repeated supported observations demonstrate the
Transparency-controlled Insight/Deception training split. The bootstrap capability is
compiled only for the isolated workflow; there is no standalone public fixture
reducer. Schema changes are destructive in this pre-launch workflow, so rerun
the isolated profile to recreate its database. Select **Zealous Prayer Demo**
and open **Margareta the Pilgrim** to inspect the same Lutheran action kept
visible, greyed out, and annotated with its unavailable reason.
