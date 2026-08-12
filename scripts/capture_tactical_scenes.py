#!/usr/bin/env python3
"""Capture a compact deterministic environment/time review matrix and index it."""

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import html
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
from typing import Sequence


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
FIXTURE_ROOT = REPOSITORY_ROOT / "assets" / "tactical-scenes"
ANSI_ESCAPE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
BASE_DAY_MINUTE = 236 * 24 * 60
NAMED_TIMES = {
    "morning": BASE_DAY_MINUTE + 8 * 60,
    "noon": BASE_DAY_MINUTE + 12 * 60,
    "grazing": BASE_DAY_MINUTE + 17 * 60,
    # Day 249 23:00: Sun -31.1 deg, Moon +24.6 deg, 99% illuminated.
    "moonlit": 359_940,
}
SKY_VIEWS = ("sun", "twilight", "moon", "stars")
SKY_MINUTES = {"sun": 172 * 1440 + 12 * 60, "twilight": 80 * 1440 + 18 * 60,
               "moon": 53_155, "stars": 637_860}
EXPECTED_PIPELINE = "tactical_scene_native_capture_v4"
EXPECTED_PROFILE_VERSION = 6
EXPECTED_CAMERA_VERSION = 5
EXPECTED_RESOLUTION = [1280, 720]
EXPECTED_PRESENTATION_REQUEST = {
    "shadows": True,
    "atmosphere": True,
    "celestial": True,
    "environment_light": True,
    "environment_map_size": 64,
    "bloom": True,
    "ssao": True,
    "max_vista_lods": 3,
}
SOURCE_PATHS = (
    "Cargo.lock",
    "Cargo.toml",
    "crates/adventuresim-tactical-client/Cargo.toml",
    "crates/adventuresim-tactical-client/src/presentation",
    "crates/adventuresim-tactical-client/src/tactical_scene_viewer.rs",
    "crates/adventuresim-tactical-client/src/tactical_scene_viewer_main.rs",
    "crates/adventuresim-tactical-client/src/tactical_sky_viewer.rs",
    "crates/adventuresim-tactical-client/src/tactical_sky_viewer_main.rs",
    "scripts/capture_tactical_scenes.py",
    "assets/shaders",
    "assets/textures",
    "assets/tactical-scenes",
)


@dataclass(frozen=True)
class EnvironmentCase:
    fixture: str
    views: tuple[str, ...]


CURATED_ENVIRONMENTS = (
    EnvironmentCase(
        "sparse-woodland",
        (
            "beauty-ground",
            "tree-root-detail",
            "tree-branch-junction",
            "terrain-grazing-detail",
            "grass-seam-detail",
            "forest-floor-debris-detail",
            "horizon",
        ),
    ),
    EnvironmentCase(
        "steep-open-hillside",
        ("beauty-ground", "rock-detail", "terrain-grazing-detail", "horizon"),
    ),
    EnvironmentCase(
        "flat-dry-grassland",
        ("beauty-ground", "terrain-grazing-detail", "grass-seam-detail", "horizon"),
    ),
    EnvironmentCase(
        "narrow-peak-lod-boundary",
        ("horizon", "vista-lod-oblique"),
    ),
)


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--settle-frames", type=int, default=12)
    parser.add_argument(
        "--fixture",
        action="append",
        dest="fixtures",
        help="capture only a curated fixture; repeat to select a subset",
    )
    parser.add_argument(
        "--time",
        action="append",
        dest="times",
        choices=tuple(NAMED_TIMES),
        help="capture only this named time; repeat to select a subset",
    )
    parser.add_argument("--skip-build", action="store_true", help="use existing debug binaries")
    return parser.parse_args(argv)


def selected_matrix(fixtures: Sequence[str] | None, times: Sequence[str] | None) -> list[tuple[EnvironmentCase, str, int]]:
    by_name = {case.fixture: case for case in CURATED_ENVIRONMENTS}
    selected_fixtures = list(fixtures or by_name)
    unknown = sorted(set(selected_fixtures) - set(by_name))
    if unknown:
        raise ValueError(f"fixtures are not in the environment-only review profile: {', '.join(unknown)}")
    selected_times = list(times or ("morning", "grazing", "moonlit"))
    return [
        (by_name[fixture], time_name, NAMED_TIMES[time_name])
        for fixture in selected_fixtures
        for time_name in selected_times
    ]


def target_directory() -> Path:
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=REPOSITORY_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return Path(json.loads(result.stdout)["target_directory"])


