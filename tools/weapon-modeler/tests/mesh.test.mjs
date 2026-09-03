import { triangleVertices } from "../src/topology.js";
import assert from "node:assert/strict";
import test from "node:test";
import { billHeadCurveSpans, billHeadOutline, buildWeapon, figureEightGuard, glaiveOutline, glaiveSpineCurveSpans, knuckleBow, maceFlangeOutline, measureMassProperties, sampleAdaptiveCurve, sampleCubicBezier, signedVolume, triangulatePolygon, tubePath, tubeRadialSegments, validateWeapon } from "../src/mesh.js";
import { automaticGripPoint } from "../src/glb-export.js";
import { HEAD_KINDS, PRESETS, copyPreset, getPath, setControlValue, setPath } from "../src/presets.js";
import { fitDistance, projectedFit } from "../src/renderer.js";

test("ear clipping triangulates a concave axe outline", () => {
  const polygon = [[0, 0], [2, 0], [2, 2], [1, 1], [0, 2]];
  assert.equal(triangulatePolygon(polygon).triangles.length, polygon.length - 2);
});

test("every preset produces finite nonempty geometry", () => {
  for (const preset of PRESETS) {
    const mesh = buildWeapon(preset.definition);
    assert.ok(mesh.positions.length > 0, preset.id);
    assert.ok(mesh.positions.every(Number.isFinite), preset.id);
    assert.ok(mesh.normals.every(Number.isFinite), preset.id);
    assert.ok(mesh.stats.triangles > 100, preset.id);
    const isShield = preset.definition.components.some((component) => ["roundShield", "shapedShield"].includes(component.kind));
    assert.ok(mesh.stats.dimensions[1] > (isShield ? 0.3 : 0.45), preset.id);
    assert.ok(mesh.stats.dimensions.every((value) => Number.isFinite(value) && value > 0), preset.id);
    assert.ok(mesh.stats.volume > 0, preset.id);
    for (const control of preset.controls) {
      const value = getPath(preset.definition, control.path);
      assert.ok(Number.isFinite(value), `${preset.id}: ${control.path}`);
      assert.ok(value >= control.min && value <= control.max, `${preset.id}: ${control.path}`);
    }
  }
});

test("mesh mass distribution derives realistic pommel mass and handling", () => {
  const preset = copyPreset(PRESETS.find((candidate) => candidate.id === "landsknecht-longsword"));
  const mesh = buildWeapon(preset.definition);
  const baseline = measureMassProperties(mesh, automaticGripPoint(mesh.resolvedDefinition));
  const pommelMass = baseline.components.find((component) => component.id === "pommel").massKg;
  assert.ok(baseline.massKg > 1 && baseline.massKg < 3, baseline.massKg);
  assert.ok(pommelMass > 0.1 && pommelMass < 0.5, pommelMass);
  assert.ok(pommelMass < baseline.massKg * 0.25, pommelMass / baseline.massKg);
  assert.ok(baseline.centerOfMassFromGripM > 0);
  assert.ok(baseline.momentOfInertiaKgM2 > 0);
  assert.ok(baseline.balance > 0 && baseline.balance < 1);

  const largerPommel = preset.definition.components.find((component) => component.id === "pommel");
  largerPommel.profile = largerPommel.profile.map(([height, radius]) => [height, radius * 1.08]);
  const changedMesh = buildWeapon(preset.definition);
  const changed = measureMassProperties(changedMesh, automaticGripPoint(changedMesh.resolvedDefinition));
  assert.ok(changed.centerOfMassFromGripM < baseline.centerOfMassFromGripM);
  assert.ok(changed.balance < baseline.balance);
});

test("sword presets expose a hand-center clearance below the guard", () => {
  const swords = PRESETS.filter((preset) => preset.definition.gripClearance !== undefined);
  assert.ok(swords.length >= 8);
  for (const source of swords) {
    const preset = copyPreset(source);
    const control = preset.controls.find((candidate) => candidate.path === "gripClearance");
    assert.ok(control, source.id);
    assert.equal(preset.definition.gripClearance, 0.05, source.id);
    const mesh = buildWeapon(preset.definition);
    const point = automaticGripPoint(mesh.resolvedDefinition);
    const top = mesh.resolvedDefinition._frames["grip.top"];
    assert.ok(Math.abs(Math.hypot(...point.map((value, axis) => value - top[axis])) - 0.05) < 1e-9, source.id);
  }

  const invalid = copyPreset(swords[0]);
  invalid.definition.gripClearance = 0.5;
  assert.ok(validateWeapon(invalid.definition, invalid.controls).errors.some((error) => error.includes("within the modeled grip")));
});

