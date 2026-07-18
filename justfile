# Adventure Simulator - local development
# Install just: cargo install just
# List commands: just --list

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

spacetime_port := "3000"
web_port := "8080"
secure_web_port := "8443"
tactical_port := "6000"
tactical_web_port := "6001"
public_bind := "0.0.0.0"

spacetime_url := "http://localhost:" + spacetime_port
spacetime_module := "adventuresim-stdb-module"
spacetime_version := "2.6.1"

strategic_dir := "crates/adventuresim-stdb-module"
strategic_static := strategic_dir + "/static"
strategic_web_dir := "crates/strategic-web"
caddy_config := "Caddyfile.dev"
caddy_bin := env_var_or_default("CADDY_BIN", "caddy")

run_dir := "/tmp/adventure-simulator-1"
spawner_pid := run_dir + "/spawner.pid"
spawner_log := run_dir + "/spawner.log"
stdb_pid := run_dir + "/spacetime.pid"
stdb_log := run_dir + "/spacetime.log"

cert_dir := "utils"
cert_pem := cert_dir + "/cert.pem"
cert_key := cert_dir + "/key.pem"

# Default recipe - show available commands
default:
    @just --list

# Verify required tools are available
preflight: spacetime-version-check
    @command -v python3 >/dev/null 2>&1 || { echo "Missing python3. Install it before running."; exit 1; }

# Refuse to build, generate, publish, or start with a mismatched CLI/runtime.
spacetime-version-check:
    @command -v spacetime >/dev/null 2>&1 || { echo "Missing 'spacetime' CLI. Install version {{spacetime_version}} before running."; exit 1; }
    @version_output="$(spacetime --version 2>&1)"; \
      grep -Fq "spacetimedb tool version {{spacetime_version}};" <<<"$version_output" || { \
        echo "Expected SpacetimeDB CLI {{spacetime_version}}, but found:" >&2; \
        echo "$version_output" >&2; \
        exit 1; \
      }; \
      grep -Fq "spacetimedb-lib version {{spacetime_version}};" <<<"$version_output" || { \
        echo "Expected SpacetimeDB library {{spacetime_version}}, but found:" >&2; \
        echo "$version_output" >&2; \
        exit 1; \
      }

[script("powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File")]
caddy-preflight:
    if (-not (Get-Command "{{caddy_bin}}" -ErrorAction SilentlyContinue)) {
        Write-Error "Missing Caddy executable '{{caddy_bin}}'. Install it from https://caddyserver.com/docs/install or set CADDY_BIN to its full path."
        exit 1
    }

# Start the canonical server-rendered browser stack.
dev:
    @just web

# Full dev with WASM game built
dev-full:
    @just web

# Start the local browser stack, resetting local data only for breaking schema changes.
web: preflight build-wasm spacetime-start publish-on-conflict _seed-world build-tactical
    @just _spawner-start
    @echo ""
    @echo "Starting strategic-web server..."
    @echo "Open: http://localhost:{{web_port}}"
    @echo "Tactical servers bind on 127.0.0.1:{{tactical_web_port}}+"
    @echo ""
    @SPACETIMEDB_HOST={{spacetime_url}} \
     SPACETIMEDB_DATABASE={{spacetime_module}} \
     BIND_ADDRESS=127.0.0.1:{{web_port}} \
     STATIC_DIR={{strategic_web_dir}}/static \
     TACTICAL_STATIC_DIR={{strategic_static}} \
     cargo run -p strategic-web

# Start the browser stack after intentionally deleting and reseeding database data.
# Use only for disposable development state or the approved pre-launch 1.x reset.
web-reset: preflight build-wasm spacetime-start publish-reset _seed-world build-tactical
    @just _spawner-start
    @echo ""
    @echo "Starting strategic-web server after a database reset..."
    @echo "Open: http://localhost:{{web_port}}"
    @echo "Tactical servers bind on 127.0.0.1:{{tactical_web_port}}+"
    @echo ""
    @SPACETIMEDB_HOST={{spacetime_url}} \
     SPACETIMEDB_DATABASE={{spacetime_module}} \
     BIND_ADDRESS=127.0.0.1:{{web_port}} \
     STATIC_DIR={{strategic_web_dir}}/static \
     TACTICAL_STATIC_DIR={{strategic_static}} \
     cargo run -p strategic-web

