const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const script = fs.readFileSync(path.join(root, "static", "developer-quest-editor.js"), "utf8");
const layout = fs.readFileSync(path.join(root, "src", "templates", "layout.rs"), "utf8");
const route = fs.readFileSync(path.join(root, "src", "routes", "developer_quests.rs"), "utf8");
const {
  replaceAtPath,
  hydrateWitnessBinding,
  hydratePatternBinding,
  schemaRepeaterDefault,
} = require(path.join(root, "static", "developer-quest-editor.js"));

test("quest editor is settlement-only and gated by the existing browser-local developer mode", () => {
  assert.match(layout, /data-developer-quest-open/);
  assert.match(layout, /data-developer-only/);
  assert.match(script, /data-environment/);
  assert.match(script, /dataset\.environment !== "settlement"/);
  assert.match(script, /data-developer-mode/);
  assert.doesNotMatch(script, /localStorage/);
  const opener = layout.indexOf("data-developer-quest-open");
  const guard = layout.lastIndexOf("@if let Some(name) = logged_in_as", opener);
  const right = layout.lastIndexOf('div class="top-bar-right"', opener);
  assert.ok(right < guard && guard < opener);
});

test("native modal preserves drafts, supports repeaters, and restores focus", () => {
  assert.match(layout, /dialog class="developer-quest-dialog"/);
  assert.match(layout, /aria-labelledby="developer-quest-title"/);
  assert.match(script, /structuredClone\(schema\.definition\)/);
  assert.match(script, /value\.push\(cloneDefault/);
  assert.match(script, /value\.splice\(index, 1\)/);
  assert.match(script, /dialog\.addEventListener\("close", \(\) => opener\?\.focus\(\)\)/);
});

test("submission handles structured 422 diagnostics, override, and duplicate prevention", () => {
  assert.match(script, /response\.status === 422/);
  assert.match(script, /allow_implausible/);
  assert.match(script, /if \(submitting \|\| !draft\) return/);
  assert.match(script, /submit\.disabled = true/);
  assert.match(script, /data-developer-field-error/);
  assert.doesNotMatch(script, /location\.(assign|replace)|window\.location/);
});

test("HTTP adapter derives settlement and leaves discovery to normal rumors", () => {
  assert.match(route, /current_settlement_id/);
  const request = route.split("struct SpawnRequest {")[1].split("}")[0];
  assert.doesNotMatch(request, /settlement_id/);
  assert.match(route, /"discovery":"normal_rumor"/);
  assert.doesNotMatch(route, /rumor_receipt|journal|case_site_pin|referral/);
  assert.match(route, /SELECT \* FROM character_time WHERE character_id/);
  assert.match(route, /now_minute/);
});

test("typed constructors replace whole tagged values and structured options", () => {
  const model = {
    evidence: [{ inspection_topics: [{ check: null }] }],
    actions: [{ outputs: [{ kind: "ambush_ready" }] }],
  };
  replaceAtPath(model, "evidence.0.inspection_topics.0.check", {
    stat: "eyesight", difficulty_milli: 1234, success_description: "detail", reveals_clue: true,
  });
  replaceAtPath(model, "actions.0.outputs.0", {
    kind: "pattern_condition",
    evidence_id: "evidence:new",
    condition: { kind: "broad_survey" },
  });
  assert.deepEqual(model.evidence[0].inspection_topics[0].check, {
    stat: "eyesight", difficulty_milli: 1234, success_description: "detail", reveals_clue: true,
  });
  assert.deepEqual(model.actions[0].outputs[0], {
    kind: "pattern_condition",
    evidence_id: "evidence:new",
    condition: { kind: "broad_survey" },
  });
});

test("NPC selection atomically hydrates locked witness and pattern bindings", () => {
  const binding = {
    npc_id: "npc:two", display_name: "Else", demographic: "merchant",
    age_band: "adult", sex: "female", profession: "merchant",
    visible_description: "tall", expected_location: "market",
    expected_location_label: "Market", presence_version: 42,
    allowed_circumstances: ["road"],
  };
  const witness = { npc_id: "npc:one", circumstance: "night_window" };
  hydrateWitnessBinding(witness, binding);
  assert.deepEqual(
    {
      npc_id: witness.npc_id, name: witness.display_name,
      demographic: witness.demographic, circumstance: witness.circumstance,
      location: witness.expected_location, description: witness.visible_description,
    },
    {
      npc_id: "npc:two", name: "Else", demographic: "merchant",
      circumstance: "road", location: "market", description: "tall",
    },
  );
  const pattern = { cohort_id: "cohort:new" };
  hydratePatternBinding(pattern, binding, "riverdale");
  assert.equal(pattern.expected_settlement_id, "riverdale");
  assert.equal(pattern.presence_version, 42);
  assert.equal(pattern.npc_id, "npc:two");
});

test("open YAML content IDs are supplied by schema rather than JavaScript", () => {
  for (const openId of ["cave", "crypt", "footprints", "cloth_scrap", "wolf", "goblin", "laborer", "night_window"]) {
    assert.doesNotMatch(script, new RegExp(`["']${openId}["']`));
  }
  assert.match(script, /site_kind:\s*"sites"/);
  assert.match(script, /evidence_kind:\s*"evidence"/);
  assert.match(script, /threat:\s*"threats"/);
});

test("repeater defaults follow renamed catalog identities after old first entries are removed", () => {
  const schema = {
    definition: { template_id: "renamed_template" },
    options: {
      templates: [{
        value: "renamed_template",
        binding: {
          routes: ["removed_route", "renamed_route"],
          objectives: ["removed_objective", "renamed_objective"],
        },
      }],
      configured_routes: [{ value: "removed_route" }, { value: "renamed_route" }],
      configured_objectives: [{ value: "removed_objective" }, { value: "renamed_objective" }],
      sites: [{ value: "removed_site" }, { value: "renamed_site" }],
      evidence: [{ value: "removed_evidence" }, { value: "renamed_evidence" }],
      threats: [{ value: "removed_threat" }, { value: "renamed_threat" }],
      finale_kinds: ["removed_finale", "renamed_finale"],
    },
  };
  for (const options of Object.values(schema.options)) options.shift();
  schema.options.templates = [{
    value: "renamed_template",
    binding: {
      routes: ["renamed_route"],
      objectives: ["renamed_objective"],
    },
  }];

  assert.equal(schemaRepeaterDefault(schema, "configured_routes"), "renamed_route");
  assert.equal(schemaRepeaterDefault(schema, "action_route"), "renamed_route");
  assert.equal(schemaRepeaterDefault(schema, "configured_objectives"), "renamed_objective");
  assert.equal(schemaRepeaterDefault(schema, "site_kind"), "renamed_site");
  assert.equal(schemaRepeaterDefault(schema, "evidence_kind"), "renamed_evidence");
  assert.equal(schemaRepeaterDefault(schema, "threat"), "renamed_threat");
  assert.equal(schemaRepeaterDefault(schema, "finale_kind"), "renamed_finale");
  assert.doesNotMatch(script, /configured_routes:\s*["']physical_trail["']/);
  assert.doesNotMatch(script, /configured_objectives:\s*["']defeat["']/);
  assert.doesNotMatch(script, /actions:\s*\{[^}]*route:\s*["']physical_trail["']/);
  assert.doesNotMatch(script, /finales:\s*\{[^}]*kind:\s*["']defeat["']/);
});
