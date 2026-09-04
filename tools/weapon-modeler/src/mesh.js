import { detailSamples, detailError, roundSegments, withDetail } from "./detail.js";
import { indexTriangles, triangleVertices } from "./topology.js";
import { cross, dot, length, normalize, subtract } from "./math.js";
import { projectedFit } from "./renderer.js";
import { effectiveGripRadius, MAX_ROUND_GRIP_RADIUS_M, MAX_SWORD_GRIP_THICKNESS_M, MAX_SWORD_GRIP_WIDTH_M } from "./anatomy.js";

export const MATERIALS = {
  steel: { color: [0.58, 0.62, 0.64], density: 7850 },
  darkSteel: { color: [0.29, 0.31, 0.31], density: 7850 },
  wood: { color: [0.34, 0.19, 0.085], density: 720 },
  leather: { color: [0.24, 0.11, 0.055], density: 920 },
  darkLeather: { color: [0.055, 0.045, 0.038], density: 920 },
  brass: { color: [0.58, 0.43, 0.18], density: 8500 },
};

function signedArea(points) {
  return (
    points.reduce((sum, point, index) => {
      const next = points[(index + 1) % points.length];
      return sum + point[0] * next[1] - next[0] * point[1];
    }, 0) / 2
  );
}

function pointInTriangle(point, a, b, c) {
  const sign = (p1, p2, p3) => (p1[0] - p3[0]) * (p2[1] - p3[1]) - (p2[0] - p3[0]) * (p1[1] - p3[1]);
  const d1 = sign(point, a, b),
    d2 = sign(point, b, c),
    d3 = sign(point, c, a);
  return !((d1 < 0 || d2 < 0 || d3 < 0) && (d1 > 0 || d2 > 0 || d3 > 0));
}

function orientation2d(a, b, c) {
  return (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
}
function segmentsIntersect(a, b, c, d) {
  const abC = orientation2d(a, b, c),
    abD = orientation2d(a, b, d),
    cdA = orientation2d(c, d, a),
    cdB = orientation2d(c, d, b);
  return abC * abD < -1e-12 && cdA * cdB < -1e-12;
}
export function simplePolygonErrors(points, label = "outline") {
  const errors = [];
  if (!Array.isArray(points) || points.length < 3) return [`${label}: requires at least three points`];
  if (points.some((point) => !Array.isArray(point) || point.length !== 2 || !point.every(Number.isFinite))) return [`${label}: contains malformed/non-finite points`];
  for (let index = 0; index < points.length; index += 1) {
    const next = (index + 1) % points.length;
    if (Math.hypot(points[next][0] - points[index][0], points[next][1] - points[index][1]) < 1e-8) errors.push(`${label}: zero-length edge ${index}`);
    for (let other = index + 1; other < points.length; other += 1) {
      const otherNext = (other + 1) % points.length;
      if (other === index || other === next || otherNext === index) continue;
      if (segmentsIntersect(points[index], points[next], points[other], points[otherNext])) errors.push(`${label}: edges ${index} and ${other} intersect`);
    }
  }
  return errors;
}

export function triangulatePolygon(input) {
  const extent = Math.max(1, ...input.flatMap((point) => point.map(Math.abs)));
  const minimumDistance = extent * 1e-9,
    minimumArea = extent * extent * 1e-10;
  let points = input.filter((point, index) => index === 0 || Math.hypot(point[0] - input[index - 1][0], point[1] - input[index - 1][1]) > minimumDistance);
  if (points.length > 1 && Math.hypot(points[0][0] - points.at(-1)[0], points[0][1] - points.at(-1)[1]) <= minimumDistance) points.pop();
  let changed = true;
  while (changed && points.length > 3) {
    changed = false;
    points = points.filter((point, index) => {
      const previous = points[(index - 1 + points.length) % points.length],
        next = points[(index + 1) % points.length];
      const area = Math.abs(orientation2d(previous, point, next));
      const between = (point[0] - previous[0]) * (next[0] - point[0]) + (point[1] - previous[1]) * (next[1] - point[1]) >= 0;
      if (area <= minimumArea && between) {
        changed = true;
        return false;
      }
      return true;
    });
  }
  if (signedArea(points) < 0) points.reverse();
  const remaining = points.map((_, index) => index);
  const triangles = [];
  let guard = points.length * points.length;
  while (remaining.length > 3 && guard-- > 0) {
    let clipped = false;
    for (let cursor = 0; cursor < remaining.length; cursor += 1) {
      const previous = remaining[(cursor - 1 + remaining.length) % remaining.length];
      const current = remaining[cursor];
      const next = remaining[(cursor + 1) % remaining.length];
      const a = points[previous],
        b = points[current],
        c = points[next];
      const convex = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]) > 1e-12;
      if (!convex) continue;
      if (remaining.some((candidate) => candidate !== previous && candidate !== current && candidate !== next && pointInTriangle(points[candidate], a, b, c))) continue;
      triangles.push([previous, current, next]);
      remaining.splice(cursor, 1);
      clipped = true;
      break;
    }
    if (!clipped) break;
  }
  if (remaining.length === 3) triangles.push([...remaining]);
  return { points, triangles };
}

function makeBuilder(materialName, label) {
  const material = MATERIALS[materialName] ?? MATERIALS.steel;
  const positions = [],
    normals = [],
    colors = [], groups = [];
  function triangle(a, b, c, surface = 0) {
    groups.push(surface);
    const normal = normalize(cross(subtract(b, a), subtract(c, a)));
    for (const point of [a, b, c]) {
      positions.push(...point);
      normals.push(...normal);
      colors.push(...material.color);
    }
  }
  return { positions, normals, colors, groups, triangle, material, label };
}

function finish(builder) {
  const mesh = {
    positions: builder.positions,
    normals: builder.normals,
    colors: builder.colors,
    material: builder.material,
    label: builder.label,
  };
  if (signedVolume({ ...mesh, indices: Array.from({ length: mesh.positions.length / 3 }, (_, i) => i) }) < -1e-10) {
    for (let index = 0; index < mesh.positions.length; index += 9) {
      for (const values of [mesh.positions, mesh.normals, mesh.colors]) for (let axis = 0; axis < 3; axis += 1) [values[index + 3 + axis], values[index + 6 + axis]] = [values[index + 6 + axis], values[index + 3 + axis]];
      for (let normalIndex = index; normalIndex < index + 9; normalIndex += 1) mesh.normals[normalIndex] *= -1;
    }
  }
  return { ...mesh, ...indexTriangles(mesh, builder.groups) };
}

export function signedVolume(mesh) {
  let volume = 0;
  for (const [a, b, c] of triangleVertices(mesh)) {
    volume += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6;
  }
  return volume;
}

function move(point, offset) {
  return [point[0] + offset[0], point[1] + offset[1], point[2] + offset[2]];
}

function componentRange(component) {
  if (component.kind === "roundShield") return [-component.radius, component.radius];
  if (component.kind === "shapedShield") return [-component.height / 2 - (component.bottomDepth ?? 0), component.height / 2 + (component.topDepth ?? 0)];
  if (["grip", "ovalGrip", "slabGrip", "blade", "sectionBlade", "diamondBlade", "spear", "fork", "partisan", "glaive", "bill", "sleeve"].includes(component.kind)) return [0, component.length ?? 0];
  if (component.kind === "guard") return [-(component.height ?? 0.02) / 2, (component.height ?? 0.02) / 2];
  if (["ringGuard", "figureEight"].includes(component.kind)) return [-(component.height ?? component.radius ?? component.width * 0.15), component.height ?? component.radius ?? component.width * 0.15];
  if (component.kind === "knuckleBow" || component.kind === "tube") return [0, component.length ?? component.points?.at(-1)?.[1] ?? 0];
  if (component.kind === "pommel") {
    if ((component.construction === "composite" ? component.baseConstruction : component.construction) === "lathed")
      return [Math.min(...component.profile.map((point) => point[0])), Math.max(...component.profile.map((point) => point[0]))];
    return [0, (component.height ?? 0.06) * (component.lengthScale ?? 1)];
  }
  if (component.kind === "guardAssembly") {
    const ys = Object.values(component.nodes).map((point) => point[1]);
    return [Math.min(...ys), Math.max(...ys)];
  }
  if (component.kind === "socket") return [Math.min(...component.profile.map((point) => point[0])), Math.max(...component.profile.map((point) => point[0]))];
  if (component.kind === "mace") return [-component.length / 2, component.length / 2 + (component.crownLength ?? 0)];
  if (component.kind === "collar") return [-component.width / 2, component.width / 2];
  if (component.kind === "box") return [-component.size[1] / 2, component.size[1] / 2];
  return [0, 0];
}

const COMPONENT_KINDS = new Set(["blade", "sectionBlade", "diamondBlade", "axe", "spear", "guard", "guardAssembly", "knuckleBow", "ringGuard", "tube", "figureEight", "fork", "partisan", "glaive", "facetedBeak", "bill", "box", "pick", "beak", "hammer", "socket", "pommel", "collar", "sleeve", "mace", "grip", "ovalGrip", "slabGrip", "roundShield", "shapedShield"]);
const MOUNT_MODES = new Set(["shaft-top", "shaft-top-centered", "shaft-top-sleeve", "component-end"]);
const COMMON_COMPONENT_KEYS = new Set("kind id label offset rotation material mount attach stretchBetween anchor insertion".split(" "));
const KIND_KEYS = Object.fromEntries(
  Object.entries({
    blade: "length width thickness curvature taper singleEdge tipWidth belly",
    sectionBlade: "length width thickness taper section",
    diamondBlade: "length width thickness taper",
    axe: "width height thickness beard curvature side rootWidth upperShoulder lowerShoulder flare toe heel beardDrop upperCusp lowerCusp",
    spear: "length width thickness shoulder rootWidth bellyPosition acuteness",
    guard: "width height thickness sweep controlWidth tipScale terminalSwell mirrorMode leftLength rightLength leftSweep rightSweep leftSet rightSet section sectionWidth sectionDepth sectionTwist terminal leftTerminal rightTerminal terminalSize",
    guardAssembly: "nodes members plates anchorNode nodeBindings",
    knuckleBow: "width length bar thickness side bulge samples radialSegments",
    ringGuard: "radius bar arcStart arcEnd samples radialSegments",
    tube: "points radius radialSegments",
    figureEight: "width height bar samples radialSegments",
    fork: "length width baseWidth thickness crotch tineWidth tineTaper shoulderBlend crotchRound",
    partisan: "length width lugWidth thickness lugDrop bellyPosition rootWidth lugSweep acuteness",
    glaive: "length width thickness curvature root edgeCurvature spineCurvature bellyPosition pointLength rootLength",
    facetedBeak: "length root tip thickness direction set bendPosition tipThickness",
    bill: "length width hook thickness root rootLength bellyPosition hookDepth hookCurvature pointLength",
    box: "size fitShaftSide",
    pick: "length radius direction",
    beak: "length radius direction curvature thickness rootSection tipSection bendPosition droop",
    hammer: "length face neck thickness direction crown neckRatio faceFlare crownLength faceThickness",
    socket: "profile segments fitShaft wall",
    pommel: "construction baseConstruction profile segments widthScale lengthScale height diameter thickness faceConvexity rimBevel facets fluteCount fluteDepth twist outlineStyle notchDepth lobeSpread shoulderWidth sockets ornaments",
    collar: "width radius segments",
    sleeve: "length radius topRadius segments fitShaft wall",
    mace: "length rootRadius shoulderRadius cuspRadius cuspHeight concavity crownLength flanges flangeThickness profileSamples segments waist flangeDepth",
    grip: "length radius bottomScale topScale wraps segments",
    ovalGrip: "length width thickness bottomScale topScale segments",
    slabGrip: "length width thickness scaleThickness",
    roundShield: "radius thickness rings radialSegments outerCurve centerCurve centerRadius rimRadius bossRadius bossHeight fittingMode fittingAngle mirrored gripLength gripRadius fittingSpacing fittingClearance strapWidth strapThickness rimMaterial bossMaterial gripMaterial strapMaterial",
    shapedShield: "width height thickness edgeSegments topShape bottomShape topDepth bottomDepth topRoundness bottomRoundness sideTaper cornerRadius cylindricalCurve centerCurve centerWidth centerHeight rimRadius bossRadius bossHeight fittingMode fittingAngle mirrored gripLength gripRadius fittingSpacing fittingClearance strapWidth strapThickness rimMaterial bossMaterial gripMaterial strapMaterial",
  }).map(([kind, keys]) => [kind, new Set(keys.split(" "))]),
);
const SHAFT_KEYS = new Set(["length", "radius", "topScale", "bottomScale", "segments", "material"]);
const INTEGER_FIELDS = new Set(["segments", "radialSegments", "samples", "profileSamples", "flanges", "wraps", "rings", "edgeSegments"]);
const NON_NUMERIC_FIELDS = new Set(["kind", "id", "label", "material", "mount", "attach", "stretchBetween", "anchor", "anchorNode", "nodeBindings", "offset", "rotation", "profile", "size", "points", "section", "construction", "baseConstruction", "sockets", "ornaments", "outlineStyle", "mirrorMode", "terminal", "leftTerminal", "rightTerminal", "nodes", "members", "plates", "fitShaft", "fitShaftSide", "controlWidth", "topShape", "bottomShape", "fittingMode", "mirrored", "rimMaterial", "bossMaterial", "gripMaterial", "strapMaterial"]);
const REQUIRED_FIELDS = {
  blade: ["length", "width", "thickness"],
  sectionBlade: ["length", "width", "thickness"],
  diamondBlade: ["length", "width", "thickness"],
  axe: ["width", "height", "thickness"],
  spear: ["length", "width", "thickness"],
  guard: ["width", "height", "thickness"],
  guardAssembly: ["nodes", "members"],
  knuckleBow: ["width", "length"],
  ringGuard: ["radius"],
  tube: ["points", "radius"],
  figureEight: ["width"],
  fork: ["length", "width", "baseWidth", "thickness"],
  partisan: ["length", "width", "lugWidth", "thickness"],
  glaive: ["length", "width", "thickness"],
  facetedBeak: ["length", "root", "thickness"],
  bill: ["length", "width", "hook", "thickness"],
  box: ["size"],
  pick: ["length", "radius"],
  beak: ["length", "radius"],
  hammer: ["length", "face", "neck", "thickness"],
  socket: ["profile"],
  pommel: ["construction"],
  collar: ["width", "radius"],
  sleeve: ["length", "radius"],
  mace: ["length", "rootRadius", "shoulderRadius", "cuspRadius", "flanges", "flangeThickness"],
  grip: ["length", "radius"],
  ovalGrip: ["length", "width", "thickness"],
  slabGrip: ["length", "width", "thickness"],
  roundShield: ["radius", "thickness", "rings", "radialSegments", "outerCurve", "centerCurve", "centerRadius", "fittingMode"],
  shapedShield: ["width", "height", "thickness", "edgeSegments", "topShape", "bottomShape", "topDepth", "bottomDepth", "topRoundness", "bottomRoundness", "sideTaper", "cornerRadius", "cylindricalCurve", "centerCurve", "centerWidth", "centerHeight", "fittingMode"],
};

