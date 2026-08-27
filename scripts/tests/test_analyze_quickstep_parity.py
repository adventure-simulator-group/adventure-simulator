from __future__ import annotations

import ast
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
SCRIPT = ROOT / "scripts" / "analyze_quickstep_parity.py"
SPEC = importlib.util.spec_from_file_location("analyze_quickstep_parity", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class AnalyzeQuickstepParityTests(unittest.TestCase):
    SOURCE = ROOT / "assets_src/biped/unarmed/quickstep_right.glb"
    RUNTIME = ROOT / "assets/animations/biped/unarmed/quickstep_right.glb"

    def authored_progresses(self) -> list[float]:
        source = MODULE.MotionSampler(self.SOURCE)
        start = source.positions(0.0)["root"]
        end = source.positions(1.0)["root"]
        direction = end[[0, 2]] - start[[0, 2]]
        direction /= np.linalg.norm(direction)
        distance = float(np.dot(end[[0, 2]] - start[[0, 2]], direction))
        return [
            float(
                np.dot(
                    source.positions(phase)["root"][[0, 2]] - start[[0, 2]],
                    direction,
                )
                / distance
            )
            for phase in MODULE.KEY_PHASES
        ]

    def write_trace(self, path: Path, progresses: list[float], heights=None) -> None:
        heights = heights or [0.0] * len(progresses)
        frames = []
        for index, (phase, progress, height) in enumerate(
            zip(MODULE.KEY_PHASES, progresses, heights, strict=True)
        ):
            frames.append(
                {
                    "action": "Dodge",
                    "action_phase": phase,
                    "terrain_height": height,
                    "controller_transform": {
                        "translation": [progress, 1.0, 0.0]
                    },
                    "frame": index,
                }
            )
        path.write_text(
            "".join(json.dumps(frame) + "\n" for frame in frames),
            encoding="utf-8",
        )

    def test_authored_controller_curve_reconstructs_the_source_motion(self):
        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / "trace.jsonl"
            self.write_trace(trace, self.authored_progresses())
            result = MODULE.analyze_parity(
                self.SOURCE,
                self.RUNTIME,
                trace,
                maximum_bone_error_metres=0.005,
                maximum_planted_foot_excess_drift_metres=0.005,
            )

        self.assertTrue(result["parity_valid"])
        self.assertLess(result["maximum_bone_error_metres"], 0.005)

    def test_combat_force_profile_matches_the_authored_root_landmarks(self):
        combat = (ROOT / "content/tactical/combat.yaml").read_text(encoding="utf-8")
        line = next(
            line
            for line in combat.splitlines()
            if "quickstep_authored_displacement_profile:" in line
        )
        configured = ast.literal_eval(line.split(":", 1)[1].strip())

        np.testing.assert_allclose(configured, self.authored_progresses(), atol=1.0e-6)

    def test_late_controller_motion_exposes_frame_nine_to_twelve_foot_slide(self):
        progresses = self.authored_progresses()
        progresses[-2] = max(0.0, progresses[-2] - 0.12)
        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / "trace.jsonl"
            self.write_trace(trace, progresses)
            result = MODULE.analyze_parity(
                self.SOURCE,
                self.RUNTIME,
                trace,
                maximum_bone_error_metres=0.08,
                maximum_planted_foot_excess_drift_metres=0.03,
            )

        self.assertFalse(result["planted_foot_valid"])
        planted = result["authored_planted_foot"]
        self.assertGreater(result["late_foot_drift"][planted]["excess_metres"], 0.03)

    def test_non_flat_trace_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / "trace.jsonl"
            self.write_trace(
                trace,
                self.authored_progresses(),
                heights=[0.0, 0.0, 0.02, 0.03, 0.03],
            )
            result = MODULE.analyze_parity(self.SOURCE, self.RUNTIME, trace)

        self.assertFalse(result["flat_terrain_valid"])
        self.assertFalse(result["parity_valid"])


if __name__ == "__main__":
    unittest.main()
