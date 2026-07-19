const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

const source = fs.readFileSync("crates/strategic-web/static/strategic-time.js", "utf8");
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

test("celestial path is continuous through dawn, noon, dusk, and midnight", () => {
  const at = (hour) => window.strategicTimeLighting(hour * 60);
  const dawn = at(6);
  const noon = at(12);
  const dusk = at(18);
  const midnight = at(0);
  assert.ok(noon.glowY < dawn.glowY);
  assert.ok(midnight.glowY < dusk.glowY);
  for (const hour of [0, 6, 18]) {
    const before = window.strategicTimeLighting(((hour * 60 - 1) + 1440) % 1440);
    const after = window.strategicTimeLighting((hour * 60 + 1) % 1440);
    assert.ok(Math.abs(before.glowX - after.glowX) < 1);
    assert.ok(Math.abs(before.glowY - after.glowY) < 1);
  }
});
