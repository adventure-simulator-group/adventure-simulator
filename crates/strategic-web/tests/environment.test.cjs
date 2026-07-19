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
  assert.match(layoutCss, /\.settlement-time[\s\S]*background: rgb\(5 8 13 \/ 38%\)/);
  assert.match(layoutCss, /\.settlement-services \{[\s\S]*align-items: flex-end/);
  assert.match(baseCss, /--building-interactive:color-mix/);
  assert.match(strategicCss, /\.trade-inventory-row \{[\s\S]*background: var\(--building-interactive\)/);
  assert.match(strategicCss, /\.main-grid \.btn:not\(\.btn-danger\)[\s\S]*background: var\(--building-interactive\)/);
});

test("building tabs have roofs without making the desktop header scroll", () => {
  assert.match(baseCss, /--settlement-header-height:112px/);
  assert.match(layoutCss, /body:has\(\.settlement-top-bar\) \.main-grid \{[\s\S]*var\(--settlement-header-height\)/);
  assert.match(layoutCss, /data-environment="settlement"[\s\S]*\.nav-tab::before/);
  assert.match(layoutCss, /clip-path: polygon\(50% 0, 100% 100%, 0 100%\)/);
  assert.match(layoutCss, /\.settlement-services \.nav-tab \{[\s\S]*height: 4\.25rem/);
  assert.match(layoutCss, /\.settlement-services \{[\s\S]*overflow: hidden/);
  assert.match(layoutCss, /\.settlement-identity \{[\s\S]*background: var\(--building-surface\)/);
  assert.match(layoutCss, /\.settlement-time \{[\s\S]*border-top:/);
});

test("settlement side panels use tint-derived beams and corner blocks", () => {
  assert.match(layoutCss, /data-environment="settlement"[\s\S]*:is\(\.left-sidebar, \.right-sidebar\)/);
  assert.match(layoutCss, /--building-frame: color-mix\(in srgb, var\(--building-surface\)/);
  assert.match(layoutCss, /--building-frame-corner: color-mix/);
  assert.match(layoutCss, /--building-frame-corner-size: 1\.35rem/);
  assert.match(layoutCss, /--building-panel-recess: color-mix/);
  assert.match(layoutCss, /padding-block: var\(--building-frame-corner-size\)/);
  assert.match(layoutCss, /padding-inline: var\(--building-frame-corner-size\)/);
  assert.match(layoutCss, /border: 0/);
  assert.match(layoutCss, /left top \/ var\(--building-frame-corner-size\) var\(--building-frame-corner-size\) no-repeat/);
  assert.match(layoutCss, /calc\(100% \+ var\(--left-rail-scrollbar-reserve, 0px\)\) bottom \/ var\(--building-frame-corner-size\) var\(--building-frame-corner-size\) no-repeat local/);
  assert.match(layoutCss, /center top \/ 100% 0\.55rem no-repeat local/);
  assert.ok(layoutCss.indexOf("var(--left-rail-scrollbar-reserve, 0px)) bottom") < layoutCss.indexOf("center top / 100% 0.55rem"));
  assert.doesNotMatch(layoutCss, /:is\(\.left-sidebar, \.right-sidebar\)::after/);
});

test("strategic left rails keep their scrollbars on the outer edge", () => {
  assert.match(layoutCss, /--left-rail-scrollbar-reserve: 8px;/);
  assert.match(layoutCss, /\.left-sidebar \{[\s\S]*direction: rtl;[\s\S]*scrollbar-gutter: stable;/);
  assert.match(layoutCss, /\.left-sidebar > \* \{ direction: ltr; \}/);
  assert.match(strategicCss, /\.left-sidebar \.encumbrance-inventory-scroll \{[\s\S]*direction: rtl;/);
  assert.match(strategicCss, /\.left-sidebar \.encumbrance-inventory-scroll > \* \{ direction: ltr; \}/);
});

test("settlement frames compensate for the left scrollbar gutter", () => {
  assert.match(layoutCss, /calc\(100% \+ var\(--left-rail-scrollbar-reserve, 0px\)\) top/);
  assert.match(layoutCss, /calc\(100% \+ var\(--left-rail-scrollbar-reserve, 0px\)\) center \/ 0\.55rem 100% no-repeat local/);
  assert.match(layoutCss, /inset calc\(-1 \* var\(--left-rail-scrollbar-reserve, 0px\)\) 0 0 var\(--building-frame\)/);
});

test("skill schedule columns fit inside a framed left rail", () => {
  assert.match(strategicCss, /\.skill-schedule \.party-skill-name-column \{ width: 2rem; \}/);
  assert.match(strategicCss, /\.skill-schedule \.schedule-effect-column \{ width: 1\.3rem; \}/);
  assert.match(strategicCss, /\.skill-schedule \.religion-auto-column \{ width: 1\.45rem; \}/);
  assert.match(strategicCss, /\.skill-schedule \.party-skill-time-column \{ width: 2\.25rem; \}/);
  assert.match(strategicCss, /\.skill-schedule \.religion-expand-column \{ width: 1\.35rem; \}/);
});

test("building state is re-applied when live regions replace party links", () => {
  assert.match(buildingSource, /new MutationObserver/);
  assert.match(buildingSource, /mutation\.addedNodes/);
  assert.match(buildingSource, /syncPartyLinks\(node\)/);
});
