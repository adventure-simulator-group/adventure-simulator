import assert from "node:assert/strict";
import test from "node:test";
import { buildWeapon, closedManifoldErrors, firearmStockLayout, measureMassProperties, signedVolume, validateWeapon } from "../src/mesh.js";
import { PRESETS, copyPreset, setControlValue } from "../src/presets.js";
import { triangleVertices } from "../src/topology.js";

const preset = (id) => PRESETS.find((candidate) => candidate.id === id);
const firearmIds = ["peter-peck-double-wheellock-pistol-1545", "german-matchlock-arquebus-16c", "single-wheellock-pistol-study"];
const bounds = (part) => { const points = Array.from({ length: part.positions.length / 3 }, (_, i) => part.positions.slice(i * 3, i * 3 + 3)); return { min: [0, 1, 2].map((a) => Math.min(...points.map((p) => p[a]))), max: [0, 1, 2].map((a) => Math.max(...points.map((p) => p[a]))) }; };
const overlap = (a, b, axis) => a.min[axis] <= b.max[axis] && a.max[axis] >= b.min[axis];

test("firearms, lead ball, and ball pouch are independent small-arm outputs", () => {
  assert.deepEqual(firearmIds.map((id) => preset(id).definition.components[0].kind), ["firearm", "firearm", "firearm"]);
  assert.equal(preset("lead-round-ball").definition.components[0].kind, "leadBall"); assert.equal(preset("small-arms-ball-pouch").definition.components[0].kind, "ballPouch");
  assert.deepEqual(preset("lead-round-ball").controls.map((control) => control.label), ["Ball radius"]);
  assert.equal(PRESETS.some((candidate) => /cannon|artillery/i.test(candidate.id)), false);
});

test("museum-anchored endpoints preserve dimensions, caliber, barrel length, and arquebus mass", () => {
  const pistol = preset(firearmIds[0]), pistolResult = validateWeapon(pistol.definition, pistol.controls), arquebus = preset(firearmIds[1]), arquebusResult = validateWeapon(arquebus.definition, arquebus.controls);
  const pistolComponent = pistol.definition.components[0];
  assert.equal(pistolResult.valid, true, pistolResult.errors.join(" | ")); assert.ok(Math.abs(pistolResult.mesh.stats.dimensions[1] - 0.492) < 0.001); assert.equal(pistolComponent.bore, 0.0117); assert.equal(pistolComponent.barrelCount, 2);
  assert.equal(pistolComponent.barrelLength, 0.254); assert.equal(pistolComponent.secondaryBarrelLength, 0.194);
  const upper = bounds(pistolResult.mesh.parts.find((part) => part.label === "upper barrel round fore-barrel")), lower = bounds(pistolResult.mesh.parts.find((part) => part.label === "lower barrel round fore-barrel"));
  assert.ok(upper.min[2] > lower.min[2], "barrels are stacked vertically, not side-by-side"); assert.ok(overlap(upper, lower, 0)); assert.ok(Math.abs(upper.max[1] - 0.492) < 1e-6); assert.ok(Math.abs(lower.max[1] - 0.432) < 1e-6);
  assert.equal(arquebusResult.valid, true, arquebusResult.errors.join(" | ")); assert.ok(Math.abs(arquebusResult.mesh.stats.dimensions[1] - 1.603) < 0.015); assert.equal(arquebus.definition.components[0].barrelLength, 1.216); assert.equal(arquebus.definition.components[0].bore, 0.0177);
  const mass = measureMassProperties(arquebusResult.mesh).massKg; assert.ok(mass > 5.8 && mass < 6.5, mass);
});

