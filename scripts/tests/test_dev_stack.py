import io
import json
import os
from pathlib import Path
import socket
import tempfile
import unittest
from contextlib import redirect_stderr
from unittest import mock

import scripts.dev_stack as dev_stack


class ProfileTests(unittest.TestCase):
    def test_profile_has_worktree_isolated_identifiers(self):
        with tempfile.TemporaryDirectory() as state:
            one = dev_stack.profile_values("renderer-demo", 23100, root=Path("/repo/one"), state_root=Path(state))
            two = dev_stack.profile_values("renderer-demo", 23100, root=Path("/repo/two"), state_root=Path(state))
        self.assertNotEqual(one["database"], two["database"])
        self.assertNotEqual(one["profile_dir"], two["profile_dir"])
        self.assertEqual(one["web_port"], 23101)
        self.assertEqual(one["tactical_port"], 23102)

    def test_rejects_injection_and_bad_ports(self):
        for name in ("../main", "demo;rm", "UPPER", "", "demo'quote"):
            with self.assertRaises(ValueError):
                dev_stack.profile_values(name, 23100)
        with self.assertRaises(ValueError):
            dev_stack.profile_values("demo", 65531)

    def test_destructive_server_must_be_exact_loopback(self):
        dev_stack.validate_loopback_server("http://127.0.0.1:23100", 23100)
        for server in ("https://example.com:23100", "http://0.0.0.0:23100", "http://localhost:3000", "http://localhost:23100/evil"):
            with self.assertRaises(ValueError):
                dev_stack.validate_loopback_server(server, 23100)

    def test_occupied_port_detected(self):
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            port = listener.getsockname()[1]
            self.assertEqual(dev_stack.ports_in_use([port]), [port])

    def test_strategic_only_profile_does_not_reserve_tactical_port(self):
        values = dev_stack.profile_values("demo", 23100)
        self.assertEqual(dev_stack.profile_ports(values, with_tactical=False), [23100, 23101])
        self.assertEqual(
            dev_stack.profile_ports(values, with_tactical=True),
            [23100, 23101, 23102],
        )

    def test_symlink_profile_path_rejected(self):
        with tempfile.TemporaryDirectory() as state:
            root = Path(state)
            original = Path.is_symlink

            def reports_injected_link(path):
                return path.name == "fingerprint" or original(path)

            with mock.patch.object(Path, "is_symlink", reports_injected_link):
                with self.assertRaises(ValueError):
                    dev_stack.ensure_secure_directory(root / "fingerprint" / "profile", root)

    def test_path_escape_rejected(self):
        with tempfile.TemporaryDirectory() as state:
            with self.assertRaises(ValueError):
                dev_stack.ensure_secure_directory(Path(state).parent / "outside", Path(state))

    def test_profile_lock_contention(self):
        with tempfile.TemporaryDirectory() as state:
            path = Path(state, "lock")
            with dev_stack.ProfileLock(path):
                with self.assertRaises(ValueError):
                    with dev_stack.ProfileLock(path):
                        pass


