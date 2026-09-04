import assert from "node:assert/strict";
import test from "node:test";
import { closedManifoldErrors, crossbowStockLayout, crossbowTipLoopLayout, measureMassProperties, signedVolume, validateWeapon } from "../src/mesh.js";
import { PRESETS, copyPreset, setControlValue } from "../src/presets.js";

const preset = (id) => PRESETS.find((candidate) => candidate.id === id);
const crossbowIds = ["german-cranequin-crossbow-1544", "central-composite-arbalest", "light-target-crossbow-comparative"];
const bounds = (part) => { const points = Array.from({ length: part.positions.length / 3 }, (_, i) => part.positions.slice(i * 3, i * 3 + 3)); return { min: [0, 1, 2].map((a) => Math.min(...points.map((p) => p[a]))), max: [0, 1, 2].map((a) => Math.max(...points.map((p) => p[a]))) }; };
const overlap = (a, b, axis) => a.min[axis] <= b.max[axis] && a.max[axis] >= b.min[axis];

test("crossbows, quarrels, and German bolt quiver are independent outputs", () => {
  assert.deepEqual(crossbowIds.map((id) => preset(id).definition.components[0].kind), ["crossbow", "crossbow", "crossbow"]);
  assert.equal(preset("crossbow-bolt-1544").definition.components[0].kind, "crossbowBolt");
  assert.equal(preset("bolt-quiver-1544").definition.components[0].kind, "boltQuiver");
  assert.equal(PRESETS.some((candidate) => candidate.id === "light-target-crossbow-1544"), false);
});

test("working lock has string notch, butt shelf, axle, sear, contacting trigger, and paired runner rails", () => {
  for (const id of crossbowIds) {
    const result = validateWeapon(preset(id).definition, preset(id).controls), labels = result.mesh.parts.map((p) => p.label);
    assert.equal(result.valid, true, `${id}: ${result.errors.join(" | ")}`);
    for (const label of ["left rotating nut cheek", "right rotating nut cheek", "nut axle and bearing", "nut string notch floor", "bolt butt nut shelf", "left recessed bolt runner rail", "right recessed bolt runner rail", "nut sear notch and tooth", "long trigger to sear"]) assert.ok(labels.includes(label), `${id}: ${label}`);
    const string = bounds(result.mesh.parts.find((p) => p.label === "served crossbow nocking span")), floor = bounds(result.mesh.parts.find((p) => p.label === "nut string notch floor")), leftCheek = bounds(result.mesh.parts.find((p) => p.label === "left rotating nut cheek")), rightCheek = bounds(result.mesh.parts.find((p) => p.label === "right rotating nut cheek"));
    assert.ok(overlap(string, floor, 0) && overlap(string, floor, 1) && string.min[2] >= floor.max[2] - 1e-8, `${id}: string seats on notch floor without intersecting it`);
    assert.ok(string.min[0] < leftCheek.max[0] && string.max[0] > rightCheek.min[0], `${id}: string crosses the open gap between nut cheeks`);
    const sear = bounds(result.mesh.parts.find((p) => p.label === "nut sear notch and tooth")), trigger = bounds(result.mesh.parts.find((p) => p.label === "long trigger to sear"));
    assert.ok([0, 1, 2].every((axis) => overlap(sear, trigger, axis)), `${id}: trigger engages sear`);
  }
});

test("spanning modes expose family-constrained mating load paths", () => {
  const expected = new Map([[crossbowIds[0], ["cranequin stock rest peg", "cranequin rack purchase rail"]], [crossbowIds[1], ["left goats-foot pivot lug", "right goats-foot pivot lug", "goats-foot pivot axle"]], [crossbowIds[2], ["belt-hook purchase bar"]]]);
  for (const id of crossbowIds) {
    const source = preset(id), result = validateWeapon(source.definition, source.controls), stocks = result.mesh.parts.filter((p) => /tiller stock|stock cheek/.test(p.label)).map(bounds);
    for (const label of expected.get(id)) { const part = bounds(result.mesh.parts.find((p) => p.label === label)); assert.ok(stocks.some((stock) => overlap(part, stock, 0) && overlap(part, stock, 1)), `${id}: ${label} mates with stock`); }
    const spanning = source.choiceControls.find((choice) => choice.label === "Spanning accommodation"); assert.equal(spanning.options.length, 1); assert.equal(spanning.options[0].value, source.definition.components[0].spanningMode);
  }
});

