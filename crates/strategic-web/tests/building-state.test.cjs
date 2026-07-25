const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const source = fs.readFileSync(path.join(__dirname, "..", "static", "building-state.js"), "utf8");

test("building state preserves non-service settlement locations", () => {
  assert.match(source, /"", "residences", "keep", "map"/);
  assert.match(source, /if \(building\) url\.searchParams\.set\("building", building\)/);
  assert.match(source, /else url\.searchParams\.delete\("building"\)/);
});
