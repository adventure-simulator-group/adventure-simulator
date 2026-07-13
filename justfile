# Adventure Simulator - local development
# Install just: cargo install just
# List commands: just --list

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

spacetime_port := "3000"
ui_port := "8000"
web_port := "8080"
tactical_port := "6000"
tactical_web_port := "6001"
public_bind := "0.0.0.0"

spacetime_url := "http://localhost:" + spacetime_port
spacetime_module := "adventuresim-stdb-module"

strategic_dir := "crates/adventuresim-stdb-module"
strategic_static := strategic_dir + "/static"
strategic_web_dir := "crates/strategic-web"

run_dir := "/tmp/adventure-simulator-1"
http_pid := run_dir + "/http.pid"
http_log := run_dir + "/http.log"
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
preflight:
    @command -v spacetime >/dev/null 2>&1 || { echo "Missing 'spacetime' CLI. Install it before running."; exit 1; }
    @command -v python3 >/dev/null 2>&1 || { echo "Missing python3. Install it before running."; exit 1; }

# Start SpacetimeDB, publish module, and serve the strategic UI
dev: preflight spacetime-start publish serve-ui
    @echo "Strategic UI: http://localhost:{{ui_port}}/map.html"
    @echo "SpacetimeDB: http://localhost:{{spacetime_port}}"
    @echo "Optional tactical server: just tactical"
    @echo "Build WASM client: just build-wasm"

# Same as dev, then open the browser
dev-open: dev open-ui

# Full dev with WASM game built
dev-full: preflight build-wasm spacetime-start publish serve-ui
    @echo "Strategic UI: http://localhost:{{ui_port}}/map.html"
    @echo "SpacetimeDB: http://localhost:{{spacetime_port}}"
    @echo "WASM game: ready (click 'Enter World' after starting a mission)"

# Start the local browser stack.
web: preflight build-wasm spacetime-start publish-reset _seed-world build-tactical
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
_seed-world server=spacetime_url:
    @spacetime call --server {{server}} {{spacetime_module}} seed_world || echo "Seeding (may already be seeded)"

# Create or reset the injured Wounded Demo character used to verify damage bars.
_seed-damaged-character server=spacetime_url:
    @spacetime call --server {{server}} {{spacetime_module}} seed_damaged_character

# Start SpacetimeDB if it is not already listening
spacetime-start:
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
spacetime-start-public:
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
publish server=spacetime_url:
    @cd "{{strategic_dir}}" && spacetime publish --server {{server}} {{spacetime_module}}

# Publish and clear the module database
publish-reset server=spacetime_url:
    @cd "{{strategic_dir}}" && spacetime publish --delete-data=always --server {{server}} {{spacetime_module}}

# Serve the strategic UI locally (with proxy to SpacetimeDB)
serve-ui:
    @mkdir -p "{{run_dir}}"
    @if [ -f "{{http_pid}}" ] && kill -0 "$(cat "{{http_pid}}")" 2>/dev/null; then \
        echo "UI server already running (pid $(cat "{{http_pid}}"))"; \
    else \
        rm -f "{{http_pid}}"; \
        SPACETIMEDB_URL={{spacetime_url}} python3 "{{strategic_static}}/serve.py" {{ui_port}} >"{{http_log}}" 2>&1 & \
        echo $! > "{{http_pid}}"; \
        sleep 1; \
        echo "UI server running on http://localhost:{{ui_port}}/map.html"; \
    fi

# Serve the strategic UI on all interfaces (for VPS/public use)
serve-ui-public:
    @mkdir -p "{{run_dir}}"
    @if [ -f "{{http_pid}}" ] && kill -0 "$(cat "{{http_pid}}")" 2>/dev/null; then \
        echo "UI server already running (pid $(cat "{{http_pid}}"))"; \
    else \
        rm -f "{{http_pid}}"; \
        SPACETIMEDB_URL={{spacetime_url}} python3 "{{strategic_static}}/serve.py" {{ui_port}} >"{{http_log}}" 2>&1 & \
        echo $! > "{{http_pid}}"; \
        sleep 1; \
        echo "UI server running on http://{{public_bind}}:{{ui_port}}/map.html"; \
    fi

# Stop the strategic UI server
stop-ui:
    @if [ -f "{{http_pid}}" ] && kill -0 "$(cat "{{http_pid}}")" 2>/dev/null; then \
        kill "$(cat "{{http_pid}}")"; \
        rm -f "{{http_pid}}"; \
        echo "UI server stopped"; \
    else \
        rm -f "{{http_pid}}"; \
        echo "UI server not running"; \
    fi

# Open the strategic UI in a browser
open-ui:
    @url="http://localhost:{{ui_port}}/map.html"; \
    if command -v xdg-open >/dev/null 2>&1; then \
        xdg-open "$$url" >/dev/null 2>&1; \
    elif command -v open >/dev/null 2>&1; then \
        open "$$url" >/dev/null 2>&1; \
    elif command -v firefox >/dev/null 2>&1; then \
        firefox "$$url" >/dev/null 2>&1; \
    elif command -v google-chrome >/dev/null 2>&1; then \
        google-chrome "$$url" >/dev/null 2>&1; \
    else \
        echo "Open $$url"; \
    fi

