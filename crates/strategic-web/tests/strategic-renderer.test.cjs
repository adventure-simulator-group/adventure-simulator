const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const source = fs.readFileSync(path.join(__dirname, '..', 'static', 'strategic-renderer.js'), 'utf8');
const tacticalHtml = fs.readFileSync(path.join(__dirname, '..', '..', 'adventuresim-stdb-module', 'static', 'tactical.html'), 'utf8');
const buildScript = fs.readFileSync(path.join(__dirname, '..', '..', '..', 'scripts', 'build_wasm.sh'), 'utf8');

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
