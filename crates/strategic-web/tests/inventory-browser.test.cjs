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
});

test("bulk controls mount inside a semantic header cell", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/party-trade.js"), "utf8");
  assert.match(source, /createElement\("th"\)/);
  assert.match(source, /cell\.append\(actions\)/);
  assert.doesNotMatch(source, /headerRow\.append\(actions\)/);
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
