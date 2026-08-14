# Development Workflow

For fast puzzle generation, interactive play, structural difficulty sweeps,
and deterministic regression replay without starting SpacetimeDB or the web
stack, use the dependency-light [`puzzle-lab`](puzzle-lab.md) CLI.

## Strategic scenario gallery

Start every in-game strategic fixture with one isolated command:

```powershell
just web-isolated-strategic scenario-gallery 23100
```

The guarded bootstrap always creates the complete gallery; there is no visual
demo flag and no feature-specific profile. Open **Character select**, search
the distinct **Test scenarios** roster, and choose **Select and open**. Each
entry owns a deterministic character and durable state, including disease,
wounds, knowledge, social interactions, autopsy, outbreak, all puzzle kinds,
and every compiled road encounter. Reset the disposable isolated profile to
restore irreversible scenarios.

The **Scenario inspector** lists a bounded union of generated local problems,
open contracts and Order errantries, and registered road encounters. Player-safe
summaries are visually separated from private subject and canonical case IDs.
For a registered recurring-threat scenario, **Trigger next incident / attack**
appends exactly one incident at the official current minute through the same
materializer as scheduled progression; it does not advance the world clock or
run settlement-wide activity.

The **Combat, withdrawal, or surrender** scenario chooses a deterministic
authored-negotiable hostile threat. Follow its ordinary journal and site flow;
at the finale, the enemy page exposes both the existing combat controls and the
hostile conversation dock. The scenario character has enough social training
to exercise acceptance reliably and carries ordinary party rations, filled
waterskins, and a field tent with enough surplus and shelter for the normal
outbound and return journeys. Its generated finale site uses a fixture-only
nearby distance of about 1 km so unrelated random road encounters do not
dominate this focused demo; ordinary generated quests and travel risk are
unchanged. At the hostile site it demonstrates combat, negotiated withdrawal,
and a surrender demand or NPC surrender offer. Reset the isolated profile to
restore the unresolved branch after any outcome. The fixture seeds the
public-awareness threshold used by the authored NPC offer policy, so the offer
mode is directly demonstrable rather than merely theoretical.

Scenario adoption is available only to the registered strategic gateway in a
module compiled with the development capability. Each opaque browser owner
receives owner-scoped access to the shared registered primaries; scenario
characters are not captured by the first browser. Ordinary character grants
remain exclusive. Ordinary builds project no scenario catalog and cannot adopt
or update it.

## Puzzle demo (scenario character)

Use the single scenario-gallery command above, then select **Sigil puzzle**,
**Witness puzzle**, **Rune puzzle**, **Logic-grid puzzle**, or
**Provision puzzle**. Each already owns a deterministic Order-sourced quest,
active journey, persisted road camp, finale hostile, and selected puzzle trial,
then opens
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

Bootstrap reuses the durable puzzle state when invoked repeatedly. The redirect
is read from the safe challenge projection rather than reconstructed by the HTTP adapter. See
[Errantry and modular challenges](errantry-and-challenges.md).

## Outbreak demo

Use the single scenario-gallery command and select **Discovered outbreak**.
Its character owns a deterministic
generated outbreak with private progressing patients, an optional exact-course
disease victim or carrier-autoresolve victim, and an ordinary remediation path.
It prepares the selected character
with Physiology, Surgery, Bestiary, social and urban-investigation skills plus a
surgery kit.

Bootstrap completes the same observer-safe discovery transition as accepting a
local NPC rumor: the selected character starts with the exact problem's receipt,
witness referral, and journal-visible lead/action graph. It also records the dry
notice that makes the case immediately available in the journal index; identical
notice and lead presentation rows are collapsed. Continue through the normal quest,
physiology, surgery, bestiary, and dialogue surfaces. Repeated loading is idempotent
for the scenario character and its dedicated settlement. The gallery is available
only in a development-bootstrap module.

## Autopsy demo

Use the single scenario-gallery command and select **Autopsy**. Its character's
Surgery, Physiology, and Bestiary
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

Bootstrap the isolated scenario gallery, select **Herbalism Demo**,
and use Cut/Grind Edge Actions on its concrete ingredient rows. The character
carries a knife and mortar and pestle so both timing lanes can be compared.
Willow and comfrey demonstrate heated transformations; poppy, tincture spirit,
glass bottles, and a jar demonstrate the six-week passive tincture path.
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

