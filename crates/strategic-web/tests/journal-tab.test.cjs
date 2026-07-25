const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const source = fs.readFileSync(path.join(__dirname, "..", "static", "journal-tab.js"), "utf8");

test("journal swaps only the location side rails without navigating", () => {
  assert.match(source, /querySelector\("\[data-journal-tab\]"\)/);
  assert.match(source, /strategicFetch\("\/quests"/);
  assert.match(source, /querySelector\("\[data-journal-case-index\]"\)/);
  assert.match(source, /querySelector\("\[data-journal-case-log\]"\)/);
  assert.match(source, /originalLeft\.replaceWith\(journalLeft\)/);
  assert.match(source, /originalRight\.replaceWith\(journalRight\)/);
  assert.doesNotMatch(source, /location\.(?:href|assign|replace)/);
  assert.doesNotMatch(source, /showModal/);
});

test("journal restores the original rails and supports quest selection", () => {
  assert.match(source, /journalLeft\.replaceWith\(originalLeft\)/);
  assert.match(source, /journalRight\.replaceWith\(originalRight\)/);
  assert.match(source, /data-journal-case-select/);
  assert.match(source, /data-journal-case-panel/);
  assert.match(source, /event\.key === "Escape"/);
  assert.match(source, /aria-pressed/);
});
