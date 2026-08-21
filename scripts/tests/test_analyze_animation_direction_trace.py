import importlib.util
import json
import math
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "analyze_animation_direction_trace.py"
SPEC = importlib.util.spec_from_file_location("analyze_animation_direction_trace", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def frame(index, direction, elapsed, angle):
    movement = {
        "forward": [0.0, 0.5],
        "backward": [0.0, -0.5],
        "left": [-0.5, 0.0],
        "right": [0.5, 0.0],
    }[direction]
    half = angle / 2.0
    half_step_seconds = 0.75
    contact_sequence = int(elapsed / half_step_seconds)
    half_step_progress = (elapsed % half_step_seconds) / half_step_seconds
    swing_height = 0.04 * math.sin(math.pi * half_step_progress) ** 2
    left_swing = contact_sequence % 2 == 0
    root_x = index / 10.0
    return {
        "scenario_frame": index,
        "elapsed_seconds": index / 64.0,
        "render_delta_seconds": 1.0 / 64.0,
        "authoritative": {"locomotion_sample_tick": index},
        "presented": {
            "contact_sequence": contact_sequence,
            "contact_foot": "Left" if contact_sequence % 2 == 0 else "Right",
        },
        "evaluation": {"gait_phase": (elapsed / half_step_seconds / 2.0) % 1.0},
        "subject_rotation_xyzw": [0.0, 0.0, 0.0, 1.0],
        "input": {
            "command_index": list(MODULE.DIRECTION_VECTORS).index(direction),
            "command_kind": "move",
            "command_elapsed_seconds": elapsed,
            "request": {"movement": movement, "weapon_guard": "Raised"},
        },
        "bones": [
            {
                "name": "pelvis",
                "translation": [root_x, 1.0, 0.0],
                "rotation_xyzw": [0.0, 0.0, 0.0, 1.0],
            },
            {
                "name": "chest",
                "translation": [root_x, 1.5, 0.0],
                "rotation_xyzw": [math.sin(half), 0.0, 0.0, math.cos(half)],
            },
            {
                "name": "head",
                "translation": [root_x, 2.0, 0.0],
                "rotation_xyzw": [math.sin(half), 0.0, 0.0, math.cos(half)],
            },
            {
                "name": "thigh.L",
                "translation": [root_x - 0.2, 1.0, 0.0],
                "rotation_xyzw": [0.0, 0.0, 0.0, 1.0],
            },
            {
                "name": "shin.L",
                "translation": [root_x - 0.2, 0.5, 0.0],
                "rotation_xyzw": [0.0, 0.0, 0.0, 1.0],
            },
            {
                "name": "foot.L",
                "translation": [root_x - 0.2, swing_height if left_swing else 0.0, 0.0],
                "rotation_xyzw": [0.0, 0.0, 0.0, 1.0],
            },
            {
                "name": "thigh.R",
                "translation": [root_x + 0.2, 1.0, 0.0],
                "rotation_xyzw": [0.0, 0.0, 0.0, 1.0],
            },
            {
                "name": "shin.R",
                "translation": [root_x + 0.2, 0.5, 0.0],
                "rotation_xyzw": [0.0, 0.0, 0.0, 1.0],
            },
            {
                "name": "foot.R",
                "translation": [root_x + 0.2, 0.0 if left_swing else swing_height, 0.0],
                "rotation_xyzw": [0.0, 0.0, 0.0, 1.0],
            },
        ],
    }


class AnalyzeAnimationDirectionTraceTests(unittest.TestCase):
    def test_rough_direction_scores_lower_and_root_translation_is_removed(self):
        frames = []
        index = 0
        for direction in MODULE.DIRECTION_VECTORS:
            for sample in range(80):
                elapsed = sample / 32.0
                angle = 0.002 * sample
                if direction == "backward" and sample % 4 == 0:
                    angle += 0.25
                frames.append(frame(index, direction, elapsed, angle))
                index += 1
        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / "trace.jsonl"
            trace.write_text(
                "".join(json.dumps(item) + "\n" for item in frames), encoding="utf-8"
            )
            result = MODULE.analyze_trace(trace, 0.25, 0.25)

        self.assertEqual(result["ranking_smoothest_to_roughest"][-1], "backward")
        self.assertLess(
            result["directions"]["backward"]["smoothness_score"],
            result["directions"]["forward"]["smoothness_score"],
        )
        self.assertLess(
            result["directions"]["forward"]["metrics"]["local_position_jerk"][
                "peak_threshold_ratio"
            ],
            0.25,
        )
        self.assertTrue(result["benchmark_valid"])

    def test_render_stalls_and_source_tick_gaps_invalidate_the_benchmark(self):
        frames = []
        index = 0
        for direction in MODULE.DIRECTION_VECTORS:
            for sample in range(80):
                item = frame(index, direction, sample / 32.0, 0.002 * sample)
                if direction == "left" and sample == 40:
                    item["render_delta_seconds"] = 0.2
                    item["authoritative"]["locomotion_sample_tick"] += 12
                frames.append(item)
                index += 1
        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / "trace.jsonl"
            trace.write_text(
                "".join(json.dumps(item) + "\n" for item in frames), encoding="utf-8"
            )
            result = MODULE.analyze_trace(trace, 0.25, 0.25)

        self.assertFalse(result["benchmark_valid"])
        timing = result["directions"]["left"]["timing"]
        self.assertEqual(timing["render_stall_count"], 1)
        self.assertEqual(timing["source_tick_gap_count"], 1)

    def test_sustained_sideways_foot_displacement_forces_score_to_zero(self):
        frames = []
        index = 0
        for direction in MODULE.DIRECTION_VECTORS:
            for sample in range(80):
                item = frame(index, direction, sample / 32.0, 0.002 * sample)
                root_x = index / 10.0
                foot_x = root_x - (0.9 if direction == "backward" else 0.2)
                item["bones"].extend(
                    [
                        {
                            "name": "thigh.L",
                            "translation": [root_x - 0.2, 1.0, 0.0],
                            "rotation_xyzw": [0.0, 0.0, 0.0, 1.0],
                        },
                        {
                            "name": "shin.L",
                            "translation": [(root_x + foot_x) / 2.0, 0.5, 0.0],
                            "rotation_xyzw": [0.0, 0.0, 0.0, 1.0],
                        },
                        {
                            "name": "foot.L",
                            "translation": [foot_x, 0.0, 0.0],
                            "rotation_xyzw": [0.0, 0.0, 0.0, 1.0],
                        },
                    ]
                )
                frames.append(item)
                index += 1
        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / "trace.jsonl"
            trace.write_text(
                "".join(json.dumps(item) + "\n" for item in frames), encoding="utf-8"
            )
            result = MODULE.analyze_trace(trace, 0.25, 0.25)

        backward = result["directions"]["backward"]
        self.assertTrue(backward["catastrophic_stance"]["failed"])
        self.assertEqual(backward["quality_score"], 0.0)
        self.assertGreater(backward["motion_smoothness_score"], 0.0)
        self.assertTrue(result["guard_catastrophic_stance"]["failed"])
        self.assertEqual(result["benchmark_quality_score"], 0.0)

    def test_advancing_cadence_with_floor_sliding_feet_forces_score_to_zero(self):
        frames = []
        index = 0
        for direction in MODULE.DIRECTION_VECTORS:
            for sample in range(80):
                item = frame(index, direction, sample / 32.0, 0.002 * sample)
                if direction == "forward":
                    for bone in item["bones"]:
                        if bone["name"] in {"foot.L", "foot.R"}:
                            bone["translation"][1] = 0.0
                frames.append(item)
                index += 1
        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / "trace.jsonl"
            trace.write_text(
                "".join(json.dumps(item) + "\n" for item in frames), encoding="utf-8"
            )
            result = MODULE.analyze_trace(trace, 0.25, 0.25)

        forward = result["directions"]["forward"]
        self.assertFalse(forward["catastrophic_stance"]["failed"])
        self.assertFalse(forward["guard_step_liveness"]["passed"])
        self.assertGreater(
            forward["guard_step_liveness"]["completed_half_step_count"], 0
        )
        self.assertEqual(
            forward["guard_step_liveness"]["visible_half_step_count"], 0
        )
        self.assertEqual(forward["quality_score"], 0.0)


if __name__ == "__main__":
    unittest.main()
