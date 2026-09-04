import assert from "node:assert/strict";
import test from "node:test";

import { parseArguments } from "../cli.mjs";
import { automaticGripPoint, buildSkinnedWeaponGlb, encodeGlb, parseGlb } from "../src/glb-export.js";
import { buildWeapon } from "../src/mesh.js";
import { PRESETS } from "../src/presets.js";

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
    indices: [0, 1, 2],
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

test("GLB preserves shared vertex indices and smooth normals at every LOD", () => {
  for (const lod of ["low", "medium", "high"]) {
    const mesh = buildWeapon(PRESETS.find((preset) => preset.id === "buckler").definition, { lod });
    const parsed = parseGlb(buildSkinnedWeaponGlb(baseRig(), mesh, { attachment: "r_weapon" }));
    const primitive = parsed.document.meshes[0].primitives[0];
    assert.deepEqual(accessorValues(parsed, primitive.indices), mesh.indices);
    assert.equal(parsed.document.accessors[primitive.attributes.POSITION].count, mesh.positions.length / 3);
    assert.ok(mesh.indices.length > mesh.positions.length / 3 * 2);
    const normals = accessorValues(parsed, primitive.attributes.NORMAL);
    for (let i = 0; i < normals.length; i++) assert.ok(Math.abs(normals[i] - mesh.normals[i]) < 1e-6);
  }
});

test("bow export preserves separately animatable string nodes without changing melee export", () => {
  const source = PRESETS.find((preset) => preset.id === "german-self-bow-1544"),
    mesh = buildWeapon(source.definition),
    parsed = parseGlb(buildSkinnedWeaponGlb(baseRig(), mesh, { attachment: "r_weapon", name: "test-bow" })),
    document = parsed.document,
    semantic = ["upper bowstring control span", "lower bowstring control span", "upper bowstring end loop", "lower bowstring end loop", "served nocking control span"];
  assert.equal(document.extras.adventuresim_weapon.animation_contract, "bow-string-nodes-v1");
  assert.deepEqual(document.extras.adventuresim_weapon.semantic_nodes, semantic);
  const attachment = document.nodes.find((node) => node.name === "r_weapon");
  for (const label of semantic) {
    const index = document.nodes.findIndex((node) => node.name === label), node = document.nodes[index];
    assert.ok(index >= 0 && attachment.children.includes(index), label);
    assert.equal(node.skin, undefined);
    assert.equal(node.extras.adventuresim_animation_role, label);
    assert.equal(document.meshes[node.mesh].primitives[0].attributes.JOINTS_0, undefined);
  }
  const body = document.nodes.find((node) => node.name === "test-bow");
  assert.equal(body.skin, 0);
  assert.ok(document.accessors[document.meshes[body.mesh].primitives[0].attributes.POSITION].count < mesh.positions.length / 3);

  const melee = parseGlb(buildSkinnedWeaponGlb(baseRig(), buildWeapon(PRESETS.find((preset) => preset.id === "landsknecht-longsword").definition), { name: "test-sword" })).document;
  assert.equal(melee.meshes.length, 1);
  assert.equal(melee.extras.adventuresim_weapon.animation_contract, undefined);
  assert.deepEqual(melee.extras.adventuresim_weapon.semantic_nodes, []);
});

test("crossbow export preserves separately animatable served spans and seated tip loops", () => {
  const source = PRESETS.find((preset) => preset.id === "german-cranequin-crossbow-1544"), mesh = buildWeapon(source.definition),
    parsed = parseGlb(buildSkinnedWeaponGlb(baseRig(), mesh, { attachment: "r_weapon", name: "test-crossbow" })), document = parsed.document,
    semantic = ["left crossbow string control span", "right crossbow string control span", "served crossbow nocking span", "left crossbow string end loop", "right crossbow string end loop"];
  assert.equal(document.extras.adventuresim_weapon.animation_contract, "crossbow-string-nodes-v1");
  assert.deepEqual(document.extras.adventuresim_weapon.semantic_nodes, semantic);
  const attachment = document.nodes.find((node) => node.name === "r_weapon");
  for (const label of semantic) {
    const index = document.nodes.findIndex((node) => node.name === label), node = document.nodes[index];
    assert.ok(index >= 0 && attachment.children.includes(index), label);
    assert.equal(node.skin, undefined);
    assert.equal(node.extras.adventuresim_animation_role, label);
    assert.equal(document.meshes[node.mesh].primitives[0].attributes.JOINTS_0, undefined);
  }
  const body = document.nodes.find((node) => node.name === "test-crossbow");
  assert.equal(body.skin, 0);
  assert.ok(document.accessors[document.meshes[body.mesh].primitives[0].attributes.POSITION].count < mesh.positions.length / 3);
});

