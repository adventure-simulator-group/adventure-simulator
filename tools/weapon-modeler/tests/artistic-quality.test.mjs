import assert from "node:assert/strict";
import test from "node:test";
import { buildWeapon, lathe, shieldHandAperture, signedVolume, validateWeapon } from "../src/mesh.js";
import { PRESETS, copyPreset, setControlValue } from "../src/presets.js";
import { triangleVertices } from "../src/topology.js";
import { adversarialReviewCases, reviewCases } from "../src/review-cases.js";

test("round surfaces share radial normals and indices while caps and octagonal flats remain split", () => {
  const mesh = lathe([[0, 0.02], [0.2, 0.02]], 24);
  assert.ok(mesh.positions.length / 3 < mesh.indices.length / 2);
  let sideCount = 0, capCount = 0;
  for (let i = 0; i < mesh.positions.length; i += 3) {
    const [x, , z] = mesh.positions.slice(i, i + 3), normal = mesh.normals.slice(i, i + 3);
    if (Math.abs(normal[1]) > 0.99) { capCount++; continue; }
    assert.ok(Math.abs(normal[1]) < 1e-6);
    assert.ok(normal[0] * x / 0.02 + normal[2] * z / 0.02 > 0.999);
    sideCount++;
  }
  assert.ok(sideCount > 0 && capCount > 0);
  const octagon = lathe([[0, 0.02], [0.2, 0.02]], 8, "wood", [0, 0, 0], "octagon", 1, true);
  const cornerNormals = [];
  for (let i = 0; i < octagon.positions.length; i += 3) if (Math.abs(octagon.positions[i] - 0.02) < 1e-8 && Math.abs(octagon.positions[i + 1]) < 1e-8 && Math.abs(octagon.normals[i + 1]) < 0.01) cornerNormals.push(octagon.normals.slice(i, i + 3));
  assert.equal(cornerNormals.length, 2);
  assert.ok(cornerNormals[0].reduce((sum, value, axis) => sum + value * cornerNormals[1][axis], 0) < 0.8);
});

test("all preset LODs keep bounds, enclosed volume, and attachment construction", () => {
  for (const preset of PRESETS) {
    const levels = ["low", "medium", "high"].map((lod) => {
      const result = validateWeapon(preset.definition, preset.controls, { lod });
      assert.equal(result.valid, true, `${preset.id}/${lod}: ${result.errors.join(" | ")}`);
      return result.mesh;
    });
    assert.ok(levels[0].stats.triangles < levels[1].stats.triangles && levels[1].stats.triangles < levels[2].stats.triangles, preset.id);
    for (const mesh of levels.slice(0, 2)) {
      for (let axis = 0; axis < 3; axis++) assert.ok(Math.abs(mesh.stats.dimensions[axis] - levels[2].stats.dimensions[axis]) < 0.006, `${preset.id} extent ${axis}`);
      assert.ok(Math.abs(mesh.stats.volume / levels[2].stats.volume - 1) < 0.08, `${preset.id} volume drift`);
      assert.deepEqual(mesh.resolvedDefinition._frames, levels[2].resolvedDefinition._frames);
    }
  }
});

test("adverse joints and silhouettes survive every LOD", () => {
  for (const specimen of adversarialReviewCases()) for (const lod of ["low", "medium", "high"]) {
    const result = validateWeapon(specimen.definition, [], { lod });
    assert.equal(result.valid, true, `${specimen.id}/${lod}: ${result.errors.join(" | ")}`);
  }
});

test("a narrow pommel receives a tapered grip seat without an overhanging bottom cap", () => {
  const definition = adversarialReviewCases()[0].definition, mesh = buildWeapon(definition);
  const pommel = mesh.resolvedDefinition.components.find((part) => part.id === "pommel");
  const grip = mesh.parts.find((part) => part.componentId === "grip");
  const y = Math.min(...grip.positions.filter((_, index) => index % 3 === 1));
  const radius = pommel.profile.at(-1)[1];
  for (let i = 0; i < grip.positions.length; i += 3) if (Math.abs(grip.positions[i + 1] - y) < 1e-8) assert.ok(Math.hypot(grip.positions[i], grip.positions[i + 2]) <= radius + 1e-8);
});

test("mirroring a halberd keeps its rear fluke opposite the axe and below its root", () => {
  const preset = copyPreset(PRESETS.find((p) => p.id === "halberd-1540"));
  for (const side of [-1, 1]) {
    preset.definition.components[1].side = side;
    const mesh = buildWeapon(preset.definition), beak = mesh.resolvedDefinition.components.find((part) => part.kind === "beak");
    assert.equal(beak.direction, -side);
    const part = mesh.parts.find((part) => part.componentId === beak.id);
    const tip = [];
    for (let i = 0; i < part.positions.length; i += 3) if (Math.abs(part.positions[i] - (beak.offset[0] + beak.direction * beak.length)) < 1e-8) tip.push(part.positions[i + 1]);
    assert.ok(tip.length && Math.max(...tip) < beak.offset[1]);
  }
});

test("center-gripped bucklers open into their hollow boss and strapped shields keep their body", () => {
  const preset = copyPreset(PRESETS.find((p) => p.id === "buckler"));
  const mesh = buildWeapon(preset.definition), aperture = shieldHandAperture(preset.definition.components[0]);
  assert.ok(aperture > 0.045);
  const body = mesh.parts.find((part) => part.shieldRole === "body");
  for (const triangle of triangleVertices(body)) {
    const signs = triangle.map((a, i) => { const b = triangle[(i + 1) % 3]; return a[0] * b[1] - a[1] * b[0]; });
    assert.ok(!(signs.every((s) => s > 1e-10) || signs.every((s) => s < -1e-10)), "body triangle closes hand aperture");
  }
  const boss = mesh.parts.find((part) => part.shieldRole === "boss");
  assert.ok(boss.positions.length / 3 < boss.indices.length / 2);
  assert.ok(boss.normals.some((value, i) => i % 3 === 2 && value < -0.9), "boss has inward-facing cavity skin");
  const { bossRadius, bossHeight } = preset.definition.components[0];
  assert.ok(signedVolume(boss) < 0.15 * (2 / 3 * Math.PI * bossRadius ** 2 * bossHeight), "boss must be a thin shell rather than a solid hemisphere");
  preset.definition.components[0].fittingMode = "grip-and-strap";
  assert.equal(shieldHandAperture(preset.definition.components[0]), 0);
});

test("authored furniture choices remain valid with small pommels and all detail levels", () => {
  for (const source of PRESETS.filter((preset) => preset.choiceControls?.some((choice) => choice.label === "Pommel form"))) {
    const choice = source.choiceControls.find((control) => control.label === "Pommel form");
    for (const option of choice.options) for (const lod of ["low", "high"]) {
      const preset = copyPreset(source); setControlValue(preset.definition, choice, structuredClone(option.value));
      preset.definition.components.find((part) => part.id === "pommel").widthScale = 0.65;
      const result = validateWeapon(preset.definition, preset.controls, { lod });
      assert.equal(result.valid, true, `${source.id}/${option.label}/${lod}: ${result.errors.join(" | ")}`);
    }
  }
});

test("random review specimens are reproducible regardless of gallery selection order", () => {
  const one = reviewCases(1544, ["buckler"]), two = reviewCases(1544, ["halberd-1540", "buckler"]).slice(2);
  assert.deepEqual(one, two);
  assert.ok(one[1].changes.length > 0);
  assert.notDeepEqual(one[1].definition, reviewCases(1545, ["buckler"])[1].definition);
});
