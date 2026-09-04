import assert from "node:assert/strict";
import test from "node:test";
import { arrowQuiverProfile, arrowQuiverStrapPath, bowCompositeLayerLayout, bowTipLoopLayout, closedManifoldErrors, sectionOutline, signedVolume, simplePolygonErrors, validateWeapon } from "../src/mesh.js";
import { PRESETS, copyPreset, setControlValue } from "../src/presets.js";
import { triangleVertices } from "../src/topology.js";

const preset = (id) => PRESETS.find((candidate) => candidate.id === id);

test("bows, arrows, and quivers are independently selectable outputs", () => {
  assert.deepEqual(PRESETS.filter((candidate) => candidate.definition.components.some((component) => component.kind === "archeryBow")).map((candidate) => candidate.id), ["german-self-bow-1544", "composite-recurve-bow-1544"]);
  assert.deepEqual(preset("german-self-bow-1544").definition.components.map((part) => part.kind), ["archeryBow"]);
  assert.deepEqual(preset("flight-arrow-1544").definition.components.map((part) => part.kind), ["arrow"]);
  assert.deepEqual(preset("arrow-quiver-1544").definition.components.map((part) => part.kind), ["arrowQuiver"]);
});

test("one served string retains independent spans and closed tip end-loops without center jewelry", () => {
  for (const id of ["german-self-bow-1544", "composite-recurve-bow-1544"]) {
    const source = preset(id), result = validateWeapon(source.definition, source.controls);
    assert.equal(result.valid, true, `${id}: ${result.errors.join(" | ")}`);
    const labels = result.mesh.parts.map((part) => part.label);
    for (const label of ["upper bowstring control span", "lower bowstring control span", "served nocking control span", "upper bowstring end loop", "lower bowstring end loop"])
      assert.equal(labels.filter((candidate) => candidate === label).length, 1, `${id}: ${label}`);
    assert.equal(labels.some((label) => label.includes("nocking loop")), false, "center toroidal loops were removed");
    for (const label of ["upper bowstring end loop", "lower bowstring end loop"]) {
      const loop = result.mesh.parts.find((part) => part.label === label);
      assert.ok(signedVolume(loop) > 0);
      assert.deepEqual(closedManifoldErrors(loop, label), []);
    }
  }
});

test("self bow uses a deep D-section and composite bow exposes three material layers", () => {
  const self = validateWeapon(preset("german-self-bow-1544").definition, preset("german-self-bow-1544").controls),
    composite = validateWeapon(preset("composite-recurve-bow-1544").definition, preset("composite-recurve-bow-1544").controls);
  assert.equal(self.valid, true, self.errors.join(" | "));
  assert.ok(self.mesh.parts.some((part) => part.label === "upper D-section bow limb"));
  assert.ok(preset("german-self-bow-1544").definition.components[0].limbDepth >= 0.03);
  assert.equal(composite.valid, true, composite.errors.join(" | "));
  for (const layer of ["wood core", "horn belly", "sinew backing"])
    assert.equal(composite.mesh.parts.filter((part) => part.label.endsWith(layer)).length, 2, layer);
  assert.equal(new Set(composite.mesh.parts.filter((part) => /core|belly|backing/.test(part.label)).map((part) => part.material)).size, 3);
});

test("D-section perimeter is one simple flat-back and curved-belly loop", () => {
  const outline = sectionOutline("dShape", 0.032, 0.036, 16), keys = outline.map((point) => point.map((value) => value.toFixed(9)).join(","));
  assert.deepEqual(simplePolygonErrors(outline, "D-section"), []);
  assert.equal(new Set(keys).size, keys.length, "perimeter has no coincident/retraced vertex");
  const flatEdges = outline.filter((point) => Math.abs(point[0] + 0.016) < 1e-9);
  assert.equal(flatEdges.length, 2, "only the two flat-back endpoints lie on the back plane");
  assert.ok(outline.slice(1, -1).some((point) => point[0] > 0.015), "curved belly reaches the opposite side");
});

test("composite layers meet exactly through the full taper without gaps or overlap", () => {
  const component = preset("composite-recurve-bow-1544").definition.components[0];
  for (const progress of [0, 0.25, 0.5, 0.75, 1]) {
    const layout = bowCompositeLayerLayout(component, progress);
    assert.ok(Math.abs(layout.horn.interval[1] - layout.core.interval[0]) < 1e-12, `horn/core at ${progress}`);
    assert.ok(Math.abs(layout.core.interval[1] - layout.back.interval[0]) < 1e-12, `core/back at ${progress}`);
    assert.ok(Math.abs(layout.horn.interval[0] + component.limbDepth * layout.scale / 2) < 1e-12);
    assert.ok(Math.abs(layout.back.interval[1] - component.limbDepth * layout.scale / 2) < 1e-12);
  }
});

test("arrow nock has a real open slot wider than the default bowstring", () => {
  const source = preset("flight-arrow-1544"), result = validateWeapon(source.definition, source.controls),
    component = source.definition.components[0], nock = result.mesh.parts.find((part) => part.label === "slotted arrow nock");
  assert.equal(result.valid, true, result.errors.join(" | "));
  assert.ok(component.nockSlotWidth > preset("german-self-bow-1544").definition.components[0].stringRadius * 2);
  for (const triangle of triangleVertices(nock)) {
    const xs = triangle.map((point) => point[0]), ys = triangle.map((point) => point[1]);
    assert.ok(!(Math.max(...ys) < -component.nockLength * 0.45 && Math.min(...xs) < -component.nockSlotWidth / 2 && Math.max(...xs) > component.nockSlotWidth / 2), "triangle bridges the open nock slot");
  }
});

