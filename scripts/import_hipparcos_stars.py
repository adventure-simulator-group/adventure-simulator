#!/usr/bin/env python3
"""Compile naked-eye Hipparcos stars into the tactical runtime CSV.

The default source is a VizieR TSV query against CDS catalogue I/239. Only
fields required by the renderer are retained. The generated file is sorted by
HIP identifier so repeated imports are byte-stable.
"""

from __future__ import annotations

import argparse
import csv
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = ROOT / "assets" / "data" / "hipparcos-bright-stars.csv"
SOURCE_URL = (
    "https://vizier.cds.unistra.fr/viz-bin/asu-tsv?"
    "-source=I%2F239%2Fhip_main&-out=HIP,Vmag,B-V"
    "&-out.add=_RAJ2000,_DEJ2000"
    "&Vmag=..6.5&-out.max=unlimited"
)
MAX_MAGNITUDE = 6.5


def field(line: str, first: int, last: int) -> str:
    """Read an inclusive one-indexed fixed-width field."""
    return line[first - 1 : last].strip()


def source_lines(source: Path | None):
    if source is not None:
        text = source.read_text(encoding="utf-8")
    else:
        with urllib.request.urlopen(SOURCE_URL) as response:
            text = response.read().decode("utf-8")
    yield from text.splitlines()


def compile_catalog(lines) -> list[tuple[int, float, float, float, float]]:
    stars = []
    for line in lines:
        if not line or line.startswith("#") or line.startswith("_RAJ2000\t") or line.startswith("---"):
            continue
        columns = line.split("\t")
        if len(columns) >= 5:
            values = [value.strip() for value in columns[:5]]
            try:
                right_ascension = float(values[0])
                declination = float(values[1])
                hip = int(values[2])
                magnitude = float(values[3])
            except ValueError:
                continue
            try:
                color_index = float(values[4])
            except ValueError:
                color_index = 0.65
            stars.append((hip, right_ascension, declination, magnitude, color_index))
            continue
        try:
            hip = int(field(line, 9, 14))
            magnitude = float(field(line, 42, 46))
            right_ascension = float(field(line, 52, 63))
            declination = float(field(line, 65, 76))
        except ValueError:
            continue
        if magnitude > MAX_MAGNITUDE:
            continue
        try:
            color_index = float(field(line, 246, 251))
        except ValueError:
            color_index = 0.65
        stars.append((hip, right_ascension, declination, magnitude, color_index))
    stars.sort(key=lambda star: star[0])
    return stars


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, help="local VizieR TSV response")
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    args = parser.parse_args()
    stars = compile_catalog(source_lines(args.source))
    if not 5_000 <= len(stars) <= 12_000:
        raise SystemExit(f"unexpected bright-star count: {len(stars)}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="ascii") as output:
        writer = csv.writer(output, lineterminator="\n")
        writer.writerow(("hip", "ra_deg", "dec_deg", "v_mag", "b_v"))
        for star in stars:
            writer.writerow(
                (
                    star[0],
                    f"{star[1]:.8f}",
                    f"{star[2]:.8f}",
                    f"{star[3]:.2f}",
                    f"{star[4]:.3f}",
                )
            )
    print(f"wrote {len(stars)} stars to {args.output}")


if __name__ == "__main__":
    main()
