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

test("food reducers require the registered gateway and reject tactical actors", () => {
  assert.equal((food.match(/crate::strategic::require_strategic_gateway\(ctx\)\?/g) || []).length, 2);
  assert.match(strategic, /pub\(crate\) fn require_strategic_gateway[\s\S]*authority\.identity != ctx\.sender\(\)/);
  assert.match(food, /Eating is unavailable during a tactical encounter/);
  assert.match(food, /Cooking is unavailable during a tactical encounter/);
});

test("interrupted cooking commits its safe time prefix without consuming inputs", () => {
  const cook = food.match(/pub fn cook_food[\s\S]*?\n}\n\n#\[cfg\(test\)\]/)?.[0];
  assert.ok(cook, "cook reducer boundary");
  const wait = cook.indexOf("advance_character_wait_time");
  const water = cook.indexOf("party.pooled_water_ml -=");
  const ingredients = cook.indexOf("delete_personal_food_lot");
  assert.ok(wait >= 0 && wait < water && wait < ingredients);
  assert.match(cook, /if !crate::time::advance_character_wait_time[\s\S]*return Ok\(\(\)\)/);
});

test("food mass, value, and provenance are lot-authoritative", () => {
  assert.match(food, /ingredient_quantities: Vec<f32>/);
  assert.match(food, /retain_lot_fraction\(&mut lot, 1\.0 - ratio\)/);
  assert.match(strategic, /Food batches must be sold as complete valid lots/);
  assert.match(capability, /food_lot\(\)[\s\S]*lot\.mass_kg\.max\(0\.0\)/);
  assert.match(food, /consume_food_amount\(ctx, character_id, output\.id[\s\S]*refresh_character_capability\(ctx, character_id\)/);
  assert.match(template, /merchant_inventory_sell_price\(definition, food_lot\)/);
  assert.match(template, /merchant_inventory_weight\(definition, food_lot\)/);
});

test("portrait navigation exposes cooking and all edible lots aggregate", () => {
  assert.match(template, /party-cooking-action/);
  assert.match(template, /\?cook=true/);
  assert.doesNotMatch(template, /data-character-activity="cooking"/);
  assert.match(template, /class="cooking-method-list"/);
  assert.match(template, /cooking_method\("pan-fry", "Pan-fry", "meal"/);
  assert.match(template, /data-food-lot=\[adventuresim_core::food::definition/);
  assert.match(inventoryBrowser, /data-food-lot="true"/);
  assert.doesNotMatch(inventoryBrowser, /â|Ã|�/);
});
