const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { readRustModuleSource } = require("./rust-module-source.cjs");

const {
  wrappedFocusIndex, dialogOwnsBodyLock, openerIdentity, submitAutomaticChatToggle,
} = require("../static/character-action-dialog.js");
const socialTemplate = fs.readFileSync(path.join(__dirname, "../src/templates/settlement/social.rs"), "utf8");
const healthTemplate = fs.readFileSync(path.join(__dirname, "../src/templates/settlement/character_health.rs"), "utf8");
const tradeTemplate = fs.readFileSync(path.join(__dirname, "../src/templates/settlement/trade.rs"), "utf8");
const chromeTemplate = fs.readFileSync(path.join(__dirname, "../src/templates/settlement/chrome.rs"), "utf8");
const template = [socialTemplate, healthTemplate, tradeTemplate, chromeTemplate].join("\n");
const styles = fs.readFileSync(path.join(__dirname, "../static/css/strategic.css"), "utf8");
const components = fs.readFileSync(path.join(__dirname, "../static/css/components.css"), "utf8");
const routes = readRustModuleSource(path.join(__dirname, "../src/routes/settlements/mod.rs"));

test("encounter counterparties use durable characters and ordinary actions", () => {
  const worldActors = fs.readFileSync(path.join(__dirname, "../../adventuresim-stdb-module/src/world_actor.rs"), "utf8");
  const encounters = fs.readFileSync(path.join(__dirname, "../../adventuresim-stdb-module/src/strategic/encounters.rs"), "utf8");
  const surgery = fs.readFileSync(path.join(__dirname, "../../adventuresim-stdb-module/src/surgery.rs"), "utf8");
  const travel = fs.readFileSync(path.join(__dirname, "../src/templates/settlement/travel.rs"), "utf8");
  assert.match(worldActors, /CharacterContextMembership/);
  assert.match(worldActors, /apply_async_socializing/);
  assert.match(worldActors, /materialize_road_encounter_cast/);
  assert.match(worldActors, /Road cast retry found partial Character authority/);
  assert.match(worldActors, /deactivate_context_roster/);
  assert.match(worldActors, /row\.active = false/);
  assert.match(encounters, /context_character_ids/);
  assert.doesNotMatch(encounters, /u64::MAX\.saturating_sub\(index\)/);
  assert.equal((surgery.match(/contextual_treatment_decision/g) || []).length >= 2, true);
  assert.match(travel, /aria-label="Counterparty"/);
  assert.match(travel, /presentation\.cast/);
  assert.match(travel, /aria-label="Roadside characters"/);
  assert.match(travel, /\{ "Request" \}/);
  assert.match(travel, /"Emergency treatment"[\s\S]*"Request treatment"/);
  assert.doesNotMatch(travel, /challenge\.actor_character_id/);
});

test("character dialogs trap focus in either direction", () => {
  assert.equal(wrappedFocusIndex(3, 0, true), 2);
  assert.equal(wrappedFocusIndex(3, 2, false), 0);
  assert.equal(wrappedFocusIndex(3, -1, false), 0);
  assert.equal(wrappedFocusIndex(0, -1, false), -1);
});

test("remote dialog replacements retain body lock and stable opener identity", () => {
  assert.equal(dialogOwnsBodyLock(false, true), true);
  assert.equal(dialogOwnsBodyLock(false, false), false);
  assert.equal(openerIdentity({ dataset: { dialogOpener: "/party/2/social?building=inn" } }), "/party/2/social?building=inn");
  const lifecycle = fs.readFileSync(path.join(__dirname, "../static/character-action-dialog.js"), "utf8");
  assert.match(lifecycle, /restoreKey}-pending/);
});

test("automatic social preference submits immediately when the checkbox changes", () => {
  let submissions = 0;
  const input = {
    matches: (selector) => selector === "[data-automatic-social-chat]",
    form: { requestSubmit: () => { submissions += 1; } },
  };
  assert.equal(submitAutomaticChatToggle(input), true);
  assert.equal(submissions, 1);
  assert.equal(submitAutomaticChatToggle({ matches: () => false }), false);
});