# Start the browser stack behind locally trusted HTTPS. Caddy negotiates HTTP/2
# (and HTTP/3 when available) while strategic-web remains internal on port 8080.
web-secure: preflight caddy-preflight build-wasm spacetime-start publish _seed-world build-tactical
    #!/usr/bin/env bash
    set -euo pipefail
    just _spawner-start
    caddy start --config "{{caddy_config}}" --adapter caddyfile
    cleanup() {
        caddy stop --address 127.0.0.1:2020 >/dev/null 2>&1 || true
    }
    trap cleanup EXIT INT TERM
    echo ""
    echo "Starting strategic-web behind HTTPS..."
    echo "Open: https://localhost:{{secure_web_port}}"
    echo "Backend: http://127.0.0.1:{{web_port}}"
    echo ""
    SPACETIMEDB_HOST={{spacetime_url}} \
    SPACETIMEDB_DATABASE={{spacetime_module}} \
    BIND_ADDRESS=127.0.0.1:{{web_port}} \
    STATIC_DIR={{strategic_web_dir}}/static \
    TACTICAL_STATIC_DIR={{strategic_static}} \
    cargo run -p strategic-web

# Install Caddy's development root certificate in the host trust store.
[script("powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File")]
secure-web-trust: caddy-preflight
    & "{{caddy_bin}}" trust --config "{{caddy_config}}" --adapter caddyfile
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Start a fresh local stack with an injured character for stat-bar UI verification.
web-damaged: preflight build-wasm spacetime-start publish-reset _seed-world _seed-damaged-character build-tactical
    @just _spawner-start
    @echo ""
    @echo "Starting strategic-web server with Wounded Demo..."
    @echo "Open: http://localhost:{{web_port}}"
    @echo "Tactical servers bind on 127.0.0.1:{{tactical_web_port}}+"
    @echo ""
    @SPACETIMEDB_HOST={{spacetime_url}} \
     SPACETIMEDB_DATABASE={{spacetime_module}} \
     BIND_ADDRESS=127.0.0.1:{{web_port}} \
     STATIC_DIR={{strategic_web_dir}}/static \
     TACTICAL_STATIC_DIR={{strategic_static}} \
     cargo run -p strategic-web

# Seed the world with initial settlements and quests
_seed-world server=spacetime_url: spacetime-version-check
    @spacetime call --server {{server}} {{spacetime_module}} seed_world || echo "Seeding (may already be seeded)"

# Create or reset the injured Wounded Demo character used to verify damage bars.
_seed-damaged-character server=spacetime_url: spacetime-version-check
    @spacetime call --server {{server}} {{spacetime_module}} seed_damaged_character

# Start SpacetimeDB if it is not already listening
spacetime-start: spacetime-version-check
    @mkdir -p "{{run_dir}}"
    @if python3 -c 'import socket, sys; s=socket.socket(); s.settimeout(0.2); code=s.connect_ex(("127.0.0.1", {{spacetime_port}})); s.close(); sys.exit(0 if code==0 else 1)'; then \
        echo "SpacetimeDB already running on http://localhost:{{spacetime_port}}"; \
    else \
        rm -f "{{stdb_pid}}"; \
        spacetime start --listen-addr 127.0.0.1:{{spacetime_port}} >"{{stdb_log}}" 2>&1 & \
        echo $! > "{{stdb_pid}}"; \
        sleep 2; \
        if ! python3 -c 'import socket, sys; s=socket.socket(); s.settimeout(0.2); code=s.connect_ex(("127.0.0.1", {{spacetime_port}})); s.close(); sys.exit(0 if code==0 else 1)'; then \
            echo "SpacetimeDB failed to start. See {{stdb_log}}"; \
            exit 1; \
        fi; \
    fi

