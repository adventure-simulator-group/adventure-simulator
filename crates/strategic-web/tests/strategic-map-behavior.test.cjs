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
  assert.deepEqual(Array.from(helpers.zoomedView([0, 0, 80, 160 / 3], .5)), [20, 40 / 3, 40, 80 / 3]);
  assert.deepEqual(Array.from(helpers.zoomedView([0, 0, 20, 40 / 3], .5)), [0, 0, 20, 40 / 3]);
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

test("pin symbols retain their screen size while zooming and resetting", () => {
  const { document, helpers } = load();
  document.body.innerHTML = `<section data-strategic-map><button data-map-zoom="in"></button><button data-map-reset></button><svg data-map-svg viewBox="100 100 195 130"><g data-map-pin-symbol></g></svg></section>`;
  const map = document.querySelector("section");
  helpers.initializeMap(map, null);
  const symbol = map.querySelector("[data-map-pin-symbol]");
  assert.equal(symbol.getAttribute("transform"), "scale(0.50000)");
  map.querySelector("[data-map-zoom]").click();
  assert.equal(symbol.getAttribute("transform"), "scale(0.40000)");
  map.querySelector("[data-map-reset]").click();
  assert.equal(symbol.getAttribute("transform"), "scale(0.50000)");
});

test("pin links remain ordinary destination URLs", () => {
  const { document, helpers } = load();
  document.body.innerHTML = `<section data-strategic-map><svg data-map-svg viewBox="0 0 400 200"><a data-map-pin href="/locations/settlement/a/map?destination=b"><circle/></a></svg></section>`;
  helpers.initializeMap(document.querySelector("section"), null);
  assert.equal(document.querySelector("[data-map-pin]").getAttribute("href"), "/locations/settlement/a/map?destination=b");
});

test("tile zoom follows display density and respects the generated ceiling", () => {
  const { helpers } = load();
  assert.equal(helpers.tileZoom(400, 800, 1, 4), 1);
  assert.equal(helpers.tileZoom(90, 1200, 1, 4), 4);
  assert.equal(helpers.tileZoom(20, 1200, 2, 4), 4);
});

test("visible tile range loads exact intersections and clamps to the world", () => {
  const { helpers } = load();
  const range = helpers.visibleTileRange([0, 0, 90, 60], 512, 4);
  assert.equal(range.span, 32);
  assert.deepEqual(
    [range.minX, range.maxX, range.minY, range.maxY],
    [0, 2, 0, 1],
  );
});

test("theme changes replace only the cached raster tile layer", () => {
  const { document, helpers } = load();
  document.body.innerHTML = `<section data-strategic-map data-map-theme="atlas" data-map-tile-size="512" data-map-max-tile-zoom="4" data-map-tile-version="abc123" data-map-tile-root="/map/tiles/"><button data-map-theme-choice="paper"></button><button data-map-theme-choice="atlas"></button><svg data-map-svg viewBox="590 390 90 60"><g data-map-tile-layer></g><g data-map-pin-symbol></g></svg></section>`;
  const map = document.querySelector("section");
  const svg = map.querySelector("svg");
  svg.getBoundingClientRect = () => ({ width: 1200, height: 800 });
  helpers.initializeMap(map, null);
  assert.match(map.querySelector("[data-map-tile-layer] image").getAttribute("href"), /^\/map\/tiles\/atlas\/4\//);
  map.querySelector('[data-map-theme-choice="paper"]').click();
  assert.match(map.querySelector("[data-map-tile-layer] image").getAttribute("href"), /^\/map\/tiles\/paper\/4\//);
  assert.equal(map.querySelectorAll("[data-map-pin-symbol]").length, 1);
});
