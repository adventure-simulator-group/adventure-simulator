# Adventure Simulator - local development
# Install just: cargo install just
# List commands: just --list

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

spacetime_port := "3000"
ui_port := "8000"
tactical_port := "6000"
public_bind := "0.0.0.0"

spacetime_url := "http://localhost:" + spacetime_port
spacetime_module := "strategic-db"

strategic_dir := "crates/strategic-db"
strategic_static := strategic_dir + "/static"

run_dir := "/tmp/adventure-simulator-1"
http_pid := run_dir + "/http.pid"
http_log := run_dir + "/http.log"
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
stop: spacetime-stop stop-ui

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

# Build the strategic SpacetimeDB module
build-strategic:
    @cd "{{strategic_dir}}" && spacetime build

# Generate SpacetimeDB SDK client bindings
generate-db-client:
    @echo "Generating SpacetimeDB client bindings..."
    @spacetime generate --lang rust --out-dir crates/strategic-db-client/src --project-path "{{strategic_dir}}"
    @echo "Bindings generated in crates/strategic-db-client/src/"

# Build the tactical server and spawner
build-tactical: generate-db-client
    @cargo build --package tactical-server --package tactical-spawner

# Build the WASM client
build-wasm:
    @echo "Building WASM client..."
    @command -v wasm-bindgen >/dev/null 2>&1 || { echo "Missing wasm-bindgen. Install with: cargo install wasm-bindgen-cli"; exit 1; }
    @rustup target add wasm32-unknown-unknown 2>/dev/null || true
    @cargo build --package adventure-simulator-client --target wasm32-unknown-unknown --release
    @mkdir -p "{{strategic_static}}/wasm"
    @wasm-bindgen \
        --out-dir "{{strategic_static}}/wasm" \
        --target web \
        --no-typescript \
        target/wasm32-unknown-unknown/release/adventure-simulator-client.wasm
    @echo "WASM built to {{strategic_static}}/wasm/"
    @ls -lh "{{strategic_static}}/wasm/"

# Build everything
build-all: build-strategic build-tactical build-wasm

# Run the tactical spawner (watches for pending missions and starts servers)
spawner: build-tactical
    @cargo run --package tactical-spawner -- \
        --spacetimedb-url {{spacetime_url}} \
        --spacetimedb-module {{spacetime_module}} \
        --tactical-server-bin "$(pwd)/target/debug/tactical-server" \
        --base-port {{tactical_port}}

# Run a single tactical server (for testing)
tactical mission_id="test-mission" scene_key="town_a":
    @cargo run --package tactical-server -- \
        --addr "0.0.0.0:{{tactical_port}}" \
        --mission-id {{mission_id}} \
        --scene-key {{scene_key}} \
        --spacetimedb-url {{spacetime_url}} \
        --spacetimedb-module {{spacetime_module}} \
        --no-timeout \
        --dump-digest

# Run a native tactical client (for testing `just tactical`)
client id="0" digest_file="tactical-server.digest":
    @cargo run --package adventure-simulator-client -- \
        --id "{{id}}" \
        --server-addr "127.0.0.1:{{tactical_port}}" \
        --digest `cat {{ digest_file }}`

# Generate self-signed WebTransport certificates
certs sans="127.0.0.1,localhost":
    @command -v openssl >/dev/null 2>&1 || { echo "Missing openssl. Install it before running."; exit 1; }
    @bash "{{cert_dir}}/generate_certificates.sh" "{{sans}}"
    @echo "Wrote {{cert_pem}}, {{cert_key}}, and {{cert_dir}}/digest.txt"

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
