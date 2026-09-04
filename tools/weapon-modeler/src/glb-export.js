import { mat4Multiply, normalize } from "./math.js";

const GLB_MAGIC = 0x46546c67;
const GLB_VERSION = 2;
const JSON_CHUNK = 0x4e4f534a;
const BIN_CHUNK = 0x004e4942;

function bytes(value) {
  return value instanceof Uint8Array
    ? value
    : new Uint8Array(value.buffer ?? value, value.byteOffset ?? 0, value.byteLength);
}

function padded(value, fill = 0) {
  const result = new Uint8Array(Math.ceil(value.byteLength / 4) * 4);
  result.fill(fill);
  result.set(value);
  return result;
}

export function parseGlb(value) {
  const source = bytes(value);
  const view = new DataView(source.buffer, source.byteOffset, source.byteLength);
  if (source.byteLength < 20 || view.getUint32(0, true) !== GLB_MAGIC) throw new Error("Rig is not a GLB file");
  if (view.getUint32(4, true) !== GLB_VERSION) throw new Error("Rig must use glTF 2.0");
  if (view.getUint32(8, true) !== source.byteLength) throw new Error("Rig GLB length is invalid");
  let offset = 12, document, binary = new Uint8Array();
  while (offset + 8 <= source.byteLength) {
    const length = view.getUint32(offset, true), kind = view.getUint32(offset + 4, true);
    const chunk = source.subarray(offset + 8, offset + 8 + length);
    if (chunk.byteLength !== length) throw new Error("Rig GLB contains a truncated chunk");
    if (kind === JSON_CHUNK) document = JSON.parse(new TextDecoder().decode(chunk).trimEnd());
    if (kind === BIN_CHUNK) binary = chunk;
    offset += 8 + length;
  }
  if (!document) throw new Error("Rig GLB has no JSON document");
  return { document, binary };
}

export function encodeGlb(document, binary) {
  const json = padded(new TextEncoder().encode(JSON.stringify(document)), 0x20);
  const bin = padded(bytes(binary));
  const total = 12 + 8 + json.byteLength + 8 + bin.byteLength;
  const result = new Uint8Array(total), view = new DataView(result.buffer);
  view.setUint32(0, GLB_MAGIC, true); view.setUint32(4, GLB_VERSION, true); view.setUint32(8, total, true);
  view.setUint32(12, json.byteLength, true); view.setUint32(16, JSON_CHUNK, true); result.set(json, 20);
  const binHeader = 20 + json.byteLength;
  view.setUint32(binHeader, bin.byteLength, true); view.setUint32(binHeader + 4, BIN_CHUNK, true); result.set(bin, binHeader + 8);
  return result;
}

function localMatrix(node) {
  if (node.matrix) return new Float32Array(node.matrix);
  const [x, y, z, w] = node.rotation ?? [0, 0, 0, 1];
  const [sx, sy, sz] = node.scale ?? [1, 1, 1];
  const [tx, ty, tz] = node.translation ?? [0, 0, 0];
  const x2 = x + x, y2 = y + y, z2 = z + z;
  const xx = x * x2, xy = x * y2, xz = x * z2, yy = y * y2, yz = y * z2, zz = z * z2;
  const wx = w * x2, wy = w * y2, wz = w * z2;
  return new Float32Array([
    (1 - (yy + zz)) * sx, (xy + wz) * sx, (xz - wy) * sx, 0,
    (xy - wz) * sy, (1 - (xx + zz)) * sy, (yz + wx) * sy, 0,
    (xz + wy) * sz, (yz - wx) * sz, (1 - (xx + yy)) * sz, 0,
    tx, ty, tz, 1,
  ]);
}

function nodeGlobals(nodes) {
  const parents = new Array(nodes.length).fill(-1);
  nodes.forEach((node, parent) => (node.children ?? []).forEach((child) => { parents[child] = parent; }));
  const globals = new Array(nodes.length);
  const visit = (index) => {
    if (globals[index]) return globals[index];
    const local = localMatrix(nodes[index]);
    globals[index] = parents[index] < 0 ? local : mat4Multiply(visit(parents[index]), local);
    return globals[index];
  };
  nodes.forEach((_, index) => visit(index));
  return globals;
}

