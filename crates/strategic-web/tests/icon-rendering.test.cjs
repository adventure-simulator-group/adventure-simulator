const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const staticRoot = path.join(__dirname, "..");

test("travel planner uses accessible icon-free route markers", () => {
  const source = fs.readFileSync(path.join(staticRoot, "static", "travel-planner.js"), "utf8");
  assert.doesNotMatch(source, /gameIcon|camping-tent|travel-party-pin/);
  assert.doesNotMatch(source, /\u{1f3e0}|\u{26fa}|\u{1f3f0}|\u{1f9d1}/u);
  assert.match(source, /displayLabel: "Start"/);
  assert.match(source, /displayLabel: roundTrip \? "Quest" : "End"/);
  assert.match(source, /element\.setAttribute\("aria-label", node\.label\)/);
  assert.doesNotMatch(source, /element\.innerHTML/);
});

test("party notifications use safe local icons for decisions and leadership", () => {
  const source = fs.readFileSync(path.join(staticRoot, "static", "party-notifications.js"), "utf8");
  for (const icon of ["check-mark", "cross-mark", "crown"]) {
    assert.ok(source.includes(`"${icon}"`));
    assert.ok(fs.existsSync(path.join(staticRoot, "static", "icons", "game", `${icon}.svg`)));
  }
  assert.doesNotMatch(source, /\u{2713}|\u{265b}/u);
  assert.doesNotMatch(source, /\.innerHTML\s*=/);
  assert.match(source, /setAttribute\("aria-label", label\)/);
});

test("travel planner renders journey provisions and exact staged market quantities", () => {
  const planner = fs.readFileSync(path.join(staticRoot, "static", "travel-planner.js"), "utf8");
  const trade = fs.readFileSync(path.join(staticRoot, "static", "party-trade.js"), "utf8");
  assert.match(planner, /roundTrip \? minutes \* 2 : minutes/);
  assert.match(planner, /node\.minute \/ totalMinutes/);
  assert.match(planner, /VERTICAL_PATH_START/);
  assert.match(planner, /VERTICAL_PATH_END/);
  assert.match(planner, /VERTICAL_PATH_END - VERTICAL_PATH_START/);
  assert.doesNotMatch(planner, /strokeDasharray/);
  assert.doesNotMatch(planner, /RETURN_PATH/);
  assert.match(planner, /const vertical = 5 \+ progress \* 90/);
  assert.match(planner, /journeyTurnaroundMinutes/);
  assert.match(planner, /setPathRange\(planner\.querySelector\("\[data-travel-progress\]"\), 0, progressPercent\)/);
  assert.match(planner, /Math\.ceil\(Math\.max\(0, \(remainingDays \+ target - foodDays\)/);
  assert.match(planner, /params\.set\("provision_rations"/);
  assert.match(planner, /params\.set\("provision_waterskins"/);
  assert.match(trade, /data-inventory-tab="party"/);
  assert.match(trade, /draft\.set\(itemId, quantity\)/);
  assert.doesNotMatch(planner, /Math\.min\(10000/);
  assert.doesNotMatch(trade, /quantity\) <= 10000/);
  assert.match(trade, /quantity <= 4294967295/);
});

test("travel provisioning keeps target math without forecast prose", () => {
  const planner = fs.readFileSync(path.join(staticRoot, "static", "travel-planner.js"), "utf8");
  const css = fs.readFileSync(path.join(staticRoot, "static", "css", "strategic.css"), "utf8");
  const template = fs.readFileSync(path.join(staticRoot, "src", "templates", "settlement.rs"), "utf8");
  assert.match(planner, /target < 0 \? "negative" : target > 0 \? "positive" : "zero"/);
  assert.doesNotMatch(planner, /Target:/);
  assert.doesNotMatch(planner, /shortfall/);
  assert.doesNotMatch(template, /data-resource-target-label/);
  assert.doesNotMatch(template, /data-resource-surplus/);
  assert.match(template, /game_icon\("Food", "meal"\)/);
  assert.match(template, /game_icon\("Water", "water-drop"\)/);
  assert.match(template, /"days surplus"/);
  assert.match(css, /target-sign="negative"/);
  assert.match(css, /\.travel-resource-meters[^}]+display: flex; gap: 0/);
  assert.match(css, /\.travel-progress-path[^}]+stroke: #fff/);
  assert.doesNotMatch(css, /travel-party-pin|rotate\(-90deg\)/);
  assert.doesNotMatch(css, /travel-resource-path\.target[^}]*stroke-dasharray/);
  assert.match(template, /@if selected\.is_some\(\) \{\s+p class="text-muted small-copy" data-provisioning-status/);
  assert.doesNotMatch(planner, /travel-plan-label/);
  assert.match(template, /data-travel-progress/);
  assert.match(template, /data-journey-turnaround-minutes/);
  assert.match(template, /"travel-planner-vertical no-destination"/);
  assert.doesNotMatch(template, /Break camp to travel the next planned leg|The whole party rests/);
  assert.match(css, /\.camp-journey-section[^}]+flex: 1 1 auto/);
});

test("merchant provisioning initializes only once the Party tab DOM exists", () => {
  const trade = fs.readFileSync(path.join(staticRoot, "static", "party-trade.js"), "utf8");
  const layout = fs.readFileSync(path.join(staticRoot, "src", "templates", "layout.rs"), "utf8");
  assert.match(layout, /party-trade\.js[^\n]+defer/);
  assert.match(trade, /DOMContentLoaded", initializeProvisioningDraft, \{ once: true \}/);
  assert.match(trade, /partyTab\.click\(\)/);
  assert.match(trade, /\["travel_ration", parseQuantity\("provision_rations"\)\]/);
  assert.match(trade, /\["waterskin", parseQuantity\("provision_waterskins"\)\]/);
});
