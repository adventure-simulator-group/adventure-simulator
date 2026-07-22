const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const source = fs.readFileSync(path.join(__dirname, "..", "static", "dialogue-client.js"), "utf8");

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
  assert.match(source, /view\.topics\.forEach/);
  assert.match(source, /item\.append\(topicAnchor/);
  assert.match(source, /row\.append\(topicAnchor/);
});

test("client renders only persisted authoritative views and enforces prompt bounds", () => {
  assert.match(source, /view\.events\.forEach/);
  assert.match(source, /expected_revision:currentView\.revision/);
  assert.match(source, /choices\.length<prompt\.min_choices/);
});

test("private herbalist results retain medication focus without entering the catalog", () => {
  assert.match(source, /view\.examination/);
  assert.match(source, /data-dialogue-medication/);
  assert.match(source, /data-herbalist-medication-name/);
});
