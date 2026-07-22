const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
const test = require("node:test");

const root = path.join(__dirname, "../..");
const food = fs.readFileSync(path.join(root, "adventuresim-stdb-module/src/food.rs"), "utf8");
const item = fs.readFileSync(path.join(root, "adventuresim-stdb-module/src/item.rs"), "utf8");
const strategic = fs.readFileSync(path.join(root, "adventuresim-stdb-module/src/strategic.rs"), "utf8");
const capability = fs.readFileSync(path.join(root, "adventuresim-stdb-module/src/capability.rs"), "utf8");
const template = fs.readFileSync(path.join(root, "strategic-web/src/templates/settlement.rs"), "utf8");
const inventoryBrowser = fs.readFileSync(path.join(root, "strategic-web/static/inventory-browser.js"), "utf8");

test("food acquisitions remain independent lots instead of merchant-merged stacks", () => {
  assert.match(item, /individual = durable \|\| kind == Some\(ItemKind::Medication\) \|\| food/);
  assert.match(strategic, /if !durable\s*&& !food\s*&& let Some\(mut stack\)/);
  assert.match(strategic, /for _ in 0\.\.quantity \{[\s\S]*quantity: 1[\s\S]*create_party_food_lot\(ctx, row\.id, item_id, 1, minute\)/);
});

test("food reducers reject tactical actors and interrupted cooking rolls back", () => {
  assert.match(food, /Eating is unavailable during a tactical encounter/);
  assert.match(food, /Cooking is unavailable during a tactical encounter/);
  assert.match(food, /if !crate::time::advance_character_wait_time[\s\S]*return Err\("Cooking was interrupted/);
});

test("food mass, value, and provenance are lot-authoritative", () => {
  assert.match(food, /ingredient_quantities: Vec<f32>/);
  assert.match(food, /retain_lot_fraction\(&mut lot, 1\.0 - ratio\)/);
  assert.match(strategic, /Food batches must be sold as complete valid lots/);
  assert.match(capability, /food_lot\(\)[\s\S]*lot\.mass_kg\.max\(0\.0\)/);
});

test("character activity navigation exposes cooking and all edible lots aggregate", () => {
  assert.match(template, /data-character-activity="cooking"[\s\S]*\?activity=cooking/);
  assert.match(template, /data-food-lot=\[adventuresim_core::food::definition/);
  assert.match(inventoryBrowser, /data-food-lot="true"/);
});
