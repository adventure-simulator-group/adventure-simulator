const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const source = fs.readFileSync(path.join(__dirname, "..", "static", "dialogue-client.js"), "utf8");

test("dialogue client is schema-driven with stable authoritative actions", () => {
  assert.match(source, /api\/dialogue\/start/);
  assert.match(source, /api\/dialogue\/topic/);
  assert.match(source, /api\/dialogue\/answer/);
  assert.match(source, /fragment\.kind === "text"/);
  assert.match(source, /fragment\.kind === "topic"/);
  assert.match(source, /prompt\.choices\.forEach/);
  assert.doesNotMatch(source, /professionDetails|openQuestOffer|beginHerbalistConversation|dialogueActions/);
});

test("known topics share the inline topic action contract", () => {
  assert.match(source, /conversation\.topics\.filter\(\(topic\) => topic\.initially_known\)/);
  assert.match(source, /item\.append\(topicAnchor/);
  assert.match(source, /row\.append\(topicAnchor/);
});