Browser-local mode remains presentation only. The reducer requires the
registered strategic gateway and the compiled development capability, so a
normal module build cannot author arbitrary quests.

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
`just test-strategic-sim`. The isolated
`just strategic-sim-core-loop <new-output-dir>` command installs deterministic
direct-contract and generated-local-problem fixtures for two ordinary parties,
then fails unless both public quest paths and the final survival/return gates
complete for the exact seeded IDs and designated leaders. Aggregate activity or
a safe abandonment cannot substitute for direct accept/travel/encounter/report
and generated discovery/intake/completion evidence. It writes `report.json`
first and an actionable `failure.json` when a
coverage metric is unmet. It also evaluates the authoritative strategic
incident, escalation, recruitment, and quest systems. See
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
exploratory imported-world gameplay evaluation; it deliberately does not force
the deterministic quest fixture. Both recipes create, claim, and delete their own
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
just lint             # Run Clippy for all workspace targets/features with warnings denied

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

The tactical crates currently target the Bevy 0.19 ecosystem. Keep both the
native and explicit Wasm feature builds in the verification set when updating
Bevy, Avian, Ahoy, Replicon, Aeronet, Enhanced Input, or Flair; a successful
native build does not prove the browser dependency boundary.

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

Party enrollment and terminal submission are explicit lifecycle enums. The
frozen resolution and bounded receipt stay together through bounded retry,
acknowledgement, presentation, and shutdown; an ambiguous acknowledgement
timeout fails closed without presenting an uncommitted result.

## Testing a Single Server

### Private animation-graph dependency

The tactical client pins the private
`adventure-simulator-group/bevy_animation_graph` repository by full commit SHA.
Developers must configure a Git credential with read access; `gh auth setup-git`
is the preferred local GitHub CLI integration. If Cargo's embedded transport
does not find that helper, set `CARGO_NET_GIT_FETCH_WITH_CLI=true`.

Use a fresh empty Cargo home to verify both the immutable lock and credential:

```powershell
$fetchCargoHome = Join-Path $env:TEMP ("animation-graph-fetch-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $fetchCargoHome | Out-Null
$env:CARGO_HOME = $fetchCargoHome
$env:CARGO_NET_GIT_FETCH_WITH_CLI = "true"
cargo fetch --locked
```

CI needs a GitHub App token, fine-grained PAT, or deploy credential explicitly
authorized for the sibling private repository. The Fabelgeist
workflow's repo-scoped `GITHUB_TOKEN` cannot read it. Repository secrets are
also withheld from untrusted fork pull requests, so private-fetch verification
must run on a trusted internal branch/check or remain explicitly unavailable
for that fork event.

After changing the canonical unarmed `walk.glb` or `run.glb`, regenerate the
closed runtime cycles and their mirrored comparison clips with:

```powershell
python scripts/build_locomotion_cycles.py
python scripts/mirror_gait_assets.py
python scripts/build_locomotion_cycles.py --check
python scripts/mirror_gait_assets.py --check
```

The generators require Python 3 and NumPy. Runtime locomotion samples the
closed canonical cycle directly; mirrored files remain available for semantic
fallback and comparison. It does not fractionally mirror an FK result.

For normal native tactical development, use the supervised launcher:

```bash
just tactical-play animation
```

The native gameplay client resolves its default Bevy asset source to the
repository's `assets/` directory, independent of the executable location under
`target/`. It validates the required terrain, sky, foliage, tree, and leaf-card
presentation assets before opening a window, so a misplaced asset root fails at
startup instead of silently rendering fallback materials.

For animation-graph authoring, launch the optional native editor from the
repository root:

```powershell
just animation-graph-editor assets
```

The launcher first reports missing optional semantic motion files and validates anchor frame bounds, the
ordinary locomotion route, the raised/right-attack route, and exact/mirrored
catalog fallback resolution. It then validates and queries the same centralized
runtime graph assets used by gameplay for a representative ordinary stride and
right-lead attack. Missing optional files are warnings; invalid anchors, files
required by either deterministic route, and graph load/schema/query failures are
fatal. It prints catalog problems together and does not open the UI with a
broken required route. The editor feature is disabled by default and is
not part of the Wasm or server build.

Use the gameplay capture fixture for deterministic preview evidence rather than
treating the editor viewport as final output:

```powershell
just animation-graph-preview steady-walk-2.0 target/animation-captures/graph-preview
```

