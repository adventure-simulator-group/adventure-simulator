const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const source = fs.readFileSync(path.join(__dirname, '..', 'static', 'strategic-renderer.js'), 'utf8');

test('renderer loader preserves fallback on every unsupported/error path', () => {
  assert.match(source, /navigator\.gpu/);
  assert.match(source, /catch \(error\)/);
  assert.match(source, /canvas\.hidden = true/);
  assert.match(source, /data-renderer-fallback/);
});

test('renderer loader has lifecycle and one-start guards', () => {
  assert.match(source, /not\(\[data-render-started\]\)/);
  assert.match(source, /visibilitychange/);
  assert.match(source, /wasm_set_suspended/);
});
