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
        cls.source, cls.binary = MODULE.read_glb(pathlib.Path("assets_src/base.glb"))

    def test_source_contract_and_placeholder_removal(self):
        document = MODULE.validate_and_prepare(copy.deepcopy(self.source))
        roots = document["scenes"][document["scene"]]["nodes"]
        root_names = {document["nodes"][index]["name"] for index in roots}
        self.assertIn("mesh_node", root_names)
        self.assertIn("Skeleton", root_names)
        self.assertNotIn("weapon", root_names)
        joint_names = {document["nodes"][i]["name"] for i in document["skins"][0]["joints"]}
        self.assertEqual(len(joint_names), 74)
        self.assertIn("weapon.L", joint_names)
        self.assertIn("weapon.R", joint_names)
        self.assertEqual(document.get("animations", []), [])

    def test_duplicate_joint_is_rejected(self):
        document = copy.deepcopy(self.source)
        document["skins"][0]["joints"][1] = document["skins"][0]["joints"][0]
        with self.assertRaises(MODULE.GlbError):
            MODULE.validate_and_prepare(document)

    def test_reparented_runtime_joint_is_rejected(self):
        document = copy.deepcopy(self.source)
        indices = {node.get("name"): index for index, node in enumerate(document["nodes"])}
        foot = indices["foot.L"]
        document["nodes"][indices["shin_twist.L"]]["children"].remove(foot)
        document["nodes"][indices["shin.L"]].setdefault("children", []).append(foot)
        with self.assertRaisesRegex(MODULE.GlbError, "expected foot.L parent shin_twist.L"):
            MODULE.validate_and_prepare(document)

    def test_malformed_json_shapes_are_glb_errors(self):
        for document in [
            [],
            {**copy.deepcopy(self.source), "nodes": [[]]},
            {**copy.deepcopy(self.source), "skins": [None]},
            {**copy.deepcopy(self.source), "scenes": [None]},
            {**copy.deepcopy(self.source), "extras": []},
            {**copy.deepcopy(self.source), "skins": [{"joints": [[]] * 74}]},
        ]:
            with self.assertRaises(MODULE.GlbError):
                MODULE.validate_and_prepare(document)

    def test_encoding_is_deterministic(self):
        first = MODULE.encode_glb(MODULE.validate_and_prepare(copy.deepcopy(self.source)), self.binary)
        second = MODULE.encode_glb(MODULE.validate_and_prepare(copy.deepcopy(self.source)), self.binary)
        self.assertEqual(first, second)


if __name__ == "__main__":
    unittest.main()