function schemaErrors(input) {
  const errors = [];
  if (!input || typeof input !== "object" || Array.isArray(input)) return ["definition must be an object"];
  for (const key of Object.keys(input)) if (!["shaft", "components", "gripClearance"].includes(key)) errors.push(`definition: unknown field ${key}`);
  if (!Array.isArray(input.components)) return ["definition.components must be an array"];
  if (input.gripClearance !== undefined && (!Number.isFinite(input.gripClearance) || input.gripClearance < 0)) errors.push("definition.gripClearance must be a non-negative finite distance");
  if (input.shaft !== undefined && (!input.shaft || typeof input.shaft !== "object" || !Number.isFinite(input.shaft.length) || !Number.isFinite(input.shaft.radius))) errors.push("shaft requires finite length and radius");
  if (input.shaft && typeof input.shaft === "object") {
    for (const key of Object.keys(input.shaft)) if (!SHAFT_KEYS.has(key)) errors.push(`shaft: unknown field ${key}`);
    if (input.shaft.material !== undefined && !Object.hasOwn(MATERIALS, input.shaft.material)) errors.push(`shaft: unknown material ${input.shaft.material}`);
    for (const key of ["length", "radius", "topScale", "bottomScale"]) if (input.shaft[key] !== undefined && !Number.isFinite(input.shaft[key])) errors.push(`shaft.${key} must be finite`);
    if (input.shaft.segments !== undefined && (!Number.isInteger(input.shaft.segments) || input.shaft.segments < 3)) errors.push("shaft.segments must be an integer of at least 3");
  }
  input.components.forEach((component, index) => {
    const prefix = `components[${index}]`;
    if (!component || typeof component !== "object" || Array.isArray(component)) {
      errors.push(`${prefix} must be an object`);
      return;
    }
    const allowed = new Set([...COMMON_COMPONENT_KEYS, ...(KIND_KEYS[component.kind] ?? [])]);
    for (const key of Object.keys(component)) if (!allowed.has(key)) errors.push(`${prefix}: ${component.kind ?? "unknown"} does not allow field ${key}`);
    if (!COMPONENT_KINDS.has(component.kind)) errors.push(`${prefix}: unknown component kind ${String(component.kind)}`);
    if (component.mount !== undefined && !MOUNT_MODES.has(component.mount)) errors.push(`${prefix}: unknown mount mode ${component.mount}`);
    if (component.material !== undefined && !Object.hasOwn(MATERIALS, component.material)) errors.push(`${prefix}: unknown material ${component.material}`);
    for (const [key, value] of Object.entries(component)) if (!NON_NUMERIC_FIELDS.has(key) && value !== undefined && !Number.isFinite(value)) errors.push(`${prefix}.${key} must be finite`);
    for (const key of INTEGER_FIELDS) {
      const minimum = key === "wraps" ? 0 : key === "samples" ? 4 : 3;
      if (component[key] !== undefined && (!Number.isInteger(component[key]) || component[key] < minimum)) errors.push(`${prefix}.${key} must be an integer of at least ${minimum}`);
    }
    if (component.fitShaft !== undefined && typeof component.fitShaft !== "boolean") errors.push(`${prefix}.fitShaft must be boolean`);
    if (component.fitShaftSide !== undefined && typeof component.fitShaftSide !== "boolean") errors.push(`${prefix}.fitShaftSide must be boolean`);
    if (component.mirrored !== undefined && typeof component.mirrored !== "boolean") errors.push(`${prefix}.mirrored must be boolean`);
    if (["roundShield", "shapedShield"].includes(component.kind)) {
      if (!new Set(["grip", "grip-and-strap"]).has(component.fittingMode)) errors.push(`${prefix}.fittingMode must be grip or grip-and-strap`);
      for (const key of ["rimMaterial", "bossMaterial", "gripMaterial", "strapMaterial"])
        if (component[key] !== undefined && !Object.hasOwn(MATERIALS, component[key])) errors.push(`${prefix}: unknown ${key} ${component[key]}`);
    }
    if (component.kind === "shapedShield") {
      if (!new Set(["flat", "rounded", "singlePeak", "doublePeak"]).has(component.topShape)) errors.push(`${prefix}.topShape is not supported`);
      if (!new Set(["flat", "rounded", "point"]).has(component.bottomShape)) errors.push(`${prefix}.bottomShape is not supported`);
    }
    if (component.kind === "pommel") {
      if (!new Set(["lathed", "plate", "faceted", "writhen", "outline", "composite"]).has(component.construction)) errors.push(`${prefix}.construction is not supported`);
      const baseConstruction = component.construction === "composite" ? component.baseConstruction : component.construction;
      if (baseConstruction === "lathed" && component.profile === undefined) errors.push(`${prefix}.profile is required for a lathed pommel`);
      if (["plate", "faceted", "writhen", "outline"].includes(baseConstruction) && (!(component.height > 0) || !(component.diameter > 0) || !(component.thickness > 0))) errors.push(`${prefix}: constructed pommel requires positive height, diameter, and thickness`);
      if (component.construction === "composite") {
        if (!["lathed", "plate", "faceted", "writhen", "outline"].includes(component.baseConstruction)) errors.push(`${prefix}.baseConstruction must be a simple pommel construction`);
        if (!component.sockets || Object.values(component.sockets).some((point) => !Array.isArray(point) || point.length !== 3 || !point.every(Number.isFinite))) errors.push(`${prefix}.sockets must map names to finite 3D positions`);
        if (!Array.isArray(component.ornaments)) errors.push(`${prefix}.ornaments must be an array`);
        else for (const ornament of component.ornaments) {
          for (const key of Object.keys(ornament)) if (!["style", "socket", "scale", "rotation", "material", "smooth", "positions", "indices"].includes(key)) errors.push(`${prefix}.ornaments: unknown field ${key}`);
          if (!component.sockets?.[ornament.socket]) errors.push(`${prefix}: missing ornament socket ${ornament.socket}`);
          if (!["crown", "escutcheon", "authored"].includes(ornament.style)) errors.push(`${prefix}: unsupported ornament style ${ornament.style}`);
          if (!(ornament.scale > 0)) errors.push(`${prefix}: ornament scale must be positive`);
          if (ornament.rotation !== undefined && (!Array.isArray(ornament.rotation) || ornament.rotation.length !== 3 || !ornament.rotation.every(Number.isFinite))) errors.push(`${prefix}: ornament rotation must have three finite degrees`);
          if (ornament.material !== undefined && !Object.hasOwn(MATERIALS, ornament.material)) errors.push(`${prefix}: unknown ornament material ${ornament.material}`);
          if (ornament.style === "authored" && (!Array.isArray(ornament.positions) || ornament.positions.length % 3 || !ornament.positions.every(Number.isFinite) || !Array.isArray(ornament.indices) || ornament.indices.length % 3 || ornament.indices.some((index) => !Number.isInteger(index) || index < 0 || index >= ornament.positions.length / 3))) errors.push(`${prefix}: authored ornament needs finite positions and triangle indices`);
        }
      }
    }
    if (component.kind === "guard") {
      if (!GUARD_SECTIONS.has(component.section ?? "round")) errors.push(`${prefix}.section is not supported`);
      for (const key of ["terminal", "leftTerminal", "rightTerminal"]) if (component[key] !== undefined && !GUARD_TERMINALS.has(component[key]) && !(key !== "terminal" && component[key] === "shared")) errors.push(`${prefix}.${key} is not supported`);
      if (!new Set(["symmetric", "opposed", "independent"]).has(component.mirrorMode ?? "opposed")) errors.push(`${prefix}.mirrorMode is not supported`);
    }
    if (component.kind === "guardAssembly") {
      if (!component.nodes || typeof component.nodes !== "object" || Object.values(component.nodes ?? {}).some((point) => !Array.isArray(point) || point.length !== 3 || !point.every(Number.isFinite))) errors.push(`${prefix}.nodes must map names to finite 3D points`);
      if (!Array.isArray(component.members) || component.members.some((member) => !Array.isArray(member.path) || member.path.length < 2 || member.path.some((name) => typeof name !== "string"))) errors.push(`${prefix}.members need named-node paths`);
      else {
        const names = new Set(Object.keys(component.nodes ?? {})), links = new Map([...names].map((name) => [name, new Set()]));
        if (!names.has(component.anchorNode)) errors.push(`${prefix}.anchorNode must name an assembly node`);
        for (const [name, binding] of Object.entries(component.nodeBindings ?? {})) {
          if (!names.has(name)) errors.push(`${prefix}.nodeBindings references missing node ${name}`);
          for (const key of Object.keys(binding)) if (!["frame", "between", "t", "offset"].includes(key)) errors.push(`${prefix}.nodeBindings.${name}: unknown field ${key}`);
          if (Boolean(binding.frame) === Boolean(binding.between)) errors.push(`${prefix}.nodeBindings.${name} needs exactly one frame or between binding`);
          if (binding.between && (!Array.isArray(binding.between) || binding.between.length !== 2 || binding.between.some((target) => !names.has(target) || component.nodeBindings?.[target]?.between))) errors.push(`${prefix}.nodeBindings.${name}: between needs two direct nodes`);
          if (binding.t !== undefined && (!(binding.t >= 0) || !(binding.t <= 1))) errors.push(`${prefix}.nodeBindings.${name}.t must be within 0–1`);
          if (binding.offset !== undefined && (!Array.isArray(binding.offset) || binding.offset.length !== 3 || !binding.offset.every(Number.isFinite))) errors.push(`${prefix}.nodeBindings.${name}.offset must be three finite numbers`);
        }
        for (const [memberIndex, member] of component.members.entries()) {
          for (const key of Object.keys(member)) if (!["label", "path", "section", "sectionWidth", "sectionDepth", "sectionTwist", "radialSegments", "material", "tipScale", "terminalSwell"].includes(key)) errors.push(`${prefix}.members[${memberIndex}]: unknown field ${key}`);
          if (!GUARD_SECTIONS.has(member.section ?? "round")) errors.push(`${prefix}.members[${memberIndex}].section is not supported`);
          if (!(member.sectionWidth > 0) || !(member.sectionDepth > 0)) errors.push(`${prefix}.members[${memberIndex}] section dimensions must be positive`);
          for (let i = 0; i < member.path.length; i++) {
            if (!names.has(member.path[i])) errors.push(`${prefix}.members[${memberIndex}] references missing node ${member.path[i]}`);
            if (i) { links.get(member.path[i - 1])?.add(member.path[i]); links.get(member.path[i])?.add(member.path[i - 1]); }
          }
        }
        const connected = new Set(), pending = names.has(component.anchorNode) ? [component.anchorNode] : [];
        while (pending.length) { const name = pending.pop(); if (connected.has(name)) continue; connected.add(name); pending.push(...(links.get(name) ?? [])); }
        const memberNodes = new Set(component.members.flatMap((member) => member.path));
        if ([...memberNodes].some((name) => !connected.has(name))) errors.push(`${prefix}.members must form one connected graph from anchorNode`);
        for (const [plateIndex, plate] of (component.plates ?? []).entries()) {
          for (const key of Object.keys(plate)) if (!["outline", "cutout", "thickness", "material", "dishDepth", "rimRadius"].includes(key)) errors.push(`${prefix}.plates[${plateIndex}]: unknown field ${key}`);
          if (!Array.isArray(plate.outline) || plate.outline.length < 3 || !Array.isArray(plate.cutout) || plate.cutout.length !== plate.outline.length) errors.push(`${prefix}.plates[${plateIndex}] needs equal outline and cutout loops`);
          else if ([...plate.outline, ...(plate.cutout ?? [])].some((name) => !names.has(name))) errors.push(`${prefix}.plates[${plateIndex}] references a missing node`);
          if (!(plate.thickness > 0)) errors.push(`${prefix}.plates[${plateIndex}].thickness must be positive`);
          for (const key of ["dishDepth", "rimRadius"]) if (plate[key] !== undefined && (!Number.isFinite(plate[key]) || plate[key] < 0 || plate[key] > 0.03)) errors.push(`${prefix}.plates[${plateIndex}].${key} must be within 0–0.03 m`);
        }
      }
    }
    if (component.offset !== undefined && (!Array.isArray(component.offset) || component.offset.length !== 3 || !component.offset.every(Number.isFinite))) errors.push(`${prefix}.offset must be three finite numbers`);
    if (component.rotation !== undefined && (!Array.isArray(component.rotation) || component.rotation.length !== 3 || !component.rotation.every(Number.isFinite))) errors.push(`${prefix}.rotation must be three finite numbers`);
    if (component.size !== undefined && (!Array.isArray(component.size) || component.size.length !== 3 || !component.size.every(Number.isFinite))) errors.push(`${prefix}.size must be three finite numbers`);
    if (component.profile !== undefined && (!Array.isArray(component.profile) || component.profile.length < 2 || component.profile.some((point) => !Array.isArray(point) || point.length !== 2 || !point.every(Number.isFinite)))) errors.push(`${prefix}.profile must contain finite [height,radius] pairs`);
    if (component.points !== undefined && (!Array.isArray(component.points) || component.points.length < 2 || component.points.some((point) => !Array.isArray(point) || point.length !== 2 || !point.every(Number.isFinite)))) errors.push(`${prefix}.points must contain finite 2D points`);
    if (component.attach && !["base", "center", "top", undefined].includes(component.attach.at)) errors.push(`${prefix}: unknown attachment anchor ${component.attach.at}`);
    if (component.attach && typeof component.attach === "object") for (const key of Object.keys(component.attach)) if (!["to", "at", "offset", "overlap"].includes(key)) errors.push(`${prefix}.attach: unknown field ${key}`);
    if (component.attach !== undefined && (!component.attach || typeof component.attach !== "object" || Array.isArray(component.attach) || typeof component.attach.to !== "string")) errors.push(`${prefix}.attach requires a target frame`);
    if (component.attach?.offset !== undefined && (!Array.isArray(component.attach.offset) || component.attach.offset.length !== 3 || !component.attach.offset.every(Number.isFinite))) errors.push(`${prefix}.attach.offset must be three finite numbers`);
    if (component.attach?.overlap !== undefined && (!Number.isFinite(component.attach.overlap) || component.attach.overlap < 0)) errors.push(`${prefix}.attach.overlap must be a non-negative finite number`);
    const placementCount = [component.mount !== undefined, component.attach !== undefined, component.stretchBetween !== undefined].filter(Boolean).length;
    if (placementCount > 1) errors.push(`${prefix}: mount, attach, and stretchBetween are mutually exclusive placement declarations`);
    if (component.stretchBetween !== undefined) {
      const framePattern = /^(weapon\.root|shaft\.(bottom|top)|[A-Za-z0-9 _-]+\.(origin|base|bottom|top|center))$/;
      if (component.kind !== "knuckleBow") errors.push(`${prefix}: stretchBetween is only supported by knuckleBow`);
      if (!Array.isArray(component.stretchBetween) || component.stretchBetween.length !== 2 || !component.stretchBetween.every((frame) => typeof frame === "string" && framePattern.test(frame)) || component.stretchBetween[0] === component.stretchBetween[1]) errors.push(`${prefix}.stretchBetween requires two distinct attachment frame strings`);
    }
    for (const field of REQUIRED_FIELDS[component.kind] ?? []) if (component[field] === undefined) errors.push(`${prefix}: ${component.kind} requires ${field}`);
  });
  return errors;
}

export function resolveDefinition(input) {
  const definition = JSON.parse(JSON.stringify(input)),
    frames = new Map(),
    errors = [],
    ids = new Set();
  frames.set("weapon.root", [0, 0, 0]);
  if (definition.shaft) {
    frames.set("shaft.bottom", [0, 0, 0]);
    frames.set("shaft.top", [0, definition.shaft.length, 0]);
  }
  const axe = definition.components.find((part) => part.kind === "axe" && part.mount === "shaft-top");
  if (axe) for (const rear of definition.components.filter((part) => ["beak", "facetedBeak", "hammer"].includes(part.kind) && part.mount === "shaft-top")) {
    rear.direction = -(axe.side ?? 1);
    if (rear.offset) rear.offset[0] = Math.abs(rear.offset[0]) * rear.direction;
  }
  for (let index = 0; index < definition.components.length; index += 1) {
    const component = definition.components[index],
      id = component.id ?? component.label ?? `component-${index}`;
    component.id = id;
    if (ids.has(id)) errors.push(`${id}: duplicate component id`);
    ids.add(id);
    if (definition.shaft && component.kind === "box" && component.fitShaftSide) component.offset[0] = Math.sign(component.offset[0] || 1) * (definition.shaft.radius * (definition.shaft.topScale ?? 0.92) + component.size[0] / 2 - 0.002);
    const local = component.offset ?? [0, 0, 0];
    if (definition.shaft && ["socket", "sleeve"].includes(component.kind) && component.mount?.startsWith("shaft-top") && component.fitShaft !== false) {
      const contactRadius = definition.shaft.radius * (definition.shaft.topScale ?? 0.92),
        wall = component.wall ?? 0.003,
        outer = contactRadius + wall;
      component.fitShaft = true;
      component.wall = wall;
      component._shaftContactRadius = contactRadius;
      if (component.kind === "socket") component.profile = component.profile.map(([y]) => [y, outer]);
      else {
        component.radius = outer;
        component.topRadius = outer;
      }
    }
    if (component.kind === "pommel" && component.profile) component.profile = component.profile.map(([y, radius]) => [y * (component.lengthScale ?? 1), radius * (component.widthScale ?? 1)]);
    const range = componentRange(component);
    if (component.mount === "component-end" && !component.attach) component.attach = { to: `${component.anchor}.top`, at: "center" };
    if (component.mount?.startsWith("shaft-top")) {
      const shaftTop = frames.get("shaft.top");
      if (!shaftTop) errors.push(`${id}: shaft-top mount requires a shaft`);
      else {
        let y = shaftTop[1] + local[1];
        if (component.mount === "shaft-top-centered") y += component.length / 2 - (component.insertion ?? 0.012);
        if (component.mount === "shaft-top-sleeve") y -= (component.insertion ?? 0.012) + component.length;
        component.offset = [shaftTop[0] + local[0], y, shaftTop[2] + local[2]];
        const anchorY = component.mount === "shaft-top-centered" ? range[0] : component.mount === "shaft-top-sleeve" ? range[1] : 0;
        const anchor = rotatePoint([0, anchorY, 0], component.rotation),
          contact = move(anchor, component.offset),
          expected = [shaftTop[0] + local[0], shaftTop[1] + local[1] - (component.insertion ?? 0), shaftTop[2] + local[2]];
        component._resolvedAttachment = {
          target: "shaft.top",
          distance: length(subtract(contact, expected)),
          overlap: component.insertion ?? 0,
          contact,
          expected,
        };
      }
    }
    if (component.stretchBetween) {
      const from = frames.get(component.stretchBetween[0]),
        to = frames.get(component.stretchBetween[1]);
      if (!from || !to) errors.push(`${id}: missing stretch frame ${!from ? component.stretchBetween[0] : component.stretchBetween[1]}`);
      else {
        if (to[1] <= from[1]) errors.push(`${id}: stretch target must lie above its source`);
        component.offset = [from[0] + local[0], from[1] + local[1], from[2] + local[2]];
        component.length = Math.max(0.001, to[1] - from[1]);
        const start = move(rotatePoint([0, 0, 0], component.rotation), component.offset),
          end = move(rotatePoint([(component.bar ?? 0.012) * (component.side ?? 1), component.length, 0], component.rotation), component.offset);
        component._resolvedStretch = {
          from: component.stretchBetween[0],
          to: component.stretchBetween[1],
          start,
          end,
          fromTarget: from,
          toTarget: to,
        };
      }
    } else if (component.attach) {
      const target = frames.get(component.attach.to);
      if (!target) errors.push(`${id}: missing attachment frame ${component.attach.to}`);
      else {
        const at = component.attach.at ?? "base",
          localY = at === "center" ? (range[0] + range[1]) / 2 : at === "top" ? range[1] : range[0];
        const delta = component.attach.offset ?? [0, 0, 0],
          overlap = component.attach.overlap ?? 0;
        const localAnchor = rotatePoint(component.kind === "guardAssembly" ? component.nodes[component.anchorNode] : [0, localY, 0], component.rotation),
          expected = [target[0] + delta[0], target[1] + delta[1] - overlap, target[2] + delta[2]];
        component.offset = subtract(expected, localAnchor);
        const contact = move(localAnchor, component.offset);
        component._resolvedAttachment = {
          target: component.attach.to,
          distance: length(subtract(contact, expected)),
          overlap,
          contact,
          expected,
        };
      }
    }
    if (component.kind === "guardAssembly") {
      for (const [name, binding] of Object.entries(component.nodeBindings ?? {})) if (binding.frame) {
        const frame = frames.get(binding.frame);
        if (!frame) errors.push(`${id}.${name}: missing node frame ${binding.frame}`);
        else component.nodes[name] = move(inverseRotatePoint(subtract(frame, component.offset), component.rotation), binding.offset ?? [0, 0, 0]);
      }
      for (const [name, binding] of Object.entries(component.nodeBindings ?? {})) if (binding.between) {
        const [a, b] = binding.between.map((node) => component.nodes[node]);
        component.nodes[name] = move(a.map((value, axis) => value + (b[axis] - value) * (binding.t ?? 0.5)), binding.offset ?? [0, 0, 0]);
      }
    }
    const origin = component.offset ?? [0, 0, 0],
      resolvedRange = componentRange(component);
    const base = move(rotatePoint([0, resolvedRange[0], 0], component.rotation), origin),
      top = move(rotatePoint([0, resolvedRange[1], 0], component.rotation), origin),
      center = move(rotatePoint([0, (resolvedRange[0] + resolvedRange[1]) / 2, 0], component.rotation), origin);
    frames.set(`${id}.origin`, origin);
    frames.set(`${id}.base`, base);
    frames.set(`${id}.bottom`, base);
    frames.set(`${id}.top`, top);
    frames.set(`${id}.center`, center);
    if (["roundShield", "shapedShield"].includes(component.kind)) {
      const layout = shieldFittingLayout(component),
        [x, y] = layout.gripCenter,
        localGrip = [x, y, shieldSurfaceZ(component, x, y, false) - component.fittingClearance],
        grip = move(rotatePoint(localGrip, component.rotation), origin);
      frames.set(`${id}.grip`, grip);
      frames.set("shield.grip", grip);
    }
  }
  definition._resolutionErrors = errors;
  definition._frames = Object.fromEntries(frames);
  return definition;
}

function rotatePoint([x, y, z], rotation = [0, 0, 0]) {
  const [rx, ry, rz] = rotation.map((degrees) => (degrees * Math.PI) / 180);
  let a = y * Math.cos(rx) - z * Math.sin(rx),
    b = y * Math.sin(rx) + z * Math.cos(rx);
  y = a;
  z = b;
  a = x * Math.cos(ry) + z * Math.sin(ry);
  b = -x * Math.sin(ry) + z * Math.cos(ry);
  x = a;
  z = b;
  a = x * Math.cos(rz) - y * Math.sin(rz);
  b = x * Math.sin(rz) + y * Math.cos(rz);
  x = a;
  y = b;
  return [x, y, z];
}

function inverseRotatePoint([x, y, z], rotation = [0, 0, 0]) {
  const [rx, ry, rz] = rotation.map((degrees) => (-degrees * Math.PI) / 180);
  let a = x * Math.cos(rz) - y * Math.sin(rz),
    b = x * Math.sin(rz) + y * Math.cos(rz);
  x = a;
  y = b;
  a = x * Math.cos(ry) + z * Math.sin(ry);
  b = -x * Math.sin(ry) + z * Math.cos(ry);
  x = a;
  z = b;
  a = y * Math.cos(rx) - z * Math.sin(rx);
  b = y * Math.sin(rx) + z * Math.cos(rx);
  y = a;
  z = b;
  return [x, y, z];
}

export function transformMesh(mesh, rotation = [0, 0, 0], offset = [0, 0, 0]) {
  const result = { ...mesh, positions: [], normals: [] };
  for (let index = 0; index < mesh.positions.length; index += 3) result.positions.push(...move(rotatePoint(mesh.positions.slice(index, index + 3), rotation), offset));
  for (let index = 0; index < mesh.normals.length; index += 3) result.normals.push(...normalize(rotatePoint(mesh.normals.slice(index, index + 3), rotation)));
  return result;
}

export function prism(points2d, thickness, material = "steel", offset = [0, 0, 0], label = "prism") {
  const builder = makeBuilder(material, label);
  const outlineErrors = simplePolygonErrors(points2d, label);
  if (outlineErrors.length) throw new Error(outlineErrors.join("; "));
  const { points, triangles } = triangulatePolygon(points2d);
  if (triangles.length !== points.length - 2 || triangles.some(([a, b, c]) => Math.abs(orientation2d(points[a], points[b], points[c])) < 1e-12)) throw new Error(`${label}: triangulation is incomplete or degenerate`);
  const half = thickness / 2;
  for (const [a, b, c] of triangles) {
    builder.triangle(move([points[a][0], points[a][1], half], offset), move([points[b][0], points[b][1], half], offset), move([points[c][0], points[c][1], half], offset));
    builder.triangle(move([points[c][0], points[c][1], -half], offset), move([points[b][0], points[b][1], -half], offset), move([points[a][0], points[a][1], -half], offset));
  }
  for (let index = 0; index < points.length; index += 1) {
    const next = (index + 1) % points.length;
    const a = move([points[index][0], points[index][1], -half], offset);
    const b = move([points[next][0], points[next][1], -half], offset);
    const c = move([points[next][0], points[next][1], half], offset);
    const d = move([points[index][0], points[index][1], half], offset);
    builder.triangle(a, b, c);
    builder.triangle(a, c, d);
  }
  return finish(builder);
}

