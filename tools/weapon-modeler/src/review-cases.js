import { PRESETS, copyPreset, getControlValue, setControlValue } from "./presets.js";
import { validateWeapon } from "./mesh.js";

// Store the seed and exact definitions with captures: future changes to control
// ranges must not silently change an already reviewed specimen.
export function reviewCases(seed = 1544, ids = PRESETS.map((preset) => preset.id)) {
  let state = seed >>> 0;
  const random = () => ((state = (Math.imul(state, 1664525) + 1013904223) >>> 0) / 4294967296);
  return ids.flatMap((id) => {
    state = [...id].reduce((hash, character) => Math.imul(hash ^ character.charCodeAt(0), 16777619) >>> 0, seed >>> 0);
    const preset = copyPreset(PRESETS.find((item) => item.id === id));
    const cases = [{ id: `${id}-default`, name: preset.name, variant: "Default", definition: structuredClone(preset.definition), changes: [], rejected: [] }];
    const changes = [], rejected = [];
    for (const control of preset.controls) {
      if (random() > 0.45) continue;
      const steps = Math.round((control.max - control.min) / control.step);
      const value = Number((control.min + Math.floor(random() * (steps + 1)) * control.step).toFixed(10));
      const candidate = structuredClone(preset.definition);
      setControlValue(candidate, control, value);
      const validation = validateWeapon(candidate, preset.controls);
      if (validation.valid) {
        changes.push({ label: control.label, from: getControlValue(preset.definition, control), to: value });
        preset.definition = candidate;
      } else rejected.push({ label: control.label, value, errors: validation.errors });
    }
    for (const control of preset.choiceControls) {
      if (random() > 0.5) continue;
      const option = control.options[Math.floor(random() * control.options.length)], candidate = structuredClone(preset.definition);
      setControlValue(candidate, control, structuredClone(option.value));
      const validation = validateWeapon(candidate, preset.controls);
      if (validation.valid) {
        changes.push({ label: control.label, to: option.label, value: option.value }); preset.definition = candidate;
      } else rejected.push({ label: control.label, value: option.label, errors: validation.errors });
    }
    cases.push({ id: `${id}-seed-${seed}`, name: preset.name, variant: `Seed ${seed}`, definition: preset.definition, changes, rejected });
    return cases;
  });
}

export function adversarialReviewCases() {
  const specimen = (id, suffix, edit) => {
    const preset = copyPreset(PRESETS.find((item) => item.id === id));
    edit(preset.definition.components);
    return { id: `${id}-${suffix}`, name: preset.name, variant: suffix, definition: preset.definition, changes: [], rejected: [] };
  };
  return [
    specimen("landsknecht-longsword", "narrow-pommel-wide-grip", (parts) => { parts[0].widthScale = 0.65; Object.assign(parts[1], { width: 0.038, thickness: 0.028 }); }),
    specimen("landsknecht-longsword", "large-pommel-short-grip", (parts) => { Object.assign(parts[0], { widthScale: 1.4, lengthScale: 1.5 }); parts[1].length = 0.20; }),
    specimen("landsknecht-longsword", "rounded-bulb-swept-terminals", (parts) => {
      parts[0].profile = PRESETS.find((item) => item.id === "landsknecht-longsword").choiceControls.find((choice) => choice.label === "Pommel form").options[1].value;
      Object.assign(parts[2], { sweep: -0.04, symmetricSweep: 1, terminalSwell: 1, tipScale: 0.6 });
    }),
    specimen("halberd-1540", "mirrored-cusped-head", (parts) => { Object.assign(parts[1], { side: -1, upperCusp: 0.16, lowerCusp: 0.12, curvature: 0, thickness: 0.006 }); }),
    specimen("buckler", "thin-bowl-large-boss", (parts) => { Object.assign(parts[0], { thickness: 0.002, bossRadius: 0.14, bossHeight: 0.07, gripLength: 0.12, radialSegments: 12, rings: 3 }); }),
    specimen("pavise", "deep-rib-low-authored-resolution", (parts) => { Object.assign(parts[0], { centerCurve: 0.06, centerWidth: 0.12, edgeSegments: 6 }); }),
  ];
}
