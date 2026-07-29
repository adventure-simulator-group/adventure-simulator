const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const file = path.join(__dirname, "..", "static", "service-quests.js");
const source = fs.readFileSync(file, "utf8");

test("service quest script owns trusted recruitment inspection without quest-marker UI", () => {
  assert.doesNotMatch(source, /dialogueActions|nextDialogueActionId|createDocumentFragment|chat-npc-message/);
  assert.doesNotMatch(source, /beginHerbalistConversation|openProfessionTopic|openQuestOffer/);
  assert.doesNotMatch(source, /data-service-quest-badge/);
  assert.doesNotMatch(source, /api\/active-quest-marker/);
  assert.match(source, /data-service-recruitment/);
  assert.match(source, /role\.left_html/);
  assert.match(source, /role\.right_html/);
  assert.match(source, /activity\.recruitment/);
  assert.match(source, /recruiting\.leader_id/);
  assert.doesNotMatch(source, /quest\.recruitment/);
});
