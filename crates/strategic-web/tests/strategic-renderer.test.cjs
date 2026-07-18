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
  assert.match(source, /let busy = false/);
  assert.match(liveStateSource, /data-live-tactical-handoff/);
  assert.match(liveStateSource, /data-handoff-intercept-ready/);
});