Choose any scenario accepted by `animation-viewer`; the output retains its
scenario telemetry, semantic-route counts, `manifest.json`, `failure.txt`, PNG
sequences, and HTML review surface.

Use `just tactical-play diagnostic` to run the same native gameplay client
with a bounded analogue-input script and a per-render-frame animation-state
JSONL log. The generated script, `animation-state-<session>.jsonl`, and process
logs are written to the supervised run directory reported by `just tactical-status`.
The bounded script keeps forward movement held while raising and lowering guard,
so the trace also verifies that raised-stance replication and locomotion remain live.
This is the preferred reproducer when deterministic `animation-viewer`
captures disagree with visible networked gameplay.
Scripted diagnostic mode forces the default third-person camera and suppresses
live keyboard and mouse buttons, motion, and scrolling so activity in another
application cannot alter the capture. Third person is also the normal tactical
client's default; F9 still toggles camera mode outside scripted diagnostics.
Only `diagnostic` enables the per-frame JSONL log by default, supplies scripted
input, and exits automatically. Interactive `animation` and `combat` profiles
avoid unbounded diagnostic files; use the native client's explicit
`--animation-log PATH` option when an interactive recording needs correlation.
On Windows, `presentation_trace=auto` records a
`presentmon-<session>.csv` ETW trace for the bounded diagnostic profile when
PresentMon is installed. Use `presentation_trace=required` for a capture that
must include independent display timing (including an interactive profile), or
`off` to disable it. The JSONL includes wall-clock time, the render thread's
latest render-schedule completion counter, each frame's predicted gait-phase
travel and bounded drift correction, and new authoritative phase measurements.
Only PresentMon confirms that a frame reached the Windows presentation path.
On Windows, the diagnostic profile also uses OBS Studio capture when it is
installed. The supervisor uses a dedicated OBS profile and scene collection,
creates the required capture source, and releases the scripted movement only
after a low-resolution OBS source screenshot contains nonblack pixels and
recording has started. It stops when the input script exits and moves the finalized
video to `<capture-source>-capture-<session>.<extension>` in the same run directory.
This does not control or stop an OBS process that was already
running, and it restores the previously selected profile, scene collection,
scene, and exact WebSocket configuration bytes after capture. Set the sixth
`just tactical-play` argument to `off` to disable capture or `required` to fail
when OBS is unavailable. OBS 28 or newer must have been started once so its
built-in WebSocket configuration exists. The launcher searches PATH and the
standard Windows install location; `OBS_PATH` and `OBS_WEBSOCKET_CONFIG`
override those paths. `OBS_PROFILE` and `OBS_COLLECTION` rename the dedicated
workspace (both default to `Fabelgeist Diagnostics`). The
seventh argument selects `window` (the default Windows Graphics Capture path)
or `display`. Display capture creates a monitor source, automatically matches
the monitor containing the tactical window, and crops it to the client area's
live Win32 coordinates, allowing the final DWM
desktop composition to be compared with the application's WGC surface. The
client is temporarily topmost without being focused during display capture so
activity in another application cannot occlude the measured pixels. The
temporary window source forces Windows Graphics Capture because OBS's
automatic BitBlt choice can return stale frames for Bevy's Vulkan window.
Set `OBS_MONITOR_ID` to an OBS `monitor_id` value when identical monitor names
make automatic selection ambiguous. OBS capture requires no external ffmpeg;
the lifecycle script uses Python's standard library, with `PYTHON_BIN` available
when Python is installed outside PATH.
The fifth `just tactical-play` argument selects `auto-vsync`,
`auto-no-vsync`, `fifo`, `fifo-relaxed`, `mailbox`, or `immediate` for
swapchain frame-pacing comparisons; normal launches default to `auto-vsync`.
The eighth argument selects `auto`, `vulkan`, or `dx12` as wgpu's render
backend. For example, `just tactical-play diagnostic 25020 default off
auto-vsync required display dx12` records a deterministic DX12 Display Capture
without requiring PresentMon.
Pass a fourth argument of `no-shadows`, `no-bloom`,
`no-atmosphere`, or `minimal` to compare GPU-oriented rendering presets; MSAA
is already disabled in every tactical preset.
The normal client uses a 64×64 generated atmosphere environment map. Use
`no-environment-light` to omit it while retaining the visible sky.

