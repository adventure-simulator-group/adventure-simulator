import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest import mock


PATH = Path(__file__).parents[1] / "real_world_tactical.py"
SPEC = importlib.util.spec_from_file_location("real_world_tactical", PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class RealWorldTacticalTests(unittest.TestCase):
    def test_parser_accepts_signed_decimal_coordinates(self):
        args = MODULE.parser().parse_args(["capture", "51.3397", "-1.2345"])
        self.assertEqual((args.latitude, args.longitude), (51.3397, -1.2345))
        self.assertEqual(args.absolute_minute, MODULE.DEFAULT_MINUTE)

    def test_capture_uses_materialized_scene_and_five_composition_views(self):
        with tempfile.TemporaryDirectory() as directory:
            scene = Path(directory) / "scene.json"
            scene.write_text("{}", encoding="utf-8")
            completed = mock.Mock(returncode=0)
            with mock.patch.object(MODULE, "materialize", return_value=scene), mock.patch.object(
                MODULE, "source_identity", return_value="identity"
            ), mock.patch.object(MODULE.subprocess, "run", return_value=completed) as run:
                self.assertEqual(MODULE.main(["capture", "51.3397", "10.705"]), 0)
            command = run.call_args.args[0]
            self.assertIn(str(scene), command)
            self.assertEqual(command.count("--view"), 5)
            self.assertEqual(
                [command[index + 1] for index, value in enumerate(command) if value == "--view"],
                [
                    "beauty-ground", "beauty-overhead", "horizon",
                    "vista-lod-oblique", "vista-valley-oblique",
                ],
            )
            self.assertEqual(run.call_args.kwargs["env"]["CAPTURE_SOURCE_IDENTITY"], "identity")


if __name__ == "__main__":
    unittest.main()
