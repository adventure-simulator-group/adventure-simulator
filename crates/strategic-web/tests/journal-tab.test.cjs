const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const source = fs.readFileSync(path.join(__dirname, "..", "static", "journal-tab.js"), "utf8");

test("journal swaps the location layout without navigating", () => {
  assert.match(source, /querySelector\("\[data-journal-tab\]"\)/);
  assert.match(source, /strategicFetch\("\/quests"/);
  assert.match(source, /querySelector\("\[data-journal-case-index\]"\)/);
  assert.match(source, /querySelector\("\[data-journal-case-log\]"\)/);
  assert.match(source, /querySelector\("\[data-journal-context\]"\)/);
  assert.match(source, /originalLeft\.replaceWith\(journalLeft\)/);
  assert.match(source, /originalCenter\.replaceWith\(journalCenter\)/);
  assert.match(source, /originalRight\.replaceWith\(journalRight\)/);
  assert.doesNotMatch(source, /location\.(?:href|assign|replace)/);
  assert.doesNotMatch(source, /showModal/);
});

test("journal restores the original rails and overlay selection targets the center log", () => {
  assert.match(source, /journalLeft\.replaceWith\(originalLeft\)/);
  assert.match(source, /journalCenter\.replaceWith\(originalCenter\)/);
  assert.match(source, /journalRight\.replaceWith\(originalRight\)/);
  assert.match(source, /data-journal-case-select/);
  assert.match(source, /data-journal-case-panel/);
  assert.match(source, /log: root\.querySelector\("\[data-journal-case-log\]"\)/);
  assert.match(source, /layout\.log\?\.querySelectorAll/);
  assert.match(source, /event\.key === "Escape"/);
  assert.match(source, /aria-pressed/);
  assert.match(source, /ArrowLeft/);
  assert.match(source, /ArrowRight/);
  assert.match(source, /event\.key === "Home"/);
  assert.match(source, /event\.key === "End"/);
  assert.match(source, /tab\.tabIndex = selected \? 0 : -1/);
});

test("direct journal page tabs use their owning layout without requiring overlay state", () => {
  assert.match(source, /const root = tab\.closest\("\.main-grid"\) \|\| document/);
  assert.match(source, /if \(caseTab\) \{/);
  assert.doesNotMatch(source, /if \(caseTab && state\)/);
  assert.match(source, /if \(!tab\) return/);
  assert.doesNotMatch(source, /if \(!tab \|\| !state\) return/);
  assert.match(source, /layout\.index\.querySelectorAll/);
});
