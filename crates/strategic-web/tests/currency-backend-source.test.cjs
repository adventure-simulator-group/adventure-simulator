const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");

const source = fs.readFileSync("crates/adventuresim-stdb-module/src/item.rs", "utf8");

test("currency issuance validates and uses the persisted settlement denomination", () => {
  assert.match(source, /currency_id_for_settlement[\s\S]*ctx\.db\.settlement\(\)\.id\(\)\.find/);
  assert.match(source, /CURRENCY_IDS\.contains\(&settlement\.currency_id\.as_str\(\)\)/);
  assert.match(source, /let currency_id = currency_id_for_settlement\(ctx, settlement_id\)\?/);
});

test("repeated personal currency credits merge their existing denomination stack", () => {
  assert.match(source, /character_and_item_id\(\)[\s\S]*filter\(\(character_id, &currency_id\)\)/);
  assert.match(source, /if let Some\(quantity\) = merged_currency_quantity\(stack\.quantity, amount\)/);
  assert.match(source, /existing\.checked_add\(credit\)/);
  assert.match(source, /inventory_item\(\)\.id\(\)\.update\(stack\)/);
});
