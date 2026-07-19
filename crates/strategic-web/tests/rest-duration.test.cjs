const assert = require("node:assert/strict");
const test = require("node:test");

const {
  formatClock,
  minutesUntilWake,
  targetForDuration,
} = require("../static/rest-duration.js");

test("08:00 wake preserves the full-day minimum at its boundaries", () => {
  assert.equal(minutesUntilWake(7 * 60 + 59, 8 * 60), 1441);
  assert.equal(minutesUntilWake(8 * 60, 8 * 60), 1440);
  assert.equal(minutesUntilWake(8 * 60 + 1, 8 * 60), 2879);
});

test("wake arithmetic preserves arbitrary current and target minutes", () => {
  assert.equal(minutesUntilWake(13 * 60 + 37, 6 * 60 + 12), 2435);
  assert.equal(formatClock(6 * 60 + 12), "06:12");
});

test("large typed hour values round to a minute and update target modulo day", () => {
  assert.equal(targetForDuration(7 * 60 + 59, 49.51), 9 * 60 + 30);
  assert.equal(targetForDuration(23 * 60 + 47, 24.016), 23 * 60 + 48);
});

test("markup keeps wake time settlement-only, accessible, and detached from days", () => {
  const source = require("node:fs").readFileSync("crates/strategic-web/src/templates/settlement.rs", "utf8");
  const settlementControl = source.slice(source.indexOf("fn settlement_rest_duration_control"));
  assert.match(settlementControl, /type="range"/);
  assert.match(settlementControl, /aria-label="Wake time"/);
  assert.match(settlementControl, /disabled\[!hours_active\]/);
  assert.doesNotMatch(source.slice(source.indexOf("pub fn camp_page"), source.indexOf("fn rest_duration_control")), /data-wake-time/);
});