# Start SpacetimeDB bound to all interfaces (for VPS/public use)
spacetime-start-public: spacetime-version-check
    @mkdir -p "{{run_dir}}"
    @if python3 -c 'import socket, sys; s=socket.socket(); s.settimeout(0.2); code=s.connect_ex(("127.0.0.1", {{spacetime_port}})); s.close(); sys.exit(0 if code==0 else 1)'; then \
        echo "SpacetimeDB already running on http://localhost:{{spacetime_port}}"; \
    else \
        rm -f "{{stdb_pid}}"; \
        spacetime start --listen-addr {{public_bind}}:{{spacetime_port}} >"{{stdb_log}}" 2>&1 & \
        echo $! > "{{stdb_pid}}"; \
        sleep 2; \
        if ! python3 -c 'import socket, sys; s=socket.socket(); s.settimeout(0.2); code=s.connect_ex(("127.0.0.1", {{spacetime_port}})); s.close(); sys.exit(0 if code==0 else 1)'; then \
            echo "SpacetimeDB failed to start. See {{stdb_log}}"; \
            exit 1; \
        fi; \
    fi

# Stop SpacetimeDB
spacetime-stop:
    @if [ -f "{{stdb_pid}}" ] && kill -0 "$(cat "{{stdb_pid}}")" 2>/dev/null; then \
        kill "$(cat "{{stdb_pid}}")"; \
        rm -f "{{stdb_pid}}"; \
        echo "SpacetimeDB stopped"; \
    else \
        rm -f "{{stdb_pid}}"; \
        echo "SpacetimeDB not running"; \
    fi

# Publish the strategic module (optional target can be provided: just publish target=localhost)
publish server=spacetime_url: spacetime-version-check
    @cd "{{strategic_dir}}" && spacetime publish --server {{server}} {{spacetime_module}}

# Publish locally, clearing data only when a breaking schema change requires it.
publish-on-conflict: spacetime-version-check
    @cd "{{strategic_dir}}" && spacetime publish --delete-data=on-conflict --server {{spacetime_url}} {{spacetime_module}}

# Publish and clear the module database
publish-reset server=spacetime_url: spacetime-version-check
    @cd "{{strategic_dir}}" && spacetime publish --delete-data=always --server {{server}} {{spacetime_module}}

# Stop all running services started by this justfile
stop: _spawner-stop spacetime-stop

# Show status of local services
status:
    @if python3 -c 'import socket, sys; s=socket.socket(); s.settimeout(0.2); code=s.connect_ex(("127.0.0.1", {{spacetime_port}})); s.close(); sys.exit(0 if code==0 else 1)'; then \
        echo "SpacetimeDB: running (http://localhost:{{spacetime_port}})"; \
    else \
        echo "SpacetimeDB: not running"; \
    fi
    @if [ -f "{{spawner_pid}}" ] && kill -0 "$(cat "{{spawner_pid}}")" 2>/dev/null; then \
        echo "Tactical spawner: running (pid $(cat "{{spawner_pid}}"))"; \
    else \
        echo "Tactical spawner: not running"; \
    fi

# Build the strategic SpacetimeDB module
build-strategic: spacetime-version-check
    @cd "{{strategic_dir}}" && spacetime build

# Generate SpacetimeDB SDK client bindings
generate-db-client: spacetime-version-check
	@echo "Generating SpacetimeDB client bindings..."
	@spacetime generate --lang rust --out-dir crates/adventuresim-stdb-client/src --module-path "{{strategic_dir}}"
	@cargo fmt --package adventuresim-stdb-client
	@echo "Bindings generated in crates/adventuresim-stdb-client/src/"

# Download and extract the Viabundus v2 CSV source data into viabundus/.
init-viabundus:
	@python3 scripts/init_viabundus.py

# Download and verify the pinned NOAA OWDA v1.0 NetCDF source.
init-owda:
	@python3 scripts/init_owda.py

# Download and verify Jung/IIASA European PNV v1.1 COGs.
init-jung-pnv:
	@python3 scripts/init_jung_pnv.py

