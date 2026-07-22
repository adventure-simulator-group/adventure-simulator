#!/usr/bin/env python3
"""Build the tactical WebAssembly client and synchronize browser assets."""

from __future__ import annotations

from pathlib import Path
import shutil
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
STATIC_DIR = ROOT / "crates" / "adventuresim-stdb-module" / "static"
WASM_DIR = STATIC_DIR / "wasm"
ASSET_DIR = STATIC_DIR / "assets"


def run(command: list[str], *, check: bool = True) -> int:
    result = subprocess.run(command, cwd=ROOT)
    if check and result.returncode:
        raise subprocess.CalledProcessError(result.returncode, command)
    return result.returncode


def sync_assets() -> None:
    if ASSET_DIR.exists():
        shutil.rmtree(ASSET_DIR)
    shutil.copytree(ROOT / "assets", ASSET_DIR)
    for source in (ROOT / "crates").glob("*/assets"):
        if source.is_dir():
            shutil.copytree(source, ASSET_DIR, dirs_exist_ok=True)


def main() -> int:
    wasm_bindgen = shutil.which("wasm-bindgen")
    if wasm_bindgen is None:
        print("Missing wasm-bindgen. Install with: cargo install wasm-bindgen-cli", file=sys.stderr)
        return 1
    try:
        print("Building WASM client...")
        run(["rustup", "target", "add", "wasm32-unknown-unknown"], check=False)
        run(["cargo", "build", "--package", "adventuresim-tactical-client", "--target", "wasm32-unknown-unknown", "--release"])
        WASM_DIR.mkdir(parents=True, exist_ok=True)
        print("Generating JS bindings...")
        run([
            wasm_bindgen, "--out-dir", str(WASM_DIR), "--target", "web", "--no-typescript",
            str(ROOT / "target" / "wasm32-unknown-unknown" / "release" / "adventuresim-tactical-client.wasm"),
        ])
        print("Syncing browser assets...")
        sync_assets()
    except (OSError, subprocess.CalledProcessError) as error:
        print(error, file=sys.stderr)
        return 1
    print(f"WASM built to {WASM_DIR}")
    for path in sorted(WASM_DIR.iterdir()):
        print(f"  {path.name}: {path.stat().st_size} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
