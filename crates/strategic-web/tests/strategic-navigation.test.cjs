const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

test("soft navigation declares the negotiated, lifecycle, history, and cancellation contract", () => {
  const source = fs.readFileSync("crates/strategic-web/static/strategic-navigation.js", "utf8");
  for (const token of [
    "X-Strategic-Navigation", "strategic-page-unmounting", "strategic-page-mounted",
    "strategic-soft-navigation-start", "strategic-soft-navigation-complete",
    "AbortController", "popstate", "strategicScroll", "aria-busy", "response.url",
  ]) assert.ok(source.includes(token), token);
});

test("hard navigation boundaries and native anchor affordances remain explicit", () => {
  const source = fs.readFileSync("crates/strategic-web/static/strategic-navigation.js", "utf8");
  for (const token of [
    "download", "hardNavigation", "_self", "/characters", "/map/data-license",
    "/missions/", "/tactical/", "event.metaKey", "event.ctrlKey", "raw.startsWith(\"#\")",
  ]) assert.ok(source.includes(token), token);
});

test("page lifecycle resets permanent services and remounts idempotent modules", () => {
  const read = (name) => fs.readFileSync(`crates/strategic-web/static/${name}.js`, "utf8");
  for (const name of [
    "background-fetch", "building-state", "cooking", "dialogue-client",
    "inventory-browser", "live-regions", "local-chat",
    "party-notifications", "party-recruitment", "physical-evidence",
    "rest-duration", "service-quests", "strategic-map", "strategic-time",
    "training-schedule", "travel-planner", "chat-resize",
  ]) {
    assert.match(read(name), /strategic-page-mounted/, `${name} remount`);
  }
  assert.match(read("background-fetch"), /leavingPage = false/);
  assert.match(read("live-regions"), /navigating = false/);
  assert.match(read("building-state"), /observer\?\.disconnect/);
  assert.match(read("local-chat"), /lifecycle\?\.abort/);
  assert.match(read("journal-tab"), /strategic-page-unmounting/);
  assert.match(read("character-action-dialog"), /const mount = \(\) =>/);
  assert.match(read("character-action-dialog"), /const unmount = \(\) =>/);
  assert.match(read("character-action-dialog"), /strategic-page-mounted/);
  assert.match(read("character-action-dialog"), /\{ signal \}/);
  assert.match(read("party-recruitment"), /strategic-page-unmounting/);
  assert.match(read("party-recruitment"), /!overlay\.isConnected/);
  const resize = read("chat-resize");
  for (const token of [
    "--chat-height", "--chat-panel-height", "CHAT_BOTTOM_GAP",
    "chat-resizing", "is-resizing", "setPointerCapture",
  ]) assert.ok(resize.includes(token), `chat resize ${token}`);
});

test("negotiated mutations cannot follow a redirect with a hidden GET", () => {
  const source = fs.readFileSync("crates/strategic-web/static/strategic-mutations.js", "utf8");
  assert.match(source, /method: "POST"/);
  assert.match(source, /"X-Strategic-Navigation": "true"/);
  assert.match(source, /redirect: "error"/);
  assert.match(source, /strategicCommitPage/);
  assert.match(source, /X-Strategic-Hard-Navigation/);
  assert.match(source, /\["http:", "https:"\]\.includes\(target\.protocol\)/);
  assert.match(source, /target\.origin !== location\.origin/);
  assert.match(source, /unsafe navigation target/);
  assert.match(source, /originPage !== document\.querySelector/);
  assert.doesNotMatch(source, /strategic-live-refresh-requested/);
});

test("negotiated navigation validates root metadata and never evaluates scripts", () => {
  const source = fs.readFileSync("crates/strategic-web/static/strategic-navigation.js", "utf8");
  assert.match(source, /X-Strategic-Response/);
  assert.match(source, /dataset\.scriptProfile/);
  assert.match(source, /querySelector\("#strategic-page"\)/);
  assert.doesNotMatch(source, /\beval\(|new Function|createElement\(["']script/);
  assert.match(source, /profile !== "strategic"/);
  assert.match(source, /strategicBoundaryUrl/);
  assert.match(source, /kind: "service"/);
  assert.match(source, /kind: "control"/);
  assert.match(source, /kind: "equipment"/);
  assert.match(source, /restoreFocus\(replacement, restore\.strategicFocus\)/);
});
