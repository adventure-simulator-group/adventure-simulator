#!/usr/bin/env python3
"""Materialize, capture, or play a tactical scene at WGS84 coordinates."""

from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = Path("target/strategic-map/terrain-routing-v3.json")
DEFAULT_PACK = Path("target/strategic-map/terrain-routing-v3.pack")
DEFAULT_MINUTE = 340_320
REVIEW_LOCATIONS = {
    "harz-forest": (51.6500, 10.5000),
    "brocken-summit": (51.7990, 10.6170),
}
SOURCE_PATHS = (
    Path("Cargo.lock"),
    Path("crates/adventuresim-tactical-client/Cargo.toml"),
    Path("crates/adventuresim-tactical-client/src/presentation"),
    Path("crates/adventuresim-tactical-client/src/tactical_scene_viewer.rs"),
    Path("crates/adventuresim-tactical-client/src/tactical_scene_viewer"),
    Path("crates/adventuresim-tactical-server-dispatcher/Cargo.toml"),
    Path("crates/adventuresim-tactical-server-dispatcher/src/bin/materialize-real-world-scene.rs"),
    Path("crates/adventuresim-tactical-server-dispatcher/src/lib.rs"),
    Path("crates/adventuresim-tactical-server-dispatcher/src/scene_input.rs"),
    Path("assets/shaders"),
    Path("assets/textures"),
    Path("scripts/real_world_tactical.py"),
)


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("action", choices=("materialize", "capture", "play"))
    result.add_argument("latitude", type=float)
    result.add_argument("longitude", type=float)
    result.add_argument("--absolute-minute", type=int, default=DEFAULT_MINUTE)
    result.add_argument("--terrain-manifest", type=Path, default=DEFAULT_MANIFEST)
    result.add_argument("--terrain-pack", type=Path, default=DEFAULT_PACK)
    result.add_argument("--scene-output", type=Path, default=Path("target/tactical-real-world-scenes"))
    result.add_argument("--output", type=Path)
    result.add_argument("--settle-frames", type=int, default=12)
    result.add_argument("--base-port", type=int, default=24920)
    return result


def review_parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description="Capture curated real-world tactical vistas")
    result.add_argument("--output", type=Path, default=Path("target/tactical-real-world-captures/review"))
    result.add_argument("--absolute-minute", type=int, default=DEFAULT_MINUTE)
    result.add_argument("--terrain-manifest", type=Path, default=DEFAULT_MANIFEST)
    result.add_argument("--terrain-pack", type=Path, default=DEFAULT_PACK)
    result.add_argument("--settle-frames", type=int, default=12)
    return result


def review(argv: list[str]) -> int:
    args = review_parser().parse_args(argv)
    if args.output.exists():
        raise ValueError("real-world review output must be a fresh directory")
    for name, (latitude, longitude) in REVIEW_LOCATIONS.items():
        command = [
            sys.executable, __file__, "capture", str(latitude), str(longitude),
            "--absolute-minute", str(args.absolute_minute),
            "--terrain-manifest", str(args.terrain_manifest),
            "--terrain-pack", str(args.terrain_pack),
            "--scene-output", str(args.output / "scenes" / name),
            "--output", str(args.output / name),
            "--settle-frames", str(args.settle_frames),
        ]
        if subprocess.run(command, cwd=ROOT, check=False).returncode != 0:
            return 1
    return 0


def materialize(args: argparse.Namespace) -> Path:
    command = [
        "cargo", "run", "-p", "adventuresim-tactical-server-dispatcher",
        "--bin", "materialize-real-world-scene", "--",
        "--latitude", str(args.latitude), "--longitude", str(args.longitude),
        "--absolute-minute", str(args.absolute_minute),
        "--terrain-manifest", str(args.terrain_manifest),
        "--terrain-pack", str(args.terrain_pack),
        "--output-dir", str(args.scene_output),
    ]
    completed = subprocess.run(command, cwd=ROOT, check=True, text=True, stdout=subprocess.PIPE)
    print(completed.stdout, end="")
    prefix = "TACTICAL_REAL_WORLD_SCENE="
    path = next((Path(line[len(prefix):]) for line in completed.stdout.splitlines() if line.startswith(prefix)), None)
    if path is None or not path.is_file():
        raise RuntimeError("materializer did not report a valid tactical scene path")
    return path


def source_files() -> list[Path]:
    files: list[Path] = []
    for relative in SOURCE_PATHS:
        path = ROOT / relative
        files.extend(path.rglob("*") if path.is_dir() else (path,))
    return sorted(file for file in files if file.is_file())


def source_identity() -> str:
    digest = hashlib.sha256()
    for file in source_files():
        digest.update(file.relative_to(ROOT).as_posix().encode())
        digest.update(file.read_bytes())
    revision = subprocess.run(
        ("git", "rev-parse", "HEAD"), cwd=ROOT, check=True, text=True, stdout=subprocess.PIPE
    ).stdout.strip()
    dirty = subprocess.run(
        ("git", "status", "--porcelain", "--", *(str(path) for path in SOURCE_PATHS)),
        cwd=ROOT, check=True, text=True, stdout=subprocess.PIPE,
    ).stdout.strip()
    return f"{revision}:{'dirty' if dirty else 'clean'}:{digest.hexdigest()}"


def main(argv: list[str] | None = None) -> int:
    if argv is None:
        argv = sys.argv[1:]
    if argv and argv[0] == "review":
        return review(argv[1:])
    args = parser().parse_args(argv)
    if args.absolute_minute < 0 or args.settle_frames < 1:
        raise ValueError("absolute-minute must be nonnegative and settle-frames must be positive")
    scene = materialize(args)
    if args.action == "materialize":
        return 0
    if args.action == "capture":
        output = args.output or Path(
            f"target/tactical-real-world-captures/{args.latitude:+.5f}_{args.longitude:+.5f}_{args.absolute_minute}"
        )
        command = [
            "cargo", "run", "-p", "adventuresim-tactical-client", "--bin", "tactical-scene-viewer", "--",
            "--scene-input", str(scene), "--profile", "environment-review",
            "--settle-frames", str(args.settle_frames), "--output", str(output),
        ]
        for view in (
            "beauty-ground", "beauty-overhead", "horizon",
            "vista-lod-oblique", "vista-valley-oblique",
        ):
            command.extend(("--view", view))
        environment = {**os.environ, "CAPTURE_SOURCE_IDENTITY": source_identity()}
    else:
        command = [
            sys.executable, "scripts/dev_stack.py", "tactical-play", "animation", str(args.base_port),
            "--scene-input", str(scene),
        ]
        environment = None
    return subprocess.run(command, cwd=ROOT, env=environment, check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