def run_child(parts: Sequence[str], source_identity: str) -> tuple[bool, str]:
    environment = os.environ.copy()
    environment["CAPTURE_SOURCE_IDENTITY"] = source_identity
    result = subprocess.run(parts, cwd=REPOSITORY_ROOT, capture_output=True, text=True, env=environment)
    if result.stdout:
        print(result.stdout, end="")
    if result.stderr:
        print(result.stderr, end="", file=sys.stderr)
    clean_log = ANSI_ESCAPE.sub("", f"{result.stdout}\n{result.stderr}")
    runtime_error = any(
        "ERROR" in line or line.lstrip().lower().startswith("error:")
        for line in clean_log.splitlines()
    )
    return result.returncode == 0 and not runtime_error, clean_log


def source_identity() -> dict:
    head = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=REPOSITORY_ROOT, check=True,
        capture_output=True, text=True,
    ).stdout.strip()
    hasher = hashlib.sha256()
    files: list[Path] = []
    for relative in SOURCE_PATHS:
        path = REPOSITORY_ROOT / relative
        files.extend(path.rglob("*") if path.is_dir() else [path])
    for path in sorted(path for path in files if path.is_file()):
        relative = path.relative_to(REPOSITORY_ROOT).as_posix()
        hasher.update(relative.encode())
        hasher.update(b"\0")
        hasher.update(path.read_bytes())
        hasher.update(b"\0")
    status = subprocess.run(
        ["git", "status", "--porcelain", "--", *SOURCE_PATHS], cwd=REPOSITORY_ROOT,
        check=True, capture_output=True, text=True,
    ).stdout
    digest = hasher.hexdigest()
    return {"head": head, "dirty": bool(status.strip()), "source_sha256": digest,
            "identity": f"{head}:{'dirty' if status.strip() else 'clean'}:{digest}"}


def validated_png_set(directory: Path, expected_views: Sequence[str]) -> None:
    expected = {f"{view}.png" for view in expected_views}
    actual = {path.name for path in directory.glob("*.png")}
    if actual != expected:
        raise ValueError(f"PNG set differs from request: expected {sorted(expected)}, got {sorted(actual)}")
    for filename in expected:
        if directory.joinpath(filename).stat().st_size <= 64:
            raise ValueError(f"empty or truncated screenshot: {filename}")


def validated_child_manifest(
    path: Path, expected_fixture: str, expected_minute: int,
    expected_views: Sequence[str], expected_identity: str, expected_head: str,
) -> dict:
    if not path.is_file():
        raise ValueError("child did not produce manifest.json")
    manifest = json.loads(path.read_text(encoding="utf-8"))
    captured = [record["view"] for record in manifest.get("captures", [])]
    if manifest.get("capture_profile") != "environment-review":
        raise ValueError("child manifest has the wrong capture profile")
    if captured != list(expected_views) or manifest.get("requested_views") != list(expected_views):
        raise ValueError(f"child views differ from request: {captured!r}")
    if not manifest.get("validation", {}).get("passed"):
        raise ValueError("child semantic validation failed")
    if path.parent.joinpath("failure.txt").exists():
        raise ValueError("child left failure.txt")
    checks = {
        "fixture": expected_fixture,
        "absolute_minute": expected_minute,
        "pipeline": EXPECTED_PIPELINE,
        "capture_profile_version": EXPECTED_PROFILE_VERSION,
        "camera_version": EXPECTED_CAMERA_VERSION,
        "resolution": EXPECTED_RESOLUTION,
        "source_identity": expected_identity,
        "revision": expected_head,
    }
    for field, expected in checks.items():
        if manifest.get(field) != expected:
            raise ValueError(f"child {field} differs: expected {expected!r}, got {manifest.get(field)!r}")
    presentation = manifest.get("presentation_features", {})
    if presentation.get("requested") != EXPECTED_PRESENTATION_REQUEST:
        raise ValueError("child requested presentation features differ from production")
    observed = presentation.get("observed", {})
    if not presentation.get("requested_matches_observed"):
        raise ValueError("child did not observe requested production presentation features")
    if observed.get("settings") != EXPECTED_PRESENTATION_REQUEST:
        raise ValueError("child observed graphics settings differ from production")
    if not (observed.get("camera_environment_map")
            and observed.get("camera_environment_map_size") == [64, 64]
            and observed.get("camera_bloom") and observed.get("camera_ssao")
            and isinstance(observed.get("camera_exposure_ev100"), (int, float))
            and -1.0 <= observed["camera_exposure_ev100"] <= 15.0
            and observed.get("camera_tonemapping") == "AcesFitted"
            and observed.get("ambient_brightness", 0) > 0):
        raise ValueError("child observed camera or ambient lighting state is incomplete")
    if not manifest.get("validation", {}).get("lighting_readiness"):
        raise ValueError("child lighting readiness failed")
    if not all(capture.get("lighting_ready") for capture in manifest.get("captures", [])):
        raise ValueError("one or more requested views lacked stable lighting readbacks")
    if "forest-floor-debris-detail" in expected_views:
        debris = next(
            capture for capture in manifest["captures"]
            if capture["view"] == "forest-floor-debris-detail"
        )
        if not all(
            isinstance(debris.get(field), (int, float)) and 0 <= debris[field] <= .275
            for field in ("debris_leaf_distance_metres", "debris_twig_distance_metres")
        ):
            raise ValueError("debris view lacks an in-frame rendered leaf/twig pair")
    validated_png_set(path.parent, expected_views)
    if expected_minute == NAMED_TIMES["moonlit"]:
        celestial = manifest.get("celestial", {})
        if not (celestial.get("sun_altitude_degrees", 90) < -12
                and celestial.get("moon_altitude_degrees", -90) > 20
                and celestial.get("lunar_illumination", 0) > .9):
            raise ValueError("moonlit child lacks a risen, illuminated Moon under a dark sky")
    return manifest


