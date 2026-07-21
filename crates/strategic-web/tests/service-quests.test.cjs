const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const source = fs.readFileSync(
  path.join(__dirname, "..", "static", "service-quests.js"),
  "utf8",
);
const { serviceQuestTabState } = require(path.join(__dirname, "..", "static", "service-quests.js"));

test("herbalist greeting offers a clickable examination without replacing quest dialogue", () => {
  assert.match(source, /Greetings, traveler, what brings you to my humble shop/);
  assert.match(source, /link\("Feeling ill", requestHerbalistExamination\)/);
  assert.match(source, /herbalist\/examination/);
  assert.match(source, /openRecruitmentOffer|openQuestOffer/);
  assert.match(source, /finished the work we discussed/);
});

test("herbalist result renderer consumes only canonical name fields", () => {
  const examinationBlock = source.slice(
    source.indexOf("const requestHerbalistExamination"),
    source.indexOf("const beginHerbalistConversation"),
  );
  assert.match(examinationBlock, /diagnosis\.disease_name/);
  assert.match(examinationBlock, /diagnosis\.medication_name/);
  for (const forbidden of ["symptom", "finding", "vital", "stage", "infection", "skill"])
    assert.doesNotMatch(examinationBlock, new RegExp(`diagnosis\\.${forbidden}`, "i"));
});

test("herbalist examination dialogue is visible but explicitly non-persisting", () => {
  assert.match(source, /const privateLine =[\s\S]*?persist: false/);
  assert.match(source, /row\.dataset\.privateDialogue = "true"/);
  const examinationBlock = source.slice(
    source.indexOf("const requestHerbalistExamination"),
    source.indexOf("const beginHerbalistConversation"),
  );
  assert.match(examinationBlock, /privateLine\("player", "You"/);
  assert.match(examinationBlock, /privateLine\("npc", "Herbalist", result\.message/);
  assert.match(examinationBlock, /privateLine\("npc", "Herbalist", recommendation\)/);
  assert.doesNotMatch(examinationBlock, /\bline\("(?:player|npc)"/);
});

test("active quest state drives the red Map tab marker", () => {
  assert.match(source, /\[data-map-quest-badge\]/);
  assert.match(source, /\/api\/active-quest-marker/);
  assert.match(source, /setMapQuestActive\(true, quest\.description\)/);
  assert.match(source, /setMapQuestActive\(false\)/);
  assert.match(source, /Map, active quest/);
  assert.match(source, /marker\.description/);
  assert.match(source, /queueStrategicInitialLoad\(refreshMapQuestMarker\)/);
  assert.match(source, /strategic-live-update", refreshMapQuestMarker/);
});

test("service quest badges keep tab names accessible and used dialogue links inert", () => {
  assert.match(source, /dataServiceLabel|dataset\.serviceLabel/);
  assert.match(source, /stateLabel \? `\$\{baseLabel\}, \$\{stateLabel\}` : baseLabel/);
  assert.match(source, /anchor\.setAttribute\("aria-disabled", "true"\)/);
  assert.match(source, /anchor\.removeAttribute\("href"\)/);
  assert.match(source, /data-close-service-role-inspection/);
});

test("service tab state is appended only for actionable quest states", () => {
  assert.equal(serviceQuestTabState([{ state: "ready" }]), "quest ready to report");
  assert.equal(serviceQuestTabState([{ state: "available" }]), "quest available");
  assert.equal(serviceQuestTabState([{ state: "recruiting" }]), "recruitment available");
  assert.equal(serviceQuestTabState([{ state: "underway" }]), null);
  assert.equal(serviceQuestTabState([]), null);
  assert.equal(
    serviceQuestTabState([{ state: "underway" }, { state: "available" }]),
    "quest available",
  );
});

test("every settlement service greets with a profession and can begin training", () => {
  for (const service of ["merchants", "weapons", "armor", "clothing", "herbalist", "inn", "religion"])
    assert.match(source, new RegExp(`${service}: \\{ label:`));
  assert.match(source, /Welcome! What can a humble/);
  assert.match(source, /openProfessionTopic/);
  assert.match(source, /link\("apprentice"/);
  assert.match(source, /link\("novice"/);
  assert.match(source, /professions\/\$\{encodeURIComponent\(serviceId\)\}\/apprenticeship/);
  assert.match(source, /method: "POST"/);
  assert.match(source, /privateLine\("player", "You", religious/);
  assert.match(source, /privateLine\("npc", speaker, result\.message/);
  for (const title of ["novice", "cleric", "teacher"])
    assert.match(source, new RegExp(title));
});
