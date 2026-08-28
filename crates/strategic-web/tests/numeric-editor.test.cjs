const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const { stepNumericValue } = require("../static/numeric-editor.js");
const strategicCalendar = require("./strategic-calendar-fixture.cjs");

const MAX_TARGET_SURPLUS_DAYS = strategicCalendar.daysPerYear;

const decimalOptions = {
  parse(value) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  },
  format(value) { return String(Number(value.toFixed(2))); },
  step: 0.25,
  minimum: -MAX_TARGET_SURPLUS_DAYS,
  maximum: MAX_TARGET_SURPLUS_DAYS,
  initialValue: 0,
};

test("shared numeric editor steps, clamps, and recovers invalid drafts", () => {
  assert.equal(stepNumericValue("1", 1, decimalOptions), "1.25");
  assert.equal(
    stepNumericValue(String(-MAX_TARGET_SURPLUS_DAYS), -1, decimalOptions),
    String(-MAX_TARGET_SURPLUS_DAYS),
  );
  assert.equal(
    stepNumericValue(String(MAX_TARGET_SURPLUS_DAYS), 1, decimalOptions),
    String(MAX_TARGET_SURPLUS_DAYS),
  );
  assert.equal(stepNumericValue("invalid", -1, decimalOptions), "-0.25");
});

test("skill allocation, provisioning, and inventory targets use the shared numeric editor", () => {
  const root = path.join(__dirname, "..", "static");
  const schedule = fs.readFileSync(path.join(root, "training-schedule.js"), "utf8");
  const travel = fs.readFileSync(path.join(root, "travel-planner.js"), "utf8");
  const trade = fs.readFileSync(path.join(root, "party-trade.js"), "utf8");
  assert.match(schedule, /StrategicNumericEditor\.open/);
  assert.match(travel, /StrategicNumericEditor\.open/);
  assert.match(trade, /StrategicNumericEditor\.open/);
  assert.match(travel, /step: \.25/);
  assert.match(travel, /minimum: -MAX_TARGET_SURPLUS_DAYS/);
  assert.match(trade, /step: 1/);
  assert.match(trade, /maximum: 4294967295/);
});

test("shared numeric editor keeps its trigger in layout while positioning", () => {
  const source = fs.readFileSync(path.join(__dirname, "..", "static", "numeric-editor.js"), "utf8");
  assert.match(source, /display\.style\.visibility = 'hidden'/);
  assert.doesNotMatch(source, /display\.hidden = true/);
});
