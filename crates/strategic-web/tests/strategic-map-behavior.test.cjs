const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");
const { parseHTML } = require("linkedom");

const source = fs.readFileSync(path.join(__dirname, "..", "static", "strategic-map.js"), "utf8");

const load = ({ ResizeObserver } = {}) => {
  const { document } = parseHTML(`<main></main>`);
  const context = { document, localStorage: null, ResizeObserver };
  context.globalThis = context;
  vm.runInNewContext(source, context);
  return { document, helpers: context.StrategicMap };
};

test("camera expands to the rendered element aspect ratio without cropping its fitted area", () => {
  const { helpers } = load();
  assert.deepEqual(
    Array.from(helpers.viewForElement([100, 200, 300, 200], 300, 600)),
    [100, 0, 300, 600],
  );
  assert.deepEqual(
    Array.from(helpers.viewForElement([100, 200, 300, 200], 900, 300)),
    [-50, 200, 600, 200],
  );
});

test("element resize preserves camera center and world scale while updating loaded tiles", () => {
  let resizeMap;
  class TestResizeObserver {
    constructor(callback) { resizeMap = callback; }
    observe() {}
  }
  const { document, helpers } = load({ ResizeObserver: TestResizeObserver });
  document.body.innerHTML = `<section data-strategic-map data-map-theme="paper" data-map-tile-size="128" data-map-tile-gutter="0" data-map-max-tile-zoom="0" data-map-tile-version="digest" data-map-tile-root="/map/tiles/"><svg data-map-svg viewBox="100 200 300 200"><g data-map-tile-layer></g></svg></section>`;
  const map = document.querySelector("section");
  const svg = map.querySelector("svg");
  let rect = { width: 300, height: 600 };
  svg.getBoundingClientRect = () => rect;
  helpers.initializeMap(map);
  assert.equal(svg.getAttribute("viewBox"), "100.00 0.00 300.00 600.00");
  assert.equal(map.querySelectorAll("[data-map-tile-layer] image").length, 20);

  rect = { width: 600, height: 300 };
  resizeMap();
  assert.equal(svg.getAttribute("viewBox"), "-50.00 150.00 600.00 300.00");
  assert.equal(map.querySelectorAll("[data-map-tile-layer] image").length, 15);
});

test("zoom preserves focus and clamps readable bounds", () => {
  const { helpers } = load();
  assert.deepEqual(Array.from(helpers.zoomedView([100, 100, 400, 200], .5)), [200, 150, 200, 100]);
  assert.deepEqual(Array.from(helpers.zoomedView([0, 0, 80, 160 / 3], .5)), [20, 40 / 3, 40, 80 / 3]);
  assert.deepEqual(Array.from(helpers.zoomedView([0, 0, 10, 20 / 3], .5)), [0, 0, 10, 20 / 3]);
  assert.deepEqual(Array.from(helpers.zoomedView([0, 0, 20, 50], .1)), [5, 12.5, 10, 25]);
  assert.deepEqual(Array.from(helpers.zoomedView([5, 12.5, 10, 25], .5)), [5, 12.5, 10, 25]);
});

test("keyboard pan and reset change only the SVG viewBox", () => {
  const { document, helpers } = load();
  document.body.innerHTML = `<section data-strategic-map><svg data-map-svg viewBox="100 100 400 200"></svg></section>`;
  const map = document.querySelector("section");
  helpers.initializeMap(map, null);
  const svg = map.querySelector("svg");
  const keydown = new document.defaultView.Event("keydown", { bubbles: true, cancelable: true });
  Object.defineProperty(keydown, "key", { value: "ArrowRight" });
  svg.dispatchEvent(keydown);
  assert.equal(svg.getAttribute("viewBox"), "132.00 100.00 400.00 200.00");
  const reset = new document.defaultView.Event("keydown", { bubbles: true, cancelable: true });
  Object.defineProperty(reset, "key", { value: "Home" });
  svg.dispatchEvent(reset);
  assert.equal(svg.getAttribute("viewBox"), "100.00 100.00 400.00 200.00");
});

test("pin symbols retain their screen size while zooming and resetting", () => {
  const { document, helpers } = load();
  document.body.innerHTML = `<section data-strategic-map><svg data-map-svg viewBox="100 100 195 130"><g data-map-pin-symbol></g></svg></section>`;
  const map = document.querySelector("section");
  helpers.initializeMap(map, null);
  const svg = map.querySelector("svg");
  const symbol = map.querySelector("[data-map-pin-symbol]");
  assert.equal(symbol.getAttribute("transform"), "scale(0.50000)");
  const zoom = new document.defaultView.Event("keydown", { bubbles: true, cancelable: true });
  Object.defineProperty(zoom, "key", { value: "+" });
  svg.dispatchEvent(zoom);
  assert.equal(symbol.getAttribute("transform"), "scale(0.40000)");
  const reset = new document.defaultView.Event("keydown", { bubbles: true, cancelable: true });
  Object.defineProperty(reset, "key", { value: "Home" });
  svg.dispatchEvent(reset);
  assert.equal(symbol.getAttribute("transform"), "scale(0.50000)");
});

test("two-pointer pinch zooms without visible controls", () => {
  const { document, helpers } = load();
  document.body.innerHTML = `<section data-strategic-map><svg data-map-svg viewBox="100 100 400 200"></svg></section>`;
  const map = document.querySelector("section");
  const svg = map.querySelector("svg");
  svg.getBoundingClientRect = () => ({ left: 0, top: 0, width: 800, height: 400 });
  helpers.initializeMap(map, null);
  const pointer = (type, pointerId, clientX, clientY) => {
    const event = new document.defaultView.Event(type, { bubbles: true, cancelable: true });
    Object.defineProperties(event, {
      pointerId: { value: pointerId }, clientX: { value: clientX }, clientY: { value: clientY },
    });
    svg.dispatchEvent(event);
  };

  pointer("pointerdown", 1, 200, 200);
  pointer("pointerdown", 2, 600, 200);
  pointer("pointermove", 2, 700, 200);

  assert.equal(svg.getAttribute("viewBox"), "120.00 120.00 320.00 160.00");
  assert.equal(map.querySelectorAll("button").length, 0);
});

