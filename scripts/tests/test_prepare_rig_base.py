import copy
import importlib.util
import pathlib
import unittest


PATH = pathlib.Path(__file__).parents[1] / "prepare_rig_base.py"
SPEC = importlib.util.spec_from_file_location("prepare_rig_base", PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(MODULE)


class PrepareRigBaseTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source, cls.binary = MODULE.read_glb(pathlib.Path("assets_src/biped/unarmed/base.glb"))

    def test_source_contract_is_mhr(self):
        document = MODULE.validate_and_prepare(copy.deepcopy(self.source))
        roots = document["scenes"][document["scene"]]["nodes"]
        root_names = {document["nodes"][index]["name"] for index in roots}
        self.assertIn("John Fabelgeist", root_names)
        self.assertIn("Skeleton", root_names)
        joint_names = {document["nodes"][i]["name"] for i in document["skins"][0]["joints"]}
        self.assertEqual(len(joint_names), 130)
        self.assertIn("body_world", joint_names)
        self.assertIn("l_wrist", joint_names)
        self.assertIn("r_wrist", joint_names)
        self.assertIn("l_weapon", joint_names)
        self.assertIn("r_weapon", joint_names)
        self.assertIn("c_camera", joint_names)
        self.assertEqual(document.get("animations", []), [])

    def test_duplicate_joint_is_rejected(self):
        document = copy.deepcopy(self.source)
        document["skins"][0]["joints"][1] = document["skins"][0]["joints"][0]
        with self.assertRaises(MODULE.GlbError):
            MODULE.validate_and_prepare(document)

    def test_reparented_runtime_joint_is_rejected(self):
        document = copy.deepcopy(self.source)
        indices = {node.get("name"): index for index, node in enumerate(document["nodes"])}
        foot = indices["l_foot"]
        document["nodes"][indices["l_lowleg"]]["children"].remove(foot)
        document["nodes"][indices["l_upleg"]].setdefault("children", []).append(foot)
        with self.assertRaisesRegex(MODULE.GlbError, "expected l_foot parent l_lowleg"):
            MODULE.validate_and_prepare(document)

    def test_malformed_json_shapes_are_glb_errors(self):
        for document in [
            [],
            {**copy.deepcopy(self.source), "nodes": [[]]},
            {**copy.deepcopy(self.source), "skins": [None]},
            {**copy.deepcopy(self.source), "scenes": [None]},
            {**copy.deepcopy(self.source), "extras": []},
            {**copy.deepcopy(self.source), "skins": [{"joints": [[]] * 130}]},
        ]:
            with self.assertRaises(MODULE.GlbError):
                MODULE.validate_and_prepare(document)

    def test_encoding_is_deterministic(self):
        first = MODULE.encode_glb(MODULE.validate_and_prepare(copy.deepcopy(self.source)), self.binary)
        second = MODULE.encode_glb(MODULE.validate_and_prepare(copy.deepcopy(self.source)), self.binary)
        self.assertEqual(first, second)


if __name__ == "__main__":
    unittest.main()
