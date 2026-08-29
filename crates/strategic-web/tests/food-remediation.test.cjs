const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
const test = require("node:test");
const { readRustModuleSource } = require("./rust-module-source.cjs");

const root = path.join(__dirname, "../..");
const food = readRustModuleSource(path.join(root, "adventuresim-stdb-module/src/food/mod.rs"));
const item = fs.readFileSync(path.join(root, "adventuresim-stdb-module/src/item.rs"), "utf8");
const strategic = readRustModuleSource(path.join(root, "adventuresim-stdb-module/src/strategic/mod.rs"));
const capability = fs.readFileSync(path.join(root, "adventuresim-stdb-module/src/capability.rs"), "utf8");
const template = [
  "character_details.rs",
  "character_skills.rs",
  "trade.rs",
].map((file) => fs.readFileSync(path.join(root, "strategic-web/src/templates/settlement", file), "utf8")).join("\n");
const inventoryBrowser = fs.readFileSync(path.join(root, "strategic-web/static/inventory-browser.js"), "utf8");

test("food acquisitions remain independent lots instead of merchant-merged stacks", () => {
  assert.match(item, /fn requires_stable_object[\s\S]*food \|\| measured[\s\S]*ItemKind::Medication/);
  assert.match(item, /individual = requires_stable_object\(definition\.as_ref\(\), food, measured\)/);
  assert.match(strategic, /if !durable\s*&& !food\s*&& let Some\(mut stack\)/);
  assert.match(strategic, /for _ in 0\.\.quantity \{[\s\S]*quantity: 1[\s\S]*create_party_food_lot\(ctx, row\.id, item_id, 1, minute\)/);
});

test("food reducers require the registered gateway and reject tactical actors", () => {
  const reducerCount = (food.match(/#\[reducer\]/g) || []).length;
  const gatewayCount = (food.match(/crate::strategic::require_strategic_gateway\(ctx\)\?/g) || []).length;
  assert.equal(gatewayCount, reducerCount);
  assert.match(strategic, /pub\(crate\) fn require_strategic_gateway[\s\S]*authority\.identity != ctx\.sender\(\)/);
  assert.match(food, /Eating is unavailable during a tactical encounter/);
  assert.match(food, /Cooking is unavailable during a tactical encounter/);
});

test("fireplace submission consolidates immediately without advancing time", () => {
  const cook = food.match(/pub fn add_fireplace_ingredients[\s\S]*?pub fn retrieve_fireplace_dish/)?.[0];
  assert.ok(cook, "fireplace ingredient reducer boundary");
  assert.doesNotMatch(cook, /advance_character_wait_time/);
  assert.match(cook, /Everything above is preflight/);
  assert.match(cook, /fireplace_dish\(\)\.insert/);
  assert.match(cook, /inventory_scope\.as_str\(\)/);
});

test("food mass, value, and provenance are lot-authoritative", () => {
  assert.match(food, /ingredient_quantities: Vec<f32>/);
  assert.match(food, /retain_lot_fraction\(&mut lot, retained\)/);
  assert.match(strategic, /Food batches must be sold as complete valid lots/);
  assert.match(capability, /food_lot\(\)[\s\S]*lot\.mass_kg\.max\(0\.0\)/);
  assert.match(food, /mass \+= water_ml \/ 1_000\.0/);
  assert.doesNotMatch(food, /Soup cannot be carried/);
  assert.match(template, /merchant_inventory_sell_price\(definition, food_lot\)/);
  assert.match(template, /merchant_inventory_weight\(definition, food_lot\)/);
});

test("fireplace navigation replaces the cooking skill modal", () => {
  assert.match(template, /let cooking_href: Option<String> = None/);
  assert.match(template, /Cooking is informational/);
  assert.doesNotMatch(template, /\?cook=true/);
  assert.doesNotMatch(template, /cooking-dialog-title/);
  assert.match(template, /Start spit roast/);
  assert.match(template, /fireplace_inventory_row/);
  assert.match(template, /data-food-lot=\[adventuresim_core::food::definition/);
  assert.match(inventoryBrowser, /data-food-lot="true"/);
  assert.doesNotMatch(inventoryBrowser, /â|Ã|�/);
});
