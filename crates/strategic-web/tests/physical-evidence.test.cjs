const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const assert = require("node:assert/strict");

const source = fs.readFileSync(
  path.join(__dirname, "..", "static", "physical-evidence.js"),
  "utf8",
);

test("physical evidence uses circular counterparty portraits and italic narration", () => {
  assert.match(source, /party-portrait settlement-npc-portrait physical-evidence-portrait/);
  assert.match(source, /document\.createElement\("em"\)/);
  assert.match(source, /data-evidence-topic/);
});

test("inspection sends only opaque evidence and topic choices", () => {
  assert.match(source, /evidence_id: topic\.dataset\.evidenceId/);
  assert.match(source, /topic_id: topic\.dataset\.evidenceTopic/);
  assert.doesNotMatch(source, /difficulty_milli|difficulty_bps|canonical/);
});

test("inspection topics remain available for deterministic retries", () => {
  assert.doesNotMatch(source, /\.disabled\s*=\s*true.*evidence-topic/s);
  assert.match(source, /item\.topics\.forEach/);
});

test("Bestiary results expose qualitative monster candidates and provenance", () => {
  assert.match(source, /Possible monster kinds:/);
  assert.match(source, /result\.monster_kind/);
  assert.match(source, /result\.support_band/);
  assert.match(source, /result\.provenance/);
  assert.doesNotMatch(source, /support_bps|percent|bestiaryColor|canonical|difficulty/);
});
