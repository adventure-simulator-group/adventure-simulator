import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

import scripts.build_wasm as build_wasm
import scripts.just_tasks as just_tasks
import utils.generate_certificates as certificates


class JustTaskTests(unittest.TestCase):
    def test_load_world_preserves_default_database_and_accepts_isolated_database(self):
        justfile = (Path(__file__).resolve().parents[2] / "justfile").read_text(encoding="utf-8")
        self.assertIn(
            "load-world server=spacetime_url database=spacetime_module:",
            justfile,
        )
        self.assertIn(
            "load-viabundus-world server=spacetime_url database=spacetime_module:",
            justfile,
        )
        load_recipe = justfile.split("load-world server=", 1)[1].split(
            "# Compatibility name", 1
        )[0]
        self.assertIn("--database {{ database }}", load_recipe)

    def test_web_environment_uses_absolute_cross_platform_paths(self):
        environment = just_tasks.web_environment()

        self.assertEqual(environment["SPACETIMEDB_HOST"], "http://localhost:3000")
        self.assertEqual(environment["BIND_ADDRESS"], "127.0.0.1:8080")
        self.assertTrue(Path(environment["STATIC_DIR"]).is_absolute())
        self.assertTrue(Path(environment["TACTICAL_STATIC_DIR"]).is_absolute())

    @mock.patch.object(just_tasks.shutil, "which", return_value="spacetime")
    @mock.patch.object(just_tasks.subprocess, "run")
    def test_spacetime_version_requires_tool_and_library_versions(self, run, _which):
        run.return_value = mock.Mock(
            returncode=0,
            stdout=(
                "spacetimedb tool version 2.6.1; build abc\n"
                "spacetimedb-lib version 2.6.1; build def\n"
            ),
        )
        self.assertEqual(just_tasks.spacetime_version_check(), 0)

        run.return_value.stdout = "spacetimedb tool version 2.6.1; build abc\n"
        self.assertEqual(just_tasks.spacetime_version_check(), 1)

    def test_sync_tree_replaces_base_then_merges_overlays(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            base = root / "base"
            overlay = root / "overlay"
            destination = root / "destination"
            base.mkdir()
            overlay.mkdir()
            (base / "shared.txt").write_text("base", encoding="utf-8")
            (base / "base.txt").write_text("base", encoding="utf-8")
            (overlay / "shared.txt").write_text("overlay", encoding="utf-8")
            (destination).mkdir()
            (destination / "stale.txt").write_text("stale", encoding="utf-8")

            just_tasks.sync_tree(base, destination, clear=True)
            just_tasks.sync_tree(overlay, destination, clear=False)

            self.assertFalse((destination / "stale.txt").exists())
            self.assertEqual((destination / "base.txt").read_text(encoding="utf-8"), "base")
            self.assertEqual((destination / "shared.txt").read_text(encoding="utf-8"), "overlay")

    @unittest.skipUnless(os.name == "nt", "Windows process probing behavior")
    def test_windows_process_probe_does_not_use_os_kill(self):
        kernel32 = mock.Mock()
        kernel32.OpenProcess.return_value = 123

        def report_active(_handle, pointer):
            pointer._obj.value = 259
            return True

        kernel32.GetExitCodeProcess.side_effect = report_active
        with mock.patch.object(just_tasks.ctypes, "WinDLL", return_value=kernel32), \
                mock.patch.object(just_tasks.os, "kill") as kill:
            self.assertTrue(just_tasks.process_exists(42))
        kill.assert_not_called()
        kernel32.CloseHandle.assert_called_once_with(123)

    @mock.patch.object(just_tasks, "run")
    @mock.patch.object(just_tasks, "executable", return_value="spacetime")
    def test_simulation_database_is_deleted_after_failure(self, _executable, run):
        run.return_value = 7
        with mock.patch.object(just_tasks.secrets, "token_hex", side_effect=["a" * 64, "b" * 8]), \
                mock.patch.object(just_tasks.time, "time_ns", return_value=123), \
                mock.patch.object(just_tasks.os, "getpid", return_value=456), \
                mock.patch.object(just_tasks.subprocess, "run") as cleanup:
            self.assertEqual(
                just_tasks.strategic_sim(
                    "1", "2", "3", "4", "5", "http://localhost:3000", just_tasks.MODULE_DIR,
                ),
                7,
            )

        cleanup.assert_called_once()
        command = cleanup.call_args.args[0]
        self.assertEqual(command[:4], ["spacetime", "delete", "--yes", "--server"])
        self.assertTrue(command[-1].startswith("adventuresim-sim-123-456-"))


class WasmAssetTests(unittest.TestCase):
    def test_asset_sync_removes_stale_files_and_merges_crate_assets(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "assets").mkdir()
            (root / "assets" / "base.txt").write_text("base", encoding="utf-8")
            crate_assets = root / "crates" / "demo" / "assets"
            crate_assets.mkdir(parents=True)
            (crate_assets / "crate.txt").write_text("crate", encoding="utf-8")
            output = root / "static" / "assets"
            output.mkdir(parents=True)
            (output / "stale.txt").write_text("stale", encoding="utf-8")

            with mock.patch.object(build_wasm, "ROOT", root), mock.patch.object(build_wasm, "ASSET_DIR", output):
                build_wasm.sync_assets()

            self.assertFalse((output / "stale.txt").exists())
            self.assertEqual((output / "base.txt").read_text(encoding="utf-8"), "base")
            self.assertEqual((output / "crate.txt").read_text(encoding="utf-8"), "crate")


class CertificateTests(unittest.TestCase):
    def test_certificate_config_distinguishes_ip_and_dns_names(self):
        config = certificates.openssl_config(
            certificates.parse_sans("127.0.0.1, localhost, ::1")
        )

        self.assertIn("IP.1 = 127.0.0.1", config)
        self.assertIn("DNS.1 = localhost", config)
        self.assertIn("IP.2 = ::1", config)

    def test_certificate_names_reject_config_injection(self):
        for value in ("", "example.com\n[evil]", "example.com=evil"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                certificates.parse_sans(value)


class BashRemovalTests(unittest.TestCase):
    def test_automation_has_no_bash_scripts_or_recipe_dependency(self):
        root = Path(__file__).resolve().parents[2]
        listed = subprocess.run(
            ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
            cwd=root, check=True, text=True, stdout=subprocess.PIPE,
        ).stdout.splitlines()
        shell_scripts = [path for path in listed if path.endswith((".sh", ".bash")) and (root / path).is_file()]
        self.assertEqual(shell_scripts, [])
        justfile = (root / "justfile").read_text(encoding="utf-8")
        for marker in ("/usr/bin/env bash", "set shell := [\"bash\"", "@bash "):
            self.assertNotIn(marker, justfile)


if __name__ == "__main__":
    unittest.main()
