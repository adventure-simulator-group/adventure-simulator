const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
const test = require("node:test");

const source = fs.readFileSync(path.join(__dirname, "../static/cooking.js"), "utf8");

test("cooking stages lot IDs and bounded quantities", () => {
  assert.match(source, /data-cooking-lot/);
  assert.match(source, /Math\.max\(1, Math\.min/);
  assert.match(source, /\.join\(","\)/);
});

test("cook exposes the shared duration formula to hover and accessibility", () => {
  assert.match(source, /Math\.sqrt\(Math\.max\(0, mass - 0\.5\)\) \* 8/);
  assert.match(source, /submit\.title = reason/);
  assert.match(source, /aria-label/);
  assert.match(source, /strategic-live-regions-refreshed/);
});
