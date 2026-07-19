const test = require("node:test");
const assert = require("node:assert/strict");
const {
  parsePanelState,
  serializePanelState,
  compareValues,
} = require("../static/inventory-browser.js");

test("panel state is independently namespaced", () => {
  const search = "?inv.left.q=sword&inv.left.sort=weight&inv.left.dir=desc&inv.left.cols=reach,damage&inv.right.q=mail";
  assert.deepEqual(parsePanelState(search, "left", ["reach", "damage"]), {
    query: "sword", sort: "weight", direction: "desc", columns: ["reach", "damage"],
  });
  assert.equal(parsePanelState(search, "right", []).query, "mail");
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