export function lathe(profile, segments = 14, material = "wood", offset = [0, 0, 0], label = "lathe", radialScale = 1, exactSegments = false) {
  const largestRadius = Math.max(...profile.map((point) => point[1]));
  if (!(exactSegments && segments <= 8)) segments = roundSegments(largestRadius, segments);
  const builder = makeBuilder(material, label);
  let band = 1;
  for (let ring = 0; ring < profile.length - 1; ring += 1) {
    if (ring > 0) {
      const slope = (i) => Math.atan2(profile[i + 1][1] - profile[i][1], profile[i + 1][0] - profile[i][0]);
      if (Math.abs(slope(ring) - slope(ring - 1)) > Math.PI / 3) band++;
    }
    for (let segment = 0; segment < segments; segment += 1) {
      const a0 = (segment / segments) * Math.PI * 2;
      const a1 = ((segment + 1) / segments) * Math.PI * 2;
      const [y0, r0] = profile[ring],
        [y1, r1] = profile[ring + 1];
      const a = move([Math.cos(a0) * r0, y0, Math.sin(a0) * r0 * radialScale], offset);
      const b = move([Math.cos(a1) * r0, y0, Math.sin(a1) * r0 * radialScale], offset);
      const c = move([Math.cos(a1) * r1, y1, Math.sin(a1) * r1 * radialScale], offset);
      const d = move([Math.cos(a0) * r1, y1, Math.sin(a0) * r1 * radialScale], offset);
      builder.triangle(a, c, b, exactSegments && segments <= 8 ? 0 : `lathe:${band}`);
      builder.triangle(a, d, c, exactSegments && segments <= 8 ? 0 : `lathe:${band}`);
    }
  }
  const cap = (profileIndex, reverse) => {
    const [y, radius] = profile[profileIndex];
    for (let segment = 0; segment < segments; segment += 1) {
      const a0 = (segment / segments) * Math.PI * 2,
        a1 = ((segment + 1) / segments) * Math.PI * 2;
      const center = move([0, y, 0], offset);
      const a = move([Math.cos(a0) * radius, y, Math.sin(a0) * radius * radialScale], offset);
      const b = move([Math.cos(a1) * radius, y, Math.sin(a1) * radius * radialScale], offset);
      reverse ? builder.triangle(center, a, b) : builder.triangle(center, b, a);
    }
  };
  cap(0, true);
  cap(profile.length - 1, false);
  return finish(builder);
}

export function box(size, material, offset, label = "box") {
  const [x, y, z] = size.map((value) => value / 2);
  return prism(
    [
      [-x, -y],
      [x, -y],
      [x, y],
      [-x, y],
    ],
    z * 2,
    material,
    offset,
    label,
  );
}

function hollowSocket(profile, innerRadius, material, label) {
  const builder = makeBuilder(material, label), segments = roundSegments(Math.max(...profile.map((p) => p[1])));
  const point = (row, segment, inner) => [Math.cos(segment / segments * Math.PI * 2) * (inner ? innerRadius : profile[row][1]), profile[row][0], Math.sin(segment / segments * Math.PI * 2) * (inner ? innerRadius : profile[row][1])];
  for (const inner of [false, true]) for (let row = 0; row < profile.length - 1; row++) for (let segment = 0; segment < segments; segment++) {
    const next = (segment + 1) % segments, a = point(row, segment, inner), b = point(row, next, inner), c = point(row + 1, next, inner), d = point(row + 1, segment, inner);
    const group = inner ? "bore" : "socket";
    inner ? (builder.triangle(a, b, c, group), builder.triangle(a, c, d, group)) : (builder.triangle(a, c, b, group), builder.triangle(a, d, c, group));
  }
  for (const row of [0, profile.length - 1]) for (let segment = 0; segment < segments; segment++) {
    const next = (segment + 1) % segments, a = point(row, segment, false), b = point(row, next, false), c = point(row, next, true), d = point(row, segment, true);
    row === 0 ? (builder.triangle(a, b, c), builder.triangle(a, c, d)) : (builder.triangle(a, c, b), builder.triangle(a, d, c));
  }
  return finish(builder);
}

function shapedPlate(points, thicknessAt, material, offset, label) {
  const base = prism(points, 1, material, offset, label), builder = makeBuilder(material, label);
  for (let index = 0; index < base.indices.length; index += 3) {
    const ids = base.indices.slice(index, index + 3), normal = base.normals.slice(ids[0] * 3, ids[0] * 3 + 3);
    const vertices = ids.map((id) => {
      const point = base.positions.slice(id * 3, id * 3 + 3);
      point[2] = offset[2] + (point[2] - offset[2]) * thicknessAt(point[0] - offset[0], point[1] - offset[1]);
      return point;
    });
    builder.triangle(...vertices, Math.abs(normal[2]) > 0.9 ? `plate:${Math.sign(normal[2])}` : 0);
  }
  return finish(builder);
}

function roundedPlate(outline, thickness, material, offset, label, bevelFraction = 0.14) {
  const { points, triangles } = triangulatePolygon(outline), builder = makeBuilder(material, label);
  const center = [0, (Math.min(...points.map((p) => p[1])) + Math.max(...points.map((p) => p[1]))) / 2];
  const vertex = (index, side, inset) => move([center[0] + (points[index][0] - center[0]) * (inset ? 1 - bevelFraction : 1), center[1] + (points[index][1] - center[1]) * (inset ? 1 - bevelFraction : 1), side * thickness * (inset ? 0.5 : 0.28)], offset);
  for (const side of [-1, 1]) {
    for (const [a, b, c] of triangles) {
      const vertices = [a, b, c].map((index) => vertex(index, side, true));
      if (side < 0) vertices.reverse();
      builder.triangle(...vertices);
    }
    for (let i = 0; i < points.length; i++) {
      const j = (i + 1) % points.length, a = vertex(i, side, false), b = vertex(j, side, false), c = vertex(j, side, true), d = vertex(i, side, true);
      side > 0 ? (builder.triangle(a, b, c, "bevel-front"), builder.triangle(a, c, d, "bevel-front")) : (builder.triangle(a, c, b, "bevel-back"), builder.triangle(a, d, c, "bevel-back"));
    }
  }
  for (let i = 0; i < points.length; i++) {
    const j = (i + 1) % points.length, a = vertex(i, -1, false), b = vertex(j, -1, false), c = vertex(j, 1, false), d = vertex(i, 1, false);
    builder.triangle(a, b, c, "perimeter"); builder.triangle(a, c, d, "perimeter");
  }
  return finish(builder);
}

export function sampleAdaptiveCurve(evaluate, { minimumSegments = 8, maxChord = Infinity, maxDeviation = Infinity, maxDepth = 8 } = {}) {
  minimumSegments = detailSamples(minimumSegments, 2);
  maxChord = detailError(maxChord);
  maxDeviation = detailError(maxDeviation);
  const samples = [];
  const refine = (t0, p0, t1, p1, depth) => {
    const midpointT = (t0 + t1) / 2,
      midpoint = evaluate(midpointT);
    const chord = Math.hypot(p1[0] - p0[0], p1[1] - p0[1]);
    const deviation = Math.max(pointSegmentDistance(evaluate(t0 * 0.75 + t1 * 0.25), p0, p1), pointSegmentDistance(midpoint, p0, p1), pointSegmentDistance(evaluate(t0 * 0.25 + t1 * 0.75), p0, p1));
    if (depth < maxDepth && (chord > maxChord || deviation > maxDeviation)) {
      refine(t0, p0, midpointT, midpoint, depth + 1);
      refine(midpointT, midpoint, t1, p1, depth + 1);
    } else samples.push(p1);
  };
  const segments = Math.max(1, Math.floor(minimumSegments));
  samples.push(evaluate(0));
  for (let index = 0; index < segments; index += 1) {
    const t0 = index / segments,
      t1 = (index + 1) / segments;
    refine(t0, samples.at(-1), t1, evaluate(t1), 0);
  }
  const finiteQuality = [maxChord, maxDeviation].filter(Number.isFinite);
  const minimumSpacing = Math.max(1e-10, (finiteQuality.length ? Math.min(...finiteQuality) : 1) * 1e-7);
  const compact = [samples[0]];
  for (const point of samples.slice(1, -1)) if (Math.hypot(point[0] - compact.at(-1)[0], point[1] - compact.at(-1)[1]) > minimumSpacing) compact.push(point);
  const endpoint = samples.at(-1);
  if (Math.hypot(endpoint[0] - compact.at(-1)[0], endpoint[1] - compact.at(-1)[1]) <= minimumSpacing && compact.length > 1) compact[compact.length - 1] = endpoint;
  else compact.push(endpoint);
  return compact;
}

export function sampleCubicBezier([p0, p1, p2, p3], quality) {
  return sampleAdaptiveCurve((t) => {
    const u = 1 - t;
    return [u ** 3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t ** 3 * p3[0], u ** 3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t ** 3 * p3[1]];
  }, quality);
}

function appendCurve(target, points) {
  target.push(...points.slice(target.length ? 1 : 0));
  return target;
}

export function coneX(lengthValue, radius, direction, material, offset, label = "spike") {
  const builder = makeBuilder(material, label);
  const segments = 12,
    baseX = offset[0],
    tipX = offset[0] + lengthValue * direction;
  for (let segment = 0; segment < segments; segment += 1) {
    const a0 = (segment / segments) * Math.PI * 2,
      a1 = ((segment + 1) / segments) * Math.PI * 2;
    const a = [baseX, offset[1] + Math.cos(a0) * radius, offset[2] + Math.sin(a0) * radius];
    const b = [baseX, offset[1] + Math.cos(a1) * radius, offset[2] + Math.sin(a1) * radius];
    const tip = [tipX, offset[1], offset[2]];
    direction > 0 ? builder.triangle(a, b, tip) : builder.triangle(b, a, tip);
  }
  const center = [baseX, offset[1], offset[2]];
  for (let segment = 0; segment < segments; segment += 1) {
    const a0 = (segment / segments) * Math.PI * 2,
      a1 = ((segment + 1) / segments) * Math.PI * 2;
    const a = [baseX, offset[1] + Math.cos(a0) * radius, offset[2] + Math.sin(a0) * radius],
      b = [baseX, offset[1] + Math.cos(a1) * radius, offset[2] + Math.sin(a1) * radius];
    direction > 0 ? builder.triangle(center, a, b) : builder.triangle(center, b, a);
  }
  return finish(builder);
}

export function curvedBeak(parameters, offset = [0, 0, 0], label = "curved beak") {
  return prism(curvedBeakOutline(parameters), parameters.thickness ?? parameters.radius * 0.75, parameters.material ?? "steel", offset, label);
}

export function curvedBeakOutline(parameters) {
  const { length: beakLength, radius, direction = -1, curvature = 0.16, thickness = radius * 0.75 } = parameters;
  const rootSection = parameters.rootSection ?? radius * 1.5,
    tipSection = parameters.tipSection ?? radius * 0.12;
  const bendPosition = parameters.bendPosition ?? 0.55,
    droop = parameters.droop ?? curvature * 0.35;
  const edge = (sign) => (t) => {
    const x = direction * beakLength * t;
    const bendExponent = Math.log(0.5) / Math.log(Math.max(0.15, Math.min(0.85, bendPosition)));
    const bend = Math.pow(t, bendExponent);
    const y = curvature * bend + droop * t,
      half = (rootSection * (1 - t) + tipSection * t) / 2;
    return [x, y + sign * half];
  };
  const quality = {
    minimumSegments: 12,
    maxChord: beakLength / 22,
    maxDeviation: Math.max(curvature, rootSection) / 180,
  };
  const upper = sampleAdaptiveCurve(edge(1), quality),
    lower = sampleAdaptiveCurve(edge(-1), quality);
  return [...upper, ...lower.reverse()];
}

export function hammerPoll(parameters, offset = [0, 0, 0], label = "hammer poll") {
  const { length: pollLength, face = 0.09, neck = face * 0.55, thickness = face, direction = 1, crown = 0.12 } = parameters;
  const s = direction,
    neckRatio = Math.max(0.05, Math.min(0.88, parameters.neckRatio ?? (parameters.neckLength ? parameters.neckLength / pollLength : 0.72))),
    neckLength = pollLength * neckRatio,
    faceFlare = parameters.faceFlare ?? 0;
  const faceHeight = face * (1 + faceFlare),
    crownReach = Math.max(0, parameters.crownLength ?? pollLength * crown);
  return prism(
    [
      [0, -neck / 2],
      [neckLength * s, -neck / 2],
      [pollLength * s, -faceHeight / 2],
      [(pollLength + crownReach) * s, 0],
      [pollLength * s, faceHeight / 2],
      [neckLength * s, neck / 2],
      [0, neck / 2],
    ],
    parameters.faceThickness ?? thickness,
    parameters.material ?? "steel",
    offset,
    label,
  );
}

export function curvedBlade(parameters, offset = [0, 0, 0], label = "blade") {
  return prism(curvedBladeOutline(parameters), parameters.thickness, "steel", offset, label);
}

export function curvedBladeOutline(parameters) {
  const { length: bladeLength, width, thickness, curvature = 0, taper = 1.25, singleEdge = 0, tipWidth = 0.025, belly = 0 } = parameters;
  const edge = (side) => (t) => {
    const center = curvature * t * t;
    const taperWidth = tipWidth + (1 - tipWidth) * Math.pow(1 - t, taper);
    const halfWidth = width * 0.5 * taperWidth * (1 + belly * Math.sin(Math.PI * t));
    return side < 0 ? [center - halfWidth * (1 - singleEdge), t * bladeLength] : [center + halfWidth * (1 + singleEdge), t * bladeLength];
  };
  const quality = {
    minimumSegments: 16,
    maxChord: bladeLength / 28,
    maxDeviation: Math.max(width, Math.abs(curvature)) / 220,
  };
  const left = sampleAdaptiveCurve(edge(-1), quality),
    right = sampleAdaptiveCurve(edge(1), quality);
  return [...left, ...right.reverse()];
}

export function axeHead(parameters, offset = [0, 0, 0], label = "axe head") {
  const side = parameters.side ?? 1, root = parameters.rootWidth ?? parameters.width * 0.18;
  return shapedPlate(axeHeadOutline(parameters), (x, y) => {
    const t = Math.max(0, Math.min(1, (0.42 - y / parameters.height) / 0.9));
    const edgeX = parameters.width * (0.82 + (parameters.flare ?? 0) * (t - 0.5) + (parameters.curvature ?? 0.12) * Math.sin(Math.PI * t));
    const remaining = 1 - Math.max(0, Math.min(1, (side * x + root) / (edgeX + root)));
    return 0.0006 + (parameters.thickness - 0.0006) * remaining;
  }, "steel", offset, label);
}

export function axeHeadOutline(parameters) {
  const { width, height, thickness, beard = 0.35, curvature = 0.12, side = 1 } = parameters;
  const s = side;
  const socket = parameters.rootWidth ?? Math.min(width * 0.18, 0.055),
    flare = parameters.flare ?? 0;
  const upperShoulder = parameters.upperShoulder ?? 0.38,
    lowerShoulder = parameters.lowerShoulder ?? 0.26;
  const toe = parameters.toe ?? 0,
    heel = parameters.heel ?? 0,
    beardDrop = parameters.beardDrop ?? beard * 0.45;
  const edge = sampleAdaptiveCurve(
    (t) => {
      const y = height * (0.42 - t * 0.9);
      return [width * (0.82 + flare * (t - 0.5) + curvature * Math.sin(Math.PI * t)) * s, y];
    },
    {
      minimumSegments: 8,
      maxChord: Math.max(width, height) / 18,
      maxDeviation: width / 220,
    },
  );
  const points = [[-socket * s, height * upperShoulder], [width * 0.3 * s, height * (upperShoulder - (parameters.upperCusp ?? 0))], [width * (0.68 + flare * 0.12) * s, height * (0.48 + toe)], ...edge, [width * beard * s, -height * (0.34 + beardDrop + heel)], [width * 0.18 * s, -height * (lowerShoulder - (parameters.lowerCusp ?? 0))], [-socket * s, -height * lowerShoulder]];
  return s < 0 ? points.reverse() : points;
}

export function diamondBlade(parameters, offset = [0, 0, 0], label = "diamond blade") {
  const builder = makeBuilder(parameters.material ?? "steel", label),
    samples = detailSamples(14);
  for (let index = 0; index < samples; index += 1) {
    const t0 = index / samples,
      t1 = (index + 1) / samples;
    const y0 = t0 * parameters.length,
      y1 = t1 * parameters.length;
    const w0 = parameters.width * 0.5 * (1 - t0 * (parameters.taper ?? 0.92)),
      w1 = parameters.width * 0.5 * (1 - t1 * (parameters.taper ?? 0.92));
    const d0 = parameters.thickness * 0.5 * (1 - t0 * 0.7),
      d1 = parameters.thickness * 0.5 * (1 - t1 * 0.7);
    const rings = [
      [
        [-w0, y0, 0],
        [0, y0, d0],
        [w0, y0, 0],
        [0, y0, -d0],
      ],
      [
        [-w1, y1, 0],
        [0, y1, d1],
        [w1, y1, 0],
        [0, y1, -d1],
      ],
    ];
    for (let side = 0; side < 4; side += 1) {
      const next = (side + 1) % 4;
      builder.triangle(move(rings[0][side], offset), move(rings[0][next], offset), move(rings[1][next], offset));
      builder.triangle(move(rings[0][side], offset), move(rings[1][next], offset), move(rings[1][side], offset));
    }
  }
  return finish(builder);
}

export function fanPommel(parameters, offset = [0, 0, 0], label = "fan pommel") {
  const { width, height, thickness } = parameters;
  const points = [[-width * 0.18, 0]];
  for (let index = 0; index < 12; index += 1) {
    const t = index / 12;
    points.push([-width * (0.18 + 0.32 * Math.sin((t * Math.PI) / 2)), height * (0.04 + 0.36 * t)]);
  }
  for (let index = 0; index <= 24; index += 1) {
    const angle = Math.PI - (index / 24) * Math.PI;
    points.push([(Math.cos(angle) * width) / 2, height * 0.4 + Math.sin(angle) * height * 0.6]);
  }
  for (let index = 11; index >= 0; index -= 1) {
    const t = index / 12;
    points.push([width * (0.18 + 0.32 * Math.sin((t * Math.PI) / 2)), height * (0.04 + 0.36 * t)]);
  }
  points.push([width * 0.18, 0]);
  return roundedPlate(points.map(([x, y]) => [x, height - y]).reverse(), thickness, parameters.material ?? "steel", offset, label);
}

export function facetedBeak(parameters, offset = [0, 0, 0], label = "faceted beak") {
  const { length: beakLength, root, tip = root * 0.16, thickness, direction = -1, set = 0 } = parameters,
    s = direction;
  const bendPosition = parameters.bendPosition ?? 0.22,
    bend = Math.max(0.12, Math.min(0.7, bendPosition));
  return prism(
    [
      [0, -root / 2],
      [beakLength * bend * s, set * bend - root * 0.42],
      [beakLength * s, set - tip / 2],
      [beakLength * s, set + tip / 2],
      [beakLength * bend * s, set * bend + root * 0.42],
      [0, root / 2],
    ],
    parameters.tipThickness ?? thickness,
    parameters.material ?? "steel",
    offset,
    label,
  );
}

export function billHead(parameters, offset = [0, 0, 0], label = "forged bill") {
  return prism(billHeadOutline(parameters), parameters.thickness, parameters.material ?? "steel", offset, label);
}

