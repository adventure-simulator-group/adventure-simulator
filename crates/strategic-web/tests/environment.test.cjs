const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

const source = fs.readFileSync("crates/strategic-web/static/strategic-time.js", "utf8");
const buildingSource = fs.readFileSync("crates/strategic-web/static/building-state.js", "utf8");
const baseCss = fs.readFileSync("crates/strategic-web/static/css/base.css", "utf8");
const layoutCss = fs.readFileSync("crates/strategic-web/static/css/layout.css", "utf8");
const strategicCss = fs.readFileSync("crates/strategic-web/static/css/strategic.css", "utf8");
const window = {
  queueStrategicInitialLoad: () => new Promise(() => {}),
  strategicBackgroundFetch() {},
  reportStrategicError() {},
};
vm.runInNewContext(source, {
  window,
  document: { documentElement: { style: { setProperty() {} } }, querySelectorAll: () => [] },
  Promise,
});

test("sun and moon cross the sky from edge to edge with a high noon", () => {
  const at = (hour) => window.strategicTimeLighting(hour * 60);
  const dawn = at(6);
  const noon = at(12);
  const dusk = at(18);
  const midnight = at(0);
  const afternoon = at(14.2);
  assert.ok(dawn.glowX < 0);
  assert.equal(noon.glowX, 50);
  assert.ok(at(17.99).glowX > 100);
  assert.ok(dusk.glowX < 0);
  assert.equal(midnight.glowX, 50);
  assert.ok(noon.glowY < dawn.glowY);
  assert.ok(midnight.glowY < dusk.glowY);
  assert.ok(afternoon.glowX > 55 && afternoon.glowX < 70);
  assert.ok(afternoon.glowY < 25);
  for (const hour of [0, 12]) {
    const before = window.strategicTimeLighting(((hour * 60 - 1) + 1440) % 1440);
    const after = window.strategicTimeLighting((hour * 60 + 1) % 1440);
    assert.ok(Math.abs(before.glowX - after.glowX) < 1);
    assert.ok(Math.abs(before.glowY - after.glowY) < 1);
  }
});

test("daytime sky is bright while strategic surfaces stay building-derived", () => {
  const noon = window.strategicTimeLighting(12 * 60);
  const channels = noon.low.match(/\d+/g).map(Number);
  assert.ok(channels[0] >= 75 && channels[1] >= 150 && channels[2] >= 220);
  assert.match(layoutCss, /\.settlement-time[\s\S]*background: rgb\(5 8 13 \/ 76%\)/);
  assert.match(layoutCss, /\.settlement-services \{[\s\S]*align-items: flex-end/);
  assert.match(baseCss, /--building-interactive:color-mix/);
  assert.match(strategicCss, /\.trade-inventory-row \{[\s\S]*background: var\(--building-interactive\)/);
  assert.match(strategicCss, /\.main-grid \.btn:not\(\.btn-danger\)[\s\S]*background: var\(--building-interactive\)/);
});

test("building state is re-applied when live regions replace party links", () => {
  assert.match(buildingSource, /new MutationObserver/);
  assert.match(buildingSource, /mutation\.addedNodes/);
  assert.match(buildingSource, /syncPartyLinks\(node\)/);
});
