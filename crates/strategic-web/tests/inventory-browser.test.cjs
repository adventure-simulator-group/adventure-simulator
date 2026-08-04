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

test("container browsing preserves counterpart state and accessible fallbacks", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/inventory-browser.js"), "utf8");
  assert.match(source, /data-container-open/);
  assert.match(source, /data-container-close/);
  assert.match(source, /_containerPanelStack/);
  assert.match(source, /snapshot\.active\?\.focus/);
  assert.match(source, /inventory-container-move/);
  assert.match(source, /application\/x-adventuresim-inventory-object/);
  assert.match(source, /Container capacity exceeded|container-capacity/);
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

test("alcohol rows use one aggregate parent and preserve concrete component actions", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/inventory-browser.js"), "utf8");
  assert.match(source, /groupAlcoholRows\(browser\)/);
  assert.match(source, /alcohol-parent-row/);
  assert.match(source, /alcohol-component-row/);
  assert.match(source, /data-alcohol-toggle/);
  assert.match(source, /row\._alcoholComponents/);
  assert.match(source, /not\(\.alcohol-component-row\)/);
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

test("provisioning explicitly selects party scope and reveals the staged food row", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/party-trade.js"), "utf8");
  assert.match(source, /selectMerchantInventoryScope\(partyTab\)/);
  assert.match(source, /scope\.value = tab\.dataset\.inventoryTab/);
  assert.match(source, /itemId === "travel_ration"/);
  assert.match(source, /querySelector\(":scope > \.food-parent-row"\)/);
  assert.match(source, /querySelector\("\[data-food-toggle\]"\)\?\.click\(\)/);
  assert.doesNotMatch(source, /partyTab\.click\(\)/);
});

test("live sidebar replacement remeasures connected inventory rails", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/party-trade.js"), "utf8");
  assert.match(source, /"strategic-live-regions-refreshed", \(\) => refreshInventoryPanel\(document\)/);
  assert.match(source, /if \(!hasVisibleBrowser\) grid\?\.style\.removeProperty/);
});

test("trade offer dialogs focus on open, contain focus, dismiss, and restore their opener", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/party-trade.js"), "utf8");
  assert.match(source, /function openFauxDialog/);
  assert.match(source, /function closeFauxDialog/);
  assert.match(source, /fauxDialogOpeners/);
  assert.match(source, /event\.key === "Escape"/);
  assert.match(source, /event\.key === "Tab"/);
  assert.match(source, /opener\?\.isConnected/);
  for (const id of [
    "party-offer", "loot-transfer-offer", "pool-transfer-offer", "merchant-offer", "inventory-discard",
  ]) assert.match(source, new RegExp(`\"${id}\"`));
  assert.match(source, /document\.body\.classList\.add\("faux-dialog-open"\)/);
  assert.match(source, /document\.body\.classList\.remove\("faux-dialog-open"\)/);
  assert.match(source, /strategic-page-unmounting/);
  assert.doesNotMatch(source, /form\.hidden = !hasDraft/);
  assert.doesNotMatch(
    source,
    /addEventListener\("submit"[\s\S]*?classList\.remove\("faux-dialog-open"\)/,
  );
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

test("food lots use a disclosure parent without becoming fungible", () => {
  assert.equal(typeof require("../static/inventory-browser.js").groupFoodRows, "function");
  const source = fs.readFileSync(path.join(__dirname, "../static/inventory-browser.js"), "utf8");
  assert.match(source, /data-item-kind=\\?"food/);
  assert.match(source, /data-food-lot=\\?"true/);
  assert.match(source, /food-component-row/);
  assert.match(source, /Show food lots/);
});

test("containers hydrate authoritative trees and wire accessible mutations", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/inventory-browser.js"), "utf8");
  assert.match(source, /\/api\/inventory\/containers/);
  assert.match(source, /data-container-parent-object-id/);
  assert.match(source, /inventory-container-move/);
  assert.match(source, /data-container-close/);
  assert.match(source, /data-container-remove/);
  assert.match(source, /data-container-pour/);
  assert.match(source, /data-container-drain/);
  assert.match(source, /application\/x-adventuresim-inventory-object/);
});

test("container hydration is race-safe and empty-stack opening is read-only", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/inventory-browser.js"), "utf8");
  assert.match(source, /containerHydrationGenerations = new WeakMap/);
  assert.match(source, /generation !== containerHydrationGenerations\.get\(root\)/);
  assert.doesNotMatch(source, /\/api\/inventory\/containers\/open/);
  assert.match(source, /parentLegacy/);
  assert.match(source, /parent_scope/);
  assert.match(source, /data-container-toggle/);
  assert.match(source, /\/api\/inventory\/containers\/remove/);
  assert.match(source, /authoritativeContainerSnapshot\.presentations/);
  assert.match(source, /inventory-container-snapshot-row/);
  assert.match(source, /inventoryCounterpart/);
  assert.match(source, /previousId/);
  assert.match(source, /data-container-move-into/);
});

test("nested container panels preserve live nodes and row drops do not bubble", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/inventory-browser.js"), "utf8");
  assert.match(source, /rows: panelRows/);
  assert.match(source, /replaceChildren\(\.\.\.snapshot\.rows\)/);
  assert.doesNotMatch(source, /tbodyHtml/);
  assert.match(source, /event\.stopPropagation\(\)/);
});

test("container decoration requires an owned or authoritative object row", () => {
  const source = fs.readFileSync(path.join(__dirname, "../static/inventory-browser.js"), "utf8");
  assert.match(source, /!row\.dataset\.containerObjectId && !rowInventoryKey\(row\)/);
});
