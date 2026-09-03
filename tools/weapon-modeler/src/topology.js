import { cross, dot, normalize, subtract } from "./math.js";

// Welding is local to a construction surface. A blade bevel, end cap, or
// octagonal flat deliberately has a separate surface from its neighbor.
export function indexTriangles(source, groups) {
  const positions = [], normals = [], colors = [], indices = [], sums = [], faces = [], lookup = new Map();
  for (let triangle = 0; triangle < source.positions.length / 9; triangle++) {
    const points = [0, 1, 2].map((corner) => source.positions.slice(triangle * 9 + corner * 3, triangle * 9 + corner * 3 + 3));
    const face = normalize(cross(subtract(points[1], points[0]), subtract(points[2], points[0])));
    const surface = groups[triangle] || `flat:${face.map((value) => Math.round(value * 1e8)).join(",")}`;
    for (let corner = 0; corner < 3; corner++) {
      const point = points[corner], key = `${surface}:${point.map((value) => Math.round(value * 1e9)).join(",")}`;
      const candidates = lookup.get(key) ?? [];
      // Even a nominally smooth sweep can contain an authored sharp corner.
      // Split that crease rather than average normals across opposing faces.
      let index = candidates.find((candidate) => faces[candidate].every((neighbor) => dot(neighbor, face) > 0.25));
      if (index === undefined) {
        index = positions.length / 3; candidates.push(index); lookup.set(key, candidates); faces.push([]);
        positions.push(...point); colors.push(...source.colors.slice(triangle * 9 + corner * 3, triangle * 9 + corner * 3 + 3)); sums.push([0, 0, 0]);
      }
      faces[index].push(face);
      const a = normalize(subtract(points[(corner + 1) % 3], point)), b = normalize(subtract(points[(corner + 2) % 3], point));
      const angle = Math.acos(Math.max(-1, Math.min(1, dot(a, b))));
      for (let axis = 0; axis < 3; axis++) sums[index][axis] += face[axis] * angle;
      indices.push(index);
    }
  }
  for (const sum of sums) normals.push(...normalize(sum));
  return { positions, normals, colors, indices };
}

export function* triangleVertices(mesh) {
  for (let offset = 0; offset < mesh.indices.length; offset += 3) {
    yield mesh.indices.slice(offset, offset + 3).map((index) => mesh.positions.slice(index * 3, index * 3 + 3));
  }
}