test("preset parameters can be independently copied and changed", () => {
  const copy = copyPreset(PRESETS[0]);
  const original = getPath(copy.definition, "shaft.length");
  setPath(copy.definition, "shaft.length", original + 0.2);
  assert.equal(getPath(copy.definition, "shaft.length"), original + 0.2);
  assert.equal(getPath(PRESETS[0].definition, "shaft.length"), original);
});

test("hafted weapons share a shaft-top mount and remain attached when resized", () => {
  const hafted = PRESETS.filter((preset) => preset.definition.shaft);
  assert.ok(hafted.length >= 10);
  for (const preset of hafted) {
    assert.ok(preset.definition.components.some((component) => (component.kind === "socket" && component.mount === "shaft-top") || (component.kind === "sleeve" && component.mount === "shaft-top-sleeve")), preset.id);
    const copy = copyPreset(preset);
    const before = buildWeapon(copy.definition).stats.bounds.max[1];
    copy.definition.shaft.length += 0.2;
    const after = buildWeapon(copy.definition).stats.bounds.max[1];
    assert.ok(Math.abs((after - before) - 0.2) < 1e-6, preset.id);
  }
});

test("the library covers reusable head families and supports a modular head swap", () => {
  const used = new Set(PRESETS.flatMap((preset) => preset.definition.components.map((component) => component.kind)));
  for (const kind of HEAD_KINDS) assert.ok(used.has(kind), kind);
  const halberd = copyPreset(PRESETS.find((preset) => preset.id === "halberd-1540"));
  const lucerne = PRESETS.find((preset) => preset.id === "lucerne-hammer");
  halberd.definition.components[1] = deepClone(lucerne.definition.components[1]);
  const mesh = buildWeapon(halberd.definition);
  assert.ok(mesh.positions.every(Number.isFinite));
  assert.ok(mesh.stats.partCount > 5);
});

test("front camera fit contains every preset on portrait and landscape canvases", () => {
  const fov = 35 * Math.PI / 180;
  for (const preset of PRESETS) {
    const bounds = buildWeapon(preset.definition).stats.bounds;
    for (const aspect of [0.55, 1, 1.8]) {
      const distance = fitDistance(bounds, aspect, fov);
      const visibleHalfHeight = distance * Math.tan(fov / 2);
      const visibleHalfWidth = visibleHalfHeight * aspect;
      assert.ok(visibleHalfHeight > (bounds.max[1] - bounds.min[1]) / 2, `${preset.id}: vertical`);
      assert.ok(visibleHalfWidth > (bounds.max[0] - bounds.min[0]) / 2, `${preset.id}: horizontal`);
    }
  }
});

test("all twenty-eight presets fit the live 1280x720 viewer canvas with margin", () => {
  assert.equal(PRESETS.length, 28);
  const canvasAspect = (1280 - 350) / (720 - 88), fov = 35 * Math.PI / 180;
  for (const preset of PRESETS) {
    const bounds = buildWeapon(preset.definition).stats.bounds, distance = fitDistance(bounds, canvasAspect, fov);
    const normalizedX = ((bounds.max[0] - bounds.min[0]) / 2) / (distance * Math.tan(fov / 2) * canvasAspect);
    const normalizedY = ((bounds.max[1] - bounds.min[1]) / 2) / (distance * Math.tan(fov / 2));
    assert.ok(normalizedX <= 0.8 && normalizedY <= 0.8, `${preset.id}: ${normalizedX}, ${normalizedY}`);
  }
});

