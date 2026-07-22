const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
const test = require("node:test");

const source = fs.readFileSync(path.join(__dirname, "../static/cooking.js"), "utf8");
const template = fs.readFileSync(path.join(__dirname, "../src/templates/settlement.rs"), "utf8");

test("cooking stages inventory rows into a bounded pot draft", () => {
  assert.match(source, /data-cooking-stage/);
  assert.match(source, /data-cooking-unstage/);
  assert.match(source, /const staged = new Map\(\)/);
  assert.match(source, /Math\.min\(entry\.available, entry\.quantity \+ 1\)/);
  assert.match(source, /\.join\(","\)/);
});

test("cook exposes the shared duration formula to hover and accessibility", () => {
  assert.match(source, /Math\.sqrt\(Math\.max\(0, mass - 0\.5\)\) \* 8/);
  assert.match(source, /submit\.title = reason/);
  assert.match(source, /aria-label/);
  assert.match(source, /strategic-live-regions-refreshed/);
});

test("cooking methods submit through the valid center form", () => {
  assert.match(template, /form id="cooking-submit-form" class="cooking-submit-form"/);
  assert.match(template, /name="method" value=\(value\) form="cooking-submit-form"/);
});
