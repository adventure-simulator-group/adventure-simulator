import { PRESETS, copyPreset, controlVisible, getControlValue, setControlValue } from "./presets.js";
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
    const changes = [], rejected = [], visible = (control) => controlVisible(preset.definition, control);
    for (const control of preset.choiceControls) {
      if (!visible(control)) continue;
      if (random() > 0.5) continue;
      const option = control.options[Math.floor(random() * control.options.length)], candidate = structuredClone(preset.definition);
      setControlValue(candidate, control, structuredClone(option.value));
      const validation = validateWeapon(candidate, preset.controls);
      if (validation.valid) { changes.push({ label: control.label, to: option.label, value: option.value }); preset.definition = candidate; }
      else rejected.push({ label: control.label, value: option.label, errors: validation.errors });
    }
    for (const control of preset.controls.filter(visible)) {
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
  const cases = [
    specimen("landsknecht-longsword", "narrow-pommel-wide-grip", (parts) => { parts[0].widthScale = 0.65; Object.assign(parts[1], { width: 0.038, thickness: 0.028 }); }),
    specimen("landsknecht-longsword", "large-pommel-short-grip", (parts) => { Object.assign(parts[0], { widthScale: 1.4, lengthScale: 1.5 }); parts[1].length = 0.20; }),
    specimen("landsknecht-longsword", "rounded-bulb-swept-terminals", (parts) => {
      parts[0].profile = PRESETS.find((item) => item.id === "landsknecht-longsword").choiceControls.find((choice) => choice.label === "Lathed pommel profile").options[1].value;
      Object.assign(parts[2], { sweep: -0.04, mirrorMode: "symmetric", terminalSwell: 1, tipScale: 0.6 });
    }),
    specimen("halberd-1540", "mirrored-cusped-head", (parts) => { Object.assign(parts[1], { side: -1, upperCusp: 0.16, lowerCusp: 0.12, curvature: 0, thickness: 0.006 }); }),
    specimen("buckler", "thin-bowl-large-boss", (parts) => { Object.assign(parts[0], { thickness: 0.002, bossRadius: 0.14, bossHeight: 0.07, gripLength: 0.12, radialSegments: 12, rings: 3 }); }),
    specimen("pavise", "deep-rib-low-authored-resolution", (parts) => { Object.assign(parts[0], { centerCurve: 0.06, centerWidth: 0.12, edgeSegments: 6 }); }),
  ];
  const longsword = PRESETS.find((item) => item.id === "landsknecht-longsword");
  for (const construction of ["plate", "faceted", "writhen", "outline"]) cases.push(specimen("landsknecht-longsword", `${construction}-pommel-extreme`, (parts) => { Object.assign(parts[0], { construction, widthScale: construction === "outline" ? 1.35 : 0.8, lengthScale: 1.25, fluteDepth: 0.2, twist: 140, facets: 6, notchDepth: 0.4 }); }));
  const ornamentControl = longsword.choiceControls.find((control) => control.label === "Pommel ornament");
  for (const option of ornamentControl.options.slice(1)) cases.push(specimen("landsknecht-longsword", `composite-${option.label.toLowerCase().replaceAll(" ", "-")}`, (parts) => { Object.assign(parts[0], { construction: "composite", baseConstruction: "faceted", ornaments: structuredClone(option.value) }); }));
  for (const [section, terminal] of [["oval", "ball"], ["diamond", "pyramidal"], ["flat", "scroll"], ["triangular", "fishtail"], ["round", "vase"], ["diamond", "disk"]]) cases.push(specimen("landsknecht-longsword", `${section}-${terminal}-independent-quillons`, (parts) => { Object.assign(parts[2], { section, terminal, mirrorMode: "independent", leftLength: 0.09, rightLength: 0.21, leftSweep: -0.06, rightSweep: 0.04, leftSet: -0.025, rightSet: 0.03, sectionTwist: section === "round" ? 0 : 180, tipScale: 0.5 }); }));
  const shell = PRESETS.find((item) => item.id === "reitschwert-1540").choiceControls.find((control) => control.label === "Compound-hilt shell study").options[1].value;
  cases.push(specimen("reitschwert-1540", "later-pierced-shell-study", (parts) => { parts.find((part) => part.kind === "guardAssembly").plates = structuredClone(shell); }));
  cases.push(specimen("landsknecht-longsword", "flat-section-half-turn-proof", (parts) => { Object.assign(parts[2], { section: "flat", sectionWidth: 0.018, sectionDepth: 0.005, sectionTwist: 180, mirrorMode: "opposed", sweep: 0.01, terminal: "none", tipScale: 1, terminalSwell: 0 }); }));
  const ridingSword = PRESETS.find((item) => item.id === "reitschwert-1540"), gripControl = ridingSword.controls.find((control) => control.label === "Grip length");
  for (const endpoint of ["min", "max"]) cases.push(specimen("reitschwert-1540", `${endpoint}-grip-connected-bow`, (parts) => { parts.find((part) => part.id === "grip").length = gripControl[endpoint]; }));
  cases.push(specimen("reitschwert-1540", "curved-double-shell-study", (parts) => {
    const graph = parts.find((part) => part.kind === "guardAssembly"), outer = [[-0.055, 0], [-0.065, 0.045], [-0.045, 0.078], [0, 0.062], [0.045, 0.078], [0.065, 0.045], [0.055, 0], [0, 0.012]], inner = [[-0.024, 0.02], [-0.03, 0.04], [-0.02, 0.05], [0, 0.044], [0.02, 0.05], [0.03, 0.04], [0.024, 0.02], [0, 0.025]];
    for (let i = 0; i < 8; i++) { graph.nodes[`shell-${i}`] = [...outer[i], 0.008]; graph.nodes[`aperture-${i}`] = [...inner[i], 0.008]; }
    graph.plates = [{ outline: outer.map((_, i) => `shell-${i}`), cutout: inner.map((_, i) => `aperture-${i}`), thickness: 0.003, dishDepth: 0.009, rimRadius: 0.002 }];
  }));
  return cases;
}
