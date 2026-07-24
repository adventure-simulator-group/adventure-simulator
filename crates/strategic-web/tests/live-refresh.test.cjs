const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const assert = require("node:assert/strict");
const vm = require("node:vm");

const root = path.join(__dirname, "..");

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
      dataset: { liveRefreshUrl: "/locations/settlement/riverdale/inn" },
    }),
  };
  assert.equal(
    window.strategicLiveRefreshUrl(
      marker,
      { pathname: "/settlements/riverdale/rest/inn", search: "" },
    ),
    "/locations/settlement/riverdale/inn",
  );
  assert.equal(
    window.strategicLiveRefreshUrl(
      { querySelector: () => null },
      { pathname: "/locations/settlement/riverdale/inn", search: "?building=inn" },
    ),
    "/locations/settlement/riverdale/inn?building=inn",
  );
});
