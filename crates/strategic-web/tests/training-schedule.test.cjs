const assert = require("node:assert/strict");
const test = require("node:test");

const {
  calculateLeisurePreview,
  createLatestSaveQueue,
  parseClock,
  religionAllocationTotal,
  metaInputActive,
  setAllocationInteractive,
  religionInputActive,
  signedEffect,
  stepClockValue,
} = require("../static/training-schedule.js");

test("religion and combat independently select exactly one allocation branch", () => {
  const root = {
    querySelector(selector) {
      if (selector.includes("religion")) return { checked: false };
      if (selector.includes("combat")) return { checked: true };
      return null;
    },
  };
  assert.equal(metaInputActive({ dataset: { religionAutoBudget: "" } }, root), false);
  assert.equal(metaInputActive({ dataset: { religionManualBudget: "" } }, root), true);
  assert.equal(metaInputActive({ dataset: { combatAutoBudget: "" } }, root), true);
  assert.equal(metaInputActive({ dataset: { combatManualBudget: "" } }, root), false);
});

test("Combat allocation controls become keyboard interactive in both toggle directions", () => {
  const state = { inactive: null, ariaDisabled: null, tabindex: null };
  const control = {
    setAttribute(name, value) { state[name === "aria-disabled" ? "ariaDisabled" : name] = value; },
  };
  const allocation = {
    classList: { toggle(_name, value) { state.inactive = value; } },
    querySelectorAll() { return [control]; },
  };
  setAllocationInteractive(allocation, false);
  assert.deepEqual(state, { inactive: true, ariaDisabled: "true", tabindex: "-1" });
  setAllocationInteractive(allocation, true);
  assert.deepEqual(state, { inactive: false, ariaDisabled: "false", tabindex: "0" });
});

test("Religion allocation counts either the auto budget or manual traditions exactly once", () => {
  const allocation = {
    religion_minutes: 120,
    religion_roman_catholic_minutes: 45,
    religion_lutheran_minutes: 30,
    religion_judaism_minutes: 15,
  };
  assert.equal(religionAllocationTotal(allocation, true), 120);
  assert.equal(religionAllocationTotal(allocation, false), 90);
});

test("Religion mode toggles preserve inactive branch values across remounts", () => {
  const values = {
    religion_minutes: 120,
    religion_judaism_minutes: 45,
    religion_lutheran_minutes: 30,
  };
  const inputs = {
    religion_minutes: { dataset: { religionAutoBudget: "" } },
    religion_judaism_minutes: { dataset: { religionManualBudget: "" } },
    religion_lutheran_minutes: { dataset: { religionManualBudget: "" } },
  };
  const active = (autoTrain) => Object.fromEntries(Object.entries(values)
    .filter(([name]) => religionInputActive(inputs[name], autoTrain)));
  assert.deepEqual(active(true), { religion_minutes: 120 });
  assert.deepEqual(active(false), {
    religion_judaism_minutes: 45,
    religion_lutheran_minutes: 30,
  });
  assert.deepEqual(values, {
    religion_minutes: 120,
    religion_judaism_minutes: 45,
    religion_lutheran_minutes: 30,
  });
  assert.equal(religionAllocationTotal(values, true), 120);
  assert.equal(religionAllocationTotal(values, false), 75);
});

const nextTurn = () => new Promise((resolve) => setImmediate(resolve));
const leisurePreview = (overrides = {}) => calculateLeisurePreview({
  baselineFatigue: 600,
  currentFatigue: 0,
  fatiguePreviewDivisor: 100,
  laborFatigueRate: 50,
  laborMinutes: 0,
  leisureMinutes: 360,
  moraleLimit: 4,
  moraleScale: 200,
  recoveryRate: 100,
  ...overrides,
});

test("schedule time parser accepts clock and whole-hour forms", () => {
  assert.equal(parseClock("8"), 8 * 60);
  assert.equal(parseClock("08"), 8 * 60);
  assert.equal(parseClock("8:30"), 8 * 60 + 30);
  assert.equal(parseClock("08:30"), 8 * 60 + 30);
});

test("schedule time parser accepts compact three- and four-digit forms", () => {
  assert.equal(parseClock("830"), 8 * 60 + 30);
  assert.equal(parseClock("0830"), 8 * 60 + 30);
  assert.equal(parseClock("1230"), 12 * 60 + 30);
});

test("schedule time parser retains day bounds and minute validation", () => {
  assert.equal(parseClock("24"), 24 * 60);
  assert.equal(parseClock("2400"), 24 * 60);
  for (const invalid of ["", "25", "24:15", "2415", "8:60", "860", "12345", "noon"]) {
    assert.equal(parseClock(invalid), null, invalid);
  }
});

test("opened time editor steps drafts by quarter-hours within day bounds", () => {
  assert.equal(stepClockValue("08:00", 15), "08:15");
  assert.equal(stepClockValue("08:00", -15), "07:45");
  assert.equal(stepClockValue("24:00", 15), "24:00");
  assert.equal(stepClockValue("00:00", -15), "00:00");
  assert.equal(stepClockValue("invalid", 15, 120), "02:15");
});

test("Leisure preview preserves fatigue through six hours and then offsets activity", () => {
  assert.equal(leisurePreview({ leisureMinutes: 300 }).fatigueDelta, 100);
  assert.equal(leisurePreview({ leisureMinutes: 360 }).fatigueDelta, 0);
  const offset = leisurePreview({ laborMinutes: 240, leisureMinutes: 480 });
  assert.equal(offset.fatigueDelta, 0);
  assert.equal(offset.leisureFatigue, -2);
});