# Plan SoilGrids preparation. Pass `--prepare` manually after installing GDAL.
init-soilgrids:
	@python3 scripts/init_soilgrids.py

# Plan, initialize, or verify the remaining accepted world-data sources.
plan-glo30:
	@python3 scripts/world_source_init.py glo30 --plan
init-glo30:
	@python3 scripts/world_source_init.py glo30 --init
verify-glo30:
	@python3 scripts/world_source_init.py glo30 --verify-only

plan-luh1:
	@python3 scripts/world_source_init.py luh1 --plan
init-luh1:
	@python3 scripts/world_source_init.py luh1 --init
verify-luh1:
	@python3 scripts/world_source_init.py luh1 --verify-only

plan-forest-cover:
	@python3 scripts/world_source_init.py forest --plan
init-forest-cover:
	@python3 scripts/world_source_init.py forest --init
verify-forest-cover:
	@python3 scripts/world_source_init.py forest --verify-only

plan-tree-species:
	@python3 scripts/world_source_init.py trees4f --plan
init-tree-species:
	@python3 scripts/world_source_init.py trees4f --init
verify-tree-species:
	@python3 scripts/world_source_init.py trees4f --verify-only

plan-geology:
	@python3 scripts/world_source_init.py egdi --plan
init-geology:
	@python3 scripts/world_source_init.py egdi --init
verify-geology:
	@python3 scripts/world_source_init.py egdi --verify-only

plan-religion:
	@python3 scripts/world_source_init.py religion --plan
init-religion:
	@python3 scripts/world_source_init.py religion --init
verify-religion:
	@python3 scripts/world_source_init.py religion --verify-only

plan-hydrology:
	@python3 scripts/world_source_init.py eu-hydro --plan
init-hydrology:
	@python3 scripts/world_source_init.py eu-hydro --init
verify-hydrology:
	@python3 scripts/world_source_init.py eu-hydro --verify-only

# Compile all initialized sources into the 1544 strategic world artifact.
compile-world:
	@cargo run --package adventuresim-world-import --

# Compatibility name for the former Python normalizer.
normalise-viabundus: compile-world

# Compile and load the world into the published local module.
load-world server=spacetime_url: spacetime-version-check
	@cargo run --package adventuresim-world-import -- --load --server {{server}} --database {{spacetime_module}}

# Compatibility name for the former Viabundus-only loader.
load-viabundus-world server=spacetime_url: spacetime-version-check
	@cargo run --package adventuresim-world-import -- --load --server {{server}} --database {{spacetime_module}}

# Build the tactical server and spawner
build-tactical: generate-db-client
	@cargo build --package adventuresim-tactical-server --package adventuresim-tactical-server-dispatcher

# Build the WASM client
build-wasm:
	@bash scripts/build_wasm.sh

# Build everything
build-all: build-strategic build-tactical build-wasm

# Run the tactical spawner (watches for pending missions and starts servers)
spawner: build-tactical
	@cargo run --package adventuresim-tactical-server-dispatcher -- \
		--spacetimedb-url {{spacetime_url}} \
		--spacetimedb-module {{spacetime_module}} \
		--tactical-server-bin "$(pwd)/target/debug/adventuresim-tactical-server" \
		--base-port {{tactical_port}}

# Start the tactical spawner in the background.
_spawner-start host="127.0.0.1" base_port=tactical_web_port:
    @mkdir -p "{{run_dir}}"
    @if [ -f "{{spawner_pid}}" ] && kill -0 "$(cat "{{spawner_pid}}")" 2>/dev/null; then \
        echo "Tactical spawner already running (pid $(cat "{{spawner_pid}}"))"; \
    else \
        rm -f "{{spawner_pid}}"; \
        RUST_LOG=info setsid "$(pwd)/target/debug/adventuresim-tactical-server-dispatcher" \
            --spacetimedb-url {{spacetime_url}} \
            --spacetimedb-module {{spacetime_module}} \
            --tactical-server-bin "$(pwd)/target/debug/adventuresim-tactical-server" \
            --base-port {{base_port}} \
            --host {{host}} >"{{spawner_log}}" 2>&1 < /dev/null & \
        echo $! > "{{spawner_pid}}"; \
        sleep 1; \
        if ! kill -0 "$(cat "{{spawner_pid}}")" 2>/dev/null; then \
            echo "Tactical spawner failed to start. See {{spawner_log}}"; \
            exit 1; \
        fi; \
        echo "Tactical spawner running; log: {{spawner_log}}"; \
    fi

