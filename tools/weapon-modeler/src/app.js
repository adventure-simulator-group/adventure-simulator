import { measureMassProperties, validateWeapon } from "./mesh.js";
import { automaticGripPoint, buildSkinnedWeaponGlb } from "./glb-export.js";
import { HAFT_MODULES, HEAD_ASSEMBLIES, PRESETS, composeWeapon, compositionControls, copyPreset, controlVisible, getControlValue, setControlValue } from "./presets.js";
import { WeaponRenderer } from "./renderer.js";

const elements = Object.fromEntries(["preset", "reset", "family", "name", "description", "controls", "stats", "status", "definition", "apply-definition", "definition-error", "dirty-state", "viewport"].map((id) => [id, document.getElementById(id)]));
const renderer = new WeaponRenderer(elements.viewport);
const lod = document.getElementById("lod");
lod.addEventListener("change", () => rebuild(true));
let active;
let currentValidation;
const composer = { haft: document.getElementById("composer-haft"), head: document.getElementById("composer-head"), build: document.getElementById("compose-weapon"), status: document.getElementById("composer-status") };
const exporter = { name: document.getElementById("export-name"), joint: document.getElementById("export-joint"), button: document.getElementById("export-glb"), status: document.getElementById("export-status") };

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
    const validation = validateWeapon(active.definition, active.controls, { lod: lod.value });
    if (!validation.valid) throw new Error(validation.errors.join(" · "));
    currentValidation = validation;
    const mesh = validation.mesh;
    const shield = validation.resolved.components.find((component) => ["roundShield", "shapedShield"].includes(component.kind));
    if (shield) exporter.joint.value = shield.mirrored ? "l_weapon" : "r_weapon";
    renderer.setMesh(mesh);
    const physical = measureMassProperties(mesh, automaticGripPoint(validation.resolved));
    mesh.stats.physical = physical;
    const pommelMass = physical.components.filter((component) => component.id === "pommel").reduce((sum, component) => sum + component.massKg, 0);
    const balancePoint = physical.centerOfMassFromGripM * 1_000;
    const [width, length, depth] = mesh.stats.dimensions;
    elements.stats.innerHTML = `
      <dt>Overall length</dt><dd>${length.toFixed(3)} m</dd>
      <dt>Maximum breadth</dt><dd>${width.toFixed(3)} m</dd>
      <dt>Maximum depth</dt><dd>${depth.toFixed(3)} m</dd>
      <dt>Calculated mass</dt><dd>${physical.massKg.toFixed(2)} kg</dd>
      <dt>Pommel mass</dt><dd>${pommelMass > 0 ? `${(pommelMass * 1_000).toFixed(0)} g (${((pommelMass / physical.massKg) * 100).toFixed(0)}%)` : "—"}</dd>
      <dt>Center of mass</dt><dd>${Math.abs(balancePoint).toFixed(0)} mm ${balancePoint >= 0 ? "forward of" : "behind"} grip center</dd>
      <dt>Moment about grip</dt><dd>${physical.momentOfInertiaKgM2.toFixed(3)} kg·m²</dd>
      <dt>Handling coefficient</dt><dd>${physical.balance.toFixed(3)} (lower redirects more easily)</dd>
      <dt>Parts</dt><dd>${mesh.stats.partCount}</dd>
      <dt>Triangles</dt><dd>${mesh.stats.triangles.toLocaleString()}</dd>
      <dt>Rough mesh-volume diagnostic</dt><dd>${(mesh.stats.volume * 1e6).toFixed(0)} cm³</dd>`;
    elements.status.textContent = `${physical.massKg.toFixed(2)} kg · balance ${physical.balance.toFixed(3)} · ${mesh.stats.triangles.toLocaleString()} triangles`;
    elements["dirty-state"].textContent = dirty ? "Modified" : "Preset values";
    elements["definition-error"].textContent = "";
    updateDefinitionText();
  } catch (error) {
    elements["definition-error"].textContent = error.message;
  }
}