export function billHeadCurveSpans(parameters) {
  const { length: headLength, width, hook, thickness, root = 0.032 } = parameters;
  const rootLength = parameters.rootLength ?? 0.06,
    belly = parameters.bellyPosition ?? 0.48,
    hookDepth = parameters.hookDepth ?? 0.19,
    hookCurve = parameters.hookCurvature ?? 0.22,
    pointLength = parameters.pointLength ?? 0.24;
  const rootLeft = [-root, -rootLength],
    apex = [0, headLength],
    shoulder = [width, headLength * 0.68];
  const hookUpper = [width + hook * 0.72, headLength * (0.68 + hookCurve * 0.55)],
    hookTip = [width + hook, headLength * (0.68 - hookDepth)];
  const hookInner = [width + hook * 0.46, headLength * (0.62 + hookCurve * 0.2)],
    rootRight = [root, -rootLength];
  const shoulderHandle = Math.min(width, hook) * 0.16,
    crownHandle = hook * 0.18,
    innerHandle = hook * 0.16;
  return [
    {
      name: "body-to-apex",
      points: [rootLeft, [-root * 0.92, headLength * (1 - belly) * 0.42], [-root * 0.52, headLength * 0.84], apex],
      end: "apex",
    },
    {
      name: "apex-to-shoulder",
      points: [apex, [width * 0.1, headLength * (0.97 - pointLength * 0.08)], [shoulder[0] - shoulderHandle, shoulder[1]], shoulder],
      start: "apex",
      end: "shoulder",
    },
    {
      name: "shoulder-to-hook-crown",
      points: [shoulder, [shoulder[0] + shoulderHandle, shoulder[1]], [hookUpper[0] - crownHandle, hookUpper[1]], hookUpper],
      start: "shoulder",
      end: "hookUpper",
    },
    {
      name: "hook-crown-to-tip",
      points: [hookUpper, [hookUpper[0] + crownHandle, hookUpper[1]], [hookTip[0], headLength * (0.62 - hookDepth * 0.25)], hookTip],
      start: "hookUpper",
      end: "hookTip",
    },
    {
      name: "tip-to-hook-inner",
      points: [hookTip, [hookTip[0], headLength * (0.56 - hookDepth)], [hookInner[0] + innerHandle, hookInner[1]], hookInner],
      start: "hookTip",
      end: "hookInner",
    },
    {
      name: "hook-inner-to-root",
      points: [hookInner, [hookInner[0] - innerHandle, hookInner[1]], [width * 0.62, headLength * belly * 0.28], rootRight],
      start: "hookInner",
    },
  ];
}

export function billHeadOutline(parameters) {
  const { length: headLength, width, hook } = parameters;
  const quality = {
    minimumSegments: 3,
    maxChord: headLength / 28,
    maxDeviation: Math.min(width, hook) / 90,
  };
  const points = [];
  for (const span of billHeadCurveSpans(parameters)) appendCurve(points, sampleCubicBezier(span.points, quality));
  return points;
}

export function maceFlangeOutline(component) {
  const half = component.length / 2,
    rootRadius = component.rootRadius,
    shoulderRadius = component.shoulderRadius;
  const cuspRadius = component.cuspRadius,
    cuspY = -half + component.length * component.cuspHeight;
  const samples = component.profileSamples ?? 10,
    exponent = 1.03 + Math.max(0, Math.min(0.98, component.concavity ?? 0)) * 2.97;
  const quality = {
    minimumSegments: samples,
    maxChord: component.length / 22,
    maxDeviation: (cuspRadius - Math.min(rootRadius, shoulderRadius)) / 150,
  };
  const lower = sampleAdaptiveCurve((t) => [rootRadius + (cuspRadius - rootRadius) * Math.pow(t, exponent), -half + (cuspY + half) * t], quality);
  const upper = sampleAdaptiveCurve((t) => [shoulderRadius + (cuspRadius - shoulderRadius) * Math.pow(1 - t, exponent), cuspY + (half - cuspY) * t], quality);
  const points = [...lower, ...upper.slice(1)];
  return points;
}

export function maceFlangeMesh(component, material = "steel", label = "mace flange") {
  const outer = maceFlangeOutline(component),
    builder = makeBuilder(material, label),
    halfThickness = component.flangeThickness / 2;
  const inner = outer.map(([, y]) => {
    const t = (y + component.length / 2) / component.length;
    return [(component.rootRadius + (component.shoulderRadius - component.rootRadius) * t) * 0.55, y];
  });
  const point = ([radius, y], z) => [radius, y, z];
  for (let index = 0; index < outer.length - 1; index += 1) {
    const o0 = outer[index],
      o1 = outer[index + 1],
      i0 = inner[index],
      i1 = inner[index + 1];
    builder.triangle(point(o0, halfThickness), point(i0, halfThickness), point(i1, halfThickness));
    builder.triangle(point(o0, halfThickness), point(i1, halfThickness), point(o1, halfThickness));
    builder.triangle(point(o0, -halfThickness), point(i1, -halfThickness), point(i0, -halfThickness));
    builder.triangle(point(o0, -halfThickness), point(o1, -halfThickness), point(i1, -halfThickness));
    builder.triangle(point(o0, -halfThickness), point(o0, halfThickness), point(o1, halfThickness));
    builder.triangle(point(o0, -halfThickness), point(o1, halfThickness), point(o1, -halfThickness));
    builder.triangle(point(i0, -halfThickness), point(i1, halfThickness), point(i0, halfThickness));
    builder.triangle(point(i0, -halfThickness), point(i1, -halfThickness), point(i1, halfThickness));
  }
  for (const index of [0, outer.length - 1]) {
    const outerPoint = outer[index],
      innerPoint = inner[index],
      reverse = index === 0;
    const a = point(innerPoint, -halfThickness),
      b = point(outerPoint, -halfThickness),
      c = point(outerPoint, halfThickness),
      d = point(innerPoint, halfThickness);
    reverse ? (builder.triangle(a, c, b), builder.triangle(a, d, c)) : (builder.triangle(a, b, c), builder.triangle(a, c, d));
  }
  return finish(builder);
}

export function spearHead(parameters, offset = [0, 0, 0], label = "spear head") {
  const { length: headLength, width, thickness, shoulder = 0.18 } = parameters;
  const rootWidth = parameters.rootWidth ?? width * 0.4, belly = parameters.bellyPosition ?? shoulder, acuteness = parameters.acuteness ?? 1;
  const samples = detailSamples(12), stops = [...new Set([0, belly, ...Array.from({ length: samples }, (_, index) => index / samples)])].sort((a, b) => a - b);
  const builder = makeBuilder("steel", label);
  const ring = (t) => {
    const half = (t <= belly ? rootWidth + (width - rootWidth) * Math.pow(t / belly, 0.8) : width * Math.pow((1 - t) / (1 - belly), acuteness)) / 2;
    const depth = thickness / 2 * (1 - t * 0.9);
    return [[-half, t * headLength, 0], [0, t * headLength, depth], [half, t * headLength, 0], [0, t * headLength, -depth]].map((point) => move(point, offset));
  };
  for (let row = 0; row < stops.length; row++) for (let side = 0; side < 4; side++) {
    const a = ring(stops[row]), next = (side + 1) % 4;
    if (row === stops.length - 1) builder.triangle(a[side], move([0, headLength, 0], offset), a[next], `facet:${side}`);
    else {
      const b = ring(stops[row + 1]);
      builder.triangle(a[side], b[next], a[next], `facet:${side}`); builder.triangle(a[side], b[side], b[next], `facet:${side}`);
    }
  }
  const base = ring(0);
  for (let side = 0; side < 4; side++) builder.triangle(offset, base[side], base[(side + 1) % 4]);
  return finish(builder);
}

export function guard(parameters, offset = [0, 0, 0], label = "guard") {
  const { width, height, thickness, sweep = 0 } = parameters;
  const mode = parameters.mirrorMode ?? "opposed", half = width / 2;
  const arm = (side) => {
    const namedLength = mode === "independent" ? parameters[side < 0 ? "leftLength" : "rightLength"] ?? half : half,
      authoredSweep = mode === "independent" ? parameters[side < 0 ? "leftSweep" : "rightSweep"] : undefined, authoredSet = mode === "independent" ? parameters[side < 0 ? "leftSet" : "rightSet"] ?? 0 : 0,
      sideSweep = authoredSweep ?? (mode === "symmetric" ? sweep : sweep * side), samples = detailSamples(11, 5);
    return Array.from({ length: samples + 1 }, (_, index) => { const t = index / samples; return [side * namedLength * t, sideSweep * t * t, authoredSet * t]; });
  };
  const sectionWidth = parameters.sectionWidth ?? height * 0.44, sectionDepth = parameters.sectionDepth ?? thickness,
    centerline = [...arm(-1).reverse(), ...arm(1).slice(1)], members = [sweptMember(centerline, { ...parameters, centeredTaper: true, section: parameters.section ?? "round", sectionWidth, sectionDepth, material: parameters.material ?? "steel" }, offset, `${label} quillons`)];
  for (const side of [-1, 1]) {
    const points = arm(side), choice = mode === "independent" ? parameters[side < 0 ? "leftTerminal" : "rightTerminal"] : undefined, terminal = choice && choice !== "shared" ? choice : parameters.terminal ?? "none",
      tangent = subtract(points.at(-1), points.at(-2)), mesh = terminalMesh(terminal, parameters.terminalSize ?? height * 0.3, tangent, parameters.material ?? "steel", `${label} ${terminal} terminal`);
    if (mesh) members.push(transformMesh(mesh, [0, 0, 0], move(points.at(-1), offset)));
  }
  const blockWidth = Math.max(height * 1.2, parameters._gripWidth ?? 0);
  const block = roundedPlate([[-blockWidth / 2, -height * 0.28], [blockWidth / 2, -height * 0.28], [blockWidth * 0.58, height * 0.28], [-blockWidth * 0.58, height * 0.28]], thickness, "steel", offset, `${label} block`);
  return mergeMeshes([...members, block]);
}

export function guardPlate(plate, nodes, material = "steel", label = "guard plate") {
  const roundedLoop = (names) => {
    const controls = names.map((name) => nodes[name]), samples = detailSamples(8, 5);
    return controls.flatMap((point, index) => {
      const previous = controls[(index - 1 + controls.length) % controls.length], next = controls[(index + 1) % controls.length], start = point.map((value, axis) => (value + previous[axis]) / 2), end = point.map((value, axis) => (value + next[axis]) / 2);
      return Array.from({ length: samples }, (_, sample) => { const t = sample / samples, u = 1 - t; return point.map((value, axis) => u * u * start[axis] + 2 * u * t * value + t * t * end[axis]); });
    });
  };
  if (!Array.isArray(plate.cutout)) throw new Error(`${label}: a matched cutout loop is required`);
  const outer = roundedLoop(plate.outline), hole = roundedLoop(plate.cutout);
  if (signedArea(outer) < 0) { outer.reverse(); hole.reverse(); }
  if (outer.length !== hole.length) throw new Error(`${label}: outline and cutout need matching vertex counts`);
  const builder = makeBuilder(material, label), half = (plate.thickness ?? 0.003) / 2, bands = detailSamples(4, 2),
    ring = (band) => outer.map((point, index) => { const t = band / bands, inner = hole[index]; return [point[0] + (inner[0] - point[0]) * t, point[1] + (inner[1] - point[1]) * t, (point[2] ?? 0) + ((inner[2] ?? 0) - (point[2] ?? 0)) * t + (plate.dishDepth ?? 0) * Math.sin(Math.PI * t)]; });
  for (let i = 0; i < outer.length; i++) {
    const j = (i + 1) % outer.length;
    for (let band = 0; band < bands; band++) for (const side of [-1, 1]) { const current = ring(band), next = ring(band + 1), a = [...current[i].slice(0, 2), current[i][2] + half * side], b = [...current[j].slice(0, 2), current[j][2] + half * side], c = [...next[j].slice(0, 2), next[j][2] + half * side], d = [...next[i].slice(0, 2), next[i][2] + half * side]; side > 0 ? (builder.triangle(a, b, c, "plate-front"), builder.triangle(a, c, d, "plate-front")) : (builder.triangle(a, c, b, "plate-back"), builder.triangle(a, d, c, "plate-back")); }
    for (const loop of [outer, hole]) { const a = [loop[i][0], loop[i][1], (loop[i][2] ?? 0) - half], b = [loop[j][0], loop[j][1], (loop[j][2] ?? 0) - half], c = [loop[j][0], loop[j][1], (loop[j][2] ?? 0) + half], d = [loop[i][0], loop[i][1], (loop[i][2] ?? 0) + half]; loop === outer ? (builder.triangle(a, b, c), builder.triangle(a, c, d)) : (builder.triangle(a, c, b), builder.triangle(a, d, c)); }
  }
  const shell = finish(builder);
  return plate.rimRadius > 0 ? mergeMeshes([shell, sweptTube3d([...outer, outer[0]], plate.rimRadius, material, `${label} rolled rim`, 10, true)]) : shell;
}

export function guardAssembly(component, offset = [0, 0, 0], label = "guard assembly") {
  const nodes = component.nodes, meshes = component.members.map((member, index) => {
    const anchors = member.path.map((name) => { if (!nodes[name]) throw new Error(`${label}: missing node ${name}`); return nodes[name]; }), samples = detailSamples(5, 3),
      points = anchors.flatMap((point, row) => row === anchors.length - 1 ? [point] : Array.from({ length: samples }, (_, i) => {
        const t = i / samples, a = anchors[Math.max(0, row - 1)], b = point, c = anchors[row + 1], d = anchors[Math.min(anchors.length - 1, row + 2)];
        return point.map((_, axis) => 0.5 * ((2 * b[axis]) + (-a[axis] + c[axis]) * t + (2 * a[axis] - 5 * b[axis] + 4 * c[axis] - d[axis]) * t * t + (-a[axis] + 3 * b[axis] - 3 * c[axis] + d[axis]) * t * t * t));
      }));
    return sweptMember(points, { ...member, material: member.material ?? component.material ?? "steel" }, offset, `${label} ${member.label ?? index + 1}`);
  });
  for (const [index, plate] of (component.plates ?? []).entries()) meshes.push(transformMesh(guardPlate(plate, nodes, plate.material ?? component.material ?? "steel", `${label} plate ${index + 1}`), [0, 0, 0], offset));
  return mergeMeshes(meshes);
}

export function knuckleBow(parameters, offset = [0, 0, 0], label = "knuckle bow") {
  const { width, length: bowLength, bar = 0.012, thickness = 0.012, side = 1, bulge = 0.035 } = parameters;
  const s = side;
  const samples = parameters.samples ?? 18;
  const points = sampleAdaptiveCurve(
    (t) => {
      const u = 1 - t;
      return [2 * u * t * width * s + t * t * bar * s, 2 * u * t * (bowLength * 0.48 + bulge) + t * t * bowLength];
    },
    {
      minimumSegments: samples,
      maxChord: Math.max(width, bowLength) / samples,
      maxDeviation: Math.min(bar, thickness) / 10,
    },
  );
  const radius = Math.min(bar, thickness) / 2,
    swept = tubePath(points, radius, parameters.material ?? "steel", offset, label, parameters.radialSegments ?? 8);
  const anchors = [points[0], points.at(-1)].map((point) =>
    lathe(
      [
        [-radius, radius * 0.8],
        [0, radius * 1.35],
        [radius, radius * 0.8],
      ],
      10,
      "darkSteel",
      [offset[0] + point[0], offset[1] + point[1], offset[2]],
      `${label} anchor`,
    ),
  );
  return mergeMeshes([swept, ...anchors]);
}

function pointSegmentDistance(point, a, b) {
  const dx = b[0] - a[0],
    dy = b[1] - a[1],
    denominator = dx * dx + dy * dy;
  const t = denominator ? Math.max(0, Math.min(1, ((point[0] - a[0]) * dx + (point[1] - a[1]) * dy) / denominator)) : 0;
  return Math.hypot(point[0] - (a[0] + dx * t), point[1] - (a[1] + dy * t));
}
export function tubeCenterlineErrors(points, radius, closed = false, label = "tube") {
  const errors = [],
    segmentCount = points.length - 1,
    cumulative = [0];
  for (let index = 0; index < segmentCount; index += 1) cumulative.push(cumulative.at(-1) + Math.hypot(points[index + 1][0] - points[index][0], points[index + 1][1] - points[index][1]));
  const pathLength = cumulative.at(-1);
  for (let index = 0; index < segmentCount; index += 1) {
    if (Math.hypot(points[index + 1][0] - points[index][0], points[index + 1][1] - points[index][1]) < 1e-8) errors.push(`${label}: centerline segment ${index} has zero length`);
    for (let other = index + 2; other < segmentCount; other += 1) {
      if (closed && index === 0 && other === segmentCount - 1) continue;
      if (segmentsIntersect(points[index], points[index + 1], points[other], points[other + 1])) errors.push(`${label}: centerline segments ${index} and ${other} intersect`);
      const forwardArc = cumulative[other] - cumulative[index + 1];
      const separatedArc = closed ? Math.min(forwardArc, Math.max(0, pathLength - cumulative[other + 1] + cumulative[index])) : forwardArc;
      if (separatedArc > radius * 3) {
        const separation = Math.min(pointSegmentDistance(points[index], points[other], points[other + 1]), pointSegmentDistance(points[index + 1], points[other], points[other + 1]), pointSegmentDistance(points[other], points[index], points[index + 1]), pointSegmentDistance(points[other + 1], points[index], points[index + 1]));
        if (separation < radius * 1.5) errors.push(`${label}: nonadjacent centerline segments are too close for the bar radius`);
      }
    }
  }
  return errors;
}

export function tubePath(points, radius, material = "steel", offset = [0, 0, 0], label = "tube", radialSegments = 8, closed = false, allowCrossing = false, radii = null) {
  if (!allowCrossing) {
    const errors = tubeCenterlineErrors(points, radius, closed, label);
    if (errors.length) throw new Error(errors.join("; "));
  }
  radialSegments = tubeRadialSegments(radius, radialSegments);
  const builder = makeBuilder(material, label),
    count = points.length;
  const ringPoint = (index, segment) => {
    const previous = points[index === 0 ? (closed ? count - 2 : 0) : index - 1],
      next = points[index === count - 1 ? (closed ? 1 : count - 1) : index + 1];
    const dx = next[0] - previous[0],
      dy = next[1] - previous[1],
      tangentLength = Math.hypot(dx, dy) || 1;
    const nx = -dy / tangentLength,
      ny = dx / tangentLength,
      angle = (segment / radialSegments) * Math.PI * 2;
    const ringRadius = radii?.[index] ?? radius;
    return move([points[index][0] + nx * Math.cos(angle) * ringRadius, points[index][1] + ny * Math.cos(angle) * ringRadius, Math.sin(angle) * ringRadius], offset);
  };
  const span = closed ? count - 1 : count - 1;
  for (let index = 0; index < span; index += 1)
    for (let segment = 0; segment < radialSegments; segment += 1) {
      const nextSegment = (segment + 1) % radialSegments,
        a = ringPoint(index, segment),
        b = ringPoint(index, nextSegment),
        c = ringPoint(index + 1, nextSegment),
        d = ringPoint(index + 1, segment);
      builder.triangle(a, b, c, "tube");
      builder.triangle(a, c, d, "tube");
    }
  if (!closed)
    for (const [index, reverse] of [
      [0, true],
      [count - 1, false],
    ]) {
      const center = move([points[index][0], points[index][1], 0], offset);
      for (let segment = 0; segment < radialSegments; segment += 1) {
        const a = ringPoint(index, segment),
          b = ringPoint(index, (segment + 1) % radialSegments);
        reverse ? builder.triangle(center, b, a) : builder.triangle(center, a, b);
      }
    }
  return finish(builder);
}

export function tubeRadialSegments(radius, requested = 0, { maxChord = 0.006, maxSagitta = 0.0003 } = {}) {
  const chordSegments = maxChord >= radius * 2 ? 3 : Math.ceil(Math.PI / Math.asin(maxChord / (radius * 2)));
  const sagittaSegments = maxSagitta >= radius ? 3 : Math.ceil(Math.PI / Math.acos(1 - maxSagitta / radius));
  return Math.max(roundSegments(radius, requested), Math.ceil(chordSegments / detailError(1)), Math.ceil(sagittaSegments / Math.sqrt(detailError(1))));
}

export function ringGuard(parameters, offset = [0, 0, 0], label = "side ring") {
  const samples = parameters.samples ?? 24,
    start = parameters.arcStart ?? 0,
    end = parameters.arcEnd ?? Math.PI * 2;
  const points = sampleAdaptiveCurve(
    (t) => {
      const angle = start + t * (end - start);
      return [Math.cos(angle) * parameters.radius, Math.sin(angle) * parameters.radius];
    },
    {
      minimumSegments: samples,
      maxChord: Math.max(0.004, parameters.radius * 0.24),
      maxDeviation: Math.max(0.00025, parameters.radius / 220),
    },
  );
  return tubePath(points, parameters.bar ?? 0.007, parameters.material ?? "steel", offset, label, parameters.radialSegments ?? 8, Math.abs(end - start) >= Math.PI * 1.99);
}

export function figureEightGuard(parameters, offset = [0, 0, 0], label = "figure-eight guard") {
  const samples = parameters.samples ?? 48,
    halfWidth = parameters.width / 2,
    height = parameters.height ?? parameters.width * 0.28;
  const points = sampleAdaptiveCurve(
    (t) => {
      const angle = t * Math.PI * 2;
      return [halfWidth * Math.sin(angle), height * Math.sin(angle) * Math.cos(angle)];
    },
    {
      minimumSegments: samples,
      maxChord: parameters.width / 30,
      maxDeviation: Math.min(parameters.bar ?? 0.009, height) / 12,
    },
  );
  return tubePath(points, parameters.bar ?? 0.009, parameters.material ?? "steel", offset, label, parameters.radialSegments ?? 8, true, true);
}

