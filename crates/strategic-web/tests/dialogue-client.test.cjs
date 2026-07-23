const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const source = fs.readFileSync(path.join(__dirname, "..", "static", "dialogue-client.js"), "utf8");
const { dialogueCompletion, dialogueSubmission } = require("../static/dialogue-client.js");

test("dialogue client is schema-driven with stable authoritative actions", () => {
  assert.match(source, /api\/dialogue\/start/);
  assert.match(source, /api\/dialogue\/topic/);
  assert.match(source, /api\/dialogue\/answer/);
  assert.doesNotMatch(source, /api\/dialogue\/catalog/);
  assert.match(source, /fragment\.kind === "text"/);
  assert.match(source, /fragment\.kind === "topic"/);
  assert.match(source, /prompt\.choices\.forEach/);
  assert.doesNotMatch(source, /professionDetails|openQuestOffer|beginHerbalistConversation|dialogueActions/);
});

test("known topics share the inline topic action contract", () => {
  assert.match(source, /data-dialogue-topic-pane/);
  assert.match(source, /topicList\.replaceChildren/);
  assert.match(source, /row\.append\(topicAnchor/);
  assert.doesNotMatch(source, /querySelector\("\.right-sidebar"\)/);
});

test("client renders only persisted authoritative views and enforces prompt bounds", () => {
  assert.match(source, /view\.events\.forEach/);
  assert.match(source, /expected_revision:\s*currentView\.revision/);
  assert.match(source, /choices\.length\s*<\s*prompt\.min_choices/);
});

test("private herbalist results retain medication focus without entering the catalog", () => {
  assert.match(source, /view\.examination/);
  assert.match(source, /data-dialogue-medication/);
  assert.match(source, /data-herbalist-medication-name/);
});

test("settlement NPC selection is accessible and actor-backed", () => {
  assert.match(source, /api\/settlements\/\$\{encodeURIComponent\(npcStrip\.dataset\.npcSettlement\)\}/);
  assert.match(source, /setAttribute\("aria-label", `Talk to/);
  assert.match(source, /aria-pressed/);
  assert.match(source, /ArrowLeft/);
  assert.match(source, /ArrowRight/);
  assert.match(source, /chat\.dataset\.localChatSubject = npc\.id/);
  assert.match(source, /npcDescription\.replaceChildren/);
});

test("unique topic prefixes complete while ambiguous prefixes do not", () => {
  const topics = [
    { id: "profession", label: "Profession" },
    { id: "provisions", label: "Provisions" },
    { id: "apprenticeship", label: "Apprenticeship" },
  ];
  assert.equal(dialogueCompletion("app", topics), "Apprenticeship");
  assert.equal(dialogueCompletion("pro", topics), null);
  assert.equal(dialogueCompletion("Profession", topics), null);
  assert.deepEqual(dialogueSubmission("profession", topics), ["profession"]);
  assert.equal(dialogueSubmission("prof", topics), null);
});

test("typed prompt answers support yes/no and comma-separated multiple choices", () => {
  const choices = [
    { id: "yes", label: "Yes" },
    { id: "no", label: "No" },
    { id: "later", label: "Ask me later" },
  ];
  assert.equal(dialogueCompletion("y", choices), "Yes");
  assert.deepEqual(dialogueSubmission("YES", choices), ["yes"]);
  assert.equal(dialogueCompletion("Yes, a", choices, true), "Yes, Ask me later");
  assert.deepEqual(dialogueSubmission("Yes, ask me later", choices, true), ["yes", "later"]);
  assert.equal(dialogueSubmission("Yes, yes", choices, true), null);
});