test("label priority reveals progressively smaller settlements while zooming", () => {
  const { helpers } = load();
  assert.equal(helpers.labelPriorityThreshold(800), 80);
  assert.equal(helpers.labelPriorityThreshold(390), 60);
  assert.equal(helpers.labelPriorityThreshold(90), 40);
  assert.equal(helpers.labelPriorityThreshold(50), 20);
});

test("label layout keeps important names and moves collisions to the alternate side", () => {
  const { document, helpers } = load();
  document.body.innerHTML = `<svg viewBox="0 0 100 66.67">
    <g data-map-label data-map-x="50" data-map-y="30" data-map-label-priority="100" data-map-label-width="70" data-map-label-essential="true"><text>Current</text></g>
    <g data-map-label data-map-x="51" data-map-y="30" data-map-label-priority="60" data-map-label-width="70" data-map-label-essential="false"><text>Town</text></g>
    <g data-map-label data-map-x="80" data-map-y="50" data-map-label-priority="50" data-map-label-width="70" data-map-label-essential="false"><text>Village</text></g>
  </svg>`;
  const svg = document.querySelector("svg");
  svg.getBoundingClientRect = () => ({ width: 600, height: 400 });
  helpers.layoutLabels(svg, [0, 0, 100, 66.67]);
  const labels = svg.querySelectorAll("[data-map-label]");
  assert.equal(labels[0].getAttribute("display"), "inline");
  assert.equal(labels[1].getAttribute("display"), "inline");
  assert.equal(labels[1].querySelector("text").getAttribute("text-anchor"), "end");
  assert.equal(labels[2].getAttribute("display"), "inline");
});

test("collision helper treats padded touching labels as overlapping", () => {
  const { helpers } = load();
  assert.equal(helpers.boxesOverlap(
    { left: 0, right: 20, top: 0, bottom: 10 },
    { left: 22, right: 42, top: 0, bottom: 10 },
  ), true);
  assert.equal(helpers.boxesOverlap(
    { left: 0, right: 20, top: 0, bottom: 10 },
    { left: 24, right: 44, top: 0, bottom: 10 },
  ), false);
});

test("pin links remain ordinary destination URLs", () => {
  const { document, helpers } = load();
  document.body.innerHTML = `<section data-strategic-map><svg data-map-svg viewBox="0 0 400 200"><a data-map-pin href="/locations/settlement/a/map?destination=b"><circle/></a></svg></section>`;
  helpers.initializeMap(document.querySelector("section"), null);
  assert.equal(document.querySelector("[data-map-pin]").getAttribute("href"), "/locations/settlement/a/map?destination=b");
});

test("tile zoom follows display density and respects the generated ceiling", () => {
  const { helpers } = load();
  assert.equal(helpers.tileZoom(400, 800, 1, 6), 1);
  assert.equal(helpers.tileZoom(90, 1200, 1, 6), 4);
  assert.equal(helpers.tileZoom(10, 1200, 1, 6), 6);
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

test("missing deepest tiles deterministically crop their complete parent tile", () => {
  const { document, helpers } = load();
  assert.deepEqual(
    { ...helpers.parentTileFallback(7, 75, 61, 512, 4) },
    { zoom: 6, x: 37, y: 30, left: 295.9375, top: 239.9375, size: 8.125 },
  );
  document.body.innerHTML = `<section data-strategic-map data-map-theme="paper" data-map-tile-size="512" data-map-tile-gutter="4" data-map-max-tile-zoom="7" data-map-tile-version="digest" data-map-tile-root="/map/tiles/"><svg data-map-svg viewBox="300 244 5 3.33"><g data-map-tile-layer></g></svg></section>`;
  const map = document.querySelector("section");
  const svg = map.querySelector("[data-map-svg]");
  svg.getBoundingClientRect = () => ({ width: 1200, height: 800 });
  helpers.initializeMap(map);
  const image = map.querySelector("[data-map-tile-layer] image");
  const missingHref = image.getAttribute("href");
  image.dispatchEvent(new document.defaultView.Event("error"));
  assert.notEqual(image.getAttribute("href"), missingHref);
  assert.match(image.getAttribute("href"), /^\/map\/tiles\/paper\/6\//);
  assert.equal(image.closest("svg").getAttribute("overflow"), "hidden");
});

test("paper tiles render independently of the dynamic pin layer", () => {
  const { document, helpers } = load();
  document.body.innerHTML = `<section data-strategic-map data-map-theme="paper" data-map-tile-size="512" data-map-tile-gutter="4" data-map-max-tile-zoom="6" data-map-tile-version="abc123" data-map-tile-root="/map/tiles/"><svg data-map-svg viewBox="590 390 10 6.67"><g data-map-tile-layer></g><g data-map-pin-symbol></g></svg></section>`;
  const map = document.querySelector("section");
  const svg = map.querySelector("svg");
  svg.getBoundingClientRect = () => ({ width: 1200, height: 800 });
  helpers.initializeMap(map, null);
  assert.match(map.querySelector("[data-map-tile-layer] image").getAttribute("href"), /^\/map\/tiles\/paper\/6\//);
  assert.equal(map.querySelector("[data-map-tile-layer] image").getAttribute("width"), "8.125");
  assert.equal(map.querySelectorAll("[data-map-pin-symbol]").length, 1);
});