export function sectionBlade(parameters, offset = [0, 0, 0], label = "sectioned blade") {
  const builder = makeBuilder(parameters.material ?? "steel", label),
    samples = detailSamples(18);
  const style = parameters.section ?? "diamond";
  const ring = (t) => {
    const halfWidth = parameters.width * 0.5 * (0.025 + 0.975 * Math.pow(1 - t, parameters.taper ?? 0.8));
    const halfDepth = parameters.thickness * 0.5 * (1 - t * 0.72);
    if (style === "fullered")
      return [
        [-halfWidth, 0],
        [-halfWidth * 0.72, halfDepth],
        [-halfWidth * 0.28, halfDepth * 0.32],
        [0, halfDepth * 0.22],
        [halfWidth * 0.28, halfDepth * 0.32],
        [halfWidth * 0.72, halfDepth],
        [halfWidth, 0],
        [0, -halfDepth * 0.8],
      ];
    return [
      [-halfWidth, 0],
      [0, halfDepth],
      [halfWidth, 0],
      [0, -halfDepth],
    ];
  };
  for (let index = 0; index < samples; index += 1) {
    const t0 = index / samples,
      t1 = (index + 1) / samples,
      r0 = ring(t0),
      r1 = ring(t1),
      count = r0.length;
    for (let side = 0; side < count; side += 1) {
      const next = (side + 1) % count;
      const a = move([r0[side][0], t0 * parameters.length, r0[side][1]], offset),
        b = move([r0[next][0], t0 * parameters.length, r0[next][1]], offset),
        c = move([r1[next][0], t1 * parameters.length, r1[next][1]], offset),
        d = move([r1[side][0], t1 * parameters.length, r1[side][1]], offset);
      builder.triangle(a, b, c);
      builder.triangle(a, c, d);
    }
  }
  for (const [t, reverse] of [
    [0, true],
    [1, false],
  ]) {
    const values = ring(t),
      center = move([0, t * parameters.length, 0], offset);
    for (let side = 0; side < values.length; side += 1) {
      const next = (side + 1) % values.length,
        a = move([values[side][0], t * parameters.length, values[side][1]], offset),
        b = move([values[next][0], t * parameters.length, values[next][1]], offset);
      reverse ? builder.triangle(center, b, a) : builder.triangle(center, a, b);
    }
  }
  return finish(builder);
}

export function forgedFork(parameters, offset = [0, 0, 0], label = "forged fork") {
  const { length: tineLength, width, baseWidth, thickness, crotch = 0.34 } = parameters,
    half = width / 2,
    root = baseWidth / 2,
    tine = parameters.tineWidth ?? width * 0.18;
  const taper = parameters.tineTaper ?? 0.55,
    shoulder = parameters.shoulderBlend ?? 0.2,
    crotchRound = parameters.crotchRound ?? 0.05;
  return prism(
    [
      [-root, 0],
      [-half, tineLength * shoulder],
      [-half, tineLength],
      [-half + tine * taper, tineLength * 0.94],
      [-tine * 0.45, tineLength * (crotch + crotchRound)],
      [0, tineLength * crotch],
      [tine * 0.45, tineLength * (crotch + crotchRound)],
      [half - tine * taper, tineLength * 0.94],
      [half, tineLength],
      [half, tineLength * shoulder],
      [root, 0],
    ],
    thickness,
    "steel",
    offset,
    label,
  );
}

export function partisanBlade(parameters, offset = [0, 0, 0], label = "partisan blade") {
  const { length: bladeLength, width, lugWidth, thickness, lugDrop = 0.08 } = parameters,
    lug = lugWidth / 2;
  const belly = parameters.bellyPosition ?? 0.32,
    rootWidth = parameters.rootWidth ?? width * 0.18,
    lugSweep = parameters.lugSweep ?? 0.055,
    acuteness = parameters.acuteness ?? 1;
  const shoulderY = bladeLength * Math.max(0.18, Math.min(0.48, belly)),
    pointShoulder = shoulderY + (bladeLength - shoulderY) * (1 - 1 / (1 + acuteness));
  return prism(
    [
      [0, bladeLength],
      [-width * 0.34, pointShoulder],
      [-width / 2, shoulderY],
      [-width * 0.34, bladeLength * 0.12],
      [-lug, bladeLength * lugDrop],
      [-lug * 0.72, 0],
      [-rootWidth / 2, bladeLength * lugSweep],
      [rootWidth / 2, bladeLength * lugSweep],
      [lug * 0.72, 0],
      [lug, bladeLength * lugDrop],
      [width * 0.34, bladeLength * 0.12],
      [width / 2, shoulderY],
      [width * 0.34, pointShoulder],
    ],
    thickness,
    "steel",
    offset,
    label,
  );
}

export function glaiveSpineCurveSpans(parameters) {
  const { length: bladeLength, width, curvature = 0.1 } = parameters,
    apexX = curvature * 0.42;
  const spineCurve = parameters.spineCurvature ?? 0.2,
    pointLength = parameters.pointLength ?? 0.24;
  const apex = [apexX, bladeLength],
    spineTop = [apexX - width * 0.1, bladeLength * (1 - pointLength)],
    spineLower = [-width * 0.42, bladeLength * 0.18];
  const spineJoinHandle = [-width * 0.08, -bladeLength * pointLength * 0.12];
  return [
    {
      name: "apex-to-spine",
      points: [apex, [apexX - width * 0.012, bladeLength * 0.96], [spineTop[0] - spineJoinHandle[0], spineTop[1] - spineJoinHandle[1]], spineTop],
      end: "spineTop",
    },
    {
      name: "spine-to-root",
      points: [spineTop, [spineTop[0] + spineJoinHandle[0], spineTop[1] + spineJoinHandle[1]], [-width * (0.34 + spineCurve * 0.04), bladeLength * 0.31], spineLower],
      start: "spineTop",
    },
  ];
}

export function glaiveOutline(parameters) {
  const { length: bladeLength, width, curvature = 0.1, root = 0.035 } = parameters,
    apexX = curvature * 0.42;
  const rootLength = parameters.rootLength ?? 0.08,
    belly = parameters.bellyPosition ?? 0.42,
    edgeCurve = parameters.edgeCurvature ?? 0.24,
    spineCurve = parameters.spineCurvature ?? 0.2,
    pointLength = parameters.pointLength ?? 0.24;
  const points = [
    [-root, -rootLength],
    [root, -rootLength],
    [root * 1.18, bladeLength * 0.025],
    [width * 0.48, bladeLength * 0.12],
  ];
  const edgeLimit = 1 - pointLength * 0.34,
    quality = {
      minimumSegments: 12,
      maxChord: bladeLength / 28,
      maxDeviation: width / 180,
    };
  appendCurve(
    points,
    sampleAdaptiveCurve((u) => {
      const t = u * edgeLimit;
      return [apexX + (1 - t) * width * (0.54 + edgeCurve * Math.sin(Math.PI * Math.min(1, t / Math.max(0.1, belly)))), bladeLength * (0.08 + (0.84 - pointLength * 0.34) * t)];
    }, quality),
  );
  const nearApex = points.at(-1),
    apex = [apexX, bladeLength],
    spineTop = [apexX - width * 0.1, bladeLength * (1 - pointLength)],
    spineLower = [-width * 0.42, bladeLength * 0.18],
    rootShoulder = [-root * 1.18, bladeLength * 0.025];
  appendCurve(points, sampleCubicBezier([nearApex, [nearApex[0] * 0.72 + apexX * 0.28, bladeLength * 0.86], [apexX + width * 0.006, bladeLength * 0.96], apex], { ...quality, minimumSegments: 4 }));
  const spineSpans = glaiveSpineCurveSpans(parameters);
  appendCurve(points, sampleCubicBezier(spineSpans[0].points, { ...quality, minimumSegments: 4 }));
  appendCurve(points, sampleCubicBezier(spineSpans[1].points, quality));
  appendCurve(points, sampleCubicBezier([spineLower, [-width * 0.32, bladeLength * 0.13], [-root * 1.45, bladeLength * 0.05], rootShoulder], { ...quality, minimumSegments: 4 }));
  return points;
}

export function glaiveBlade(parameters, offset = [0, 0, 0], label = "glaive blade") {
  return prism(glaiveOutline(parameters), parameters.thickness, "steel", offset, label);
}

function shieldCurve(component, x, y) {
  if (component.kind === "roundShield") {
    const radial = Math.min(1, Math.hypot(x, y) / component.radius),
      broad = component.outerCurve * (1 - radial * radial),
      centerLimit = component.centerRadius / component.radius,
      centerT = centerLimit > 0 ? radial / centerLimit : 1,
      center = centerT < 1 ? component.centerCurve * (1 - centerT * centerT) ** 2 : 0;
    return broad + center;
  }
  const horizontal = Math.min(1, Math.abs((2 * x) / component.width)),
    broad = component.cylindricalCurve * (1 - horizontal * horizontal),
    centerDistance = (x / (component.centerWidth / 2)) ** 2 + (y / (component.centerHeight / 2)) ** 2,
    center = centerDistance < 1 ? component.centerCurve * (1 - centerDistance) ** 2 : 0;
  return broad + center;
}

function shieldSurfaceZ(component, x, y, front = true) {
  return shieldCurve(component, x, y) + (front ? component.thickness / 2 : -component.thickness / 2);
}

function endingProfile(shape, u, depth, roundness) {
  const distance = Math.min(1, Math.abs(u));
  if (shape === "flat") return 0;
  const triangular = 1 - distance,
    rounded = 0.5 + 0.5 * Math.cos(distance * Math.PI),
    smooth = triangular * (1 - roundness) + rounded * roundness;
  if (shape === "rounded") return depth * (Math.sqrt(Math.max(0, 1 - distance * distance)) * (1 - roundness) + rounded * roundness);
  if (shape === "doublePeak") {
    const lobeDistance = Math.min(1, Math.abs(Math.abs(u) - 0.48) / 0.52),
      lobe = (1 - lobeDistance) * (1 - roundness) + (0.5 + 0.5 * Math.cos(lobeDistance * Math.PI)) * roundness;
    return depth * Math.max(0, lobe);
  }
  return depth * smooth;
}

function shieldResolution(component) {
  const error = detailError(0.001);
  const broad = Math.sqrt(component.cylindricalCurve / error);
  const rib = component.width / component.centerWidth * Math.sqrt(component.centerCurve / error);
  const count = Math.max(detailSamples(component.edgeSegments, 6), Math.ceil(broad), Math.ceil(rib));
  return count + count % 2;
}

const GUARD_SECTIONS = new Set(["round", "oval", "diamond", "flat", "triangular"]);
const GUARD_TERMINALS = new Set(["none", "ball", "disk", "pyramidal", "scroll", "fishtail", "vase"]);

function sectionOutline(section, width, depth, segments) {
  if (section === "round") depth = width;
  if (section === "round" || section === "oval") {
    const count = tubeRadialSegments(Math.max(width, depth) / 2, segments);
    return Array.from({ length: count }, (_, index) => {
      const angle = index / count * Math.PI * 2;
      return [Math.cos(angle) * width / 2, Math.sin(angle) * depth / 2];
    });
  }
  if (section === "diamond") return [[width / 2, 0], [0, depth / 2], [-width / 2, 0], [0, -depth / 2]];
  if (section === "triangular") return [[0, depth * 0.58], [-width / 2, -depth * 0.42], [width / 2, -depth * 0.42]];
  return [[-width / 2, -depth / 2], [width / 2, -depth / 2], [width / 2, depth / 2], [-width / 2, depth / 2]];
}

/** Sweep an authored section along a connected 3D centerline. Polygonal
 * sections deliberately keep hard ridges; round and oval sections average. */
export function sweptMember(points, parameters = {}, offset = [0, 0, 0], label = "swept member") {
  const section = parameters.section ?? "round", width = parameters.sectionWidth ?? parameters.width ?? 0.012,
    depth = parameters.sectionDepth ?? parameters.depth ?? width, outline = sectionOutline(section, width, depth, parameters.radialSegments ?? 12),
    builder = makeBuilder(parameters.material ?? "steel", label), smooth = section === "round" || section === "oval", closed = length(subtract(points[0], points.at(-1))) < 1e-8;
  if (closed && Math.abs((parameters.sectionTwist ?? 0) % 360) > 1e-8) throw new Error(`${label}: a closed member needs whole-turn section twist`);
  const tangents = points.map((_, row) => normalize(subtract(points[row === points.length - 1 ? (closed ? 1 : row) : row + 1], points[row === 0 ? (closed ? points.length - 2 : 0) : row - 1]))), frames = [];
  const rotateAround = (vector, axis, angle) => {
    const cosine = Math.cos(angle), sine = Math.sin(angle), side = cross(axis, vector), projection = dot(axis, vector);
    return vector.map((value, i) => value * cosine + side[i] * sine + axis[i] * projection * (1 - cosine));
  };
  for (let row = 0; row < points.length; row++) {
    const tangent = tangents[row];
    if (!row) {
      const reference = Math.abs(tangent[2]) < 0.8 ? [0, 0, 1] : [0, 1, 0];
      frames.push(normalize(cross(reference, tangent)));
    } else {
      const axis = cross(tangents[row - 1], tangent), sine = length(axis), cosine = dot(tangents[row - 1], tangent);
      if (cosine < -0.99) throw new Error(`${label}: member centerline reverses direction`);
      frames.push(sine < 1e-8 ? frames[row - 1] : normalize(rotateAround(frames[row - 1], axis.map((value) => value / sine), Math.atan2(sine, cosine))));
    }
  }
  if (closed) {
    const angle = Math.atan2(dot(cross(frames.at(-1), frames[0]), tangents[0]), dot(frames.at(-1), frames[0]));
    for (let row = 0; row < frames.length; row++) frames[row] = rotateAround(frames[row], tangents[row], angle * row / (frames.length - 1));
    frames[frames.length - 1] = frames[0];
  }
  const point = (row, side) => {
    const tangent = tangents[row], normal = frames[row];
    const binormal = normalize(cross(tangent, normal)), twist = ((parameters.sectionTwist ?? 0) * row / Math.max(1, points.length - 1)) * Math.PI / 180,
      progress = row / Math.max(1, points.length - 1), t = parameters.centeredTaper ? Math.abs(progress * 2 - 1) : progress, taper = (1 + ((parameters.tipScale ?? 1) - 1) * t) * (1 + (parameters.terminalSwell ?? 0) * Math.max(0, (t - 0.72) / 0.28) ** 2),
      [u, v] = outline[side].map((value) => value * taper), rotatedU = u * Math.cos(twist) - v * Math.sin(twist), rotatedV = u * Math.sin(twist) + v * Math.cos(twist);
    return move([points[row][0] + normal[0] * rotatedU + binormal[0] * rotatedV, points[row][1] + normal[1] * rotatedU + binormal[1] * rotatedV, points[row][2] + normal[2] * rotatedU + binormal[2] * rotatedV], offset);
  };
  for (let row = 0; row < points.length - 1; row++) for (let side = 0; side < outline.length; side++) {
    const next = (side + 1) % outline.length, group = smooth ? "sweep" : `section:${side}`;
    builder.triangle(point(row, side), point(row, next), point(row + 1, next), group);
    builder.triangle(point(row, side), point(row + 1, next), point(row + 1, side), group);
  }
  if (!closed) for (const [row, reverse] of [[0, true], [points.length - 1, false]]) for (let side = 0; side < outline.length; side++) {
    const center = move(points[row], offset), a = point(row, side), b = point(row, (side + 1) % outline.length);
    reverse ? builder.triangle(center, b, a) : builder.triangle(center, a, b);
  }
  return finish(builder);
}

function terminalMesh(style, size, tangent, material, label) {
  if (!style || style === "none") return null;
  const rotation = [Math.atan2(tangent[2], Math.hypot(tangent[0], tangent[1])) * 180 / Math.PI, 0, Math.atan2(-tangent[0], tangent[1]) * 180 / Math.PI];
  if (style === "ball") return lathe([[-size, 0.002], [-size * 0.72, size * 0.7], [0, size], [size * 0.72, size * 0.7], [size, 0.002]], 14, material, [0, 0, 0], label);
  if (style === "disk") return transformMesh(lathe([[-size * 0.22, size], [size * 0.22, size]], 14, material, [0, 0, 0], label), rotation);
  if (style === "pyramidal") return transformMesh(prism([[-size, -size], [size, -size], [0, size * 1.25]], size * 1.4, material, [0, 0, 0], label), rotation);
  if (style === "fishtail") return transformMesh(roundedPlate([[-size * 0.45, -size], [-size, size], [0, size * 0.45], [size, size], [size * 0.45, -size]], size * 0.65, material, [0, 0, 0], label), rotation);
  if (style === "scroll") {
    const samples = detailSamples(18, 8), points = Array.from({ length: samples }, (_, i) => { const t = i / (samples - 1), a = t * Math.PI * 1.6, r = size * (1 - t * 0.65); return [Math.cos(a) * r - size, Math.sin(a) * r, 0]; });
    return transformMesh(sweptMember(points, { section: "round", sectionWidth: size * 0.35, sectionDepth: size * 0.35, material }, [0, 0, 0], label), rotation);
  }
  return transformMesh(lathe([[-size, size * 0.45], [-size * 0.45, size], [size * 0.35, size * 0.7], [size, size * 0.35]], 14, material, [0, 0, 0], label), rotation);
}

function pommelOutline(component) {
  const width = (component.diameter ?? 0.055) * (component.widthScale ?? 1), height = (component.height ?? 0.06) * (component.lengthScale ?? 1);
  if ((component.outlineStyle ?? "fishtail") === "fishtail") {
    const notch = component.notchDepth ?? 0.22, spread = component.lobeSpread ?? 0.9, shoulder = component.shoulderWidth ?? 0.42;
    return [[-width * shoulder / 2, 0], [-width / 2, height * 0.72], [-width * spread / 2, height], [0, height * (1 - notch)], [width * spread / 2, height], [width / 2, height * 0.72], [width * shoulder / 2, 0]].map(([x, y]) => [x, height - y]).reverse();
  }
  return [[-width * 0.22, 0], [-width / 2, height * 0.45], [-width * 0.4, height], [width * 0.4, height], [width / 2, height * 0.45], [width * 0.22, 0]];
}

