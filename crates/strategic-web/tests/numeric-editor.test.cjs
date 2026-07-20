const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const { stepNumericValue } = require("../static/numeric-editor.js");

const decimalOptions = {
  parse(value) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  },
  format(value) { return String(Number(value.toFixed(2))); },
  step: 0.25,
  minimum: -365,
  maximum: 365,
  initialValue: 0,
};

test("shared numeric editor steps, clamps, and recovers invalid drafts", () => {
  assert.equal(stepNumericValue("1", 1, decimalOptions), "1.25");
  assert.equal(stepNumericValue("-365", -1, decimalOptions), "-365");
  assert.equal(stepNumericValue("365", 1, decimalOptions), "365");
  assert.equal(stepNumericValue("invalid", -1, decimalOptions), "-0.25");
});

test("skill allocation and provisioning both use the shared numeric editor", () => {
  const root = path.join(__dirname, "..", "static");
  const schedule = fs.readFileSync(path.join(root, "training-schedule.js"), "utf8");
  const travel = fs.readFileSync(path.join(root, "travel-planner.js"), "utf8");
  assert.match(schedule, /StrategicNumericEditor\.open/);
  assert.match(travel, /StrategicNumericEditor\.open/);
  assert.match(travel, /step: \.25/);
  assert.match(travel, /minimum: -365/);
});

test("shared numeric editor keeps its trigger in layout while positioning", () => {
  const source = fs.readFileSync(path.join(__dirname, "..", "static", "numeric-editor.js"), "utf8");
  assert.match(source, /display\.style\.visibility = 'hidden'/);
  assert.doesNotMatch(source, /display\.hidden = true/);
});
