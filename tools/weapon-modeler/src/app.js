import { validateWeapon } from "./mesh.js";
import { HAFT_MODULES, HEAD_ASSEMBLIES, PRESETS, composeWeapon, compositionControls, copyPreset, getControlValue, setControlValue } from "./presets.js";
import { WeaponRenderer } from "./renderer.js";

const elements = Object.fromEntries(["preset", "reset", "family", "name", "description", "controls", "stats", "status", "definition", "apply-definition", "definition-error", "dirty-state", "viewport"].map((id) => [id, document.getElementById(id)]));
const renderer = new WeaponRenderer(elements.viewport);
let active;
const composer = { haft: document.getElementById("composer-haft"), head: document.getElementById("composer-head"), build: document.getElementById("compose-weapon"), status: document.getElementById("composer-status") };

document.querySelectorAll("[data-pose]").forEach((button) => button.addEventListener("click", () => renderer.setView(button.dataset.pose)));
document.querySelectorAll("[data-focus]").forEach((button) => button.addEventListener("click", () => renderer.setView("front", button.dataset.focus)));

for (const preset of PRESETS) {
  const option = document.createElement("option"); option.value = preset.id; option.textContent = preset.name; elements.preset.append(option);
}
for (const module of HAFT_MODULES) { const option = document.createElement("option"); option.value = module.id; option.textContent = module.name; composer.haft.append(option); }
for (const assembly of HEAD_ASSEMBLIES) { const option = document.createElement("option"); option.value = assembly.id; option.textContent = assembly.name; composer.head.append(option); }

function updateDefinitionText() { elements.definition.value = JSON.stringify(active.definition, null, 2); }

function rebuild(dirty = false) {
  try {
    const validation = validateWeapon(active.definition, active.controls);
    if (!validation.valid) throw new Error(validation.errors.join(" · "));
    const mesh = validation.mesh;
    renderer.setMesh(mesh);
    const [width, length, depth] = mesh.stats.dimensions;
    elements.stats.innerHTML = `
      <dt>Overall length</dt><dd>${length.toFixed(3)} m</dd>
      <dt>Maximum breadth</dt><dd>${width.toFixed(3)} m</dd>
      <dt>Maximum depth</dt><dd>${depth.toFixed(3)} m</dd>
      <dt>Parts</dt><dd>${mesh.stats.partCount}</dd>
      <dt>Triangles</dt><dd>${mesh.stats.triangles.toLocaleString()}</dd>
      <dt>Rough mesh-volume diagnostic</dt><dd>${(mesh.stats.volume * 1e6).toFixed(0)} cm³</dd>`;
    elements.status.textContent = `${mesh.stats.triangles.toLocaleString()} triangles · ${mesh.stats.partCount} parts`;
    elements["dirty-state"].textContent = dirty ? "Modified" : "Preset values";
    elements["definition-error"].textContent = "";
    updateDefinitionText();
  } catch (error) {
    elements["definition-error"].textContent = error.message;
  }
}

function renderControls() {
  elements.controls.replaceChildren();
  for (const control of active.controls) {
    const row = document.createElement("div"); row.className = "control-row";
    const label = document.createElement("label"); label.textContent = control.label;
    const input = document.createElement("input"); input.type = "range"; input.min = control.min; input.max = control.max; input.step = control.step; input.value = getControlValue(active.definition, control);
    const output = document.createElement("output");
    const show = () => { output.value = `${Number(input.value).toFixed(Math.max(0, String(control.step).split(".")[1]?.length ?? 0))}${control.unit ? ` ${control.unit}` : ""}`; };
    input.addEventListener("input", () => {
      const candidate = JSON.parse(JSON.stringify(active.definition)); setControlValue(candidate, control, Number(input.value));
      const validation = validateWeapon(candidate, active.controls);
      if (!validation.valid) { input.value = getControlValue(active.definition, control); show(); elements["definition-error"].textContent = `Rejected ${control.label}: ${validation.errors.join(" · ")}`; return; }
      active.definition = candidate; show(); rebuild(true);
    });
    show(); row.append(label, input, output); elements.controls.append(row);
  }
}

function select(id) {
  active = copyPreset(PRESETS.find((preset) => preset.id === id) ?? PRESETS[0]);
  elements.preset.value = active.id; elements.family.textContent = active.family; elements.name.textContent = active.name; elements.description.textContent = active.description;
  renderControls(); rebuild(false);
}

elements.preset.addEventListener("change", () => select(elements.preset.value));
elements.reset.addEventListener("click", () => select(active.id));
elements["apply-definition"].addEventListener("click", () => {
  try { const candidate = JSON.parse(elements.definition.value); const validation = validateWeapon(candidate, active.controls); if (!validation.valid) throw new Error(validation.errors.join(" · ")); active.definition = candidate; renderControls(); rebuild(true); }
  catch (error) { elements["definition-error"].textContent = error.message; }
});

composer.build.addEventListener("click", () => {
  const definition = composeWeapon(composer.haft.value, composer.head.value), controls = compositionControls(definition), validation = validateWeapon(definition, controls);
  if (!validation.valid) { composer.status.textContent = `Composition rejected: ${validation.errors.join(" · ")}`; return; }
  active = { id: "composed", name: `${composer.haft.selectedOptions[0].textContent} + ${composer.head.selectedOptions[0].textContent}`, family: "Composed preview", description: "A validated modular assembly built from independent haft and head modules.", definition, controls };
  elements.family.textContent = active.family; elements.name.textContent = active.name; elements.description.textContent = active.description; renderControls(); rebuild(true); composer.status.textContent = "Composition valid: attachments, winding, manifold topology, and camera fit passed.";
});

select(PRESETS[0].id);
