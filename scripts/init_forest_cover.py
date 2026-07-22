#!/usr/bin/env python3
"""Prepare the 2018 northern-Germany Copernicus forest-cover tile set."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ENV = ROOT / ".env"
DEFAULT_OUTPUT = ROOT / "target/world-data-sources/raw/forest-cover"
AUTH_URL = "https://identity.dataspace.copernicus.eu/auth/realms/CDSE/protocol/openid-connect/token"
PROCESS_URL = "https://sh.dataspace.copernicus.eu/api/v1/process"
COLLECTIONS = {
    "tcd": "edd3c5f5-da8e-463f-8c9a-712aa451d37e",
    "bcd": "a06a42ae-f899-4a07-a5cd-fb7fd920d6c1",
    "ccd": "a0edd575-c763-4c4a-a910-631df3df4506",
}
FORMAT = "adventuresim-copernicus-forest-2018-v1"
SOURCE = "clms-forest-2018"
VERSION = "2018"
PIXELS = 1_000
DEFAULT_BOUNDS = (8, 50, 12, 53)
MAX_RESPONSE_BYTES = 4 * 1024 * 1024
MAX_TOTAL_BYTES = 512 * 1024 * 1024
ATTEMPTS = 6
INITIAL_RETRY_SECONDS = 2
MAX_RETRY_SECONDS = 60
TOKEN_MARGIN_SECONDS = 60
USER_AGENT = "AdventureSimulator-forest-init/1"
SAFE_ENV_KEY = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")


TCD_EVALSCRIPT = """//VERSION=3
function setup() {
  return {
    input: [{ bands: ["TCD", "dataMask"] }],
    output: { bands: 1, sampleType: "UINT8", nodataValue: 255 }
  };
}
function evaluatePixel(sample) {
  return [sample.dataMask ? sample.TCD : 255];
}
"""


DLT_EVALSCRIPT = """//VERSION=3
function setup() {
  return {
    input: [
      { datasource: "bcd", bands: ["BCD", "dataMask"] },
      { datasource: "ccd", bands: ["CCD", "dataMask"] }
    ],
    output: { bands: 1, sampleType: "UINT8", nodataValue: 255 }
  };
}
function evaluatePixel(samples) {
  let broadleaf = samples.bcd.length ? samples.bcd[0] : null;
  let conifer = samples.ccd.length ? samples.ccd[0] : null;
  if (!broadleaf || !conifer || !broadleaf.dataMask || !conifer.dataMask) return [255];
  let total = broadleaf.BCD + conifer.CCD;
  if (total <= 0) return [255];
  if (broadleaf.BCD * 4 >= total * 3) return [1];
  if (conifer.CCD * 4 >= total * 3) return [2];
  return [3];
}
"""


def dotenv(path: Path) -> dict[str, str]:
    """Read a small, non-expanding subset of dotenv syntax."""
    if not path.is_file():
        return {}
    values: dict[str, str] = {}
    for number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[7:].lstrip()
        if "=" not in line:
            raise RuntimeError(f"invalid .env assignment at line {number}")
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        if not SAFE_ENV_KEY.fullmatch(key):
            raise RuntimeError(f"invalid .env key at line {number}")
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
            value = value[1:-1]
        values[key] = value
    return values


def credentials(path: Path) -> tuple[str, str]:
    file_values = dotenv(path)
    client_id = os.environ.get(
        "COPERNICUS_CLIENT_ID", file_values.get("COPERNICUS_CLIENT_ID", "")
    )
    client_secret = os.environ.get(
        "COPERNICUS_CLIENT_SECRET", file_values.get("COPERNICUS_CLIENT_SECRET", "")
    )
    if not client_id or not client_secret:
        raise RuntimeError(
            "COPERNICUS_CLIENT_ID and COPERNICUS_CLIENT_SECRET must be set in the environment or .env"
        )
    return client_id, client_secret


def retry_delay(attempt: int) -> int:
    if attempt < 1:
        raise ValueError("retry attempts start at one")
    return min(INITIAL_RETRY_SECONDS * (2 ** (attempt - 1)), MAX_RETRY_SECONDS)


def validate_bounds(bounds: tuple[int, int, int, int]) -> tuple[int, int, int, int]:
    west, south, east, north = bounds
    if not (-180 <= west < east <= 180 and -90 <= south < north <= 90):
        raise argparse.ArgumentTypeError("forest bounds must be an ordered integer EPSG:4326 rectangle")
    tile_count = (east - west) * (north - south)
    if tile_count <= 0 or tile_count > 512:
        raise argparse.ArgumentTypeError("forest bounds must contain 1..512 one-degree tiles")
    return bounds


def coordinate(value: int, positive: str, negative: str, width: int) -> str:
    return f"{positive if value >= 0 else negative}{abs(value):0{width}d}"


def tile_key(latitude: int, longitude: int) -> str:
    return f"{coordinate(latitude, 'N', 'S', 2)}_{coordinate(longitude, 'E', 'W', 3)}"


def tiles(bounds: tuple[int, int, int, int]) -> list[tuple[int, int]]:
    west, south, east, north = validate_bounds(bounds)
    return [(latitude, longitude) for latitude in range(south, north) for longitude in range(west, east)]


def data_input(collection: str, identifier: str | None = None) -> dict[str, object]:
    value: dict[str, object] = {
        "type": f"byoc-{COLLECTIONS[collection]}",
        "dataFilter": {
            "timeRange": {
                "from": "2018-01-01T00:00:00Z",
                "to": "2018-12-31T23:59:59Z",
            },
            "mosaickingOrder": "leastRecent",
        },
        "processing": {"upsampling": "NEAREST", "downsampling": "NEAREST"},
    }
    if identifier is not None:
        value["id"] = identifier
    return value


def request_payload(kind: str, latitude: int, longitude: int) -> bytes:
    if kind == "TCD":
        inputs = [data_input("tcd")]
        evalscript = TCD_EVALSCRIPT
    elif kind == "DLT":
        inputs = [data_input("bcd", "bcd"), data_input("ccd", "ccd")]
        evalscript = DLT_EVALSCRIPT
    else:
        raise ValueError("forest request kind must be TCD or DLT")
    value = {
        "input": {
            "bounds": {
                "bbox": [longitude, latitude, longitude + 1, latitude + 1],
                "properties": {"crs": "http://www.opengis.net/def/crs/EPSG/0/4326"},
            },
            "data": inputs,
        },
        "output": {
            "width": PIXELS,
            "height": PIXELS,
            "responses": [{"identifier": "default", "format": {"type": "image/tiff"}}],
        },
        "evalscript": evalscript,
    }
    return json.dumps(value, ensure_ascii=True, separators=(",", ":")).encode("utf-8")


class TokenProvider:
    def __init__(self, client_id: str, client_secret: str):
        self.client_id = client_id
        self.client_secret = client_secret
        self.value = ""
        self.expires_at = 0.0

    def invalidate(self) -> None:
        self.value = ""
        self.expires_at = 0.0

    def token(self) -> str:
        if self.value and time.monotonic() < self.expires_at - TOKEN_MARGIN_SECONDS:
            return self.value
        body = urllib.parse.urlencode({
            "grant_type": "client_credentials",
            "client_id": self.client_id,
            "client_secret": self.client_secret,
        }).encode("ascii")
        request = urllib.request.Request(
            AUTH_URL,
            data=body,
            headers={"Content-Type": "application/x-www-form-urlencoded", "User-Agent": USER_AGENT},
            method="POST",
        )
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                result = json.loads(response.read(256 * 1024))
        except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError, OSError) as error:
            raise RuntimeError("Copernicus OAuth token request failed") from error
        token = result.get("access_token")
        expires = result.get("expires_in")
        if not isinstance(token, str) or not token or not isinstance(expires, int) or not 60 <= expires <= 86_400:
            raise RuntimeError("Copernicus OAuth token response was invalid")
        self.value = token
        self.expires_at = time.monotonic() + expires
        return token


def read_tiff_value(data: bytes, endian: str, type_id: int, count: int, offset: int) -> tuple[float, ...]:
    formats = {3: "H", 4: "I", 12: "d"}
    sizes = {3: 2, 4: 4, 12: 8}
    if type_id not in formats or count <= 0:
        return ()
    size = sizes[type_id] * count
    start = offset
    if start < 0 or start + size > len(data):
        return ()
    return tuple(float(value) for value in struct.unpack_from(endian + formats[type_id] * count, data, start))


def tiff_tags(path: Path) -> dict[int, tuple[float, ...]]:
    data = path.read_bytes()
    if not 128 <= len(data) <= MAX_RESPONSE_BYTES:
        raise RuntimeError(f"{path.name} has an invalid byte size")
    if data[:2] == b"II":
        endian = "<"
    elif data[:2] == b"MM":
        endian = ">"
    else:
        raise RuntimeError(f"{path.name} is not a TIFF")
    if struct.unpack_from(endian + "H", data, 2)[0] != 42:
        raise RuntimeError(f"{path.name} has an unsupported TIFF header")
    ifd = struct.unpack_from(endian + "I", data, 4)[0]
    if ifd + 2 > len(data):
        raise RuntimeError(f"{path.name} has an invalid TIFF directory")
    count = struct.unpack_from(endian + "H", data, ifd)[0]
    if count > 256 or ifd + 2 + count * 12 > len(data):
        raise RuntimeError(f"{path.name} has an oversized TIFF directory")
    result: dict[int, tuple[float, ...]] = {}
    for index in range(count):
        entry = ifd + 2 + index * 12
        tag, type_id, values = struct.unpack_from(endian + "HHI", data, entry)
        sizes = {3: 2, 4: 4, 12: 8}
        if type_id not in sizes or values <= 0:
            continue
        byte_count = sizes[type_id] * values
        value_offset = entry + 8 if byte_count <= 4 else struct.unpack_from(endian + "I", data, entry + 8)[0]
        parsed = read_tiff_value(data, endian, type_id, values, value_offset)
        if parsed:
            result[tag] = parsed
    return result


def validate_tiff(path: Path, latitude: int, longitude: int) -> None:
    tags = tiff_tags(path)
    if tags.get(256) != (float(PIXELS),) or tags.get(257) != (float(PIXELS),):
        raise RuntimeError(f"{path.name} is not {PIXELS}x{PIXELS}")
    if tags.get(258) != (8.0,) or tags.get(277, (1.0,)) != (1.0,):
        raise RuntimeError(f"{path.name} is not a single-band UInt8 TIFF")
    scale = tags.get(33550)
    tiepoint = tags.get(33922)
    geokeys = tags.get(34735)
    if scale is None or len(scale) < 2 or abs(scale[0] - 0.001) > 1e-9 or abs(scale[1] - 0.001) > 1e-9:
        raise RuntimeError(f"{path.name} is not on the 0.001-degree grid")
    if tiepoint is None or len(tiepoint) < 6 or abs(tiepoint[3] - longitude) > 1e-9 or abs(tiepoint[4] - (latitude + 1)) > 1e-9:
        raise RuntimeError(f"{path.name} does not span its named degree tile")
    if geokeys is None or len(geokeys) < 4:
        raise RuntimeError(f"{path.name} lacks GeoTIFF CRS metadata")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def download_tiff(provider: TokenProvider, kind: str, latitude: int, longitude: int, destination: Path) -> None:
    payload = request_payload(kind, latitude, longitude)
    for attempt in range(1, ATTEMPTS + 1):
        request = urllib.request.Request(
            PROCESS_URL,
            data=payload,
            headers={
                "Authorization": f"Bearer {provider.token()}",
                "Content-Type": "application/json",
                "Accept": "image/tiff",
                "User-Agent": USER_AGENT,
            },
            method="POST",
        )
        temporary: Path | None = None
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                content_type = response.headers.get_content_type()
                length = response.headers.get("Content-Length")
                if content_type not in ("image/tiff", "image/tif"):
                    raise RuntimeError(f"Copernicus returned {content_type}, not a TIFF")
                if length is not None and int(length) > MAX_RESPONSE_BYTES:
                    raise RuntimeError("Copernicus TIFF exceeded its byte bound")
                handle, name = tempfile.mkstemp(prefix=f".{destination.name}.", suffix=".part", dir=destination.parent)
                temporary = Path(name)
                total = 0
                with os.fdopen(handle, "wb") as output:
                    while block := response.read(64 * 1024):
                        total += len(block)
                        if total > MAX_RESPONSE_BYTES:
                            raise RuntimeError("Copernicus TIFF exceeded its byte bound")
                        output.write(block)
            validate_tiff(temporary, latitude, longitude)
            os.replace(temporary, destination)
            return
        except urllib.error.HTTPError as error:
            if error.code == 401:
                provider.invalidate()
            if error.code not in (401, 408, 429, 500, 502, 503, 504) or attempt == ATTEMPTS:
                raise RuntimeError(f"Copernicus {kind} request failed with HTTP {error.code}") from error
        except (urllib.error.URLError, TimeoutError, OSError) as error:
            if attempt == ATTEMPTS:
                raise RuntimeError(f"Copernicus {kind} request failed after {ATTEMPTS} attempts") from error
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
        time.sleep(retry_delay(attempt))
    raise AssertionError("download retry loop must return or raise")


def expected_names(bounds: tuple[int, int, int, int]) -> list[str]:
    return sorted(f"{kind}_{tile_key(latitude, longitude)}.tif" for latitude, longitude in tiles(bounds) for kind in ("TCD", "DLT"))


def inventory(directory: Path, bounds: tuple[int, int, int, int]) -> dict[str, object]:
    files = []
    total = 0
    for name in expected_names(bounds):
        path = directory / name
        match = re.fullmatch(r"(?:TCD|DLT)_([NS])(\d{2})_([EW])(\d{3})\.tif", name)
        assert match is not None
        latitude = int(match.group(2)) * (1 if match.group(1) == "N" else -1)
        longitude = int(match.group(4)) * (1 if match.group(3) == "E" else -1)
        if not path.is_file():
            raise RuntimeError(f"prepared forest tile is missing: {name}")
        validate_tiff(path, latitude, longitude)
        size = path.stat().st_size
        total += size
        if total > MAX_TOTAL_BYTES:
            raise RuntimeError("prepared forest set exceeds its total byte bound")
        files.append({"name": name, "size": size, "sha256": sha256(path)})
    return {"schema": 1, "source": SOURCE, "version": VERSION, "files": files}


def write_json(path: Path, value: object) -> None:
    payload = json.dumps(value, ensure_ascii=True, indent=2, sort_keys=True) + "\n"
    handle, name = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".tmp", dir=path.parent)
    temporary = Path(name)
    try:
        with os.fdopen(handle, "w", encoding="utf-8", newline="\n") as output:
            output.write(payload)
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def verify(directory: Path, bounds: tuple[int, int, int, int]) -> dict[str, object]:
    marker = directory / "forest-cover-manifest.json"
    source_inventory = directory / "source-inventory.json"
    if json.loads(marker.read_text(encoding="utf-8")) != {"format": FORMAT}:
        raise RuntimeError("forest-cover-manifest.json has the wrong format marker")
    recorded = json.loads(source_inventory.read_text(encoding="utf-8"))
    actual = inventory(directory, bounds)
    if recorded != actual:
        raise RuntimeError("forest source inventory does not match the prepared files")
    allowed = set(expected_names(bounds)) | {marker.name, source_inventory.name}
    extras = sorted(path.name for path in directory.iterdir() if path.name not in allowed)
    if extras:
        raise RuntimeError(f"forest directory contains unexpected entries: {', '.join(extras)}")
    return actual


def publish(staging: Path, output: Path) -> Path | None:
    output.parent.mkdir(parents=True, exist_ok=True)
    backup = None
    if output.exists():
        sources_root = next((parent for parent in output.parents if parent.name == "world-data-sources"), None)
        backup_root = (
            sources_root.parent / "world-data-backups"
            if sources_root is not None
            else output.parent / "world-data-backups"
        )
        backup_root.mkdir(parents=True, exist_ok=True)
        backup = backup_root / f"forest-cover-{time.strftime('%Y%m%d-%H%M%S')}"
        if backup.exists():
            raise RuntimeError(f"backup destination already exists: {backup}")
        os.replace(output, backup)
    try:
        os.replace(staging, output)
    except BaseException:
        if backup is not None and not output.exists():
            os.replace(backup, output)
        raise
    return backup


def prepare(output: Path, env_file: Path, bounds: tuple[int, int, int, int]) -> Path | None:
    output.parent.mkdir(parents=True, exist_ok=True)
    staging = output.parent / f".{output.name}-cdse-2018-staging"
    staging.mkdir(parents=True, exist_ok=True)
    lock = output.parent / f".{output.name}-cdse-2018.lock"
    try:
        descriptor = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        os.close(descriptor)
    except FileExistsError as error:
        raise RuntimeError(f"forest initialization lock already exists: {lock}") from error
    try:
        client_id, client_secret = credentials(env_file)
        provider = TokenProvider(client_id, client_secret)
        complete = 0
        names = expected_names(bounds)
        for latitude, longitude in tiles(bounds):
            key = tile_key(latitude, longitude)
            for kind in ("TCD", "DLT"):
                destination = staging / f"{kind}_{key}.tif"
                if destination.is_file():
                    try:
                        validate_tiff(destination, latitude, longitude)
                        complete += 1
                        continue
                    except RuntimeError:
                        destination.unlink(missing_ok=True)
                existing = output / destination.name
                if existing.is_file():
                    try:
                        validate_tiff(existing, latitude, longitude)
                        shutil.copy2(existing, destination)
                        complete += 1
                        continue
                    except RuntimeError:
                        pass
                print(f"[{complete + 1}/{len(names)}] {destination.name}", flush=True)
                download_tiff(provider, kind, latitude, longitude, destination)
                complete += 1
        write_json(staging / "forest-cover-manifest.json", {"format": FORMAT})
        write_json(staging / "source-inventory.json", inventory(staging, bounds))
        verify(staging, bounds)
        return publish(staging, output)
    finally:
        lock.unlink(missing_ok=True)


def plan(env_file: Path, output: Path, bounds: tuple[int, int, int, int]) -> dict[str, object]:
    file_values = dotenv(env_file)
    configured = all(
        os.environ.get(key, file_values.get(key, ""))
        for key in ("COPERNICUS_CLIENT_ID", "COPERNICUS_CLIENT_SECRET")
    )
    return {
        "source": SOURCE,
        "version": VERSION,
        "bounds": list(bounds),
        "degree_tiles": len(tiles(bounds)),
        "prepared_files": len(expected_names(bounds)),
        "process_requests": len(expected_names(bounds)),
        "credential_preflight": "present (values redacted)" if configured else "absent",
        "env_file": str(env_file),
        "output": str(output),
        "collections": COLLECTIONS,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--plan", action="store_true")
    mode.add_argument("--prepare", action="store_true")
    mode.add_argument("--verify-only", action="store_true")
    parser.add_argument("--env-file", type=Path, default=DEFAULT_ENV)
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--west", type=int, default=DEFAULT_BOUNDS[0])
    parser.add_argument("--south", type=int, default=DEFAULT_BOUNDS[1])
    parser.add_argument("--east", type=int, default=DEFAULT_BOUNDS[2])
    parser.add_argument("--north", type=int, default=DEFAULT_BOUNDS[3])
    args = parser.parse_args()
    bounds = validate_bounds((args.west, args.south, args.east, args.north))
    if args.plan:
        print(json.dumps(plan(args.env_file, args.output_dir, bounds), indent=2, sort_keys=True))
    elif args.prepare:
        backup = prepare(args.output_dir, args.env_file, bounds)
        print(f"prepared {len(tiles(bounds))} forest degree tiles in {args.output_dir}")
        if backup is not None:
            print(f"previous partial forest data retained at {backup}")
    else:
        result = verify(args.output_dir, bounds)
        total = sum(int(entry["size"]) for entry in result["files"])
        print(f"verified {len(result['files'])} forest rasters ({total} bytes)")


if __name__ == "__main__":
    main()