test("Leisure preview grants morale only after current and generated fatigue are gone", () => {
  const carried = leisurePreview({ currentFatigue: 200, leisureMinutes: 480 });
  assert.equal(carried.fatigueDelta, -200);
  assert.equal(carried.morale, 0);
  const partiallyCarried = leisurePreview({ currentFatigue: 100, leisureMinutes: 480 });
  const fatigueFree = leisurePreview({ leisureMinutes: 480 });
  assert.ok(Math.abs(partiallyCarried.morale - fatigueFree.morale / 2) < 0.0001);
  const surplus = leisurePreview({ leisureMinutes: 540 });
  assert.equal(surplus.fatigueDelta, 0);
  assert.ok(surplus.morale > 0 && surplus.morale < 4);
});

test("signed effects normalize values that display as negative zero", () => {
  assert.equal(signedEffect("fatigue", -0.0006), "0");
  assert.equal(signedEffect("fatigue", -0.04), "0");
  assert.equal(signedEffect("fatigue", -0.06), "-0.1");
});

test("schedule saves serialize requests and coalesce to the newest snapshot", async () => {
  const requests = [];
  const states = [];
  let drained = 0;
  const queue = createLatestSaveQueue(
    (snapshot) => new Promise((resolve, reject) => requests.push({ reject, resolve, snapshot })),
    {
      onState: (state) => states.push(state),
      onDrained: () => { drained += 1; },
    },
  );

  queue.stage("first");
  queue.flush();
  assert.deepEqual(requests.map(({ snapshot }) => snapshot), ["first"]);
  queue.stage("intermediate");
  queue.flush();
  queue.stage("newest");
  queue.flush();
  assert.equal(requests.length, 1, "only one request may be in flight");

  requests[0].resolve();
  await nextTurn();
  assert.deepEqual(requests.map(({ snapshot }) => snapshot), ["first", "newest"]);
  assert.equal(drained, 0, "the queue is not drained while the follow-up is pending");

  requests[1].resolve();
  await nextTurn();
  assert.equal(drained, 1);
  assert.equal(queue.status().dirty, false);
  assert.equal(states.at(-1).pending, false);
});

test("failed schedule saves retain pending optimistic state until a newer retry", async () => {
  const requests = [];
  let drained = 0;
  const queue = createLatestSaveQueue(
    (snapshot) => new Promise((resolve, reject) => requests.push({ reject, resolve, snapshot })),
    { onDrained: () => { drained += 1; } },
  );

  queue.stage("optimistic");
  queue.flush();
  requests[0].reject(new Error("offline"));
  await nextTurn();
  assert.equal(queue.status().dirty, true);
  assert.equal(queue.status().pending, true);
  assert.match(queue.status().error.message, /offline/);
  assert.equal(drained, 0);

  queue.stage("newest-after-retry");
  queue.flush();
  assert.equal(requests[1].snapshot, "newest-after-retry");
  requests[1].resolve();
  await nextTurn();
  assert.equal(queue.status().pending, false);
  assert.equal(drained, 1);
});

test("a flushed newer snapshot supersedes an older request that fails", async () => {
  const requests = [];
  let drained = 0;
  const queue = createLatestSaveQueue(
    (snapshot) => new Promise((resolve, reject) => requests.push({ reject, resolve, snapshot })),
    { onDrained: () => { drained += 1; } },
  );

  queue.stage("older-in-flight");
  queue.flush();
  queue.stage("newer-ready");
  queue.flush();
  requests[0].reject(new Error("older failed"));
  await nextTurn();

  assert.deepEqual(requests.map(({ snapshot }) => snapshot), ["older-in-flight", "newer-ready"]);
  assert.equal(queue.status().inFlight, true);
  assert.equal(queue.status().error, null);
  requests[1].resolve();
  await nextTurn();
  assert.equal(queue.status().pending, false);
  assert.equal(drained, 1);
});

test("an unflushed newer snapshot waits for its debounce after an older failure", async () => {
  const requests = [];
  const queue = createLatestSaveQueue(
    (snapshot) => new Promise((resolve, reject) => requests.push({ reject, resolve, snapshot })),
  );

  queue.stage("older-in-flight");
  queue.flush();
  queue.stage("newer-debouncing");
  requests[0].reject(new Error("older failed"));
  await nextTurn();
  assert.equal(requests.length, 1);
  assert.equal(queue.status().pending, true);

  queue.flush();
  assert.equal(requests[1].snapshot, "newer-debouncing");
  requests[1].resolve();
  await nextTurn();
  assert.equal(queue.status().pending, false);
});

test("retry reattempts the retained snapshot after a terminal failure", async () => {
  const requests = [];
  let drained = 0;
  const queue = createLatestSaveQueue(
    (snapshot) => new Promise((resolve, reject) => requests.push({ reject, resolve, snapshot })),
    { onDrained: () => { drained += 1; } },
  );

  queue.stage("retained");
  queue.flush();
  requests[0].reject(new Error("offline"));
  await nextTurn();
  queue.retry();
  assert.deepEqual(requests.map(({ snapshot }) => snapshot), ["retained", "retained"]);
  requests[1].resolve();
  await nextTurn();
  assert.equal(queue.status().pending, false);
  assert.equal(drained, 1);
});
