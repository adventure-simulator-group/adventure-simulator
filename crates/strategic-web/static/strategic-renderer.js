const surface = document.querySelector("#strategic-render-surface");
const canvas = surface?.querySelector("#game-canvas");
let runtimePromise;
let host;
const forgeDesigns = new WeakMap();
const forgeConstraints = new WeakMap();
let currentForgeDesign;
let orbitingForge = false;

const command = (payload) => runtimePromise
  ?.then((runtime) => runtime.wasm_command(JSON.stringify(payload)))
  .catch((error) => console.error("strategic renderer command failed", error));

const hide = () => {
  host?.removeAttribute("data-renderer-ready");
  host = undefined;
  currentForgeDesign = undefined;
  orbitingForge = false;
  command({ type: "hide-strategic-scene" });
};

const mount = () => {
  if (document.body.hasAttribute("data-tactical-active")) return;
  const nextHost = document.querySelector('[data-bevy-scene="forge"]');
  if (!nextHost || !surface) {
    hide();
    return;
  }
  host = nextHost;
  runtimePromise?.then(() => {
    if (host !== nextHost) return;
    nextHost.setAttribute("data-renderer-ready", "");
    initializeForge(document.querySelector("[data-forge-customization]"));
  });
};

const enumOptions = {
  material: ["Wood", "Leather", "DarkLeather", "Brass", "Steel", "DarkSteel"],
  role: ["Structure", "Grip", "Guard", "Socket", "Head"],
  section: ["Flat", "Diamond", "Fullered"],
  profile: ["Straight", "Spear", "Cleaver", "Curved"],
};