test("each barrel is hollow and open at muzzle with a separate closed breech", () => {
  for (const id of firearmIds) {
    const source = preset(id), result = validateWeapon(source.definition, source.controls), c = source.definition.components[0]; assert.equal(result.valid, true, result.errors.join(" | "));
    const names = c.barrelCount === 2 ? ["upper", "lower"] : ["barrel 1"], lengths = c.barrelCount === 2 ? [c.barrelLength, c.secondaryBarrelLength] : [c.barrelLength], centers = c.barrelCount === 2 ? [(c.bore / 2 + c.barrelWall) * 3.15, (c.bore / 2 + c.barrelWall) * 1.05] : [c.bore / 2 + c.barrelWall];
    for (let index = 0; index < names.length; index++) {
      const name = names[index], fore = result.mesh.parts.find((p) => p.label === `${name} barrel round fore-barrel`), breech = result.mesh.parts.find((p) => p.label === `${name} barrel closed breech`), end = c.length - c.barrelLength + lengths[index];
      assert.deepEqual(closedManifoldErrors(fore, fore.label), []); assert.ok(signedVolume(breech) > 0);
      for (const triangle of triangleVertices(fore)) if (triangle.every((point) => Math.abs(point[1] - end) < 1e-7)) assert.ok(triangle.every((point) => Math.hypot(point[0], point[2] - centers[index]) >= c.bore / 2 - 1e-6), `${id}: ${name} muzzle center remains open`);
    }
  }
});

test("Peck lock has two functional ignition trains with split pyrite jaws and shared trigger/sear/safety", () => {
  const result = validateWeapon(preset(firearmIds[0]).definition, preset(firearmIds[0]).controls), find = (label) => result.mesh.parts.find((part) => part.label === label), part = (label) => bounds(find(label)), plate = part("combined firearm lock plate");
  assert.equal(result.mesh.parts.filter((part) => /wheellock (upper|lower) wheel$/.test(part.label)).length, 2);
  assert.equal(result.mesh.parts.filter((part) => /touchhole channel to bore$/.test(part.label)).length, 2);
  for (const name of ["upper", "lower"]) {
    const wheel = part(`wheellock ${name} wheel`), axle = part(`wheellock ${name} axle and bearing`), spring = part(`wheellock ${name} mainspring`), arm = part(`wheellock ${name} cock arm`), upperJaw = part(`wheellock ${name} cock upper jaw`), lowerJaw = part(`wheellock ${name} cock lower jaw`), pyrite = part(`wheellock ${name} visible pyrite`), pan = part(`wheellock ${name} pan cavity bottom`), cover = part(`wheellock ${name} pan cover`), touchhole = part(`${name} touchhole channel to bore`);
    assert.ok(overlap(wheel, axle, 0) && overlap(wheel, axle, 1) && overlap(wheel, axle, 2)); assert.ok(overlap(wheel, spring, 1)); assert.ok(overlap(wheel, plate, 0));
    assert.ok(overlap(arm, upperJaw, 1) && overlap(arm, lowerJaw, 1)); assert.ok(overlap(pyrite, upperJaw, 1) && overlap(pyrite, lowerJaw, 1)); assert.ok(pyrite.min[2] < upperJaw.max[2] && pyrite.max[2] > lowerJaw.min[2]);
    assert.ok(overlap(cover, pan, 0) && overlap(cover, pan, 1)); assert.ok(touchhole.min[0] <= pan.max[0] && touchhole.max[0] >= 0); assert.ok(find(`wheellock ${name} pan cover`).animationPivot.every(Number.isFinite));
  }
  for (const label of ["wheellock safety lever", "firearm trigger blade", "wheellock sear linkage"]) assert.ok(find(label).animationPivot.every(Number.isFinite), label);
});