function transformPoint(matrix, point) {
  return [
    matrix[0] * point[0] + matrix[4] * point[1] + matrix[8] * point[2] + matrix[12],
    matrix[1] * point[0] + matrix[5] * point[1] + matrix[9] * point[2] + matrix[13],
    matrix[2] * point[0] + matrix[6] * point[1] + matrix[10] * point[2] + matrix[14],
  ];
}

function transformNormal(matrix, normal) {
  return normalize([
    matrix[0] * normal[0] + matrix[4] * normal[1] + matrix[8] * normal[2],
    matrix[1] * normal[0] + matrix[5] * normal[1] + matrix[9] * normal[2],
    matrix[2] * normal[0] + matrix[6] * normal[1] + matrix[10] * normal[2],
  ]);
}

function floatBytes(values) {
  return bytes(new Float32Array(values));
}

function ushortBytes(values) {
  return bytes(new Uint16Array(values));
}

function uintBytes(values) {
  return bytes(new Uint32Array(values));
}

function appendView(document, chunks, data, target) {
  const offset = chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0);
  const aligned = Math.ceil(offset / 4) * 4;
  if (aligned !== offset) chunks.push(new Uint8Array(aligned - offset));
  const view = { buffer: 0, byteOffset: aligned, byteLength: data.byteLength, target };
  document.bufferViews.push(view); chunks.push(data);
  return document.bufferViews.length - 1;
}

function appendAccessor(document, bufferView, componentType, count, type, bounds) {
  const accessor = { bufferView, componentType, count, type };
  if (bounds) Object.assign(accessor, bounds);
  document.accessors.push(accessor);
  return document.accessors.length - 1;
}

function mergeParts(parts) {
  const merged = { positions: [], normals: [], colors: [], indices: [], parts };
  for (const part of parts) {
    const base = merged.positions.length / 3;
    merged.positions.push(...part.positions); merged.normals.push(...part.normals); merged.colors.push(...part.colors);
    merged.indices.push(...part.indices.map((index) => base + index));
  }
  return merged;
}

const BOW_STRING_PARTS = new Set([
  "upper bowstring control span", "lower bowstring control span",
  "served nocking control span", "upper bowstring end loop", "lower bowstring end loop",
  "left crossbow string control span", "right crossbow string control span",
  "served crossbow nocking span", "left crossbow string end loop", "right crossbow string end loop",
  "wheellock wheel and axle", "wheellock cock and pyrite jaws", "wheellock pan cover",
  "wheellock safety catch", "firearm trigger and sear", "matchlock serpentine and jaws",
  "matchlock pan cover", "matchlock linkage and trigger",
]);
const isSemanticPart = (part) => BOW_STRING_PARTS.has(part.label) || (Array.isArray(part.animationPivot) && part.animationPivot.length === 3 && part.animationPivot.every(Number.isFinite));

export function automaticGripPoint(resolvedDefinition) {
  const frames = resolvedDefinition?._frames ?? {};
  if (frames["shield.grip"]) return [...frames["shield.grip"]];
  if (Number.isFinite(resolvedDefinition?.gripClearance) && frames["grip.base"] && frames["grip.top"]) {
    const base = frames["grip.base"], top = frames["grip.top"];
    const direction = base.map((value, axis) => value - top[axis]);
    const length = Math.hypot(...direction);
    return top.map((value, axis) => value + direction[axis] * (resolvedDefinition.gripClearance / length));
  }
  if (frames["grip.center"]) return [...frames["grip.center"]];
  const bottom = frames["shaft.bottom"], top = frames["shaft.top"];
  if (bottom && top) {
    const length = Math.hypot(...top.map((value, axis) => value - bottom[axis]));
    const amount = Math.min(0.45, Math.max(0.18, length * 0.2)) / (length || 1);
    return bottom.map((value, axis) => value + (top[axis] - value) * amount);
  }
  return [...(frames["weapon.root"] ?? [0, 0, 0])];
}

