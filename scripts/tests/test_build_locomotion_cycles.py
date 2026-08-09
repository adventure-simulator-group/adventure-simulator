from __future__ import annotations

import importlib.util
import pathlib
import sys
import tempfile
import unittest

import numpy as np


ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location(
    "build_locomotion_cycles", ROOT / "scripts" / "build_locomotion_cycles.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class LocomotionCycleTests(unittest.TestCase):
    def test_committed_runtime_cycles_match_authored_semantic_poses(self) -> None:
        for motion, passing_frame in MODULE.MOTIONS.items():
            generated = MODULE.build_cycle(
                MODULE.SOURCE_DIR / f"{motion}.glb", passing_frame
            )
            committed = MODULE.ASSET_DIR / f"{motion}.glb"
            self.assertEqual(committed.read_bytes(), generated, motion)

    def test_runtime_quarter_cycle_preserves_the_exact_authored_pose(self) -> None:
        for motion, passing_frame in MODULE.MOTIONS.items():
            source_document, source_binary = MODULE.read_glb(
                MODULE.SOURCE_DIR / f"{motion}.glb"
            )
            source_values = MODULE.channel_values(source_document, source_binary)
            generated = MODULE.build_cycle(
                MODULE.SOURCE_DIR / f"{motion}.glb", passing_frame
            )
            with tempfile.TemporaryDirectory() as temporary:
                generated_document, generated_binary = MODULE.decode_glb_bytes(
                    generated, pathlib.Path(temporary) / f"{motion}.glb"
                )
            generated_values = MODULE.channel_values(
                generated_document, generated_binary
            )
            for key in source_values:
                np.testing.assert_allclose(
                    generated_values[key][16],
                    source_values[key][passing_frame],
                    atol=1e-6,
                    err_msg=f"{motion} channel {key}",
                )


if __name__ == "__main__":
    unittest.main()