test("arrow nock clearance metadata covers every compatible bow-string endpoint", () => {
  const arrow = preset("flight-arrow-1544"), slot = arrow.controls.find((control) => control.label === "Nock slot width"),
    maximumStringRadius = Math.max(...["german-self-bow-1544", "composite-recurve-bow-1544"].map((id) => preset(id).controls.find((control) => control.label === "String thickness").max)),
    component = arrow.definition.components[0];
  assert.equal(component.maximumStringRadius, maximumStringRadius);
  assert.ok(slot.min >= maximumStringRadius * 2 + component.nockClearance);
  for (const slotWidth of [slot.min, slot.max]) {
    const changed = copyPreset(arrow); changed.definition.components[0].nockSlotWidth = slotWidth;
    assert.equal(validateWeapon(changed.definition, changed.controls).valid, true, `slot ${slotWidth}`);
  }
});

test("quiver has a sealed bottom and remains hollow at the mouth", () => {
  const source = preset("arrow-quiver-1544"), result = validateWeapon(source.definition, source.controls),
    body = result.mesh.parts.find((part) => part.label === "open arrow quiver body"),
    cap = result.mesh.parts.find((part) => part.label === "sealed quiver bottom"),
    mouthY = source.definition.components[0].length;
  assert.equal(result.valid, true, result.errors.join(" | "));
  assert.ok(cap && signedVolume(cap) > 0);
  for (const triangle of triangleVertices(body)) {
    if (triangle.every((point) => Math.abs(point[1] - mouthY) < 1e-7))
      assert.ok(triangle.every((point) => Math.hypot(point[0], point[2]) > source.definition.components[0].mouthRadius - source.definition.components[0].wall - 1e-6), "mouth contains a center cap");
  }
});

test("quiver strap anchors follow evaluated tapered radii with intentional overlap", () => {
  const source = preset("arrow-quiver-1544");
  for (const endpoint of ["min", "max"]) {
    const changed = copyPreset(source);
    for (const control of changed.controls) setControlValue(changed.definition, control, control[endpoint]);
    const component = changed.definition.components[0], profile = arrowQuiverProfile(component), path = arrowQuiverStrapPath(component),
      radiusAt = (y) => { const upper = profile.findIndex((point) => point[0] >= y), hi = profile[Math.max(0, upper)], lo = profile[Math.max(0, upper - 1)], t = hi[0] === lo[0] ? 0 : (y - lo[0]) / (hi[0] - lo[0]); return lo[1] + (hi[1] - lo[1]) * t; };
    for (const anchor of [path[0], path[2]]) {
      const overlap = radiusAt(anchor[1]) - anchor[0];
      assert.ok(overlap > 0 && Math.abs(overlap - component.strapThickness * 0.35) < 1e-9, `${endpoint} anchor overlap`);
    }
    assert.equal(validateWeapon(changed.definition, changed.controls).valid, true, endpoint);
  }
});

test("tip end-loops use the local limb frame, contact the span, and encircle the nock", () => {
  for (const id of ["german-self-bow-1544", "composite-recurve-bow-1544"]) {
    const component = preset(id).definition.components[0];
    for (const upper of [false, true]) {
      const layout = bowTipLoopLayout(component, upper), radial = component.limbDepth * component.tipScale * 0.54,
        lateral = component.limbWidth * component.tipScale * 0.54;
      assert.ok(Math.abs(layout.tangent.reduce((sum, value, axis) => sum + value * layout.normal[axis], 0)) < 1e-9);
      assert.ok(Math.abs(layout.tangent.reduce((sum, value, axis) => sum + value * layout.binormal[axis], 0)) < 1e-9);
      assert.ok(layout.radialAxis > radial && layout.widthAxis > lateral, `${id} encirclement`);
      assert.ok(layout.points.some((point) => Math.hypot(...point.map((value, axis) => value - layout.attachment[axis])) < 1e-8), `${id} span contact`);
    }
  }
});

test("representative bow, arrow, and carrier controls materially alter owned geometry", () => {
  for (const [id, label] of [["german-self-bow-1544", "Limb reflex"], ["flight-arrow-1544", "Nock slot width"], ["arrow-quiver-1544", "Quiver mouth radius"]]) {
    const source = preset(id), baseline = validateWeapon(source.definition, source.controls), changed = copyPreset(source),
      control = changed.controls.find((candidate) => candidate.label === label);
    setControlValue(changed.definition, control, control.max);
    const result = validateWeapon(changed.definition, changed.controls);
    assert.equal(result.valid, true, `${id}/${label}: ${result.errors.join(" | ")}`);
    assert.notDeepEqual(result.mesh.positions, baseline.mesh.positions);
  }
});

test("discrete bow, ammunition, and carrier choices remain structurally valid", () => {
  for (const id of ["german-self-bow-1544", "composite-recurve-bow-1544", "flight-arrow-1544", "arrow-quiver-1544"]) {
    const source = preset(id);
    for (const choice of source.choiceControls ?? []) for (const option of choice.options) {
      const changed = copyPreset(source);
      setControlValue(changed.definition, choice, option.value);
      const result = validateWeapon(changed.definition, changed.controls);
      assert.equal(result.valid, true, `${id}/${choice.label}/${option.label}: ${result.errors.join(" | ")}`);
    }
  }
});
