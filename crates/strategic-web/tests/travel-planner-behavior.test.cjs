const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const root = path.join(__dirname, "..");
const plannerPath = path.join(root, "static", "travel-planner.js");

const plannerHelpers = () => {
  let source = fs.readFileSync(plannerPath, "utf8");
  source = source.replace(
    "  initializeTravelPlanner();",
    "  globalThis.__planner = { parseSegments, position, turnaroundElapsed, moonName, moonGeometry, provisionQuantities };",
  );
  const context = { document: { addEventListener() {} } };
  vm.runInNewContext(source, context);
  return context.__planner;
};

test("persisted quest segments place turnaround after outbound walking and intervening rest", () => {
  const helpers = plannerHelpers();
  const segments = helpers.parseSegments("w,0,480,0,480,0,.3,.3,0|m,480,600,480,0,.3,0,0,600|w,1080,960,480,960,0,.5,.5,0");
  assert.equal(helpers.turnaroundElapsed(segments, 720, 2040), 1320);
  assert.notEqual(helpers.turnaroundElapsed(segments, 720, 2040), 2040);
  assert.ok(helpers.position(1080, 2040) > helpers.position(480, 2040), "progress advances during rest");
});

test("provision target math supports positive and negative surplus", () => {
  const helpers = plannerHelpers();
  assert.deepEqual(
    { ...helpers.provisionQuantities({ remainingDays: 2, target: 1, foodDays: 1, waterDays: 2, members: 2, rationKcal: 3000, skinMl: 4000 }) },
    { rations: 8, skins: 2 },
  );
  assert.deepEqual(
    { ...helpers.provisionQuantities({ remainingDays: 2, target: -1, foodDays: 1, waterDays: 2, members: 2, rationKcal: 3000, skinMl: 4000 }) },
    { rations: 0, skins: 0 },
  );
});

test("moon geometry distinguishes canonical phases and mirrors waxing from waning", () => {
  const helpers = plannerHelpers();
  const fresh = helpers.moonGeometry(0);
  const first = helpers.moonGeometry(.25);
  const full = helpers.moonGeometry(.5);
  const last = helpers.moonGeometry(.75);
  const waxing = helpers.moonGeometry(.125);
  const waning = helpers.moonGeometry(.875);
  assert.equal(fresh.path, "");
  assert.match(full.path, /A 8 8 0 1 1/);
  assert.equal(first.transform, "");
  assert.equal(last.transform, "translate(20 0) scale(-1 1)");
  assert.equal(waxing.path, waning.path);
  assert.notEqual(waxing.transform, waning.transform);
  assert.equal(helpers.moonName(.125), "waxing crescent");
  assert.equal(helpers.moonName(.875), "waning crescent");
});

test("planner source covers midnight chronology, hidden fatigue detail, config bounds, and live remount", () => {
  const source = fs.readFileSync(plannerPath, "utf8");
  const template = fs.readFileSync(path.join(root, "src", "templates", "settlement.rs"), "utf8");
  assert.match(source, /Math\.ceil\(departure \/ DAY\) \* DAY/);
  assert.match(source, /travel-midnight-tick/);
  assert.match(source, /summary\.setAttribute\("aria-label", summary\.title\)/);
  assert.match(source, /peak >= 1/);
  assert.doesNotMatch(template, /class="travel-resource-summary"/);
  assert.match(template, /name="walking_hours" min="1" max="16"/);
  assert.doesNotMatch(template, /name="fixed_camp_hours"/);
  assert.match(template, /hours camp\/downtime per full day/);
  assert.match(source, /TRACK_END - TRACK_START\) \* node\.duration \/ elapsedTotal/);
  assert.match(template, /data-selected-round-trip/);
  assert.match(source, /planner\.dataset\.selectedRoundTrip === "true"/);
  assert.match(source, /strategic-live-regions-refreshed/);
  assert.match(source, /dataset\.travelPlannerReady === "true"/);
  assert.match(source, /if \(!name \|\| elapsedTotal <= 0\) \{ planner\.hidden = true/);
});

test("camp renderer coalesces contiguous actual and forecast portions before deriving walking gaps", () => {
  const template = fs.readFileSync(path.join(root, "src", "templates", "settlement.rs"), "utf8");
  assert.match(template, /last\.movement_minute == camp\.movement_minute/);
  assert.match(template, /\*was_actual \|= actual/);
  assert.match(template, /\*was_forecast \|= forecast/);
  assert.match(template, /let kind = if actual && forecast \{\s*"m"/);
});

test("authoritative travel guards stale sync, bounded legacy vectors, and terminal provision use", () => {
  const strategic = fs.readFileSync(path.join(root, "..", "adventuresim-stdb-module", "src", "strategic.rs"), "utf8");
  const time = fs.readFileSync(path.join(root, "..", "adventuresim-stdb-module", "src", "time.rs"), "utf8");
  assert.match(strategic, /synchronize_party_departure_time[\s\S]+revalidate_party_after_departure_sync/);
  assert.match(strategic, /pending_incident[\s\S]+departure_snapshot_allows_travel/);
  assert.match(strategic, /stops\.len\(\) >= MAX_ITINERARY_SEGMENTS/);
  const personalNeeds = time.indexOf("apply_elapsed_needs(ctx, member_id, elapsed)?;");
  const personalTerminal = time.indexOf("if terminal.is_some()", personalNeeds);
  assert.ok(personalNeeds >= 0 && personalNeeds < personalTerminal, "personal camp sync consumes needs before terminal return");
  const partyNeeds = time.indexOf("apply_elapsed_needs(ctx, member_id, elapsed)?;", personalNeeds + 1);
  const partyTerminal = time.indexOf("if terminal.is_some()", partyNeeds);
  assert.ok(partyNeeds > personalNeeds && partyNeeds < partyTerminal, "party camp consumes needs before terminal return");
  assert.match(strategic, /plan_version == 0[\s\S]+reconstruct_legacy_journey_coordinates/);
});
