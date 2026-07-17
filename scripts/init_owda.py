#!/usr/bin/env python3
"""Download and verify the pinned NOAA OWDA v1.0 NetCDF source."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import tempfile
import urllib.error
import urllib.request
from pathlib import Path


VERSION = "1.0"
SOURCE_URL = "https://www.ncei.noaa.gov/pub/data/paleo/drought/owda.nc"
DATASET_DOI = "10.25921/rjm6-mq74"
PAPER_DOI = "10.1126/sciadv.1500561"
EXPECTED_SIZE = 228_226_363
EXPECTED_SHA256 = "c044aa52e9e81932841b642b6977fa6f84beb9fe73c3db502b90f4295b1d65bd"
USER_AGENT = "adventure-simulator-owda-initializer/1.0"
MANIFEST_NAME = ".owda-source.json"


def digest_file(path: Path) -> tuple[int, str]:
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            size += len(chunk)
            digest.update(chunk)
    return size, digest.hexdigest()


def verify_file(
    path: Path,
    expected_size: int = EXPECTED_SIZE,
    expected_sha256: str = EXPECTED_SHA256,
) -> None:
    try:
        size, sha256 = digest_file(path)
    except OSError as error:
        raise RuntimeError(f"Could not read OWDA cache {path}: {error}") from error
    if size != expected_size or sha256 != expected_sha256:
        raise RuntimeError(
            f"OWDA verification failed for {path}: got {size} bytes and SHA-256 "
            f"{sha256}; expected {expected_size} bytes and {expected_sha256}"
        )


def source_record() -> dict[str, object]:
    return {
        "dataset_doi": DATASET_DOI,
        "paper_doi": PAPER_DOI,
        "sha256": EXPECTED_SHA256,
        "size_bytes": EXPECTED_SIZE,
        "source_url": SOURCE_URL,
        "version": VERSION,
    }


def write_manifest(destination: Path) -> None:
    manifest = destination.parent / MANIFEST_NAME
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=destination.parent, prefix=".owda-source-", delete=False
    ) as output:
        temporary = Path(output.name)
        json.dump(source_record(), output, indent=2, sort_keys=True)
        output.write("\n")
    try:
        os.replace(temporary, manifest)
    finally:
        temporary.unlink(missing_ok=True)


def stream_response(response, output, expected_size: int) -> None:
    content_length = response.headers.get("Content-Length")
    if content_length is not None:
        try:
            declared_size = int(content_length)
        except ValueError as error:
            raise RuntimeError(f"OWDA response has invalid Content-Length {content_length!r}") from error
        if declared_size != expected_size:
            raise RuntimeError(
                f"OWDA response declares {declared_size} bytes; expected {expected_size}"
            )

    size = 0
    while chunk := response.read(1024 * 1024):
        size += len(chunk)
        if size > expected_size:
            raise RuntimeError(
                f"OWDA response exceeded the pinned {expected_size}-byte size"
            )
        output.write(chunk)


def download(
    destination: Path,
    expected_size: int = EXPECTED_SIZE,
    expected_sha256: str = EXPECTED_SHA256,
) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    request = urllib.request.Request(SOURCE_URL, headers={"User-Agent": USER_AGENT})
    try:
        with tempfile.NamedTemporaryFile(
            "wb", dir=destination.parent, prefix="owda-", suffix=".nc", delete=False
        ) as output:
            temporary = Path(output.name)
            with urllib.request.urlopen(request, timeout=60) as response:
                stream_response(response, output, expected_size)
        verify_file(temporary, expected_size, expected_sha256)
        os.replace(temporary, destination)
    except (OSError, urllib.error.URLError, urllib.error.HTTPError) as error:
        raise RuntimeError(f"Could not prepare pinned OWDA v{VERSION}: {error}") from error
    finally:
        if "temporary" in locals():
            temporary.unlink(missing_ok=True)


def initialise(destination: Path, force: bool = False) -> None:
    if destination.exists():
        try:
            verify_file(destination)
        except RuntimeError:
            if not force:
                raise
        else:
            write_manifest(destination)
            print(f"Verified pinned OWDA v{VERSION} at {destination}.")
            return
    print(f"Downloading pinned OWDA v{VERSION} from {SOURCE_URL}...")
    download(destination)
    write_manifest(destination)
    print(f"Initialised and verified {destination}.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--force", action="store_true", help="replace a mismatched cached file")
    parser.add_argument(
        "--destination",
        type=Path,
        default=Path(__file__).resolve().parents[1]
        / "target/world-data-sources/raw/climate/owda.nc",
    )
    args = parser.parse_args()
    initialise(args.destination.resolve(), args.force)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RuntimeError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
