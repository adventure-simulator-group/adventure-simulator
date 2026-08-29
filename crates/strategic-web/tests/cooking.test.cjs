const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
const test = require("node:test");
const { readRustModuleSource } = require("./rust-module-source.cjs");

const source = fs.readFileSync(path.join(__dirname, "../static/cooking.js"), "utf8");
const inventoryBrowserSource = fs.readFileSync(path.join(__dirname, "../static/inventory-browser.js"), "utf8");
const template = readRustModuleSource(path.join(__dirname, "../src/templates/settlement/mod.rs"));
const dialogue = fs.readFileSync(path.join(__dirname, "../static/dialogue-client.js"), "utf8");

test("cooking stages inventory rows into a bounded pot draft", () => {
  assert.match(source, /data-cooking-stage/);
  assert.match(source, /data-cooking-unstage/);
  assert.match(source, /const staged = new Map\(\)/);
  assert.match(source, /Math\.min\(entry\.available, entry\.quantity \+ amountStep\)/);
  assert.match(source, /\.join\(","\)/);
});

test("preview explains quality inputs without claiming stew is discarded", () => {
  assert.match(source, /bake: 30/);
  assert.match(source, /15% calories lost to drippings/);
  assert.match(source, /below 2% of ingredient mass: quality drops one tier/);
  assert.doesNotMatch(source, /eaten now; leftovers are discarded/);
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

test("fireplace submission is explicit and irreversible", () => {
  assert.match(template, /Start spit roast/);
  assert.match(template, /Loose food selected here is immediately consolidated/);
  assert.match(template, /any number of placed vessels/);
  assert.match(template, /inventory_scope/);
  assert.match(template, /personal/);
  assert.match(template, /party/);
  assert.match(template, /Rest while cooking/);
  assert.match(template, /name="unit" value="minutes"/);
});

test("fireplace inventory exposes food and instrument exchanges", () => {
  assert.match(template, /fireplace_inventory_row/);
  assert.match(template, /cooking_pan/);
  assert.match(template, /cooking_pot/);
  assert.match(template, /portable_oven/);
  assert.match(inventoryBrowserSource, /browser\.dataset\.inventoryBrowser === "cooking-inventory-right"/);
});

test("environmental fireplace survives dynamic NPC loading", () => {
  assert.match(dialogue, /querySelectorAll\("\[data-location-fixture\]"\)/);
  assert.match(dialogue, /replaceChildren\(\.\.\.buttons, \.\.\.locationFixtures\)/);
  assert.match(template, /data-cooking-activity\[dish\.is_none\(\)\]/);
});
