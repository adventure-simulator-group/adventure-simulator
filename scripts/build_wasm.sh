#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

STATIC_DIR="crates/adventuresim-stdb-module/static"
WASM_DIR="$STATIC_DIR/wasm"
ASSET_DIR="$STATIC_DIR/assets"
WASM_TARGET_DIR="target/wasm32-unknown-unknown/release"

echo "Building WASM client..."
command -v wasm-bindgen >/dev/null 2>&1 || {
  echo "Missing wasm-bindgen. Install with: cargo install wasm-bindgen-cli"
  exit 1
}

rustup target add wasm32-unknown-unknown 2>/dev/null || true

cargo build --package adventuresim-tactical-client --target wasm32-unknown-unknown --release

mkdir -p "$WASM_DIR" "$ASSET_DIR"

echo "Generating JS bindings..."
wasm-bindgen \
  --out-dir "$WASM_DIR" \
  --target web \
  --no-typescript \
  "$WASM_TARGET_DIR/adventuresim-tactical-client.wasm"

echo "Syncing browser assets..."
rsync -a --delete assets/ "$ASSET_DIR/"
for d in crates/*/assets/; do
  [ -d "$d" ] && rsync -a "$d" "$ASSET_DIR/"
done

echo "WASM built to $WASM_DIR"
ls -lh "$WASM_DIR"
