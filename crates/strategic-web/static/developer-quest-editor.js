(() => {
  "use strict";
  const replaceAtPath = (root, path, constructor) => {
    const parts = path.split(".").map((part) => /^\d+$/.test(part) ? Number(part) : part);
    const key = parts.pop();
    const parent = parts.reduce((item, part) => item[part], root);
    parent[key] = structuredClone(constructor);
    return root;
  };
  const hydrateWitnessBinding = (target, binding) => {
    Object.assign(target, {
      npc_id: binding.npc_id,
      display_name: binding.display_name,
      demographic: binding.demographic,
      expected_location: binding.expected_location,
      expected_location_label: binding.expected_location_label,
      visible_description: binding.visible_description,
    });
    if (!binding.allowed_circumstances.includes(target.circumstance)) {
      target.circumstance = binding.allowed_circumstances[0];
    }
    return target;
  };
  const hydratePatternBinding = (target, binding, settlementId) => {
    Object.assign(target, {
      npc_id: binding.npc_id,
      demographic: binding.demographic,
      age_band: binding.age_band,
      sex: binding.sex,
      profession: binding.profession,
      expected_settlement_id: settlementId,
      expected_location: binding.expected_location,
      expected_location_label: binding.expected_location_label,
      presence_version: binding.presence_version,
    });
    return target;
  };
  const schemaRepeaterDefault = (schema, key, templateId = schema.definition?.template_id) => {
    const template = schema.options?.templates?.find((option) => option.value === templateId);
    if ((key === "configured_routes" || key === "action_route")
      && template?.binding?.routes?.length) {
      return template.binding.routes[0];
    }
    if (key === "configured_objectives" && template?.binding?.objectives?.length) {
      return template.binding.objectives[0];
    }
    const groups = {
      configured_routes: "configured_routes",
      configured_objectives: "configured_objectives",
      site_kind: "sites",
      evidence_kind: "evidence",
      threat: "threats",
      finale_kind: "finale_kinds",
    };
    const option = schema.options?.[groups[key]]?.[0];
    return typeof option === "string" ? option : option?.value || "";
  };
  if (typeof module !== "undefined") {
    module.exports = {
      replaceAtPath,
      hydrateWitnessBinding,
      hydratePatternBinding,
      schemaRepeaterDefault,
    };
  }
  if (typeof document === "undefined") return;
  const dialog = document.querySelector("[data-developer-quest-dialog]");
  if (!dialog) return;
  const form = dialog.querySelector("[data-developer-quest-form]");
  const fields = dialog.querySelector("[data-developer-quest-fields]");
  const errors = dialog.querySelector("[data-developer-quest-errors]");
  const status = dialog.querySelector("[data-developer-quest-status]");
  const settlement = dialog.querySelector("[data-developer-quest-settlement]");
  const submit = dialog.querySelector("[data-developer-quest-submit]");
  let schema;
  let draft;
  let opener;
  let submitting = false;

  const label = (value) => String(value).replaceAll("_", " ").replaceAll(".", " › ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
  const splitPath = (path) => path === "" ? [] : path.split(".").map((part) => /^\d+$/.test(part) ? Number(part) : part);
  const getAt = (path) => splitPath(path).reduce((value, key) => value?.[key], draft);
  const setAt = (path, value) => {
    const parts = splitPath(path);
    const key = parts.pop();
    const parent = parts.reduce((item, part) => item[part], draft);
    parent[key] = value;
  };

  const optionGroup = (path) => {
    if (path === "template_id") return "templates";
    if (/^configured_routes\.\d+$/.test(path)) return "configured_routes";
    if (/^configured_objectives\.\d+$/.test(path)) return "configured_objectives";
    if (path === "cause.hostile" || /^hostile_groups\.\d+\.2$/.test(path)) return "threats";
    if (/^sites\.\d+\.kind$/.test(path)) return "sites";
    if (/^evidence\.\d+\.kind$/.test(path)) return "evidence";
    if (/^witnesses\.\d+\.npc_id$/.test(path)) return "witnesses";
    if (/^pattern_targets\.\d+\.npc_id$/.test(path)) return "witnesses";
    if (path.endsWith(".demographic")) return "witness_demographics";
    if (path.endsWith(".circumstance")) return "circumstances";
    if (path.endsWith(".description")) return "descriptions";
    if (path === "family" || path.endsWith(".family")) return "template_families";
    if (path === "role" || path.endsWith(".role")) return "site_roles";
    if (path.endsWith(".reliability")) return "reliabilities";
    if (path === "stat" || path.endsWith(".stat")) return "evidence_check_stats";
    if (path === "route" || path.endsWith(".route")) return "route_classes";
    if (path === "stage" || path.endsWith(".stage")) return "destination_stages";
    if (/^finales\.\d+\.kind$/.test(path)) return "finale_kinds";
    if (path === "action" || path.endsWith(".action")) return "dialogue_actions";
    if (path === "terrain" || path.endsWith(".terrain")) return "terrains";
    if (/^actions\.\d+\.kind$/.test(path)) return "investigation_actions";
    if (path === "symptom" || path.endsWith(".symptom")) return "symptoms";
    if (path === "encounter_archetype" || path.endsWith(".encounter_archetype")) return "encounter_archetypes";
    if (/^actions\.\d+\.outputs\.\d+\.kind$/.test(path)) return "action_output_kinds";
    if (path.endsWith(".condition.kind")) return "pattern_condition_kinds";
    if (path.endsWith(".consequence.kind")) return "action_consequence_kinds";
    return null;
  };
  const normalizedOptions = (group) => {
    const options = schema.options[group] || [];
    return options.map((option) => typeof option === "string"
      ? { value: option, label: label(option) }
      : option);
  };

  const emptyDefaults = {
    canonical_events: { id: "event:new", proposition_id: "proposition:new", subject: "subject", predicate: "affected", object: "object", occurred_at: 0 },
    sites: { id: "site:new", kind: null, role: "evidence", terrain: "underground", safe_label: "Place a witness described", exact_location_initially_known: false, is_true_location: false },
    areas: { id: "area:new", safe_label: "Nearby search area", terrain: "forest", contains_site_ids: [] },
    witnesses: { id: "witness:new", npc_id: "", display_name: "", demographic: null, circumstance: null, description: null, expected_location: "", expected_location_label: "", visible_description: "", testimony: [] },
    pattern_targets: { cohort_id: "cohort:new", npc_id: "", demographic: null, age_band: "adult", sex: "female", profession: "", expected_settlement_id: "", expected_location: "", expected_location_label: "", presence_version: 0 },
    evidence: { id: "evidence:new", kind: null, proposition_id: "proposition:new", site_id: "site:new", portrait_label: "Physical evidence", portrait_icon: "footprint", base_description: "You inspect the evidence.", inspection_topics: [], safe_description: "Physical evidence", corrects_proposition_id: null },
    track_trails: { id: "track-trail:new", segment_ids: [] },
    track_segments: { id: "track-segment:new", trail_id: "track-trail:new", ordinal: 0, terrain: "settlement", safe_finding: "The trail continues across this ground.", predecessor: null, next: null },
    actions: { id: "action:new", kind: "inspect_site", route: null, target_kind: "site", target_id: "site:new", prerequisite: null, alternate: "action:new", active_initially: false, safe_summary: "Inspect the site", track_segment_id: null, outputs: [] },
    custody: ["asset:new", "site:new"],
    hostile_groups: ["group:new", "site:new", null, 1],
    finales: { id: "finale:new", kind: null, site_id: "site:new", hostile_group_id: "group:new", subject_id: null, asset_id: null, strategic_outcome_compatible: true },
    dialogue_producers: { action: "expose", objective_id: "objective:new", recipient_npc_id: "", subject_ref: null, asset_id: null },
    bridges: { id: "bridge:new", explanation: "", event_id: "event:new", evidence_id: "evidence:new", action_id: "action:new", lead_summary: "" },
    testimony: { proposition_id: "proposition:new", reliability: "truthful", truthful_text: "", spoken_text: "", destination_stage: "unknown", site_id: null, corrects_proposition_id: null, referred_witness_ids: [] },
    inspection_topics: { id: "topic:new", label: "Inspect", inspection_description: "", check: null },
    outputs: { kind: "destination", stage: "unknown", site_id: null },
    contains_site_ids: "site:new",
    referred_witness_ids: "witness:new",
    segment_ids: "track-segment:new",
    configured_routes: null,
    configured_objectives: null,
    alternatives: { objectives: [] },
    objectives: { id: "objective:new", requirement: { Defeat: { hostile_group_id: "group:new", count: 1 } } },
  };
  const cloneDefault = (path, array) => {
    if (array.length) {
      const clone = structuredClone(array[array.length - 1]);
      const suffix = `-copy-${array.length + 1}`;
      if (clone && typeof clone === "object" && !Array.isArray(clone) && typeof clone.id === "string") clone.id += suffix;
      return clone;
    }
    const key = splitPath(path).at(-1);
    const clone = structuredClone(emptyDefaults[key] ?? "");
    if (key === "configured_routes" || key === "configured_objectives") {
      return schemaRepeaterDefault(schema, key, draft.template_id);
    }
    if (key === "sites") clone.kind = schemaRepeaterDefault(schema, "site_kind");
    if (key === "evidence") clone.kind = schemaRepeaterDefault(schema, "evidence_kind");
    if (key === "hostile_groups") clone[2] = schemaRepeaterDefault(schema, "threat");
    if (key === "actions") clone.route = schemaRepeaterDefault(schema, "action_route", draft.template_id);
    if (key === "finales") clone.kind = schemaRepeaterDefault(schema, "finale_kind");
    if (key === "witnesses") {
      const binding = schema.options.witnesses[0]?.binding;
      if (binding) Object.assign(clone, binding, { circumstance: binding.allowed_circumstances[0], description: schema.options.descriptions[0]?.value || "" });
    }
    if (key === "pattern_targets") hydratePatternObject(clone, schema.options.witnesses[0]?.binding);
    return clone;
  };

  const readonlyBindingPath = (path) =>
    /^witnesses\.\d+\.(display_name|demographic|expected_location|expected_location_label|visible_description)$/.test(path)
    || /^pattern_targets\.\d+\.(demographic|age_band|sex|profession|expected_settlement_id|expected_location|expected_location_label|presence_version)$/.test(path);
  const witnessOption = (npcId) => schema.options.witnesses.find((option) => option.value === npcId);
  function hydratePatternObject(target, binding) {
    if (!binding) return;
    hydratePatternBinding(target, binding, schema.settlement.id);
  }
  function hydrateNpcPath(path, npcId) {
    const binding = witnessOption(npcId)?.binding;
    if (!binding) return;
    if (/^witnesses\.\d+\.npc_id$/.test(path)) {
      const witness = getAt(path.split(".").slice(0, 2).join("."));
      hydrateWitnessBinding(witness, binding);
    } else if (/^pattern_targets\.\d+\.npc_id$/.test(path)) {
      hydratePatternObject(getAt(path.split(".").slice(0, 2).join(".")), binding);
    }
  }

  function primitiveControl(name, value, path) {
    const wrapper = document.createElement("label");
    wrapper.append(document.createTextNode(label(name)));
    const group = optionGroup(path);
    let control;
    if (group) {
      control = document.createElement("select");
      let choices = normalizedOptions(group);
      if (group === "circumstances" && /^witnesses\.\d+\.circumstance$/.test(path)) {
        const witness = getAt(path.split(".").slice(0, 2).join("."));
        const allowed = witnessOption(witness.npc_id)?.binding.allowed_circumstances || [];
        choices = choices.filter((option) => allowed.includes(option.value));
      }
      for (const option of choices) {
        const element = document.createElement("option");
        element.value = option.value; element.textContent = option.label;
        element.selected = String(value) === String(option.value);
        control.append(element);
      }
    } else if (typeof value === "boolean") {
      control = document.createElement("input");
      control.type = "checkbox"; control.checked = value;
    } else if (typeof value === "number") {
      control = document.createElement("input");
      control.type = "number"; control.step = "1"; control.value = value;
    } else if (value === null) {
      control = document.createElement("input");
      control.value = ""; control.placeholder = "None";
    } else if (String(value).length > 72 || /text|description|summary|explanation/.test(name)) {
      control = document.createElement("textarea");
      control.rows = 2; control.value = value;
    } else {
      control = document.createElement("input");
      control.value = value;
    }
    control.dataset.developerQuestPath = path;
    control.disabled = readonlyBindingPath(path);
    control.addEventListener("change", () => {
      let next = control.type === "checkbox" ? control.checked : control.value;
      if (typeof value === "number") next = Number(next);
      if (value === null && next === "") next = null;
      setAt(path, next);
      if (group === "witnesses") {
        hydrateNpcPath(path, next);
        render();
      }
    });
    wrapper.append(control);
    return wrapper;
  }

  function renderCause(container, value, path) {
    const fieldset = document.createElement("fieldset");
    const legend = document.createElement("legend"); legend.textContent = "Canonical Cause"; fieldset.append(legend);
    const kindLabel = document.createElement("label"); kindLabel.append("Cause kind");
    const kind = document.createElement("select");
    const kinds = schema.options.cause_kinds;
    const current = typeof value === "string" ? value : Object.keys(value)[0];
    for (const candidate of kinds) {
      const option = document.createElement("option"); option.value = candidate; option.textContent = label(candidate); option.selected = candidate === current; kind.append(option);
    }
    kind.dataset.developerQuestPath = path;
    kind.addEventListener("change", () => {
      replaceAtPath(draft, path, schema.constructors.variants.cause[kind.value]);
      render();
    });
    kindLabel.append(kind); fieldset.append(kindLabel);
    if (current === "hostile") fieldset.append(primitiveControl("Threat", value.hostile, `${path}.hostile`));
    container.append(fieldset);
  }

  function renderObjectiveRequirement(container, value, path) {
    const fieldset = document.createElement("fieldset");
    const legend = document.createElement("legend"); legend.textContent = "Objective Requirement"; fieldset.append(legend);
    const current = Object.keys(value)[0];
    const wrapper = document.createElement("label"); wrapper.append("Requirement kind");
    const select = document.createElement("select"); select.dataset.developerQuestPath = path;
    for (const optionValue of schema.options.objective_requirements) {
      const option = document.createElement("option"); option.value = optionValue; option.textContent = label(optionValue); option.selected = optionValue === current; select.append(option);
    }
    select.addEventListener("change", () => { replaceAtPath(draft, path, schema.constructors.variants.objective_requirement[select.value]); render(); });
    wrapper.append(select); fieldset.append(wrapper);
    renderValue(fieldset, current, value[current], `${path}.${current}`);
    container.append(fieldset);
  }

  const taggedVariantGroup = (path) => {
    if (/^actions\.\d+\.outputs\.\d+$/.test(path)) return "action_output";
    if (path.endsWith(".condition")) return "pattern_condition";
    if (path.endsWith(".consequence")) return "action_consequence";
    return null;
  };
  function renderTaggedVariant(container, name, value, path, group) {
    const constructors = schema.constructors.variants[group];
    const fieldset = document.createElement("fieldset");
    const legend = document.createElement("legend"); legend.textContent = label(name); fieldset.append(legend);
    const wrapper = document.createElement("label"); wrapper.append("Variant");
    const select = document.createElement("select"); select.dataset.developerQuestPath = `${path}.kind`;
    for (const key of Object.keys(constructors)) {
      const option = document.createElement("option"); option.value = key; option.textContent = label(key); option.selected = key === value.kind; select.append(option);
    }
    select.addEventListener("change", () => {
      replaceAtPath(draft, path, constructors[select.value]);
      render();
    });
    wrapper.append(select); fieldset.append(wrapper);
    for (const [key, child] of Object.entries(value)) {
      if (key !== "kind") renderValue(fieldset, key, child, `${path}.${key}`);
    }
    container.append(fieldset);
  }
  function renderOptionalCheck(container, name, value, path) {
    const fieldset = document.createElement("fieldset");
    const legend = document.createElement("legend"); legend.textContent = label(name); fieldset.append(legend);
    const wrapper = document.createElement("label"); wrapper.append("Inspection check");
    const select = document.createElement("select"); select.dataset.developerQuestPath = path;
    for (const [optionValue, optionLabel] of [["none", "No check"], ["configured", "Configured deterministic check"]]) {
      const option = document.createElement("option"); option.value = optionValue; option.textContent = optionLabel;
      option.selected = (optionValue === "none") === (value === null); select.append(option);
    }
    select.addEventListener("change", () => {
      replaceAtPath(draft, path, select.value === "none" ? null : schema.constructors.optional.evidence_check);
      render();
    });
    wrapper.append(select); fieldset.append(wrapper);
    if (value !== null) {
      for (const [key, child] of Object.entries(value)) renderValue(fieldset, key, child, `${path}.${key}`);
    }
    container.append(fieldset);
  }

  function renderValue(container, name, value, path) {
    if (path === "cause") return renderCause(container, value, path);
    if (/^objectives\.alternatives\.\d+\.objectives\.\d+\.requirement$/.test(path)) {
      return renderObjectiveRequirement(container, value, path);
    }
    if (/^evidence\.\d+\.inspection_topics\.\d+\.check$/.test(path)) {
      return renderOptionalCheck(container, name, value, path);
    }
    const variantGroup = taggedVariantGroup(path);
    if (variantGroup && value && typeof value === "object") {
      return renderTaggedVariant(container, name, value, path, variantGroup);
    }
    if (Array.isArray(value)) {
      const fieldset = document.createElement("fieldset");
      const legend = document.createElement("legend"); legend.textContent = `${label(name)} (${value.length})`; fieldset.append(legend);
      value.forEach((item, index) => {
        const itemPath = `${path}.${index}`;
        const itemBox = document.createElement("div"); itemBox.className = "developer-quest-array-item";
        renderValue(itemBox, `${name} ${index + 1}`, item, itemPath);
        const remove = document.createElement("button"); remove.type = "button"; remove.className = "btn btn-small developer-quest-remove";
        remove.textContent = "Remove"; remove.setAttribute("aria-label", `Remove ${label(name)} ${index + 1}`);
        remove.addEventListener("click", () => { value.splice(index, 1); render(); });
        itemBox.append(remove); fieldset.append(itemBox);
      });
      const actions = document.createElement("div"); actions.className = "developer-quest-array-actions";
      const add = document.createElement("button"); add.type = "button"; add.className = "btn btn-small";
      add.textContent = `Add ${label(name).replace(/s$/, "")}`; add.setAttribute("aria-label", add.textContent);
      add.addEventListener("click", () => { value.push(cloneDefault(path, value)); render(); });
      actions.append(add); fieldset.append(actions); container.append(fieldset); return;
    }
    if (value && typeof value === "object") {
      const fieldset = document.createElement("fieldset");
      const legend = document.createElement("legend"); legend.textContent = label(name); fieldset.append(legend);
      if (/^witnesses\.\d+$/.test(path)) {
        const binding = witnessOption(value.npc_id)?.binding;
        if (binding) {
          const snapshot = document.createElement("p");
          snapshot.className = "developer-quest-binding-snapshot";
          snapshot.textContent = `Bound NPC snapshot: ${binding.age_band}, ${binding.sex}, ${binding.profession}; presence ${binding.presence_version}. Allowed circumstances: ${binding.allowed_circumstances.map(label).join(", ")}.`;
          fieldset.append(snapshot);
        }
      }
      for (const [key, child] of Object.entries(value)) renderValue(fieldset, key, child, path ? `${path}.${key}` : key);
      container.append(fieldset); return;
    }
    container.append(primitiveControl(name, value, path));
  }

  function render() {
    fields.replaceChildren();
    for (const [key, value] of Object.entries(draft)) renderValue(fields, key, value, key);
  }
  function clearErrors() {
    errors.hidden = true; errors.replaceChildren();
    dialog.querySelectorAll("[data-developer-field-error]").forEach((field) => field.removeAttribute("data-developer-field-error"));
  }
  function showErrors(diagnostics) {
    clearErrors();
    const heading = document.createElement("strong"); heading.textContent = "Quest was not created:";
    const list = document.createElement("ul");
    for (const item of diagnostics) {
      const row = document.createElement("li"); row.textContent = `${item.path}: ${item.message}`; list.append(row);
      const field = dialog.querySelector(`[data-developer-quest-path="${CSS.escape(item.path)}"]`)
        || dialog.querySelector(`[data-developer-quest-path^="${CSS.escape(item.path)}"]`);
      field?.setAttribute("data-developer-field-error", "");
    }
    errors.append(heading, list); errors.hidden = false;
    const first = dialog.querySelector("[data-developer-field-error]");
    (first || errors).focus?.(); first?.scrollIntoView({ block: "center" });
  }
  async function openEditor(button) {
    if (!document.documentElement.hasAttribute("data-developer-mode")) return;
    if (document.querySelector("[data-environment]")?.dataset.environment !== "settlement") return;
    opener = button; clearErrors(); status.textContent = "Loading startup catalog and current witnesses…";
    dialog.showModal();
    try {
      const response = await fetch("/api/developer/quests/schema", { headers: { Accept: "application/json" } });
      if (!response.ok) throw new Error(`Schema request failed (${response.status})`);
      schema = await response.json(); draft = structuredClone(schema.definition);
      settlement.textContent = `${schema.settlement.name} · catalog ${schema.catalog_revision.slice(0, 12)}`;
      status.textContent = "Configure canonical truth. The quest remains undiscovered after creation.";
      render();
    } catch (error) {
      showErrors([{ path: "$", message: error.message }]);
      status.textContent = "";
    }
  }
  document.addEventListener("click", (event) => {
    const open = event.target.closest("[data-developer-quest-open]");
    if (open) openEditor(open);
    if (event.target.closest("[data-developer-quest-close]")) dialog.close();
  });
  dialog.addEventListener("close", () => opener?.focus());
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (submitting || !draft) return;
    submitting = true; submit.disabled = true; clearErrors(); status.textContent = "Validating and creating latent quest…";
    try {
      const response = await fetch("/api/developer/quests", {
        method: "POST", headers: { "Content-Type": "application/json", Accept: "application/json" },
        body: JSON.stringify({ definition: draft, allow_implausible: dialog.querySelector("[data-developer-quest-override]").checked }),
      });
      const body = await response.json().catch(() => ({}));
      if (response.status === 422 || !response.ok) {
        showErrors(body.diagnostics || [{ path: "$", message: `Quest creation failed (${response.status})` }]);
        status.textContent = "Fix structural errors, or explicitly override compatibility warnings.";
        return;
      }
      status.textContent = "Quest created. It remains latent until ordinary tavern or NPC rumor discovery.";
      setTimeout(() => dialog.close(), 700);
    } catch (error) {
      showErrors([{ path: "$", message: error.message }]); status.textContent = "";
    } finally {
      submitting = false; submit.disabled = false;
    }
  });
})();
