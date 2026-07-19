const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const source = fs.readFileSync(
  path.join(__dirname, "..", "static", "service-quests.js"),
  "utf8",
);

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