This builds the native tactical server and client before creating a mission,
starts a worktree-isolated SpacetimeDB instance, publishes and seeds it, starts
the server, verifies that its one-use claim was consumed and its listener is
owned by the recorded process, and only then launches the native client. The
`animation` fixture disables enemy combat and mission timeout so rendering,
camera, and animation work can remain open indefinitely. `combat` enables
normal enemy behavior. `networking` creates the validated database and server
without launching a client. Tactical-only profiles do not build or serve a
browser/WASM client.

The supervisor prints the database, mission, tactical address, combat mode,
and log directory. Press Ctrl+C in its terminal to stop only children whose
recorded executable and process-start identities still match. Run
`just tactical-status` from another terminal to inspect database,
claim/authority, listener, and client state. After closing only the native
client, run `just tactical-client` to validate the live server and relaunch it.
If the server has died after consuming its claim, relaunch fails with a
recovery command instead of opening a client that remains on `Connecting...`.

The three-terminal workflow below remains available for advanced/manual
debugging. It exposes more lifecycle details and therefore more footguns.

For testing without the spawner:

```bash
just tactical mission_id="test-123"
```

Standalone tactical development defaults to the committed `dense-woodland`
fixture. To exercise the versioned tactical scene boundary with a different
bounded synthetic world input, pass another fixture (or set
`TACTICAL_SCENE_INPUT`):

```powershell
$env:TACTICAL_MISSION_ID = "test-123"
$env:TACTICAL_SCENE_KEY = "grassland"
$env:TACTICAL_SCENE_INPUT = "assets/tactical-scenes/flat-dry-grassland.json"
just tactical
```

The server validates schema/generation versions, dimensions, sample counts,
finite values, geographic and environmental bounds, weather consistency, and
the 32 MiB file cap before opening its game listener. It deterministically
repairs impassable deployment pads and the central encounter corridor, then
logs the stable scene digest and repair counts. If `--scene-input` is omitted,
the server loads `assets/tactical-scenes/dense-woodland.json`; there is no
separate synthetic terrain fallback.

The committed catalog under `assets/tactical-scenes/` covers flat grassland,
steep slopes, dense and sparse woodland, wetlands, cultivated roadside, snow,
heavy rain and wind, distant valley ridges, a narrow LOD-boundary peak, and a
scene requiring playability repair. Their shared test clock and weather
interval start at 10:00 on the canonical fixture day so default interactive
runs use mid-morning light. Regenerate the catalog with
`cargo run -p adventuresim-tactical-core --bin generate-scene-fixtures`; add
`-- --check` in verification. The isolated and supervised workflows accept a
fixture path as their final argument:

```powershell
just tactical-isolated tactical-rain 23200 mission:tactical-rain hills 0 1 assets/tactical-scenes/heavy-rain-high-wind.json
just tactical-play combat 24920 default off auto-vsync off window auto assets/tactical-scenes/dense-woodland.json
```

For deterministic visual review without a database, server, character, or
desktop screenshot tool, capture one fixture through the real tactical scene
presentation plugin:

```powershell
just tactical-scene-capture dense-woodland
just tactical-scene-capture heavy-rain-high-wind target/tactical-scene-captures/rain-review
just tactical-scene-capture sparse-woodland target/tactical-scene-captures/daylight-review 12 340560
just tactical-environment-review target/tactical-scene-captures/environment-review
just tactical-tree-canopy-series target/tactical-scene-captures/oak-canopy-review
just tactical-tree-leaf-benchmark target/tactical-scene-captures/tree-leaf-benchmark 180
just tactical-tree-lighting-benchmark target/tactical-scene-captures/tree-lighting-benchmark 180
just tactical-tree-leaf-comparison target/tactical-scene-captures/tree-leaf-comparison
```

To review production terrain at an actual WGS84 coordinate, first build or
install the final schema-6 terrain routing pack, then pass signed decimal
latitude and longitude:

```powershell
just init-world-runtime
just tactical-real-world-capture 51.7990 10.6170 target/tactical-real-world-captures/brocken
just tactical-real-world-play 51.7990 10.6170
```

For vista publication, prefer the curated coordinate matrix over adding more
synthetic environment fixtures:

```powershell
just tactical-real-world-review
```