test("firearm export preserves separately animatable lock nodes without changing the skinned stock", () => {
  for (const id of ["peter-peck-double-wheellock-pistol-1545", "german-matchlock-arquebus-16c"]) {
    const source = PRESETS.find((preset) => preset.id === id), mesh = buildWeapon(source.definition), semanticParts = mesh.parts.filter((part) => part.animationPivot), semantic = semanticParts.map((part) => part.label),
      parsed = parseGlb(buildSkinnedWeaponGlb(baseRig(), mesh, { attachment: "r_weapon", name: id })), document = parsed.document;
    assert.equal(document.extras.adventuresim_weapon.animation_contract, "firearm-lock-nodes-v2", id);
    assert.deepEqual(document.extras.adventuresim_weapon.semantic_nodes, semantic, id);
    const attachment = document.nodes.find((node) => node.name === "r_weapon");
    for (const [partIndex, label] of semantic.entries()) {
      const index = document.nodes.findIndex((node) => node.name === label), node = document.nodes[index];
      assert.ok(index >= 0 && attachment.children.includes(index), `${id}: ${label}`);
      assert.equal(node.skin, undefined, label);
      assert.equal(node.extras.adventuresim_animation_role, label);
      assert.equal(document.meshes[node.mesh].primitives[0].attributes.JOINTS_0, undefined, label);
      const part = semanticParts[partIndex], local = accessorValues(parsed, document.meshes[node.mesh].primitives[0].attributes.POSITION).slice(0, 3);
      assert.deepEqual(node.extras.adventuresim_local_pivot, [0, 0, 0]);
      node.translation.forEach((value, axis) => assert.ok(Math.abs(value - part.animationPivot[axis]) < 1e-7, `${label}: node translation owns world pivot`));
      local.forEach((value, axis) => assert.ok(Math.abs(value + node.translation[axis] - part.positions[axis]) < 1e-6, `${label}: vertices are recentered on pivot`));
      const quarterTurn = [-local[1], local[0], local[2]], rotatedWorld = quarterTurn.map((value, axis) => value + node.translation[axis]);
      assert.ok(Math.abs(Math.hypot(...rotatedWorld.map((value, axis) => value - node.translation[axis])) - Math.hypot(...local)) < 1e-6, `${label}: rotation occurs about explicit local origin`);
    }
    const body = document.nodes.find((node) => node.name === id);
    assert.equal(body.skin, 0, id);
    assert.ok(document.accessors[document.meshes[body.mesh].primitives[0].attributes.POSITION].count < mesh.positions.length / 3, id);
  }
});

test("ball pouch flap exports at its hinge-local pivot", () => {
  const source = PRESETS.find((preset) => preset.id === "small-arms-ball-pouch"), mesh = buildWeapon(source.definition), part = mesh.parts.find((candidate) => candidate.label === "ball pouch hinged flap"),
    parsed = parseGlb(buildSkinnedWeaponGlb(baseRig(), mesh, { attachment: "r_weapon", name: source.id })), document = parsed.document, node = document.nodes.find((candidate) => candidate.name === part.label);
  assert.equal(document.extras.adventuresim_weapon.animation_contract, "pouch-flap-node-v1"); assert.deepEqual(document.extras.adventuresim_weapon.semantic_nodes, [part.label]);
  assert.deepEqual(node.translation, part.animationPivot); assert.deepEqual(node.extras.adventuresim_local_pivot, [0, 0, 0]);
  const local = accessorValues(parsed, document.meshes[node.mesh].primitives[0].attributes.POSITION).slice(0, 3);
  local.forEach((value, axis) => assert.ok(Math.abs(value + node.translation[axis] - part.positions[axis]) < 1e-6));
});

test("chooses the modeled grip center and a bounded polearm handhold", () => {
  assert.deepEqual(automaticGripPoint({ _frames: { "shield.grip": [0.12, -0.04, -0.08] } }), [0.12, -0.04, -0.08]);
  const configured = automaticGripPoint({ gripClearance: 0.05, _frames: { "grip.base": [0, 0.1, 0], "grip.top": [0, 0.3, 0] } });
  assert.ok(Math.abs(configured[1] - 0.25) < 1e-9);
  assert.deepEqual(automaticGripPoint({ _frames: { "grip.center": [0, 0.22, 0] } }), [0, 0.22, 0]);
  assert.deepEqual(automaticGripPoint({ _frames: { "shaft.bottom": [0, 0, 0], "shaft.top": [0, 4, 0] } }), [0, 0.45, 0]);
});

test("CLI accepts a skinned output parameter and validates its attachment", () => {
  assert.deepEqual(parseArguments(["--preset", "landsknecht-longsword", "--skinned", "assets_src/weapons/longsword.glb"]), {
    preset: "landsknecht-longsword", skinned: "assets_src/weapons/longsword.glb",
  });
  assert.throws(() => parseArguments(["--preset", "landsknecht-longsword", "--skinned", "longsword.obj"]), /must end in \.glb/);
  assert.throws(() => parseArguments(["--preset", "landsknecht-longsword", "--skinned", "longsword.glb", "--joint", "weapon.R"]), /r_weapon or l_weapon/);
});