const titleize = (value) => value.replace(/_/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
const setPath = (object, path, value) => {
  const parent = path.slice(0, -1).reduce((current, key) => current[key], object);
  parent[path.at(-1)] = value;
};

const appendField = (container, key, value, path, constraints) => {
  const label = document.createElement("label");
  const name = document.createElement("span");
  name.textContent = titleize(key);
  label.append(name);
  let input;
  if (typeof value === "number") {
    const constraint = constraints.get(path.join("."));
    if (!constraint) return;
    input = document.createElement("input");
    input.type = "range";
    [input.min, input.max, input.step] = [constraint.min, constraint.max, constraint.step];
    input.value = value;
    const output = document.createElement("output");
    output.textContent = value;
    label.append(output);
  } else if (typeof value === "string" && enumOptions[key]) {
    input = document.createElement("select");
    for (const optionValue of enumOptions[key]) {
      input.add(new Option(optionValue, optionValue, false, optionValue === value));
    }
  } else return;
  input.dataset.forgePath = JSON.stringify(path);
  label.append(input);
  container.append(label);
};

const appendObjectFields = (container, object, path, constraints) => {
  for (const [key, value] of Object.entries(object)) {
    if (key === "id" || key === "catalog_id" || key === "role" || (key === "component" && path.includes("attachment"))) continue;
    if (value && typeof value === "object" && !Array.isArray(value)) {
      appendObjectFields(container, value, [...path, key], constraints);
    } else {
      appendField(container, key, value, [...path, key], constraints);
    }
  }
};

const renderForgeEditor = (root, design) => {
  const editor = root.querySelector("[data-forge-editor]");
  const constraints = forgeConstraints.get(root) || new Map();
  editor.replaceChildren();
  design.components.forEach((component, index) => {
    const group = document.createElement("details");
    group.open = index < 2;
    const summary = document.createElement("summary");
    summary.textContent = titleize(component.id);
    group.append(summary);
    appendObjectFields(group, component, ["components", index], constraints);
    editor.append(group);
  });
};

const updateForge = async (root) => {
  const runtime = await runtimePromise;
  const design = forgeDesigns.get(root);
  const json = JSON.stringify(design);
  try {
    const recipe = runtime.wasm_encode_weapon_design(json);
    const quote = JSON.parse(runtime.wasm_quote_weapon_design(json));
    root.querySelector("[data-forge-recipe]").value = Array.from(recipe).join(",");
    const materials = root.querySelector("[data-forge-materials]");
    materials.replaceChildren(...Object.entries(quote.materials).map(([id, kg]) => {
      const row = document.createElement("div");
      const label = document.createElement("dt");
      const amount = document.createElement("dd");
      label.textContent = titleize(id.replace("_stock", ""));
      amount.textContent = `${kg.toFixed(3)} kg`;
      row.append(label, amount);
      return row;
    }));
    root.querySelector("[data-forge-eta]").textContent = `${Math.floor(quote.minutes / 60)} h ${quote.minutes % 60} min`;
    root.querySelector("[data-forge-submit]").disabled = false;
    command({
      type: "show-strategic-scene",
      scene: { type: "forge", catalog_id: design.catalog_id, design_json: json },
    });
  } catch (error) {
    root.querySelector("[data-forge-submit]").disabled = true;
    root.querySelector("[data-forge-eta]").textContent = "Invalid recipe";
  }
};

const loadForgeChassis = async (root, catalogId) => {
  const runtime = await runtimePromise;
  const design = JSON.parse(runtime.wasm_default_weapon_design(catalogId));
  const constraints = JSON.parse(runtime.wasm_weapon_editor_fields(JSON.stringify(design)));
  currentForgeDesign = design;
  forgeDesigns.set(root, design);
  forgeConstraints.set(root, new Map(constraints.map((constraint) => [constraint.path, constraint])));
  renderForgeEditor(root, design);
  await updateForge(root);
};

const initializeForge = async (root) => {
  if (!root || forgeDesigns.has(root)) return;
  const runtime = await runtimePromise;
  const catalog = root.querySelector("[data-forge-catalog]");
  for (const id of JSON.parse(runtime.wasm_weapon_catalog())) {
    catalog.add(new Option(titleize(id), id, false, id === "longsword"));
  }
  if (currentForgeDesign) {
    catalog.value = currentForgeDesign.catalog_id;
    forgeDesigns.set(root, currentForgeDesign);
    const constraints = JSON.parse(runtime.wasm_weapon_editor_fields(JSON.stringify(currentForgeDesign)));
    forgeConstraints.set(root, new Map(constraints.map((constraint) => [constraint.path, constraint])));
    renderForgeEditor(root, currentForgeDesign);
    await updateForge(root);
  } else {
    await loadForgeChassis(root, catalog.value || "longsword");
  }
};

const forgeHostContains = (event) => {
  if (!host) return false;
  const bounds = host.getBoundingClientRect();
  return event.clientX >= bounds.left && event.clientX <= bounds.right
    && event.clientY >= bounds.top && event.clientY <= bounds.bottom;
};

if (surface && canvas) {
  runtimePromise = import("/tactical/wasm/adventuresim-tactical-client.js")
    .then(async (runtime) => {
      await runtime.default();
      const [graphics, audio] = await Promise.all([
        fetch("/tactical/assets/config/tactical-graphics.yaml"),
        fetch("/tactical/assets/config/tactical-audio.yaml"),
      ]);
      if (!graphics.ok) throw new Error(`tactical graphics config: HTTP ${graphics.status}`);
      if (!audio.ok) throw new Error(`tactical audio config: HTTP ${audio.status}`);
      runtime.wasm_boot(await graphics.text(), await audio.text());
      return runtime;
    })
    .catch((error) => {
      console.warn("persistent Bevy renderer unavailable; retaining HTML fallback", error);
      throw error;
    });
  runtimePromise.catch(() => hide());
  mount();
  document.addEventListener("strategic-page-mounted", mount);
  document.addEventListener("strategic-page-unmounting", hide);
  document.addEventListener("strategic-live-regions-refreshed", (event) => {
    if (!event.detail?.regions?.includes("left-sidebar")) return;
    initializeForge(document.querySelector("[data-forge-customization]"));
  });
  document.addEventListener("input", (event) => {
    const root = event.target.closest?.("[data-forge-customization]");
    if (!root || !event.target.matches("[data-forge-path]")) return;
    const design = forgeDesigns.get(root);
    const value = event.target.type === "range" ? Number(event.target.value) : event.target.value;
    setPath(design, JSON.parse(event.target.dataset.forgePath), value);
    event.target.parentElement.querySelector("output")?.replaceChildren(String(value));
    updateForge(root);
  });
  document.addEventListener("change", (event) => {
    const root = event.target.closest?.("[data-forge-customization]");
    if (root && event.target.matches("[data-forge-catalog]")) loadForgeChassis(root, event.target.value);
  });
  document.addEventListener("pointerdown", (event) => {
    if (event.button !== 1 || !forgeHostContains(event)) return;
    event.preventDefault();
    orbitingForge = true;
  });
  window.addEventListener("pointermove", (event) => {
    if (!orbitingForge) return;
    event.preventDefault();
    command({ type: "orbit-forge", delta_x: event.movementX, delta_y: event.movementY });
  });
  window.addEventListener("pointerup", (event) => {
    if (event.button === 1) orbitingForge = false;
  });
  window.addEventListener("blur", () => { orbitingForge = false; });
  document.addEventListener("auxclick", (event) => {
    if (event.button === 1 && forgeHostContains(event)) event.preventDefault();
  });
  document.addEventListener("wheel", (event) => {
    if (!forgeHostContains(event)) return;
    event.preventDefault();
    const unit = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? 16
      : event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? host.clientHeight : 1;
    command({ type: "zoom-forge", delta: event.deltaY * unit });
  }, { passive: false });
  document.addEventListener("click", (event) => {
    const link = event.target.closest?.("[data-persistent-tactical]");
    if (!link) return;
    event.preventDefault();
    document.body.setAttribute("data-tactical-active", "");
    command({
      type: "enter-tactical",
      server_addr: link.dataset.serverAddr,
      character_id: Number(link.dataset.characterId),
    });
  });
  window.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || !document.body.hasAttribute("data-tactical-active")) return;
    document.body.removeAttribute("data-tactical-active");
    command({ type: "exit-tactical" });
    mount();
  });
}