It captures a lower Harz forest and the Brocken summit through the production
terrain pack, exercising distant tree canopy, aggregate grass response,
rock/slope material, snow, relief, and all three regional LODs. Synthetic scene
inputs remain useful for focused semantic and failure-mode tests, but they are
no longer the primary vista-quality evidence.

Both commands use `target/strategic-map/terrain-routing-v3.json` and its paired
`.pack` by default; override the final two recipe arguments for another
verified final pack. The materializer rejects coordinates outside that pack.
It calls the same trusted dispatcher sampler as production, producing the
one-metre 101-by-101 playable grid, environmental classifications, weather,
and three peak-preserving vista LODs. The capture records the coordinate,
terrain-package digest, and complete `SceneSource::ImportedPackage` scene in
`input.json`; its five plates cover the ground composition, overhead playable
area, horizon, and unobstructed playable-to-highest and playable-to-lowest
vista transitions. The manifest records minimum, maximum, and total regional
relief so a summit capture cannot be mistaken for flat source data. `play` launches
the ordinary supervised animation demo with the identical immutable scene
document. Coordinates are deterministic inputs; these commands never query a
live map service or silently synthesize missing terrain.

The leaf benchmark forces the dense-woodland fixture through the 8-triangle
cambered PBR card and the 2-triangle flat PBR card at the same ground camera. It
writes `leaf-benchmark.json` and a
`comparison.md` table after per-mode warm-up. These are uncapped end-to-end
frame durations rather than isolated GPU timestamps, so compare modes from the
same run and machine.

The tree-lighting benchmark holds the dense forest, daylight, camera, wind, and
forced LOD0 cambered leaves constant while measuring baseline, canopy ambient
occlusion, directional leaf self shadows, and both effects together. It writes
`tree-lighting-benchmark.json` and `tree-lighting-comparison.md`. The AO mode is
a WebGPU-safe canopy-visibility term rather than native screen-space AO, which
Bevy does not currently expose on WebGPU.

For coordinate-derived demos, attribute production tree and LOD cost with:

```powershell
just tactical-scene-performance-benchmark <input.json> <fresh-output> 120
```

The release-mode timing pass compares the natural scene against hidden
playable/vista trees, playable leaves, grass, understory, litter, loose stones,
procedural rocks, playable terrain, vista terrain, and forced tree LOD0 through
LOD4 at one locked camera. It writes `scene-performance-benchmark.json` and
`scene-performance-comparison.md`. Performance runs use an unpaced headless
schedule and render to an offscreen 2560x1440 texture so desktop-compositor
presentation cannot pin fast scenes to the monitor refresh interval. The
reported end-to-end frame duration still
includes Bevy scheduling, extraction, and render-world synchronization; it is
not interchangeable with summed GPU timestamp duration. The QHD/60 target is
16.667 ms at P95. Reports also retain P99, worst-frame, over-budget count,
budget utilization, and signed headroom. A conclusive pass requires both wall
P95 and summed GPU P95 to fit the budget; without GPU timestamps the report
marks the verdict unavailable instead of treating CPU-side wall timing as a
pass. Run the opt-in `tactical-scene-render-diagnostics` recipe when Metal,
DX12, or Vulkan GPU-pass timestamps and shader invocation counters are needed;
those queries add enough
observer/synchronization overhead that their frame times must not be compared
directly with the timing-only results.

