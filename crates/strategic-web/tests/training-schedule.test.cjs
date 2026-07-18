const assert = require("node:assert/strict");
const test = require("node:test");

const { parseClock } = require("../static/training-schedule.js");

test("schedule time parser accepts clock and whole-hour forms", () => {
  assert.equal(parseClock("8"), 8 * 60);
  assert.equal(parseClock("08"), 8 * 60);
  assert.equal(parseClock("8:30"), 8 * 60 + 30);
  assert.equal(parseClock("08:30"), 8 * 60 + 30);
});

test("schedule time parser accepts compact three- and four-digit forms", () => {
  assert.equal(parseClock("830"), 8 * 60 + 30);
  assert.equal(parseClock("0830"), 8 * 60 + 30);
  assert.equal(parseClock("1230"), 12 * 60 + 30);
});

test("schedule time parser retains day bounds and minute validation", () => {
  assert.equal(parseClock("24"), 24 * 60);
  assert.equal(parseClock("2400"), 24 * 60);
  for (const invalid of ["", "25", "24:15", "2415", "8:60", "860", "12345", "noon"]) {
    assert.equal(parseClock(invalid), null, invalid);
  }
});
