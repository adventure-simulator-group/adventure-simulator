const assert = require("node:assert/strict");
const test = require("node:test");

const {
  createLatestSaveQueue,
  parseClock,
} = require("../static/training-schedule.js");

const nextTurn = () => new Promise((resolve) => setImmediate(resolve));

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