def validated_sky_manifest(path: Path, expected_view: str, expected_identity: str, expected_head: str) -> dict:
    if not path.is_file():
        raise ValueError("sky child did not produce manifest")
    manifest = json.loads(path.read_text(encoding="utf-8"))
    for field, expected in {"pipeline": "tactical_sky_native_capture_v2", "view": expected_view,
                            "resolution": [1600, 900], "source_identity": expected_identity}.items():
        if manifest.get(field) != expected:
            raise ValueError(f"sky {field} differs")
    if manifest.get("revision") != expected_head or manifest.get("absolute_minute") != SKY_MINUTES[expected_view]:
        raise ValueError("sky revision or canonical minute differs")
    if not manifest.get("validation", {}).get("passed") or path.parent.joinpath(f"{expected_view}.failure.txt").exists():
        raise ValueError("sky semantic validation failed")
    png = path.parent / f"{expected_view}.png"
    if not png.is_file() or png.stat().st_size <= 64:
        raise ValueError("sky PNG missing or truncated")
    return manifest


def write_index(output: Path, aggregate: dict) -> None:
    cards = []
    for run in aggregate["runs"]:
        rel = run["directory"]
        status = "passed" if run["passed"] else "FAILED"
        images = "".join(
            f'<img src="{html.escape(rel)}/{html.escape(view)}.png" alt="{html.escape(view)}">'
            for view in run["requested_views"]
        )
        cards.append(
            f'<section class="{status.lower()}"><h2><a href="{html.escape(rel)}/index.html">'
            f'{html.escape(run["fixture"])} / {html.escape(run["time_name"])}</a> '
            f'<strong>{status}</strong></h2><div>{images}</div></section>'
        )
    sky = "".join(
        f'<figure><img src="sky/{view}.png" alt="{view}"><figcaption>{view}</figcaption></figure>'
        for view in SKY_VIEWS
    )
    document = f"""<!doctype html><meta charset="utf-8"><title>Fabelgeist environment review</title>
<style>body{{margin:2rem;background:#111820;color:#edf2f4;font:15px system-ui}}a{{color:#8dd6ff}}
section{{background:#202a34;padding:1rem;margin:1rem 0}}section.failed strong{{color:#ff8e8e}}
section div,.sky{{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:.5rem}}
img{{width:100%;height:auto}}figure{{margin:0}}</style>
<h1>Deterministic environment review matrix</h1><p><a href="manifest.json">aggregate manifest</a></p>
<h2>Sky evidence</h2><div class="sky">{sky}</div>{''.join(cards)}"""
    output.joinpath("index.html").write_text(document, encoding="utf-8")


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        matrix = selected_matrix(args.fixtures, args.times)
    except ValueError as error:
        raise SystemExit(str(error)) from error
    output = (args.output or REPOSITORY_ROOT / "target" / "tactical-scene-captures" / f"environment-review-{int(time.time())}").resolve()
    if output.exists():
        raise SystemExit(f"capture output already exists; choose a fresh directory: {output}")
    output.mkdir(parents=True)
    print(f"CAPTURE_MATRIX_OUTPUT={output}", flush=True)

    identity = source_identity()
    identity_value = identity["identity"]
    stamp = REPOSITORY_ROOT / "target" / "tactical-scene-capture-build.json"

    if not args.skip_build:
        subprocess.run(
            ["cargo", "build", "-p", "adventuresim-tactical-client", "--bin", "tactical-scene-viewer", "--bin", "tactical-sky-viewer"],
            cwd=REPOSITORY_ROOT,
            check=True,
        )
        stamp.parent.mkdir(parents=True, exist_ok=True)
        stamp.write_text(json.dumps(identity, indent=2), encoding="utf-8")
    else:
        if not stamp.is_file():
            raise SystemExit("--skip-build requires a capture build stamp from a prior successful runner build")
        stamped = json.loads(stamp.read_text(encoding="utf-8"))
        if stamped.get("identity") != identity_value:
            raise SystemExit("--skip-build refused: capture source identity differs from the stamped binaries")
    binary_root = target_directory() / "debug"
    suffix = ".exe" if os.name == "nt" else ""
    scene_exe = binary_root / f"tactical-scene-viewer{suffix}"
    sky_exe = binary_root / f"tactical-sky-viewer{suffix}"
    failures: list[str] = []
    runs: list[dict] = []

    sky_output = output / "sky"
    sky_output.mkdir()
    sky_results = []
    for view in SKY_VIEWS:
        destination = sky_output / f"{view}.png"
        ok, _ = run_child([str(sky_exe), "--view", view, "--output", str(destination), "--settle-frames", str(max(48, args.settle_frames))], identity_value)
        error = None
        sky_manifest = None
        try:
            sky_manifest = validated_sky_manifest(
                sky_output / f"{view}.manifest.json", view, identity_value, identity["head"]
            )
        except (OSError, ValueError, KeyError, json.JSONDecodeError) as exception:
            error = str(exception)
        ok = ok and error is None
        sky_results.append({"view": view, "screenshot": f"sky/{view}.png", "manifest": f"sky/{view}.manifest.json", "passed": ok, "error": error,
                            "absolute_minute": sky_manifest.get("absolute_minute") if sky_manifest else None})
        if not ok:
            failures.append(f"sky/{view}")

    for case, time_name, absolute_minute in matrix:
        relative = f"{case.fixture}/{time_name}"
        child_output = output / relative
        # Night cells assess integrated readability and silhouettes. Fine
        # material/junction diagnostics remain paired with morning and grazing
        # light, where their subjects are actually assessable.
        requested_views = (
            ("beauty-ground", "horizon") if time_name == "moonlit" else case.views
        )
        command = [
            str(scene_exe), "--fixture", case.fixture, "--output", str(child_output),
            "--settle-frames", str(args.settle_frames), "--absolute-minute", str(absolute_minute),
            "--profile", "environment-review",
        ]
        for view in requested_views:
            command.extend(("--view", view))
        process_ok, _ = run_child(command, identity_value)
        error = None
        manifest = None
        try:
            manifest = validated_child_manifest(
                child_output / "manifest.json", case.fixture, absolute_minute,
                requested_views, identity_value, identity["head"],
            )
        except (OSError, ValueError, KeyError, json.JSONDecodeError) as exception:
            error = str(exception)
        passed = process_ok and error is None
        if not passed:
            failures.append(relative)
        runs.append({
            "fixture": case.fixture,
            "time_name": time_name,
            "absolute_minute": absolute_minute,
            "directory": relative,
            "requested_views": list(requested_views),
            "passed": passed,
            "error": error,
            "child_scene_digest": manifest.get("scene_digest") if manifest else None,
        })

    aggregate = {
        "schema": "fabelgeist_environment_review_matrix_v1",
        "profile": "environment-review",
        "scope": {
            "environment_only": True, "characters": False, "weather_iteration": False,
            "water_iteration": False, "cloud_iteration": False, "cave_iteration": False,
        },
        "settle_frames": args.settle_frames,
        "expected_presentation_features": EXPECTED_PRESENTATION_REQUEST,
        "source": identity,
        "named_times": NAMED_TIMES,
        "curated_environments": [asdict(case) for case in CURATED_ENVIRONMENTS],
        "sky": sky_results,
        "runs": runs,
        "passed": not failures,
        "failures": failures,
    }
    output.joinpath("manifest.json").write_text(json.dumps(aggregate, indent=2), encoding="utf-8")
    write_index(output, aggregate)
    if failures:
        output.joinpath("failure.txt").write_text("Environment review capture failed:\n" + "\n".join(failures) + "\n", encoding="utf-8")
        return 1
    print(f"TACTICAL_ENVIRONMENT_REVIEW_VALID={output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
