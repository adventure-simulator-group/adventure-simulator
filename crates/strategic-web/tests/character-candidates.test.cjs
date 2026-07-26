const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { createSeed, loadOrCreate, lockSubmit } = require("../static/character-candidates.js");

const cryptoStub = { getRandomValues(bytes) { bytes.forEach((_, i) => { bytes[i] = i; }); return bytes; } };
function storage(initial = null, broken = false) { let value = initial; return { getItem() { if (broken) throw Error(); return value; }, setItem(_, next) { if (broken) throw Error(); value = next; }, removeItem() { value = null; }, value: () => value }; }

test("secure seed is opaque 128-bit lowercase hex", () => assert.equal(createSeed(cryptoStub), "000102030405060708090a0b0c0d0e0f"));
test("candidate coordinates remain stable for the tab", () => { const s = storage(); const a = loadOrCreate(s, cryptoStub, 1); const b = loadOrCreate(s, { getRandomValues() { throw Error(); } }, 1); assert.deepEqual(a, b); });
test("corrupt and unavailable storage falls back to a fresh URL-carried seed", () => { assert.equal(loadOrCreate(storage("bad"), cryptoStub, 1).seed.length, 32); assert.equal(loadOrCreate(storage(null, true), cryptoStub, 1).version, 1); });
test("confirmation locks repeat submissions", () => { const button = { disabled: false }; const form = { dataset: {}, querySelectorAll() { return [button]; } }; assert.equal(lockSubmit(form), true); assert.equal(button.disabled, true); assert.equal(lockSubmit(form), false); });
test("age-first and candidate-only wrapping contracts remain explicit", () => {
  const template = fs.readFileSync(path.join(__dirname, "../src/templates/character.rs"), "utf8");
  const css = fs.readFileSync(path.join(__dirname, "../static/css/strategic.css"), "utf8");
  const client = fs.readFileSync(path.join(__dirname, "../static/character-candidates.js"), "utf8");
  assert.match(template, /data-candidate-age="young"/);
  assert.match(template, /data-candidate-age="adult"/);
  assert.match(template, /data-candidate-age="old"/);
  assert.match(template, /data-candidate-roster/);
  assert.match(css, /:has\(\[data-candidate-roster\]\) \.party-portrait-overlay/);
  const candidateRules = css.slice(css.indexOf(".center-content:has([data-candidate-roster]) .party-portrait-overlay"));
  assert.match(candidateRules, /position:\s*relative/);
  assert.match(candidateRules, /flex-wrap:\s*wrap/);
  assert.match(candidateRules, /overflow:\s*visible/);
  assert.match(candidateRules, /> \[data-party-portrait-members\][\s\S]*display:\s*flex/);
  const ordinaryMobile = css.slice(css.indexOf("@media (max-width: 768px)"));
  assert.match(ordinaryMobile, /\.party-portrait-overlay[\s\S]*overflow-x:\s*auto/);
  assert.match(client, /age=\$\{link\.dataset\.candidateAge\}/);
});
