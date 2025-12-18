#!/usr/bin/env bash
set -euo pipefail

crate="adventure-simulator-stdb-bevy-web-demo"
target="wasm32-unknown-unknown"
profile="debug"
cargo_flags=()

if [[ "${1:-}" == "--release" ]]; then
  profile="release"
  cargo_flags+=(--release)
  shift
fi

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "Missing wasm-bindgen CLI."
  echo "Install: cargo install wasm-bindgen-cli --locked"
  exit 1
fi

lock_wbg_version="$(awk '
  $1=="name" && $3=="\"wasm-bindgen\"" {found=1; next}
  found && $1=="version" {gsub(/\"/, "", $3); print $3; exit}
' Cargo.lock)"
cli_wbg_version="$(wasm-bindgen --version | awk '{print $2}')"

if [[ -n "${lock_wbg_version}" && "${cli_wbg_version}" != "${lock_wbg_version}" ]]; then
  echo "wasm-bindgen CLI version mismatch:"
  echo "  Cargo.lock wants: ${lock_wbg_version}"
  echo "  wasm-bindgen is:   ${cli_wbg_version}"
  echo
  echo "Fix with:"
  echo "  cargo install wasm-bindgen-cli --version ${lock_wbg_version} --locked"
  exit 1
fi

cargo build -p "${crate}" --target "${target}" "${cargo_flags[@]}"

wasm="target/${target}/${profile}/adventure_simulator_stdb_bevy_web_demo.wasm"
if [[ ! -f "${wasm}" ]]; then
  echo "Expected wasm output not found: ${wasm}"
  exit 1
fi

out_dir="ui/spacetimedb_bevy_demo/pkg"
mkdir -p "${out_dir}"

wasm-bindgen "${wasm}" \
  --target web \
  --out-name as_stdb_bevy_demo \
  --out-dir "${out_dir}"

if command -v wasm-opt >/dev/null 2>&1; then
  wasm_opt_in="${out_dir}/as_stdb_bevy_demo_bg.wasm"
  wasm_opt_out="${out_dir}/as_stdb_bevy_demo_bg.opt.wasm"
  wasm-opt -Oz --strip-debug --strip-dwarf "${wasm_opt_in}" -o "${wasm_opt_out}"
  mv "${wasm_opt_out}" "${wasm_opt_in}"
fi

echo "Built: ${out_dir}/as_stdb_bevy_demo.js"
