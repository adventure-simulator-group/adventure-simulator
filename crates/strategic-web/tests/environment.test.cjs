const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

const source = fs.readFileSync("crates/strategic-web/static/strategic-time.js", "utf8");
const buildingSource = fs.readFileSync("crates/strategic-web/static/building-state.js", "utf8");
const baseCss = fs.readFileSync("crates/strategic-web/static/css/base.css", "utf8");
const layoutCss = fs.readFileSync("crates/strategic-web/static/css/layout.css", "utf8");
const componentsCss = fs.readFileSync("crates/strategic-web/static/css/components.css", "utf8");
const strategicCss = fs.readFileSync("crates/strategic-web/static/css/strategic.css", "utf8");

test("grouped inventory disclosures stay beside their labels in narrow merchant rails", () => {
  assert.match(strategicCss, /:is\(\.currency-parent-row, \.alcohol-parent-row, \.food-parent-row\) \.inventory-item-label \{[\s\S]*display: inline-block;[\s\S]*max-width: calc\(100% - 1\.5rem\);/);
  assert.match(strategicCss, /:is\(\.currency-parent-row, \.alcohol-parent-row, \.food-parent-row\) \.currency-disclosure \{[\s\S]*vertical-align: middle;/);
});
const layoutTemplate = fs.readFileSync("crates/strategic-web/src/templates/layout.rs", "utf8");
const settlementTemplate = fs.readFileSync("crates/strategic-web/src/templates/settlement.rs", "utf8");
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

test("settlement tabs layer tiered tintable buildings and proportional horizons beneath service icons", () => {
  assert.match(baseCss, /--settlement-header-height:144px/);
  assert.match(layoutCss, /body:has\(\.settlement-top-bar\) \.main-grid \{[\s\S]*var\(--settlement-header-height\)/);
  assert.match(layoutCss, /data-environment="settlement"[\s\S]*\.service-tab-building/);
  assert.match(layoutCss, /\.service-tab-building \{[\s\S]*inset: -0\.45rem -0\.65rem -0\.5rem/);
  assert.match(layoutCss, /background-blend-mode: color, normal/);
  assert.match(layoutCss, /mask: var\(--service-building-image\) center bottom \/ contain no-repeat/);
  assert.match(layoutCss, /data-environment="settlement"[\s\S]*\.service-tab-building \{[\s\S]*display: block/);
  assert.match(layoutCss, /filter: brightness\(var\(--building-light, 78%\)\) brightness\(0\.55\)/);
  assert.doesNotMatch(layoutCss, /clip-path: polygon\(50% 0, 100% 100%, 0 100%\)/);
  assert.match(layoutCss, /\.service-tab-building \{[\s\S]*pointer-events: none/);
  assert.match(layoutCss, /\.settlement-top-bar\[data-environment="settlement"\]::before \{[\s\S]*background-size: cover;[\s\S]*background-position: center bottom;[\s\S]*brightness\(var\(--building-light/);
  assert.doesNotMatch(layoutCss, /background-size: 100% 100%/);
  assert.match(layoutCss, /\.service-tab-icon \{[\s\S]*z-index: 2/);
  assert.match(layoutCss, /\.service-tab-icon::after \{[\s\S]*background-color: #fff[\s\S]*mask: var\(--service-tab-icon\)/);
  assert.doesNotMatch(layoutCss, /\.service-tab-icon::before/);
  assert.match(layoutCss, /data-environment="settlement"[\s\S]*\.service-tab-icon \{[\s\S]*bottom: var\(--service-icon-bottom, 1\.2rem\)[\s\S]*filter: brightness\(var\(--building-light, 78%\)\)/);
  assert.match(layoutCss, /data-building-tier="village"[\s\S]*--service-icon-bottom: 1\.2rem[\s\S]*--service-icon-size: 1\.6rem/);
  assert.match(layoutCss, /data-building-tier="town"[\s\S]*--service-icon-bottom: 1\.5rem[\s\S]*--service-icon-size: 1\.7rem/);
  assert.match(layoutCss, /data-building-tier="city"[\s\S]*--service-icon-bottom: 1\.8rem[\s\S]*--service-icon-size: 1\.75rem/);
  assert.match(layoutCss, /data-environment="settlement"[\s\S]*\.settlement-services \.nav-tab \{[\s\S]*width: 5\.25rem[\s\S]*height: 6\.75rem/);
  assert.match(layoutCss, /\.service-notification-badge \{[\s\S]*z-index: 3/);
  assert.match(layoutCss, /data-environment="settlement"[\s\S]*\.service-notification-badge \{[\s\S]*right: calc\(50% - var\(--service-icon-size, 1\.6rem\) \/ 2 - 0\.4rem\)[\s\S]*bottom: calc\(var\(--service-icon-bottom, 1\.2rem\) \+ var\(--service-icon-size, 1\.6rem\) - 0\.4rem\)/);
  assert.match(layoutCss, /\.nav-tab\.active \{[\s\S]*border-bottom: 3px solid var\(--accent-light\)/);
  assert.match(layoutCss, /\.nav-tab\.active::after \{[\s\S]*z-index: 4[\s\S]*bottom: 0[\s\S]*height: 3px/);
  assert.match(layoutCss, /\.settlement-top-bar \{[\s\S]*padding-inline: 0/);
  assert.match(layoutCss, /\.settlement-top-bar \.settlement-location \{[\s\S]*position: absolute;[\s\S]*z-index: 10;[\s\S]*top: 0\.5rem;[\s\S]*left: 0\.75rem/);
  assert.match(layoutCss, /\.settlement-top-bar \.top-bar-right \{[\s\S]*position: absolute;[\s\S]*z-index: 10;[\s\S]*top: 0\.5rem;[\s\S]*right: 0\.75rem/);
  assert.match(layoutCss, /\.settlement-services \{[\s\S]*width: 100%;[\s\S]*padding-inline: 0;[\s\S]*overflow: visible/);
  assert.match(layoutCss, /@media \(max-width: 1200px\)[\s\S]*data-environment="settlement"[\s\S]*width: 4\.25rem/);
  assert.match(layoutCss, /\.settlement-identity \{[\s\S]*background: var\(--building-surface\)/);
  assert.match(layoutCss, /\.settlement-time \{[\s\S]*border-top:/);
  for (const tier of ["village", "town", "city"]) {
    for (const service of ["map", "merchants", "weapons", "armor", "clothing", "herbalist", "inn", "religion"]) {
      assert.match(layoutCss, new RegExp(`building/${tier}/${service}\\.png`));
    }
    for (const variant of ["inland", "coastal", "river"]) {
      assert.match(layoutCss, new RegExp(`background/${tier}/${variant}\\.png`));
    }
  }
  assert.equal((layoutCss.match(/building\/(?:village|town|city)\/map\.png\?v=watchtower-2/g) || []).length, 3);
  for (const icon of ["travel", "market", "weapons", "armor", "clothing", "herbalist", "inn"]) {
    assert.match(layoutCss, new RegExp(`settlement-services/${icon}\\.png`));
  }
  assert.match(buildingSource, /"clothing", "herbalist", "inn"/);
});

test("settlement smithies and wilderness tabs use independent non-interactive effect layers", () => {
  assert.match(layoutTemplate, /path == "weapons"[\s\S]*building-chimney-smoke/);
  assert.match(layoutTemplate, /class="wilderness-flame campfire-flame"[\s\S]*aria-hidden="true"/);
  assert.match(layoutTemplate, /smoke_effect\("wilderness-smoke campfire-smoke"\)/);
  assert.match(layoutTemplate, /class="topbar-scene-effect-plane"/);
  assert.match(layoutTemplate, /svg class=\(class\)[\s\S]*aria-hidden="true"[\s\S]*focusable="false"/);
  assert.match(layoutCss, /\.wilderness-smoke,[\s\S]*\.wilderness-flame \{[\s\S]*z-index: 1;[\s\S]*pointer-events: none/);
  assert.match(layoutCss, /\.service-notification-badge \{[\s\S]*z-index: 3/);
  assert.match(layoutCss, /\.nav-tab\.active::after \{[\s\S]*z-index: 4/);
  assert.match(layoutCss, /@media \(prefers-reduced-motion: reduce\)[\s\S]*animation: none/);
  assert.doesNotMatch(layoutTemplate, /<filter|feTurbulence|feDisplacementMap/);
  assert.match(layoutCss, /\.topbar-scene-effect-plane \{[\s\S]*bottom: var\(--topbar-prop-baseline\);[\s\S]*width: 6\.55rem;[\s\S]*height: 6\.55rem;[\s\S]*scale\(var\(--topbar-prop-scale\)\)/);
  assert.match(layoutCss, /@media \(max-width: 1200px\)[\s\S]*--topbar-prop-scale: 0\.8473;[\s\S]*data-environment="wilderness"[\s\S]*--topbar-prop-scale: 0\.8473;/);
  assert.match(layoutCss, /\.campfire-smoke \{[\s\S]*--smoke-rise-distance: -180px;/);
  assert.match(layoutCss, /@media \(max-width: 768px\)[\s\S]*padding: 0\.75rem 0\.5rem 0\.35rem;[\s\S]*overflow-y: hidden;[\s\S]*--topbar-prop-scale: 0\.8855;[\s\S]*\.campfire-smoke \{[\s\S]*--smoke-rise-distance: -110px;/);

  const smokeFrames = layoutCss.match(/@keyframes wilderness-smoke-rise \{[\s\S]*?\n\}/)?.[0] ?? "";
  const flameFrames = layoutCss.match(/@keyframes wilderness-flame-flicker \{[\s\S]*?\n\}/)?.[0] ?? "";
  for (const frames of [smokeFrames, flameFrames]) {
    assert.match(frames, /transform:/);
    assert.match(frames, /opacity:/);
    assert.doesNotMatch(frames, /filter:|fill:|stroke:/);
  }
  const particleFrames = layoutCss.match(/@keyframes campfire-particle-rise \{[\s\S]*?\n\}/)?.[0] ?? "";
  assert.match(particleFrames, /#ffe06a[\s\S]*#ed7a25[\s\S]*#bf3826[\s\S]*#827b74/);
  assert.match(particleFrames, /transform:/);
  assert.match(particleFrames, /opacity:/);
  assert.equal((layoutTemplate.match(/class="fire-particle"/g) || []).length, 1);
  assert.match(layoutTemplate, /let particles = \[[\s\S]*\];[\s\S]*@for \(cx, cy, radius, drift, delay, duration\) in particles/);
  assert.match(layoutTemplate, /let puffs = \[[\s\S]*\];[\s\S]*@for \(cx, cy, radius, drift, delay, duration\) in puffs/);
});

test("quest and camp headers share the tent while keeping fire and enemy layers independent", () => {
  for (const [view, variant] of [
    ["camp", "camp-tent"],
    ["map", "camp-tent"],
    ["enemy", "encounter-boulders"],
  ]) {
    assert.match(layoutTemplate, new RegExp(`data-location-view="${view}"`));
    assert.match(layoutCss, new RegExp(`ornament/${variant}/ornament\\.png`));
  }
  assert.match(layoutTemplate, /aria-label="Map"[\s\S]*aria-label="Enemy"/);
  assert.doesNotMatch(layoutTemplate, /aria-label="(?:Encounter|Loot)"/);
  assert.match(layoutCss, /service-tab-icon-enemy[\s\S]*death-skull\.svg/);
  assert.match(layoutCss, /data-environment="wilderness"[\s\S]*\.wilderness-tab-prop \{[\s\S]*background-blend-mode: color, normal[\s\S]*pointer-events: none/);
  assert.match(settlementTemplate, /actual_camp_intervals[\s\S]*movement_minute == journey\.completed_minutes/);
  assert.match(settlementTemplate, /camp_location_layout_with_session\([\s\S]*camp_fire_lit/);
  assert.doesNotMatch(settlementTemplate, /camp-fire|fire-state|rested=.*Query/);
});

test("wilderness headers select a tintable physical horizon", () => {
  assert.match(layoutTemplate, /data-wilderness-variant=\(wilderness_variant\(location_id\)\.as_str\(\)\)/);
  assert.match(layoutCss, /data-environment="wilderness"\]::before \{[\s\S]*background-image: var\(--wilderness-horizon-image\);[\s\S]*background-position: center bottom;[\s\S]*background-size: cover;[\s\S]*brightness\(var\(--building-light/);
  for (const variant of ["forest", "grassland", "hills"]) {
    assert.match(layoutCss, new RegExp(`data-wilderness-variant="${variant}"[\\s\\S]*background/wilderness/${variant}\\.png`));
  }
});

test("settlement side panels use tint-derived frames around neutral recesses", () => {
  assert.match(layoutCss, /data-environment="settlement"[\s\S]*:is\(\.left-sidebar, \.right-sidebar\)/);
  assert.match(layoutCss, /--building-frame: color-mix\(in srgb, var\(--building-frame-tint\)/);
  assert.match(layoutCss, /--building-frame-corner: color-mix/);
  assert.match(layoutCss, /--building-frame-corner-size: 1\.35rem/);
  assert.match(layoutCss, /--building-panel-recess: var\(--content-surface-recess\)/);
  assert.match(layoutCss, /padding-block: var\(--building-frame-corner-size\)/);
  assert.match(layoutCss, /padding-inline: var\(--building-frame-corner-size\)/);
  assert.match(layoutCss, /border: 0/);
  assert.match(layoutCss, /left top \/ var\(--building-frame-corner-size\) var\(--building-frame-corner-size\) no-repeat/);
  assert.match(layoutCss, /calc\(100% \+ var\(--left-rail-scrollbar-reserve, 0px\)\) bottom \/ var\(--building-frame-corner-size\) var\(--building-frame-corner-size\) no-repeat local/);
  assert.match(layoutCss, /center top \/ 100% 0\.55rem no-repeat local/);
  assert.ok(layoutCss.indexOf("var(--left-rail-scrollbar-reserve, 0px)) bottom") < layoutCss.indexOf("center top / 100% 0.55rem"));
  assert.doesNotMatch(layoutCss, /:is\(\.left-sidebar, \.right-sidebar\)::after/);
  for (const opacity of ["4%", "3%", "6%"]) {
    assert.match(layoutCss, new RegExp(`architectural-edge:[^;]*\\/ ${opacity.replace("%", "\\%")}\\)`));
  }
});

test("patterned rails keep text and controls on opaque reading surfaces", () => {
  assert.match(
    layoutCss,
    /data-environment="settlement"[\s\S]*:is\(\.left-sidebar, \.right-sidebar\) \.sidebar-section \{[\s\S]*background: var\(--content-surface-recess\)/,
  );
});

test("ceremonial blackletter is never transformed to all caps", () => {
  assert.match(layoutCss, /\.entry-message \{[\s\S]*font-family: var\(--font-display\)[\s\S]*text-transform: none/);
  assert.match(layoutCss, /\.sidebar-header \{[\s\S]*font-family: var\(--font-display\)[\s\S]*text-transform: none/);
  assert.match(componentsCss, /\.panel-header \{[\s\S]*font-family: var\(--font-display\)[\s\S]*text-transform: none/);
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
  assert.match(layoutCss, /right center \/ 0\.55rem 100% no-repeat local/);
  assert.doesNotMatch(layoutCss, /inset calc\(-1 \* var\(--left-rail-scrollbar-reserve, 0px\)\) 0 0 var\(--building-frame\)/);
  assert.ok(
    layoutCss.indexOf("right top / var(--building-frame-corner-size)")
      < layoutCss.indexOf("right center / 0.55rem 100% no-repeat local"),
  );
});

test("skill schedule columns fit inside a framed left rail", () => {
  assert.match(strategicCss, /\.skill-schedule \.party-skill-name-column \{ width: 2rem; \}/);
  assert.match(strategicCss, /\.skill-schedule \.schedule-effect-column \{ width: 1\.3rem; \}/);
  assert.match(strategicCss, /\.skill-schedule \.religion-auto-column \{ width: 1\.45rem; \}/);
  assert.match(strategicCss, /\.skill-schedule \.party-skill-time-column \{ width: 2\.25rem; \}/);
  assert.match(strategicCss, /\.skill-schedule \.religion-expand-column \{ width: 1\.35rem; \}/);
});

test("character selection actions wrap long adventurer names inside their cards", () => {
  assert.match(strategicCss, /\.character-select-action \{[\s\S]*max-width: 100%;[\s\S]*overflow-wrap: anywhere;[\s\S]*white-space: normal;/);
});

test("stacked inventory rails shed desktop overhang and scroll within the available width", () => {
  assert.match(strategicCss, /@media \(max-width: 768px\)[\s\S]*:has\(\.inventory-browser\)[\s\S]*width: 100%;[\s\S]*direction: ltr;[\s\S]*overflow-x: hidden;/);
  assert.match(strategicCss, /\.inventory-browser-table-frame \{ overflow-x: auto; overflow-y: visible; \}/);
  assert.match(strategicCss, /\.smith-wares-scroll,[\s\S]*\.encumbrance-inventory-scroll[\s\S]*margin-inline: 0;[\s\S]*padding-inline: 0;/);
});

test("rest duration radios use a bounded accessible hiding technique", () => {
  assert.match(strategicCss, /\.rest-duration-unit input \{[\s\S]*width: 1px;[\s\S]*height: 1px;[\s\S]*clip-path: inset\(50%\);/);
  assert.match(strategicCss, /\.rest-duration-unit:focus-within \{ outline: 2px solid var\(--accent\)/);
});

test("building state is re-applied when live regions replace party links", () => {
  assert.match(buildingSource, /new MutationObserver/);
  assert.match(buildingSource, /mutation\.addedNodes/);
  assert.match(buildingSource, /syncPartyLinks\(node\)/);
});
