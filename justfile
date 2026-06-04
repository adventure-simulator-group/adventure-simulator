# Adventure Simulator - local development
# Install just: cargo install just

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

web_port := "8080"
tactical_port := "6000"
public_bind := "0.0.0.0"
database_url := "sqlite://adventuresim.db"
strategic_web_dir := "crates/strategic-web"
tactical_static := strategic_web_dir + "/static/tactical"

default:
    @just --list

preflight:
    @command -v cargo >/dev/null 2>&1 || { echo "Missing cargo"; exit 1; }

# Run the full local browser stack. The app runs migrations and seeds the default world on startup.
web public_host="127.0.0.1": preflight build-wasm build-tactical
    @echo "Open: http://localhost:{{web_port}}"
    @DATABASE_URL={{database_url}} \
     BIND_ADDRESS=127.0.0.1:{{web_port}} \
     STATIC_DIR={{strategic_web_dir}}/static \
     TACTICAL_STATIC_DIR={{tactical_static}} \
     TACTICAL_SERVER_BIN="$(pwd)/target/debug/adventuresim-tactical-server" \
     TACTICAL_BIND_HOST=127.0.0.1 \
     TACTICAL_PUBLIC_HOST="{{public_host}}" \
     STRATEGIC_INTERNAL_URL=http://127.0.0.1:{{web_port}} \
     cargo run -p strategic-web

# Run the full browser stack on all interfaces, useful for a VPS.
vps-web public_host: preflight build-wasm build-tactical
    @echo "Open: http://{{public_host}}:{{web_port}}"
    @DATABASE_URL={{database_url}} \
     BIND_ADDRESS={{public_bind}}:{{web_port}} \
     STATIC_DIR={{strategic_web_dir}}/static \
     TACTICAL_STATIC_DIR={{tactical_static}} \
     TACTICAL_SERVER_BIN="$(pwd)/target/debug/adventuresim-tactical-server" \
     TACTICAL_BIND_HOST={{public_bind}} \
     TACTICAL_PUBLIC_HOST="{{public_host}}" \
     STRATEGIC_INTERNAL_URL=http://127.0.0.1:{{web_port}} \
     cargo run -p strategic-web

open-web:
    @url="http://localhost:{{web_port}}"; \
    if command -v xdg-open >/dev/null 2>&1; then \
        xdg-open "$$url" >/dev/null 2>&1 & \
    elif command -v open >/dev/null 2>&1; then \
        open "$$url" >/dev/null 2>&1 & \
    else \
        echo "Open $$url"; \
    fi

build-tactical:
    @cargo build --package adventuresim-tactical-server

build-wasm:
    @bash scripts/build_wasm.sh

build-all: build-tactical build-wasm
    @cargo build --workspace

check:
    @cargo fmt --all --check
    @cargo check --workspace

# Run a standalone tactical server for manual testing.
tactical mission_id="test-mission" scene_key="hills" strategic_url="http://127.0.0.1:8080":
    @cargo run --package adventuresim-tactical-server -- \
        --addr "0.0.0.0:{{tactical_port}}" \
        --public-addr "127.0.0.1:{{tactical_port}}" \
        --mission-id "{{mission_id}}" \
        --scene-key "{{scene_key}}" \
        --strategic-url "{{strategic_url}}" \
        --no-timeout

# Run a native tactical client against `just tactical`.
client id="0":
    @cargo run --package adventuresim-tactical-client -- \
        --id "{{id}}" \
        --server-addr "127.0.0.1:{{tactical_port}}"

clean-db:
    @rm -f adventuresim.db adventuresim.db-shm adventuresim.db-wal
