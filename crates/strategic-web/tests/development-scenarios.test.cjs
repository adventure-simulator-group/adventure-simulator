const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "..");
const characterTemplate = fs.readFileSync(path.join(root, "src/templates/character.rs"), "utf8");
const characterRoutes = fs.readFileSync(path.join(root, "src/routes/characters.rs"), "utf8");
const questRoutes = fs.readFileSync(path.join(root, "src/routes/developer_quests.rs"), "utf8");
const scenarioModule = fs.readFileSync(path.join(root, "../adventuresim-stdb-module/src/strategic/development_scenarios.rs"), "utf8");
const scenarioScript = fs.readFileSync(path.join(root, "static/development-scenarios.js"), "utf8");
const layoutTemplate = fs.readFileSync(path.join(root, "src/templates/layout.rs"), "utf8");

test("development roster is grouped, searchable, and adopts only registered primaries", () => {
  assert.match(characterTemplate, /Test scenarios/);
  assert.match(characterTemplate, /data-scenario-search/);
  assert.match(characterTemplate, /scenario\.primary_character_id/);
  assert.match(characterRoutes, /adopt_development_scenarios/);
  assert.match(characterRoutes, /backend_development_scenarios/);
  assert.doesNotMatch(characterRoutes, /SELECT \* FROM character/);
  assert.match(scenarioScript, /dataset\.scenarioSearchText/);
});

test("quest inspector unifies bounded quest kinds and offers only its typed update", () => {
  assert.match(questRoutes, /Player-safe:/);
  assert.match(questRoutes, /Private subject ID/);
  assert.match(questRoutes, /Canonical case ID/);
  assert.match(questRoutes, /Trigger next incident \/ attack/);
  assert.match(questRoutes, /trigger_development_scenario_incident/);
  assert.match(questRoutes, /supports_incident_action/);
  assert.match(scenarioModule, /MAX_SUBJECT_INPUTS/);
  assert.match(scenarioModule, /MAX_KIND_INPUTS/);
  assert.match(scenarioModule, /MAX_OUTPUTS/);
  assert.match(scenarioModule, /"errantry contract"/);
  assert.match(scenarioModule, /"road encounter"/);
  assert.doesNotMatch(questRoutes, /manifest_json|authority_json/);
});

test("scenario inspector crosses the full-page navigation boundary", () => {
  assert.match(
    layoutTemplate,
    /a href="\/developer\/scenarios"[^\n{]*data-hard-navigation/,
  );
});
