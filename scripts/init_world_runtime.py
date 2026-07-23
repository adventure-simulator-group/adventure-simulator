#!/usr/bin/env python3
"""Install the pinned compiled world and strategic-map runtime release."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import tempfile
import urllib.error
import urllib.request

import world_runtime_release as runtime


def download(url: str, destination: Path, expected_size: int) -> None:
    partial = destination.with_name(destination.name + ".part")
    existing = partial.stat().st_size if partial.exists() else 0
    if existing > expected_size:
        runtime.fail("partial runtime download exceeds its pinned size")
    headers = {"User-Agent": "adventure-simulator-runtime-init/1"}
    if existing:
        headers["Range"] = f"bytes={existing}-"
    request = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            if existing and response.status != 206:
                runtime.fail("runtime release server did not honor byte-range resume")
            if not existing and response.status != 200:
                runtime.fail(f"runtime release download returned HTTP {response.status}")
            length = response.headers.get("Content-Length")
            if length is not None and existing + int(length) != expected_size:
                runtime.fail("runtime release response has an unexpected size")
            destination.parent.mkdir(parents=True, exist_ok=True)
            with partial.open("ab") as output:
                total = existing
                while block := response.read(runtime.CHUNK):
                    total += len(block)
                    if total > expected_size:
                        runtime.fail("runtime release download exceeds its pinned size")
                    output.write(block)
        if partial.stat().st_size != expected_size:
            runtime.fail("runtime release download is incomplete")
        os.replace(partial, destination)
    except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError, OSError) as error:
        raise RuntimeError(f"runtime release download failed: {error}") from error


def write_marker(repository: Path, lock: dict[str, object]) -> None:
    marker = repository / "target" / "world-runtime-release.lock.json"
    marker.parent.mkdir(parents=True, exist_ok=True)
    handle, name = tempfile.mkstemp(prefix=f".{marker.name}.", suffix=".tmp", dir=marker.parent)
    os.close(handle)
    temporary = Path(name)
    try:
        temporary.write_bytes(runtime.canonical_json(lock) + b"\n")
        os.replace(temporary, marker)
    finally:
        temporary.unlink(missing_ok=True)


def initialize(repository: Path, lock_path: Path, replace: bool = False) -> None:
    repository = repository.resolve()
    lock = runtime.read_lock(lock_path)
    if runtime.installed_files_match(repository, lock):
        write_marker(repository, lock)
        print("Pinned world runtime release is already installed")
        return
    marker = repository / "target" / "world-runtime-release.lock.json"
    replace_downloaded_release = False
    if not replace and marker.is_file():
        try:
            previous = runtime.read_lock(marker)
            replace_downloaded_release = runtime.installed_files_match(repository, previous)
        except RuntimeError:
            replace_downloaded_release = False
    cache = repository / "target" / "world-runtime-cache" / str(lock["archive_sha256"])
    archive = cache / Path(str(lock["archive_url"])).name
    if not archive.is_file() or archive.stat().st_size != lock["archive_size"] or runtime.sha256(archive) != lock["archive_sha256"]:
        archive.unlink(missing_ok=True)
        download(str(lock["archive_url"]), archive, int(lock["archive_size"]))
    runtime.install(archive, lock, repository, replace or replace_downloaded_release)
    write_marker(repository, lock)
    print(f"Installed compiled world runtime release {lock['release']}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=Path.cwd())
    parser.add_argument("--release-lock", type=Path, default=Path("world-runtime-release.lock.json"))
    parser.add_argument("--replace", action="store_true")
    args = parser.parse_args()
    repository = args.repository.resolve()
    lock_path = args.release_lock if args.release_lock.is_absolute() else repository / args.release_lock
    initialize(repository, lock_path, args.replace)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError) as error:
        raise SystemExit(f"error: {error}")
