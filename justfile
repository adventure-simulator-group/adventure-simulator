# Adventure Simulator - local development
# Install just: cargo install just

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

web_port := "8080"
tactical_port := "6000"
database_url := "sqlite://adventuresim.db"
strategic_web_dir := "crates/strategic-web"
tactical_static := strategic_web_dir + "/static/tactical"

default:
    @just --list

preflight:
    @command -v cargo >/dev/null 2>&1 || { echo "Missing cargo"; exit 1; }

# Run the full local browser stack with a fresh SQLite database.
web: preflight build-wasm build-tactical
    @rm -f adventuresim.db adventuresim.db-shm adventuresim.db-wal
    @echo "Open: http://localhost:{{web_port}}"
    @DATABASE_URL={{database_url}} \
     BIND_ADDRESS=127.0.0.1:{{web_port}} \
     STATIC_DIR={{strategic_web_dir}}/static \
     TACTICAL_STATIC_DIR={{tactical_static}} \
     TACTICAL_SERVER_BIN="$(pwd)/target/debug/adventuresim-tactical-server" \
     STRATEGIC_INTERNAL_URL=http://127.0.0.1:{{web_port}} \
     cargo run -p strategic-web

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
        --addr "127.0.0.1:{{tactical_port}}" \
        --mission-id "{{mission_id}}" \
        --scene-key "{{scene_key}}" \
        --strategic-url "{{strategic_url}}" \
        --no-timeout

# Run a native tactical client against `just tactical`.
client id="0":
    @cargo run --package adventuresim-tactical-client -- \
        --id "{{id}}" \
        --server-addr "127.0.0.1:{{tactical_port}}"
