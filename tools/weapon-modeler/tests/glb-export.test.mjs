import assert from "node:assert/strict";
import test from "node:test";

import { parseArguments } from "../cli.mjs";
import { automaticGripPoint, buildSkinnedWeaponGlb, encodeGlb, parseGlb } from "../src/glb-export.js";

function baseRig() {
  return encodeGlb({
    asset: { version: "2.0" }, scene: 0,
    scenes: [{ nodes: [0, 3] }],
    nodes: [
      { name: "Skeleton", children: [1] },
      { name: "body_world", children: [2] },
      { name: "r_weapon", translation: [1, 2, 3] },
      { name: "body", mesh: 0, skin: 0 },
    ],
    meshes: [{ primitives: [] }], materials: [],
    skins: [{ name: "MHR", skeleton: 1, joints: [1, 2] }],
    accessors: [], bufferViews: [], buffers: [{ byteLength: 0 }],
  }, new Uint8Array());
}

function triangle() {
  return {
    positions: [0, 0.5, 0, 1, 0.5, 0, 0, 1.5, 0],
    normals: [0, 0, 1, 0, 0, 1, 0, 0, 1],
    colors: [0.5, 0.6, 0.7, 0.5, 0.6, 0.7, 0.5, 0.6, 0.7],
  };
}

function accessorValues(parsed, accessorIndex) {
  const accessor = parsed.document.accessors[accessorIndex];
  const view = parsed.document.bufferViews[accessor.bufferView];
  const components = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4 }[accessor.type];
  const begin = (view.byteOffset ?? 0) + (accessor.byteOffset ?? 0);
  const length = accessor.count * components;
  if (accessor.componentType === 5_126) return [...new Float32Array(parsed.binary.buffer, parsed.binary.byteOffset + begin, length)];
  return [...new Uint16Array(parsed.binary.buffer, parsed.binary.byteOffset + begin, length)];
}

test("exports the weapon as a skin bound entirely to the selected character joint", () => {
  const output = buildSkinnedWeaponGlb(baseRig(), triangle(), { name: "test-sword", attachment: "r_weapon", gripPoint: [0, 0.5, 0] });
  const parsed = parseGlb(output), document = parsed.document;
  assert.equal(document.skins.length, 1);
  assert.equal(document.meshes.length, 1);
  const weaponNode = document.nodes.find((node) => node.name === "test-sword");
  assert.equal(weaponNode.skin, 0);
  const primitive = document.meshes[0].primitives[0];
  const attributes = primitive.attributes;
  assert.deepEqual(accessorValues(parsed, attributes.JOINTS_0), [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]);
  assert.deepEqual(accessorValues(parsed, attributes.WEIGHTS_0), [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]);
  assert.deepEqual(accessorValues(parsed, attributes.POSITION).slice(0, 3), [1, 2, 3]);
  assert.deepEqual(accessorValues(parsed, primitive.indices), [0, 1, 2]);
  assert.equal(document.bufferViews[document.accessors[primitive.indices].bufferView].target, 34_963);
  assert.equal(document.extras.adventuresim_weapon.skinned, true);
  assert.deepEqual(document.animations, []);
});

test("rejects a rig that does not contain the requested attachment", () => {
  assert.throws(() => buildSkinnedWeaponGlb(baseRig(), triangle(), { attachment: "l_weapon" }), /missing l_weapon/);
});

test("chooses the modeled grip center and a bounded polearm handhold", () => {
  const configured = automaticGripPoint({ gripClearance: 0.05, _frames: { "grip.base": [0, 0.1, 0], "grip.top": [0, 0.3, 0] } });
  assert.ok(Math.abs(configured[1] - 0.25) < 1e-9);
  assert.deepEqual(automaticGripPoint({ _frames: { "grip.center": [0, 0.22, 0] } }), [0, 0.22, 0]);
  assert.deepEqual(automaticGripPoint({ _frames: { "shaft.bottom": [0, 0, 0], "shaft.top": [0, 4, 0] } }), [0, 0.45, 0]);
});

test("CLI accepts a skinned output parameter and validates its attachment", () => {
  assert.deepEqual(parseArguments(["--preset", "landsknecht-longsword", "--skinned", "assets_src/weapons/longsword.glb"]), {
    preset: "landsknecht-longsword", skinned: "assets_src/weapons/longsword.glb", joint: "r_weapon",
  });
  assert.throws(() => parseArguments(["--preset", "landsknecht-longsword", "--skinned", "longsword.obj"]), /must end in \.glb/);
  assert.throws(() => parseArguments(["--preset", "landsknecht-longsword", "--skinned", "longsword.glb", "--joint", "weapon.R"]), /r_weapon or l_weapon/);
});
