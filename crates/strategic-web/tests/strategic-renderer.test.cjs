const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const source = fs.readFileSync(path.join(__dirname, '..', 'static', 'strategic-renderer.js'), 'utf8');
const liveStateSource = fs.readFileSync(path.join(__dirname, '..', 'static', 'live-state.js'), 'utf8');
const tacticalHtml = fs.readFileSync(path.join(__dirname, '..', '..', 'adventuresim-stdb-module', 'static', 'tactical.html'), 'utf8');
const buildScript = fs.readFileSync(path.join(__dirname, '..', '..', '..', 'scripts', 'build_wasm.sh'), 'utf8');
const helpers = require('../static/strategic-renderer-helpers.js');

test('renderer loader preserves fallback on every unsupported/error path', () => {
  assert.match(source, /navigator\.gpu/);
  assert.match(source, /catch \(error\)/);
  assert.match(source, /canvas\.hidden = true/);
  assert.match(source, /classList\.add\('renderer-enhanced'\)/);
  assert.doesNotMatch(source, /querySelector\('\[data-renderer-fallback\]'\)\.hidden = true/);
});

test('renderer loader has lifecycle and one-start guards', () => {
  assert.match(source, /not\(\[data-render-started\]\)/);
  assert.match(source, /visibilitychange/);
  assert.match(source, /wasm_set_suspended/);
  assert.match(source, /wasm_set_suspended\(document\.hidden\)/);
});

test('loader imports the wasm-bindgen filename used by the tactical page and build', () => {
  const filename = 'adventuresim-tactical-client.js';
  assert.match(source, new RegExp(filename.replace('.', '\\.')));
  assert.match(tacticalHtml, new RegExp(filename.replace('.', '\\.')));
  assert.match(buildScript, /adventuresim-tactical-client\.wasm/);
});

