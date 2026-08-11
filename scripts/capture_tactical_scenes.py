#!/usr/bin/env python3
"""Build once, capture deterministic tactical fixtures, and index the results."""

from __future__ import annotations

import argparse
import html
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
FIXTURE_ROOT = REPOSITORY_ROOT / "assets" / "tactical-scenes"
ANSI_ESCAPE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--settle-frames", type=int, default=12)
    parser.add_argument(
        "--fixture",
        action="append",
        dest="fixtures",
        help="capture only this fixture; repeat to select an A/B subset",
    )
    return parser.parse_args()


def command(*parts: str) -> None:
    subprocess.run(parts, cwd=REPOSITORY_ROOT, check=True)


def target_directory() -> Path:
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=REPOSITORY_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return Path(json.loads(result.stdout)["target_directory"])


def write_index(output: Path, fixtures: list[str]) -> None:
    cards = []
    for fixture in fixtures:
        manifest_path = output / fixture / "manifest.json"
        status = "missing"
        details = "capture did not produce a manifest"
        if manifest_path.exists():
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            status = "passed" if manifest["validation"]["passed"] else "FAILED"
            obstacles = manifest["obstacles"]
            details = (
                f"trees {obstacles['generated_trees']}, rocks {obstacles['generated_rocks']}, "
                f"vista {manifest['vista']['diameter_metres'] / 1000:.0f} km"
            )
        cards.append(
            f'<li class="{status.lower()}"><a href="{html.escape(fixture)}/index.html">'
            f"{html.escape(fixture)}</a><strong>{status}</strong><span>{details}</span></li>"
        )
    document = f"""<!doctype html>
<meta charset="utf-8"><title>Fabelgeist tactical scene matrix</title>
<style>
body{{margin:2rem;background:#111820;color:#edf2f4;font:16px system-ui;max-width:80rem}}
ul{{display:grid;grid-template-columns:repeat(auto-fit,minmax(19rem,1fr));gap:1rem;padding:0}}
li{{display:grid;grid-template-columns:1fr auto;gap:.5rem;background:#202a34;padding:1rem;border-radius:.5rem}}
li span{{grid-column:1/-1;color:#b8c5cf}}li.failed strong,li.missing strong{{color:#ff8e8e}}
a{{color:#8dd6ff}}
</style><h1>Tactical scene capture matrix</h1><ul>{''.join(cards)}</ul>"""
    (output / "index.html").write_text(document, encoding="utf-8")


def main() -> int:
    args = parse_args()
    fixtures = args.fixtures or sorted(path.stem for path in FIXTURE_ROOT.glob("*.json"))
    unknown = [name for name in fixtures if not (FIXTURE_ROOT / f"{name}.json").is_file()]
    if unknown:
        raise SystemExit(f"unknown tactical fixtures: {', '.join(unknown)}")
    output = (args.output or REPOSITORY_ROOT / "target" / "tactical-scene-captures" / f"matrix-{int(time.time())}").resolve()
    if output.exists():
        raise SystemExit(f"capture output already exists; choose a fresh directory: {output}")
    output.mkdir(parents=True)
    print(f"CAPTURE_MATRIX_OUTPUT={output}", flush=True)

    command(
        "cargo",
        "build",
        "-p",
        "adventuresim-tactical-client",
        "--bin",
        "tactical-scene-viewer",
    )
    executable = target_directory() / "debug" / (
        "tactical-scene-viewer.exe" if os.name == "nt" else "tactical-scene-viewer"
    )
    failures = []
    for fixture in fixtures:
        result = subprocess.run(
            [
                str(executable),
                "--fixture",
                fixture,
                "--output",
                str(output / fixture),
                "--settle-frames",
                str(args.settle_frames),
            ],
            cwd=REPOSITORY_ROOT,
            capture_output=True,
            text=True,
        )
        if result.stdout:
            print(result.stdout, end="")
        if result.stderr:
            print(result.stderr, end="", file=sys.stderr)
        clean_log = ANSI_ESCAPE.sub("", f"{result.stdout}\n{result.stderr}")
        runtime_error = any(
            "ERROR" in line or line.lstrip().lower().startswith("error:")
            for line in clean_log.splitlines()
        )
        if result.returncode or runtime_error:
            failures.append(fixture)
        write_index(output, fixtures)
    if failures:
        print(f"capture validation failed: {', '.join(failures)}", file=sys.stderr)
        return 1
    print(f"TACTICAL_SCENE_MATRIX_VALID={output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
