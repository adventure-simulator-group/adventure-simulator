const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
const test = require("node:test");

const source = fs.readFileSync(path.join(__dirname, "../static/cooking.js"), "utf8");
const inventoryBrowserSource = fs.readFileSync(path.join(__dirname, "../static/inventory-browser.js"), "utf8");
const template = fs.readFileSync(path.join(__dirname, "../src/templates/settlement/trade.rs"), "utf8");

test("cooking stages inventory rows into a bounded pot draft", () => {
  assert.match(source, /data-cooking-stage/);
  assert.match(source, /data-cooking-unstage/);
  assert.match(source, /const staged = new Map\(\)/);
  assert.match(source, /Math\.min\(entry\.available, entry\.quantity \+ amountStep\)/);
  assert.match(source, /\.join\(","\)/);
});

test("preview explains retained-food exceptions and quality inputs", () => {
  assert.match(source, /bake: 30/);
  assert.match(source, /15% calories lost to drippings/);
  assert.match(source, /below 2% of ingredient mass: quality drops one tier/);
  assert.match(source, /eaten now; leftovers are discarded/);
  assert.match(source, /stewWaterMl/);
  assert.match(source, /kg water included in flavor mass/);
  assert.match(source, /culinaryFatMass < mass \* panFatRatio/);
  assert.match(source, /flavor score/);
  assert.match(template, /data-cooking-preview/);
  assert.match(template, /data-culinary-fat/);
  assert.match(template, /data-pan-fat-ratio/);
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

test("cooking inventory shows only food and expands its food group initially", () => {
  assert.match(template, /let ingredients = inventory/);
  assert.match(template, /@for item in ingredients/);
  assert.match(inventoryBrowserSource, /browser\.dataset\.inventoryBrowser === "cooking-inventory-right"/);
});