test("shared renderer projection contains front and oblique vertices inside margin", () => {
  for (const preset of PRESETS) {
    const mesh = buildWeapon(preset.definition);
    for (const [pose, yaw, pitch] of [["front", 0, 0], ["oblique", 0.68, 0.18]]) {
      const fit = projectedFit(mesh.positions, mesh.stats.bounds, 1280 / 720, yaw, pitch);
      assert.equal(fit.contained, true, `${preset.id} ${pose}`); assert.ok(fit.maxProjected <= 0.8 + 1e-7, `${preset.id} ${pose} margin`);
    }
  }
});

test("component-local rotation reorients an interchangeable mounted head", () => {
  const base = { shaft: { length: 1, radius: 0.02 }, components: [{ kind: "hammer", label: "poll", mount: "shaft-top", offset: [0, 0, 0], length: 0.2, face: 0.08, thickness: 0.05, direction: 1 }] };
  const horizontal = buildWeapon(base).stats.dimensions;
  base.components[0].rotation = [0, 90, 0];
  const rotated = buildWeapon(base).stats.dimensions;
  assert.ok(horizontal[0] > rotated[0] * 2);
  assert.ok(rotated[2] > horizontal[2] * 2);
});

test("swept knuckle bow leaves a real aperture", () => {
  const mesh = knuckleBow({ width: 0.12, length: 0.18, bar: 0.015, thickness: 0.012, side: 1 });
  const point = [0.025, 0.09];
  let coveringTriangles = 0;
  for (const vertices of triangleVertices(mesh)) {
    const triangle = vertices.map((point) => point.slice(0, 2));
    if (contains2d(point, ...triangle)) coveringTriangles += 1;
  }
  assert.equal(coveringTriangles, 0);
});

test("compound controls remain coherent at both extremes", () => {
  for (const preset of PRESETS) for (const control of preset.controls.filter((candidate) => candidate.paths?.length > 1)) {
    const copy = copyPreset(preset);
    for (const value of [control.min, control.max]) {
      setControlValue(copy.definition, control, value);
      assert.ok(control.paths.every((path) => getPath(copy.definition, path) === value), `${preset.id}: ${control.label}`);
      assert.ok(buildWeapon(copy.definition).positions.every(Number.isFinite));
    }
  }
});

test("Kriegsspiess control cannot collapse into a short hand weapon", () => {
  const pike = PRESETS.find((preset) => preset.id === "kriegsspiess");
  assert.ok(pike.controls.find((control) => control.path === "shaft.length").min >= 3);
});

function contains2d([px, py], [ax, ay], [bx, by], [cx, cy]) {
  const cross2d = (x0, y0, x1, y1, x2, y2) => (x0 - x2) * (y1 - y2) - (x1 - x2) * (y0 - y2);
  const d1 = cross2d(px, py, ax, ay, bx, by), d2 = cross2d(px, py, bx, by, cx, cy), d3 = cross2d(px, py, cx, cy, ax, ay);
  return !((d1 < -1e-8 || d2 < -1e-8 || d3 < -1e-8) && (d1 > 1e-8 || d2 > 1e-8 || d3 > 1e-8));
}

test("figure-eight guard has two recognizable apertures", () => {
  const mesh = figureEightGuard({ width: 0.22, height: 0.055, bar: 0.009 });
  for (const point of [[-0.055, 0], [0.055, 0]]) {
    let covering = 0;
    for (const vertices of triangleVertices(mesh)) if (contains2d(point, ...vertices.map((vertex) => vertex.slice(0, 2)))) covering += 1;
    assert.equal(covering, 0);
  }
});

test("Reitschwert has compound rings, an open bow, and a sectioned straight blade", () => {
  const preset = PRESETS.find((candidate) => candidate.id === "reitschwert-1540");
  assert.ok(preset);
  assert.equal(preset.definition.components.filter((component) => component.kind === "ringGuard").length, 1);
  assert.ok(preset.definition.components.some((component) => component.kind === "knuckleBow"));
  assert.ok(preset.definition.components.filter((component) => component.label?.includes("boss")).length >= 4);
  assert.equal(preset.definition.components.at(-1).kind, "sectionBlade");
});