export function pommelMesh(component, offset = [0, 0, 0], label = "pommel") {
  const construction = component.construction, material = component.material ?? "steel";
  if (construction === "composite") {
    const base = pommelMesh({ ...component, construction: component.baseConstruction, ornaments: undefined, sockets: undefined }, offset, `${label} base`), meshes = [base];
    for (const ornament of component.ornaments) {
      const socket = move(component.sockets[ornament.socket], offset), scale = ornament.scale, ornamentMaterial = ornament.material ?? material;
      // Face ornaments seat against the actual base surface, so changing bun
      // diameter or construction cannot bury an authored relief inside it.
      const faceSign = Math.sign(component.sockets[ornament.socket][2]);
      if (faceSign) for (const [a, b, c] of triangleVertices(base)) {
        const denominator = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
        if (Math.abs(denominator) < 1e-12 || !pointInTriangle(socket, a, b, c)) continue;
        const u = ((b[1] - c[1]) * (socket[0] - c[0]) + (c[0] - b[0]) * (socket[1] - c[1])) / denominator,
          v = ((c[1] - a[1]) * (socket[0] - c[0]) + (a[0] - c[0]) * (socket[1] - c[1])) / denominator, z = u * a[2] + v * b[2] + (1 - u - v) * c[2];
        socket[2] = faceSign > 0 ? Math.max(socket[2], z) : Math.min(socket[2], z);
      }
      if (ornament.style === "crown") {
        const band = hollowSocket([[0, scale * 0.58], [scale * 0.34, scale * 0.62]], scale * 0.42, ornamentMaterial, `${label} crown band`), prongs = [band];
        for (let index = 0; index < 5; index++) { const angle = index / 5 * Math.PI * 2, point = [Math.cos(angle) * scale * 0.52, scale * 0.25, Math.sin(angle) * scale * 0.52]; prongs.push(transformMesh(prism([[-scale * 0.12, 0], [scale * 0.12, 0], [0, scale * 0.65]], scale * 0.16, ornamentMaterial, [0, 0, 0], `${label} crown point`), [0, -angle * 180 / Math.PI, 0], point)); }
        meshes.push(transformMesh(mergeMeshes(prongs), ornament.rotation, socket));
      } else if (ornament.style === "escutcheon") {
        const plate = roundedPlate([[-scale * 0.55, 0], [-scale * 0.48, scale * 0.72], [0, scale], [scale * 0.48, scale * 0.72], [scale * 0.55, 0], [0, -scale * 0.36]], scale * 0.13, ornamentMaterial, [0, 0, 0], `${label} escutcheon`);
        meshes.push(transformMesh(plate, ornament.rotation, socket));
      } else {
        const builder = makeBuilder(ornamentMaterial, `${label} authored ornament`);
        for (let i = 0; i < ornament.indices.length; i += 3) builder.triangle(...ornament.indices.slice(i, i + 3).map((index) => ornament.positions.slice(index * 3, index * 3 + 3).map((value) => value * scale)), ornament.smooth ? "authored" : i / 3);
        meshes.push(transformMesh(finish(builder), ornament.rotation, socket));
      }
    }
    return mergeMeshes(meshes);
  }
  if (construction === "lathed") return lathe(component.profile, component.segments ?? 14, material, offset, label);
  if (construction === "plate") {
    const radius = (component.diameter ?? 0.06) * (component.widthScale ?? 1) / 2, height = (component.height ?? component.diameter ?? 0.06) * (component.lengthScale ?? 1), samples = detailSamples(24, 10);
    const outline = Array.from({ length: samples }, (_, i) => { const a = i / samples * Math.PI * 2; return [Math.cos(a) * radius, height * 0.425 + Math.sin(a) * height * 0.425]; });
    const plate = roundedPlate(outline, component.thickness ?? 0.018, material, offset, label, component.rimBevel ?? 0.15), seat = lathe([[height * 0.66, 0.009], [height, 0.01]], 14, material, offset, `${label} tang seat`, (component.thickness ?? 0.018) / 0.02),
      bulge = (component.thickness ?? 0.018) * (component.faceConvexity ?? 0.15), faceRadius = radius * (1 - (component.rimBevel ?? 0.15)),
      faces = bulge > 0 ? [-1, 1].map((side) => transformMesh(lathe([[0, faceRadius], [bulge * 0.65, faceRadius * 0.72], [bulge, 0.001]], 18, material, [0, 0, 0], `${label} convex face`, height * 0.425 / radius), [90 * side, 0, 0], [offset[0], offset[1] + height * 0.425, offset[2] + side * (component.thickness ?? 0.018) * 0.49])) : [];
    return mergeMeshes([plate, seat, ...faces]);
  }
  if (construction === "outline") {
    if (component.outlineStyle === "fan") return fanPommel({ width: component.diameter * (component.widthScale ?? 1), height: component.height * (component.lengthScale ?? 1), thickness: component.thickness, material }, offset, label);
    const plate = roundedPlate(pommelOutline(component), component.thickness ?? 0.018, material, offset, label), height = (component.height ?? 0.06) * (component.lengthScale ?? 1), seat = lathe([[height * 0.78, 0.008], [height, 0.01]], 14, material, offset, `${label} tang seat`, (component.thickness ?? 0.018) / 0.02);
    return mergeMeshes([plate, seat]);
  }
  const faceted = construction === "faceted", flutes = component.fluteCount ?? 8, h = (component.height ?? 0.06) * (component.lengthScale ?? 1), r = (component.diameter ?? 0.06) * (component.widthScale ?? 1) / 2,
    depth = faceted ? 0 : component.fluteDepth ?? 0.12, twist = (component.twist ?? 75) * Math.PI / 180,
    axialSamples = Math.max(detailSamples(10, 12), Math.ceil(Math.hypot(h, 2 * r, r * twist) / detailError(0.015))),
    profile = faceted ? [[0, r * 0.28], [h * 0.12, r * 0.72], [h * 0.32, r], [h * 0.68, r * 0.94], [h * 0.9, r * 0.56], [h, r * 0.3]] : Array.from({ length: axialSamples + 1 }, (_, i) => { const t = i / axialSamples; return [h * t, r * (0.28 + 0.06 * t + 0.82 * Math.sin(Math.PI * t) * (1 - 0.35 * t))]; }),
    fluteSamples = Math.max(6, Math.ceil(Math.PI / Math.sqrt(2 * detailError(0.0006) / Math.max(0.0001, r * depth)))),
    segments = faceted ? component.facets ?? 8 : Math.max(roundSegments(r, component.segments ?? 18), flutes * fluteSamples),
    builder = makeBuilder(material, label);
  const vertex = (row, segment) => { const [y, base] = profile[row], baseAngle = segment / segments * Math.PI * 2, t = (y - profile[0][0]) / Math.max(1e-6, profile.at(-1)[0] - profile[0][0]), angle = baseAngle - (faceted ? 0 : twist * t), radius = base * (1 + depth * Math.cos(baseAngle * flutes) * Math.sin(Math.PI * t)); return move([Math.cos(angle) * radius, y, Math.sin(angle) * radius], offset); };
  for (let row = 0; row < profile.length - 1; row++) for (let segment = 0; segment < segments; segment++) { const next = (segment + 1) % segments, group = faceted ? `facet:${segment}` : "writhen"; builder.triangle(vertex(row, segment), vertex(row + 1, next), vertex(row, next), group); builder.triangle(vertex(row, segment), vertex(row + 1, segment), vertex(row + 1, next), group); }
  for (const [row, reverse] of [[0, true], [profile.length - 1, false]]) for (let segment = 0; segment < segments; segment++) { const center = move([0, profile[row][0], 0], offset), a = vertex(row, segment), b = vertex(row, (segment + 1) % segments); reverse ? builder.triangle(center, a, b) : builder.triangle(center, b, a); }
  return finish(builder);
}

function shapedShieldSections(component) {
  const count = shieldResolution(component),
    halfWidth = component.width / 2,
    halfHeight = component.height / 2,
    cornerFraction = Math.min(0.45, component.cornerRadius / halfWidth),
    cornerDrop = (u) => {
      if (!cornerFraction) return 0;
      const t = Math.max(0, Math.min(1, (Math.abs(u) - (1 - cornerFraction)) / cornerFraction));
      return component.cornerRadius * (1 - Math.sqrt(Math.max(0, 1 - t * t)));
    },
    top = [],
    bottom = [];
  for (let index = 0; index <= count; index += 1) {
    const u = -1 + (index / count) * 2,
      x = u * halfWidth;
    top.push([x, halfHeight + endingProfile(component.topShape, u, component.topDepth, component.topRoundness) - cornerDrop(u)]);
  }
  for (let index = count; index >= 0; index -= 1) {
    const u = -1 + (index / count) * 2,
      x = u * halfWidth * (1 - component.sideTaper),
      bottomShape = component.bottomShape === "point" ? "singlePeak" : component.bottomShape;
    bottom.push([x, -halfHeight - endingProfile(bottomShape, u, component.bottomDepth, component.bottomRoundness) + cornerDrop(u) * (1 - component.sideTaper)]);
  }
  return { top, bottom: [...bottom].reverse() };
}

export function shapedShieldOutline(component) {
  const { top, bottom } = shapedShieldSections(component);
  return [...top, ...[...bottom].reverse()];
}

function shapedShieldShell(component, label) {
  const builder = makeBuilder(component.material ?? "wood", label),
    { top, bottom } = shapedShieldSections(component),
    verticalSegments = Math.max(4, Math.ceil(shieldResolution(component) / 2)),
    planarPoint = (column, row) => {
      const t = row / verticalSegments;
      return [bottom[column][0] + (top[column][0] - bottom[column][0]) * t, bottom[column][1] + (top[column][1] - bottom[column][1]) * t];
    },
    vertex = (column, row, front) => {
      const [x, y] = planarPoint(column, row);
      return [x, y, shieldSurfaceZ(component, x, y, front)];
    };
  for (let column = 0; column < top.length - 1; column += 1)
    for (let row = 0; row < verticalSegments; row += 1) {
      const frontA = vertex(column, row, true), frontB = vertex(column + 1, row, true), frontC = vertex(column + 1, row + 1, true), frontD = vertex(column, row + 1, true),
        backA = vertex(column, row, false), backB = vertex(column + 1, row, false), backC = vertex(column + 1, row + 1, false), backD = vertex(column, row + 1, false);
      builder.triangle(frontA, frontB, frontC, "front");
      builder.triangle(frontA, frontC, frontD, "front");
      builder.triangle(backA, backC, backB, "back");
      builder.triangle(backA, backD, backC, "back");
    }
  for (const row of [0, verticalSegments])
    for (let column = 0; column < top.length - 1; column += 1) {
      const frontA = vertex(column, row, true), frontB = vertex(column + 1, row, true), backA = vertex(column, row, false), backB = vertex(column + 1, row, false);
      row === 0 ? (builder.triangle(frontA, backB, frontB), builder.triangle(frontA, backA, backB)) : (builder.triangle(frontA, frontB, backB), builder.triangle(frontA, backB, backA));
    }
  for (const column of [0, top.length - 1])
    for (let row = 0; row < verticalSegments; row += 1) {
      const frontA = vertex(column, row, true), frontB = vertex(column, row + 1, true), backA = vertex(column, row, false), backB = vertex(column, row + 1, false);
      column === 0 ? (builder.triangle(frontA, frontB, backB), builder.triangle(frontA, backB, backA)) : (builder.triangle(frontA, backB, frontB), builder.triangle(frontA, backA, backB));
    }
  return finish(builder);
}

export function shieldHandAperture(component) {
  return component.kind === "roundShield" && component.fittingMode === "grip" && component.bossHeight > 0
    ? Math.min(component.bossRadius * 0.72, component.gripLength * 0.4) : 0;
}

export function roundShieldShell(component, label = "round shield body") {
  const builder = makeBuilder(component.material ?? "wood", label),
    segments = roundSegments(component.radius, component.radialSegments),
    rings = detailSamples(component.rings, 3),
    aperture = shieldHandAperture(component),
    point = (ring, segment, front) => {
      if (ring === 0 && aperture === 0) return [0, 0, shieldSurfaceZ(component, 0, 0, front)];
      const radius = aperture + (component.radius - aperture) * (ring / rings),
        angle = (segment / segments) * Math.PI * 2,
        x = Math.cos(angle) * radius,
        y = Math.sin(angle) * radius;
      return [x, y, shieldSurfaceZ(component, x, y, front)];
    };
  for (const front of [true, false]) {
    if (aperture === 0) for (let segment = 0; segment < segments; segment += 1) {
      const next = (segment + 1) % segments,
        center = point(0, 0, front),
        a = point(1, segment, front),
        b = point(1, next, front);
      front ? builder.triangle(center, a, b, "front") : builder.triangle(center, b, a, "back");
    }
    for (let ring = aperture > 0 ? 0 : 1; ring < rings; ring += 1)
      for (let segment = 0; segment < segments; segment += 1) {
        const next = (segment + 1) % segments,
          a = point(ring, segment, front),
          b = point(ring + 1, segment, front),
          c = point(ring + 1, next, front),
          d = point(ring, next, front);
        front ? (builder.triangle(a, b, c, "front"), builder.triangle(a, c, d, "front")) : (builder.triangle(a, c, b, "back"), builder.triangle(a, d, c, "back"));
      }
  }
  for (let segment = 0; segment < segments; segment += 1) {
    const next = (segment + 1) % segments,
      a = point(rings, segment, true),
      b = point(rings, next, true),
      c = point(rings, next, false),
      d = point(rings, segment, false);
    builder.triangle(a, c, b);
    builder.triangle(a, d, c);
  }
  if (aperture > 0) for (let segment = 0; segment < segments; segment++) {
    const next = (segment + 1) % segments;
    const a = point(0, segment, true), b = point(0, next, true), c = point(0, next, false), d = point(0, segment, false);
    builder.triangle(a, b, c); builder.triangle(a, c, d);
  }
  return finish(builder);
}

function sweptTube3d(points, radius, material, label, radialSegments = 12, closed = false) {
  radialSegments = roundSegments(radius, radialSegments);
  const firstDirection = normalize(subtract(points[1], points[0]));
  const plane = points.slice(2).map((point) => cross(firstDirection, subtract(point, points[0]))).find((normal) => length(normal) > 1e-8);
  const reference = plane ? normalize(plane) : (Math.abs(firstDirection[2]) < 0.9 ? [0, 0, 1] : [0, 1, 0]);
  const builder = makeBuilder(material, label),
    count = points.length,
    ringPoint = (index, segment) => {
      const previous = points[index === 0 ? (closed ? count - 2 : 0) : index - 1],
        next = points[index === count - 1 ? (closed ? 1 : count - 1) : index + 1],
        tangent = normalize(subtract(next, previous)),
        normal = normalize(cross(tangent, reference)),
        binormal = normalize(cross(tangent, normal)),
        angle = (segment / radialSegments) * Math.PI * 2;
      return points[index].map((value, axis) => value + normal[axis] * Math.cos(angle) * radius + binormal[axis] * Math.sin(angle) * radius);
    };
  for (let index = 0; index < count - 1; index += 1)
    for (let segment = 0; segment < radialSegments; segment += 1) {
      const nextSegment = (segment + 1) % radialSegments,
        a = ringPoint(index, segment),
        b = ringPoint(index, nextSegment),
        c = ringPoint(index + 1, nextSegment),
        d = ringPoint(index + 1, segment);
      builder.triangle(a, b, c, "tube");
      builder.triangle(a, c, d, "tube");
    }
  if (!closed)
    for (const [index, reverse] of [[0, true], [count - 1, false]])
      for (let segment = 0; segment < radialSegments; segment += 1) {
        const a = ringPoint(index, segment),
          b = ringPoint(index, (segment + 1) % radialSegments);
        reverse ? builder.triangle(points[index], b, a) : builder.triangle(points[index], a, b);
      }
  return finish(builder);
}

function shieldOutline(component) {
  if (component.kind === "shapedShield") return shapedShieldOutline(component);
  const points = [];
  const segments = roundSegments(component.radius, component.radialSegments);
  for (let index = 0; index < segments; index += 1) {
    const angle = (index / segments) * Math.PI * 2;
    points.push([Math.cos(angle) * component.radius, Math.sin(angle) * component.radius]);
  }
  return points;
}

function rotatePlane([x, y], angle) {
  return [x * Math.cos(angle) - y * Math.sin(angle), x * Math.sin(angle) + y * Math.cos(angle)];
}

export function shieldFittingLayout(component) {
  const angle = ((component.fittingAngle ?? 0) * Math.PI) / 180,
    mirrored = component.mirrored ? -1 : 1,
    positionAxis = rotatePlane([1, 0], angle),
    fittingAxis = rotatePlane([0, 1], angle),
    spacing = component.fittingMode === "grip-and-strap" ? component.fittingSpacing * mirrored : 0,
    gripCenter = positionAxis.map((value) => value * -spacing / 2),
    strapCenter = positionAxis.map((value) => value * spacing / 2);
  return { angle, positionAxis, fittingAxis, gripCenter, strapCenter };
}

function shieldFittingMeshes(component) {
  const layout = shieldFittingLayout(component),
    clearance = component.fittingClearance,
    half = component.gripLength / 2,
    endpoint = (center, amount) => [center[0] + layout.fittingAxis[0] * amount, center[1] + layout.fittingAxis[1] * amount],
    gripEnds = [endpoint(layout.gripCenter, -half), endpoint(layout.gripCenter, half)],
    backs = gripEnds.map(([x, y]) => shieldSurfaceZ(component, x, y, false)),
    lifted = Math.min(...backs) - clearance,
    path = [
      [...gripEnds[0], backs[0] - component.gripRadius * 0.65],
      [...endpoint(layout.gripCenter, -half * 0.76), lifted],
      [...endpoint(layout.gripCenter, half * 0.76), lifted],
      [...gripEnds[1], backs[1] - component.gripRadius * 0.65],
    ],
    grip = sweptTube3d(path, component.gripRadius, component.gripMaterial ?? "wood", "shield handle", 12);
  const penetration = Math.min(0.0003, component.thickness * 0.15);
  for (let index = 0; index < grip.positions.length; index += 3) {
    const x = grip.positions[index], y = grip.positions[index + 1];
    grip.positions[index + 2] = Math.min(grip.positions[index + 2], shieldSurfaceZ(component, x, y, false) + penetration);
  }
  const fitted = makeBuilder(component.gripMaterial ?? "wood", "shield handle");
  for (const triangle of triangleVertices(grip)) fitted.triangle(...triangle, "handle");
  Object.assign(grip, finish(fitted));
  grip.shieldRole = "fitting";
  const meshes = [grip];
  if (component.fittingMode === "grip-and-strap") {
    const strapHalf = component.gripLength * 0.46,
      strapEnds = [endpoint(layout.strapCenter, -strapHalf), endpoint(layout.strapCenter, strapHalf)],
      strapBacks = strapEnds.map(([x, y]) => shieldSurfaceZ(component, x, y, false)),
      strapZ = Math.min(...strapBacks) - clearance * 0.72,
      band = transformMesh(box([component.strapWidth, component.gripLength * 0.92, component.strapThickness], component.strapMaterial ?? "leather", [0, 0, 0], "forearm strap"), [0, 0, component.fittingAngle ?? 0], [layout.strapCenter[0], layout.strapCenter[1], strapZ]);
    band.shieldRole = "fitting";
    meshes.push(band);
    for (let index = 0; index < strapEnds.length; index += 1) {
      const halfWidth = component.strapWidth / 2,
        corners = [-1, 1].flatMap((across) => [-1, 1].map((along) => [
          strapEnds[index][0] + layout.positionAxis[0] * across * halfWidth + layout.fittingAxis[0] * along * halfWidth,
          strapEnds[index][1] + layout.positionAxis[1] * across * halfWidth + layout.fittingAxis[1] * along * halfWidth,
        ])),
        contactBack = Math.min(...corners.map(([x, y]) => shieldSurfaceZ(component, x, y, false))),
        footBottom = strapZ - component.strapThickness / 2,
        footTop = contactBack + Math.min(component.strapThickness / 2, component.thickness * 0.2),
        footDepth = footTop - footBottom,
        foot = transformMesh(box([component.strapWidth, component.strapWidth, footDepth], component.strapMaterial ?? "leather", [0, 0, 0], "strap attachment"), [0, 0, component.fittingAngle ?? 0], [strapEnds[index][0], strapEnds[index][1], (footBottom + footTop) / 2]);
      foot.shieldRole = "fitting";
      meshes.push(foot);
    }
  }
  return meshes;
}

function shieldBossMesh(component) {
  const segments = roundSegments(component.bossRadius, 32), rows = detailSamples(12, 6);
  const builder = makeBuilder(component.bossMaterial ?? "steel", "shield boss");
  const wall = Math.min(0.0015, component.bossHeight * 0.25);
  const point = (row, segment, inner) => {
    const latitude = row / rows * Math.PI / 2, angle = segment / segments * Math.PI * 2;
    const radius = component.bossRadius * Math.cos(latitude);
    const x = radius * Math.cos(angle), y = radius * Math.sin(angle);
    return [x, y, shieldSurfaceZ(component, x, y, true) + component.bossHeight * Math.sin(latitude) - 0.0005 - (inner ? wall : 0)];
  };
  for (const inner of [false, true]) {
    const surface = inner ? "boss-inner" : "boss-outer";
    for (let row = 0; row < rows; row++) for (let segment = 0; segment < segments; segment++) {
      const next = (segment + 1) % segments;
      const a = point(row, segment, inner), b = point(row, next, inner), c = point(row + 1, next, inner), d = point(row + 1, segment, inner);
      inner ? builder.triangle(a, c, b, surface) : builder.triangle(a, b, c, surface);
      if (row < rows - 1) inner ? builder.triangle(a, d, c, surface) : builder.triangle(a, c, d, surface);
    }
  }
  for (let segment = 0; segment < segments; segment++) {
    const next = (segment + 1) % segments;
    const a = point(0, segment, false), b = point(0, next, false), c = point(0, next, true), d = point(0, segment, true);
    builder.triangle(a, c, b); builder.triangle(a, d, c);
  }
  return finish(builder);
}

