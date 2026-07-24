const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const source = fs.readFileSync(path.join(__dirname, "..", "static", "journal-dialog.js"), "utf8");

test("journal opens as a progressively enhanced dialog", () => {
  assert.match(source, /querySelectorAll\("\[data-journal-open\]"\)/);
  assert.match(source, /dialog\.showModal\(\)/);
  assert.match(source, /strategicFetch\("\/quests"/);
  assert.match(source, /querySelector\("\[data-investigation-journal\]"\)/);
  assert.match(source, /event\.target === dialog/);
  assert.match(source, /activeOpener\?\.focus\(\)/);
});