test("matchlock has pivoted serpentine, split jaws and match, open pan, touchhole, linkage, and trigger", () => {
  const result = validateWeapon(preset(firearmIds[1]).definition, preset(firearmIds[1]).controls), find = (label) => result.mesh.parts.find((part) => part.label === label), part = (label) => bounds(find(label)), arm = part("matchlock serpentine arm"), upperJaw = part("matchlock serpentine upper jaw"), lowerJaw = part("matchlock serpentine lower jaw"), match = part("visible match cord"), pan = part("matchlock pan cavity bottom"), cover = part("matchlock pan cover"), linkage = part("matchlock trigger linkage"), trigger = part("firearm trigger blade"), plate = part("combined firearm lock plate"), touchhole = part("matchlock touchhole channel to bore");
  assert.ok(overlap(arm, upperJaw, 1) && overlap(arm, lowerJaw, 1)); assert.ok(overlap(match, upperJaw, 1) && overlap(match, lowerJaw, 1)); assert.ok(overlap(cover, pan, 0) && overlap(cover, pan, 1)); assert.ok(touchhole.min[0] <= pan.max[0] && touchhole.max[0] >= 0); assert.ok(overlap(linkage, plate, 0) && overlap(linkage, trigger, 1));
  for (const label of ["matchlock serpentine arm", "matchlock serpentine upper jaw", "matchlock serpentine lower jaw", "visible match cord", "matchlock pan cover", "matchlock trigger linkage", "firearm trigger blade"]) assert.ok(find(label).animationPivot.every(Number.isFinite), label);
});

test("every ignition train penetrates radially through the barrel wall into its bore at bore/wall endpoints", () => {
  for (const id of firearmIds) {
    const source = preset(id), boreControl = source.controls.find((control) => control.label === "Bore diameter"), wallControl = source.controls.find((control) => control.label === "Barrel wall");
    for (const bore of [boreControl.min, boreControl.max]) for (const wall of [wallControl.min, wallControl.max]) {
      const changed = copyPreset(source); setControlValue(changed.definition, boreControl, bore); setControlValue(changed.definition, wallControl, wall);
      const result = validateWeapon(changed.definition, changed.controls), expected = changed.definition.components[0].barrelCount;
      assert.equal(result.valid, true, `${id}/${bore}/${wall}: ${result.errors.join(" | ")}`);
      const channels = result.mesh.parts.filter((part) => part.label.endsWith("touchhole channel to bore")); assert.equal(channels.length, expected, id);
      for (const channel of channels) {
        const end = channel.touchholeCenterline.at(-1), center = channel.boreCenter, radial = Math.hypot(end[0] - center[0], end[2] - center[2]);
        assert.ok(radial <= channel.boreRadius, `${id}/${channel.label}: ${radial} reaches ${channel.boreRadius}`);
        const start = channel.touchholeCenterline[0], outsideRadius = Math.hypot(start[0] - center[0], start[2] - center[2]); assert.ok(outsideRadius > bore / 2 + wall);
      }
    }
  }
});

test("combined stock plan and side profile respond to all principal stock controls", () => {
  const source = preset(firearmIds[1]), signature = (definition) => validateWeapon(definition, source.controls).mesh.parts.find((p) => p.label === "combined-profile firearm stock").positions.join(","), baseline = signature(source.definition);
  for (const label of ["Butt width", "Lock waist width", "Fore-stock width", "Stock depth", "Butt drop"]) { const changed = copyPreset(source), control = changed.controls.find((candidate) => candidate.label === label); setControlValue(changed.definition, control, control.min); assert.notEqual(signature(changed.definition), baseline, label); }
  const layout = firearmStockLayout(source.definition.components[0]); assert.equal(layout.barrelStart, 0.387);
});