export function buildSkinnedWeaponGlb(baseGlb, mesh, options = {}) {
  const attachment = options.attachment ?? "r_weapon";
  const name = options.name ?? "weapon";
  const gripPoint = options.gripPoint ?? [0, 0, 0];
  const semanticStringParts = (mesh?.parts ?? []).filter(isSemanticPart);
  const skinnedMesh = semanticStringParts.length ? mergeParts(mesh.parts.filter((part) => !isSemanticPart(part))) : mesh;
  if (!skinnedMesh?.positions?.length || skinnedMesh.positions.length !== skinnedMesh.normals?.length || skinnedMesh.positions.length !== skinnedMesh.colors?.length) {
    throw new Error("Weapon mesh positions, normals, and colors must have matching non-zero lengths");
  }
  if (!skinnedMesh.indices?.length || skinnedMesh.indices.length % 3 !== 0 || skinnedMesh.indices.some((index) => !Number.isInteger(index) || index < 0 || index >= skinnedMesh.positions.length / 3)) throw new Error("Weapon mesh must contain complete triangles");

  const parsed = parseGlb(baseGlb);
  const document = JSON.parse(JSON.stringify(parsed.document));
  if (document.buffers?.length !== 1 || document.buffers[0].uri) throw new Error("Character rig must contain one embedded buffer");
  if (!document.skins?.length) throw new Error("Character rig contains no skin");
  document.nodes ??= []; document.bufferViews ??= []; document.accessors ??= [];
  const attachmentNode = document.nodes.findIndex((node) => node.name === attachment);
  if (attachmentNode < 0) throw new Error(`Character rig is missing ${attachment}`);
  const skinIndex = document.skins.findIndex((skin) => skin.joints?.includes(attachmentNode));
  if (skinIndex < 0) throw new Error(`${attachment} is not part of the character skin`);
  const jointIndex = document.skins[skinIndex].joints.indexOf(attachmentNode);
  if (jointIndex > 65_535) throw new Error("Attachment joint index does not fit glTF JOINTS_0");

  const matrix = nodeGlobals(document.nodes)[attachmentNode];
  const positions = [], normals = [];
  for (let index = 0; index < skinnedMesh.positions.length; index += 3) {
    const local = skinnedMesh.positions.slice(index, index + 3).map((value, axis) => value - gripPoint[axis]);
    positions.push(...transformPoint(matrix, local));
    normals.push(...transformNormal(matrix, skinnedMesh.normals.slice(index, index + 3)));
  }
  const vertexCount = positions.length / 3;
  const joints = Array.from({ length: vertexCount }, () => [jointIndex, 0, 0, 0]).flat();
  const weights = Array.from({ length: vertexCount }, () => [1, 0, 0, 0]).flat();
  const bounds = { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] };
  for (let index = 0; index < positions.length; index += 3) for (let axis = 0; axis < 3; axis += 1) {
    bounds.min[axis] = Math.min(bounds.min[axis], positions[index + axis]);
    bounds.max[axis] = Math.max(bounds.max[axis], positions[index + axis]);
  }

  const chunks = [new Uint8Array(parsed.binary)];
  const position = appendAccessor(document, appendView(document, chunks, floatBytes(positions), 34_962), 5_126, vertexCount, "VEC3", bounds);
  const normal = appendAccessor(document, appendView(document, chunks, floatBytes(normals), 34_962), 5_126, vertexCount, "VEC3");
  const color = appendAccessor(document, appendView(document, chunks, floatBytes(skinnedMesh.colors), 34_962), 5_126, vertexCount, "VEC3");
  const joint = appendAccessor(document, appendView(document, chunks, ushortBytes(joints), 34_962), 5_123, vertexCount, "VEC4");
  const weight = appendAccessor(document, appendView(document, chunks, floatBytes(weights), 34_962), 5_126, vertexCount, "VEC4");
  const indexComponentType = vertexCount <= 65_536 ? 5_123 : 5_125;
  const indexValues = skinnedMesh.indices;
  const indices = appendAccessor(
    document,
    appendView(document, chunks, indexComponentType === 5_123 ? ushortBytes(indexValues) : uintBytes(indexValues), 34_963),
    indexComponentType,
    indexValues.length,
    "SCALAR",
  );

  const oldMeshNodes = new Set();
  document.nodes.forEach((node, index) => { if (node.mesh !== undefined) { oldMeshNodes.add(index); delete node.mesh; delete node.skin; } });
  document.materials = [{ name: "Weapon", pbrMetallicRoughness: { baseColorFactor: [1, 1, 1, 1], metallicFactor: 0.35, roughnessFactor: 0.5 } }];
  document.meshes = [{ name, primitives: [{ attributes: { POSITION: position, NORMAL: normal, COLOR_0: color, JOINTS_0: joint, WEIGHTS_0: weight }, indices, material: 0, mode: 4 }] }];
  const weaponNode = document.nodes.length;
  document.nodes.push({ name, mesh: 0, skin: skinIndex });
  const semanticNodes = [];
  for (const part of semanticStringParts) {
    const partPositions = [], pivot = part.animationPivot ?? null;
    for (let index = 0; index < part.positions.length; index += 3) partPositions.push(...part.positions.slice(index, index + 3).map((value, axis) => value - (pivot?.[axis] ?? gripPoint[axis])));
    const partBounds = { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] };
    for (let index = 0; index < partPositions.length; index += 3) for (let axis = 0; axis < 3; axis++) {
      partBounds.min[axis] = Math.min(partBounds.min[axis], partPositions[index + axis]);
      partBounds.max[axis] = Math.max(partBounds.max[axis], partPositions[index + axis]);
    }
    const partVertexCount = partPositions.length / 3,
      partPosition = appendAccessor(document, appendView(document, chunks, floatBytes(partPositions), 34_962), 5_126, partVertexCount, "VEC3", partBounds),
      partNormal = appendAccessor(document, appendView(document, chunks, floatBytes(part.normals), 34_962), 5_126, partVertexCount, "VEC3"),
      partColor = appendAccessor(document, appendView(document, chunks, floatBytes(part.colors), 34_962), 5_126, partVertexCount, "VEC3"),
      partIndexType = partVertexCount <= 65_536 ? 5_123 : 5_125,
      partIndices = appendAccessor(document, appendView(document, chunks, partIndexType === 5_123 ? ushortBytes(part.indices) : uintBytes(part.indices), 34_963), partIndexType, part.indices.length, "SCALAR"),
      meshIndex = document.meshes.length,
      nodeIndex = document.nodes.length;
    document.meshes.push({ name: part.label, primitives: [{ attributes: { POSITION: partPosition, NORMAL: partNormal, COLOR_0: partColor }, indices: partIndices, material: 0, mode: 4 }] });
    document.nodes.push({ name: part.label, mesh: meshIndex, ...(pivot ? { translation: pivot.map((value, axis) => value - gripPoint[axis]) } : {}), extras: { adventuresim_animation_role: part.label, ...(pivot ? { adventuresim_local_pivot: [0, 0, 0], adventuresim_weapon_pivot: pivot } : {}) } });
    semanticNodes.push(nodeIndex);
  }
  if (semanticNodes.length) document.nodes[attachmentNode].children = [...(document.nodes[attachmentNode].children ?? []), ...semanticNodes];
  const sceneIndex = document.scene ?? 0;
  document.scenes[sceneIndex].nodes = (document.scenes[sceneIndex].nodes ?? []).filter((node) => !oldMeshNodes.has(node));
  document.scenes[sceneIndex].nodes.push(weaponNode);
  document.animations = [];
  document.asset = { ...document.asset, generator: "Fabelgeist weapon modeler" };
  document.extras = { ...(document.extras ?? {}), adventuresim_weapon: {
    name, attachment, grip_point: gripPoint, skinned: true,
    animation_contract: semanticNodes.length ? (semanticStringParts.some((part) => /wheellock|matchlock|firearm trigger/.test(part.label)) ? "firearm-lock-nodes-v2" : semanticStringParts.some((part) => part.label === "ball pouch hinged flap") ? "pouch-flap-node-v1" : semanticStringParts.some((part) => part.label.includes("crossbow")) ? "crossbow-string-nodes-v1" : "bow-string-nodes-v1") : undefined,
    semantic_nodes: semanticStringParts.map((part) => part.label),
  } };

  const size = chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0), binary = new Uint8Array(size);
  let offset = 0; for (const chunk of chunks) { binary.set(chunk, offset); offset += chunk.byteLength; }
  document.buffers[0].byteLength = binary.byteLength;
  return encodeGlb(document, binary);
}