export function shieldMeshes(component) {
  const parts = [],
    outline = shieldOutline(component),
    body = component.kind === "roundShield" ? roundShieldShell(component, component.label ?? "round shield body") : shapedShieldShell(component, component.label ?? "shaped shield body");
  body.shieldRole = "body";
  parts.push(body);
  if ((component.rimRadius ?? 0) > 0) {
    const centerline = outline.map(([x, y]) => [x, y, shieldCurve(component, x, y)]);
    centerline.push([...centerline[0]]);
    const rim = sweptTube3d(centerline, component.rimRadius, component.rimMaterial ?? "darkSteel", "shield rim", 12, true);
    rim.shieldRole = "rim";
    parts.push(rim);
  }
  if ((component.bossHeight ?? 0) > 0 && (component.bossRadius ?? 0) > 0) {
    const boss = shieldBossMesh(component);
    boss.shieldRole = "boss";
    parts.push(boss);
  }
  parts.push(...shieldFittingMeshes(component));
  return parts;
}

export function mergeMeshes(parts) {
  const merged = { positions: [], normals: [], colors: [], indices: [], parts, stats: {} };
  for (const part of parts) {
    const base = merged.positions.length / 3;
    for (const index of part.indices) merged.indices.push(base + index);
    for (const value of part.positions) merged.positions.push(value);
    for (const value of part.normals) merged.normals.push(value);
    for (const value of part.colors) merged.colors.push(value);
  }
  const density = parts[0]?.material?.density;
  if (density && parts.every((part) => part.material?.density === density)) merged.material = parts[0].material;
  merged.label = parts.map((part) => part.label).filter(Boolean).join(" + ");
  merged.stats = measureMesh(merged, parts);
  return merged;
}