# Stop the tactical spawner.
_spawner-stop:
    @if [ -f "{{spawner_pid}}" ] && kill -0 "$(cat "{{spawner_pid}}")" 2>/dev/null; then \
        kill "$(cat "{{spawner_pid}}")"; \
        rm -f "{{spawner_pid}}"; \
        echo "Tactical spawner stopped"; \
    else \
        rm -f "{{spawner_pid}}"; \
        echo "Tactical spawner not running"; \
    fi

# Run a single tactical server (for testing)
tactical mission_id="test-mission" scene_key="hills" bots="3":
	@cargo run --package adventuresim-tactical-server --features "debug" -- \
		--addr "0.0.0.0:{{tactical_port}}" \
		--mission-id {{mission_id}} \
		--scene-key {{scene_key}} \
		--spacetimedb-url {{spacetime_url}} \
		--spacetimedb-module {{spacetime_module}} \
		--bots {{bots}} \
		--no-timeout

# Run a native tactical client (for testing `just tactical`)
client id="0" features="":
	@cargo run --package adventuresim-tactical-client --features "debug,{{features}}" -- \
		--id "{{id}}" \
		--server-addr "127.0.0.1:{{tactical_port}}"

# Generate self-signed WebTransport certificates
certs sans="127.0.0.1,localhost":
    @command -v openssl >/dev/null 2>&1 || { echo "Missing openssl. Install it before running."; exit 1; }
    @bash "{{cert_dir}}/generate_certificates.sh" "{{sans}}"
    @echo "Wrote {{cert_pem}}, {{cert_key}}, and {{cert_dir}}/digest.txt"

