export const MAX_ROUND_GRIP_RADIUS_M = 0.022;
export const MAX_SWORD_GRIP_WIDTH_M = 0.038;
export const MAX_SWORD_GRIP_THICKNESS_M = 0.028;

export function effectiveGripRadius(component) {
  return component.radius * Math.max(component.bottomScale ?? 1, component.topScale ?? 1);
}

export function maximumAuthoredGripRadius(component) {
  const scale = Math.max(component.bottomScale ?? 1, component.topScale ?? 1);
  return Math.floor(MAX_ROUND_GRIP_RADIUS_M / scale * 1000 + 1e-9) / 1000;
}
