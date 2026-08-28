const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const root = path.join(__dirname, "..");
const script = fs.readFileSync(path.join(root, "static", "developer-mode.js"), "utf8");
const dialogue = fs.readFileSync(path.join(root, "static", "dialogue-client.js"), "utf8");
const layoutCss = fs.readFileSync(path.join(root, "static", "css", "layout.css"), "utf8");
const componentsCss = fs.readFileSync(path.join(root, "static", "css", "components.css"), "utf8");
const strategicCss = fs.readFileSync(path.join(root, "static", "css", "strategic.css"), "utf8");
const layout = fs.readFileSync(path.join(root, "src", "templates", "layout.rs"), "utf8");
const settlement = fs.readFileSync(path.join(root, "src", "templates", "settlement", "social.rs"), "utf8");
const rest = fs.readFileSync(path.join(root, "src", "templates", "settlement", "rest.rs"), "utf8");

test("developer mode is persisted off by default and enables explicit developer inputs", () => {
  assert.match(script, /localStorage\.getItem\(STORAGE_KEY\) === "on"/);
  assert.match(script, /link\.hidden = !enabled/);
  assert.match(script, /input\.disabled = !enabled/);
  assert.match(script, /new MutationObserver/);
  assert.doesNotMatch(rest, /advance_development_clock/);
  assert.match(dialogue, /target = "_blank"/);
  assert.match(dialogue, /noopener noreferrer/);
  assert.match(dialogue, /dialogue-source-icon/);
  assert.doesNotMatch(dialogue, /link\.textContent = "Edit"/);
  assert.match(layoutCss, /\.dialogue-source-link[\s\S]*border-radius: 50%/);
  assert.match(layoutCss, /\.dialogue-source-link\[hidden\] \{ display: none; \}/);
  assert.match(layoutCss, /\.dialogue-source-icon[\s\S]*hammer-nails\.svg/);
  assert.match(strategicCss, /\.inventory-source-link[\s\S]*\.inventory-source-icon/);
  assert.match(strategicCss, /\.inventory-source-icon[\s\S]*hammer-nails\.svg/);
});

test("toggle is emitted immediately before every character portrait menu", () => {
  const helper = layout.slice(layout.indexOf("fn character_switcher"));
  assert.ok(helper.indexOf("data-developer-mode-toggle") < helper.indexOf('details class="character-switcher"'));
  assert.match(helper, /aria-label="Enable developer mode"/);
  assert.match(helper, /aria-pressed="false"/);
});

test("developer-only location details stay hidden until developer mode is enabled", () => {
  const hidden = ".location-stat-list > div[data-developer-only] { display: none; }";
  const revealed = "html[data-developer-mode] .location-stat-list > div[data-developer-only] { display: flex; }";
  assert.ok(componentsCss.indexOf(hidden) >= 0);
  assert.ok(componentsCss.indexOf(hidden) < componentsCss.indexOf(revealed));
  assert.ok(layout.indexOf("css/layout.css") < layout.indexOf("css/components.css"));
});

test("dialogue catalog revision is server generated without a hard-coded source URL", () => {
  assert.doesNotMatch(settlement, /data-dialogue-source-url/);
  assert.match(settlement, /CATALOG_DIGEST/);
});
