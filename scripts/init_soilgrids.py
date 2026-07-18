#!/usr/bin/env python3
"""Plan, prepare, or verify the pinned SoilGrids rolling-v2 Europe subset."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import urllib.request
import urllib.error
from urllib.parse import urlparse

BASE = "https://files.isric.org/soilgrids/latest/data"
PROPERTIES = {
    "sand": "g/kg", "silt": "g/kg", "clay": "g/kg", "cfvo": "cm3/dm3",
    "soc": "dg/kg", "phh2o": "pH*10", "cec": "mmol(c)/kg", "bdod": "cg/cm3",
    "wv0033": "10^-3 cm3/cm3", "wv1500": "10^-3 cm3/cm3",
}
DEPTHS = ("0-5cm", "5-15cm", "15-30cm", "30-60cm", "60-100cm", "100-200cm")
QUANTILES = ("Q0.05", "Q0.50", "mean", "Q0.95")
WRB = ("most-probable", "Histosols-probability", "Leptosols-probability")
EXTENT = (900_000, 900_000, 7_400_000, 5_500_000)
MAX_SOURCE_BYTES = 256 * 1024 * 1024
MAX_MANIFEST_BYTES = 2 * 1024 * 1024
MAX_SOURCE_SECONDS = 300
MAX_PIXELS = 32_000_000


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def cell_size(value: str) -> int:
    parsed = int(value)
    if not 250 <= parsed <= 100_000 or parsed % 250:
        raise argparse.ArgumentTypeError("grid cell size must be 250..100000 and divisible by 250")
    validate_grid(parsed)
    return parsed


def validate_grid(size: int) -> tuple[int, int]:
    return validate_extent(EXTENT, size)


def validate_extent(extent: tuple[int, int, int, int], size: int) -> tuple[int, int]:
    if extent != EXTENT:
        raise argparse.ArgumentTypeError("SoilGrids preparation requires the fixed Europe extent")
    west, south, east, north = extent
    if any(boundary % size for boundary in extent):
        raise argparse.ArgumentTypeError("SoilGrids extent is shifted off the zero-origin grid")
    width, height = east - west, north - south
    if width <= 0 or height <= 0 or width % size or height % size:
        raise argparse.ArgumentTypeError("SoilGrids extent must be exactly divisible by grid cell size")
    columns, rows = width // size, height // size
    if columns * rows > MAX_PIXELS:
        raise argparse.ArgumentTypeError(f"prepared raster exceeds {MAX_PIXELS} pixel importer bound")
    return columns, rows


def layers() -> list[dict[str, str]]:
    result = []
    for prop, unit in PROPERTIES.items():
        for depth in DEPTHS:
            for quantile in (("mean",) if prop in ("wv0033", "wv1500") else QUANTILES):
                stem = f"{prop}_{depth}_{quantile}"
                if prop in ("wv0033", "wv1500"):
                    url = f"https://files.isric.org/soilgrids/latest/data_aggregated/1000m/{prop}/{stem}_1000.tif"
                else:
                    remote_quantile = "Q0.5" if quantile == "Q0.50" else quantile
                    url = f"{BASE}/{prop}/{prop}_{depth}_{remote_quantile}.vrt"
                result.append({"property": prop, "depth": depth, "quantile": quantile,
                    "unit": unit, "filename": f"{stem}.tif", "source_url": url})
    for quantile, remote in (("most-probable", "MostProbable"),
                             ("Histosols-probability", "Histosols"),
                             ("Leptosols-probability", "Leptosols")):
        stem = f"wrb_{quantile}"
        result.append({"property": "wrb", "depth": "surface", "quantile": quantile,
            "unit": "class-or-percent", "filename": f"{stem}.tif", "source_url": f"{BASE}/wrb/{remote}.vrt"})
    return result


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def command(layer: dict[str, str], vrt: str | Path, output: Path, size: int) -> list[str]:
    west, south, east, north = EXTENT
    return ["gdalwarp", "-overwrite", "-of", "GTiff", "-ot", "Float32", "-srcnodata",
        "255" if layer["property"] == "wrb" else "-32768", "-dstnodata", "nan",
        "-co", "TILED=YES", "-co", "COMPRESS=DEFLATE",
        "-t_srs", "EPSG:3035", "-te", str(west), str(south), str(east), str(north),
        "-tr", str(size), str(size), "-tap", "-r", "near" if layer["property"] == "wrb" else "average",
        str(vrt), str(output)]


def vsicurl(url: str) -> str:
    parsed = urlparse(url)
    if parsed.scheme != "https" or parsed.hostname != "files.isric.org" or not parsed.path.startswith("/soilgrids/latest/") or parsed.query or parsed.fragment:
        raise RuntimeError("SoilGrids source URL is outside the fixed HTTPS host/path")
    return "/vsicurl/" + url


def source_metadata(url: str) -> dict[str, object]:
    vsicurl(url)
    request = urllib.request.Request(url, headers={"User-Agent": "adventure-simulator-world-import/1"})
    started = time.monotonic()
    digest = hashlib.sha256()
    opener = urllib.request.build_opener(NoRedirect)
    with opener.open(request, timeout=30) as response:
        final_url = response.geturl()
        if final_url != url:
            raise RuntimeError("SoilGrids fixed URL redirected; update the source contract explicitly")
        length = response.headers.get("Content-Length")
        if length is not None and int(length) > MAX_SOURCE_BYTES:
            raise RuntimeError(f"source exceeds {MAX_SOURCE_BYTES} byte bound")
        total = 0
        while chunk := response.read(64 * 1024):
            if time.monotonic() - started > MAX_SOURCE_SECONDS:
                raise RuntimeError("SoilGrids source retrieval exceeded total-time bound")
            total += len(chunk)
            if total > MAX_SOURCE_BYTES:
                raise RuntimeError(f"source exceeds {MAX_SOURCE_BYTES} byte bound")
            digest.update(chunk)
        return {"source_observation_size": total,
            "source_observation_sha256": digest.hexdigest(), "source_observation_etag": response.headers.get("ETag"),
            "source_observation_last_modified": response.headers.get("Last-Modified")}


def validate_prepared(path: Path, size: int) -> None:
    columns, rows = validate_grid(size)
    result = subprocess.run(["gdalinfo", "-json", str(path)], check=True, shell=False,
        capture_output=True, text=True)
    info = json.loads(result.stdout)
    transform = info.get("geoTransform")
    bands = info.get("bands", [])
    wkt = info.get("coordinateSystem", {}).get("wkt", "")
    compression = info.get("metadata", {}).get("IMAGE_STRUCTURE", {}).get("COMPRESSION")
    if info.get("size") != [columns, rows] or transform != [EXTENT[0], size, 0.0, EXTENT[3], 0.0, -size]:
        raise RuntimeError("prepared SoilGrids raster size/geotransform mismatch")
    if 'ID["EPSG",3035]' not in wkt.replace(" ", "") and 'AUTHORITY["EPSG","3035"]' not in wkt.replace(" ", ""):
        raise RuntimeError("prepared SoilGrids raster is not EPSG:3035")
    if len(bands) != 1 or bands[0].get("type") != "Float32" or str(bands[0].get("noDataValue")).lower() != "nan":
        raise RuntimeError("prepared SoilGrids raster must be single-band Float32 with NaN nodata")
    if compression != "DEFLATE":
        raise RuntimeError("prepared SoilGrids raster must use DEFLATE compression")


def verify(directory: Path, size: int) -> None:
    manifest_path = directory / "soilgrids-manifest.json"
    if manifest_path.stat().st_size > MAX_MANIFEST_BYTES:
        raise RuntimeError("manifest exceeds size cap")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest["schema"] != 1 or manifest["source"] != "ISRIC SoilGrids rolling-v2":
        raise RuntimeError("manifest source/schema mismatch")
    validate_grid(size)
    if tuple(manifest[k] for k in ("west", "south", "east", "north")) != EXTENT:
        raise RuntimeError("manifest extent is not the fixed Europe extent")
    if any(boundary % size for boundary in EXTENT):
        raise RuntimeError("manifest extent is not congruent to the zero-origin grid")
    if manifest["source_reproducibility"] != "unpinned-rolling-latest" or manifest["crs"] != "EPSG:3035" or manifest["cell_size_meters"] != size:
        raise RuntimeError("manifest grid mismatch")
    expected = {(x["property"], x["depth"], x["quantile"]): x for x in layers()}
    if len(manifest["files"]) != 207:
        raise RuntimeError("manifest must contain exactly 207 files")
    identities, filenames = set(), set()
    generation = manifest["generation"]
    if not generation or any(c not in "0123456789abcdef" for c in generation):
        raise RuntimeError("unsafe generation identifier")
    root = (directory / "generations" / generation).resolve()
    if root.parent != (directory / "generations").resolve():
        raise RuntimeError("generation escapes output directory")
    for entry in manifest["files"]:
        identity = (entry["property"], entry["depth"], entry["quantile"])
        canonical = expected.get(identity)
        if identity in identities or entry["filename"] in filenames or canonical is None:
            raise RuntimeError("duplicate or unexpected manifest layer")
        identities.add(identity); filenames.add(entry["filename"])
        if any(entry[k] != canonical[k] for k in ("filename", "source_url", "unit")):
            raise RuntimeError("manifest layer differs from canonical inventory")
        path = (root / entry["filename"]).resolve()
        if path.parent != root:
            raise RuntimeError("prepared path escapes generation directory")
        if path.stat().st_size != entry["prepared_size"] or sha256(path) != entry["prepared_sha256"]:
            raise RuntimeError(f"prepared file mismatch: {entry['filename']}")
    if identities != set(expected):
        raise RuntimeError("manifest layer inventory mismatch")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, default=Path("target/world-data-sources/prepared/soilgrids"))
    parser.add_argument("--grid-cell-size-meters", type=cell_size, default=1000)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--prepare", action="store_true", help="download fixed VRTs and execute gdalwarp")
    mode.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    if args.verify_only:
        verify(args.output_dir, args.grid_cell_size_meters)
        print("SoilGrids prepared subset verified")
        return
    if not args.prepare:
        for layer in layers():
            print(" ".join(command(layer, vsicurl(layer["source_url"]), args.output_dir / "<staged-generation>" / layer["filename"], args.grid_cell_size_meters)))
        print("Bootstrap plan only: rolling `latest` is unpinned; a completed generation snapshots hashes but cannot make raw reacquisition reproducible.")
        return
    if shutil.which("gdalwarp") is None or shutil.which("gdalinfo") is None:
        raise RuntimeError("gdalwarp and gdalinfo are required for --prepare")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    generations = args.output_dir / "generations"
    generations.mkdir(exist_ok=True)
    records = []
    temp = Path(tempfile.mkdtemp(prefix=".soilgrids-generation-", dir=generations))
    try:
        for index, layer in enumerate(layers()):
            prepared = temp / layer["filename"]
            metadata = source_metadata(layer["source_url"])
            subprocess.run(command(layer, vsicurl(layer["source_url"]), prepared, args.grid_cell_size_meters), check=True, shell=False)
            records.append({**layer, **metadata, "prepared_size": prepared.stat().st_size,
                "prepared_sha256": sha256(prepared)})
        if len(records) != 207:
            raise RuntimeError("staged generation is incomplete")
        for record in records:
            prepared = temp / record["filename"]
            validate_prepared(prepared, args.grid_cell_size_meters)
            if prepared.stat().st_size != record["prepared_size"] or sha256(prepared) != record["prepared_sha256"]:
                raise RuntimeError("staged generation changed before publication")
        from datetime import datetime, timezone
        west, south, east, north = EXTENT
        manifest = {"schema": 1, "source": "ISRIC SoilGrids rolling-v2", "source_version": "latest",
            "source_reproducibility": "unpinned-rolling-latest",
            "retrieved_at": datetime.now(timezone.utc).isoformat(), "crs": "EPSG:3035",
            "origin_easting_meters": 0, "origin_northing_meters": 0,
            "cell_size_meters": args.grid_cell_size_meters, "west": west, "south": south,
            "east": east, "north": north, "files": records}
        generation = hashlib.sha256(json.dumps(manifest, sort_keys=True).encode()).hexdigest()
        manifest["generation"] = generation
        generation_dir = generations / generation
        if generation_dir.exists():
            raise RuntimeError("generation already exists; verify or choose a fresh output directory")
        os.replace(temp, generation_dir)
        temporary_manifest = args.output_dir / ".soilgrids-manifest.json.tmp"
        temporary_manifest.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        os.replace(temporary_manifest, args.output_dir / "soilgrids-manifest.json")
        verify(args.output_dir, args.grid_cell_size_meters)
    finally:
        if temp.exists():
            shutil.rmtree(temp)


if __name__ == "__main__":
    main()
