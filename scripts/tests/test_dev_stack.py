import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import scripts.dev_stack as dev_stack


class ProfileTests(unittest.TestCase):
    def test_profile_has_isolated_identifiers(self):
        values = dev_stack.profile_values("renderer-demo", 23100)
        self.assertEqual(values["database"], "adventuresim-dev-renderer-demo")
        self.assertEqual(values["spacetime_port"], 23100)
        self.assertEqual(values["web_port"], 23101)
        self.assertEqual(values["tactical_port"], 23102)
        self.assertIn("renderer-demo", values["data_dir"])
        self.assertIn("renderer-demo", values["run_dir"])

    def test_rejects_injection_and_bad_ports(self):
        for name in ("../main", "demo;rm", "UPPER", ""):
            with self.assertRaises(ValueError):
                dev_stack.profile_values(name, 23100)
        with self.assertRaises(ValueError):
            dev_stack.profile_values("demo", 65531)

    def test_destructive_server_must_be_exact_loopback(self):
        dev_stack.validate_loopback_server("http://127.0.0.1:23100", 23100)
        for server in ("https://example.com:23100", "http://0.0.0.0:23100", "http://localhost:3000", "http://localhost:23100/evil"):
            with self.assertRaises(ValueError):
                dev_stack.validate_loopback_server(server, 23100)


class WorkflowTests(unittest.TestCase):
    @mock.patch.object(dev_stack, "run_checked")
    def test_seed_propagates_reducer_failure(self, run_checked):
        run_checked.return_value.returncode = 7
        run_checked.return_value.stdout = "reducer rejected\n"
        self.assertEqual(dev_stack.seed("http://localhost:1", "db"), 7)

    @mock.patch.object(dev_stack, "run_checked")
    def test_ordinary_publish_never_adds_delete_flag(self, run_checked):
        run_checked.return_value.returncode = 0
        run_checked.return_value.stdout = ""
        self.assertEqual(dev_stack.publish("http://localhost:3000", "canonical", None, 0), 0)
        self.assertNotIn("--delete-data=always", run_checked.call_args.args[0])

    @mock.patch.object(dev_stack, "run_checked")
    def test_isolated_reset_is_noninteractive_and_identity_bound(self, run_checked):
        run_checked.return_value.returncode = 0
        run_checked.return_value.stdout = ""
        self.assertEqual(
            dev_stack.publish(
                "http://127.0.0.1:23100", "adventuresim-dev-demo", "demo", 23100
            ),
            0,
        )
        command = run_checked.call_args.args[0]
        self.assertIn("--delete-data=always", command)
        self.assertIn("--yes", command)
        with self.assertRaises(ValueError):
            dev_stack.publish("https://example.com:23100", "adventuresim-dev-demo", "demo", 23100)
        with self.assertRaises(ValueError):
            dev_stack.publish("http://127.0.0.1:23100", "canonical", "demo", 23100)

    def test_binding_diff_detects_changed_and_extra_files(self):
        with tempfile.TemporaryDirectory() as left, tempfile.TemporaryDirectory() as right:
            Path(left, "a.rs").write_text("fn a() {}\n")
            Path(right, "a.rs").write_text("fn b() {}\n")
            Path(right, "b.rs").write_text("")
            self.assertEqual(dev_stack.binding_differences(Path(left), Path(right)), ["b.rs", "a.rs"])

    def test_spawner_identity_rejects_another_checkout(self):
        with tempfile.TemporaryDirectory() as temp:
            identity = Path(temp, "identity.json")
            pid = Path(temp, "pid")
            pid.write_text(str(1234))
            identity.write_text(json.dumps({"repository": "elsewhere", "profile": "canonical"}))
            with mock.patch.object(dev_stack, "process_is_running", return_value=True):
                expected = {"repository": "expected", "profile": "canonical"}
                self.assertEqual(dev_stack.check_spawner_identity(identity, pid, expected), 2)


if __name__ == "__main__":
    unittest.main()