test("curated compact heads stay inside reference-scale breadth envelopes", () => {
  const breadth = (id) => buildWeapon(PRESETS.find((preset) => preset.id === id).definition).stats.dimensions[0];
  assert.ok(breadth("halberd-1540") >= 0.24 && breadth("halberd-1540") <= 0.28);
  assert.ok(breadth("lucerne-hammer") <= 0.27);
  assert.ok(breadth("pollaxe") <= 0.26);
  assert.ok(breadth("reiter-war-hammer") >= 0.11 && breadth("reiter-war-hammer") <= 0.16);
  assert.ok(breadth("katzbalger") <= 0.17);
});

test("glaive contours converge to one acute apex in the final quarter", () => {
  const points = glaiveOutline({ length: 0.54, width: 0.105, curvature: 0.13, root: 0.032 });
  const maximum = Math.max(...points.map((point) => point[1])), apexes = points.filter((point) => Math.abs(point[1] - maximum) < 1e-9);
  assert.equal(apexes.length, 1);
  const index = points.indexOf(apexes[0]), previous = points[index - 1], apex = points[index], next = points[index + 1];
  assert.ok(previous[1] >= maximum * 0.74 && next[1] >= maximum * 0.74);
  const a = [previous[0] - apex[0], previous[1] - apex[1]], b = [next[0] - apex[0], next[1] - apex[1]];
  const angle = Math.acos((a[0] * b[0] + a[1] * b[1]) / (Math.hypot(...a) * Math.hypot(...b))) * 180 / Math.PI;
  assert.ok(angle < 35, `apex angle ${angle}`);
});

test("glaive root shoulders flare continuously beyond the tang", () => {
  const points = glaiveOutline({ length: 0.54, width: 0.105, curvature: 0.13, root: 0.032 });
  const transition = points.filter(([, y]) => y > -0.08 && y <= 0.54 * 0.12 + 1e-9);
  assert.ok(Math.max(...transition.map(([x]) => x)) > 0.032 * 1.15);
  assert.ok(Math.min(...transition.map(([x]) => x)) < -0.032 * 1.15);
  assert.ok(transition.every((point, index) => !index || Math.hypot(point[0] - transition[index - 1][0], point[1] - transition[index - 1][1]) > 1e-6));
});

test("Messer Nagel projects 40-50 mm normal to the blade plane with a button", () => {
  const messer = PRESETS.find((preset) => preset.id === "grosse-messer"), stem = messer.definition.components.find((component) => component.label?.includes("Nagel stem")), button = messer.definition.components.find((component) => component.label?.includes("Nagel button"));
  assert.ok(stem && button);
  assert.ok(stem.points.at(-1)[1] >= 0.04 && stem.points.at(-1)[1] <= 0.05);
  assert.ok(button.profile[0][1] * 2 >= 0.01 && button.profile[0][1] * 2 <= 0.015);
});

test("hooked bill uses one continuous exposed-hook component", () => {
  const bill = PRESETS.find((preset) => preset.id === "hooked-bill"), head = bill.definition.components.find((component) => component.kind === "bill");
  assert.ok(head);
  assert.ok(head.hook >= 0.06 && head.hook <= 0.09);
  assert.equal(bill.definition.components.filter((component) => ["axe", "beak"].includes(component.kind)).length, 0);
});

test("adaptive curves honor chord and deviation budgets without redundant neighbors", () => {
  const evaluate = (t) => [t, 0.18 * Math.sin(Math.PI * 2 * t)];
  const maxChord = 0.045, maxDeviation = 0.0008;
  const points = sampleAdaptiveCurve(evaluate, { minimumSegments: 2, maxChord, maxDeviation });
  for (let index = 0; index < points.length - 1; index += 1) {
    const [a, b] = [points[index], points[index + 1]];
    assert.ok(Math.hypot(b[0] - a[0], b[1] - a[1]) <= maxChord * 1.001);
    const middle = evaluate((a[0] + b[0]) / 2);
    const dx = b[0] - a[0], dy = b[1] - a[1], denominator = dx * dx + dy * dy;
    const t = Math.max(0, Math.min(1, ((middle[0] - a[0]) * dx + (middle[1] - a[1]) * dy) / denominator));
    assert.ok(Math.hypot(middle[0] - a[0] - dx * t, middle[1] - a[1] - dy * t) <= maxDeviation * 1.001);
    assert.ok(Math.hypot(dx, dy) > 1e-8);
  }
});

