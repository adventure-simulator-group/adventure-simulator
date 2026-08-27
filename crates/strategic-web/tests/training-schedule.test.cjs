const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

globalThis.strategicCalendar = require("./strategic-calendar-fixture.cjs");

const {
  calculateLeisurePreview,
  createLatestSaveQueue,
  effectiveAllocation,
  parseClock,
  signedEffect,
  stepClockValue,
} = require("../static/training-schedule.js");

test("location mask preserves editable allocations and redistributes unavailable segments", () => {
  const saved = {
    carousing_minutes: 120,
    labor_minutes: 240,
    raiding_minutes: 180,
    thievery_minutes: 60,
  };
  const effective = effectiveAllocation(saved, [
    "carousing_minutes",
    "raiding_minutes",
  ], 42n);
  assert.deepEqual(saved, {
    carousing_minutes: 120,
    labor_minutes: 240,
    raiding_minutes: 180,
    thievery_minutes: 60,
  });
  assert.equal(effective.carousing_minutes, 0);
  assert.equal(effective.raiding_minutes, 0);
  assert.ok(effective.labor_minutes >= 240);
  assert.ok(effective.thievery_minutes >= 60);
  assert.equal(Object.values(effective).reduce((sum, value) => sum + value, 0), 600);
  const leisure = 1440 - Object.values(effective).reduce((sum, value) => sum + value, 0);
  assert.equal(leisure, 840);
});

test("location mask uses Leisure only when no planned activity is available", () => {
  const effective = effectiveAllocation(
    { carousing_minutes: 60, raiding_minutes: 120 },
    ["carousing_minutes", "raiding_minutes"],
    42n,
  );
  assert.deepEqual(effective, {
    carousing_minutes: 0,
    raiding_minutes: 0,
  });
  assert.equal(1440 - Object.values(effective).reduce((sum, value) => sum + value, 0), 1440);
});

test("weighted redistribution approaches the planned two-to-one ratio", () => {
  assert.deepEqual(
    effectiveAllocation(
      {
        combat_training_minutes: 60,
        prayer_minutes: 120,
        raiding_minutes: 90,
      },
      ["raiding_minutes"],
      42n,
    ),
    {
      combat_training_minutes: 75,
      prayer_minutes: 195,
      raiding_minutes: 0,
    },
  );

  let combat = 0;
  let prayer = 0;
  for (let seed = 0n; seed < 4000n; seed += 1n) {
    const effective = effectiveAllocation(
      {
        combat_training_minutes: 60,
        prayer_minutes: 120,
        raiding_minutes: 90,
      },
      ["raiding_minutes"],
      seed,
    );
    combat += effective.combat_training_minutes - 60;
    prayer += effective.prayer_minutes - 120;
  }
  assert.ok(prayer / combat > 1.9);
  assert.ok(prayer / combat < 2.1);
});

test("schedule editor contains only activity allocations", () => {
  const source = fs.readFileSync(
    path.join(__dirname, "../static/training-schedule.js"),
    "utf8",
  );
  for (const legacy of [
    "combat_auto_train",
    "melee_minutes",
    "ranged_minutes",
    "religion_auto_train",
    "religion_minutes_by_tradition",
  ]) {
    assert.equal(source.includes(legacy), false, legacy);
  }
});

test("skill family controls include Bestiary expansion", () => {
  const source = fs.readFileSync(
    path.join(__dirname, "../static/training-schedule.js"),
    "utf8",
  );
  assert.match(source, /\[data-bestiary-expand\]/);
  assert.match(source, /\.bestiary-detail-row/);
  assert.doesNotMatch(source, /\[data-surgery-expand\]/);
  assert.doesNotMatch(source, /\.surgery-detail-row/);
  assert.match(source, /aria-expanded/);
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
