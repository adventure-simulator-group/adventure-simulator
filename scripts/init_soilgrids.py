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
MAX_SOURCE_SECONDS = 900
MAX_PIXELS = 32_000_000
HTTP_RETRIES = 8
HTTP_INITIAL_RETRY_DELAY_SECONDS = 10
HTTP_MAX_RETRY_DELAY_SECONDS = 300
WARP_ATTEMPTS = 8
CHECKPOINT_SCHEMA = 1


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


def retry_delay(attempt: int) -> int:
    """Return a bounded exponential delay before the next retry."""
    if attempt < 1:
        raise ValueError("retry attempts start at one")
    return min(HTTP_INITIAL_RETRY_DELAY_SECONDS * (2 ** (attempt - 1)),
               HTTP_MAX_RETRY_DELAY_SECONDS)


def command(layer: dict[str, str], vrt: str | Path, output: Path, size: int) -> list[str]:
    west, south, east, north = EXTENT
    return ["gdalwarp",
        "--config", f"GDAL_HTTP_MAX_RETRY={HTTP_RETRIES}",
        "--config", f"GDAL_HTTP_RETRY_DELAY={HTTP_INITIAL_RETRY_DELAY_SECONDS}",
        "--config", "GDAL_HTTP_RETRY_CODES=429,500,502,503,504",
        "--config", "GDAL_HTTP_TCP_KEEPALIVE=YES",
        "-overwrite", "-of", "GTiff", "-ot", "Float32", "-srcnodata",
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
    for attempt in range(1, HTTP_RETRIES + 1):
        try:
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
        except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError, OSError) as error:
            if attempt == HTTP_RETRIES:
                raise RuntimeError(f"SoilGrids source metadata failed after {HTTP_RETRIES} attempts: {url}") from error
            time.sleep(retry_delay(attempt))
    raise AssertionError("metadata retry loop must return or raise")


def warp_with_retries(layer: dict[str, str], output: Path, size: int) -> None:
    arguments = command(layer, vsicurl(layer["source_url"]), output, size)
    for attempt in range(1, WARP_ATTEMPTS + 1):
        output.unlink(missing_ok=True)
        try:
            subprocess.run(arguments, check=True, shell=False)
            return
        except subprocess.CalledProcessError:
            output.unlink(missing_ok=True)
            if attempt == WARP_ATTEMPTS:
                raise
            time.sleep(retry_delay(attempt))


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


def checkpoint_path(staging: Path) -> Path:
    return staging / "checkpoint.json"


def checkpoint_header(size: int) -> dict[str, object]:
    west, south, east, north = EXTENT
    return {"schema": CHECKPOINT_SCHEMA, "source": "ISRIC SoilGrids rolling-v2",
        "source_version": "latest", "crs": "EPSG:3035", "cell_size_meters": size,
        "west": west, "south": south, "east": east, "north": north}


def load_checkpoint(staging: Path, size: int) -> list[dict[str, object]]:
    """Load and validate completed layers retained in an interrupted staging run."""
    path = checkpoint_path(staging)
    if not path.exists():
        return []
    if path.stat().st_size > MAX_MANIFEST_BYTES:
        raise RuntimeError("staging checkpoint exceeds size cap")
    checkpoint = json.loads(path.read_text(encoding="utf-8"))
    if any(checkpoint.get(key) != value for key, value in checkpoint_header(size).items()):
        raise RuntimeError("staging checkpoint has a different source or grid contract")
    records = checkpoint.get("files")
    if not isinstance(records, list) or len(records) > len(layers()):
        raise RuntimeError("staging checkpoint file inventory is invalid")
    expected = {(x["property"], x["depth"], x["quantile"]): x for x in layers()}
    root = staging.resolve()
    identities, filenames = set(), set()
    for record in records:
        identity = (record.get("property"), record.get("depth"), record.get("quantile"))
        canonical = expected.get(identity)
        if identity in identities or record.get("filename") in filenames or canonical is None:
            raise RuntimeError("staging checkpoint has duplicate or unexpected layer")
        if any(record.get(key) != canonical[key] for key in ("filename", "source_url", "unit")):
            raise RuntimeError("staging checkpoint layer differs from canonical inventory")
        prepared = (staging / canonical["filename"]).resolve()
        if prepared.parent != root:
            raise RuntimeError("staging checkpoint path escapes staging directory")
        if not prepared.is_file() or prepared.stat().st_size != record.get("prepared_size") or sha256(prepared) != record.get("prepared_sha256"):
            raise RuntimeError(f"staging checkpoint file mismatch: {canonical['filename']}")
        validate_prepared(prepared, size)
        identities.add(identity)
        filenames.add(canonical["filename"])
    return records


def save_checkpoint(staging: Path, size: int, records: list[dict[str, object]]) -> None:
    checkpoint = {**checkpoint_header(size), "files": records}
    temporary = staging / ".checkpoint.json.tmp"
    temporary.write_text(json.dumps(checkpoint, indent=2) + "\n", encoding="utf-8")
    os.replace(temporary, checkpoint_path(staging))


def discard_uncheckpointed_layers(staging: Path, records: list[dict[str, object]]) -> None:
    """Drop only incomplete raster outputs left by a process killed between checkpoints."""
    retained = {record["filename"] for record in records}
    expected = {layer["filename"] for layer in layers()}
    for prepared in staging.glob("*.tif"):
        if prepared.name in expected and prepared.name not in retained:
            prepared.unlink()


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
    staging = generations / ".soilgrids-staging"
    staging.mkdir(exist_ok=True)
    records = load_checkpoint(staging, args.grid_cell_size_meters)
    discard_uncheckpointed_layers(staging, records)
    records_by_identity = {(record["property"], record["depth"], record["quantile"]): record for record in records}
    for layer in layers():
        identity = (layer["property"], layer["depth"], layer["quantile"])
        if identity in records_by_identity:
            print(f"Reusing checkpointed {layer['filename']}")
            continue
        prepared = staging / layer["filename"]
        metadata = source_metadata(layer["source_url"])
        warp_with_retries(layer, prepared, args.grid_cell_size_meters)
        validate_prepared(prepared, args.grid_cell_size_meters)
        record = {**layer, **metadata, "prepared_size": prepared.stat().st_size,
            "prepared_sha256": sha256(prepared)}
        records.append(record)
        records_by_identity[identity] = record
        save_checkpoint(staging, args.grid_cell_size_meters, records)
    if len(records) != 207:
        raise RuntimeError("staged generation is incomplete")
    for record in records:
        prepared = staging / record["filename"]
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
    os.replace(staging, generation_dir)
    temporary_manifest = args.output_dir / ".soilgrids-manifest.json.tmp"
    temporary_manifest.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    os.replace(temporary_manifest, args.output_dir / "soilgrids-manifest.json")
    verify(args.output_dir, args.grid_cell_size_meters)


if __name__ == "__main__":
    main()
