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
    def test_runtime_root_accepts_only_an_absolute_override(self):
        with mock.patch.dict(os.environ, {"ADVENTURESIM_RUNTIME_ROOT": "relative"}):
            with self.assertRaises(ValueError):
                dev_stack.runtime_root()
        with tempfile.TemporaryDirectory() as state, mock.patch.dict(
            os.environ, {"ADVENTURESIM_RUNTIME_ROOT": state}
        ):
            self.assertEqual(dev_stack.runtime_root(), Path(state))

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
    def test_startup_benchmark_persists_early_and_attached_phases(self):
        with tempfile.TemporaryDirectory() as temporary, mock.patch.object(
            dev_stack.time, "monotonic", side_effect=[10.0, 11.0, 12.0, 13.0, 14.0]
        ):
            benchmark = dev_stack.StartupBenchmark.start()
            benchmark.record("build", 10.0)
            output = Path(temporary) / "startup.jsonl"
            benchmark.attach(output)
            benchmark.record("database", 12.0)
            events = [json.loads(line) for line in output.read_text().splitlines()]
            self.assertEqual(
                [event["phase"] for event in events], ["build", "database"]
            )
            self.assertEqual(events[0]["duration_seconds"], 1.0)
            self.assertEqual(events[1]["elapsed_seconds"], 4.0)

    def test_binding_verification_cache_requires_module_and_bindings_digests(self):
        with tempfile.TemporaryDirectory() as temporary:
            cache = Path(temporary) / "bindings.json"
            dev_stack.atomic_write_json(cache, {
                "format": 1,
                "module_input_digest": "module-a",
                "generated_bindings_digest": "bindings-a",
            })
            with mock.patch.object(
                dev_stack, "generated_bindings_digest", return_value="bindings-a"
            ), mock.patch.object(dev_stack.tempfile, "TemporaryDirectory") as generated:
                self.assertEqual(dev_stack.verify_bindings(
                    cache_path=cache, current_module_digest="module-a"
                ), 0)
                generated.assert_not_called()

            failed = mock.Mock(returncode=7, stdout="generation failed")
            with mock.patch.object(
                dev_stack, "generated_bindings_digest", return_value="bindings-b"
            ), mock.patch.object(dev_stack, "run_checked", return_value=failed):
                self.assertEqual(dev_stack.verify_bindings(
                    cache_path=cache, current_module_digest="module-a"
                ), 7)

    def test_tactical_profile_identity_invalidates_module_and_bootstrap_changes(self):
        values = dev_stack.profile_values("demo", 23100)
        first = dev_stack.tactical_profile_identity(values, "module-a", "token-a")
        self.assertNotEqual(
            first,
            dev_stack.tactical_profile_identity(values, "module-b", "token-a"),
        )
        self.assertNotEqual(
            first,
            dev_stack.tactical_profile_identity(values, "module-a", "token-b"),
        )
        self.assertNotIn("token-a", json.dumps(first))

    @mock.patch.object(dev_stack, "tactical_profile_database_is_ready", return_value=True)
    def test_tactical_profile_cache_requires_exact_identity_and_live_seed(
        self, database_is_ready
    ):
        with tempfile.TemporaryDirectory() as temporary:
            state = Path(temporary) / "state.json"
            identity = {"format": 1, "module_input_digest": "module-a"}
            dev_stack.atomic_write_json(state, identity)
            self.assertTrue(dev_stack.tactical_profile_cache_is_valid(
                state, identity, "http://localhost:1", "db"
            ))
            self.assertFalse(dev_stack.tactical_profile_cache_is_valid(
                state, {**identity, "module_input_digest": "module-b"},
                "http://localhost:1", "db",
            ))
            self.assertEqual(database_is_ready.call_count, 1)

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
        self.assertEqual(args.scene_key, "woodland")
        self.assertEqual(args.scene_input, "assets/tactical-scenes/dense-woodland.json")
        self.assertEqual(args.character_id, 1)
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

    def test_termination_accepts_a_concurrent_natural_exit(self):
        expected = {"pid": 42, "executable": "owned", "start_token": "start"}
        with mock.patch.object(
            dev_stack, "terminate_verified", side_effect=ValueError("already exiting")
        ), mock.patch.object(
            dev_stack, "process_snapshot", side_effect=[expected, None]
        ), mock.patch.object(dev_stack.time, "sleep"):
            dev_stack.terminate_verified_or_accept_exit(expected)

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
        diagnostic = parser.parse_args(["tactical-play", "diagnostic"])
        no_shadows = parser.parse_args([
            "tactical-play", "diagnostic", "25020", "--graphics-preset", "no-shadows"
        ])
        traced = parser.parse_args([
            "tactical-play", "animation", "--presentation-trace", "required"
        ])
        no_vsync = parser.parse_args([
            "tactical-play", "animation", "--present-mode", "auto-no-vsync"
        ])
        captured = parser.parse_args([
            "tactical-play", "diagnostic", "--window-capture", "required"
        ])
        display_dx12 = parser.parse_args([
            "tactical-play", "diagnostic", "--capture-source", "display",
            "--render-backend", "dx12",
        ])
        networking = parser.parse_args(["tactical-play", "networking", "25000"])
        benchmark = parser.parse_args([
            "tactical-play", "diagnostic", "--input-script", "benchmark.json"
        ])
        timed_release = parser.parse_args([
            "tactical-play", "animation", "--client-profile", "release",
            "--frame-timing-seconds", "15", "--frame-timing-warmup-seconds", "5",
        ])
        self.assertEqual(animation.base_port, 24920)
        self.assertEqual(animation.presentation_trace, "auto")
        self.assertEqual(diagnostic.mode, "diagnostic")
        self.assertEqual(no_shadows.graphics_preset, "no-shadows")
        self.assertEqual(traced.presentation_trace, "required")
        self.assertEqual(no_vsync.present_mode, "auto-no-vsync")
        self.assertEqual(diagnostic.window_capture, "auto")
        self.assertEqual(diagnostic.capture_source, "window")
        self.assertEqual(diagnostic.render_backend, "auto")
        self.assertEqual(captured.window_capture, "required")
        self.assertEqual(display_dx12.capture_source, "display")
        self.assertEqual(display_dx12.render_backend, "dx12")
        self.assertEqual(networking.base_port, 25000)
        self.assertEqual(benchmark.input_script, "benchmark.json")
        self.assertEqual(timed_release.client_profile, "release")
        self.assertEqual(timed_release.frame_timing_seconds, 15.0)
        self.assertEqual(timed_release.frame_timing_warmup_seconds, 5.0)

    def test_removed_high_environment_light_preset_is_rejected(self):
        parser = dev_stack.create_parser()
        with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            parser.parse_args([
                "tactical-play",
                "diagnostic",
                "25020",
                "--graphics-preset",
                "high-environment-light",
            ])

    def test_animation_profile_does_not_enable_unbounded_frame_logging(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / "adventuresim-tactical-client.exe"
            executable.touch()
            process = mock.Mock(pid=1234)
            process.poll.return_value = None
            config = {
                "worktree_fingerprint": "abc",
                "session_id": "session-123456789",
                "character_id": 0,
                "tactical_port": 24922,
                "play_mode": "animation",
                "window_capture": "off",
            }
            with mock.patch.object(
                dev_stack, "tactical_executable", return_value=executable
            ), mock.patch.object(
                dev_stack, "spawn_recorded", return_value=process
            ) as spawn, mock.patch.object(dev_stack.time, "sleep"):
                dev_stack.launch_recorded_tactical_client(root, config)
            command = spawn.call_args.args[0]
            self.assertNotIn("--animation-log", command)
            self.assertNotIn("--input-script", command)
            self.assertNotIn("animation_log", config)

    def test_animation_timing_profile_adds_only_compact_bounded_trace(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / "adventuresim-tactical-client.exe"
            executable.touch()
            process = mock.Mock(pid=1234)
            process.poll.return_value = None
            config = {
                "worktree_fingerprint": "abc",
                "session_id": "session-123456789",
                "character_id": 0,
                "tactical_port": 24922,
                "play_mode": "animation",
                "window_capture": "off",
                "client_profile": "release",
                "frame_timing_seconds": 15.0,
                "frame_timing_warmup_seconds": 5.0,
            }
            with mock.patch.object(
                dev_stack, "tactical_executable", return_value=executable
            ) as executable_path, mock.patch.object(
                dev_stack, "spawn_recorded", return_value=process
            ) as spawn, mock.patch.object(dev_stack.time, "sleep"):
                dev_stack.launch_recorded_tactical_client(root, config)
            executable_path.assert_called_once_with(
                "adventuresim-tactical-client", "release"
            )
            command = spawn.call_args.args[0]
            self.assertIn("--frame-timing-log", command)
            self.assertIn("--frame-timing-seconds", command)
            self.assertIn("--frame-timing-warmup-seconds", command)
            self.assertNotIn("--animation-log", command)
            self.assertNotIn("--input-script", command)
            self.assertEqual(
                Path(config["frame_timing_log"]).parent,
                root,
            )

    def test_diagnostic_profile_waits_for_capture_before_scripted_motion(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / "adventuresim-tactical-client.exe"
            executable.touch()
            process = mock.Mock(pid=1234)
            process.poll.return_value = None
            config = {
                "worktree_fingerprint": "abc",
                "session_id": "session-123456789",
                "character_id": 0,
                "tactical_port": 24922,
                "play_mode": "diagnostic",
                "window_capture": "required",
            }
            with mock.patch.object(
                dev_stack, "tactical_executable", return_value=executable
            ), mock.patch.object(
                dev_stack, "spawn_recorded", return_value=process
            ), mock.patch.object(dev_stack.time, "sleep"):
                dev_stack.launch_recorded_tactical_client(root, config)
            script = json.loads(Path(config["input_script"]).read_text())
            self.assertEqual(script["commands"][0]["type"], "wait_for_signal")
            self.assertEqual(
                script["commands"][0]["path"], config["capture_ready_signal"]
            )
            self.assertEqual(script["commands"][1]["type"], "rotate")
            command_types = [command["type"] for command in script["commands"]]
            self.assertIn("attack", command_types)
            self.assertIn("screenshot", command_types)
            screenshot = next(
                command for command in script["commands"]
                if command["type"] == "screenshot"
            )
            self.assertEqual(
                screenshot["path"], config["animation_attack_screenshot"]
            )
            self.assertIn("animation_log", config)

    def test_diagnostic_profile_copies_custom_input_script_after_capture_gate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / "adventuresim-tactical-client.exe"
            executable.touch()
            source = root / "benchmark.json"
            source.write_text(
                json.dumps({"commands": [{"type": "guard", "raised": True}]}),
                encoding="utf-8",
            )
            process = mock.Mock(pid=1234)
            process.poll.return_value = None
            config = {
                "worktree_fingerprint": "abc",
                "session_id": "session-123456789",
                "character_id": 0,
                "tactical_port": 24922,
                "play_mode": "diagnostic",
                "window_capture": "required",
                "input_script_source": str(source),
            }
            with mock.patch.object(
                dev_stack, "tactical_executable", return_value=executable
            ), mock.patch.object(
                dev_stack, "spawn_recorded", return_value=process
            ), mock.patch.object(dev_stack.time, "sleep"):
                dev_stack.launch_recorded_tactical_client(root, config)

            copied = json.loads(Path(config["input_script"]).read_text())
            self.assertEqual(copied["commands"][0]["type"], "wait_for_signal")
            self.assertEqual(copied["commands"][1], {"type": "guard", "raised": True})
            self.assertNotIn("animation_attack_screenshot", config)

    def test_presentmon_path_can_be_configured(self):
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "PresentMon.exe"
            executable.touch()
            with mock.patch.dict(os.environ, {"PRESENTMON_PATH": str(executable)}):
                self.assertEqual(dev_stack.find_presentmon(), executable)

    def test_obs_path_can_be_configured(self):
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "obs64.exe"
            executable.touch()
            with mock.patch.dict(os.environ, {"OBS_PATH": str(executable)}):
                self.assertEqual(dev_stack.find_obs(), executable)

    def test_obs_monitor_selection_matches_active_display(self):
        items = [
            {"itemName": "Display 1: Integrated Display (1920x1080)", "itemValue": "display-1"},
            {"itemName": "Display 2: Home Monitor (2560x1440)", "itemValue": "display-2"},
        ]
        geometry = {
            "monitor_name": "Home Monitor",
            "monitor_width": 2560,
            "monitor_height": 1440,
            "monitor_left": 1920,
            "monitor_top": 0,
        }
        self.assertEqual(dev_stack.select_obs_monitor_id(items, geometry), "display-2")
        self.assertEqual(
            dev_stack.select_obs_monitor_id(items, geometry, "display-1"),
            "display-1",
        )
        with self.assertRaisesRegex(RuntimeError, "OBS_MONITOR_ID"):
            dev_stack.select_obs_monitor_id(items, geometry, "missing")

    def test_obs_monitor_selection_rejects_ambiguous_display(self):
        items = [
            {"itemName": "Display: Same Monitor", "itemValue": "one"},
            {"itemName": "Display: Same Monitor", "itemValue": "two"},
        ]
        geometry = {
            "monitor_name": "Same Monitor",
            "monitor_width": 1920,
            "monitor_height": 1080,
            "monitor_left": 0,
            "monitor_top": 0,
        }
        with self.assertRaisesRegex(RuntimeError, "uniquely match"):
            dev_stack.select_obs_monitor_id(items, geometry)

    def test_obs_screenshot_readiness_requires_nonblack_pixels(self):
        def png_data_url(rgb):
            def chunk(kind, data):
                return (
                    dev_stack.struct.pack(">I", len(data))
                    + kind
                    + data
                    + b"\0\0\0\0"
                )

            header = dev_stack.struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)
            payload = (
                b"\x89PNG\r\n\x1a\n"
                + chunk(b"IHDR", header)
                + chunk(b"IDAT", dev_stack.zlib.compress(b"\0" + bytes(rgb)))
                + chunk(b"IEND", b"")
            )
            return "data:image/png;base64," + dev_stack.base64.b64encode(payload).decode()

        black = png_data_url((0, 0, 0))
        visible = png_data_url((12, 24, 36))
        self.assertFalse(dev_stack.obs_screenshot_has_visible_pixels(black))
        self.assertTrue(dev_stack.obs_screenshot_has_visible_pixels(visible))
        self.assertFalse(dev_stack.obs_screenshot_has_visible_pixels("not an image"))

        class FakeWebSocket:
            def __init__(self):
                self.calls = 0

            def request(self, request_type, data=None):
                self.calls += 1
                self.assert_request = (request_type, data)
                return {"imageData": visible if self.calls >= 3 else black}

        websocket = FakeWebSocket()
        with mock.patch.object(dev_stack.time, "sleep"):
            dev_stack.wait_for_obs_source_ready(websocket, "Tactical client", 1.0)
        self.assertEqual(websocket.calls, 3)
        self.assertEqual(websocket.assert_request[0], "GetSourceScreenshot")

    def test_obs_workspace_uses_portable_named_profile_and_collection(self):
        class FakeWebSocket:
            def __init__(self):
                self.calls = []

            def request(self, request_type, data=None):
                self.calls.append((request_type, data))
                if request_type == "GetProfileList":
                    return {
                        "currentProfileName": "Developer Profile",
                        "profiles": ["Developer Profile", "Portable Diagnostics"],
                    }
                if request_type == "GetSceneCollectionList":
                    return {
                        "currentSceneCollectionName": "Developer Scenes",
                        "sceneCollections": ["Developer Scenes", "Portable Scenes"],
                    }
                return {}

        websocket = FakeWebSocket()
        config = {}
        with tempfile.TemporaryDirectory() as directory, mock.patch.dict(
            os.environ,
            {"OBS_PROFILE": "Portable Diagnostics", "OBS_COLLECTION": "Portable Scenes"},
            clear=True,
        ), mock.patch.object(dev_stack.time, "sleep"):
            dev_stack.configure_obs_workspace(websocket, Path(directory), config)

        self.assertIn(
            ("SetCurrentProfile", {"profileName": "Portable Diagnostics"}),
            websocket.calls,
        )
        self.assertIn(
            (
                "SetCurrentSceneCollection",
                {"sceneCollectionName": "Portable Scenes"},
            ),
            websocket.calls,
        )
        self.assertFalse(any(call[0] == "CreateProfile" for call in websocket.calls))
        self.assertFalse(
            any(call[0] == "CreateSceneCollection" for call in websocket.calls)
        )
        self.assertEqual(config["obs_original_profile"], "Developer Profile")
        self.assertEqual(config["obs_original_collection"], "Developer Scenes")

    def test_restore_obs_workspace_removes_capture_and_restores_user_state(self):
        class FakeWebSocket:
            def __init__(self):
                self.calls = []

            def request(self, request_type, data=None):
                self.calls.append((request_type, data))
                return {}

        websocket = FakeWebSocket()
        config = {
            "obs_original_scene": "Gameplay",
            "obs_capture_scene": "Fabelgeist diagnostic abc",
            "obs_original_collection": "Developer Scenes",
            "obs_capture_collection": "Portable Scenes",
            "obs_original_profile": "Developer Profile",
            "obs_capture_profile": "Portable Diagnostics",
        }
        with mock.patch.object(dev_stack.time, "sleep"):
            dev_stack.restore_obs_workspace(websocket, config, remove_capture_scene=True)
        self.assertEqual(
            websocket.calls,
            [
                ("SetCurrentProgramScene", {"sceneName": "Gameplay"}),
                (
                    "RemoveScene",
                    {"sceneName": "Fabelgeist diagnostic abc"},
                ),
                (
                    "SetCurrentSceneCollection",
                    {"sceneCollectionName": "Developer Scenes"},
                ),
                ("SetCurrentProfile", {"profileName": "Developer Profile"}),
            ],
        )

    def test_obs_stop_restores_workspace_even_when_record_stop_fails(self):
        class FakeWebSocket:
            def __init__(self):
                self.calls = []
                self.closed = False

            def request(self, request_type, data=None):
                self.calls.append((request_type, data))
                if request_type == "StopRecord":
                    raise RuntimeError("recording failed")
                return {}

            def close(self):
                self.closed = True

        websocket = FakeWebSocket()
        capture = dev_stack.ObsCapture(mock.Mock(), websocket, Path("obs.identity.json"))
        config = {
            "obs_original_scene": "Idle",
            "obs_capture_scene": "Fabelgeist diagnostic abc",
            "obs_original_collection": "Developer Scenes",
            "obs_capture_collection": "Portable Scenes",
            "obs_original_profile": "Developer Profile",
            "obs_capture_profile": "Portable Diagnostics",
        }
        with tempfile.TemporaryDirectory() as directory, mock.patch.object(
            dev_stack, "stop_recorded"
        ), mock.patch.object(dev_stack.time, "sleep"):
            with self.assertRaisesRegex(RuntimeError, "recording failed"):
                dev_stack.stop_obs_capture(capture, Path(directory), config)
        self.assertIn(
            ("SetCurrentSceneCollection", {"sceneCollectionName": "Developer Scenes"}),
            websocket.calls,
        )
        self.assertIn(
            ("SetCurrentProfile", {"profileName": "Developer Profile"}),
            websocket.calls,
        )
        self.assertTrue(websocket.closed)

    def test_capture_gate_is_written_atomically(self):
        with tempfile.TemporaryDirectory() as directory:
            signal = Path(directory) / "ready.json"
            dev_stack.release_capture_gate({"capture_ready_signal": str(signal)})
            self.assertEqual(json.loads(signal.read_text()), {"ready": True})

    def test_presentation_trace_auto_is_bounded_to_diagnostic_mode(self):
        self.assertEqual(
            dev_stack.effective_presentation_trace(
                dev_stack.TacticalPlayMode.DIAGNOSTIC, "auto"
            ),
            "auto",
        )
        self.assertEqual(
            dev_stack.effective_presentation_trace(
                dev_stack.TacticalPlayMode.ANIMATION, "auto"
            ),
            "off",
        )
        self.assertEqual(
            dev_stack.effective_presentation_trace(
                dev_stack.TacticalPlayMode.COMBAT, "required"
            ),
            "required",
        )

    def test_animation_lab_and_combat_keep_full_stats(self):
        self.assertEqual(
            dev_stack.tactical_combat_scale(dev_stack.TacticalPlayMode.ANIMATION), 10_000
        )
        self.assertEqual(
            dev_stack.tactical_combat_scale(dev_stack.TacticalPlayMode.DIAGNOSTIC), 0
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

    def test_client_readiness_requires_server_received_input(self):
        with tempfile.TemporaryDirectory() as temporary:
            client_log = Path(temporary) / "client.log"
            server_log = Path(temporary) / "server.log"
            client_log.write_text("render device ready\n")
            server_log.write_text("[startup] first server input received for 399v0\n")
            process = mock.Mock()
            process.poll.return_value = None
            dev_stack.wait_for_tactical_client(process, client_log, server_log)

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