# Windows tactical development recipe
# Runs the strategic stack in WSL, then stages and runs the tactical server and
# client 0 as native Windows executables (GPU accelerated, no WSLg/UDP issues).
win-dev:
    #!/usr/bin/env bash
    set -e

    WIN_TARGET="x86_64-pc-windows-gnu"
    STAGE_DIR="/mnt/e/adventure-sim-dev"
    SERVER_EXE="./target/${WIN_TARGET}/win-dev/adventuresim-tactical-server.exe"
    CLIENT_EXE="./target/${WIN_TARGET}/win-dev/adventuresim-tactical-client.exe"

    command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 || {
        echo "Missing MinGW linker: install gcc-mingw-w64-x86-64" >&2
        exit 1
    }
    if ! rustup target list --installed | grep -Fxq "$WIN_TARGET"; then
        echo "Installing Rust target ${WIN_TARGET}..."
        rustup target add "$WIN_TARGET"
    fi

    kill_windows_processes() {
        (cd /mnt/c && cmd.exe /C "taskkill /IM adventuresim-tactical-server.exe /F >NUL 2>&1") || true
        (cd /mnt/c && cmd.exe /C "taskkill /IM adventuresim-tactical-client.exe /F >NUL 2>&1") || true
    }

    # Kill any leftover instances
    kill_windows_processes
    sleep 0.5

    cleanup() {
        echo ""
        echo "Shutting down..."
        kill_windows_processes
        kill -- -"$DEV_PID" 2>/dev/null || true
        wait "$DEV_PID" 2>/dev/null || true
    }

    echo "Starting strategic development stack..."
    setsid just dev &
    DEV_PID=$!
    trap cleanup EXIT INT TERM

    echo "Waiting for strategic web server..."
    for _ in {1..300}; do
        if python3 -c 'import socket, sys; s=socket.socket(); s.settimeout(0.2); code=s.connect_ex(("127.0.0.1", {{web_port}})); s.close(); sys.exit(code != 0)'; then
            break
        fi
        if ! kill -0 "$DEV_PID" 2>/dev/null; then
            echo "Strategic development stack exited before becoming ready" >&2
            exit 1
        fi
        sleep 1
    done
    if ! python3 -c 'import socket, sys; s=socket.socket(); s.settimeout(0.2); code=s.connect_ex(("127.0.0.1", {{web_port}})); s.close(); sys.exit(code != 0)'; then
        echo "Timed out waiting for strategic web server" >&2
        exit 1
    fi

    echo "Building server (Windows)..."
    cargo build -p adventuresim-tactical-server --features debug --target "$WIN_TARGET" --profile win-dev 2>&1
    echo "Building client (Windows)..."
    cargo build -p adventuresim-tactical-client --features debug --target "$WIN_TARGET" --profile win-dev 2>&1

    echo "Staging to E:\\adventure-sim-dev..."
    mkdir -p "$STAGE_DIR"
    cp "$SERVER_EXE" "$STAGE_DIR/adventuresim-tactical-server.exe"
    cp "$CLIENT_EXE" "$STAGE_DIR/adventuresim-tactical-client.exe"
    rsync -a --delete assets/ "$STAGE_DIR/assets/"
    # Merge per-crate asset directories (Bevy does this automatically in cargo run)
    for d in crates/*/assets/; do
        [ -d "$d" ] && rsync -a "$d" "$STAGE_DIR/assets/"
    done

    echo "Starting server..."
    cd "$STAGE_DIR" && ./adventuresim-tactical-server.exe \
        --addr 0.0.0.0:{{tactical_port}} \
        --mission-id test-mission \
        --scene-key hills \
        --spacetimedb-url http://localhost:{{spacetime_port}} \
        --spacetimedb-module {{spacetime_module}} \
        --bots 3 \
        --no-timeout &
    SERVER_PID=$!
    sleep 3

    echo "Starting client 0..."
    cd "$STAGE_DIR" && ./adventuresim-tactical-client.exe --id 0 --server-addr 127.0.0.1:{{tactical_port}} &
    CLIENT_PID=$!

    wait

# Workspace utilities
check:
    @cargo check --workspace

test-chat:
    @node --test crates/strategic-web/tests/local-chat.test.cjs

# Run a deterministic sample strategic NPC population.
strategic-sim seed="42" population="100" days="1095":
    @cargo run -p adventuresim-strategic-sim -- run --seed {{seed}} --population {{population}} --days {{days}}

test-strategic-sim:
    @cargo test -p adventuresim-strategic-sim

# Own one nonce-named local database for the duration of the command. There is
# intentionally no database or server override.
strategic-sim-core-loop seed="42" population="4" cycles="100" duration_days="365" party_size="2": spacetime-version-check spacetime-start
    #!/usr/bin/env bash
    set -euo pipefail
    set +x
    token="$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')"
    if [[ ${#token} -ne 64 ]]; then
        echo "failed to create simulation bootstrap capability" >&2
        exit 1
    fi
    export ADVENTURESIM_SIM_BOOTSTRAP_TOKEN="$token"
    nonce="$(date +%s)-$$-${RANDOM}-${RANDOM}"
    database="adventuresim-sim-${nonce}"
    cleanup() {
        spacetime delete --yes --server "{{spacetime_url}}" "$database" >/dev/null 2>&1 || true
    }
    trap cleanup EXIT INT TERM
    cd "{{strategic_dir}}"
    spacetime publish --server "{{spacetime_url}}" "$database"
    cd ../..
    cargo run -p adventuresim-strategic-sim -- core-loop \
        --host "{{spacetime_url}}" --database "$database" --run-nonce "$nonce" \
        --seed {{seed}} --population {{population}} --cycles {{cycles}} \
        --duration-days {{duration_days}} --party-size {{party_size}}

test: test-chat build-strategic
    @cargo test --workspace --exclude adventuresim-stdb-module

fmt:
    @cargo fmt --all

lint:
    @cargo clippy --workspace -- -D warnings

clean:
    @cargo clean
