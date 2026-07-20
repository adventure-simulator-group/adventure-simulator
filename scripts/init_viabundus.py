#!/usr/bin/env python3
"""Download and initialise the local Viabundus v2 CSV source directory."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
import tempfile
import urllib.error
import urllib.request
from pathlib import Path


RECORD_URL = "https://zenodo.org/api/records/16611998"
VERSION = "2"
REQUIRED_FILES = {
    "nodes.csv",
    "edges.csv",
    "population.csv",
    "alternativenames.csv",
    "descriptions.csv",
}
USER_AGENT = "adventure-simulator-viabundus-initializer/1.0"


def request(url: str):
    return urllib.request.urlopen(
        urllib.request.Request(url, headers={"User-Agent": USER_AGENT}), timeout=60
    )


def record_files() -> list[dict[str, object]]:
    try:
        with request(RECORD_URL) as response:
            record = json.load(response)
    except (urllib.error.URLError, urllib.error.HTTPError, json.JSONDecodeError) as error:
        raise RuntimeError(f"Could not retrieve the Viabundus Zenodo record: {error}") from error

    files = [
        file
        for file in record.get("files", [])
        if file.get("key", "").lower().endswith(".csv")
    ]
    names = {file.get("key") for file in files}
    if not REQUIRED_FILES.issubset(names):
        missing = ", ".join(sorted(REQUIRED_FILES - names))
        raise RuntimeError(f"The Zenodo record is missing required CSV files: {missing}")
    return files


def download_file(file: dict[str, object], destination: Path) -> dict[str, object]:
    name = file["key"]
    links = file.get("links", {})
    url = links.get("content") or links.get("self")
    if not isinstance(name, str) or not isinstance(url, str):
        raise RuntimeError("A Viabundus CSV entry has invalid download metadata.")

    expected_size = file.get("size")
    checksum = file.get("checksum")
    if not isinstance(expected_size, int) or expected_size < 0:
        raise RuntimeError(f"{name} has invalid upstream size metadata.")
    if not isinstance(checksum, str) or ":" not in checksum:
        raise RuntimeError(f"{name} has invalid upstream checksum metadata.")
    algorithm, expected_digest = checksum.split(":", 1)
    if algorithm not in hashlib.algorithms_available or not expected_digest:
        raise RuntimeError(f"{name} uses unsupported checksum metadata.")
    upstream_digest = hashlib.new(algorithm)
    sha256 = hashlib.sha256()
    written = 0
    try:
        with request(url) as response, destination.open("wb") as output:
            while chunk := response.read(1024 * 1024):
                output.write(chunk)
                written += len(chunk)
                upstream_digest.update(chunk)
                sha256.update(chunk)
    except (urllib.error.URLError, urllib.error.HTTPError) as error:
        raise RuntimeError(f"Could not download {name}: {error}") from error

    if written != expected_size:
        destination.unlink(missing_ok=True)
        raise RuntimeError(f"{name} size mismatch: expected {expected_size}, received {written}.")
    if upstream_digest.hexdigest().lower() != expected_digest.lower():
        destination.unlink(missing_ok=True)
        raise RuntimeError(f"{name} checksum mismatch.")
    return {"name": name, "sha256": sha256.hexdigest(), "url": url, "size": written}


def is_initialised(destination: Path) -> bool:
    return destination.is_dir() and all((destination / name).is_file() for name in REQUIRED_FILES)


def validate_destination(destination: Path) -> None:
    """Reject broad/destructive force targets before any network work."""
    repository = Path(__file__).resolve().parents[1]
    home = Path.home().resolve()
    anchors = {Path(destination.anchor).resolve(), repository, home}
    if destination in anchors or destination.name.lower() != "viabundus":
        raise RuntimeError(
            "Destination must be a dedicated directory named 'viabundus', not a filesystem, home, or repository root."
        )
def publish_replacement(replacement: Path, destination: Path) -> None:
    backup = destination.with_name(f".{destination.name}.previous")
    if backup.exists():
        raise RuntimeError(f"Refusing to overwrite recovery directory: {backup}")
    if destination.exists():
        destination.replace(backup)
    try:
        replacement.replace(destination)
    except Exception:
        if backup.exists() and not destination.exists():
            backup.replace(destination)
        raise
    if backup.exists():
        shutil.rmtree(backup)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--force",
        action="store_true",
        help="replace an existing initialised viabundus directory",
    )
    parser.add_argument(
        "--destination",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "viabundus",
        help="directory to initialise (default: repository viabundus/)",
    )
    args = parser.parse_args()
    destination = args.destination.resolve()
    validate_destination(destination)

    if is_initialised(destination) and not args.force:
        print(f"Viabundus v{VERSION} is already initialised at {destination}.")
        print("Use --force to replace it.")
        return 0
    if destination.exists() and not args.force:
        raise RuntimeError(f"Destination exists but is incomplete: {destination}. Use --force to replace it.")

    print(f"Downloading Viabundus v{VERSION} from {RECORD_URL}...")
    files = record_files()

    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".viabundus-", dir=destination.parent) as temporary:
        temporary_path = Path(temporary)
        replacement = temporary_path / "viabundus"
        replacement.mkdir()
        downloaded = []
        for file in files:
            name = file["key"]
            if not isinstance(name, str) or Path(name).name != name:
                raise RuntimeError(f"Refusing unsafe Viabundus filename: {name}")
            print(f"  {name}")
            downloaded.append(download_file(file, replacement / name))
        (replacement / ".viabundus-source.json").write_text(
            json.dumps(
                {
                    "files": downloaded,
                    "record_url": RECORD_URL,
                    "version": VERSION,
                },
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )

        publish_replacement(replacement, destination)

    print(f"Initialised {destination} from {len(downloaded)} CSV files.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RuntimeError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