function renderControls() {
  elements.controls.replaceChildren();
  const visible = (control) => controlVisible(active.definition, control);
  for (const control of (active.choiceControls ?? []).filter(visible)) {
    const row = document.createElement("div"); row.className = "control-row";
    const label = document.createElement("label"); label.textContent = control.label;
    const input = document.createElement("select");
    input.id = `control-${elements.controls.children.length}`; label.htmlFor = input.id;
    for (const option of control.options) {
      const element = document.createElement("option"); element.textContent = option.label; element.value = JSON.stringify(option.value); input.append(element);
    }
    input.value = JSON.stringify(getControlValue(active.definition, control));
    input.addEventListener("change", () => {
      const candidate = JSON.parse(JSON.stringify(active.definition)); setControlValue(candidate, control, JSON.parse(input.value));
      const validation = validateWeapon(candidate, active.controls, { lod: lod.value });
      if (!validation.valid) { input.value = JSON.stringify(getControlValue(active.definition, control)); elements["definition-error"].textContent = `Rejected ${control.label}: ${validation.errors.join(" · ")}`; return; }
      active.definition = candidate; renderControls(); rebuild(true);
    });
    row.append(label, input); elements.controls.append(row);
  }
  for (const control of active.controls.filter(visible)) {
    const row = document.createElement("div"); row.className = "control-row";
    const label = document.createElement("label"); label.textContent = control.label;
    const input = document.createElement("input"); input.id = `control-${elements.controls.children.length}`; label.htmlFor = input.id; input.type = "range"; input.min = control.min; input.max = control.max; input.step = control.step; input.value = getControlValue(active.definition, control);
    const output = document.createElement("output");
    const show = () => { output.value = `${Number(input.value).toFixed(Math.max(0, String(control.step).split(".")[1]?.length ?? 0))}${control.unit ? ` ${control.unit}` : ""}`; };
    input.addEventListener("input", () => {
      const candidate = JSON.parse(JSON.stringify(active.definition)); setControlValue(candidate, control, Number(input.value));
      const validation = validateWeapon(candidate, active.controls, { lod: lod.value });
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

function exportFileName() {
  const stem = exporter.name.value.trim().toLowerCase().replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "");
  if (!stem) throw new Error("Enter a file name");
  exporter.name.value = stem;
  return `${stem}.glb`;
}

exporter.button.addEventListener("click", async () => {
  exporter.button.disabled = true;
  try {
    if (!currentValidation?.valid) throw new Error("Weapon must be valid before export");
    exporter.status.textContent = "Loading character skeleton…";
    const response = await fetch("/api/rig");
    if (!response.ok) throw new Error((await response.json()).error ?? `Could not load rig (${response.status})`);
    const rigPath = response.headers.get("X-Fabelgeist-Rig-Path") ?? "character creator rig";
    const fileName = exportFileName();
    const glb = buildSkinnedWeaponGlb(await response.arrayBuffer(), currentValidation.mesh, {
      name: exporter.name.value,
      attachment: exporter.joint.value,
      gripPoint: automaticGripPoint(currentValidation.resolved),
    });
    exporter.status.textContent = "Writing skinned GLB…";
    const saved = await fetch(`/api/export?name=${encodeURIComponent(fileName)}`, { method: "POST", headers: { "Content-Type": "model/gltf-binary" }, body: glb });
    const result = await saved.json();
    if (!saved.ok) throw new Error(result.error ?? `Could not save export (${saved.status})`);
    exporter.status.textContent = `${result.path} · ${exporter.joint.value} · ${rigPath}`;
  } catch (error) {
    exporter.status.textContent = `Export failed: ${error.message}`;
  } finally {
    exporter.button.disabled = false;
  }
});

elements.preset.addEventListener("change", () => select(elements.preset.value));
elements.reset.addEventListener("click", () => select(active.id));
elements["apply-definition"].addEventListener("click", () => {
  try { const candidate = JSON.parse(elements.definition.value); const validation = validateWeapon(candidate, active.controls, { lod: lod.value }); if (!validation.valid) throw new Error(validation.errors.join(" · ")); active.definition = candidate; renderControls(); rebuild(true); }
  catch (error) { elements["definition-error"].textContent = error.message; }
});

composer.build.addEventListener("click", () => {
  const definition = composeWeapon(composer.haft.value, composer.head.value), controls = compositionControls(definition), validation = validateWeapon(definition, controls, { lod: lod.value });
  if (!validation.valid) { composer.status.textContent = `Composition rejected: ${validation.errors.join(" · ")}`; return; }
  active = { id: "composed", name: `${composer.haft.selectedOptions[0].textContent} + ${composer.head.selectedOptions[0].textContent}`, family: "Composed preview", description: "A validated modular assembly built from independent haft and head modules.", definition, controls };
  elements.family.textContent = active.family; elements.name.textContent = active.name; elements.description.textContent = active.description; renderControls(); rebuild(true); composer.status.textContent = "Composition valid: attachments, winding, manifold topology, and camera fit passed.";
});

select(PRESETS[0].id);