The acceptance target follows [Apple's current MacBook Air specifications](https://www.apple.com/macbook-air/specs/):
the base 13-inch 2026 MacBook Air with Apple M5, a
10-core CPU, 8-core GPU, 16 GiB unified memory, 153 GiB/s memory bandwidth, and
fanless cooling. Run the checked-in dense-woodland acceptance case on that
machine, connected to power, with:

```powershell
just tactical-qhd60-benchmark
```

Its default 600 measured frames per mode plus 180-frame warm-ups sustain the
full production and isolation matrix long enough to expose obvious fanless
thermal decay. The JSON records the actual host OS, architecture, chip/GPU,
memory, target profile, resolution, and whether assertions indicate a release
build. A run on another machine is useful for before/after comparisons but is
not evidence that the MacBook Air target passes. Native Metal/wgpu results are
the deterministic renderer gate; the shipping browser/WebGPU build still
needs an on-device presentation trace before release because browser and
compositor overhead are outside this harness.

To compare environment families on equal ground area, run:

```powershell
just tactical-terrain-density-benchmark
```

This constructs bare, grassland, sparse-woodland, dense-woodland, wetland, and
rocky versions of the same flat 100-by-100-metre production plot. The camera,
QHD target, clear weather, lighting, terrain geometry, and bare vista are held
constant. Each profile runs in a fresh process so scene construction cannot
leak entities or render state between measurements. The aggregate JSON and
Markdown report total GPU time, marginal cost against the bare plot, and
generated trees/scatter normalized to entities per square kilometre. Frame
cost is deliberately reported for the visible one-hectare plot rather than
multiplied by 100: frustum culling, occlusion, LOD, and overdraw make render
cost non-linear with world area. Terrain families whose positive marginal GPU
cost lies within 0.75x–1.25x of the median family are considered balanced;
more expensive families need cheaper assets or lower density, while cheaper
families have room for higher density or detail. This is a diagnostic band,
not a mandate to consume the budget: do not add overlapping scatter or
sub-pixel triangles merely to match a more expensive terrain. Cull or replace
geometry before its smallest faces approach one pixel, and reserve unused
headroom for readable larger features, terrain relief, or future effects.

Loose-stone ground cover follows that projected-size rule through three
production representations: a rounded hero mesh through 5–6 metres, a cheaper
mesh through 20–24 metres, and batched camera-facing pebble quads beyond it.
The billboard shader measures each quad's rasterized diameter and dithers it
away between roughly 1.5 and 0.75 pixels, so differently sized stones disappear
by screen size rather than sharing a visibly abrupt world-distance cutoff.

Use fresh output directories for before/after comparisons. A dense Harz scene
is the representative tree stress case; a sparse summit such as Brocken is a
counter-control for features whose setup cost can outweigh their benefit. The
benchmark both classes after changing tree LOD ranges or render features. For
clean single-mode attribution, set `TACTICAL_BENCH_ONLY_MODE` to the exact mode
label (for example, `Natural production LODs`) and use a fresh process/output.

The leaf comparison recipe renders the cambered and flat PBR foliage from 0,
30, 60, and 80 degree review azimuths under a locked
daylight minute. Use the oblique plates and each run's
`tree-recursive-lod.png` to check depth retention and mixed-sector transitions,
not only the front silhouette.

The 8-triangle cambered card is the close leaf representation and preserves
useful foreshortening and depth when individual leaves are visible. It
transitions directly to the 2-triangle flat card as that camber falls below a
useful screen size. Both production modes use the same aligned CC0 front/back
albedo, normal maps, and opacity mask, so their transition changes geometry
without introducing a material pop.

The default `semantic` profile preserves the native viewer's exhaustive 28
recorded views. The selectable `environment-review` profile is a compact suite
with dedicated, deterministically targeted tree-root, branch-junction, rock,
terrain-grazing, and grass-seam close-ups. Pass repeatable `--view` arguments
directly to `tactical-scene-viewer` for an exact ordered subset; unknown,
duplicate, missing-target, missing-image, or extra-image requests fail closed.
The profile also includes `forest-floor-debris-detail`, a deterministic
39.6-degree vertical-FOV plate 0.36 metres above and 0.92 metres from the
midpoint of the closest rendered leaf-patch/twig-patch pair within 0.55 metres.
The manifest records both subject distances from that midpoint and validation
fails closed unless each is at most 0.275 metres. Sparse woodland requests this plate
in the named morning and grazing runs (and retains it in a full three-time
matrix); validation requires both leaf and twig patches plus visible fine
detail. This is the acceptance view for the 35-metre detailed-litter cutoff.

The native viewer writes standing-eye-height (1.65 m above the sampled local
terrain), overhead, horizon, and collider-overlay PNGs alongside the exact
`input.json`, a browsable `index.html`, and a
machine-readable `manifest.json`. Focused views target the nearest tree,
when present: a complete individual-leaf tree, a canopy-ground view proving the
leaf-litter/grass boundary, a mixed recursive-LOD view, a neutral silhouette plate, an
isolated terminal-shoot close-up with a 10 cm scale marker, and matched-scale
leafed-twig, small-branch, crown-branch, and whole-tree-billboard views. The
tree sequence also includes locked baseline, canopy-AO-only,
self-shadow-only, and combined lighting plates. The
terminal plate uses the exact production branch and leaf geometry for that
seed rather than a separately authored review prop. They fall back to the ordinary ground view
for treeless fixtures. The viewer waits for custom material pipelines before
capturing, then exits unsuccessfully and writes `failure.txt` when
presentation/collider counts, collider-bounded procedural rocks, authoritative
separate dry-leaf and twig scatter for generated trees, terrain
material, coarse-input upsampling, microrelief, expected foliage, rendered
overhead foliage detail in the flat sentinel fixture, all five tree
LODs, precipitation, three vista LODs, the 50 km vista contract, non-uniform
rendered content, or the dedicated boundary-peak view fail. Explicit output directories must be
fresh so a prior capture cannot satisfy a new run accidentally.
The fourth optional recipe argument overrides `absolute_minute` for controlled
lighting comparisons and is recorded in the manifest. The canopy-series recipe
uses local noon by default so the five botanical plates remain inspectable even
when their source fixture represents midnight; pass another absolute minute to
exercise dawn, dusk, or night lighting explicitly.
The canopy series holds the fixture seed, generated obstacles, cameras, and
render settings constant while overriding only the captured
`SceneEnvironment.canopy_bps` to 0, 2500, 5000, 7500, and 10000. Each manifest
records the effective value. This test-only override makes open-grown through
forest-grown tree architecture directly comparable without introducing
neighbour queries or spawn-order coupling into runtime generation.
The manifest also records resolution, settle-frame budget, profile and camera
versions, exact requested views, camera transform/target/up/FOV, review azimuth,
effective world minute, renderer/executable/revision provenance when available,
presentation feature flags, and its capture-clock strategy. The harness advances
Bevy's virtual clock to a fixed two-second wind phase and pauses it before
settling and GPU readbacks, so readback latency does not change foliage pose.
Normal review plates use `TacticalPresentationPlugin::default()` exactly:
shadows, atmosphere/celestials, 64-pixel atmosphere environment-map lighting,
bloom and all three vista LODs are enabled just as in production. Four-sample
MSAA is the WebGPU-compatible anti-aliasing path; SSAO is not part of the
tactical renderer because Bevy 0.19 does not support it on WebGPU. The
manifest records every setting and a production-default-parity gate; the matrix
runner rejects any child whose feature map differs from this contract.
Parity is based on observed runtime state, not only requested configuration:
the manifest records the actual `TacticalGraphicsSettings`, camera environment
map and size, Bloom component, four-sample MSAA, exposure, tonemapping, and final global
ambient color/brightness. The capture harness does not override ambient light;
the production environment observer owns its final value.
After the ordinary warmup, each view performs two consecutive disposable GPU
readbacks and records their mean luminance. Their change must stay within the
larger of 1.5 display-luma levels or two percent; unavailable, non-finite, or
unstable samples fail the explicit lighting-readiness gate before review.
Branch-junction and terrain-grazing diagnostics temporarily suppress production
leaves and grass respectively, retaining production subject geometry/material
and recording the suppression in the capture record.

The matrix runner also treats engine `ERROR` output, including asynchronous
custom-shader compilation failures, as a failed fixture even if the viewer was
otherwise able to write screenshots and exit normally.

Capture the curated environment-only matrix with
`just tactical-environment-review` (`tactical-scene-matrix` is retained as an
alias). It crosses sparse woodland, a rocky open hillside, dry grassland, and a
narrow LOD-boundary peak with morning, grazing, and moonlit named times: twelve
child runs and five sky plates rather than an all-fixture/all-view explosion.
Pass a fresh output
directory as its first argument when a stable path is useful. Repeat
`--fixture` or `--time` directly against `scripts/capture_tactical_scenes.py`
for a smaller A/B matrix. The runner writes an aggregate `manifest.json`, HTML
gallery, and `failure.txt` if any child process, exact-view manifest, semantic
gate, shader log, or Sun/twilight/Moon/stars evidence fails. Weather, water,
clouds, caves, and characters are explicitly outside this review profile.
Semantic gates complement rather than replace rendered-image inspection.
The aggregate verifies fixture, named minute, pipeline/profile/camera versions,
requested and observed production presentation features, per-view lighting
readiness, resolution, exact nonempty PNG sets,
celestial conditions, and one consistent source identity. Identity includes Git HEAD, dirty state, and a SHA-256 of the
relevant presentation Rust, shaders, textures, fixtures, and lockfiles.
`--skip-build` works only when it matches a prior successful build stamp, which
prevents stale binaries being mislabeled while allowing identified dirty runs.

Use [the environment review rubric](environment-visual-review.md) and copy the
checked-in ledger template for severity, cost/benefit triage, independent
reassessment, and stopping criteria.

The focused sky viewer captures the production atmosphere and celestial
materials without loading terrain, foliage, or a tactical server. Its five
canonical views exercise the Sun, a low-Sun detail, twilight horizon, full
Moon, and catalog stars independently:

```powershell
just tactical-sky-capture sun target/tactical-sky-captures/sun.png
just tactical-sky-capture sun-detail target/tactical-sky-captures/sun-detail.png
just tactical-sky-capture twilight target/tactical-sky-captures/twilight.png
just tactical-sky-capture moon target/tactical-sky-captures/moon.png
just tactical-sky-capture stars target/tactical-sky-captures/stars.png
```

Each capture primes one disposable GPU readback after its settle interval,
then writes a 1600x900 PNG from the following stable frame. Inspect all five
images when changing atmosphere, exposure, celestial transforms, or shaders.

Production requests snapshot exact case-site coordinates and character time in
SpacetimeDB. The dispatcher loads the final routing terrain pack once, samples a
one-metre playable grid and three peak-preserving vista LODs out to 50 km,
computes authoritative weather, atomically writes the scene input under
`target/tactical-scene-inputs/`, and launches the child with that file.
Set `ADVENTURESIM_RUNTIME_ROOT` to an absolute writable directory when the
platform's normal per-user runtime directory is unavailable (for example in a
restricted automation sandbox); profile containment checks still apply.

