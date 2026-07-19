#!/usr/bin/env python3
"""Build, verify, and install a source-separated world-data input collection.

The archive is deliberately *not* a compiled world artifact.  It contains a
canonical manifest, an individually reviewable notice for every payload, and
separate payload directories.  Policy here is an engineering guardrail, not a
legal opinion: a release maintainer must review each source's terms and exact
inventory before publishing an archive.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import tempfile
import zipfile

SCHEMA = 1
CHUNK = 1024 * 1024
# A complete public bundle already contains more than 14 GiB of GLO-30 tiles.
# These bounds protect extractors without making a valid full release
# impossible. ZIP64 is deliberately enabled by the writer and accepted by the
# verifier.
MAX_ARCHIVE_BYTES = 64 * 1024 * 1024 * 1024
MAX_MEMBER_BYTES = 4 * 1024 * 1024 * 1024
MAX_TOTAL_BYTES = 64 * 1024 * 1024 * 1024
MAX_RATIO = 200
WINDOWS_DEVICES = {"CON", "PRN", "AUX", "NUL", *(f"COM{i}" for i in range(1, 10)), *(f"LPT{i}" for i in range(1, 10))}

# `destination` is relative to the repository root.  `checked-in` deliberately
# has no payload: copying it from a bundle would overwrite a tracked asset.
POLICY = {
    "viabundus-v2": ("2", "source-separated", "viabundus"),
    "hyde-3-5-c9": ("3.5 c9", "source-separated", "target/world-data-sources/raw/hyde35-land-use"),
    "copernicus-dem-glo30": ("GLO-30", "source-separated", "target/world-data-sources/raw/elevation"),
    "clms-forest-2018": ("2018", "source-separated", "target/world-data-sources/raw/forest-cover"),
    "jung-european-pnv-v1-1": ("1.1", "source-separated", "target/world-data-sources/raw/jung-pnv"),
    "eu-trees4f-v2": ("2", "source-separated", "target/world-data-sources/raw/tree-species"),
    "soilgrids-europe-prepared": ("prepared", "prepared", "target/world-data-sources/prepared/soilgrids"),
    "egdi-surface-geology-1m": ("EGDI-GE-1M-SURFACE", "source-separated", "target/world-data-sources/raw/geology"),
    "ieg-religion-1544-curated": ("adventuresim-ieg-religion-1544-v1", "checked-in", None),
    "noaa-owda-v1-derived": ("1544", "derived-only", "target/world-data-sources/prepared/owda"),
    "copernicus-eu-hydro-1-3": ("1.3", "source-separated", "target/world-data-sources/raw/hydrology"),
}

FULL_RELEASE_SOURCES = frozenset(POLICY)
REQUIRED_FILES = {
    "viabundus-v2": frozenset({"alternativenames.csv", "descriptions.csv", "edges.csv", "nodes.csv", "population.csv", ".viabundus-source.json", "settlement-ids-1544.json"}),
    "hyde-3-5-c9": frozenset({"cropland.nc", "grazing_land.nc", "urban_area.nc", "general_files.zip"}),
    "noaa-owda-v1-derived": frozenset({"settlement-profiles-1544.json"}),
}
NOTICE_TEMPLATES = {
    "viabundus-v2": "Viabundus v2 — CC BY-SA 4.0. Credit Viabundus and the University of Göttingen; retain the CC BY-SA 4.0 link and identify Adventure Simulator modifications. https://creativecommons.org/licenses/by-sa/4.0/\n",
    "hyde-3-5-c9": "HYDE 3.5 c9 — CC BY 3.0. Credit Klein Goldewijk et al./HYDE, retain https://creativecommons.org/licenses/by/3.0/, and identify interpolation/classification modifications.\n",
    "copernicus-dem-glo30": "Copernicus DEM GLO-30 — retain the prescribed Copernicus/WorldDEM production credit and European Commission/ESA no-liability notice; do not imply endorsement.\n",
    "clms-forest-2018": "Copernicus Forest 2018 — credit the European Union/Copernicus Land Monitoring Service, identify clipping/classification modifications, and do not imply endorsement.\n",
    "jung-european-pnv-v1-1": "Jung/IIASA European PNV v1.1 — CC BY 4.0. Retain attribution and https://creativecommons.org/licenses/by/4.0/; identify gameplay conversion modifications.\n",
    "eu-trees4f-v2": "EU-Trees4F v2 — CC0 1.0. Retain the dataset and publication citation despite CC0: https://doi.org/10.6084/m9.figshare.17032328.\n",
    "soilgrids-europe-prepared": "ISRIC SoilGrids prepared European subset — CC BY 4.0. Retain attribution, https://creativecommons.org/licenses/by/4.0/, retrieval provenance, and preparation modifications.\n",
    "egdi-surface-geology-1m": "EGDI Surface Geology — CC BY 4.0 plus Maltese contribution/disclaimer. Retain attribution, https://creativecommons.org/licenses/by/4.0/, and identify gameplay conversion modifications.\n",
    "ieg-religion-1544-curated": "IEG confessional maps are rights-reserved. Source images/maps are never redistributed; this checked-in coarse curated intermediate is not a facsimile or boundary dataset. Credit © IEG Mainz / Andreas Kunz.\n",
    "noaa-owda-v1-derived": "NOAA OWDA v1.0 derived-only profiles. Cite https://doi.org/10.25921/rjm6-mq74 and Cook et al. https://doi.org/10.1126/sciadv.1500561. Do not redistribute the grid or annual series.\n",
    "copernicus-eu-hydro-1-3": "Copernicus EU-Hydro v1.3 — credit the European Union/Copernicus Land Monitoring Service, identify modifications, and do not imply endorsement.\n",
}

PROHIBITED = (
    "luh", "land_use_harmonization", "ieg-map", "ieg_image", "owda.nc", "owda-grid", "owda-annual",
)
# Downloaders may leave hidden, randomly suffixed transfer fragments beside a
# completed source.  They are not source inputs and must never enter a release.
TRANSIENT_INPUT_SUFFIXES = (".part",)


def fail(message: str) -> None:
    raise RuntimeError(message)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(CHUNK), b""):
            digest.update(block)
    return digest.hexdigest()


def safe_part(part: str) -> str:
    if not part or part in {".", ".."} or "\\" in part or ":" in part or part.endswith((".", " ")):
        fail("unsafe archive path")
    if part.split(".", 1)[0].upper() in WINDOWS_DEVICES:
        fail("reserved Windows archive path")
    return part


def safe_path(name: object) -> str:
    if not isinstance(name, str) or not name or name.startswith("/") or "\\" in name:
        fail("unsafe archive path")
    path = PurePosixPath(name)
    if path.is_absolute() or not path.parts:
        fail("unsafe archive path")
    return "/".join(safe_part(part) for part in path.parts)


def safe_destination(value: object) -> str:
    path = safe_path(value)
    if path.startswith("assets/") or path.startswith(".git/"):
        fail("bundle may not install into tracked or Git paths")
    return path


def no_reparse(path: Path) -> None:
    info = path.lstat()
    if stat.S_ISLNK(info.st_mode) or bool(getattr(info, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0)):
        fail("bundle input contains a symlink, junction, or reparse point")


def regular_files(root: Path) -> list[tuple[str, Path]]:
    root = root.resolve()
    no_reparse(root)
    values = []
    for path in root.rglob("*"):
        no_reparse(path)
        if path.is_file():
            relative = safe_path(path.relative_to(root).as_posix())
            values.append((relative, path))
        elif not path.is_dir():
            fail("bundle input contains a non-regular file")
    if not values:
        fail("bundle component has no files")
    return sorted(values)


def canonical_json(value: object) -> bytes:
    return (json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def load_json_member(archive: zipfile.ZipFile, info: zipfile.ZipInfo) -> dict:
    if info.file_size > 1024 * 1024:
        fail("bundle manifest or notice is oversized")
    try:
        value = json.loads(archive.read(info).decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError, zipfile.BadZipFile) as error:
        raise RuntimeError("bundle manifest is invalid JSON") from error
    if canonical_json(value) != archive.read(info):
        fail("bundle manifest is not canonical JSON")
    if not isinstance(value, dict):
        fail("bundle manifest must be an object")
    return value


def validate_manifest(value: dict) -> list[dict]:
    if set(value) != {"schema", "profile", "components"} or value["schema"] != SCHEMA or value["profile"] not in {"full", "partial"} or not isinstance(value["components"], list):
        fail("bundle manifest has unknown fields or schema mismatch")
    components = value["components"]
    ids: set[str] = set()
    paths: set[str] = set()
    previous = ""
    for component in components:
        if set(component) != {"source", "version", "form", "destination", "notice", "notice_sha256", "files"}:
            fail("bundle component has unknown fields")
        source = component["source"]
        if not isinstance(source, str) or source not in POLICY or source <= previous or source in ids:
            fail("bundle component IDs must be known, unique, and sorted")
        previous = source
        ids.add(source)
        version, form, expected_destination = POLICY[source]
        if component["version"] != version or component["form"] != form or component["destination"] != expected_destination:
            fail("bundle component conflicts with the fail-closed policy")
        notice = safe_path(component["notice"])
        if notice != f"NOTICES/{source}.md":
            fail("bundle component notice path is noncanonical")
        if component["notice_sha256"] != sha256_bytes(NOTICE_TEMPLATES[source].encode("utf-8")):
            fail("bundle component notice is not the reviewed source-specific notice")
        files = component["files"]
        if form == "checked-in":
            if files or component["destination"] is not None:
                fail("checked-in component may not contain a payload")
            continue
        if not isinstance(files, list) or not files or not isinstance(component["destination"], str):
            fail("bundle component payload is malformed")
        safe_destination(component["destination"])
        prior = ""
        for entry in files:
            if set(entry) != {"path", "size", "sha256"}:
                fail("bundle file record has unknown fields")
            path = safe_path(entry["path"])
            if path <= prior or f"{source}/{path}" in paths or not isinstance(entry["size"], int) or not 0 < entry["size"] <= MAX_MEMBER_BYTES or not isinstance(entry["sha256"], str) or len(entry["sha256"]) != 64 or any(char not in "0123456789abcdef" for char in entry["sha256"]):
                fail("bundle file record is malformed, duplicate, or noncanonical")
            if any(token in path.casefold() for token in PROHIBITED):
                fail("bundle includes prohibited LUH1, IEG map, or raw OWDA material")
            prior = path
            paths.add(f"{source}/{path}")
        if source == "noaa-owda-v1-derived" and [entry["path"] for entry in files] != ["settlement-profiles-1544.json"]:
            fail("OWDA component must contain only the bounded settlement-profiles-1544.json input")
        required = REQUIRED_FILES.get(source)
        names = {entry["path"] for entry in files}
        if required is not None and names != required:
            fail("bundle component does not have the reviewed importer layout")
    return components


def archive_members(archive: zipfile.ZipFile) -> dict[str, zipfile.ZipInfo]:
    members: dict[str, zipfile.ZipInfo] = {}
    total = 0
    for info in archive.infolist():
        name = safe_path(info.filename)
        if name in members or info.is_dir() or info.flag_bits & 1 or info.compress_type not in {zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED, zipfile.ZIP_BZIP2, zipfile.ZIP_LZMA}:
            fail("bundle has duplicate, directory, encrypted, or unsupported ZIP members")
        mode = info.external_attr >> 16
        if stat.S_ISLNK(mode):
            fail("bundle may not contain ZIP symlinks")
        if info.file_size < 0 or info.file_size > MAX_MEMBER_BYTES or info.file_size > MAX_RATIO * max(1, info.compress_size):
            fail("bundle member exceeds uncompressed size or compression-ratio limit")
        total += info.file_size
        if total > MAX_TOTAL_BYTES:
            fail("bundle exceeds total uncompressed size limit")
        members[name] = info
    return members


def validate_owda_component(archive: zipfile.ZipFile, members: dict[str, zipfile.ZipInfo], components: list[dict]) -> None:
    component = next((item for item in components if item["source"] == "noaa-owda-v1-derived"), None)
    if component is None:
        return
    viabundus = next((item for item in components if item["source"] == "viabundus-v2"), None)
    if viabundus is None:
        fail("OWDA derived profiles require the bundled Viabundus revision")
    profile_member = members["payload/noaa-owda-v1-derived/settlement-profiles-1544.json"]
    try:
        value = json.loads(archive.read(profile_member).decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeError("OWDA derived profile is invalid JSON") from error
    if set(value) != {"schema", "source", "version", "year", "viabundus_inventory_sha256", "viabundus_settlement_ids_sha256", "profiles"} or value["schema"] != 1 or value["source"] != "noaa-owda-v1-derived" or value["version"] != "1544" or value["year"] != 1544 or not isinstance(value["profiles"], list) or not value["profiles"]:
        fail("OWDA derived profile has an unsupported schema or coverage declaration")
    inventory_member = members.get("payload/viabundus-v2/.viabundus-source.json")
    if inventory_member is None or value["viabundus_inventory_sha256"] != sha256_bytes(archive.read(inventory_member)):
        fail("OWDA derived profile does not bind to the bundled Viabundus inventory")
    settlement_member = members.get("payload/viabundus-v2/settlement-ids-1544.json")
    if settlement_member is None or value["viabundus_settlement_ids_sha256"] != sha256_bytes(archive.read(settlement_member)):
        fail("OWDA derived profile does not bind to the bundled Viabundus settlement coverage")
    try:
        settlements = json.loads(archive.read(settlement_member).decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeError("Viabundus settlement coverage inventory is invalid JSON") from error
    if set(settlements) != {"schema", "year", "settlement_ids"} or settlements["schema"] != 1 or settlements["year"] != 1544 or not isinstance(settlements["settlement_ids"], list) or not settlements["settlement_ids"] or settlements["settlement_ids"] != sorted(settlements["settlement_ids"]) or any(not isinstance(item, str) or not item for item in settlements["settlement_ids"]):
        fail("Viabundus settlement coverage inventory is noncanonical or invalid")
    previous = ""
    for profile in value["profiles"]:
        if set(profile) != {"settlement_id", "sampling", "current_milli_pdsi", "mean_milli_pdsi", "drought_summers", "wet_summers"} or not isinstance(profile["settlement_id"], str) or profile["settlement_id"] <= previous or profile["sampling"] not in {"direct", "nearest"} or not all(isinstance(profile[name], int) for name in ("current_milli_pdsi", "mean_milli_pdsi", "drought_summers", "wet_summers")) or not -32768 <= profile["current_milli_pdsi"] <= 32767 or not -32768 <= profile["mean_milli_pdsi"] <= 32767 or not 0 <= profile["drought_summers"] <= 20 or not 0 <= profile["wet_summers"] <= 20 or profile["drought_summers"] + profile["wet_summers"] > 20:
            fail("OWDA derived profile record is malformed or lacks truthful sampling provenance")
        previous = profile["settlement_id"]
    if [profile["settlement_id"] for profile in value["profiles"]] != settlements["settlement_ids"]:
        fail("OWDA derived profile does not cover exactly the bundled Viabundus settlements")


def require_digest(value: object, expected: str, label: str) -> None:
    if not isinstance(value, str) or len(value) != 64 or any(char not in "0123456789abcdef" for char in value) or value != expected:
        fail(f"{label} SHA-256 does not match the release-published expected digest")


def load_release_descriptor(path: Path, expected_descriptor_sha256: str, archive_path: Path, manifest_bytes: bytes, components: list[dict]) -> None:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeError("release descriptor is absent or invalid") from error
    descriptor_bytes = path.read_bytes()
    require_digest(expected_descriptor_sha256, sha256_bytes(descriptor_bytes), "release descriptor")
    if canonical_json(value) != descriptor_bytes or set(value) != {"schema", "profile", "archive_sha256", "manifest_sha256", "components_sha256"} or value["schema"] != SCHEMA:
        fail("release descriptor has unknown fields or is noncanonical")
    expected = {
        "archive_sha256": sha256(archive_path),
        "manifest_sha256": sha256_bytes(manifest_bytes),
        "components_sha256": sha256_bytes(canonical_json(components)),
    }
    manifest_profile = json.loads(manifest_bytes)["profile"]
    if value["profile"] not in {"full", "partial"} or value["profile"] != manifest_profile or any(value.get(name) != digest for name, digest in expected.items()):
        fail("archive does not match the externally supplied release descriptor")


def write_release_descriptor(archive_path: Path, output: Path) -> None:
    archive, components, members = inspect(archive_path)
    try:
        manifest = load_json_member(archive, members["bundle-manifest.json"])
        value = {
            "schema": SCHEMA,
            "profile": manifest["profile"],
            "archive_sha256": sha256(archive_path),
            "manifest_sha256": sha256_bytes(archive.read(members["bundle-manifest.json"])),
            "components_sha256": sha256_bytes(canonical_json(components)),
        }
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(canonical_json(value))
    finally:
        archive.close()


def write_atomic_json(path: Path, value: dict) -> None:
    temporary = path.with_name("." + path.name + ".tmp")
    temporary.write_bytes(canonical_json(value))
    os.replace(temporary, path)


def recover_transaction(repository: Path) -> None:
    journal = repository / "target" / ".world-data-bundle-transaction.json"
    if not journal.exists():
        return
    try:
        value = json.loads(journal.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeError("world-data bundle transaction journal is invalid; recover it manually") from error
    if set(value) != {"schema", "items"} or value["schema"] != 1 or not isinstance(value["items"], list):
        fail("world-data bundle transaction journal is invalid; recover it manually")
    for item in value["items"]:
        if not isinstance(item, dict) or set(item) != {"destination", "backup", "source", "backup_moved", "published"} or not isinstance(item["backup_moved"], bool) or not isinstance(item["published"], bool):
            fail("world-data bundle transaction journal is invalid; recover it manually")
    if all(item["published"] for item in value["items"]):
        journal.unlink()
        return
    quarantine = repository / "target" / "world-data-backups" / "interrupted-new"
    quarantine.mkdir(parents=True, exist_ok=True)
    for item in value["items"]:
        if not item["backup_moved"]:
            continue
        destination = repository.joinpath(*safe_destination(item["destination"]).split("/"))
        backup = Path(item["backup"]) if item["backup"] else None
        if destination.exists():
            os.replace(destination, quarantine / (item["source"] + "-" + destination.name))
        if backup is not None and backup.exists():
            destination.parent.mkdir(parents=True, exist_ok=True)
            os.replace(backup, destination)
    journal.unlink()


def inspect(archive_path: Path, descriptor: Path | None = None, descriptor_sha256: str | None = None, allow_partial: bool = True) -> tuple[zipfile.ZipFile, list[dict], dict[str, zipfile.ZipInfo]]:
    if not archive_path.is_file() or archive_path.stat().st_size > MAX_ARCHIVE_BYTES:
        fail("bundle archive is absent or oversized")
    archive = zipfile.ZipFile(archive_path)
    try:
        members = archive_members(archive)
        manifest_info = members.get("bundle-manifest.json")
        if manifest_info is None:
            fail("bundle manifest is missing")
        manifest = load_json_member(archive, manifest_info)
        components = validate_manifest(manifest)
        expected = {"bundle-manifest.json"}
        for component in components:
            notice = component["notice"]
            expected.add(notice)
            if notice not in members:
                fail("bundle component notice is missing")
            if archive.read(members[notice]) != NOTICE_TEMPLATES[component["source"]].encode("utf-8"):
                fail("bundle component notice differs from the reviewed source-specific notice")
            for entry in component["files"]:
                name = f"payload/{component['source']}/{entry['path']}"
                expected.add(name)
                info = members.get(name)
                if info is None or info.file_size != entry["size"]:
                    fail("bundle payload is missing or has a size mismatch")
                with archive.open(info) as stream:
                    digest = hashlib.sha256()
                    for block in iter(lambda: stream.read(CHUNK), b""):
                        digest.update(block)
                if digest.hexdigest() != entry["sha256"]:
                    fail("bundle payload checksum mismatch")
        if set(members) != expected:
            fail("bundle has unlisted or extra members")
        validate_owda_component(archive, members, components)
        if manifest["profile"] == "partial" and not allow_partial:
            fail("partial bundle is not accepted without --allow-partial")
        if descriptor is not None:
            if descriptor_sha256 is None:
                fail("release descriptor SHA-256 is required")
            load_release_descriptor(descriptor, descriptor_sha256, archive_path, archive.read(manifest_info), components)
        return archive, components, members
    except Exception:
        archive.close()
        raise


def parse_component(raw: str) -> tuple[str, Path]:
    try:
        source, directory = raw.split("=", 1)
    except ValueError as error:
        raise argparse.ArgumentTypeError("component must be SOURCE=DIRECTORY") from error
    if source not in POLICY or POLICY[source][1] == "checked-in":
        raise argparse.ArgumentTypeError("component source is unknown or has no payload")
    return source, Path(directory)


def build(output: Path, components: list[tuple[str, Path]], include_checked_in: bool, partial: bool = False) -> None:
    seen: set[str] = set()
    records = []
    payloads: list[tuple[str, str, Path]] = []
    for source, root in sorted(components):
        if source in seen:
            fail("bundle build received a component more than once")
        seen.add(source)
        version, form, destination = POLICY[source]
        files = []
        for relative, path in regular_files(root):
            if relative.startswith(".") and relative.endswith(TRANSIENT_INPUT_SUFFIXES):
                continue
            # Viabundus publishes several supplementary CSVs. The importer
            # deliberately consumes only its audited five-file subset; retain
            # the official sidecar but do not accidentally redistribute
            # unrelated supplementary material merely because it shares a
            # download directory.
            if source == "viabundus-v2" and relative not in REQUIRED_FILES[source]:
                continue
            if any(token in relative.casefold() for token in PROHIBITED):
                fail("bundle input includes prohibited LUH1, IEG map, or raw OWDA material")
            files.append({"path": relative, "size": path.stat().st_size, "sha256": sha256(path)})
            payloads.append((source, relative, path))
        if source == "noaa-owda-v1-derived" and [entry["path"] for entry in files] != ["settlement-profiles-1544.json"]:
            fail("OWDA component must contain only the bounded settlement-profiles-1544.json input")
        required = REQUIRED_FILES.get(source)
        names = {entry["path"] for entry in files}
        if required is not None and names != required:
            fail("bundle component does not have the reviewed importer layout")
        records.append({"source": source, "version": version, "form": form, "destination": destination, "notice": f"NOTICES/{source}.md", "notice_sha256": sha256_bytes(NOTICE_TEMPLATES[source].encode("utf-8")), "files": files})
    if include_checked_in:
        source = "ieg-religion-1544-curated"
        version, form, destination = POLICY[source]
        records.append({"source": source, "version": version, "form": form, "destination": destination, "notice": f"NOTICES/{source}.md", "notice_sha256": sha256_bytes(NOTICE_TEMPLATES[source].encode("utf-8")), "files": []})
    records.sort(key=lambda value: value["source"])
    if not partial and {record["source"] for record in records} != FULL_RELEASE_SOURCES:
        fail("default developer bundle must cover every active compiler input; pass --partial only for an explicit non-developer test/archive")
    manifest = {"schema": SCHEMA, "profile": "partial" if partial else "full", "components": records}
    validate_manifest(manifest)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(prefix=f".{output.name}.", suffix=".tmp", dir=output.parent, delete=False) as stream:
        temporary = Path(stream.name)
    try:
        with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED, allowZip64=True) as archive:
            archive.writestr("bundle-manifest.json", canonical_json(manifest))
            for record in records:
                archive.writestr(record["notice"], NOTICE_TEMPLATES[record["source"]].encode("utf-8"))
            for source, relative, path in payloads:
                archive.write(path, f"payload/{source}/{relative}")
        inspect(temporary)[0].close()
        os.replace(temporary, output)
    finally:
        temporary.unlink(missing_ok=True)


def install(archive_path: Path, descriptor: Path, descriptor_sha256: str, repository: Path, replace: bool, allow_partial: bool = False) -> list[Path]:
    archive, components, members = inspect(archive_path, descriptor, descriptor_sha256, allow_partial)
    repository = repository.resolve()
    staging_parent = repository / "target"
    staging_parent.mkdir(parents=True, exist_ok=True)
    recover_transaction(repository)
    staged = Path(tempfile.mkdtemp(prefix=".world-data-bundle-", dir=staging_parent))
    installed: list[Path] = []
    try:
        targets: list[tuple[dict, Path, Path]] = []
        for component in components:
            if component["form"] == "checked-in":
                continue
            destination = repository.joinpath(*safe_destination(component["destination"]).split("/"))
            if destination.exists() and not replace:
                fail(f"destination already exists: {destination}; pass --replace to keep a backup and replace it")
            stage = staged / component["source"]
            stage.mkdir(parents=True)
            for entry in component["files"]:
                path = stage.joinpath(*entry["path"].split("/"))
                path.parent.mkdir(parents=True, exist_ok=True)
                with archive.open(members[f"payload/{component['source']}/{entry['path']}"]) as source, path.open("xb") as output:
                    shutil.copyfileobj(source, output, CHUNK)
            targets.append((component, stage, destination))
        # Validation already hashed archive members. Re-hash staged files before any
        # publication, so an I/O failure cannot turn into a partial install.
        for component, stage, _ in targets:
            for entry in component["files"]:
                path = stage.joinpath(*entry["path"].split("/"))
                if path.stat().st_size != entry["size"] or sha256(path) != entry["sha256"]:
                    fail("staged bundle payload integrity mismatch")
        backup_root = repository / "target" / "world-data-backups"
        backup_root.mkdir(parents=True, exist_ok=True)
        published: list[tuple[Path, Path | None, Path]] = []
        journal = repository / "target" / ".world-data-bundle-transaction.json"
        try:
            for component, stage, destination in targets:
                destination.parent.mkdir(parents=True, exist_ok=True)
                backup = None
                if destination.exists():
                    backup = backup_root / (component["source"] + "-" + destination.name + ".replaced")
                    if backup.exists():
                        fail(f"recoverable backup already exists: {backup}")
                published.append((destination, backup, stage))
            items = []
            for (component, _, destination), (_, backup, _) in zip(targets, published, strict=True):
                items.append({"destination": safe_destination(str(destination.relative_to(repository)).replace("\\", "/")), "backup": str(backup) if backup is not None else "", "source": component["source"], "backup_moved": False, "published": False})
            write_atomic_json(journal, {"schema": 1, "items": items})
            for index, (destination, backup, _) in enumerate(published):
                if backup is not None:
                    os.replace(destination, backup)
                    items[index]["backup_moved"] = True
                    write_atomic_json(journal, {"schema": 1, "items": items})
            for index, (destination, _, stage) in enumerate(published):
                os.replace(stage, destination)
                installed.append(destination)
                items[index]["published"] = True
                write_atomic_json(journal, {"schema": 1, "items": items})
            journal.unlink()
        except Exception:
            # Every old destination was moved only into its local backup, and a
            # newly published staged tree can be moved back under `staged`; this
            # restores all pre-existing inputs without deleting them.
            for destination, backup, stage in reversed(published):
                if destination.exists() and not stage.exists():
                    os.replace(destination, stage)
                if backup is not None and backup.exists() and not destination.exists():
                    os.replace(backup, destination)
            journal.unlink(missing_ok=True)
            raise
        return installed
    finally:
        archive.close()
        shutil.rmtree(staged, ignore_errors=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    build_parser = commands.add_parser("build", help="create a reviewed source-separated archive")
    build_parser.add_argument("--output", required=True, type=Path)
    build_parser.add_argument("--component", action="append", default=[], type=parse_component)
    build_parser.add_argument("--include-checked-in", action="store_true", help="record the IEG curated asset without copying it")
    build_parser.add_argument("--partial", action="store_true", help="build an explicitly non-developer partial archive")
    describe_parser = commands.add_parser("describe", help="write the external release descriptor after review")
    describe_parser.add_argument("archive", type=Path)
    describe_parser.add_argument("--output", required=True, type=Path)
    verify_parser = commands.add_parser("verify", help="verify a bundle without extracting it")
    verify_parser.add_argument("archive", type=Path)
    verify_parser.add_argument("--descriptor", required=True, type=Path)
    verify_parser.add_argument("--descriptor-sha256", required=True)
    verify_parser.add_argument("--allow-partial", action="store_true")
    install_parser = commands.add_parser("install", help="stage and atomically install a verified bundle")
    install_parser.add_argument("archive", type=Path)
    install_parser.add_argument("--descriptor", required=True, type=Path)
    install_parser.add_argument("--descriptor-sha256", required=True)
    install_parser.add_argument("--repository", type=Path, default=Path.cwd())
    install_parser.add_argument("--replace", action="store_true", help="replace destinations only after retaining recoverable backups")
    install_parser.add_argument("--allow-partial", action="store_true")
    args = parser.parse_args()
    if args.command == "build":
        build(args.output, args.component, args.include_checked_in, args.partial)
    elif args.command == "describe":
        write_release_descriptor(args.archive, args.output)
    elif args.command == "verify":
        archive, _, _ = inspect(args.archive, args.descriptor, args.descriptor_sha256, args.allow_partial)
        archive.close()
    else:
        for path in install(args.archive, args.descriptor, args.descriptor_sha256, args.repository, args.replace, args.allow_partial):
            print(path)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, zipfile.BadZipFile) as error:
        raise SystemExit(f"error: {error}")
