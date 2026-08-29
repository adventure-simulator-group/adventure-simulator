const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const assert = require("node:assert/strict");
const vm = require("node:vm");
const strategicCalendar = require("./strategic-calendar-fixture.cjs");
const { readRustModuleSource } = require("./rust-module-source.cjs");

const root = path.join(__dirname, "..");

test("all strategic clock script references use the accessible clock cache key", () => {
  const layout = fs.readFileSync(path.join(root, "src", "templates", "layout.rs"), "utf8");
  const references = [...layout.matchAll(/\/static\/strategic-time\.js\?v=([^\"]+)/g)]
    .map((match) => match[1]);
  assert.deepEqual(references, ["accessible-clock-2", "accessible-clock-2"]);
});

test("strategic clock refresh keeps visible and accessible character time synchronized", async () => {
  const source = fs.readFileSync(path.join(root, "static", "strategic-time.js"), "utf8");
  const attributes = new Map();
  const listeners = new Map();
  const responses = [
    { character_minutes: 1505, official_minutes: 2945 },
    { character_minutes: 2946, official_minutes: 4386 },
  ];
  const clock = {
    textContent: "1st of First Seed · 08:00",
    title: "Loading official time…",
    setAttribute(name, value) { attributes.set(name, value); },
  };
  const document = {
    documentElement: { style: { setProperty() {} } },
    querySelectorAll: () => [clock],
    dispatchEvent() {},
    addEventListener(type, listener) { listeners.set(type, listener); },
  };
  const window = {
    strategicCalendar,
    queueStrategicInitialLoad: (load) => Promise.resolve().then(load),
    strategicBackgroundFetch: async () => ({
      json: async () => responses.shift(),
    }),
    reportStrategicError(error) { throw error; },
  };
  class CustomEvent {
    constructor(type, init) { this.type = type; this.detail = init.detail; }
  }

  vm.runInNewContext(source, { window, document, CustomEvent, Promise });
  await new Promise((resolve) => setImmediate(resolve));

  assert.equal(clock.textContent, "Day 2 · 01:05");
  assert.equal(attributes.get("aria-label"), clock.textContent);
  assert.equal(clock.title, "Official time: Day 3 · 01:05");

  listeners.get("strategic-page-mounted")();
  await new Promise((resolve) => setImmediate(resolve));

  assert.equal(clock.textContent, "Day 3 · 01:06");
  assert.equal(attributes.get("aria-label"), clock.textContent);
  assert.equal(clock.title, "Official time: Day 4 · 01:06");
});

test("SSE subscribes before reading its initial revision", () => {
  const source = fs.readFileSync(path.join(root, "src", "live.rs"), "utf8");
  const subscribe = source.indexOf("let receiver = state.live.subscribe()");
  const revision = source.indexOf("let revision = state.live.revision()");
  assert.ok(subscribe >= 0 && revision > subscribe);
});

test("busy live region retries back off, cap, and can reset on idle", () => {
  const source = fs.readFileSync(path.join(root, "static", "live-regions.js"), "utf8");
  const timers = [];
  const window = {};
  vm.runInNewContext(source, {
    window,
    document: { querySelector: () => null },
  });
  const policy = window.strategicLiveRetryPolicyFactory({
    setTimer(callback, delay) {
      timers.push({ callback, delay });
      return timers.length;
    },
    clearTimer() {},
  });
  for (let index = 0; index < 6; index += 1) {
    assert.equal(policy.schedule(() => {}), true);
  }
  assert.deepEqual(timers.map((timer) => timer.delay), [250, 500, 1000, 2000, 4000, 4000]);
  assert.equal(policy.schedule(() => {}), false);
  policy.reset();
  assert.equal(policy.schedule(() => {}), true);
  assert.equal(timers.at(-1).delay, 250);
  assert.match(source, /document\.addEventListener\("focusout", reconcileDirtyWhenIdle\)/);
});

test("live reconciliation preserves keyed client-owned regions", () => {
  const source = fs.readFileSync(path.join(root, "static", "live-regions.js"), "utf8");
  assert.match(source, /comparableRegionHtml\(current\) === comparableRegionHtml\(next\)/);
  assert.match(source, /preserveClientRegions\(current, next\)/);
  assert.match(source, /replacement\.replaceWith\(region\)/);

  const trade = readRustModuleSource(
    path.join(root, "src", "templates", "settlement", "mod.rs"),
  );
  assert.match(trade, /data-live-preserve="forge-customization"/);
});

test("POST result pages provide a safe GET URL for live-region refreshes", () => {
  const source = fs.readFileSync(path.join(root, "static", "live-regions.js"), "utf8");
  const window = {};
  vm.runInNewContext(source, {
    window,
    location: { pathname: "/settlements/riverdale/rest/inn", search: "" },
    document: { querySelector: () => null },
  });
  const marker = {
    querySelector: () => ({
      dataset: { liveRefreshUrl: "/settlements/riverdale/inn" },
    }),
  };
  assert.equal(
    window.strategicLiveRefreshUrl(
      marker,
      { pathname: "/settlements/riverdale/rest/inn", search: "" },
    ),
    "/settlements/riverdale/inn",
  );
  assert.equal(
    window.strategicLiveRefreshUrl(
      marker,
      { pathname: "/settlements/riverdale/rest/inn", search: "" },
    ),
    "/settlements/riverdale/inn",
    "repeated refreshes retain the canonical marked GET URL",
  );
  assert.equal(
    window.strategicLiveRefreshUrl(
      {
        querySelector: () => ({
          dataset: { liveRefreshUrl: "/settlements/riverdale/religion" },
        }),
      },
      { pathname: "/settlements/riverdale/rest/temple", search: "" },
    ),
    "/settlements/riverdale/religion",
  );
  for (const kind of ["inn", "temple"]) {
    assert.equal(
      window.strategicLiveRefreshUrl(
        { querySelector: () => null },
        { pathname: `/settlements/riverdale/rest/${kind}`, search: "" },
      ),
      null,
      "a missing marker must never turn a POST action into a GET refresh",
    );
  }
  assert.equal(
    window.strategicLiveRefreshUrl(
      { querySelector: () => null },
      { pathname: "/locations/settlement/riverdale/inn", search: "?building=inn" },
    ),
    "/locations/settlement/riverdale/inn?building=inn",
  );
});
