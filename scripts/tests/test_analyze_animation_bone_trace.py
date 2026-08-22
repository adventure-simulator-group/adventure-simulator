import importlib.util
import json
import math
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "analyze_animation_bone_trace.py"
SPEC = importlib.util.spec_from_file_location("analyze_animation_bone_trace", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class AnalyzeAnimationBoneTraceTests(unittest.TestCase):
    def test_removes_subject_translation_and_rotation_before_measuring_hand(self):
        quarter_turn = math.sin(math.pi / 4.0)
        frames = []
        for frame, local_forward in [(8, 0.0), (9, 0.08)]:
            frames.append(
                {
                    "scenario": "attack",
                    "scenario_frame": frame,
                    "action": "Attack",
                    "subject_translation": [10.0 + frame, 2.0, -4.0],
                    "subject_rotation_xyzw": [0.0, quarter_turn, 0.0, quarter_turn],
                    "bones": [
                        {
                            "name": "r_wrist",
                            "translation": [10.0 + frame + local_forward, 2.0, -4.0],
                        }
                    ],
                }
            )
        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / "trace.jsonl"
            trace.write_text(
                "".join(json.dumps(frame) + "\n" for frame in frames), encoding="utf-8"
            )
            result = MODULE.analyze_trace(trace)

        self.assertEqual(result["active_attack_frames"], 2)
        self.assertAlmostEqual(
            result["hands"]["r_wrist"]["maximum_excursion_metres"], 0.08, places=6
        )
        self.assertAlmostEqual(
            result["hands"]["r_wrist"]["maximum_forward_excursion_metres"],
            0.08,
            places=6,
        )


if __name__ == "__main__":
    unittest.main()