test("bill hook profile is smoothly tessellated through the recurved tip", () => {
  const parameters = { length: 0.38, width: 0.09, hook: 0.08, thickness: 0.02, root: 0.03, rootLength: 0.06, bellyPosition: 0.48, hookDepth: 0.19, hookCurvature: 0.22, pointLength: 0.24 };
  const points = billHeadOutline(parameters), hookPoints = points.filter(([x]) => x >= parameters.width);
  assert.ok(hookPoints.length >= 12, `hook samples ${hookPoints.length}`);
  assert.ok(Math.max(...hookPoints.map(([x]) => x)) >= parameters.width + parameters.hook * 0.99);
  assert.equal(points.filter(([, y]) => Math.abs(y - parameters.length) < 1e-9).length, 1);
  assert.ok(points.every((point, index) => !index || Math.hypot(point[0] - points[index - 1][0], point[1] - points[index - 1][1]) > 1e-8));
});

test("every generated preset part has consistent positive winding", () => {
  for (const preset of PRESETS) {
    const mesh = buildWeapon(preset.definition);
    for (const part of mesh.parts) {
      assert.ok(signedVolume(part) >= -1e-9, `${preset.id}: ${part.label} (${signedVolume(part)})`);
      for (let index = 0; index < part.indices.length; index += 3) {
        const [a, b, c] = part.indices.slice(index, index + 3).map((vertex) => part.positions.slice(vertex * 3, vertex * 3 + 3));
        const stored = part.normals.slice(part.indices[index] * 3, part.indices[index] * 3 + 3);
        const ab = b.map((value, axis) => value - a[axis]), ac = c.map((value, axis) => value - a[axis]);
        const geometric = [ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2], ab[0] * ac[1] - ab[1] * ac[0]];
        const magnitude = Math.hypot(...geometric);
        if (magnitude > 1e-10) assert.ok(geometric.reduce((sum, value, axis) => sum + value / magnitude * stored[axis], 0) > 0, `${preset.id}: ${part.label} normal`);
      }
    }
    assert.ok(signedVolume(mesh) > 0, preset.id);
  }
});

function assertClosedTriangleMesh(mesh, context) {
  const precision = 1e7;
  const vertexKey = (values, offset) => values.slice(offset, offset + 3).map((value) => Math.round(value * precision)).join(",");
  const edges = new Map();
  for (let index = 0; index < mesh.indices.length; index += 3) {
    const vertices = mesh.indices.slice(index, index + 3).map((vertex) => vertexKey(mesh.positions, vertex * 3));
    for (const [from, to] of [[vertices[0], vertices[1]], [vertices[1], vertices[2]], [vertices[2], vertices[0]]]) {
      const key = from < to ? `${from}|${to}` : `${to}|${from}`;
      const incidence = edges.get(key) ?? { forward: 0, reverse: 0 };
      if (from < to) incidence.forward += 1; else incidence.reverse += 1;
      edges.set(key, incidence);
    }
  }
  const boundaries = [...edges.entries()].filter(([, incidence]) => incidence.forward !== 1 || incidence.reverse !== 1);
  assert.equal(boundaries.length, 0, `${context}: ${boundaries.length} non-manifold or boundary edges; first ${boundaries[0]?.[0] ?? "none"}`);
}

test("every generated component is a closed oriented two-manifold", () => {
  for (const preset of PRESETS) {
    const mesh = buildWeapon(preset.definition);
    for (const part of mesh.parts) assertClosedTriangleMesh(part, `${preset.id}: ${part.label}`);
  }
});

