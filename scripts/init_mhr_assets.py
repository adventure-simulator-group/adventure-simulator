#!/usr/bin/env python3
"""Download, verify, and install the pinned Meta MHR authoring assets."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import sys
import tempfile
import urllib.error
import urllib.request
import uuid
import zipfile
from pathlib import Path, PurePosixPath


VERSION = "1.0.1"
SOURCE_URL = "https://github.com/facebookresearch/MHR/releases/download/v1.0.1/assets.zip"
EXPECTED_SIZE = 198_943_157
EXPECTED_SHA256 = "e4f4f205cd87c0fa106577ba1de4fc763e4eb197c924461d2ef7e6944e9d6b94"
USER_AGENT = "adventure-simulator-mhr-initializer/1.0"
MANIFEST_NAME = ".mhr-source.json"
CORE_FILES = (
    "compact_v6_1.model",
    *(f"lod{lod}.fbx" for lod in range(7)),
)
CORRECTIVE_ACTIVATION = "corrective_activation.npz"
CORRECTIVE_FILES = tuple(f"corrective_blendshapes_lod{lod}.npz" for lod in range(7))


def selected_files(lod1_correctives: bool = False, all_correctives: bool = False) -> tuple[str, ...]:
    files = list(CORE_FILES)
    if lod1_correctives or all_correctives:
        files.append(CORRECTIVE_ACTIVATION)
        files.extend(CORRECTIVE_FILES if all_correctives else (CORRECTIVE_FILES[1],))
    return tuple(files)


def digest_file(path: Path) -> tuple[int, str]:
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            size += len(chunk)
            digest.update(chunk)
    return size, digest.hexdigest()


def verify_archive(
    path: Path,
    expected_size: int = EXPECTED_SIZE,
    expected_sha256: str = EXPECTED_SHA256,
) -> None:
    try:
        size, sha256 = digest_file(path)
    except OSError as error:
        raise RuntimeError(f"Could not read MHR archive {path}: {error}") from error
    if size != expected_size or sha256 != expected_sha256:
        raise RuntimeError(
            f"MHR archive verification failed for {path}: got {size} bytes and SHA-256 "
            f"{sha256}; expected {expected_size} bytes and {expected_sha256}"
        )


def source_record() -> dict[str, object]:
    return {
        "license": "Apache-2.0",
        "sha256": EXPECTED_SHA256,
        "size_bytes": EXPECTED_SIZE,
        "source_url": SOURCE_URL,
        "version": VERSION,
    }


def stream_response(response, output, expected_size: int) -> None:
    content_length = response.headers.get("Content-Length")
    if content_length is not None:
        try:
            declared_size = int(content_length)
        except ValueError as error:
            raise RuntimeError(
                f"MHR response has invalid Content-Length {content_length!r}"
            ) from error
        if declared_size != expected_size:
            raise RuntimeError(
                f"MHR response declares {declared_size} bytes; expected {expected_size}"
            )

    size = 0
    while chunk := response.read(1024 * 1024):
        size += len(chunk)
        if size > expected_size:
            raise RuntimeError(f"MHR response exceeded the pinned {expected_size}-byte size")
        output.write(chunk)


def download(
    destination: Path,
    expected_size: int = EXPECTED_SIZE,
    expected_sha256: str = EXPECTED_SHA256,
) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    request = urllib.request.Request(SOURCE_URL, headers={"User-Agent": USER_AGENT})
    temporary: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            "wb", dir=destination.parent, prefix="mhr-", suffix=".zip", delete=False
        ) as output:
            temporary = Path(output.name)
            with urllib.request.urlopen(request, timeout=120) as response:
                stream_response(response, output, expected_size)
        verify_archive(temporary, expected_size, expected_sha256)
        os.replace(temporary, destination)
        temporary = None
    except (OSError, urllib.error.URLError, urllib.error.HTTPError) as error:
        raise RuntimeError(f"Could not download pinned MHR v{VERSION}: {error}") from error
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def archive_members(
    archive: zipfile.ZipFile, required_files: tuple[str, ...] = CORE_FILES
) -> dict[str, zipfile.ZipInfo]:
    members: dict[str, zipfile.ZipInfo] = {}
    for info in archive.infolist():
        path = PurePosixPath(info.filename)
        if path.is_absolute() or ".." in path.parts:
            raise RuntimeError(f"MHR archive contains unsafe path {info.filename!r}")
        if info.is_dir():
            continue
        basename = path.name
        if basename in required_files:
            if basename in members:
                raise RuntimeError(f"MHR archive contains duplicate {basename}")
            members[basename] = info
    missing = sorted(set(required_files) - members.keys())
    if missing:
        raise RuntimeError(f"MHR archive is missing required file {missing[0]}")
    return members


def write_manifest(destination: Path, installed_names: tuple[str, ...]) -> None:
    record = source_record()
    record["selected_files"] = list(installed_names)
    record["installed_files"] = {
        name: {"sha256": digest_file(destination / name)[1], "size_bytes": (destination / name).stat().st_size}
        for name in installed_names
    }
    manifest = destination / MANIFEST_NAME
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=destination, prefix=".mhr-source-", delete=False
    ) as output:
        temporary = Path(output.name)
        json.dump(record, output, indent=2, sort_keys=True)
        output.write("\n")
    try:
        os.replace(temporary, manifest)
    finally:
        temporary.unlink(missing_ok=True)


def installed(destination: Path, required_files: tuple[str, ...] = CORE_FILES) -> bool:
    try:
        record = json.loads((destination / MANIFEST_NAME).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return False
    if {key: record.get(key) for key in source_record()} != source_record():
        return False
    installed_files = record.get("installed_files")
    if not isinstance(installed_files, dict):
        return False
    selected_names = record.get("selected_files")
    if not isinstance(selected_names, list) or not all(isinstance(name, str) for name in selected_names):
        return False
    if not set(required_files).issubset(selected_names):
        return False
    for name in selected_names:
        expected = installed_files.get(name)
        path = destination / name
        if not isinstance(expected, dict) or not path.is_file():
            return False
        size, sha256 = digest_file(path)
        if size != expected.get("size_bytes") or sha256 != expected.get("sha256"):
            return False
    return True


def extract_archive(
    archive_path: Path, destination: Path, required_files: tuple[str, ...] = CORE_FILES
) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    # tempfile.mkdtemp deliberately creates an owner-only directory on Windows.
    # A renamed cache would then be unreadable to the ordinary sandboxed build.
    # Use an unpredictable ordinary directory so it inherits the workspace ACL.
    temporary = destination.parent / f".mhr-assets-{uuid.uuid4().hex}"
    temporary.mkdir()
    try:
        with zipfile.ZipFile(archive_path) as archive:
            members = archive_members(archive, required_files)
            for name, info in members.items():
                with archive.open(info) as source, (temporary / name).open("wb") as output:
                    shutil.copyfileobj(source, output, length=1024 * 1024)
        write_manifest(temporary, required_files)
        if destination.exists():
            shutil.rmtree(destination)
        os.replace(temporary, destination)
    except (OSError, zipfile.BadZipFile) as error:
        raise RuntimeError(f"Could not install MHR v{VERSION}: {error}") from error
    finally:
        if temporary.exists():
            shutil.rmtree(temporary)


def initialise(
    root: Path, required_files: tuple[str, ...] = CORE_FILES, force: bool = False
) -> Path:
    archive = root / "assets.zip"
    destination = root / "assets"
    if installed(destination, required_files) and not force:
        print(f"Verified pinned MHR v{VERSION} assets at {destination}.")
        return destination
    if destination.exists() and not installed(destination, CORE_FILES) and not force:
        raise RuntimeError(
            f"MHR asset destination {destination} is incomplete or modified; "
            "inspect it or rerun with --force to replace only that cache directory"
        )
    if archive.is_file():
        try:
            verify_archive(archive)
        except RuntimeError:
            if not force:
                raise
            archive.unlink()
    if not archive.is_file():
        print(f"Downloading pinned MHR v{VERSION} from {SOURCE_URL}...")
        download(archive)
    extract_archive(archive, destination, required_files)
    print(f"Initialised and verified MHR v{VERSION} assets at {destination}.")
    return destination


def main() -> int:
    repository = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--force", action="store_true", help="replace a mismatched cache")
    parser.add_argument(
        "--verify-only",
        action="store_true",
        help="verify the installed cache without downloading or replacing it",
    )
    corrective_mode = parser.add_mutually_exclusive_group()
    corrective_mode.add_argument(
        "--lod1-correctives",
        action="store_true",
        help="also install the LOD 1 corrective basis used by the default creator view",
    )
    corrective_mode.add_argument(
        "--all-correctives",
        action="store_true",
        help="also install every multi-gigabyte corrective basis",
    )
    parser.add_argument(
        "--destination",
        type=Path,
        default=repository / f"target/mhr-assets/v{VERSION}",
        help="cache root; extracted files are installed under its assets directory",
    )
    args = parser.parse_args()
    root = args.destination.resolve()
    required_files = selected_files(args.lod1_correctives, args.all_correctives)
    if args.verify_only:
        destination = root / "assets"
        if not installed(destination, required_files):
            raise RuntimeError(f"MHR asset cache is missing or invalid at {destination}")
        print(f"Verified pinned MHR v{VERSION} assets at {destination}.")
    else:
        initialise(root, required_files, args.force)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RuntimeError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
