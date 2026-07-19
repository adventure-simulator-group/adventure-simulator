from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest
import zipfile

import world_data_bundle as bundle


class WorldDataBundleTests(unittest.TestCase):
    def make_bundle(self, root: Path, *, source: str = "viabundus-v2") -> Path:
        payload = root / "input"
        payload.mkdir()
        for name in ("alternativenames.csv", "descriptions.csv", "edges.csv", "nodes.csv", "population.csv"):
            (payload / name).write_text("id,name\n1,Test\n", encoding="utf-8")
        (payload / ".viabundus-source.json").write_text("{}", encoding="utf-8")
        (payload / "settlement-ids-1544.json").write_bytes(bundle.canonical_json({"schema": 1, "year": 1544, "settlement_ids": ["test"]}))
        (payload / ".interrupted-download.part").write_bytes(b"not a source input")
        archive = root / "bundle.zip"
        bundle.build(archive, [(source, payload)], include_checked_in=True, partial=True)
        bundle.write_release_descriptor(archive, root / "bundle.release.json")
        return archive

    def descriptor(self, root: Path) -> Path:
        return root / "bundle.release.json"

    def descriptor_digest(self, root: Path) -> str:
        return bundle.sha256(self.descriptor(root))

    def test_build_verify_and_install_source_separated_collection(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.make_bundle(root)
            checked = bundle.inspect(archive)
            self.assertEqual([item["source"] for item in checked[1]], ["ieg-religion-1544-curated", "viabundus-v2"])
            self.assertNotIn("payload/viabundus-v2/.interrupted-download.part", checked[2])
            checked[0].close()
            repository = root / "repository"
            (repository / "target").mkdir(parents=True)
            installed = bundle.install(archive, self.descriptor(root), self.descriptor_digest(root), repository, replace=False, allow_partial=True)
            self.assertEqual(installed, [repository / "viabundus"])
            self.assertEqual((repository / "viabundus" / "nodes.csv").read_text(encoding="utf-8"), "id,name\n1,Test\n")
            self.assertFalse((repository / "assets" / "world-data").exists())

    def test_policy_rejects_raw_owda_and_luh_material(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "input"
            source.mkdir()
            (source / "owda.nc").write_bytes(b"not allowed")
            with self.assertRaisesRegex(RuntimeError, "prohibited"):
                bundle.build(root / "bundle.zip", [("noaa-owda-v1-derived", source)], False, partial=True)

    def test_importer_layout_requires_hyde_and_viabundus_inventories(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            hyde = root / "hyde"
            hyde.mkdir()
            for name in ("cropland.nc", "grazing_land.nc", "urban_area.nc"):
                (hyde / name).write_bytes(b"x")
            with self.assertRaisesRegex(RuntimeError, "layout"):
                bundle.build(root / "bundle.zip", [("hyde-3-5-c9", hyde)], False, partial=True)

    def test_unlisted_or_unsafe_members_are_rejected_before_install(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.make_bundle(root)
            with zipfile.ZipFile(archive, "a") as handle:
                handle.writestr("payload/viabundus-v2/extra.csv", b"extra")
            with self.assertRaisesRegex(RuntimeError, "unlisted or extra"):
                bundle.inspect(archive)
            unsafe = root / "unsafe.zip"
            with zipfile.ZipFile(unsafe, "w") as handle:
                handle.writestr("../bundle-manifest.json", b"{}")
            with self.assertRaisesRegex(RuntimeError, "unsafe"):
                bundle.inspect(unsafe)

    def test_integrity_failure_leaves_prior_destination_unchanged(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.make_bundle(root)
            repository = root / "repository"
            destination = repository / "viabundus"
            destination.mkdir(parents=True)
            (destination / "old.csv").write_text("old", encoding="utf-8")
            rewritten = root / "tampered.zip"
            with zipfile.ZipFile(archive) as source, zipfile.ZipFile(rewritten, "w") as output:
                for item in source.infolist():
                    content = b"tampered" if item.filename == "payload/viabundus-v2/nodes.csv" else source.read(item)
                    output.writestr(item.filename, content)
            rewritten.replace(archive)
            with self.assertRaisesRegex(RuntimeError, "size mismatch|checksum"):
                bundle.install(archive, self.descriptor(root), self.descriptor_digest(root), repository, replace=True, allow_partial=True)
            self.assertEqual((destination / "old.csv").read_text(encoding="utf-8"), "old")

    def test_replace_retains_recoverable_backup(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.make_bundle(root)
            repository = root / "repository"
            destination = repository / "viabundus"
            destination.mkdir(parents=True)
            (destination / "old.csv").write_text("old", encoding="utf-8")
            bundle.install(archive, self.descriptor(root), self.descriptor_digest(root), repository, replace=True, allow_partial=True)
            backup = repository / "target" / "world-data-backups" / "viabundus-v2-viabundus.replaced"
            self.assertEqual((backup / "old.csv").read_text(encoding="utf-8"), "old")
            self.assertTrue((destination / "nodes.csv").is_file())

    def test_external_descriptor_rejects_a_rebuilt_archive_and_wrong_notice(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.make_bundle(root)
            rewritten = root / "rebuilt.zip"
            with zipfile.ZipFile(archive) as source, zipfile.ZipFile(rewritten, "w") as output:
                for item in source.infolist():
                    content = source.read(item)
                    if item.filename == "NOTICES/viabundus-v2.md":
                        content = b"wrong notice\n"
                    output.writestr(item.filename, content)
            with self.assertRaisesRegex(RuntimeError, "notice|externally"):
                bundle.inspect(rewritten, self.descriptor(root), self.descriptor_digest(root))
            (root / "input" / "nodes.csv").write_text("id,name\n1,Changed\n", encoding="utf-8")
            rebuilt = root / "rebuilt-valid.zip"
            bundle.build(rebuilt, [("viabundus-v2", root / "input")], include_checked_in=True, partial=True)
            with self.assertRaisesRegex(RuntimeError, "externally"):
                bundle.inspect(rebuilt, self.descriptor(root), self.descriptor_digest(root))
            expected_descriptor_digest = self.descriptor_digest(root)
            descriptor = self.descriptor(root)
            descriptor.write_bytes(bundle.canonical_json({"schema": 1, "profile": "partial", "archive_sha256": bundle.sha256(rebuilt), "manifest_sha256": "0" * 64, "components_sha256": "0" * 64}))
            with self.assertRaisesRegex(RuntimeError, "release-published"):
                bundle.inspect(rebuilt, descriptor, expected_descriptor_digest)

    def test_normal_verification_rejects_partial_profile(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.make_bundle(root)
            with self.assertRaisesRegex(RuntimeError, "partial"):
                bundle.inspect(archive, self.descriptor(root), self.descriptor_digest(root), allow_partial=False)

    def test_recovery_restores_interrupted_replacement(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            destination = repository / "viabundus"
            backup = repository / "target" / "world-data-backups" / "viabundus-v2-viabundus.replaced"
            backup.mkdir(parents=True)
            (backup / "old.csv").write_text("old", encoding="utf-8")
            destination.mkdir(parents=True)
            (destination / "new.csv").write_text("new", encoding="utf-8")
            journal = repository / "target" / ".world-data-bundle-transaction.json"
            journal.write_bytes(bundle.canonical_json({"schema": 1, "items": [{"destination": "viabundus", "backup": str(backup), "source": "viabundus-v2", "backup_moved": True, "published": False}]}))
            bundle.recover_transaction(repository)
            self.assertEqual((destination / "old.csv").read_text(encoding="utf-8"), "old")
            self.assertFalse(journal.exists())

    def test_recovery_does_not_touch_an_initial_journal_before_any_move(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            destination = repository / "viabundus"
            destination.mkdir(parents=True)
            (destination / "old.csv").write_text("old", encoding="utf-8")
            journal = repository / "target" / ".world-data-bundle-transaction.json"
            journal.parent.mkdir(parents=True)
            journal.write_bytes(bundle.canonical_json({"schema": 1, "items": [{"destination": "viabundus", "backup": str(repository / "target" / "world-data-backups" / "backup"), "source": "viabundus-v2", "backup_moved": False, "published": False}]}))
            bundle.recover_transaction(repository)
            self.assertEqual((destination / "old.csv").read_text(encoding="utf-8"), "old")

    def test_owda_profile_requires_exact_viabundus_coverage_and_provenance(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.make_bundle(root)
            viabundus_input = root / "input"
            owda = root / "owda"
            owda.mkdir()
            inventory = (viabundus_input / ".viabundus-source.json").read_bytes()
            settlements = (viabundus_input / "settlement-ids-1544.json").read_bytes()
            profile = {
                "schema": 1, "source": "noaa-owda-v1-derived", "version": "1544", "year": 1544,
                "viabundus_inventory_sha256": bundle.sha256_bytes(inventory),
                "viabundus_settlement_ids_sha256": bundle.sha256_bytes(settlements),
                "profiles": [{"settlement_id": "test", "sampling": "nearest", "current_milli_pdsi": 1, "mean_milli_pdsi": 0, "drought_summers": 0, "wet_summers": 0}],
            }
            (owda / "settlement-profiles-1544.json").write_bytes(bundle.canonical_json(profile))
            archive = root / "owda-bundle.zip"
            bundle.build(archive, [("viabundus-v2", viabundus_input), ("noaa-owda-v1-derived", owda)], True, partial=True)
            checked = bundle.inspect(archive)
            checked[0].close()

    def test_documented_full_profile_builds_from_initialized_component_contracts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            viabundus_archive = self.make_bundle(root)
            viabundus = root / "input"
            components = [("viabundus-v2", viabundus)]
            for source in sorted(bundle.POLICY):
                if source in {"viabundus-v2", "ieg-religion-1544-curated", "noaa-owda-v1-derived"}:
                    continue
                directory = root / source
                directory.mkdir()
                if source == "hyde-3-5-c9":
                    for name in ("cropland.nc", "grazing_land.nc", "urban_area.nc", "general_files.zip"):
                        (directory / name).write_bytes(b"x")
                else:
                    (directory / "initialized-input.bin").write_bytes(b"x")
                components.append((source, directory))
            owda = root / "owda"
            owda.mkdir()
            profile = {"schema": 1, "source": "noaa-owda-v1-derived", "version": "1544", "year": 1544,
                "viabundus_inventory_sha256": bundle.sha256(viabundus / ".viabundus-source.json"),
                "viabundus_settlement_ids_sha256": bundle.sha256(viabundus / "settlement-ids-1544.json"),
                "profiles": [{"settlement_id": "test", "sampling": "direct", "current_milli_pdsi": 0, "mean_milli_pdsi": 0, "drought_summers": 0, "wet_summers": 0}]}
            (owda / "settlement-profiles-1544.json").write_bytes(bundle.canonical_json(profile))
            components.append(("noaa-owda-v1-derived", owda))
            full = root / "full.zip"
            bundle.build(full, components, include_checked_in=True)
            bundle.write_release_descriptor(full, root / "full.release.json")
            checked = bundle.inspect(full, root / "full.release.json", bundle.sha256(root / "full.release.json"), allow_partial=False)
            checked[0].close()


if __name__ == "__main__":
    unittest.main()