test("flanged-mace parameter space stays finite, closed, and oriented", () => {
  const source = PRESETS.find((preset) => preset.id === "flanged-mace");
  const controls = Object.fromEntries(source.controls.map((control) => [control.label, control]));
  const levels = (control) => [control.min, (control.min + control.max) / 2, control.max];
  for (const flangeCount of levels(controls["Flange count"])) for (const headLength of levels(controls["Head length"])) for (const concavity of levels(controls["Side concavity"])) {
    const preset = copyPreset(source);
    for (const control of preset.controls) setControlValue(preset.definition, control, (control.min + control.max) / 2);
    setControlValue(preset.definition, controls["Flange count"], Math.round(flangeCount));
    setControlValue(preset.definition, controls["Head length"], headLength);
    setControlValue(preset.definition, controls["Side concavity"], concavity);
    const mesh = buildWeapon(preset.definition);
    assert.ok(mesh.positions.every(Number.isFinite));
    assert.ok(signedVolume(mesh) > 0);
    for (const part of mesh.parts) assertClosedTriangleMesh(part, `mace ${flangeCount}/${headLength}/${concavity}: ${part.label}`);
  }
  for (const endpoint of ["min", "max"]) {
    const preset = copyPreset(source);
    for (const control of preset.controls) setControlValue(preset.definition, control, control[endpoint]);
    const mesh = buildWeapon(preset.definition);
    assert.ok(mesh.positions.every(Number.isFinite), endpoint);
    for (const part of mesh.parts) assertClosedTriangleMesh(part, `mace all-${endpoint}: ${part.label}`);
  }
});

test("sampled mace flange concavity bows increasingly below straight chords", () => {
  const base = { length: 0.25, rootRadius: 0.009, shoulderRadius: 0.0065, cuspRadius: 0.06, cuspHeight: 0.75, profileSamples: 10 };
  const deviations = [];
  for (const concavity of [0.15, 0.5, 0.92]) {
    const outline = maceFlangeOutline({ ...base, concavity });
    assert.ok(outline.length >= base.profileSamples * 2 + 1);
    const cuspY = -base.length / 2 + base.length * base.cuspHeight, targetY = (-base.length / 2 + cuspY) / 2;
    const midpoint = outline.slice(0, outline.findIndex((point) => Math.abs(point[0] - base.cuspRadius) < 1e-9) + 1).reduce((best, point) => Math.abs(point[1] - targetY) < Math.abs(best[1] - targetY) ? point : best);
    const normalizedY = (midpoint[1] + base.length / 2) / (cuspY + base.length / 2), chordRadius = base.rootRadius + (base.cuspRadius - base.rootRadius) * normalizedY;
    deviations.push(chordRadius - midpoint[0]);
    assert.ok(outline.filter((point) => Math.abs(point[0] - base.cuspRadius) < 1e-9).length === 1);
  }
  assert.ok(deviations[0] > 0);
  assert.ok(deviations[1] > deviations[0] * 1.5);
  assert.ok(deviations[2] > deviations[1]);
});

test("Gothic mace endpoint keeps its polished crown, curve, and private dark grip", () => {
  const gothic = PRESETS.find((preset) => preset.id === "gothic-flanged-mace"), head = gothic.definition.components[4], grip = gothic.definition.components[0];
  assert.equal(head.crownLength, 0.015);
  assert.equal(head.concavity, 0.92);
  assert.equal(grip.material, "darkLeather");
  assert.equal(PRESETS.find((preset) => preset.id === "flanged-mace").definition.components[0].material, "leather");
});

function deepClone(value) { return JSON.parse(JSON.stringify(value)); }

function componentAtEndpoint(id, kind, endpoint) {
  const preset = copyPreset(PRESETS.find((candidate) => candidate.id === id));
  if (endpoint !== "default") for (const control of preset.controls) setControlValue(preset.definition, control, control[endpoint]);
  return preset.definition.components.find((component) => component.kind === kind);
}

function angleBetween(a, b) {
  return Math.acos(Math.max(-1, Math.min(1, (a[0] * b[0] + a[1] * b[1]) / (Math.hypot(...a) * Math.hypot(...b))))) * 180 / Math.PI;
}

function cubicPoint([p0, p1, p2, p3], t) {
  const u = 1 - t;
  return [u ** 3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t ** 3 * p3[0], u ** 3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t ** 3 * p3[1]];
}

function pointToSegment(point, a, b) {
  const dx = b[0] - a[0], dy = b[1] - a[1], denominator = dx * dx + dy * dy;
  const t = denominator ? Math.max(0, Math.min(1, ((point[0] - a[0]) * dx + (point[1] - a[1]) * dy) / denominator)) : 0;
  return Math.hypot(point[0] - a[0] - dx * t, point[1] - a[1] - dy * t);
}