test('compiled manifest/package loading falls back to the embedded package', () => {
  assert.match(source, /fetch\(config\.startup\.package_url/);
  assert.match(source, /wasm_validate_manifest/);
  assert.match(source, /fetch\(manifest\.package_url/);
  assert.match(source, /using embedded map package/);
});

function anchor(id, href) {
  const resolved = new URL(href, 'https://game.test/map').href;
  return {
    dataset: { mapMarkerId: id }, href: resolved,
    clicked: false,
    getAttribute(name) { return name === 'href' ? href : null; },
    click() { this.clicked = true; },
  };
}

test('marker selection clicks the exact same-origin canonical anchor', () => {
  const wrong = anchor('town-10', '/map?destination=town-10');
  const exact = anchor('town-1', '/map?destination=town-1');
  const root = { querySelectorAll: () => [wrong, exact] };
  const renderer = { wasm_take_marker_selection: () => 'town-1' };
  assert.equal(helpers.consumeMarkerSelection(renderer, root, 'https://game.test/map'), true);
  assert.equal(exact.clicked, true);
  assert.equal(wrong.clicked, false);
});

test('unknown, malformed, and hostile marker events are ignored', () => {
  for (const id of ['unknown', '', 'bad\nmarker', '//evil']) {
    const known = anchor('known', 'https://game.test/map?destination=known');
    const root = { querySelectorAll: () => [known] };
    assert.equal(helpers.consumeMarkerSelection({ wasm_take_marker_selection: () => id }, root, 'https://game.test/map'), false);
    assert.equal(known.clicked, false);
  }
  const hostile = anchor('known', 'https://evil.test/steal');
  assert.equal(helpers.canonicalMarkerAnchor('known', { querySelectorAll: () => [hostile] }, 'https://game.test/map'), null);
});

test('handoff interception is gated on a successfully enhanced renderer', () => {
  assert.match(source, /renderer-enhanced/);
  assert.match(source, /handoffInterceptReady/);
  assert.match(source, /headers: \{ Accept: 'application\/json' \}/);
  assert.match(source, /handoff_schema: 1/);
  assert.match(source, /createAttemptState/);
  assert.match(source, /pagehide/);
  assert.match(source, /MutationObserver/);
  assert.match(source, /signal/);
  assert.match(liveStateSource, /data-live-tactical-handoff/);
  assert.match(liveStateSource, /data-handoff-intercept-ready/);
});

test('allocation polling propagates one signal and uses capped exponential backoff', async () => {
  const controller = new AbortController();
  const outcomes = [
    { status: 'pending' },
    { status: 'pending' },
    { status: 'ready', player_id: 7, server_url: 'host:6000' },
  ];
  const signals = [];
  const delays = [];
  const ready = await helpers.pollForReady({
    signal: controller.signal,
    requestStatus: async (signal) => {
      signals.push(signal);
      return outcomes.shift();
    },
    wait: async (delay, signal) => {
      assert.equal(signal, controller.signal);
      delays.push(delay);
    },
    maximumDelay: 750,
  });
  assert.equal(ready.status, 'ready');
  assert.deepEqual(delays, [500, 750]);
  assert.ok(signals.every((signal) => signal === controller.signal));
});

test('ready handoff remains monitored through connected and canonical ended navigation', async () => {
  const controller = new AbortController();
  const phases = ['tactical_connecting', 'tactical_connected'];
  const statuses = [
    { status: 'ready', fallback_url: '/missions/m-1/status' },
    { status: 'ended', fallback_url: '/missions/m-1/status' },
  ];
  let connected = 0;
  let navigation = null;
  const result = await helpers.monitorTacticalMission({
    signal: controller.signal,
    rendererStatus: () => phases.shift() ?? 'tactical_connected',
    requestStatus: async (signal) => {
      assert.equal(signal, controller.signal);
      return statuses.shift();
    },
    wait: async (delay, signal) => {
      assert.equal(delay, 5_000);
      assert.equal(signal, controller.signal);
    },
    onConnected: () => { connected += 1; },
    onEnded: (outcome) => {
      navigation = helpers.canonicalNavigationPath(outcome.fallback_url, 'https://game.test/locations/quest/q-1');
    },
  });
  assert.equal(result, 'ended');
  assert.equal(connected, 1);
  assert.equal(navigation, '/missions/m-1/status');
  assert.equal(helpers.canonicalNavigationPath('//evil.test/x', 'https://game.test/'), null);
});

test('tactical failure resets the single active attempt and permits one retry', async () => {
  const snapshots = [];
  const attempt = helpers.createAttemptState((snapshot) => snapshots.push(snapshot));
  assert.equal(attempt.begin(), true);
  assert.equal(attempt.begin(), false);
  const firstSignal = attempt.snapshot().signal;
  attempt.markHandedOff();
  const result = await helpers.monitorTacticalMission({
    signal: firstSignal,
    rendererStatus: () => 'tactical_failed',
    requestStatus: async () => { throw new Error('status should not be polled after renderer failure'); },
    wait: async () => {},
    onFailed: () => attempt.reset(),
  });
  assert.equal(result, 'failed');
  assert.equal(firstSignal.aborted, true);
  assert.deepEqual(attempt.snapshot(), { active: false, handedOff: false, signal: null });
  assert.equal(attempt.begin(), true);
  assert.equal(attempt.begin(), false);
  assert.notEqual(attempt.snapshot().signal, firstSignal);
  assert.equal(snapshots.at(-1).active, true);
});

test('canceling an attempt aborts in-flight polling before another request', async () => {
  const attempt = helpers.createAttemptState(() => {});
  attempt.begin();
  const signal = attempt.snapshot().signal;
  attempt.reset();
  let requests = 0;
  await assert.rejects(
    helpers.pollForReady({
      signal,
      requestStatus: async () => { requests += 1; return { status: 'pending' }; },
      wait: async () => {},
    }),
    (error) => error.name === 'AbortError',
  );
  assert.equal(requests, 0);
});
