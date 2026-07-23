const test = require("node:test");
const assert = require("node:assert/strict");
const { createSeed, loadOrCreate, lockSubmit, restoreCandidateFocus } = require("../static/character-candidates.js");

const cryptoStub = { getRandomValues(bytes) { bytes.forEach((_, i) => { bytes[i] = i; }); return bytes; } };
function storage(initial = null, broken = false) { let value = initial; return { getItem() { if (broken) throw Error(); return value; }, setItem(_, next) { if (broken) throw Error(); value = next; }, removeItem() { value = null; }, value: () => value }; }

test("secure seed is opaque 128-bit lowercase hex", () => assert.equal(createSeed(cryptoStub), "000102030405060708090a0b0c0d0e0f"));
test("candidate coordinates remain stable for the tab", () => { const s = storage(); const a = loadOrCreate(s, cryptoStub, 1); const b = loadOrCreate(s, { getRandomValues() { throw Error(); } }, 1); assert.deepEqual(a, b); });
test("corrupt and unavailable storage falls back to a fresh URL-carried seed", () => { assert.equal(loadOrCreate(storage("bad"), cryptoStub, 1).seed.length, 32); assert.equal(loadOrCreate(storage(null, true), cryptoStub, 1).version, 1); });
test("confirmation locks repeat submissions", () => { const button = { disabled: false }; const form = { dataset: {}, querySelectorAll() { return [button]; } }; assert.equal(lockSubmit(form), true); assert.equal(button.disabled, true); assert.equal(lockSubmit(form), false); });
test("modal-free roster restores focus to the portrait that opened the dialog", () => { const s = storage("3"); let focused = false; const document = { querySelector(selector) { assert.equal(selector, '[data-candidate-slot="3"]'); return { focus() { focused = true; } }; } }; assert.equal(restoreCandidateFocus(document, s), true); assert.equal(focused, true); assert.equal(s.value(), null); });