# Stop all running services started by this justfile
stop: _spawner-stop spacetime-stop stop-ui

# Run the stack on a VPS (public bind). Requires firewall/DNS setup.
vps-serve domain="localhost": preflight spacetime-start-public publish serve-ui-public
    @echo "Public UI: http://{{domain}}:{{ui_port}}/map.html"
    @echo "SpacetimeDB: http://{{domain}}:{{spacetime_port}}"
    @echo "Open firewall ports {{ui_port}} and {{spacetime_port}}, and point DNS for {{domain}} to this VPS."
    @echo "If you serve the UI over HTTPS, proxy SpacetimeDB over HTTPS too (or use ?spacetimedb=http://<host>:<port>)."

# Show status of local services
status:
    @if python3 -c 'import socket, sys; s=socket.socket(); s.settimeout(0.2); code=s.connect_ex(("127.0.0.1", {{spacetime_port}})); s.close(); sys.exit(0 if code==0 else 1)'; then \
        echo "SpacetimeDB: running (http://localhost:{{spacetime_port}})"; \
    else \
        echo "SpacetimeDB: not running"; \
    fi
    @if [ -f "{{http_pid}}" ] && kill -0 "$(cat "{{http_pid}}")" 2>/dev/null; then \
        echo "UI server: running (http://localhost:{{ui_port}}/)"; \
    else \
        echo "UI server: not running"; \
    fi
    @if [ -f "{{spawner_pid}}" ] && kill -0 "$(cat "{{spawner_pid}}")" 2>/dev/null; then \
        echo "Tactical spawner: running (pid $(cat "{{spawner_pid}}"))"; \
    else \
        echo "Tactical spawner: not running"; \
    fi

# Build the strategic SpacetimeDB module
build-strategic:
    @cd "{{strategic_dir}}" && spacetime build

# Generate SpacetimeDB SDK client bindings
generate-db-client:
	@echo "Generating SpacetimeDB client bindings..."
	@spacetime generate --lang rust --out-dir crates/adventuresim-stdb-client/src --project-path "{{strategic_dir}}"
	@echo "Bindings generated in crates/adventuresim-stdb-client/src/"

# Download and extract the Viabundus v2 CSV source data into viabundus/.
init-viabundus:
	@python3 scripts/init_viabundus.py

# Normalise the local Viabundus v2 source CSVs for the 1544 strategic world.
normalise-viabundus:
	@python3 scripts/import_viabundus.py

# Load the normalised Viabundus road graph into the published local module.
load-viabundus-world server=spacetime_url:
	@python3 scripts/import_viabundus.py --load --server {{server}} --database {{spacetime_module}}

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