class WorkflowTests(unittest.TestCase):
    @mock.patch.object(dev_stack, "run_checked")
    def test_authenticated_cli_token_is_forwarded_without_logging(self, run_checked):
        token = "header.payload.signature"
        run_checked.return_value = mock.Mock(
            returncode=0,
            stdout=f"You are logged in as identity\nAuth token: {token}\n",
        )

        self.assertEqual(dev_stack.spacetime_auth_token(), token)
        self.assertEqual(
            run_checked.call_args.args[0],
            ["spacetime", "login", "show", "--token"],
        )

    @mock.patch.object(dev_stack, "run_checked")
    def test_missing_authenticated_cli_token_is_redacted(self, run_checked):
        run_checked.return_value = mock.Mock(returncode=0, stdout="unexpected secret output")

        with self.assertRaisesRegex(RuntimeError, "exactly one") as error:
            dev_stack.spacetime_auth_token()
        self.assertNotIn("unexpected secret output", str(error.exception))

    @mock.patch.object(dev_stack, "run_checked")
    def test_seed_propagates_reducer_failure(self, run_checked):
        run_checked.return_value = mock.Mock(returncode=7, stdout="reducer rejected\n")
        self.assertEqual(dev_stack.seed("http://localhost:1", "db", "a" * 64), 7)

    @mock.patch.object(dev_stack, "run_checked")
    def test_seed_includes_sick_demo_character(self, run_checked):
        run_checked.return_value = mock.Mock(returncode=0, stdout="")

        self.assertEqual(dev_stack.seed("http://localhost:1", "db", "a" * 64), 0)
        self.assertEqual(run_checked.call_args.args[0][-3:], ["bootstrap_development_world", "a" * 64, "false"])

    @mock.patch.object(dev_stack, "run_checked")
    def test_seed_includes_damaged_demo_character(self, run_checked):
        run_checked.return_value = mock.Mock(returncode=0, stdout="")

        self.assertEqual(
            dev_stack.seed("http://localhost:1", "db", "a" * 64, include_damaged_demo=True),
            0,
        )

        self.assertEqual(run_checked.call_args.args[0][-3:], ["bootstrap_development_world", "a" * 64, "true"])

    @mock.patch.object(dev_stack, "run_checked")
    def test_publish_messages_distinguish_reset(self, run_checked):
        run_checked.return_value = mock.Mock(returncode=1, stdout="failed\n")
        ordinary = io.StringIO()
        with redirect_stderr(ordinary):
            dev_stack.publish("http://localhost:3000", "canonical")
        self.assertIn("Data was not deleted", ordinary.getvalue())
        values = dev_stack.profile_values("demo", 23100)
        listener = {"pid": 123, "executable": "stdb", "start_token": "1"}
        capability = dev_stack.ResetCapability(
            "demo", 23100, "http://127.0.0.1:23100", str(values["database"]),
            mock.Mock(held=True), listener,
        )
        reset = io.StringIO()
        with mock.patch.object(dev_stack, "listener_process_snapshot", return_value=listener), \
             mock.patch.object(dev_stack, "identity_matches", return_value=True), \
             redirect_stderr(reset):
            dev_stack.reset_publish(capability)
        self.assertIn("deletion may already have occurred", reset.getvalue().lower())
        self.assertNotIn("Data was not deleted", reset.getvalue())

    @mock.patch.object(dev_stack, "run_checked")
    def test_ordinary_publish_never_adds_delete_flag(self, run_checked):
        run_checked.return_value = mock.Mock(returncode=0, stdout="")
        self.assertEqual(dev_stack.publish("http://localhost:3000", "canonical"), 0)
        self.assertNotIn("--delete-data=always", run_checked.call_args.args[0])

    @mock.patch.object(dev_stack, "run_checked")
    def test_internal_reset_is_noninteractive_and_identity_bound(self, run_checked):
        run_checked.return_value = mock.Mock(returncode=0, stdout="")
        values = dev_stack.profile_values("demo", 23100)
        listener = {"pid": 123, "executable": "stdb", "start_token": "1"}
        capability = dev_stack.ResetCapability(
            "demo", 23100, "http://127.0.0.1:23100", str(values["database"]),
            mock.Mock(held=True), listener,
        )
        with mock.patch.object(dev_stack, "listener_process_snapshot", return_value=listener), \
             mock.patch.object(dev_stack, "identity_matches", return_value=True):
            self.assertEqual(dev_stack.reset_publish(capability), 0)
        command = run_checked.call_args.args[0]
        self.assertIn("--delete-data=always", command)
        self.assertIn("--yes", command)

    def test_internal_reset_refuses_missing_or_changed_ownership(self):
        values = dev_stack.profile_values("demo", 23100)
        listener = {"pid": 123, "executable": "stdb", "start_token": "1"}
        unlocked = dev_stack.ResetCapability(
            "demo", 23100, "http://127.0.0.1:23100", str(values["database"]),
            mock.Mock(held=False), listener,
        )
        with self.assertRaises(ValueError):
            dev_stack.reset_publish(unlocked)
        locked = dev_stack.ResetCapability(
            "demo", 23100, "http://127.0.0.1:23100", str(values["database"]),
            mock.Mock(held=True), listener,
        )
        changed = dict(listener, start_token="2")
        with mock.patch.object(dev_stack, "listener_process_snapshot", return_value=changed):
            with self.assertRaises(ValueError):
                dev_stack.reset_publish(locked)

    def test_public_publish_parser_has_no_reset_option(self):
        parser = dev_stack.create_parser()
        with redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):
                parser.parse_args([
                    "publish", "--server", "http://127.0.0.1:23100", "--database", "db",
                    "--reset-profile", "demo", "--base-port", "23100",
                ])

    def test_profile_parser_supports_strategic_only_mode(self):
        args = dev_stack.create_parser().parse_args([
            "run-profile", "--strategic-only", "demo", "23100",
        ])
        self.assertTrue(args.strategic_only)
        self.assertEqual(args.name, "demo")
        self.assertEqual(args.base_port, 23100)

    def test_binding_diff_detects_changed_and_extra_files(self):
        with tempfile.TemporaryDirectory() as left, tempfile.TemporaryDirectory() as right:
            Path(left, "a.rs").write_text("fn a() {}\n")
            Path(right, "a.rs").write_text("fn b() {}\n")
            Path(right, "b.rs").write_text("")
            self.assertEqual(dev_stack.binding_differences(Path(left), Path(right)), ["b.rs", "a.rs"])

    def test_nonpositive_pid_rejected(self):
        self.assertIsNone(dev_stack.process_snapshot(0))
        self.assertIsNone(dev_stack.process_snapshot(-10))

    def test_current_process_identity_matches(self):
        snapshot = dev_stack.process_snapshot(os.getpid())
        self.assertIsNotNone(snapshot)
        self.assertTrue(dev_stack.identity_matches(snapshot))

    def test_reused_pid_wrong_executable_or_start_token_rejected(self):
        snapshot = dev_stack.process_snapshot(os.getpid())
        wrong_exe = dict(snapshot, executable="not-this-process")
        wrong_start = dict(snapshot, start_token="not-this-start")
        self.assertFalse(dev_stack.identity_matches(wrong_exe))
        self.assertFalse(dev_stack.identity_matches(wrong_start))

    def test_stop_refuses_identity_mismatch(self):
        with tempfile.TemporaryDirectory() as temp:
            metadata = Path(temp, "process.json")
            dev_stack.atomic_write_json(metadata, {"config": {"role": "test"}, "process": {"pid": os.getpid(), "executable": "wrong", "start_token": "wrong"}})
            with self.assertRaises(ValueError):
                dev_stack.stop_recorded(metadata, {"role": "test"})
            self.assertTrue(metadata.exists())

    def test_canonical_stop_does_not_require_tactical_binaries(self):
        with tempfile.TemporaryDirectory() as temp, \
             mock.patch.object(dev_stack, "runtime_root", return_value=Path(temp)), \
             mock.patch.object(dev_stack, "spawner_identity") as spawner_identity:
            self.assertEqual(dev_stack.canonical_spawner("stop"), 0)
        spawner_identity.assert_not_called()

    def test_child_exit_rejected_during_readiness(self):
        with tempfile.TemporaryDirectory() as temp:
            metadata = Path(temp, "process.json")
            log = Path(temp, "log")
            metadata.write_text(json.dumps({"process": {"pid": 1}}))
            log.write_text("")
            child = mock.Mock()
            child.poll.return_value = 1
            with self.assertRaises(RuntimeError):
                dev_stack.wait_for_spacetime(child, metadata, log, 23100)

    def test_just_recipe_shell_quotes_untrusted_parameters(self):
        source = Path(dev_stack.ROOT, "justfile").read_text()
        lines = [line for line in source.splitlines() if "run-profile" in line]
        self.assertEqual(len(lines), 2)
        for line in lines:
            compact = line.replace(" ", "")
            self.assertIn("{{quote(profile)}}", compact)
            self.assertIn("{{quote(base_port)}}", compact)
            self.assertNotIn("'{{profile}}'", line)

    def test_strategic_only_recipes_skip_tactical_builds(self):
        source = Path(dev_stack.ROOT, "justfile").read_text()
        canonical = next(line for line in source.splitlines() if line.startswith("web-strategic:"))
        isolated = next(line for line in source.splitlines() if line.startswith("web-isolated-strategic "))
        for line in (canonical, isolated):
            self.assertNotIn("build-wasm", line)
            self.assertNotIn("build-tactical", line)


if __name__ == "__main__":
    unittest.main()
