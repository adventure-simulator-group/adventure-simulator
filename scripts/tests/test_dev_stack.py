import io
import json
import os
from pathlib import Path
import socket
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
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
        self.assertEqual(
            dev_stack.profile_ports(values, dev_stack.ProfileMode.BARE_STRATEGIC),
            [23100, 23101],
        )
        self.assertEqual(
            dev_stack.profile_ports(values, dev_stack.ProfileMode.TACTICAL),
            [23100, 23101],
        )
        self.assertEqual(
            dev_stack.profile_ports(values, dev_stack.ProfileMode.STRATEGIC),
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
        self.assertEqual(run_checked.call_args.args[0][-2:], ["bootstrap_development_world", "a" * 64])

    @mock.patch.object(dev_stack, "run_checked")
    def test_seed_has_no_optional_visual_fixture_flag(self, run_checked):
        run_checked.return_value = mock.Mock(returncode=0, stdout="")

        self.assertEqual(dev_stack.seed("http://localhost:1", "db", "a" * 64), 0)
        self.assertEqual(run_checked.call_args.args[0][-2:], ["bootstrap_development_world", "a" * 64])

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

    @mock.patch.object(dev_stack.subprocess, "run")
    def test_checked_commands_decode_output_as_utf8(self, run):
        run.return_value = mock.Mock(returncode=0, stdout="")

        dev_stack.run_checked(["spacetime", "publish"])

        self.assertEqual(run.call_args.kwargs["encoding"], "utf-8")
        self.assertEqual(run.call_args.kwargs["errors"], "replace")
        self.assertTrue(run.call_args.kwargs["text"])
        self.assertEqual(run.call_args.kwargs["stderr"], dev_stack.subprocess.STDOUT)

    def test_console_output_replaces_characters_unsupported_by_windows_encoding(self):
        class LegacyConsole(io.StringIO):
            @property
            def encoding(self):
                return "cp1252"

        console = LegacyConsole()
        with mock.patch.object(dev_stack.sys, "stdout", console):
            dev_stack.write_console("published \u0101")

        self.assertEqual(console.getvalue(), "published ?")

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

    def test_profile_parser_supports_bare_strategic_mode(self):
        args = dev_stack.create_parser().parse_args([
            "run-profile", "--mode", "bare-strategic", "demo", "23100",
        ])
        self.assertEqual(args.mode, "bare-strategic")
        self.assertEqual(args.name, "demo")
        self.assertEqual(args.base_port, 23100)

    def test_profile_parser_supports_tactical_mode(self):
        args = dev_stack.create_parser().parse_args([
            "run-profile", "--mode", "tactical", "demo", "23100",
        ])
        self.assertEqual(args.mode, "tactical")
        self.assertEqual(args.mission_id, "mission:test-mission")
        self.assertEqual(args.scene_key, "hills")
        self.assertEqual(args.character_id, 0)
        self.assertEqual(args.enemy_count, 3)

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

    def test_spacetime_launcher_exec_transition_is_allowed(self):
        self.assertTrue(dev_stack.executable_identity_matches(
            "/usr/bin/spacetime",
            "/usr/bin/spacetimedb-standalone",
        ))
        self.assertTrue(dev_stack.executable_identity_matches(
            r"C:\tools\spacetimedb-cli.exe",
            r"C:\tools\spacetime-standalone.exe",
        ))
        self.assertFalse(dev_stack.executable_identity_matches(
            "/usr/bin/python",
            "/usr/bin/spacetimedb-standalone",
        ))

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
        parameterized = [line for line in lines if "{{ quote(profile) }}" in line]
        fixed_demos = [line for line in lines if line not in parameterized]
        self.assertEqual(len(parameterized), 3)
        self.assertEqual(len(fixed_demos), 0)
        for line in parameterized:
            compact = line.replace(" ", "")
            self.assertIn("{{quote(profile)}}", compact)
            self.assertIn("{{quote(base_port)}}", compact)
            self.assertNotIn("'{{profile}}'", line)

    def test_write_and_remove_tactical_env_file(self):
        with mock.patch.object(dev_stack, "TACTICAL_ENV_FILE", Path(tempfile.mkdtemp()) / ".env.tactical"):
            dev_stack.write_tactical_env_file(
                url="http://127.0.0.1:23310",
                database="adventuresim-dev-demo-abc123",
                port=23312,
                mission_id="test-mission",
                scene_key="hills",
                character_id=0,
                enemy_count=3,
                tactical_claim="deadbeef",
            )
            content = dev_stack.TACTICAL_ENV_FILE.read_text()
            self.assertIn("TACTICAL_SPACETIMEDB_URL=http://127.0.0.1:23310", content)
            self.assertIn("TACTICAL_SPACETIMEDB_MODULE=adventuresim-dev-demo-abc123", content)
            self.assertIn("TACTICAL_PORT=23312", content)
            self.assertIn("TACTICAL_MISSION_ID=test-mission", content)
            self.assertIn("TACTICAL_SCENE_KEY=hills", content)
            self.assertIn("TACTICAL_CHARACTER_ID=0", content)
            self.assertIn("TACTICAL_BOTS=3", content)
            self.assertIn("ADVENTURESIM_TACTICAL_CLAIM=deadbeef", content)
            dev_stack.remove_tactical_env_file()
            self.assertFalse(dev_stack.TACTICAL_ENV_FILE.exists())
            dev_stack.remove_tactical_env_file()

    def test_supervised_env_omits_claim_and_records_profile_identity(self):
        with mock.patch.object(
            dev_stack, "TACTICAL_ENV_FILE", Path(tempfile.mkdtemp()) / ".env.tactical"
        ):
            dev_stack.write_tactical_env_file(
                url="http://127.0.0.1:24920",
                database="adventuresim-dev-tactical-play-animation-abc123",
                port=24922,
                mission_id="mission:animation-123",
                scene_key="hills",
                character_id=0,
                enemy_count=1,
                tactical_claim=None,
                profile="tactical-play-animation",
                worktree_fingerprint_value="abc123",
                run_dir=Path("C:/Users/test/runtime/run"),
                session_id="session-123",
                play_mode="animation",
            )
            content = dev_stack.TACTICAL_ENV_FILE.read_text()
            self.assertNotIn("ADVENTURESIM_TACTICAL_CLAIM", content)
            self.assertIn("TACTICAL_SESSION_ID=session-123", content)
            self.assertIn("TACTICAL_PLAY_MODE=animation", content)
            self.assertIn("TACTICAL_RUN_DIR=C:/Users/test/runtime/run", content)
            self.assertNotIn("TACTICAL_RUN_DIR=C:\\", content)

    def test_tactical_play_parser_profiles(self):
        parser = dev_stack.create_parser()
        animation = parser.parse_args(["tactical-play", "animation"])
        networking = parser.parse_args(["tactical-play", "networking", "25000"])
        self.assertEqual(animation.base_port, 24920)
        self.assertEqual(networking.base_port, 25000)

    def test_visual_and_networking_profiles_disable_combat(self):
        self.assertEqual(
            dev_stack.tactical_combat_scale(dev_stack.TacticalPlayMode.ANIMATION), 0
        )
        self.assertEqual(
            dev_stack.tactical_combat_scale(dev_stack.TacticalPlayMode.NETWORKING), 0
        )
        self.assertEqual(
            dev_stack.tactical_combat_scale(dev_stack.TacticalPlayMode.COMBAT), 10_000
        )

    @mock.patch.object(dev_stack, "profile_values")
    @mock.patch.object(dev_stack, "build_tactical_play", return_value=9)
    def test_build_failure_prevents_profile_or_mission_creation(self, _build, profile_values):
        self.assertEqual(
            dev_stack.tactical_play(dev_stack.TacticalPlayMode.ANIMATION, 24920), 9
        )
        profile_values.assert_not_called()

    def test_supervised_state_rejects_stale_worktree_environment(self):
        environment = {
            "TACTICAL_PROFILE": "tactical-play-animation",
            "TACTICAL_WORKTREE_FINGERPRINT": "another-worktree",
            "TACTICAL_RUN_DIR": "unused",
            "TACTICAL_SESSION_ID": "session",
            "TACTICAL_SPACETIMEDB_URL": "http://127.0.0.1:24920",
            "TACTICAL_SPACETIMEDB_MODULE": "db",
            "TACTICAL_PORT": "24922",
        }
        with mock.patch.object(dev_stack, "read_tactical_env_file", return_value=environment):
            with self.assertRaisesRegex(ValueError, "another worktree"):
                dev_stack.supervised_tactical_state()

    def test_readiness_rejects_unrecorded_listener(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            metadata = root / "server.json"
            log = root / "server.log"
            recorded = {"pid": 10, "executable": "server", "start_token": "1"}
            metadata.write_text(json.dumps({"process": recorded}))
            log.write_text("")
            process = mock.Mock()
            process.poll.return_value = None
            with mock.patch.object(dev_stack, "identity_matches", return_value=True), \
                 mock.patch.object(
                     dev_stack,
                     "listener_process_snapshot",
                     return_value={"pid": 11, "executable": "other", "start_token": "1"},
                 ):
                with self.assertRaisesRegex(RuntimeError, "unrecorded process"):
                    dev_stack.wait_for_tactical_server(
                        process, metadata, log, "http://127.0.0.1:1", "db",
                        "mission:test", 24922,
                    )

    def test_readiness_requires_consumed_claim_and_registered_authority(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            metadata = root / "server.json"
            log = root / "server.log"
            recorded = {"pid": 10, "executable": "server", "start_token": "1"}
            metadata.write_text(json.dumps({"process": recorded}))
            log.write_text("")
            process = mock.Mock()
            process.poll.return_value = None
            with mock.patch.object(dev_stack, "identity_matches", return_value=True), \
                 mock.patch.object(dev_stack, "listener_process_snapshot", return_value=recorded), \
                 mock.patch.object(
                     dev_stack, "sql_mission_row_exists", side_effect=[True, False, False]
                 ) as sql:
                self.assertEqual(
                    dev_stack.wait_for_tactical_server(
                        process, metadata, log, "http://127.0.0.1:1", "db",
                        "mission:test", 24922,
                    ),
                    recorded,
                )
            self.assertEqual(sql.call_count, 3)

    @mock.patch.object(dev_stack, "run_checked")
    def test_sql_readiness_accepts_cli_quoted_text_rows(self, run_checked):
        run_checked.return_value = mock.Mock(
            returncode=0,
            stdout=' mission_id\n------------\n "mission:animation-123" \n',
        )
        self.assertTrue(dev_stack.sql_mission_row_exists(
            "http://127.0.0.1:24920",
            "db",
            "tactical_server_authority",
            "mission:animation-123",
        ))

    def test_status_explains_consumed_claim_with_dead_server(self):
        with tempfile.TemporaryDirectory() as temporary:
            run_dir = Path(temporary)
            (run_dir / "spacetime.identity.json").write_text(json.dumps({
                "listener": {"pid": 10, "executable": "stdb", "start_token": "1"}
            }))
            environment = {
                "TACTICAL_SPACETIMEDB_URL": "http://127.0.0.1:24920",
                "TACTICAL_SPACETIMEDB_MODULE": "db",
            }
            config = {
                "profile": "tactical-play-animation",
                "worktree_fingerprint": "abc",
                "mission_id": "mission:test",
                "tactical_port": 24922,
                "native_client": True,
            }
            output = io.StringIO()
            with mock.patch.object(
                dev_stack, "supervised_tactical_state",
                return_value=(environment, config, run_dir),
            ), mock.patch.object(dev_stack, "identity_matches", return_value=True), \
                 mock.patch.object(
                     dev_stack, "sql_mission_row_exists", side_effect=[True, False, False]
                 ), redirect_stdout(output):
                self.assertEqual(dev_stack.tactical_status(), 1)
            self.assertIn("claim was already consumed", output.getvalue())
            self.assertIn("Recovery: just tactical-play animation", output.getvalue())

    def test_strategic_only_recipes_skip_tactical_builds(self):
        source = Path(dev_stack.ROOT, "justfile").read_text()
        canonical = next(line for line in source.splitlines() if line.startswith("web-strategic:"))
        isolated = next(line for line in source.splitlines() if line.startswith("web-isolated-strategic "))
        for line in (canonical, isolated):
            self.assertNotIn("build-wasm", line)
            self.assertNotIn("build-tactical", line)

    def test_canonical_web_recipes_do_not_invoke_private_fixture_bootstrap(self):
        source = Path(dev_stack.ROOT, "justfile").read_text()
        recipes = [
            next(line for line in source.splitlines() if line.startswith(prefix))
            for prefix in ("web:", "web-strategic:", "web-secure:")
        ]
        for recipe in recipes:
            self.assertNotIn("_seed-world", recipe)
        self.assertNotIn("\n_seed-world ", source)


if __name__ == "__main__":
    unittest.main()
