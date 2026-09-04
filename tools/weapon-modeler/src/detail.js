export const DETAIL_LEVELS = Object.freeze({
  low: Object.freeze({ samples: 0.5, error: 2.5, radialFloor: 8 }),
  medium: Object.freeze({ samples: 1, error: 1, radialFloor: 16 }),
  high: Object.freeze({ samples: 2, error: 0.4, radialFloor: 24 }),
});

// Mesh construction is synchronous. Restore the scoped sampling budget even
// when a malformed definition throws; nested builds cannot leak their LOD.
let active = DETAIL_LEVELS.medium;
export function withDetail(lod, generate) {
  if (!Object.hasOwn(DETAIL_LEVELS, lod)) throw new Error(`Unknown detail level: ${lod}`);
  const previous = active;
  active = DETAIL_LEVELS[lod];
  try { return generate(); } finally { active = previous; }
}
export function detailSamples(requested, minimum = 4) { return Math.max(minimum, Math.ceil(requested * active.samples)); }
export function detailError(value) { return value * active.error; }
export function roundSegments(radius, requested = 16) {
  const sagitta = Math.min(radius, detailError(0.0003));
  return Math.max(active.radialFloor, detailSamples(requested), Math.ceil(Math.PI / Math.acos(1 - sagitta / radius)));
}