function assertCubicSampling(spans, quality, context) {
  for (const span of spans) {
    const sampled = sampleCubicBezier(span.points, quality);
    for (let index = 0; index < sampled.length - 1; index += 1) assert.ok(Math.hypot(sampled[index + 1][0] - sampled[index][0], sampled[index + 1][1] - sampled[index][1]) <= quality.maxChord * 1.001, `${context} ${span.name}: chord`);
    for (let index = 0; index <= 256; index += 1) {
      const reference = cubicPoint(span.points, index / 256);
      const deviation = Math.min(...sampled.slice(0, -1).map((point, segment) => pointToSegment(reference, point, sampled[segment + 1])));
      assert.ok(deviation <= quality.maxDeviation * 1.08, `${context} ${span.name}: deviation ${deviation}`);
    }
  }
}

test("bill cubic joins remain co-directed and smooth at default and endpoint shapes", () => {
  for (const endpoint of ["default", "min", "max"]) {
    const component = componentAtEndpoint("hooked-bill", "bill", endpoint), spans = billHeadCurveSpans(component);
    for (let index = 0; index < spans.length - 1; index += 1) {
      const join = spans[index].end;
      if (["apex", "hookTip"].includes(join)) continue;
      const incoming = [spans[index].points[3][0] - spans[index].points[2][0], spans[index].points[3][1] - spans[index].points[2][1]];
      const outgoing = [spans[index + 1].points[1][0] - spans[index + 1].points[0][0], spans[index + 1].points[1][1] - spans[index + 1].points[0][1]];
      assert.ok(incoming[0] * outgoing[0] + incoming[1] * outgoing[1] > 0, `${endpoint} ${join}: reversed tangent`);
      assert.ok(angleBetween(incoming, outgoing) < 1e-4, `${endpoint} ${join}: ${angleBetween(incoming, outgoing)} degree turn`);
      assert.ok(Math.abs(Math.hypot(...incoming) - Math.hypot(...outgoing)) < 1e-10, `${endpoint} ${join}: unequal C1 handles`);
    }
    assertCubicSampling(spans, { minimumSegments: 3, maxChord: component.length / 28, maxDeviation: Math.min(component.width, component.hook) / 90 }, `bill ${endpoint}`);
  }
});

test("glaive spine join remains tangent-continuous at default and endpoint shapes", () => {
  for (const endpoint of ["default", "min", "max"]) {
    const component = componentAtEndpoint("glaive", "glaive", endpoint), spans = glaiveSpineCurveSpans(component);
    const incoming = [spans[0].points[3][0] - spans[0].points[2][0], spans[0].points[3][1] - spans[0].points[2][1]];
    const outgoing = [spans[1].points[1][0] - spans[1].points[0][0], spans[1].points[1][1] - spans[1].points[0][1]];
    assert.ok(incoming[0] * outgoing[0] + incoming[1] * outgoing[1] > 0, endpoint);
    assert.ok(angleBetween(incoming, outgoing) < 1e-4, `${endpoint}: ${angleBetween(incoming, outgoing)} degree turn`);
    assert.ok(Math.abs(Math.hypot(...incoming) - Math.hypot(...outgoing)) < 1e-10, `${endpoint}: unequal C1 handles`);
    assertCubicSampling(spans, { minimumSegments: 12, maxChord: component.length / 28, maxDeviation: component.width / 180 }, `glaive ${endpoint}`);
  }
});

test("round swept bars satisfy physical cross-section chord and sagitta budgets", () => {
  for (const radius of [0.004, 0.006, 0.009, 0.014, 0.025]) for (const requested of [8, 12, 24]) {
    const segments = tubeRadialSegments(radius, requested), chord = 2 * radius * Math.sin(Math.PI / segments), sagitta = radius * (1 - Math.cos(Math.PI / segments));
    assert.ok(segments >= Math.max(12, requested));
    assert.ok(chord <= 0.006 + 1e-12, `${radius}/${requested}: chord ${chord}`);
    assert.ok(sagitta <= 0.0003 + 1e-12, `${radius}/${requested}: sagitta ${sagitta}`);
    const mesh = tubePath([[0, 0], [0, 0.1]], radius, "steel", [0, 0, 0], "test bar", requested);
    assert.equal(mesh.indices.length / 3, segments * 4);
  }
});
