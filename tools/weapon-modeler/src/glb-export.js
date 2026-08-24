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

export function automaticGripPoint(resolvedDefinition) {
  const frames = resolvedDefinition?._frames ?? {};
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
  if (!mesh?.positions?.length || mesh.positions.length !== mesh.normals?.length || mesh.positions.length !== mesh.colors?.length) {
    throw new Error("Weapon mesh positions, normals, and colors must have matching non-zero lengths");
  }
  if (mesh.positions.length % 9 !== 0) throw new Error("Weapon mesh must contain complete triangles");

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
  for (let index = 0; index < mesh.positions.length; index += 3) {
    const local = mesh.positions.slice(index, index + 3).map((value, axis) => value - gripPoint[axis]);
    positions.push(...transformPoint(matrix, local));
    normals.push(...transformNormal(matrix, mesh.normals.slice(index, index + 3)));
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
  const color = appendAccessor(document, appendView(document, chunks, floatBytes(mesh.colors), 34_962), 5_126, vertexCount, "VEC3");
  const joint = appendAccessor(document, appendView(document, chunks, ushortBytes(joints), 34_962), 5_123, vertexCount, "VEC4");
  const weight = appendAccessor(document, appendView(document, chunks, floatBytes(weights), 34_962), 5_126, vertexCount, "VEC4");
  const indexComponentType = vertexCount <= 65_536 ? 5_123 : 5_125;
  const indexValues = Array.from({ length: vertexCount }, (_, index) => index);
  const indices = appendAccessor(
    document,
    appendView(document, chunks, indexComponentType === 5_123 ? ushortBytes(indexValues) : uintBytes(indexValues), 34_963),
    indexComponentType,
    vertexCount,
    "SCALAR",
  );

  const oldMeshNodes = new Set();
  document.nodes.forEach((node, index) => { if (node.mesh !== undefined) { oldMeshNodes.add(index); delete node.mesh; delete node.skin; } });
  document.materials = [{ name: "Weapon", pbrMetallicRoughness: { baseColorFactor: [1, 1, 1, 1], metallicFactor: 0.35, roughnessFactor: 0.5 } }];
  document.meshes = [{ name, primitives: [{ attributes: { POSITION: position, NORMAL: normal, COLOR_0: color, JOINTS_0: joint, WEIGHTS_0: weight }, indices, material: 0, mode: 4 }] }];
  const weaponNode = document.nodes.length;
  document.nodes.push({ name, mesh: 0, skin: skinIndex });
  const sceneIndex = document.scene ?? 0;
  document.scenes[sceneIndex].nodes = (document.scenes[sceneIndex].nodes ?? []).filter((node) => !oldMeshNodes.has(node));
  document.scenes[sceneIndex].nodes.push(weaponNode);
  document.animations = [];
  document.asset = { ...document.asset, generator: "Fabelgeist weapon modeler" };
  document.extras = { ...(document.extras ?? {}), adventuresim_weapon: { name, attachment, grip_point: gripPoint, skinned: true } };

  const size = chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0), binary = new Uint8Array(size);
  let offset = 0; for (const chunk of chunks) { binary.set(chunk, offset); offset += chunk.byteLength; }
  document.buffers[0].byteLength = binary.byteLength;
  return encodeGlb(document, binary);
}
