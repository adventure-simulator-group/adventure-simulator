const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");
const { parseHTML } = require("linkedom");

const source = fs.readFileSync(path.join(__dirname, "..", "static", "strategic-map.js"), "utf8");

const load = () => {
  const { document } = parseHTML(`<main></main>`);
  const context = { document, localStorage: null };
  context.globalThis = context;
  vm.runInNewContext(source, context);
  return { document, helpers: context.StrategicMap };
};

test("zoom preserves focus and clamps readable bounds", () => {
  const { helpers } = load();
  assert.deepEqual(Array.from(helpers.zoomedView([100, 100, 400, 200], .5)), [200, 150, 200, 100]);
  assert.deepEqual(Array.from(helpers.zoomedView([0, 0, 80, 53.33], .5)), [0, 0, 80, 53.33]);
});

test("theme toggle persists and exposes pressed state", () => {
  const { document, helpers } = load();
  document.body.innerHTML = `<section data-strategic-map data-map-theme="atlas"><button data-map-theme-choice="paper"></button><button data-map-theme-choice="atlas"></button><svg data-map-svg viewBox="0 0 400 200"></svg></section>`;
  const values = new Map();
  const storage = { getItem: (key) => values.get(key), setItem: (key, value) => values.set(key, value) };
  const map = document.querySelector("section");
  helpers.initializeMap(map, storage);
  map.querySelector('[data-map-theme-choice="paper"]').click();
  assert.equal(map.dataset.mapTheme, "paper");
  assert.equal(map.querySelector('[data-map-theme-choice="paper"]').getAttribute("aria-pressed"), "true");
  assert.equal(values.get("adventuresim.map-theme"), "paper");
});

test("keyboard pan and reset change only the SVG viewBox", () => {
  const { document, helpers } = load();
  document.body.innerHTML = `<section data-strategic-map><button data-map-reset></button><svg data-map-svg viewBox="100 100 400 200"></svg></section>`;
  const map = document.querySelector("section");
  helpers.initializeMap(map, null);
  const svg = map.querySelector("svg");
  const keydown = new document.defaultView.Event("keydown", { bubbles: true, cancelable: true });
  Object.defineProperty(keydown, "key", { value: "ArrowRight" });
  svg.dispatchEvent(keydown);
  assert.equal(svg.getAttribute("viewBox"), "132.00 100.00 400.00 200.00");
  map.querySelector("button").click();
  assert.equal(svg.getAttribute("viewBox"), "100.00 100.00 400.00 200.00");
});

test("pin links remain ordinary destination URLs", () => {
  const { document, helpers } = load();
  document.body.innerHTML = `<section data-strategic-map><svg data-map-svg viewBox="0 0 400 200"><a data-map-pin href="/locations/settlement/a/map?destination=b"><circle/></a></svg></section>`;
  helpers.initializeMap(document.querySelector("section"), null);
  assert.equal(document.querySelector("[data-map-pin]").getAttribute("href"), "/locations/settlement/a/map?destination=b");
});
