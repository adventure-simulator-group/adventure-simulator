const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const staticRoot = path.join(__dirname, "..");

test("travel planner constructs local accessible Game Icons without semantic emoji", () => {
  const source = fs.readFileSync(path.join(staticRoot, "static", "travel-planner.js"), "utf8");
  for (const icon of ["house", "camping-tent", "castle", "person"]) {
    assert.ok(source.includes(`gameIcon("${icon}")`) || source.includes(`icon: "${icon}"`));
    assert.ok(fs.existsSync(path.join(staticRoot, "static", "icons", "game", `${icon}.svg`)));
  }
  assert.doesNotMatch(source, /\u{1f3e0}|\u{26fa}|\u{1f3f0}|\u{1f9d1}/u);
  assert.match(source, /aria-label", "Traveling party"/);
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

test("travel planner renders return-track provisions and exact staged market quantities", () => {
  const planner = fs.readFileSync(path.join(staticRoot, "static", "travel-planner.js"), "utf8");
  const trade = fs.readFileSync(path.join(staticRoot, "static", "party-trade.js"), "utf8");
  assert.match(planner, /roundTrip \? minutes \* 2 : minutes/);
  assert.match(planner, /node\.minute \/ totalMinutes/);
  assert.match(planner, /day\$\{amount === "1"/);
  assert.match(planner, /Math\.ceil\(Math\.max\(0, \(journeyDays \+ target - foodDays\)/);
  assert.match(planner, /params\.set\("provision_rations"/);
  assert.match(planner, /params\.set\("provision_waterskins"/);
  assert.match(trade, /data-inventory-tab="party"/);
  assert.match(trade, /draft\.set\(itemId, quantity\)/);
});
