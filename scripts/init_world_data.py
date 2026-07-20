#!/usr/bin/env python3
"""Install the pinned public world-data bundle, or explicitly rebuild locally."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import urllib.error
import urllib.request
from urllib.parse import urlparse

import world_data_bundle as bundle

CHUNK = 1024 * 1024
MAX_ARCHIVE_BYTES = 64 * 1024 * 1024 * 1024
MAX_DESCRIPTOR_BYTES = 2 * 1024 * 1024
REQUIRED_FREE_OVERHEAD = 1024 * 1024 * 1024
PUBLIC_HOST = "pub-46168a4accb04d08ad0a558b0a2abfaa.r2.dev"
PUBLIC_PREFIX = "/releases/world-data/"


def fail(message: str) -> None:
    raise RuntimeError(message)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(CHUNK), b""):
            digest.update(block)
    return digest.hexdigest()


def canonical_json(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")


def load_release_lock(path: Path) -> dict[str, object]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise RuntimeError(f"invalid world-data release lock: {error}") from error
    expected = {"schema", "profile", "archive_url", "archive_size", "descriptor_url", "descriptor_sha256"}
    if set(value) != expected or value.get("schema") != 1 or value.get("profile") != "full":
        fail("world-data release lock has an unexpected schema or profile")
    if not isinstance(value["archive_size"], int) or not 0 < value["archive_size"] <= MAX_ARCHIVE_BYTES:
        fail("world-data release lock has an unsafe archive size")
    digest = value["descriptor_sha256"]
    if not isinstance(digest, str) or len(digest) != 64 or any(character not in "0123456789abcdef" for character in digest):
        fail("world-data release lock has an unsafe descriptor digest")
    for field in ("archive_url", "descriptor_url"):
        url = value[field]
        parsed = urlparse(url) if isinstance(url, str) else None
        if parsed is None or parsed.scheme != "https" or parsed.hostname != PUBLIC_HOST or not parsed.path.startswith(PUBLIC_PREFIX) or parsed.query or parsed.fragment or parsed.username or parsed.password:
            fail(f"world-data release lock has an unsafe {field}")
    return value


def download(url: str, destination: Path, maximum: int) -> None:
    """Download atomically, retaining a validated byte-range partial on interruption."""
    partial = destination.with_name(destination.name + ".part")
    existing = partial.stat().st_size if partial.exists() else 0
    if existing > maximum:
        fail(f"partial download exceeds byte cap: {partial}")
    headers = {"User-Agent": "adventure-simulator-world-init/1"}
    if existing:
        headers["Range"] = f"bytes={existing}-"
    request = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            if existing and response.status != 206:
                fail("release server did not honor the required byte-range resume")
            if not existing and response.status != 200:
                fail(f"release download returned HTTP {response.status}")
            length = response.headers.get("Content-Length")
            if length is not None and existing + int(length) > maximum:
                fail("release download exceeds byte cap")
            destination.parent.mkdir(parents=True, exist_ok=True)
            with partial.open("ab") as output:
                total = existing
                while block := response.read(CHUNK):
                    total += len(block)
                    if total > maximum:
                        fail("release download exceeds byte cap")
                    output.write(block)
    except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError, OSError) as error:
        raise RuntimeError(f"release download failed: {error}") from error
    os.replace(partial, destination)


def write_marker(path: Path, release: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    handle, name = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".tmp", dir=path.parent)
    os.close(handle)
    temporary = Path(name)
    try:
        temporary.write_bytes(canonical_json(release) + b"\n")
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def installed_marker_matches(path: Path, release: dict[str, object]) -> bool:
    try:
        return json.loads(path.read_text(encoding="utf-8")) == release
    except (OSError, json.JSONDecodeError):
        return False


def install_release(repository: Path, release: dict[str, object], replace: bool) -> None:
    cache = repository / "target" / "world-data-bundle-cache" / release["descriptor_sha256"]
    archive = cache / Path(str(release["archive_url"])).name
    descriptor = cache / Path(str(release["descriptor_url"])).name
    required = int(release["archive_size"]) * 2 + REQUIRED_FREE_OVERHEAD
    if shutil.disk_usage(repository).free < required:
        fail(f"insufficient free space: initialization requires at least {required // (1024 * 1024 * 1024)} GiB")
    if not descriptor.is_file() or sha256(descriptor) != release["descriptor_sha256"]:
        download(str(release["descriptor_url"]), descriptor, MAX_DESCRIPTOR_BYTES)
        if sha256(descriptor) != release["descriptor_sha256"]:
            descriptor.unlink(missing_ok=True)
            fail("downloaded release descriptor does not match the pinned digest")
    if not archive.is_file() or archive.stat().st_size != release["archive_size"]:
        download(str(release["archive_url"]), archive, int(release["archive_size"]))
    if archive.stat().st_size != release["archive_size"]:
        fail("downloaded archive size does not match the pinned release lock")
    installed = bundle.install(archive, descriptor, str(release["descriptor_sha256"]), repository, replace=replace)
    if not installed:
        fail("release bundle did not install any components")
    write_marker(repository / "target" / "world-data-release.lock.json", release)
    for path in installed:
        print(path)


def rebuild(repository: Path) -> None:
    subprocess.run(["cargo", "run", "--package", "adventuresim-world-import", "--"], cwd=repository, check=True, shell=False)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=Path.cwd())
    parser.add_argument("--release-lock", type=Path, default=Path("world-data-release.lock.json"))
    parser.add_argument("--replace", action="store_true", help="replace prior installed components only after retaining backups")
    parser.add_argument("--rebuild", action="store_true", help="skip the release download and compile a world from installed local inputs")
    args = parser.parse_args()
    repository = args.repository.resolve()
    if args.rebuild:
        rebuild(repository)
        return
    lock = args.release_lock if args.release_lock.is_absolute() else repository / args.release_lock
    release = load_release_lock(lock)
    marker = repository / "target" / "world-data-release.lock.json"
    if installed_marker_matches(marker, release):
        print("Pinned world-data release is already installed")
        return
    install_release(repository, release, args.replace)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, subprocess.CalledProcessError, urllib.error.HTTPError) as error:
        raise SystemExit(f"error: {error}")