test("Peck and matchlock stocks have family-specific silhouettes and an attached solid fluted pommel", () => {
  const peck = preset(firearmIds[0]), matchlock = preset(firearmIds[1]), peckLayout = firearmStockLayout(peck.definition.components[0]), matchLayout = firearmStockLayout(matchlock.definition.components[0]);
  assert.equal(peckLayout.stations.length, 8); assert.equal(matchLayout.stations.length, 9);
  assert.ok(peckLayout.stations[1].width > peckLayout.stations[0].width, "Peck stock flares smoothly out of the pommel neck");
  assert.ok(peckLayout.stations.slice(1, 6).every((station, index, stations) => index === 0 || station.top >= stations[index - 1].top), "Peck upper line has a shallow monotonic sweep");
  assert.ok(matchLayout.stations[1].width > matchlock.definition.components[0].buttWidth, "matchlock has a broad cheek/butt swell");
  assert.ok(matchLayout.stations[4].bottom < -matchlock.definition.components[0].stockDepth * 0.85, "matchlock cheek stays deep well forward");
  assert.notDeepEqual(peckLayout.stations.map((station) => [station.y / peck.definition.components[0].length, station.width / peck.definition.components[0].buttWidth]), matchLayout.stations.map((station) => [station.y / matchlock.definition.components[0].length, station.width / matchlock.definition.components[0].buttWidth]));
  const result = validateWeapon(peck.definition, peck.controls), stock = bounds(result.mesh.parts.find((part) => part.label === "combined-profile firearm stock")), pommelPart = result.mesh.parts.find((part) => part.label === "solid spiral-fluted bulb pommel"), pommel = bounds(pommelPart);
  assert.ok(pommelPart); assert.deepEqual(closedManifoldErrors(pommelPart, pommelPart.label), []); assert.ok([0, 1, 2].every((axis) => overlap(stock, pommel, axis)), "solid fluted pommel overlaps stock neck in all axes");
  assert.equal(result.mesh.parts.some((part) => /helix|spiral gilt pommel furniture/i.test(part.label)), false, "no floating spring-like ornament remains");
  const radii = Array.from({ length: pommelPart.positions.length / 3 }, (_, index) => Math.hypot(pommelPart.positions[index * 3], pommelPart.positions[index * 3 + 2] + peck.definition.components[0].stockDepth + peck.definition.components[0].buttDrop));
  assert.ok(Math.max(...radii) - Math.min(...radii) > peck.definition.components[0].buttWidth * 0.35, "fluting and bulb are built into the solid surface");
});

test("lead ball radius can match each compatible bore with positive windage", () => {
  const ball = preset("lead-round-ball"), control = ball.controls[0];
  for (const id of firearmIds) { const bore = preset(id).definition.components[0].bore, radius = bore / 2 - 0.0003; assert.ok(radius >= control.min && radius <= control.max); const changed = copyPreset(ball); setControlValue(changed.definition, control, Math.round(radius * 10000) / 10000); const result = validateWeapon(changed.definition, changed.controls); assert.equal(result.valid, true, result.errors.join(" | ")); assert.ok(changed.definition.components[0].radius * 2 < bore); }
});

test("ball pouch has an uncapped mouth, hinged overlapping flap, closure, and attached belt loops", () => {
  const source = preset("small-arms-ball-pouch"), result = validateWeapon(source.definition, source.controls), labels = result.mesh.parts.map((p) => p.label), c = source.definition.components[0]; assert.equal(result.valid, true, result.errors.join(" | "));
  for (const label of ["ball pouch front", "ball pouch back", "ball pouch left gusset", "ball pouch right gusset", "sealed ball pouch bottom", "ball pouch hinged flap", "pouch flap hinge", "belt attachment loop left", "belt attachment loop right", "horn pouch toggle"]) assert.ok(labels.includes(label), label);
  assert.equal(result.mesh.parts.some((p) => p.label.includes("mouth cap")), false); const flapPart = result.mesh.parts.find((p) => p.label === "ball pouch hinged flap"), flap = bounds(flapPart), hinge = bounds(result.mesh.parts.find((p) => p.label === "pouch flap hinge")), front = bounds(result.mesh.parts.find((p) => p.label === "ball pouch front")); assert.ok(overlap(flap, hinge, 0) && overlap(flap, hinge, 1)); assert.ok(flap.min[1] < c.height - c.flapOverlap); assert.ok(overlap(flap, front, 2), "closed flap crosses the mouth and overlaps the front"); assert.deepEqual(flapPart.animationPivot, [0, c.height, -c.depth / 2]);
  const opened = copyPreset(source), angle = opened.controls.find((control) => control.label === "Flap angle"); setControlValue(opened.definition, angle, angle.max); const openResult = validateWeapon(opened.definition, opened.controls), openFlap = bounds(openResult.mesh.parts.find((part) => part.label === "ball pouch hinged flap")); assert.equal(openResult.valid, true, openResult.errors.join(" | ")); assert.ok(openFlap.min[1] >= c.height - 0.01 || !overlap(openFlap, front, 2), "open flap clears the mouth/front access");
});