test("open nut cavity and recessed runner share one datum without unintended stock intersections", () => {
  for (const id of crossbowIds) {
    const source = preset(id), c = source.definition.components[0], layout = crossbowStockLayout(c), result = validateWeapon(source.definition, source.controls), part = (label) => bounds(result.mesh.parts.find((p) => p.label === label));
    assert.equal(result.mesh.parts.some((p) => p.label.includes("nut well")), false, `${id}: cavity is omitted, not filled`);
    const rear = part("profiled rear crossbow tiller stock"), fore = part("profiled fore-end crossbow tiller stock"), leftStock = part("left open nut-cavity stock cheek"), rightStock = part("right open nut-cavity stock cheek");
    assert.ok(rear.max[1] <= layout.cavityStart + 1e-9 && fore.min[1] >= layout.cavityEnd - 1e-9);
    assert.ok(leftStock.max[0] <= -layout.gapWidth / 2 + 1e-9 && rightStock.min[0] >= layout.gapWidth / 2 - 1e-9);
    for (const label of ["left rotating nut cheek", "right rotating nut cheek", "nut string notch floor", "bolt butt nut shelf", "nut sear notch and tooth", "long trigger to sear"]) {
      const mechanism = part(label); assert.ok(mechanism.min[0] > leftStock.max[0] - 1e-9 && mechanism.max[0] < rightStock.min[0] + 1e-9, `${id}: ${label} stays in cavity free space`);
    }
    for (const label of ["left recessed bolt runner rail", "right recessed bolt runner rail"]) { const rail = part(label); assert.ok(Math.abs(rail.min[2] - fore.max[2]) < 1e-9, `${id}: ${label} seats on fore-stock without volume overlap`); }
    const axle = part("nut axle and bearing"); assert.ok(axle.min[0] <= leftStock.max[0] && axle.max[0] >= rightStock.min[0], `${id}: axle reaches both stock bearings`);
  }
});

test("butt, waist, and nose width independently change the combined-profile stock", () => {
  const source = preset(crossbowIds[0]), signature = (definition) => validateWeapon(definition, source.controls).mesh.parts.filter((p) => /tiller stock|stock cheek/.test(p.label)).flatMap((p) => p.positions).join(",");
  const baseline = signature(source.definition);
  for (const label of ["Butt width", "Tiller waist", "Fore-end width"]) { const changed = copyPreset(source), control = changed.controls.find((candidate) => candidate.label === label); setControlValue(changed.definition, control, control.min); assert.notEqual(signature(changed.definition), baseline, label); }
});

test("served string spans and tip loops remain independently manifold and seated", () => {
  const labels = ["left crossbow string control span", "right crossbow string control span", "served crossbow nocking span", "left crossbow string end loop", "right crossbow string end loop"];
  for (const id of crossbowIds) {
    const source = preset(id), result = validateWeapon(source.definition, source.controls), c = source.definition.components[0];
    for (const label of labels) assert.equal(result.mesh.parts.filter((p) => p.label === label).length, 1, `${id}: ${label}`);
    for (const right of [false, true]) { const loop = crossbowTipLoopLayout(c, right); assert.ok(loop.depthAxis > c.prodDepth * c.prodTipScale / 2); assert.ok(loop.points.some((p) => Math.hypot(...p.map((v, a) => v - loop.attachment[a])) < 1e-8)); }
    for (const label of labels.slice(-2)) { const loop = result.mesh.parts.find((p) => p.label === label); assert.ok(signedVolume(loop) > 0); assert.deepEqual(closedManifoldErrors(loop, label), []); }
  }
});

test("war quarrel has flattened bearing butt and two angled stiff vanes; hunting bolt has three feathers", () => {
  const source = preset("crossbow-bolt-1544");
  for (const [use, count, material] of [["war", 2, "leather"], ["hunting", 3, "feather"]]) {
    const changed = copyPreset(source); changed.definition.components[0].boltUse = use;
    const result = validateWeapon(changed.definition, changed.controls), vanes = result.mesh.parts.filter((p) => p.label.startsWith(`${use} bolt vane`));
    assert.equal(result.valid, true, result.errors.join(" | ")); assert.equal(vanes.length, count); assert.ok(vanes.every((v) => v.material.density === (material === "feather" ? 350 : 920)));
    assert.ok(result.mesh.parts.some((p) => p.label === "flattened reinforced quarrel butt")); assert.ok(result.mesh.parts.some((p) => p.label === "flat bolt butt bearing face")); assert.equal(result.mesh.parts.some((p) => /nock|string seat/.test(p.label)), false);
  }
});