# Windows development recipe
# Local dev with native Windows exes (GPU accelerated, no WSLg, no UDP issues)
# Cross-compiles server + client to Windows, stages to E:\adventure-sim-dev, runs both
win-dev:
    #!/usr/bin/env bash
    set -e

    WIN_TARGET="x86_64-pc-windows-gnu"
    STAGE_DIR="/mnt/e/adventure-sim-dev"
    SERVER_EXE="./target/${WIN_TARGET}/win-dev/adventuresim-tactical-server.exe"
    CLIENT_EXE="./target/${WIN_TARGET}/win-dev/adventuresim-tactical-client.exe"

    # Kill any leftover instances
    cmd.exe /C "taskkill /IM adventuresim-tactical-server.exe /F >NUL 2>&1" || true
    cmd.exe /C "taskkill /IM adventuresim-tactical-client.exe /F >NUL 2>&1" || true
    sleep 0.5

    echo "Building server (Windows)..."
    cargo build -p adventuresim-tactical-server --target $WIN_TARGET --profile win-dev 2>&1
    echo "Building client (Windows)..."
    cargo build -p adventuresim-tactical-client --target $WIN_TARGET --profile win-dev 2>&1

    echo "Staging to E:\\adventure-sim-dev..."
    mkdir -p "$STAGE_DIR"
    cp "$SERVER_EXE" "$STAGE_DIR/adventuresim-tactical-server.exe"
    cp "$CLIENT_EXE" "$STAGE_DIR/adventuresim-tactical-client.exe"
    rsync -a --delete assets/ "$STAGE_DIR/assets/"
    # Merge per-crate asset directories (Bevy does this automatically in cargo run)
    for d in crates/*/assets/; do
        [ -d "$d" ] && rsync -a "$d" "$STAGE_DIR/assets/"
    done

    cleanup() {
        echo ""
        echo "Shutting down..."
        kill $SERVER_PID $CLIENT0_PID $CLIENT1_PID 2>/dev/null
        wait $SERVER_PID $CLIENT0_PID $CLIENT1_PID 2>/dev/null
    }
    trap cleanup EXIT INT TERM

    echo "Starting server..."
    cd "$STAGE_DIR" && ./adventuresim-tactical-server.exe \
        --mission-id test-mission \
        --scene-key hills \
        --no-timeout \
        --spacetimedb-url http://localhost:{{spacetime_port}} \
        --spacetimedb-module {{spacetime_module}} &
    SERVER_PID=$!
    sleep 3

    echo "Starting client 0..."
    cd "$STAGE_DIR" && ./adventuresim-tactical-client.exe --id 0 --server-addr 127.0.0.1:{{tactical_port}} &
    CLIENT0_PID=$!
    sleep 1

    echo "Starting client 1..."
    cd "$STAGE_DIR" && ./adventuresim-tactical-client.exe --id 1 --server-addr 127.0.0.1:{{tactical_port}} &
    CLIENT1_PID=$!

    wait

# Browser dev with a Windows tactical server.
# Tactical server runs natively on Windows; WASM + static UI stay in WSL.
win-web: preflight
    #!/usr/bin/env bash
    set -euo pipefail

    WIN_TARGET="x86_64-pc-windows-gnu"
    STAGE_DIR="/mnt/e/adventure-sim-dev"
    SERVER_EXE="./target/${WIN_TARGET}/win-dev/adventuresim-tactical-server.exe"
    SERVER_LOG="$STAGE_DIR/tactical-server.log"
    WINDOWS_HOST_IP="$(awk '/^nameserver / {print $2; exit}' /etc/resolv.conf)"
    BROWSER_URL_WINDOWS="http://127.0.0.1:{{ui_port}}/tactical.html?server=127.0.0.1:{{tactical_web_port}}&id=2&autostart=1"
    BROWSER_URL_WSL="http://127.0.0.1:{{ui_port}}/tactical.html?server=${WINDOWS_HOST_IP}:{{tactical_web_port}}&id=2&autostart=1"

    cmd.exe /C "taskkill /IM adventuresim-tactical-server.exe /F >NUL 2>&1" || true
    sleep 0.5

    echo "Building browser client..."
    bash scripts/build_wasm.sh

    echo "Ensuring strategic stack is running..."
    just spacetime-start
    just publish
    just serve-ui

    echo "Building Windows tactical server..."
    cargo build -p adventuresim-tactical-server --target "$WIN_TARGET" --profile win-dev 2>&1

    echo "Staging tactical server to E:\\adventure-sim-dev..."
    mkdir -p "$STAGE_DIR"
    cp "$SERVER_EXE" "$STAGE_DIR/adventuresim-tactical-server.exe"
    rsync -a --delete assets/ "$STAGE_DIR/assets/"
    for d in crates/*/assets/; do
        [ -d "$d" ] && rsync -a "$d" "$STAGE_DIR/assets/"
    done

    cleanup() {
        echo ""
        echo "Shutting down..."
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    }
    trap cleanup EXIT INT TERM

    echo "Starting Windows tactical server on browser-safe port {{tactical_web_port}}..."
    pushd "$STAGE_DIR" > /dev/null
    ./adventuresim-tactical-server.exe \
        --addr 0.0.0.0:{{tactical_web_port}} \
        --mission-id test-mission \
        --scene-key hills \
        --no-timeout \
        --spacetimedb-url http://localhost:{{spacetime_port}} \
        --spacetimedb-module {{spacetime_module}} > "$SERVER_LOG" 2>&1 &
    SERVER_PID=$!
    popd > /dev/null

    echo "Waiting for tactical server..."
    for _ in $(seq 1 30); do
        if powershell.exe -NoProfile -Command "(Test-NetConnection -ComputerName 127.0.0.1 -Port {{tactical_web_port}} -WarningAction SilentlyContinue).TcpTestSucceeded" | tr -d '\r' | grep -q True; then
            break
        fi
        sleep 1
    done

    if ! powershell.exe -NoProfile -Command "(Test-NetConnection -ComputerName 127.0.0.1 -Port {{tactical_web_port}} -WarningAction SilentlyContinue).TcpTestSucceeded" | tr -d '\r' | grep -q True; then
        echo "Tactical server failed to open 127.0.0.1:{{tactical_web_port}}"
        echo "Last server log lines:"
        tail -n 80 "$SERVER_LOG" || true
        exit 1
    fi

    echo ""
    echo "Browser tactical client:"
    echo "  Windows browser: $BROWSER_URL_WINDOWS"
    echo "  WSL/Linux browser: $BROWSER_URL_WSL"
    echo "Use a different 'id' query param for additional browser clients."

    wait

# Workspace utilities
check:
    @cargo check --workspace

test:
    @cargo test --workspace

fmt:
    @cargo fmt --all

lint:
    @cargo clippy --workspace -- -D warnings

clean:
    @cargo clean
