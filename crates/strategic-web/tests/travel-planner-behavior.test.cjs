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
    "  globalThis.__planner = { parseSegments, position, turnaroundElapsed, moonName, moonGeometry, calendarDate, provisionQuantities, fatigueBand, splitFatigueSegment, fatigueAtElapsed, timePeriodAt, formatClock, stepRangeValue };",
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

test("walking-hours wheel steps respect slider precision and bounds", () => {
  const helpers = plannerHelpers();
  assert.equal(helpers.stepRangeValue("8", 1, .25, 0, 24), 8.25);
  assert.equal(helpers.stepRangeValue("8", -1, .25, 0, 24), 7.75);
  assert.equal(helpers.stepRangeValue("24", 1, .25, 0, 24), 24);
  assert.equal(helpers.stepRangeValue("0", -1, .25, 0, 24), 0);
});

test("fatigue and daylight use discrete threshold bands", () => {
  const helpers = plannerHelpers();
  assert.equal(helpers.fatigueBand(.49), "green");
  assert.equal(helpers.fatigueBand(.5), "yellow");
  assert.equal(helpers.fatigueBand(.8), "red");
  assert.equal(helpers.fatigueBand(1), "stopped");
  assert.deepEqual(Array.from(helpers.splitFatigueSegment({ start: 0, duration: 120, fatigueStart: 0, fatigueEnd: 1.2 }), (part) => part.band), ["green", "yellow", "red", "stopped"]);
  assert.equal(helpers.timePeriodAt(5 * 60 + 59), "night");
  assert.equal(helpers.timePeriodAt(6 * 60), "sunrise");
  assert.equal(helpers.timePeriodAt(8 * 60), "day");
  assert.equal(helpers.timePeriodAt(18 * 60), "sunset");
  assert.equal(helpers.timePeriodAt(20 * 60), "night");
  assert.equal(helpers.formatClock(19 * 60 + 7), "19:07");
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

test("provisioning preserves the exact planner state in a reusable return URL", () => {
  const source = fs.readFileSync(plannerPath, "utf8");
  const background = fs.readFileSync(path.join(root, "static", "background-fetch.js"), "utf8");
  assert.match(source, /searchParams\.set\("target_surplus"/);
  assert.match(source, /history\.replaceState/);
  assert.match(source, /params\.set\("return_to", `\$\{returnUrl\.pathname\}\$\{returnUrl\.search\}\$\{returnUrl\.hash\}`\)/);
  assert.match(background, /window\.strategicLocalReturnUrl/);
  assert.match(background, /window\.strategicApplyReturnNavigation/);
  assert.match(background, /input\[name=\"return_to\"\]/);
  assert.match(background, /parsed\.origin === location\.origin/);
});

test("party provisioning reads and updates the active inventory pane", () => {
  const trade = fs.readFileSync(path.join(root, "static", "party-trade.js"), "utf8");
  const template = fs.readFileSync(path.join(root, "src", "templates", "settlement.rs"), "utf8");
  const strategic = fs.readFileSync(path.join(root, "..", "adventuresim-stdb-module", "src", "strategic.rs"), "utf8");
  assert.match(trade, /\[data-inventory-pane\]:not\(\[hidden\]\)/);
  assert.match(template, /party-personal-currency/);
  assert.match(template, /Personal coin available for party purchases/);
  assert.match(strategic, /consume_personal_gold\(ctx, character_id, personal\)\?;\s*if personal > 0 \{\s*credit_party_stake\(ctx, party_id, character_id, personal\)/);
});

test("calendar labels preserve the canonical Monday-first week and 365-day year", () => {
  const helpers = plannerHelpers();
  assert.deepEqual({ ...helpers.calendarDate(0) }, { weekday: "Monday", day: 1, month: "January", isSunday: false });
  assert.deepEqual({ ...helpers.calendarDate(6 * 1440) }, { weekday: "Sunday", day: 7, month: "January", isSunday: true });
  assert.deepEqual({ ...helpers.calendarDate(31 * 1440) }, { weekday: "Thursday", day: 1, month: "February", isSunday: false });
  assert.deepEqual({ ...helpers.calendarDate(364 * 1440) }, { weekday: "Monday", day: 31, month: "December", isSunday: false });
});

test("planner source covers midnight chronology, hidden fatigue detail, config bounds, and live remount", () => {
  const source = fs.readFileSync(plannerPath, "utf8");
  const template = fs.readFileSync(path.join(root, "src", "templates", "settlement.rs"), "utf8");
  assert.match(source, /Math\.ceil\(departure \/ DAY\) \* DAY/);
  assert.match(source, /travel-midnight-tick/);
  assert.match(source, /travel-calendar-label/);
  assert.match(source, /Average fatigue ·/);
  assert.match(source, /attachRailTooltip/);
  assert.match(source, /candidate !== tooltip\) candidate\.hidden = true/);
  assert.doesNotMatch(source, /linear-gradient\(to bottom/);
  assert.match(source, /summary\.setAttribute\("aria-label", summary\.title\)/);
  assert.match(source, /peak >= 1/);
  assert.doesNotMatch(template, /class="travel-resource-summary"/);
  assert.match(template, /type="range" name="walking_hours" min="0" max="24"/);
  assert.match(template, /data-walking-hours-output/);
  assert.match(template, /travel-setting-heading/);
  assert.match(source, /walkingHours\?\.addEventListener\("wheel"/);
  assert.match(template, /Travel during/);
  assert.match(template, /name="travel_at_night" value="true"/);
  assert.match(template, /data-travel-period-toggle/);
  assert.doesNotMatch(template, /name="fixed_camp_hours"/);
  assert.doesNotMatch(template, /hours camp\/downtime per full day/);
  assert.match(template, /data-target-surplus-display/);
  assert.match(source, /StrategicNumericEditor\.open/);
  assert.match(source, /TRACK_END - TRACK_START\) \* node\.duration \/ elapsedTotal/);
  assert.match(source, /M 12 0 C 3 0[\s\S]+3 100 12 100/);
  const css = fs.readFileSync(path.join(root, "static", "css", "strategic.css"), "utf8");
  assert.match(css, /\.travel-camp-tent \{[^}]*top: 50%[^}]*translateY\(-50%\)/);
  assert.match(css, /\.travel-period-thumb::after \{[^}]*sun\.svg/);
  assert.match(css, /\.travel-period-toggle input:checked \+ \.travel-period-track \.travel-period-thumb \{[^}]*translateX/);
  assert.match(css, /--travel-rail-gap: \.42rem/);
  assert.match(css, /gap: var\(--travel-rail-gap\)/);
  assert.match(css, /\.travel-resource-path\.base/);
  assert.doesNotMatch(css, /\.travel-resource-path\.border/);
  assert.match(css, /\.travel-daylight-track \{[^}]*width: 64%/);
  assert.doesNotMatch(css, /\.travel-daylight-track \{[^}]*(?:border|box-shadow)/);
  assert.match(css, /\.travel-plan-camp::before,[\s\S]*\.travel-plan-camp::after \{[^}]*height: 2px[^}]*translateY\(-50%\)/);
  assert.match(css, /\.travel-plan-camp::after \{[^}]*right: var\(--travel-last-bar-inset\)[^}]*left: var\(--travel-first-bar-inset\)/);
  assert.match(css, /\.travel-plan-camp::after \{ top: 100%; \}/);
  assert.match(css, /\.travel-plan-camp \{[^}]*pointer-events: none/);
  assert.match(css, /\.travel-plan-camp \.travel-camp-brace \{ pointer-events: auto/);
  assert.match(css, /\.travel-camp-brace path \{[^}]*stroke-width: 2/);
  assert.match(css, /\.travel-camp-tent \{[^}]*width: 1rem[^}]*height: 1rem/);
  assert.match(css, /\.travel-fatigue-segment\.green/);
  assert.match(css, /\.travel-fatigue-segment\.stopped/);
  assert.match(css, /\.travel-daylight-segment\.sunrise/);
  assert.match(css, /\.travel-daylight-segment\.sunset/);
  assert.match(css, /\.travel-rail-tooltip/);
  assert.match(css, /\.travel-midnight-tick\.sunday \.travel-calendar-label/);
  const resourcePathRule = css.match(/\.travel-resource-path \{[^}]+\}/)?.[0] || "";
  assert.match(resourcePathRule, /stroke-width: 20/);
  assert.doesNotMatch(resourcePathRule, /vector-effect/);
  assert.match(template, /travel-period-option travel-period-day/);
  assert.match(template, /travel-period-option travel-period-night/);
  assert.match(css, /\.travel-period-night::after/);
  assert.match(css, /\.travel-period-thumb::before/);
  assert.doesNotMatch(template, /travel-destination-caption/);
  assert.doesNotMatch(template, /p \{ \(&destination\.description\) \}/);
  assert.match(css, /box-shadow: inset/);
  assert.doesNotMatch(css, /\.travel-fatigue-segment\.camp\s*\{[^}]*opacity/);
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
  assert.match(
    strategic,
    /prepare_party_waterskins\(ctx, &party_id, true\)[\s\S]+\.find\(&party_id\)[\s\S]+let leg_minutes/,
    "quest departure reloads the party after filling shared waterskins before writing camp state",
  );
  assert.match(
    strategic,
    /prepare_party_waterskins\([\s\S]+departing_settlement[\s\S]+party = Some\([\s\S]+\.find\(&current_party\.id\)/,
    "settlement departure reloads the party after preparing shared waterskins",
  );
});