For a self-contained tactical database and request, prefer
`just tactical-isolated`; it writes `.env.tactical` so a subsequent bare
`just tactical` and `just client` target the same isolated instance.

To demonstrate tactical combat presentation without touching the canonical
development database, use three terminals from the repository root. First,
create and seed the disposable mission profile (leave it running):

```bash
just tactical-isolated presentation-demo 23200 mission:presentation-demo woodland 0 1
```

Then start the seeded mission server using the generated `.env.tactical`:

```bash
just tactical
```

Finally connect the seeded Party character:

```bash
just client
```

The generated tactical server claim is single-use. Native client restarts are
safe while that server remains alive, but restarting the server requires a
fresh isolated mission and claim. Keep the `tactical-isolated` owner terminal
running: it owns the database and removes `.env.tactical` during orderly
shutdown. After a reboot or forced termination, do not trust a surviving
`.env.tactical`; `just tactical-status` detects stale supervised state, and
the recovery is `just tactical-play animation`. Launch through repository
recipes so the worktree, working directory, binary, and asset roots agree.

Fight the single enemy or allow the Party character to become incapacitated.
The client shows a centered segmented EGUI incapacitation wheel around the
reticle, alongside the diagnostic live blood-loss/imbalance/incapacitation
status, then an
authoritative `VICTORY` or `DEFEAT` banner after the reducer callback confirms
strategic acceptance.
The server remains connected for the bounded three-second presentation window
and exits automatically. Stop the isolated profile with Ctrl+C in its first
terminal when finished.

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

Select **Social Demo**, open **Greta the Guard**, and open their conversation
from the portrait action. Use **Recent Tidings** for morale concerns and
**Of Thee** for observer-safe questions about Greta.
The fixture includes defeat and injury penalties, established Familiarity,
positive Affinity, exact multi-valued observer beliefs, presentation, and one
deliberately incorrect perceived sensitivity. Greta professes Lutheranism, and
Social Demo has direct Lutheran study plus correlated Catholic knowledge, so
the themed Prayer response is immediately usable. The conversation dock offers
Insight, Charm, Command, Deception, and target-specific Religion; Lighten Mood
and Flirt are distinct Charm
approaches, and repeated supported observations demonstrate the
Transparency-controlled Insight/Deception training split. The bootstrap capability is
compiled only for the isolated workflow; there is no standalone public fixture
reducer. Schema changes are destructive in this pre-launch workflow, so rerun
the isolated profile to recreate its database. Select **Zealous Prayer Demo**
and open **Margareta the Pilgrim** to inspect the same Lutheran action kept
visible, greyed out, and annotated with its unavailable reason.

Encounter development should query `backend_context_characters` by exact
`context_id` or `location_id`. After contextual schema changes, run
`just generate-db-client` before building the web or tactical server.