test("barrel bands lie in XZ sections and the side-elevation guard encloses a reachable trigger", () => {
  for (const id of firearmIds) {
    const source = preset(id), result = validateWeapon(source.definition, source.controls), c = source.definition.components[0], bands = result.mesh.parts.filter((part) => /barrel band \d+ XZ enclosure/.test(part.label));
    assert.equal(bands.length, c.bandCount, id);
    for (const band of bands) { const b = bounds(band); assert.ok(b.max[1] - b.min[1] < 0.008, band.label); assert.ok(b.min[0] < -c.foreWidth / 2 && b.max[0] > c.foreWidth / 2, band.label); assert.ok(b.min[2] < 0 && b.max[2] > c.bore / 2 + c.barrelWall, band.label); }
    const guard = bounds(result.mesh.parts.find((part) => part.label === "trigger guard in side elevation")), trigger = bounds(result.mesh.parts.find((part) => part.label === "firearm trigger blade"));
    assert.ok(guard.max[0] - guard.min[0] < 0.012); assert.ok(trigger.min[1] >= guard.min[1] - 0.006 && trigger.max[1] <= guard.max[1] + 0.006); assert.ok(trigger.min[2] >= guard.min[2] && trigger.max[2] <= guard.max[2] + 0.006);
  }
});

test("lead ball low-detail volume stays within eight percent of analytic volume", () => {
  const source = preset("lead-round-ball"), radius = source.definition.components[0].radius, analytic = 4 / 3 * Math.PI * radius ** 3;
  for (const lod of ["low", "medium", "high"]) { const mesh = buildWeapon(source.definition, { lod }); assert.ok(Math.abs(signedVolume(mesh) - analytic) / analytic < 0.08, lod); }
});

test("bounded firearm, ball, and pouch controls and coherent choices validate", () => {
  for (const id of [...firearmIds, "lead-round-ball", "small-arms-ball-pouch"]) { const source = preset(id); for (const control of source.controls) for (const endpoint of ["min", "max"]) { const changed = copyPreset(source); setControlValue(changed.definition, control, control[endpoint]); const result = validateWeapon(changed.definition, changed.controls); assert.equal(result.valid, true, `${id}/${control.label}/${endpoint}: ${result.errors.join(" | ")}`); } for (const choice of source.choiceControls ?? []) for (const option of choice.options) { const changed = copyPreset(source); setControlValue(changed.definition, choice, option.value); const result = validateWeapon(changed.definition, changed.controls); assert.equal(result.valid, true, `${id}/${choice.label}/${option.label}: ${result.errors.join(" | ")}`); } }
});

test("bounded firearm length/profile and bore/wall endpoint cross-products stay coherent", () => {
  for (const id of firearmIds) {
    const source = preset(id), groups = [
      ["Overall length", "Barrel length", "Butt width", "Lock waist width", "Fore-stock width"],
      ["Bore diameter", "Barrel wall"],
    ];
    for (const labels of groups) {
      const controls = labels.map((label) => source.controls.find((control) => control.label === label));
      for (let mask = 0; mask < 2 ** controls.length; mask++) {
        const changed = copyPreset(source);
        controls.forEach((control, index) => setControlValue(changed.definition, control, control[(mask >> index) & 1 ? "max" : "min"]));
        const result = validateWeapon(changed.definition, changed.controls);
        assert.equal(result.valid, true, `${id}/${labels.join("+")}/${mask}: ${result.errors.join(" | ")}`);
      }
    }
  }
});
