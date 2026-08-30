import importlib.util
import json
import math
import tempfile
import unittest
from pathlib import Path
from xml.etree import ElementTree


SCRIPT = Path(__file__).parents[1] / "plot_animation_motion_trace.py"
SPEC = importlib.util.spec_from_file_location("plot_animation_motion_trace", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def frame(index, time, position, angle, pose, sampling, action="Attack"):
    half = angle / 2.0
    return {
        "scenario_frame": index,
        "elapsed_seconds": time,
        "action": action,
        "subject_translation": [10.0, 0.0, -4.0],
        "subject_rotation_xyzw": [0.0, 0.0, 0.0, 1.0],
        "evaluation": {
            "action": []
            if action != "Attack"
            else [{"pose": pose, "sampling": sampling, "weight": 1.0}]
        },
        "bones": [
            {
                "name": "r_weapon",
                "translation": [10.0 + position, 0.0, -4.0],
                "rotation_xyzw": [0.0, 0.0, math.sin(half), math.cos(half)],
            }
        ],
    }


def captured_frames():
    return [
        frame(1, 4.00, 0.000, 0.000, "guard_swing", {"CurveSpan": {"coordinate": 0.0, "end": "attack_swing"}}),
        frame(2, 4.10, 0.005, 0.005, "guard_swing", {"CurveSpan": {"coordinate": 0.5, "end": "attack_swing"}}),
        frame(3, 4.20, 0.020, 0.020, "guard_swing", {"CurveSpan": {"coordinate": 1.2, "end": "attack_swing"}}),
        frame(4, 4.30, 0.045, 0.045, "guard_swing", {"ContinuationSpan": {"progress": 0.0, "end": "recover_swing"}}),
        frame(5, 4.40, 0.080, 0.080, "recover_swing", {"Span": {"progress": 0.1, "end": "continue_swing"}}),
        frame(6, 4.50, 0.125, 0.125, "continue_swing", {"Span": {"progress": 0.1, "end": "guard_swing"}}),
    ]


class PlotAnimationMotionTraceTests(unittest.TestCase):
    def test_measures_speed_and_acceleration_in_subject_space(self):
        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / "trace.jsonl"
            trace.write_text("".join(json.dumps(item) + "\n" for item in captured_frames()), encoding="utf-8")
            samples = MODULE.load_attack_cycles(trace, "r_weapon")[0]
            series = MODULE.calculate_motion(samples)

        self.assertAlmostEqual(series.linear_speed[-1], 0.5, places=6)
        self.assertAlmostEqual(series.angular_speed[-1], 0.5, places=6)
        self.assertAlmostEqual(series.linear_acceleration[-1], 1.0, places=6)
        self.assertAlmostEqual(series.angular_acceleration[-1], 1.0, places=6)

    def test_marks_authored_contact_and_follow_up_pose_boundaries(self):
        markers = MODULE.pose_markers(
            [
                MODULE.MotionSample(
                    item["scenario_frame"],
                    item["elapsed_seconds"],
                    (0.0, 0.0, 0.0),
                    (0.0, 0.0, 0.0, 1.0),
                    item["evaluation"]["action"][0],
                )
                for item in captured_frames()
            ]
        )
        labels = [marker.label for marker in markers]
        self.assertEqual(
            labels,
            [
                "guard_swing",
                "attack_swing",
                "full backswing (extrapolated)",
                "recover_swing",
                "continue_swing",
                "guard_swing",
            ],
        )
        self.assertGreater(markers[1].time, 0.1)
        self.assertLess(markers[1].time, 0.2)

    def test_marks_internal_boundaries_of_a_unified_continuation_span(self):
        payload = {
            "end": "recover_swing",
            "outgoing": "continue_swing",
            "finish": "guard_swing",
            "ready_phase": 7.0 / 24.0,
        }
        samples = [
            MODULE.MotionSample(
                index,
                progress,
                (0.0, 0.0, 0.0),
                (0.0, 0.0, 0.0, 1.0),
                {
                    "pose": "guard_swing",
                    "sampling": {"ContinuationSpan": payload | {"progress": progress}},
                },
            )
            for index, progress in enumerate((0.0, 0.2, 0.4, 0.6, 0.9), 1)
        ]

        markers = MODULE.pose_markers(samples)

        self.assertEqual(
            [marker.label for marker in markers],
            ["guard_swing", "recover_swing", "continue_swing", "guard_swing"],
        )
        self.assertAlmostEqual(markers[1].time, 7.0 / 24.0)
        self.assertAlmostEqual(markers[2].time, 0.5)

    def test_renders_four_aligned_plots_with_top_markers_and_bottom_time_axis(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "motion.svg"
            samples = [
                MODULE.MotionSample(
                    item["scenario_frame"],
                    item["elapsed_seconds"],
                    (item["bones"][0]["translation"][0] - 10.0, 0.0, 0.0),
                    tuple(item["bones"][0]["rotation_xyzw"]),
                    item["evaluation"]["action"][0],
                )
                for item in captured_frames()
            ]
            series = MODULE.calculate_motion(samples)
            MODULE.render_svg(output, "trace.jsonl", "r_weapon", series, MODULE.pose_markers(samples))
            root = ElementTree.parse(output).getroot()

        text = " ".join(node.text or "" for node in root.iter() if node.tag.endswith("text"))
        self.assertTrue(root[0].tag.endswith("rect"))
        self.assertIn("r_weapon attack-chain motion", text)
        self.assertIn("Linear speed", text)
        self.assertIn("Angular speed", text)
        self.assertIn("Linear acceleration", text)
        self.assertIn("Angular acceleration", text)
        self.assertIn("full backswing (extrapolated)", text)
        self.assertIn("Time since attack-chain start (s)", text)
        self.assertEqual(len([node for node in root.iter() if node.tag.endswith("polyline")]), 4)


if __name__ == "__main__":
    unittest.main()
