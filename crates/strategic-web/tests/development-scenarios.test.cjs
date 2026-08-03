const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "..");
const characterTemplate = fs.readFileSync(path.join(root, "src/templates/character.rs"), "utf8");
const characterRoutes = fs.readFileSync(path.join(root, "src/routes/characters.rs"), "utf8");
const questRoutes = fs.readFileSync(path.join(root, "src/routes/developer_quests.rs"), "utf8");
const scenarioScript = fs.readFileSync(path.join(root, "static/development-scenarios.js"), "utf8");

test("development roster is grouped, searchable, and adopts only registered primaries", () => {
  assert.match(characterTemplate, /Test scenarios/);
  assert.match(characterTemplate, /data-scenario-search/);
  assert.match(characterTemplate, /scenario\.primary_character_id/);
  assert.match(characterRoutes, /adopt_development_scenarios/);
  assert.match(characterRoutes, /backend_development_scenarios/);
  assert.doesNotMatch(characterRoutes, /SELECT \* FROM character/);
  assert.match(scenarioScript, /dataset\.scenarioSearchText/);
});

test("quest inspector separates safe and canonical state and offers one typed update", () => {
  assert.match(questRoutes, /Player-safe:/);
  assert.match(questRoutes, /Private problem ID/);
  assert.match(questRoutes, /Canonical case ID/);
  assert.match(questRoutes, /Trigger next incident \/ attack/);
  assert.match(questRoutes, /trigger_development_scenario_incident/);
  assert.doesNotMatch(questRoutes.split("#[cfg(any())]")[0], /manifest_json|authority_json/);
});
