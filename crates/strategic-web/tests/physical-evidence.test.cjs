const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const assert = require("node:assert/strict");
const vm = require("node:vm");

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

test("Bestiary results use exact accessible red yellow green anchors", () => {
  assert.match(source, /Bestiary check\(s\) succeeded:/);
  assert.match(source, /chip\.tabIndex = 0/);
  assert.match(source, /chip\.setAttribute\("aria-label", accessible\)/);
  assert.match(source, /chip\.dataset\.strategicTooltip = result\.label/);
  assert.match(source, /chip\.dataset\.bestiaryEnemies = JSON\.stringify\(result\.enemies \|\| \[\]\)/);
  assert.match(source, /chip\.dataset\.tooltipPinnable = ""/);
  assert.match(source, /chip\.setAttribute\("aria-pressed", "false"\)/);
  assert.doesNotMatch(source, /bestiary-result-tooltip/);
  assert.match(source, /bounded <= 5000/);
  assert.match(source, /255 \* bounded \/ 5000/);
  assert.match(source, /255 \* \(10000 - bounded\) \/ 5000/);
  assert.doesNotMatch(source, /Typical signs|Common strengths|Confirmed combat mechanics|Folklore/);

  const colorFunction = source.match(
    /const bestiaryColor = \(supportBps\) => \{[\s\S]*?^  \};/m,
  );
  assert.ok(colorFunction, "Bestiary color function is present");
  const evaluateColor = vm.runInNewContext(
    `(${colorFunction[0].replace("const bestiaryColor = ", "").replace(/;$/, "")})`,
  );
  assert.equal(evaluateColor(0), "rgb(255 0 0)");
  assert.equal(evaluateColor(5000), "rgb(255 255 0)");
  assert.equal(evaluateColor(10000), "rgb(0 255 0)");
});