function tetrahedralMassIntegrals(mesh, density, controlPoint) {
  let mass = 0;
  const firstMoment = [0, 0, 0];
  let transverseMoment = 0;
  for (const vertices of triangleVertices(mesh)) {
    const [a, b, c] = vertices;
    const signedVolume = (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6;
    const tetraMass = density * signedVolume;
    mass += tetraMass;
    for (let axis = 0; axis < 3; axis += 1) firstMoment[axis] += (tetraMass * (a[axis] + b[axis] + c[axis])) / 4;
    const secondMoment = (axis) =>
      (density * signedVolume * (a[axis] ** 2 + b[axis] ** 2 + c[axis] ** 2 + a[axis] * b[axis] + a[axis] * c[axis] + b[axis] * c[axis])) / 10;
    const shiftedSecondMoment = (axis) => secondMoment(axis) - 2 * controlPoint[axis] * ((tetraMass * (a[axis] + b[axis] + c[axis])) / 4) + controlPoint[axis] ** 2 * tetraMass;
    transverseMoment += shiftedSecondMoment(1) + (shiftedSecondMoment(0) + shiftedSecondMoment(2)) / 2;
  }
  return { mass, firstMoment, transverseMoment };
}

export function measureMassProperties(mesh, controlPoint = [0, 0, 0]) {
  const components = [];
  let massKg = 0;
  const firstMoment = [0, 0, 0];
  let momentOfInertiaKgM2 = 0;
  for (const part of mesh.parts ?? []) {
    const density = part.material?.density;
    if (!(density > 0)) continue;
    const integrals = tetrahedralMassIntegrals(part, density, controlPoint);
    if (!(integrals.mass > 0)) continue;
    massKg += integrals.mass;
    momentOfInertiaKgM2 += integrals.transverseMoment;
    for (let axis = 0; axis < 3; axis += 1) firstMoment[axis] += integrals.firstMoment[axis];
    components.push({ id: part.componentId ?? part.label ?? "part", label: part.label ?? part.componentId ?? "part", massKg: integrals.mass, centerOfMass: integrals.firstMoment.map((value) => value / integrals.mass) });
  }
  const centerOfMass = firstMoment.map((value) => value / massKg);
  const gripToTipM = Math.max(0, mesh.stats.bounds.max[1] - controlPoint[1]);
  return {
    massKg,
    centerOfMass,
    centerOfMassFromGripM: centerOfMass[1] - controlPoint[1],
    momentOfInertiaKgM2,
    balance: gripToTipM > 0 ? Math.sqrt(momentOfInertiaKgM2 / massKg) / gripToTipM : 1,
    gripToTipM,
    components,
  };
}

export function measureMesh(mesh, parts = []) {
  const bounds = {
    min: [Infinity, Infinity, Infinity],
    max: [-Infinity, -Infinity, -Infinity],
  };
  for (let index = 0; index < mesh.positions.length; index += 3) {
    for (let axis = 0; axis < 3; axis += 1) {
      bounds.min[axis] = Math.min(bounds.min[axis], mesh.positions[index + axis]);
      bounds.max[axis] = Math.max(bounds.max[axis], mesh.positions[index + axis]);
    }
  }
  let volume = 0;
  for (const [a, b, c] of triangleVertices(mesh)) {
    volume += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6;
  }
  const dimensions = bounds.min.map((value, axis) => bounds.max[axis] - value);
  return {
    bounds,
    dimensions,
    radius: length(dimensions) / 2,
    triangles: mesh.indices.length / 3,
    vertices: mesh.positions.length / 3,
    volume: Math.abs(volume),
    partCount: parts.length,
  };
}

function valueAtPath(object, path) {
  return path.split(".").reduce((current, key) => current?.[key], object);
}
function controlValue(definition, control) {
  if (control.target === "shaft") return definition.shaft?.[control.key];
  if (control.componentId) return definition.components.find((component) => component.id === control.componentId)?.[control.key];
  return valueAtPath(definition, control.path);
}

function localComponentFootprint(component) {
  if (!component) return [0, 0];
  if (component.kind === "roundShield") return [component.radius, Math.max(component.thickness / 2 + component.outerCurve + component.centerCurve + (component.bossHeight ?? 0), component.fittingClearance + component.gripRadius)];
  if (component.kind === "shapedShield") return [component.width / 2, Math.max(component.thickness / 2 + component.cylindricalCurve + component.centerCurve + (component.bossHeight ?? 0), component.fittingClearance + component.gripRadius)];
  if (component.kind === "guard") return [component.width / 2, component.thickness / 2];
  if (component.kind === "guardAssembly") return [Math.max(...Object.values(component.nodes).map((p) => Math.abs(p[0]))) + 0.01, Math.max(...Object.values(component.nodes).map((p) => Math.abs(p[2]))) + 0.01];
  if (component.kind === "box") return [component.size[0] / 2, component.size[2] / 2];
  if (["socket", "pommel"].includes(component.kind)) {
    if (component.kind === "pommel" && (component.construction === "composite" ? component.baseConstruction : component.construction) !== "lathed") return [(component.diameter ?? 0.06) * (component.widthScale ?? 1) / 2, ["plate", "outline"].includes(component.construction) ? (component.thickness ?? 0.018) / 2 : (component.diameter ?? 0.06) * (component.widthScale ?? 1) / 2];
    const radius = Math.max(...component.profile.map((point) => point[1]));
    return [radius, radius];
  }
  if (["grip", "collar", "sleeve"].includes(component.kind)) return [component.radius, component.radius];
  if (component.kind === "ovalGrip") {
    const scale = Math.max(component.bottomScale ?? 1, component.topScale ?? 1);
    return [(component.width * scale) / 2, (component.thickness * scale) / 2];
  }
  if (component.kind === "mace") return [component.rootRadius, component.rootRadius];
  if (component.kind === "axe") return [component.rootWidth ?? component.width * 0.18, component.thickness / 2];
  if (["spear", "partisan"].includes(component.kind)) return [(component.rootWidth ?? component.width * 0.4) / 2, component.thickness / 2];
  if (["glaive", "bill"].includes(component.kind)) return [component.root ?? 0.03, component.thickness / 2];
  if (component.kind === "fork") return [component.baseWidth / 2, component.thickness / 2];
  if (component.kind === "beak") return [(component.rootSection ?? component.radius * 1.5) / 2, component.thickness / 2];
  if (component.kind === "facetedBeak") return [component.root / 2, (component.tipThickness ?? component.thickness) / 2];
  if (component.kind === "hammer") return [component.neck / 2, (component.faceThickness ?? component.thickness) / 2];
  if (["tube", "ringGuard", "figureEight", "knuckleBow"].includes(component.kind)) {
    const radius = component.radius ?? component.bar ?? 0.006;
    return [radius, radius];
  }
  if (["blade", "sectionBlade", "diamondBlade"].includes(component.kind)) return [component.width / 2, component.thickness / 2];
  return [0.005, 0.005];
}

function componentFootprint(component, asParent = false) {
  if (!component) return [0, 0];
  const [halfX, halfZ] = localComponentFootprint(component),
    range = componentRange(component),
    halfY = asParent ? Math.abs(range[1] - range[0]) / 2 : 0;
  const axes = [rotatePoint([1, 0, 0], component.rotation), rotatePoint([0, 1, 0], component.rotation), rotatePoint([0, 0, 1], component.rotation)],
    extents = [halfX, halfY, halfZ];
  return [0, 2].map((worldAxis) => axes.reduce((sum, axis, localAxis) => sum + Math.abs(axis[worldAxis]) * extents[localAxis], 0));
}

export function closedManifoldErrors(mesh, label = "component") {
  const precision = 1e7,
    edges = new Map();
  const key = (offset) =>
    mesh.positions
      .slice(offset, offset + 3)
      .map((value) => Math.round(value * precision))
      .join(",");
  for (let index = 0; index < mesh.indices.length; index += 3) {
    const vertices = mesh.indices.slice(index, index + 3).map((vertex) => key(vertex * 3));
    for (const [from, to] of [
      [vertices[0], vertices[1]],
      [vertices[1], vertices[2]],
      [vertices[2], vertices[0]],
    ]) {
      const edge = from < to ? `${from}|${to}` : `${to}|${from}`,
        direction = from < to ? 1 : -1;
      const state = edges.get(edge) ?? [0, 0];
      state[direction > 0 ? 0 : 1] += 1;
      edges.set(edge, state);
    }
  }
  const invalid = [...edges.values()].filter(([forward, reverse]) => forward !== 1 || reverse !== 1).length;
  return invalid ? [`${label}: ${invalid} boundary/non-manifold edges`] : [];
}

function validateWeaponUnchecked(definition, controls = [], options = {}) {
  const errors = schemaErrors(definition);
  if (errors.length) return { valid: false, errors, resolved: null, mesh: null };
  let resolved;
  try {
    resolved = resolveDefinition(definition);
  } catch (error) {
    return {
      valid: false,
      errors: [`definition resolution failed: ${error.message}`],
      resolved: null,
      mesh: null,
    };
  }
  errors.push(...resolved._resolutionErrors);
  const finitePositive = (value, label, allowZero = false) => {
    if (!Number.isFinite(value) || (allowZero ? value < 0 : value <= 0)) errors.push(`${label} must be ${allowZero ? "non-negative" : "positive"}`);
  };
  if (resolved.gripClearance !== undefined) {
    const gripBase = resolved._frames["grip.base"],
      gripTop = resolved._frames["grip.top"];
    if (!gripBase || !gripTop) errors.push("gripClearance requires grip.base and grip.top frames");
    else if (resolved.gripClearance > length(subtract(gripTop, gripBase)) + 1e-9) errors.push("gripClearance must remain within the modeled grip");
  }
  if (resolved.shaft) {
    finitePositive(resolved.shaft.length, "shaft.length");
    finitePositive(resolved.shaft.radius, "shaft.radius");
    if (effectiveGripRadius(resolved.shaft) > MAX_ROUND_GRIP_RADIUS_M + 1e-9) errors.push(`shaft: effective grip radius exceeds anatomical maximum ${MAX_ROUND_GRIP_RADIUS_M} m`);
  }
  const resolvedById = new Map(resolved.components.map((component) => [component.id, component]));
  for (const component of resolved.components) {
    for (const value of component.offset ?? []) if (!Number.isFinite(value)) errors.push(`${component.id}: offset is not finite`);
    for (const key of ["length", "radius", "width", "height", "thickness", "rootWidth", "baseWidth", "tineWidth", "rootSection", "tipSection", "face", "neck", "faceThickness", "flangeThickness"]) if (component[key] !== undefined) finitePositive(component[key], `${component.id}.${key}`);
    if (component.kind === "grip" && effectiveGripRadius(component) > MAX_ROUND_GRIP_RADIUS_M + 1e-9) errors.push(`${component.id}: effective grip radius exceeds anatomical maximum ${MAX_ROUND_GRIP_RADIUS_M} m`);
    if (component.kind === "pommel") {
      for (const key of ["widthScale", "lengthScale"]) if (component[key] !== undefined && (component[key] < 0.5 || component[key] > 2)) errors.push(`${component.id}.${key} must be between 0.5 and 2`);
      for (const [key, min, max] of [["facets", 4, 24], ["fluteCount", 3, 24]]) if (component[key] !== undefined && (!Number.isInteger(component[key]) || component[key] < min || component[key] > max)) errors.push(`${component.id}.${key} must be an integer within ${min}–${max}`);
      for (const [key, min, max] of [["fluteDepth", 0, 0.3], ["faceConvexity", 0, 0.5], ["rimBevel", 0.01, 0.45], ["notchDepth", 0.05, 0.5], ["lobeSpread", 0.5, 1], ["twist", -180, 180]]) if (component[key] !== undefined && (component[key] < min || component[key] > max)) errors.push(`${component.id}.${key} must be within ${min}–${max}`);
    }
    if (component.kind === "guard") {
      if (component.tipScale !== undefined && (component.tipScale < 0.45 || component.tipScale > 1.5)) errors.push(`${component.id}.tipScale must be between 0.45 and 1.5`);
      if (component.terminalSwell !== undefined && (component.terminalSwell < 0 || component.terminalSwell > 1)) errors.push(`${component.id}.terminalSwell must be between 0 and 1`);
    }
    if (component.kind === "ovalGrip") {
      const scale = Math.max(component.bottomScale ?? 1, component.topScale ?? 1);
      if (component.width * scale > MAX_SWORD_GRIP_WIDTH_M + 1e-9) errors.push(`${component.id}: grip width exceeds anatomical maximum ${MAX_SWORD_GRIP_WIDTH_M} m`);
      if (component.thickness * scale > MAX_SWORD_GRIP_THICKNESS_M + 1e-9) errors.push(`${component.id}: grip thickness exceeds anatomical maximum ${MAX_SWORD_GRIP_THICKNESS_M} m`);
      if (component.width <= component.thickness) errors.push(`${component.id}: oval grip width must exceed thickness`);
    }
    if (["roundShield", "shapedShield"].includes(component.kind)) {
      if (component.thickness < (component.material === "steel" || component.material === "darkSteel" ? 0.001 : 0.006)) errors.push(`${component.id}: body thickness is below the material construction minimum`);
      for (const key of ["thickness", "gripLength", "gripRadius", "fittingClearance", "strapWidth", "strapThickness"]) finitePositive(component[key], `${component.id}.${key}`);
      for (const key of ["rimRadius", "bossRadius", "bossHeight"])
        if (component[key] !== undefined) finitePositive(component[key], `${component.id}.${key}`, true);
      if (!(component.fittingAngle >= 0 && component.fittingAngle <= 90)) errors.push(`${component.id}.fittingAngle must be within 0–90 degrees`);
      if (component.fittingMode === "grip-and-strap") finitePositive(component.fittingSpacing, `${component.id}.fittingSpacing`);
      if ((component.rimRadius ?? 0) > Math.max(0.006, component.thickness * 1.5)) errors.push(`${component.id}: rim radius is too large for the shield edge`);
      const fittingParts = shieldFittingMeshes(component);
      for (const part of fittingParts) {
        let touchesBack = false;
        for (let index = 0; index < part.positions.length; index += 3) {
          const [x, y, z] = part.positions.slice(index, index + 3);
          const back = shieldSurfaceZ(component, x, y, false),
            tolerance = Math.max(component.gripRadius, component.strapThickness) * 1.1;
          if (Math.abs(z - back) <= tolerance) touchesBack = true;
          if (z > shieldSurfaceZ(component, x, y, true) + 1e-7) {
            errors.push(`${component.id}: ${part.label} clips through the shield face`);
            break;
          }
        }
        if (part.label !== "forearm strap" && !touchesBack) errors.push(`${component.id}: ${part.label} is detached from the shield back`);
      }
      if (component.fittingMode === "grip-and-strap" && fittingParts.filter((part) => part.label === "strap attachment").length !== 2) errors.push(`${component.id}: forearm strap requires two attached feet`);
    }
    if (component.kind === "roundShield") {
      for (const key of ["outerCurve", "centerCurve"]) finitePositive(component[key], `${component.id}.${key}`, true);
      finitePositive(component.centerRadius, `${component.id}.centerRadius`);
      if (component.centerRadius >= component.radius) errors.push(`${component.id}.centerRadius must remain inside the shield rim`);
      if (component.rings < 3) errors.push(`${component.id}.rings must be at least 3`);
      if (component.radialSegments < 12) errors.push(`${component.id}.radialSegments must be at least 12`);
      if (component.outerCurve + component.centerCurve > component.radius * 0.8) errors.push(`${component.id}: combined curvature is implausibly deep`);
      if (component.bossRadius >= component.radius - (component.rimRadius ?? 0)) errors.push(`${component.id}: boss does not fit inside the round shield rim`);
      const layout = shieldFittingLayout(component),
        farthestCenter = component.fittingMode === "grip-and-strap" ? component.fittingSpacing / 2 : 0;
      if (Math.hypot(farthestCenter, component.gripLength / 2) + Math.max(component.gripRadius, component.strapWidth / 2) >= component.radius - (component.rimRadius ?? 0)) errors.push(`${component.id}: fittings do not fit inside the round shield rim`);
    }
    if (component.kind === "shapedShield") {
      for (const key of ["topDepth", "bottomDepth", "topRoundness", "bottomRoundness", "sideTaper", "cornerRadius", "cylindricalCurve", "centerCurve"]) finitePositive(component[key], `${component.id}.${key}`, true);
      for (const key of ["centerWidth", "centerHeight"]) finitePositive(component[key], `${component.id}.${key}`);
      if (component.topRoundness > 1 || component.bottomRoundness > 1) errors.push(`${component.id}: ending roundness must be within 0–1`);
      if (component.sideTaper >= 0.8) errors.push(`${component.id}: side taper must remain below 0.8`);
      if (component.cornerRadius > Math.min(component.width, component.height) * 0.25) errors.push(`${component.id}: corner radius is too large for the shield body`);
      if (component.topDepth > component.height * 0.45 || component.bottomDepth > component.height * 0.65) errors.push(`${component.id}: outline ending depth is too large for its height`);
      if (component.cylindricalCurve > component.width * 0.45) errors.push(`${component.id}: cylindrical curvature is too deep for its width`);
      if (component.centerWidth > component.width) errors.push(`${component.id}: center bump width exceeds the shield body`);
      if (component.edgeSegments < 6) errors.push(`${component.id}.edgeSegments must be at least 6`);
      errors.push(...simplePolygonErrors(shapedShieldOutline(component), `${component.id} outline`));
      if (component.bossRadius >= Math.min(component.width / 2, component.height / 2) - (component.rimRadius ?? 0)) errors.push(`${component.id}: boss does not fit inside the shaped shield rim`);
      const lateral = component.fittingMode === "grip-and-strap" ? component.fittingSpacing / 2 : 0;
      if (lateral + component.strapWidth / 2 >= component.width / 2 - (component.rimRadius ?? 0) || component.gripLength / 2 + component.gripRadius >= component.height / 2) errors.push(`${component.id}: fittings do not fit inside the shaped shield rim`);
    }
    if (component.size) component.size.forEach((value, axis) => finitePositive(value, `${component.id}.size[${axis}]`));
    if (component.profile) {
      component.profile.forEach((point, index) => {
        if (!Number.isFinite(point[0])) errors.push(`${component.id}.profile[${index}] height is not finite`);
        finitePositive(point[1], `${component.id}.profile[${index}] radius`);
      });
      for (let index = 1; index < component.profile.length; index += 1) if (component.profile[index][0] <= component.profile[index - 1][0]) errors.push(`${component.id}: profile heights must increase`);
    }
    if (component._resolvedAttachment?.distance > 1e-6) errors.push(`${component.id}: detached from ${component._resolvedAttachment.target}`);
    if ((component._resolvedAttachment?.overlap ?? 0) < 0) errors.push(`${component.id}: attachment overlap cannot be negative`);
    if ((component._resolvedAttachment?.overlap ?? 0) > 0.15) errors.push(`${component.id}: attachment overlap exceeds the contact envelope`);
    const raw = definition.components.find((candidate, index) => (candidate.id ?? candidate.label ?? `component-${index}`) === component.id);
    if (!raw.mount && !raw.attach && !raw.stretchBetween) errors.push(`${component.id}: every component must declare a mount, attachment, or stretch parent`);
    if (raw.attach && raw.offset !== undefined) errors.push(`${component.id}: attached components must express placement only through attach.offset`);
    const rawOffset = raw?.offset ?? [0, 0, 0],
      attachOffset = raw?.attach?.offset ?? [0, 0, 0],
      childFootprint = componentFootprint(component);
    if (raw.mount?.startsWith("shaft-top") && resolved.shaft) {
      const shaftRadius = resolved.shaft.radius * (resolved.shaft.topScale ?? 0.92),
        contactOffset = component.fitShaftSide ? component.offset : rawOffset;
      if (Math.abs(contactOffset[0] ?? 0) > shaftRadius + childFootprint[0] + 0.002 || Math.abs(contactOffset[2] ?? 0) > shaftRadius + childFootprint[1] + 0.002) errors.push(`${component.id}: mounted root does not overlap the shaft footprint`);
      if (["socket", "sleeve", "spear", "mace"].includes(component.kind) && Math.hypot(rawOffset[0] ?? 0, rawOffset[2] ?? 0) > 1e-6) errors.push(`${component.id}: axial head must mount concentrically on the shaft`);
    }
    if (raw.attach) {
      const [ownerId, frameName = "origin"] = raw.attach.to.split("."),
        shaftOwner = ownerId === "shaft",
        target = ownerId === "weapon" || shaftOwner ? null : resolvedById.get(ownerId);
      const shaftScale = frameName === "top" ? (resolved.shaft?.topScale ?? 0.92) : frameName === "bottom" || frameName === "base" || frameName === "origin" ? (resolved.shaft?.bottomScale ?? 1) : 1;
      const targetFootprint = (ownerId === "weapon" || shaftOwner) && resolved.shaft ? [resolved.shaft.radius * shaftScale, resolved.shaft.radius * shaftScale] : componentFootprint(target, true);
      if (Math.abs(attachOffset[0] ?? 0) > targetFootprint[0] + childFootprint[0] + 0.002 || Math.abs(attachOffset[2] ?? 0) > targetFootprint[1] + childFootprint[1] + 0.002) errors.push(`${component.id}: attachment root does not overlap parent footprint ${raw.attach.to}`);
      if ((ownerId === "weapon" || shaftOwner) && resolved.shaft && ["socket", "sleeve", "spear", "mace"].includes(component.kind) && Math.hypot(attachOffset[0] ?? 0, attachOffset[2] ?? 0) > 1e-6) errors.push(`${component.id}: axial component must attach concentrically to the shaft root`);
      const targetFrame = resolved._frames[raw.attach.to],
        contact = component._resolvedAttachment?.contact;
      if (targetFrame && contact) {
        const expectedContact = [targetFrame[0] + attachOffset[0], targetFrame[1] + attachOffset[1] - (raw.attach.overlap ?? 0), targetFrame[2] + attachOffset[2]];
        if (length(subtract(contact, expectedContact)) > 1e-6) errors.push(`${component.id}: resolved contact differs from named frame ${raw.attach.to}`);
        const targetRange = ownerId === "weapon" || shaftOwner ? [0, resolved.shaft?.length ?? 0] : componentRange(target);
        const targetOrigin = ownerId === "weapon" || shaftOwner ? [0, 0, 0] : (target.offset ?? [0, 0, 0]);
        const localContact = ownerId === "weapon" || shaftOwner ? contact : inverseRotatePoint(subtract(contact, targetOrigin), target.rotation);
        if (localContact[1] < targetRange[0] - 0.01 || localContact[1] > targetRange[1] + 0.01) errors.push(`${component.id}: attachment contact lies outside parent axial geometry ${raw.attach.to}`);
      }
    }
    if (raw.mount === "component-end") {
      const target = resolved.components.find((candidate) => candidate.id === raw.anchor || candidate.label === raw.anchor),
        targetFootprint = componentFootprint(target, true);
      if (!target || Math.abs(rawOffset[0] ?? 0) > targetFootprint[0] + childFootprint[0] + 0.002 || Math.abs(rawOffset[2] ?? 0) > targetFootprint[1] + childFootprint[1] + 0.002) errors.push(`${component.id}: component-end mount does not overlap ${raw.anchor}`);
    }
    if (raw.stretchBetween && component._resolvedStretch) {
      for (const [endName, point, targetPoint, frame] of [
        ["start", component._resolvedStretch.start, component._resolvedStretch.fromTarget, raw.stretchBetween[0]],
        ["end", component._resolvedStretch.end, component._resolvedStretch.toTarget, raw.stretchBetween[1]],
      ]) {
        const ownerId = frame.split(".")[0],
          target = ownerId === "shaft" ? null : resolvedById.get(ownerId),
          parentFootprint = ownerId === "shaft" && resolved.shaft ? [resolved.shaft.radius, resolved.shaft.radius] : componentFootprint(target, true);
        if (Math.abs(point[0] - targetPoint[0]) > parentFootprint[0] + childFootprint[0] + 0.002 || Math.abs(point[2] - targetPoint[2]) > parentFootprint[1] + childFootprint[1] + 0.002 || Math.abs(point[1] - targetPoint[1]) > 0.01) errors.push(`${component.id}: stretched ${endName} does not contact frame ${frame}`);
      }
    }
    if (["socket", "sleeve"].includes(component.kind) && (component.mount?.startsWith("shaft-top") || raw.attach?.to?.startsWith("shaft.")) && resolved.shaft) {
      const frameName = raw.attach?.to?.split(".")[1] ?? "top",
        scale = frameName === "top" ? (resolved.shaft.topScale ?? 0.92) : frameName === "center" ? ((resolved.shaft.bottomScale ?? 1) + (resolved.shaft.topScale ?? 0.92)) / 2 : (resolved.shaft.bottomScale ?? 1);
      const contactRadius = resolved.shaft.radius * scale,
        wall = component.wall ?? 0.003;
      const outerRadius = component.kind === "socket" ? Math.min(...component.profile.map((point) => point[1])) : Math.min(component.radius, component.topRadius ?? component.radius);
      if (outerRadius + 1e-9 < contactRadius + wall) errors.push(`${component.id}: outer radius ${outerRadius.toFixed(4)} cannot fit shaft ${contactRadius.toFixed(4)} plus ${wall.toFixed(4)} wall`);
    }
    if (component.kind === "axe") {
      if (component.rootWidth >= component.width * 0.55) errors.push(`${component.id}: eye/root width must remain narrower than axe reach`);
      if (!(component.upperShoulder > component.lowerShoulder)) errors.push(`${component.id}: upper shoulder must remain above lower shoulder`);
    }
    if (component.kind === "spear") {
      if (!(component.bellyPosition > 0 && component.bellyPosition < 1)) errors.push(`${component.id}: belly position must be between root and point`);
      if (component.rootWidth >= component.width) errors.push(`${component.id}: root width must be narrower than maximum width`);
      finitePositive(component.acuteness, `${component.id}.acuteness`);
    }
    if (component.kind === "hammer" && (!(component.neckRatio > 0) || component.neckRatio > 0.88)) errors.push(`${component.id}: neck ratio must be within 0–0.88`);
    if (component.kind === "beak" || component.kind === "facetedBeak") {
      if (!(component.bendPosition > 0 && component.bendPosition < 1)) errors.push(`${component.id}: bend position must lie along the beak`);
      if ((component.tipSection ?? component.tip) >= (component.rootSection ?? component.root)) errors.push(`${component.id}: tip section must remain smaller than root section`);
    }
    if (component.kind === "fork") {
      if (!(component.crotch > 0 && component.crotch + component.crotchRound < 0.8)) errors.push(`${component.id}: crotch depth/rounding leaves no tapered tine`);
      if (component.tineWidth * 2 >= component.width) errors.push(`${component.id}: tines overlap at the selected spread`);
    }
    if (component.kind === "partisan" && component.rootWidth >= component.width) errors.push(`${component.id}: blade root must remain narrower than belly`);
    if (component.kind === "mace") {
      finitePositive(component.rootRadius, `${component.id}.rootRadius`);
      finitePositive(component.shoulderRadius, `${component.id}.shoulderRadius`);
      finitePositive(component.cuspRadius, `${component.id}.cuspRadius`);
      if (component.cuspRadius <= Math.max(component.rootRadius, component.shoulderRadius)) errors.push(`${component.id}: cusp radius must exceed root and shoulder radii`);
      if (!(component.cuspHeight > 0 && component.cuspHeight < 1)) errors.push(`${component.id}: cusp height must be between 0 and 1`);
      if (!Number.isInteger(component.flanges) || component.flanges < 3) errors.push(`${component.id}: flange count must be an integer of at least 3`);
      finitePositive(component.flangeThickness, `${component.id}.flangeThickness`);
    }
  }
  for (const control of controls) {
    const value = controlValue(definition, control);
    if (!Number.isFinite(value) || value < control.min - 1e-9 || value > control.max + 1e-9) errors.push(`${control.label}: value is outside ${control.min}–${control.max}`);
    if (Number.isFinite(value) && control.step > 0 && Math.abs((value - control.min) / control.step - Math.round((value - control.min) / control.step)) > 1e-6) errors.push(`${control.label}: value does not align to step ${control.step}`);
    if (Number.isFinite(value) && control.step >= 1 && !Number.isInteger(value)) errors.push(`${control.label}: discrete value must be integral`);
    for (const path of control.paths ?? []) if (Math.abs(valueAtPath(definition, path) - value) > 1e-8) errors.push(`${control.label}: linked target ${path} differs`);
  }
  if (errors.length) return { valid: false, errors, resolved, mesh: null };
  try {
    const mesh = buildWeapon(definition, options);
    if (!mesh.positions.length || !mesh.positions.every(Number.isFinite) || !mesh.normals.every(Number.isFinite)) errors.push("generated geometry is empty or non-finite");
    if (mesh.stats.dimensions.some((value) => !Number.isFinite(value) || value <= 0)) errors.push("generated bounds are inverted or empty");
    if (!(signedVolume(mesh) > 0)) errors.push("generated orientation has non-positive signed volume");
    for (const part of mesh.parts) {
      if (!part.positions.length || part.indices.length % 3 !== 0) {
        errors.push(`${part.label}: part is empty or has incomplete triangles`);
        continue;
      }
      if (Math.abs(signedVolume(part)) <= 1e-12) errors.push(`${part.label}: part has near-zero enclosed volume`);
      if (signedVolume(part) < -1e-9) errors.push(`${part.label}: inward orientation`);
      errors.push(...closedManifoldErrors(part, part.label));
      for (let index = 0; index < part.indices.length; index += 3) {
        const vertices = part.indices.slice(index, index + 3);
        const [a, b, c] = vertices.map((vertex) => part.positions.slice(vertex * 3, vertex * 3 + 3));
        const geometric = normalize(cross(subtract(b, a), subtract(c, a)));
        if (vertices.some((vertex) => dot(geometric, part.normals.slice(vertex * 3, vertex * 3 + 3)) <= 0)) {
          errors.push(`${part.label}: stored normal opposes triangle winding`);
          break;
        }
      }
    }
    const aspect = 930 / 632;
    for (const [pose, yaw, pitch] of [
      ["front", 0, 0],
      ["oblique", 0.68, 0.18],
    ]) {
      const fit = projectedFit(mesh.positions, mesh.stats.bounds, aspect, yaw, pitch);
      if (!fit.contained) errors.push(`${pose} camera projection exceeds the framing margin`);
    }
    return { valid: errors.length === 0, errors, resolved, mesh };
  } catch (error) {
    return {
      valid: false,
      errors: [...errors, `build failed: ${error.message}`],
      resolved,
      mesh: null,
    };
  }
}

export function validateWeapon(definition, controls = [], options = {}) {
  try {
    return withDetail(options.lod ?? "medium", () => validateWeaponUnchecked(definition, controls, options));
  } catch (error) {
    return {
      valid: false,
      errors: [`validation failed unexpectedly: ${error?.message ?? String(error)}`],
      resolved: null,
      mesh: null,
    };
  }
}

export function buildWeapon(input, { lod = "medium" } = {}) {
  return withDetail(lod, () => buildWeaponAtDetail(input));
}

function buildWeaponAtDetail(input) {
  const definition = resolveDefinition(input);
  const parts = [];
  const shaft = definition.shaft;
  if (shaft) {
    const shaftMesh = lathe(
        [
          [0, shaft.radius * (shaft.bottomScale ?? 1)],
          [shaft.length, shaft.radius * (shaft.topScale ?? 0.92)],
        ],
        shaft.segments ?? 16,
        shaft.material ?? "wood",
        [0, 0, 0],
        "shaft",
        1,
        true,
      );
    shaftMesh.componentId = "shaft";
    parts.push(shaftMesh);
  }
  const add = (mesh, component, offset) => {
    const transformed = transformMesh(mesh, component.rotation, offset);
    transformed.componentId = component.id;
    parts.push(transformed);
  };
  for (const component of definition.components) {
    const offset = component.offset ?? [0, 0, 0];
    if (["roundShield", "shapedShield"].includes(component.kind)) {
      for (const shieldPart of shieldMeshes(component)) {
        const transformed = transformMesh(shieldPart, component.rotation, offset);
        transformed.componentId = component.id;
        parts.push(transformed);
      }
    }
    if (component.kind === "blade") add(curvedBlade(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "sectionBlade") add(sectionBlade(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "diamondBlade") add(diamondBlade(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "axe") add(axeHead(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "spear") add(spearHead(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "guard") add(guard({ ...component, _gripWidth: definition.components.find((part) => part.id === "grip")?.width }, [0, 0, 0], component.label), component, offset);
    if (component.kind === "guardAssembly") add(guardAssembly(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "knuckleBow") add(knuckleBow(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "ringGuard") add(ringGuard(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "tube") add(tubePath(component.points, component.radius, component.material ?? "steel", [0, 0, 0], component.label, component.radialSegments ?? 8), component, offset);
    if (component.kind === "figureEight") add(figureEightGuard(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "fork") add(forgedFork(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "partisan") add(partisanBlade(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "glaive") add(glaiveBlade(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "facetedBeak") add(facetedBeak(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "bill") add(billHead(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "box" && component.label?.includes("branch junction"))
      add(
        lathe(
          [
            [-component.size[2] / 2, component.size[0] * 0.55],
            [0, component.size[0] * 0.8],
            [component.size[2] / 2, component.size[0] * 0.55],
          ],
          10,
          component.material ?? "darkSteel",
          [0, 0, 0],
          component.label,
        ),
        { ...component, rotation: [90, 0, 0] },
        offset,
      );
    else if (component.kind === "box") add(box(component.size, component.material ?? "steel", [0, 0, 0], component.label), component, offset);
    if (component.kind === "pick") add(coneX(component.length, component.radius, component.direction ?? -1, component.material ?? "steel", [0, 0, 0], component.label), component, offset);
    if (component.kind === "beak") add(curvedBeak(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "hammer") add(hammerPoll(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "socket") add(component._shaftContactRadius ? hollowSocket(component.profile, component._shaftContactRadius, component.material ?? "steel", component.label) : lathe(component.profile, component.segments ?? 14, component.material ?? "steel", [0, 0, 0], component.label), component, offset);
    if (component.kind === "pommel") add(pommelMesh(component, [0, 0, 0], component.label), component, offset);
    if (component.kind === "collar")
      add(
        lathe(
          [
            [-component.width / 2, component.radius * 0.88],
            [-component.width * 0.32, component.radius],
            [component.width * 0.32, component.radius],
            [component.width / 2, component.radius * 0.88],
          ],
          component.segments ?? 16,
          component.material ?? "steel",
          [0, 0, 0],
          component.label,
        ),
        component,
        offset,
      );
    if (component.kind === "sleeve") {
      const profile = [[0, component.radius], [component.length, component.topRadius ?? component.radius * 0.88]];
      add(component._shaftContactRadius ? hollowSocket(profile, component._shaftContactRadius, component.material ?? "darkSteel", component.label) : lathe(profile, component.segments ?? 16, component.material ?? "darkSteel", [0, 0, 0], component.label), component, offset);
    }
    if (component.kind === "mace") {
      const half = component.length / 2;
      const rootRadius = component.rootRadius ?? component.radius * (component.waist ?? 0.36);
      const shoulderRadius = component.shoulderRadius ?? rootRadius * 0.82;
      const cuspRadius = component.cuspRadius ?? component.radius * 1.4;
      const crownLength = component.crownLength ?? 0;
      const coreProfile = [
        [-half, rootRadius],
        [half * 0.72, rootRadius * 0.9],
        [half, shoulderRadius],
      ];
      if (crownLength > 0) coreProfile.push([half + crownLength, 0.001]);
      add(lathe(coreProfile, component.segments ?? 12, component.material ?? "steel", [0, 0, 0], component.label), component, offset);
      const flanges = component.flanges ?? 0;
      for (let index = 0; index < flanges; index += 1) {
        const angle = (index / flanges) * Math.PI * 2;
        const flange = maceFlangeMesh(
          {
            ...component,
            rootRadius,
            shoulderRadius,
            cuspRadius,
            flangeThickness: component.flangeThickness ?? component.flangeDepth ?? 0.0025,
          },
          component.material ?? "steel",
          `${component.label} flange`,
        );
        parts.push(transformMesh(flange, [0, (-angle * 180) / Math.PI, 0], offset));
      }
    }
    if (component.kind === "grip") {
      add(
        lathe(
          [
            [0, component.radius * (component.bottomScale ?? 1)],
            [component.length, component.radius * (component.topScale ?? 1)],
          ],
          component.segments ?? 14,
          component.material ?? "leather",
          [0, 0, 0],
          component.label,
        ),
        component,
        offset,
      );
      const turns = component.wraps ?? 0;
      for (let turn = 1; turn < turns; turn += 1) {
        const y = offset[1] + (component.length * turn) / turns;
        parts.push(
          lathe(
            [
              [-0.002, component.radius * 1.018],
              [0.002, component.radius * 1.018],
            ],
            16,
            component.wrapMaterial ?? "darkSteel",
            [offset[0], y, offset[2]],
            "grip binding",
          ),
        );
      }
    }
    if (component.kind === "ovalGrip") {
      const parent = definition.components.find((part) => `${part.id}.top` === component.attach?.to);
      const baseRadius = component.width * (component.bottomScale ?? 1) / 2;
      let seatRadius = baseRadius;
      if (parent?.kind === "pommel" && parent.profile && parent.construction === "lathed" && !parent.rotation && !component.rotation) {
        const y = offset[1] - parent.offset[1];
        const interval = parent.profile.findIndex((point) => point[0] >= y);
        const upper = parent.profile[Math.max(0, interval)], lower = parent.profile[Math.max(0, interval - 1)];
        const t = upper[0] === lower[0] ? 0 : Math.max(0, Math.min(1, (y - lower[0]) / (upper[0] - lower[0])));
        seatRadius = Math.min(baseRadius, lower[1] + (upper[1] - lower[1]) * t);
      } else if (parent?.kind === "pommel" && !parent.rotation && !component.rotation) {
        const construction = parent.construction === "composite" ? parent.baseConstruction : parent.construction;
        const neckFraction = construction === "outline" && parent.outlineStyle === "fan" ? 0.36 : parent.shoulderWidth ?? 0.42;
        seatRadius = Math.min(baseRadius, construction === "plate" ? 0.009 : (parent.diameter ?? 0.055) * neckFraction * (parent.widthScale ?? 1) / 2, (parent.thickness ?? 0.018) * 0.48 * component.width / component.thickness);
      }
      add(
        lathe(
          [
            [0, seatRadius],
            [Math.min(0.016, component.length * 0.15), baseRadius],
            [component.length, (component.width * (component.topScale ?? 1)) / 2],
          ],
          component.segments ?? 16,
          component.material ?? "leather",
          [0, 0, 0],
          component.label,
          component.thickness / component.width,
          true,
        ),
        component,
        offset,
      );
    }
    if (component.kind === "slabGrip") {
      add(box([component.width, component.length, component.thickness], "darkSteel", [0, component.length / 2, 0], `${component.label} tang`), component, offset);
      for (const side of [-1, 1]) add(box([component.width * 0.92, component.length * 0.92, component.scaleThickness], component.material ?? "wood", [0, component.length / 2, (side * (component.thickness + component.scaleThickness)) / 2], `${component.label} scale`), component, offset);
    }
  }
  const mesh = mergeMeshes(parts);
  mesh.resolvedDefinition = definition;
  return mesh;
}
