const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const { wrappedFocusIndex } = require("../static/character-action-dialog.js");
const template = fs.readFileSync(path.join(__dirname, "../src/templates/settlement.rs"), "utf8");
const styles = fs.readFileSync(path.join(__dirname, "../static/css/strategic.css"), "utf8");

test("character dialogs trap focus in either direction", () => {
  assert.equal(wrappedFocusIndex(3, 0, true), 2);
  assert.equal(wrappedFocusIndex(3, 2, false), 0);
  assert.equal(wrappedFocusIndex(3, -1, false), 0);
  assert.equal(wrappedFocusIndex(0, -1, false), -1);
});

test("character actions use one dialog and raised-button contract", () => {
  assert.match(template, /data-character-action-dialog/);
  assert.match(template, /aria-haspopup="dialog" aria-expanded=\(open\)/);
  assert.match(template, /character-menu-button limb-surgery-button/);
  assert.match(template, /role="dialog" aria-modal="true" aria-labelledby="surgery-dialog-title"/);
  assert.match(template, /role="dialog" aria-modal="true" aria-labelledby="social-dialog-title"/);
  assert.match(template, /role="dialog" aria-modal="true" aria-labelledby="cooking-dialog-title"/);
  assert.match(styles, /border-color: #eee #353535 #353535 #eee/);
  assert.match(styles, /border-color: #353535 #eee #eee #353535/);
});

test("portrait tray keeps unrelated actions but drops cooking and examination launchers", () => {
  const start = template.indexOf("pub(crate) fn party_portrait_overlay");
  const end = template.indexOf("pub(crate) fn settlement_chat_area", start);
  const portrait = template.slice(start, end);
  assert.doesNotMatch(portrait, /party-cooking-action/);
  assert.doesNotMatch(portrait, /party-medical-examine/);
  assert.match(portrait, /party-alchemy-action/);
  assert.match(portrait, /party-member-remove/);
  assert.match(portrait, /\/inventory/);
});
