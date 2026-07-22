const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const root = path.join(__dirname, "..");
const script = fs.readFileSync(path.join(root, "static", "developer-mode.js"), "utf8");
const dialogue = fs.readFileSync(path.join(root, "static", "dialogue-client.js"), "utf8");
const layout = fs.readFileSync(path.join(root, "src", "templates", "layout.rs"), "utf8");
const settlement = fs.readFileSync(path.join(root, "src", "templates", "settlement.rs"), "utf8");

test("developer mode is persisted off by default and controls only source links", () => {
  assert.match(script, /localStorage\.getItem\(STORAGE_KEY\) === "on"/);
  assert.match(script, /link\.hidden = !enabled/);
  assert.match(dialogue, /target = "_blank"/);
  assert.match(dialogue, /noopener noreferrer/);
});

test("toggle is emitted immediately before every character portrait menu", () => {
  const helper = layout.slice(layout.indexOf("fn character_switcher"));
  assert.ok(helper.indexOf("data-developer-mode-toggle") < helper.indexOf('details class="character-switcher"'));
  assert.match(helper, /aria-label="Enable developer mode"/);
  assert.match(helper, /aria-pressed="false"/);
});

test("dialogue catalog revision is server generated without a hard-coded source URL", () => {
  assert.doesNotMatch(settlement, /data-dialogue-source-url/);
  assert.match(settlement, /CATALOG_DIGEST/);
});