test("modal character actions retain the raised-button contract while social uses the dock", () => {
  assert.match(template, /data-character-action-dialog/);
  assert.match(template, /aria-haspopup="dialog" aria-expanded=\(open\)/);
  assert.match(template, /character-menu-button limb-surgery-button/);
  assert.match(template, /role="dialog" aria-modal="true" aria-labelledby="surgery-dialog-title"/);
  assert.match(template, /data-social-conversation/);
  assert.doesNotMatch(socialTemplate, /aria-labelledby="social-dialog-title"/);
  assert.doesNotMatch(template, /cooking-dialog-title/);
  assert.match(template, /aria-label="Cook at fireplace"/);
  assert.match(styles, /\.character-menu-button[\s\S]*background: var\(--tactile-background\)/);
  assert.match(styles, /\.character-menu-button[\s\S]*box-shadow: var\(--tactile-shadow\)/);
  assert.match(styles, /\.character-menu-button:focus-visible[\s\S]*outline: 2px solid var\(--accent-light\)/);
  assert.match(styles, /\.character-menu-button:active,[\s\S]*box-shadow: var\(--tactile-shadow-pressed\)/);
  assert.match(components, /\.btn-primary[\s\S]*--tactile-face: color-mix\(in srgb, var\(--accent-dark\)/);
  assert.match(components, /\.btn-primary[\s\S]*color: #fff/);
  assert.match(styles, /\.btn:not\(\.btn-danger, \.btn-primary, \.btn-secondary\)/);
  assert.match(styles, /\.skill-schedule \.party-skill-name-column \{ width: 2\.75rem; \}/);
  assert.match(styles, /\.social-dialog \{ width: min\(40rem, 100%\); \}/);
});

test("social replaces the ordinary chat dock while surgery remains an overlay", () => {
  assert.match(routes, /render_party_personal\([\s\S]*Some\(dialog\)/);
  assert.match(routes, /render_party_stats\([\s\S]*Some\(dialog\)/);
  const socialStart = socialTemplate.indexOf("pub fn party_social_dialog");
  const socialEnd = socialTemplate.indexOf("pub(crate) fn settlement_chat_area", socialStart);
  const socialDialog = socialTemplate.slice(socialStart, socialEnd);
  const surgeryStart = healthTemplate.indexOf("pub fn surgery_dialog");
  const surgeryEnd = healthTemplate.indexOf("pub(super) fn strategic_condition_rail", surgeryStart);
  const surgeryDialog = healthTemplate.slice(surgeryStart, surgeryEnd);
  assert.doesNotMatch(socialDialog, /left-sidebar|right-sidebar|render_layout/);
  assert.match(socialDialog, /role="tablist"/);
  assert.match(socialDialog, /Recent Tidings/);
  assert.match(socialDialog, /class="settlement-chat-messages"/);
  assert.match(socialDialog, /data-local-chat-kind="player"/);
  assert.match(socialDialog, /data-strategic-tooltip=\(belief_tooltip\(belief\)\)/);
  assert.doesNotMatch(surgeryDialog, /left-sidebar|right-sidebar|render_layout/);
  assert.match(socialDialog, /preserve_building/);
  assert.match(surgeryDialog, /preserve_building/);
});

test("portrait tray keeps unrelated actions but drops the cooking launcher", () => {
  const start = template.indexOf("pub(crate) fn party_portrait_overlay");
  const end = template.indexOf("pub(crate) fn settlement_chat_area", start);
  const portrait = template.slice(start, end);
  assert.doesNotMatch(portrait, /party-cooking-action/);
  assert.match(portrait, /party-alchemy-action/);
  assert.match(portrait, /party-member-remove/);
  assert.match(portrait, /\/inventory/);
});
