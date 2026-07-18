import hashlib
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import init_owda


class FakeResponse:
    def __init__(self, chunks, content_length=None):
        self.chunks = iter(chunks)
        self.headers = {}
        self.read_calls = 0
        if content_length is not None:
            self.headers["Content-Length"] = str(content_length)

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        return False

    def read(self, _size):
        self.read_calls += 1
        return next(self.chunks, b"")


class InitOwdaTests(unittest.TestCase):
    def test_verify_file_checks_size_and_checksum(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "owda.nc"
            source.write_bytes(b"test OWDA payload")
            digest = hashlib.sha256(source.read_bytes()).hexdigest()
            init_owda.verify_file(source, source.stat().st_size, digest)
            with self.assertRaises(RuntimeError):
                init_owda.verify_file(source, source.stat().st_size + 1, digest)
            with self.assertRaises(RuntimeError):
                init_owda.verify_file(source, source.stat().st_size, "0" * 64)

    def test_manifest_records_pinned_release_provenance(self):
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "owda.nc"
            init_owda.write_manifest(destination)
            record = json.loads((destination.parent / init_owda.MANIFEST_NAME).read_text())
            self.assertEqual(record, init_owda.source_record())
            self.assertEqual(record["size_bytes"], 228_226_363)
            self.assertEqual(
                record["sha256"],
                "c044aa52e9e81932841b642b6977fa6f84beb9fe73c3db502b90f4295b1d65bd",
            )

    def test_download_rejects_mismatched_content_length_before_reading(self):
        response = FakeResponse([b"data"], content_length=5)
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "owda.nc"
            with mock.patch.object(init_owda.urllib.request, "urlopen", return_value=response):
                with self.assertRaisesRegex(RuntimeError, "declares 5 bytes; expected 4"):
                    init_owda.download(destination, 4, hashlib.sha256(b"data").hexdigest())
            self.assertEqual(response.read_calls, 0)
            self.assertFalse(destination.exists())
            self.assertEqual(list(destination.parent.glob("owda-*.nc")), [])

    def test_download_aborts_chunked_body_as_soon_as_it_exceeds_pinned_size(self):
        response = FakeResponse([b"123", b"45"])
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "owda.nc"
            with mock.patch.object(init_owda.urllib.request, "urlopen", return_value=response):
                with self.assertRaisesRegex(RuntimeError, "exceeded the pinned 4-byte size"):
                    init_owda.download(destination, 4, hashlib.sha256(b"1234").hexdigest())
            self.assertEqual(response.read_calls, 2)
            self.assertFalse(destination.exists())
            self.assertEqual(list(destination.parent.glob("owda-*.nc")), [])

    def test_verify_file_translates_filesystem_errors(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(RuntimeError, "Could not read OWDA cache"):
                init_owda.verify_file(Path(directory))


if __name__ == "__main__":
    unittest.main()
