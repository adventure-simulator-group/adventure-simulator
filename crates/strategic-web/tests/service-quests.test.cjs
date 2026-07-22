const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const file = path.join(__dirname, "..", "static", "service-quests.js");
const source = fs.readFileSync(file, "utf8");
const { serviceQuestTabState } = require(file);

test("service quest script only owns notification state", () => {
  assert.doesNotMatch(source, /dialogueActions|nextDialogueActionId|createDocumentFragment|chat-npc-message/);
  assert.doesNotMatch(source, /beginHerbalistConversation|openProfessionTopic|openQuestOffer/);
  assert.match(source, /data-service-quest-badge/);
  assert.match(source, /api\/active-quest-marker/);
});

test("service tab state is appended only for actionable quest states", () => {
  assert.equal(serviceQuestTabState([{ state: "ready" }]), "quest ready to report");
  assert.equal(serviceQuestTabState([{ state: "available" }]), "quest available");
  assert.equal(serviceQuestTabState([{ state: "recruiting" }]), "recruitment available");
  assert.equal(serviceQuestTabState([{ state: "underway" }]), null);
});
