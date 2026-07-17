#!/usr/bin/env python3
"""Plan, initialize, or verify accepted world-data source distributions.

This module is deliberately fail closed.  A source without a checked immutable
inventory can be planned and locally verified, but it cannot be downloaded.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import re
from dataclasses import dataclass
from pathlib import Path
import shutil
import stat as stat_module
import tempfile
import time
import urllib.error
import urllib.request
from urllib.parse import urlparse

CHUNK = 1024 * 1024
MAX_MANIFEST_BYTES = 1024 * 1024
MAX_FILE_BYTES = 3 * 1024 * 1024 * 1024
MAX_TOTAL_BYTES = 4 * 1024 * 1024 * 1024
CONNECT_TIMEOUT_SECONDS = 30
TOTAL_TIMEOUT_SECONDS = 900
USER_AGENT = "AdventureSimulator-world-init/1"


@dataclass(frozen=True)
class Contract:
    source: str
    version: str
    access: str
    license: str
    canonical_url: str
    doi: str | None
    output: str
    expected_names: tuple[str, ...]
    crs: str
    resolution: str
    temporal: str
    preparation: str
    blocked_reason: str | None
    credential_env: str | None = None


CONTRACTS = {
    "glo30": Contract("copernicus-dem-glo30", "GLO-30", "authenticated", "Copernicus DEM licence",
        "https://doi.org/10.5270/ESA-c5d3d65", "10.5270/ESA-c5d3d65",
        "target/world-data-sources/raw/elevation", (), "EPSG:4326", "1 arc-second",
        "timeless terrain", "direct one-degree DEM tile sampling",
        "required GLO-30 tile IDs and per-tile sizes/SHA-256 are not committed", "CDSE_TOKEN_FILE"),
    "hyde": Contract("hyde-3-2-1", "3.2.1", "restricted-record", "rights conflict: CC0 record; attribution README",
        "https://doi.org/10.17026/dans-25g-gez3", "10.17026/dans-25g-gez3",
        "target/world-data-sources/raw/historical-land-use",
        ("cropland1500AD.asc", "cropland1600AD.asc", "garea_cr.asc", "grazing1500AD.asc", "grazing1600AD.asc", "urban1500AD.asc", "urban1600AD.asc"),
        "EPSG:4326", "5 arc-minute", "1500 and 1600 CE",
        "extract seven corrected ESRI ASCII grids from the official baseline archive",
        "the seven consumed files lack a committed immutable size/SHA-256 inventory and publisher rights signals conflict"),
    "forest": Contract("clms-forest-2018", "2018", "authenticated", "Copernicus full, free and open data policy",
        "https://land.copernicus.eu/en/products/high-resolution-layer-forests-and-tree-cover", "10.2909/82f93572-9888-47ef-97a1-5cac5985a26a",
        "target/world-data-sources/raw/forest-cover", (), "EPSG:4326", "prepared 0.001-degree",
        "2018", "prepare paired one-degree TCD/DLT UInt8 GeoTIFF tiles",
        "required CLMS product items and complete TCD/DLT tile inventory are not committed", "CLMS_TOKEN_FILE"),
    "trees4f": Contract("eu-trees4f-v2", "2", "anonymous", "CC0-1.0",
        "https://ies-ows.jrc.ec.europa.eu/efdac/download/EU-Trees4F/EU-Trees4F_ens-clim.zip", "10.6084/m9.figshare.17032328",
        "target/world-data-sources/raw/tree-species", ("EU-Trees4F_ens-clim.zip",),
        "EPSG:4326 and EPSG:3035", "5 arc-minute and 10 km", "current-climate reference scenario",
        "consume the pinned current-climate ensemble archive directly", None),
    "egdi": Contract("egdi-surface-geology-1m", "EGDI-GE-1M-SURFACE", "licensed-anonymous-service", "CC BY 4.0 plus Maltese notice",
        "https://metadata.europe-geology.eu/record/full/5729ffdf-2558-48fc-a5d2-645a0a010855", None,
        "target/world-data-sources/raw/geology", ("GeologicUnitView.gpkg",), "EPSG:3034", "1:1,000,000",
        "modern mapped geology", "export indexed GeologicUnitView GeoPackage from the EGDI service",
        "the accepted aggregate lacks a committed exact byte size and SHA-256"),
    "eu-hydro": Contract("copernicus-eu-hydro-1-3", "1.3", "authenticated", "Copernicus full, free and open data policy",
        "https://doi.org/10.2909/393359a7-7ebd-4a52-80ac-1a18d5f3db9c", "10.2909/393359a7-7ebd-4a52-80ac-1a18d5f3db9c",
        "target/world-data-sources/raw/hydrology", (), "EPSG:3035", "vector river network",
        "primarily 2006/2009/2012 imagery", "extract the pinned basin GeoPackage distribution",
        "official archive/item IDs and complete basin GeoPackage inventory are not committed", "CLMS_TOKEN_FILE"),
}

TREES_FILE = {
    "name": "EU-Trees4F_ens-clim.zip",
    "size": 73_796_217,
    "sha256": "be115f771e5598e6fd180621e1a32922880cf7ac8e2cb59ba0eabd7f15bfeda4",
}
RELIGION_FILE = Path("assets/world-data/ieg-religion-1544.csv")
RELIGION_SIZE = 1_069
RELIGION_SHA256 = "7172f286bad8ef9d0bb891dfe576b3c7978ce04de14eba254a300a81299aaa8e"
RELIGION_HEADER = ("priority", "region", "min_latitude", "max_latitude", "min_longitude", "max_longitude", "status", "religions", "church")
RELIGION_STATUSES = {"established", "multi_confessional", "parity", "locally_determined"}
SAFE_BASENAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*\Z")
WINDOWS_DEVICES = {"CON", "PRN", "AUX", "NUL", *(f"COM{i}" for i in range(1, 10)), *(f"LPT{i}" for i in range(1, 10))}
GLO30_NAME = re.compile(r"Copernicus_DSM_COG_10_([NS])(\d{2})_00_([EW])(\d{3})_00_DEM\.tif\Z")
FOREST_NAME = re.compile(r"(TCD|DLT)_(([NS])(\d{2})_([EW])(\d{3}))\.tif\Z")


class RedirectPolicy(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, new_url):
        validate_url(new_url, {"ies-ows.jrc.ec.europa.eu"}, "/efdac/download/EU-Trees4F/")
        return super().redirect_request(request, fp, code, message, headers, new_url)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(CHUNK), b""):
            digest.update(block)
    return digest.hexdigest()


def validate_basename(name: object) -> str:
    if not isinstance(name, str) or not SAFE_BASENAME.fullmatch(name) or name.endswith((".", " ")) or ":" in name:
        raise RuntimeError("inventory contains an unsafe filename")
    if name.split(".", 1)[0].upper() in WINDOWS_DEVICES:
        raise RuntimeError("inventory contains a reserved Windows device filename")
    return name


def has_reparse_point(path: Path) -> bool:
    try:
        stat = path.lstat()
    except FileNotFoundError:
        return False
    return path.is_symlink() or bool(getattr(stat, "st_file_attributes", 0) & getattr(stat_module, "FILE_ATTRIBUTE_REPARSE_POINT", 0))


def validated_directory_child(root: Path, *parts: str, create: bool = False) -> Path:
    resolved_root = root.resolve()
    current = root
    for part in parts:
        validate_basename(part)
        current = current / part
        if has_reparse_point(current):
            raise RuntimeError("source path contains a symlink, junction, or reparse point")
        if create and not current.exists():
            current.mkdir()
        resolved = current.resolve()
        if resolved_root not in resolved.parents:
            raise RuntimeError("source path escapes its source directory")
    return current.resolve()


def validate_url(url: str, hosts: set[str], path_prefix: str) -> None:
    parsed = urlparse(url)
    if parsed.scheme != "https" or parsed.hostname not in hosts or not parsed.path.startswith(path_prefix) or parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise RuntimeError("source or redirect URL is outside the fixed HTTPS host/path allowlist")


def safe_child(root: Path, name: str) -> Path:
    validate_basename(name)
    resolved_root = root.resolve()
    lexical = resolved_root / name
    if has_reparse_point(lexical):
        raise RuntimeError("source files may not be symbolic links, junctions, or reparse points")
    candidate = lexical.resolve()
    if candidate.parent != resolved_root:
        raise RuntimeError("inventory path escapes its source directory")
    return candidate


def canonical_manifest(contract: Contract, files: list[dict[str, object]], status: str, reason: str | None = None) -> dict[str, object]:
    result: dict[str, object] = {
        "schema": 1, "source": contract.source, "version": contract.version,
        "status": status, "access": contract.access, "license": contract.license,
        "canonical_url": contract.canonical_url, "doi": contract.doi,
        "crs": contract.crs, "resolution": contract.resolution,
        "temporal": contract.temporal, "preparation": contract.preparation,
        "files": sorted(files, key=lambda value: str(value["name"])),
    }
    if reason is not None:
        result["blocked_reason"] = reason
    if contract.source == "eu-trees4f-v2":
        result["distribution_note"] = "Identity pins the exact JRC ENS_CLIM archive; byte equivalence to a Figshare-hosted archive has not been established. Retain the EU-Trees4F v2 citation and CC0 notice."
    return result


def write_atomic(path: Path, value: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(value, ensure_ascii=True, indent=2, sort_keys=True) + "\n"
    handle, name = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".tmp", dir=path.parent)
    os.close(handle)
    temporary = Path(name)
    try:
        temporary.write_text(payload, encoding="utf-8")
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def credential_status(contract: Contract) -> str:
    if contract.credential_env is None:
        return "not-required"
    raw = os.environ.get(contract.credential_env)
    if not raw:
        return f"absent ({contract.credential_env})"
    path = Path(raw)
    if not path.is_file() or path.is_symlink() or path.stat().st_size == 0 or path.stat().st_size > 64 * 1024:
        return f"invalid ({contract.credential_env})"
    return f"present ({contract.credential_env}; value redacted)"


def plan(contract: Contract) -> dict[str, object]:
    files = ([TREES_FILE] if contract.source == "eu-trees4f-v2" else [{"name": name, "size": None, "sha256": None} for name in contract.expected_names])
    result = canonical_manifest(contract, files, "ready" if contract.blocked_reason is None else "release-blocked", contract.blocked_reason)
    result["credential_preflight"] = credential_status(contract)
    result["output"] = contract.output
    return result


def load_inventory(directory: Path, contract: Contract) -> list[dict[str, object]]:
    path = safe_child(directory, "source-inventory.json")
    if not path.is_file() or path.stat().st_size > MAX_MANIFEST_BYTES:
        raise RuntimeError("checked source-inventory.json is absent or oversized")
    value = json.loads(path.read_text(encoding="utf-8"))
    if set(value) != {"schema", "source", "version", "files"} or value["schema"] != 1 or value["source"] != contract.source or value["version"] != contract.version or not isinstance(value["files"], list):
        raise RuntimeError("inventory schema/source/version mismatch or unknown fields")
    files = value["files"]
    names: set[str] = set()
    destinations: set[str] = set()
    for entry in files:
        if set(entry) != {"name", "size", "sha256"} or not isinstance(entry["size"], int) or not 0 < entry["size"] <= MAX_FILE_BYTES or not isinstance(entry["sha256"], str) or len(entry["sha256"]) != 64 or any(c not in "0123456789abcdef" for c in entry["sha256"]):
            raise RuntimeError("inventory has an unknown, malformed, or oversized entry")
        name = validate_basename(entry["name"])
        destination = str(safe_child(directory, name)).casefold()
        if entry["name"] in names:
            raise RuntimeError("inventory contains a duplicate filename")
        if destination in destinations or name.casefold().rstrip(" .") in {existing.casefold().rstrip(" .") for existing in names}:
            raise RuntimeError("inventory contains duplicate normalized destinations")
        names.add(name)
        destinations.add(destination)
    if files != sorted(files, key=lambda entry: entry["name"]):
        raise RuntimeError("inventory files are not in canonical filename order")
    if contract.expected_names and names != set(contract.expected_names):
        raise RuntimeError("inventory is missing or adds a consumed source file")
    if not contract.expected_names and not files:
        raise RuntimeError("inventory must pin at least one consumed source file")
    if sum(entry["size"] for entry in files) > MAX_TOTAL_BYTES:
        raise RuntimeError("inventory exceeds the total byte cap")
    validate_source_inventory(contract, names)
    return files


def validate_source_inventory(contract: Contract, names: set[str]) -> None:
    if contract.source == "copernicus-dem-glo30":
        matches = [GLO30_NAME.fullmatch(name) for name in names]
        if not names or any(
            match is None
            or int(match.group(2)) > 90
            or int(match.group(4)) > 180
            or (match.group(1) == "N" and int(match.group(2)) == 90)
            or (match.group(3) == "E" and int(match.group(4)) == 180)
            for match in matches
        ):
            raise RuntimeError("GLO-30 inventory must contain only importer-compatible DEM TIFF tile names")
    elif contract.source == "clms-forest-2018":
        pairs: dict[str, set[str]] = {}
        for name in names:
            match = FOREST_NAME.fullmatch(name)
            if (
                match is None
                or int(match.group(4)) > 90
                or int(match.group(6)) > 180
                or (match.group(3) == "N" and int(match.group(4)) == 90)
                or (match.group(5) == "E" and int(match.group(6)) == 180)
            ):
                raise RuntimeError("forest inventory must contain only importer-compatible TCD/DLT TIFF tile names")
            pairs.setdefault(match.group(2), set()).add(match.group(1))
        if not pairs or any(kinds != {"TCD", "DLT"} for kinds in pairs.values()):
            raise RuntimeError("forest inventory must contain a complete TCD/DLT pair for every tile key")
    elif contract.source == "copernicus-eu-hydro-1-3":
        if not names or any(not name.lower().endswith(".gpkg") for name in names):
            raise RuntimeError("EU-Hydro inventory must contain at least one safe GeoPackage filename")


def verify_inventory(directory: Path, contract: Contract) -> list[dict[str, object]]:
    entries = load_inventory(directory, contract)
    for entry in entries:
        path = safe_child(directory, str(entry["name"]))
        if not path.is_file() or path.stat().st_size != entry["size"] or sha256(path) != entry["sha256"]:
            raise RuntimeError(f"source identity mismatch: {entry['name']}")
    return entries


def set_remaining_timeout(response: object, seconds: float) -> None:
    setter = getattr(response, "settimeout", None)
    if callable(setter):
        setter(max(0.001, seconds))
        return
    candidate = response
    for attribute in ("fp", "raw", "_sock"):
        candidate = getattr(candidate, attribute, None)
        if candidate is None:
            return
        setter = getattr(candidate, "settimeout", None)
        if callable(setter):
            setter(max(0.001, seconds))
            return


def deadline_read(response: object, size: int, deadline: float) -> bytes:
    before = time.monotonic()
    if before >= deadline:
        raise RuntimeError("EU-Trees4F retrieval exceeded its total deadline")
    set_remaining_timeout(response, deadline - before)
    reader = getattr(response, "read1", None)
    block = reader(size) if callable(reader) else response.read(size)
    if time.monotonic() > deadline:
        raise RuntimeError("EU-Trees4F retrieval exceeded its total deadline")
    return block


def publish_tree_generation(directory: Path, candidate: Path) -> None:
    generations = validated_directory_child(directory, "generations", create=True)
    generation = validated_directory_child(generations, str(TREES_FILE["sha256"]), create=True)
    generation_file = safe_child(generation, str(TREES_FILE["name"]))
    if generation_file.is_file() and generation_file.stat().st_size == TREES_FILE["size"] and sha256(generation_file) == TREES_FILE["sha256"]:
        return
    if generation_file.exists() and has_reparse_point(generation_file):
        raise RuntimeError("generation file is a symlink, junction, or reparse point")
    handle, temporary_name = tempfile.mkstemp(prefix=".tree-generation.", suffix=".tmp", dir=generation)
    temporary = Path(temporary_name)
    try:
        with candidate.open("rb") as source, os.fdopen(handle, "wb") as output:
            shutil.copyfileobj(source, output, CHUNK)
        if temporary.stat().st_size != TREES_FILE["size"] or sha256(temporary) != TREES_FILE["sha256"]:
            raise RuntimeError("content-addressed generation verification failed")
        os.replace(temporary, generation_file)
    finally:
        try:
            os.close(handle)
        except OSError:
            pass
        temporary.unlink(missing_ok=True)


def download_trees(directory: Path, *, deadline_seconds: int = TOTAL_TIMEOUT_SECONDS, force: bool = False) -> None:
    contract = CONTRACTS["trees4f"]
    validate_url(contract.canonical_url, {"ies-ows.jrc.ec.europa.eu"}, "/efdac/download/EU-Trees4F/")
    if has_reparse_point(directory):
        raise RuntimeError("output directory may not be a symlink, junction, or reparse point")
    directory.mkdir(parents=True, exist_ok=True)
    current = safe_child(directory, TREES_FILE["name"])
    if current.is_file() and current.stat().st_size == TREES_FILE["size"] and sha256(current) == TREES_FILE["sha256"]:
        publish_tree_generation(directory, current)
        write_atomic(directory / "source-manifest.json", canonical_manifest(contract, [TREES_FILE], "verified"))
        return
    if current.exists() and not force:
        raise RuntimeError("existing EU-Trees4F archive is invalid; pass --force to replace it after a new candidate verifies")
    deadline = time.monotonic() + deadline_seconds
    handle, name = tempfile.mkstemp(prefix=".EU-Trees4F_ens-clim.", suffix=".part", dir=directory)
    os.close(handle)
    temporary = Path(name)
    try:
        request = urllib.request.Request(contract.canonical_url, headers={"User-Agent": USER_AGENT})
        opener = urllib.request.build_opener(RedirectPolicy)
        with opener.open(request, timeout=CONNECT_TIMEOUT_SECONDS) as response, temporary.open("wb") as output:
            validate_url(response.geturl(), {"ies-ows.jrc.ec.europa.eu"}, "/efdac/download/EU-Trees4F/")
            length = response.headers.get("Content-Length")
            if length is not None and int(length) != TREES_FILE["size"]:
                raise RuntimeError("EU-Trees4F Content-Length differs from the pinned size")
            total = 0
            while block := deadline_read(response, 64 * 1024, deadline):
                total += len(block)
                if total > TREES_FILE["size"]:
                    raise RuntimeError("EU-Trees4F retrieval exceeded its pinned byte size")
                output.write(block)
        if temporary.stat().st_size != TREES_FILE["size"] or sha256(temporary) != TREES_FILE["sha256"]:
            raise RuntimeError("EU-Trees4F archive checksum or size mismatch")
        publish_tree_generation(directory, temporary)
        os.replace(temporary, current)
        write_atomic(directory / "source-manifest.json", canonical_manifest(contract, [TREES_FILE], "verified"))
    finally:
        temporary.unlink(missing_ok=True)


def verify_trees(directory: Path) -> None:
    path = safe_child(directory, TREES_FILE["name"])
    if not path.is_file() or path.stat().st_size != TREES_FILE["size"] or sha256(path) != TREES_FILE["sha256"]:
        raise RuntimeError("EU-Trees4F archive is absent or differs from the pinned identity")


def verify_religion(path: Path = RELIGION_FILE) -> None:
    if path.is_symlink() or not path.is_file() or path.stat().st_size != RELIGION_SIZE or sha256(path) != RELIGION_SHA256:
        raise RuntimeError("curated religion CSV revision digest/size mismatch")
    with path.open("r", encoding="utf-8", newline="") as stream:
        reader = csv.DictReader(stream)
        if tuple(reader.fieldnames or ()) != RELIGION_HEADER:
            raise RuntimeError("curated religion CSV schema/order mismatch")
        priorities: set[int] = set()
        previous = -1
        rows = 0
        for row in reader:
            rows += 1
            priority = int(row["priority"])
            bounds = tuple(float(row[key]) for key in ("min_latitude", "max_latitude", "min_longitude", "max_longitude"))
            if priority in priorities or priority <= previous or not (-90 <= bounds[0] < bounds[1] <= 90) or not (-180 <= bounds[2] < bounds[3] <= 180) or row["status"] not in RELIGION_STATUSES or not row["region"].strip():
                raise RuntimeError("curated religion CSV ordering, bounds, or values are invalid")
            priorities.add(priority)
            previous = priority
        if rows != 14:
            raise RuntimeError("curated religion CSV must contain exactly 14 regions")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", choices=tuple(CONTRACTS) + ("religion",))
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--plan", action="store_true")
    modes.add_argument("--init", action="store_true")
    modes.add_argument("--verify-only", action="store_true")
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--force", action="store_true", help="replace only after a new candidate verifies")
    args = parser.parse_args()
    if args.source == "religion":
        if args.init:
            raise RuntimeError("IEG source images are rights-reserved and must never be downloaded or mirrored")
        if args.plan:
            print(json.dumps({"source": "ieg-religion-1544-curated", "status": "curated-committed", "path": str(RELIGION_FILE), "size": RELIGION_SIZE, "sha256": RELIGION_SHA256}, sort_keys=True))
        else:
            verify_religion(args.output_dir or RELIGION_FILE)
            print("curated IEG religion intermediate verified")
        return
    contract = CONTRACTS[args.source]
    directory = args.output_dir or Path(contract.output)
    if args.plan:
        print(json.dumps(plan(contract), indent=2, sort_keys=True))
    elif args.init:
        if contract.blocked_reason is not None:
            raise RuntimeError(f"acquisition refused: {contract.blocked_reason}; commit a checked inventory before enabling network preparation")
        download_trees(directory, force=args.force)
        print(f"{contract.source} initialized and verified")
    elif contract.source == "eu-trees4f-v2":
        verify_trees(directory)
        print(f"{contract.source} verified")
    else:
        entries = verify_inventory(directory, contract)
        print(f"{contract.source} local inventory verified; release remains blocked")


if __name__ == "__main__":
    main()
