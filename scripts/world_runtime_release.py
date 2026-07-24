#!/usr/bin/env python3
"""Build, verify, publish, and install the compiled world runtime bundle."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tempfile
import zipfile

import world_data_bundle

SCHEMA = 1
YEAR = 1544
CHUNK = 1024 * 1024
MAX_ARCHIVE_BYTES = 256 * 1024 * 1024
MAX_MEMBER_BYTES = 128 * 1024 * 1024
MAX_TOTAL_BYTES = 256 * 1024 * 1024
PUBLIC_HOST = "pub-46168a4accb04d08ad0a558b0a2abfaa.r2.dev"
PUBLIC_PREFIX = "/releases/world-runtime/"
R2_PREFIX = "releases/world-runtime"
RELEASE_PATTERN = re.compile(r"[a-z0-9][a-z0-9.-]{0,79}")

# Archive paths are deliberately fixed. The base terrain files are compiler
# inputs and must not be distributed as runtime outputs.
MEMBER_DESTINATIONS = {
    "world-1544.json": "target/world-1544.json",
    "strategic-map/strategic-map-v1.json": "target/strategic-map/strategic-map-v1.json",
    "strategic-map/strategic-map-tiles-v1.pack": "target/strategic-map/strategic-map-tiles-v1.pack",
    "strategic-map/terrain-routing-v2.json": "target/strategic-map/terrain-routing-v2.json",
    "strategic-map/terrain-routing-v2.pack": "target/strategic-map/terrain-routing-v2.pack",
    "strategic-map/STRATEGIC_MAP_DATA_LICENSE.md": "target/strategic-map/STRATEGIC_MAP_DATA_LICENSE.md",
    "WORLD_RUNTIME_DATA_NOTICE.md": "target/WORLD_RUNTIME_DATA_NOTICE.md",
}
SOURCE_PATHS = {
    archive: destination
    for archive, destination in MEMBER_DESTINATIONS.items()
    if archive != "WORLD_RUNTIME_DATA_NOTICE.md"
}


def fail(message: str) -> None:
    raise RuntimeError(message)


def canonical_json(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(CHUNK), b""):
            digest.update(block)
    return digest.hexdigest()


def bytes_sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def safe_relative(value: str) -> str:
    path = PurePosixPath(value)
    if (
        not value
        or value != path.as_posix()
        or path.is_absolute()
        or ".." in path.parts
        or any(not part or part in {".", ".."} for part in path.parts)
    ):
        fail(f"unsafe runtime bundle path: {value}")
    return value


def load_json(path: Path) -> dict[str, object]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise RuntimeError(f"invalid JSON file {path}: {error}") from error
    if not isinstance(value, dict):
        fail(f"expected a JSON object in {path}")
    return value


def validate_runtime_sources(repository: Path) -> tuple[dict[str, object], dict[str, object]]:
    for relative in SOURCE_PATHS.values():
        path = repository / relative
        if not path.is_file() or path.is_symlink():
            fail(f"missing runtime artifact: {path}")
        if path.stat().st_size <= 0 or path.stat().st_size > MAX_MEMBER_BYTES:
            fail(f"runtime artifact has an unsafe size: {path}")

    world = load_json(repository / SOURCE_PATHS["world-1544.json"])
    metadata = world.get("metadata")
    if not isinstance(metadata, dict) or metadata.get("world_year") != YEAR:
        fail("compiled world does not identify the expected world year")
    sources = metadata.get("sources")
    if not isinstance(sources, list) or not sources:
        fail("compiled world has no source manifest")
    for source in sources:
        if not isinstance(source, dict):
            fail("compiled world contains an invalid source manifest entry")

    map_manifest = load_json(repository / SOURCE_PATHS["strategic-map/strategic-map-v1.json"])
    if map_manifest.get("schema") != 4 or map_manifest.get("year") != YEAR:
        fail("strategic map has an unsupported schema or world year")
    tiles = map_manifest.get("tiles")
    if not isinstance(tiles, dict) or tiles.get("format") != "avif":
        fail("strategic map does not contain the expected AVIF tile index")
    tile_pack = repository / SOURCE_PATHS["strategic-map/strategic-map-tiles-v1.pack"]
    if tiles.get("content_sha256") != sha256(tile_pack):
        fail("strategic map tile pack does not match its manifest")

    terrain = load_json(repository / SOURCE_PATHS["strategic-map/terrain-routing-v2.json"])
    if terrain.get("schema") != 4 or terrain.get("purpose") != "final":
        fail("terrain routing package is not the final schema-4 runtime pack")
    terrain_pack = repository / SOURCE_PATHS["strategic-map/terrain-routing-v2.pack"]
    if terrain.get("content_sha256") != sha256(terrain_pack):
        fail("terrain routing pack does not match its manifest")
    if map_manifest.get("terrain_package_sha256") != terrain.get("package_sha256"):
        fail("strategic map and terrain routing package are incoherent")
    return world, map_manifest


def runtime_notice(world: dict[str, object]) -> bytes:
    metadata = world["metadata"]
    assert isinstance(metadata, dict)
    sources = metadata["sources"]
    assert isinstance(sources, list)
    lines = [
        "# Compiled world runtime data notice",
        "",
        "This notice accompanies `world-1544.json`. The compiled database is an",
        "adapted runtime artifact; it is not licensed as AGPL software. Adventure",
        "Simulator's original database selection and arrangement are offered under",
        "CC BY-SA 4.0, while every underlying source retains its own terms.",
        "The complete machine-readable source manifests and modification notes are",
        "also embedded in `world-1544.json`.",
        "",
    ]
    for source in sorted(sources, key=lambda entry: str(entry.get("id", "")) if isinstance(entry, dict) else ""):
        assert isinstance(source, dict)
        lines.extend([
            f"## {source.get('name', source.get('id', 'Source'))}",
            "",
            f"- Source: {source.get('canonical_url', '')}",
            f"- Licence/terms identifier: `{source.get('license', 'unspecified')}`",
        ])
        for notice in source.get("required_notices", []):
            lines.append(f"- {notice}")
        identity = source.get("content_identity")
        if isinstance(identity, dict) and "release-blocked" in identity:
            blocked = identity["release-blocked"]
            reason = blocked.get("reason", "unspecified") if isinstance(blocked, dict) else "unspecified"
            lines.append(f"- Upstream reproducibility warning: {reason}")
        notes = source.get("notes_markdown")
        if isinstance(notes, str) and notes:
            lines.extend(["", notes])
        lines.append("")
    return ("\n".join(lines).rstrip() + "\n").encode("utf-8")


def zip_info(name: str) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    # Runtime generation is a release-maintainer operation, but Python's
    # Windows deflater is disproportionately slow on the large generated JSON.
    # The payload is only about 60 MiB and the dominant packs are already
    # compact binary/AVIF data, so store every member for predictable builds.
    info.compress_type = zipfile.ZIP_STORED
    info.external_attr = 0o100644 << 16
    return info


def build(repository: Path, output: Path, lock_output: Path, release: str) -> dict[str, object]:
    if RELEASE_PATTERN.fullmatch(release) is None:
        fail("runtime release name must be a lowercase, filename-safe identifier")
    repository = repository.resolve()
    world, _ = validate_runtime_sources(repository)
    notice = runtime_notice(world)
    payloads: list[tuple[str, bytes]] = []
    for archive_path, source_path in SOURCE_PATHS.items():
        payloads.append((archive_path, (repository / source_path).read_bytes()))
    payloads.append(("WORLD_RUNTIME_DATA_NOTICE.md", notice))
    payloads.sort(key=lambda item: item[0])
    files = [
        {
            "path": archive_path,
            "destination": MEMBER_DESTINATIONS[archive_path],
            "size": len(payload),
            "sha256": bytes_sha256(payload),
        }
        for archive_path, payload in payloads
    ]
    manifest = {"schema": SCHEMA, "year": YEAR, "files": files}
    output.parent.mkdir(parents=True, exist_ok=True)
    handle, temporary_name = tempfile.mkstemp(prefix=f".{output.name}.", suffix=".tmp", dir=output.parent)
    os.close(handle)
    temporary = Path(temporary_name)
    try:
        with zipfile.ZipFile(temporary, "w", allowZip64=True) as archive:
            archive.writestr(zip_info("runtime-manifest.json"), canonical_json(manifest) + b"\n")
            for archive_path, payload in payloads:
                archive.writestr(zip_info(archive_path), payload)
        if temporary.stat().st_size > MAX_ARCHIVE_BYTES:
            fail("runtime archive exceeds the safety limit")
        os.replace(temporary, output)
    finally:
        temporary.unlink(missing_ok=True)
    lock = {
        "schema": SCHEMA,
        "year": YEAR,
        "release": release,
        "archive_url": f"https://{PUBLIC_HOST}{PUBLIC_PREFIX}{output.name}",
        "archive_size": output.stat().st_size,
        "archive_sha256": sha256(output),
        "files": files,
    }
    lock_output.write_bytes(canonical_json(lock) + b"\n")
    inspect(output, lock)
    return lock


def validate_lock(lock: dict[str, object]) -> list[dict[str, object]]:
    expected = {"schema", "year", "release", "archive_url", "archive_size", "archive_sha256", "files"}
    if set(lock) != expected or lock.get("schema") != SCHEMA or lock.get("year") != YEAR:
        fail("runtime release lock has an unexpected schema or year")
    if not isinstance(lock.get("release"), str) or RELEASE_PATTERN.fullmatch(str(lock["release"])) is None:
        fail("runtime release lock has an unsafe release name")
    url = lock.get("archive_url")
    prefix = f"https://{PUBLIC_HOST}{PUBLIC_PREFIX}"
    if not isinstance(url, str) or not url.startswith(prefix) or any(character in url for character in "?#@"):
        fail("runtime release lock has an unsafe archive URL")
    size = lock.get("archive_size")
    if not isinstance(size, int) or size <= 0 or size > MAX_ARCHIVE_BYTES:
        fail("runtime release lock has an unsafe archive size")
    digest = lock.get("archive_sha256")
    if not isinstance(digest, str) or re.fullmatch(r"[0-9a-f]{64}", digest) is None:
        fail("runtime release lock has an unsafe archive digest")
    files = lock.get("files")
    if not isinstance(files, list) or len(files) != len(MEMBER_DESTINATIONS):
        fail("runtime release lock has an incomplete file inventory")
    actual_paths = []
    total = 0
    for entry in files:
        if not isinstance(entry, dict) or set(entry) != {"path", "destination", "size", "sha256"}:
            fail("runtime release lock has an invalid file entry")
        path = safe_relative(entry.get("path") if isinstance(entry.get("path"), str) else "")
        destination = safe_relative(entry.get("destination") if isinstance(entry.get("destination"), str) else "")
        if MEMBER_DESTINATIONS.get(path) != destination:
            fail("runtime release lock has an unexpected file destination")
        member_size = entry.get("size")
        member_digest = entry.get("sha256")
        if not isinstance(member_size, int) or member_size <= 0 or member_size > MAX_MEMBER_BYTES:
            fail("runtime release lock has an unsafe member size")
        if not isinstance(member_digest, str) or re.fullmatch(r"[0-9a-f]{64}", member_digest) is None:
            fail("runtime release lock has an unsafe member digest")
        total += member_size
        actual_paths.append(path)
    if total > MAX_TOTAL_BYTES or actual_paths != sorted(MEMBER_DESTINATIONS):
        fail("runtime release lock file inventory is unsafe or noncanonical")
    return files


def read_lock(path: Path) -> dict[str, object]:
    lock = load_json(path)
    validate_lock(lock)
    return lock


def inspect(archive_path: Path, lock: dict[str, object]) -> None:
    files = validate_lock(lock)
    if archive_path.stat().st_size != lock["archive_size"] or sha256(archive_path) != lock["archive_sha256"]:
        fail("runtime archive does not match the pinned release lock")
    with zipfile.ZipFile(archive_path) as archive:
        infos = archive.infolist()
        names = [info.filename for info in infos]
        expected_names = ["runtime-manifest.json", *[entry["path"] for entry in files]]
        if names != expected_names or len(names) != len(set(names)):
            fail("runtime archive has an unexpected or noncanonical inventory")
        manifest = json.loads(archive.read("runtime-manifest.json"))
        if manifest != {"schema": SCHEMA, "year": YEAR, "files": files}:
            fail("runtime archive manifest does not match the pinned lock")
        for info, entry in zip(infos[1:], files, strict=True):
            if info.is_dir() or info.file_size != entry["size"] or info.file_size > MAX_MEMBER_BYTES:
                fail("runtime archive member has an unsafe size or type")
            payload = archive.read(info)
            if bytes_sha256(payload) != entry["sha256"]:
                fail("runtime archive member failed integrity verification")


def installed_files_match(repository: Path, lock: dict[str, object]) -> bool:
    for entry in validate_lock(lock):
        path = repository / entry["destination"]
        if not installed_file_matches(path, entry):
            return False
    return True


def installed_file_matches(path: Path, entry: dict[str, object]) -> bool:
    return (
        path.is_file()
        and path.stat().st_size == entry["size"]
        and sha256(path) == entry["sha256"]
    )


def install(archive_path: Path, lock: dict[str, object], repository: Path, replace: bool = False) -> None:
    inspect(archive_path, lock)
    repository = repository.resolve()
    files = validate_lock(lock)
    if installed_files_match(repository, lock):
        return
    conflicts = [
        repository / entry["destination"]
        for entry in files
        if (repository / entry["destination"]).exists()
        and not installed_file_matches(repository / entry["destination"], entry)
    ]
    if conflicts and not replace:
        fail("runtime artifacts already exist but differ from the pinned release; use --replace after preserving any local build")
    (repository / "target").mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=".world-runtime-staging-", dir=repository / "target"))
    backup = repository / "target" / "world-runtime-backups" / str(lock["archive_sha256"])[0:16]
    published: list[tuple[Path, Path | None]] = []
    try:
        with zipfile.ZipFile(archive_path) as archive:
            for entry in files:
                staged = staging / entry["destination"]
                staged.parent.mkdir(parents=True, exist_ok=True)
                staged.write_bytes(archive.read(entry["path"]))
        for entry in files:
            destination = repository / entry["destination"]
            staged = staging / entry["destination"]
            if installed_file_matches(destination, entry):
                staged.unlink()
                continue
            destination.parent.mkdir(parents=True, exist_ok=True)
            prior = None
            if destination.exists():
                prior = backup / entry["destination"]
                prior.parent.mkdir(parents=True, exist_ok=True)
                if prior.exists():
                    fail(f"runtime backup already exists: {prior}")
                os.replace(destination, prior)
            published.append((destination, prior))
            os.replace(staged, destination)
    except Exception:
        for destination, prior in reversed(published):
            destination.unlink(missing_ok=True)
            if prior is not None and prior.exists():
                destination.parent.mkdir(parents=True, exist_ok=True)
                os.replace(prior, destination)
        raise
    finally:
        shutil.rmtree(staging, ignore_errors=True)


def publish(archive_path: Path, lock: dict[str, object], env_file: Path) -> str:
    inspect(archive_path, lock)
    if shutil.which("aws") is None:
        fail("AWS CLI v2 is required for multipart R2 upload")
    environment, endpoint = world_data_bundle.r2_environment(env_file)
    key = f"{R2_PREFIX}/{archive_path.name}"
    expected_url = f"https://{PUBLIC_HOST}/{key}"
    if lock["archive_url"] != expected_url:
        fail("runtime lock URL does not match the fixed R2 release key")
    existing = subprocess.run([
        "aws", "s3api", "head-object", "--bucket", world_data_bundle.R2_BUCKET,
        "--key", key, "--endpoint-url", endpoint, "--output", "json",
    ], check=False, shell=False, env=environment, capture_output=True, text=True)
    if existing.returncode == 0:
        fail(f"refusing to overwrite immutable runtime release object: {key}")
    subprocess.run([
        "aws", "s3", "cp", str(archive_path), f"s3://{world_data_bundle.R2_BUCKET}/{key}",
        "--endpoint-url", endpoint,
    ], check=True, shell=False, env=environment)
    result = subprocess.run([
        "aws", "s3api", "head-object", "--bucket", world_data_bundle.R2_BUCKET,
        "--key", key, "--endpoint-url", endpoint, "--output", "json",
    ], check=True, shell=False, env=environment, capture_output=True, text=True)
    if json.loads(result.stdout).get("ContentLength") != archive_path.stat().st_size:
        fail("uploaded runtime archive size does not match")
    return f"s3://{world_data_bundle.R2_BUCKET}/{key}"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    build_parser = commands.add_parser("build")
    build_parser.add_argument("--repository", type=Path, default=Path.cwd())
    build_parser.add_argument("--output", type=Path, required=True)
    build_parser.add_argument("--lock-output", type=Path, required=True)
    build_parser.add_argument("--release", required=True)
    verify_parser = commands.add_parser("verify")
    verify_parser.add_argument("archive", type=Path)
    verify_parser.add_argument("--lock", type=Path, required=True)
    install_parser = commands.add_parser("install")
    install_parser.add_argument("archive", type=Path)
    install_parser.add_argument("--lock", type=Path, required=True)
    install_parser.add_argument("--repository", type=Path, default=Path.cwd())
    install_parser.add_argument("--replace", action="store_true")
    publish_parser = commands.add_parser("publish")
    publish_parser.add_argument("archive", type=Path)
    publish_parser.add_argument("--lock", type=Path, required=True)
    publish_parser.add_argument("--env-file", type=Path, default=Path(".env"))
    args = parser.parse_args()
    if args.command == "build":
        build(args.repository, args.output, args.lock_output, args.release)
    elif args.command == "verify":
        inspect(args.archive, read_lock(args.lock))
    elif args.command == "install":
        install(args.archive, read_lock(args.lock), args.repository, args.replace)
    else:
        print(publish(args.archive, read_lock(args.lock), args.env_file))


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, zipfile.BadZipFile, subprocess.CalledProcessError) as error:
        raise SystemExit(f"error: {error}")