test("quarrel butt aligns on paired rails, bears on nut shelf, and meets the served string", () => {
  const weapon = preset(crossbowIds[0]).definition.components[0], bolt = preset("crossbow-bolt-1544").definition.components[0];
  assert.ok(bolt.buttWidth > weapon.grooveWidth * 0.7 && bolt.buttWidth < weapon.buttWidth); assert.ok(bolt.buttHeight <= weapon.railHeight); assert.ok(weapon.servingWidth > bolt.buttWidth); assert.ok(weapon.nutPosition < weapon.prodPosition && weapon.railHeight > weapon.stringRadius * 2);
});

test("quarrel shaft and reinforced butt control cross-product remains coherent", () => {
  const source = preset("crossbow-bolt-1544"), radius = source.controls.find((control) => control.label === "Bolt radius"), butt = source.controls.find((control) => control.label === "Butt width");
  for (const shaftRadius of [radius.min, radius.max]) for (const buttWidth of [butt.min, butt.max]) {
    const changed = copyPreset(source); setControlValue(changed.definition, radius, shaftRadius); setControlValue(changed.definition, butt, buttWidth);
    const result = validateWeapon(changed.definition, changed.controls); assert.equal(result.valid, true, `${shaftRadius}/${buttWidth}: ${result.errors.join(" | ")}`);
  }
});

test("heavy Met-anchored preset is approximately 73.7 cm overall, 62.4 cm prod span, and 3 kg", () => {
  const source = preset(crossbowIds[0]), result = validateWeapon(source.definition, source.controls), mass = measureMassProperties(result.mesh).massKg;
  assert.ok(Math.abs(result.mesh.stats.dimensions[1] - 0.737) < 0.015, result.mesh.stats.dimensions[1]); assert.ok(Math.abs(source.definition.components[0].prodSpan - 0.624) < 0.006); assert.ok(mass > 2.75 && mass < 3.25, mass);
});

test("German bolt quiver is broad, layered, open-mouth, sealed-bottom, attached, and near catalog mass", () => {
  const source = preset("bolt-quiver-1544"), result = validateWeapon(source.definition, source.controls), c = source.definition.components[0], labels = result.mesh.parts.map((p) => p.label), mass = measureMassProperties(result.mesh).massKg;
  assert.equal(result.valid, true, result.errors.join(" | ")); assert.equal(c.length, 0.446); assert.equal(c.bottomWidth, 0.29);
  for (const label of ["bolt quiver wood front shell", "bolt quiver wood back shell", "paper lining layer", "hide outer cover layer", "sealed broad bolt quiver bottom", "open bolt quiver leather mouth binding", "attached bolt quiver shoulder strap"]) assert.ok(labels.includes(label), label);
  assert.ok(mass > 0.38 && mass < 0.54, mass); const strap = bounds(result.mesh.parts.find((p) => p.label === "attached bolt quiver shoulder strap")); assert.ok(strap.min[0] < c.bottomWidth / 2 + c.strapThickness && strap.max[1] >= c.length * 0.78);
});

test("bounded controls and family choices preserve valid topology", () => {
  for (const id of [...crossbowIds, "crossbow-bolt-1544", "bolt-quiver-1544"]) {
    const source = preset(id);
    for (const control of source.controls) for (const endpoint of ["min", "max"]) { const changed = copyPreset(source); setControlValue(changed.definition, control, control[endpoint]); const result = validateWeapon(changed.definition, changed.controls); assert.equal(result.valid, true, `${id}/${control.label}/${endpoint}: ${result.errors.join(" | ")}`); }
    for (const choice of source.choiceControls ?? []) for (const option of choice.options) { const changed = copyPreset(source); setControlValue(changed.definition, choice, option.value); const result = validateWeapon(changed.definition, changed.controls); assert.equal(result.valid, true, `${id}/${choice.label}/${option.label}: ${result.errors.join(" | ")}`); }
  }
});
