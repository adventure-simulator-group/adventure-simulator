const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const {
  parsePanelState,
  serializePanelState,
  compareValues,
  normalizeSortValue,
  rowValue,
  refresh,
  syncPanelWidth,
} = require("../static/inventory-browser.js");

test("panel state is independently namespaced", () => {
  const search = "?inv.left.q=sword&inv.left.sort=weight&inv.left.dir=desc&inv.left.cols=reach,damage&inv.right.q=mail";
  assert.deepEqual(parsePanelState(search, "left", ["reach", "damage"]), {
    query: "sword", sort: "weight", direction: "desc", columns: ["reach", "damage"],
  });
  assert.equal(parsePanelState(search, "right", []).query, "mail");
});

test("advertised sort types retain text damage and numeric target or durability values", () => {
  assert.equal(normalizeSortValue("Slash, Pierce", "text"), "Slash, Pierce");
  assert.equal(normalizeSortValue("12", "number"), 12);
  assert.equal(normalizeSortValue("0.76", "number"), 0.76);
  assert.equal(normalizeSortValue("—", "number"), "");
  const label = { dataset: { itemName: "Sword", statDamage: "Slash, Pierce" }, textContent: "Sword" };
  const durabilityBar = { dataset: { sortValue: "0.76" } };
  const durabilityCell = { dataset: {}, textContent: "Damaged", querySelector: () => durabilityBar };
  const row = { querySelector: (selector) => ({
    "[data-item-name]": label,
    ".inventory-target": { textContent: "12" },
    ".inventory-durability": durabilityCell,
  })[selector] || null };
  assert.equal(rowValue(row, "damage"), "Slash, Pierce");
  assert.equal(rowValue(row, "target"), 12);
  assert.equal(rowValue(row, "durability"), 0.76);
});

test("destination refresh is exposed for generated row insertion", () => {
  assert.equal(typeof refresh, "function");
  assert.equal(typeof syncPanelWidth, "function");
});

test("currency rows use one aggregate parent and dedicated denomination components", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/inventory-browser.js"), "utf8");
  assert.match(source, /groupCurrencyRows\(browser\)/);
  assert.match(source, /currency-parent-row/);
  assert.match(source, /currency-component-row/);
  assert.match(source, /data-coin-toggle/);
  assert.match(source, /row\._currencyComponents/);
  assert.match(source, /not\(\.currency-component-row\)/);
  assert.doesNotMatch(source, /No additional details[\s\S]*currency-component-row/);
});

test("rail measurement excludes projected action overflow", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/inventory-browser.js"), "utf8");
  assert.match(source, /table\?\.getBoundingClientRect\?\.\(\)\.width/);
  assert.match(source, /browser\.getBoundingClientRect\?\.\(\)\.width/);
  assert.doesNotMatch(source, /Math\.max\(browser\.scrollWidth, tableWidth\)/);
});

test("bulk controls mount inside a semantic header cell", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/party-trade.js"), "utf8");
  assert.match(source, /querySelector\("\[data-inventory-browser\] \.inventory-actions-header"\)/);
  assert.match(source, /headerCell\.append\(actions\)/);
  assert.doesNotMatch(source, /headerRow\.append\(actions\)/);
});

test("row controls mount in center-facing action cells", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/party-trade.js"), "utf8");
  assert.match(source, /createElement\("td"\)/);
  assert.match(source, /cell\.className = "inventory-actions-cell"/);
  assert.match(source, /row\[placeAtStart \? "prepend" : "append"\]\(cell\)/);
});

test("dynamic transfer routing survives glyph replacement", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/party-trade.js"), "utf8");
  assert.match(source, /const dynamicTransfer = event\.target\.closest\?\.\("\[data-dynamic-transfer\]"\)/);
  assert.match(source, /const clickTarget = dynamicTransfer \|\| event\.target/);
  assert.match(source, /const merchantButton = clickTarget\.closest\("\[data-merchant-buy\]"\)/);
  assert.doesNotMatch(source, /const merchantButton = event\.target\.closest/);
});

test("live sidebar replacement remeasures connected inventory rails", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/party-trade.js"), "utf8");
  assert.match(source, /"strategic-live-regions-refreshed", \(\) => refreshInventoryPanel\(document\)/);
  assert.match(source, /if \(!hasVisibleBrowser\) grid\?\.style\.removeProperty/);
});

test("serialization preserves unrelated params and round trips bookmarks", () => {
  const state = { query: "axe", sort: "value", direction: "desc", columns: ["block"] };
  const search = serializePanelState("?tab=party", "merchant-left", state);
  assert.match(search, /tab=party/);
  assert.deepEqual(parsePanelState(search, "merchant-left", ["block"]), state);
});

test("invalid and stale parameters are safely ignored", () => {
  assert.deepEqual(parsePanelState("?inv.x.sort=wat&inv.x.dir=sideways&inv.x.cols=reach,obsolete", "x", ["reach"]), {
    query: "", sort: "name", direction: "asc", columns: ["reach"],
  });
});

test("numeric and text sorting is directional and keeps blanks stable at the end", () => {
  assert.ok(compareValues(2, 10, "asc") < 0);
  assert.ok(compareValues("Sword 2", "sword 10", "asc") < 0);
  assert.ok(compareValues(2, 10, "desc") > 0);
  assert.ok(compareValues("", 2, "desc") > 0);
  assert.equal(compareValues("", "", "asc"), 0);
});
