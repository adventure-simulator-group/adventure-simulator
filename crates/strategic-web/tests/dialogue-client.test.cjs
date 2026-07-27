const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const source = fs.readFileSync(path.join(__dirname, "..", "static", "dialogue-client.js"), "utf8");
const {
  dialogueCompletion,
  dialogueResponseIsCurrent,
  dialogueSubmission,
  dialogueTopicPayload,
} = require("../static/dialogue-client.js");

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

test("topics are exposed only through inline dialogue text", () => {
  assert.doesNotMatch(source, /data-dialogue-topic-pane/);
  assert.doesNotMatch(source, /topicList\.replaceChildren/);
  assert.match(source, /row\.append\(topicAnchor/);
  assert.match(source, /document\.createDocumentFragment\(\)/);
  assert.match(source, /fragment\.append\(anchor, edit\)/);
  assert.doesNotMatch(source, /anchor\.append\(edit\)/);
  assert.doesNotMatch(source, /querySelector\("\.right-sidebar"\)/);
});

test("client renders only persisted authoritative views and enforces prompt bounds", () => {
  assert.match(source, /view\.events\.forEach/);
  assert.match(source, /expected_revision:\s*binding\.revision/);
  assert.match(source, /choices\.length\s*<\s*prompt\.min_choices/);
});

test("witness social controls use observer-safe projections and authoritative routes", () => {
  assert.match(source, /view\.witness_social/);
  assert.doesNotMatch(source, /api\/dialogue\/spend-time/);
  assert.doesNotMatch(source, /Spend time \(30 min\)/);
  assert.match(source, /api\/dialogue\/insight/);
  assert.match(source, /api\/dialogue\/approach/);
  assert.match(source, /expected_revision:\s*binding\.revision/);
  assert.match(source, /possible pressure, though it may have nothing to do with this account/);
  assert.doesNotMatch(source, /Relationship:|Familiarity:|Demeanor:/);
  assert.doesNotMatch(source, /has_bound_concern|diagnosis_correct|success_chance|target_transparency/);
  assert.match(source, /visibleLabel\.textContent = `\$\{label\} \(5 min\)`/);
  assert.doesNotMatch(source, /\(10 min\)|\(30 min\)/);
  assert.match(source, /icon: "awareness"/);
  assert.match(source, /icon: "human-ear"/);
  assert.match(source, /icon: "rose"/);
  assert.match(source, /icon: "crown"/);
  assert.match(source, /icon: "conversation"/);
  assert.match(source, /iconHelp\.dataset\.strategicTooltip = description/);
  assert.match(source, /control\.dataset\.strategicTooltip = description/);
  assert.match(source, /`\$\{label\}\. \$\{description\} Takes 5 minutes\.`/);
});

test("dialogue does not expose the removed diagnosis and medication examination flow", () => {
  assert.doesNotMatch(source, /view\.examination/);
  assert.doesNotMatch(source, /data-dialogue-medication/);
  assert.doesNotMatch(source, /data-herbalist-medication-name/);
});

test("settlement NPC selection is accessible and actor-backed", () => {
  assert.match(source, /api\/settlements\/\$\{encodeURIComponent\(npcStrip\.dataset\.npcSettlement\)\}/);
  assert.match(source, /setAttribute\("aria-label", `Talk to/);
  assert.match(source, /aria-pressed/);
  assert.match(source, /ArrowLeft/);
  assert.match(source, /ArrowRight/);
  assert.match(source, /chat\.dataset\.localChatSubject = npc\.id/);
  assert.match(source, /npcDescription\.replaceChildren/);
  assert.match(source, /selectionGeneration/);
  assert.match(source, /local-chat-subject-changed/);
  assert.match(source, /generation === selectionGeneration/);
});

test("settlement NPCs reuse the circular party portrait structure", () => {
  assert.match(source, /party-portrait settlement-npc-portrait/);
  assert.match(source, /party-portrait-initial settlement-npc-initials/);
  assert.match(source, /party-portrait-face/);
  assert.match(source, /party-portrait-name settlement-npc-name/);
  assert.match(source, /portrait\.append\(face, name\)/);
  assert.match(source, /data\.openNpcSocial|dataset\.openNpcSocial/);
  assert.match(source, /settlement-npc-social-button/);
  assert.match(source, /npc-social-summary/);
});

test("NPCs without initials use the neutral person silhouette without losing their accessible name", () => {
  const css = fs.readFileSync(path.join(__dirname, "../static/css/strategic.css"), "utf8");
  assert.match(source, /npc\.initials \? "party-portrait-face" : "party-portrait-face npc-portrait-silhouette"/);
  assert.match(source, /face\.setAttribute\("aria-hidden", "true"\)/);
  assert.match(source, /button\.setAttribute\("aria-label", `Talk to \$\{npc\.name\}`\)/);
  assert.doesNotMatch(source, /npc\.initials \|\| "\?"/);
  assert.match(css, /\.npc-portrait-silhouette::before[\s\S]*person\.svg/);
});

test("late dialogue responses cannot replace the newly selected NPC", () => {
  assert.match(source, /const actor = chat\.dataset\.localChatSubject/);
  assert.match(source, /chat\.dataset\.localChatSubject === actor/);
  assert.match(source, /dialogueResponseIsCurrent\(/);
  assert.match(source, /binding\.selectionGeneration === currentGeneration/);
  assert.match(source, /binding\.npcId === currentNpcId/);
  assert.match(source, /binding\.sessionId === currentView\?\.session_id/);
});

test("late topic responses and errors cannot supersede a newer same-session revision", () => {
  const topic = {
    sessionId: "dialogue:7:witness",
    topicId: "referred-testimony",
    revision: 2,
    selectionGeneration: 4,
    npcId: "npc:town:inn:1",
  };
  assert.equal(
    dialogueResponseIsCurrent(
      topic,
      4,
      "npc:town:inn:1",
      { session_id: topic.sessionId, revision: 2 },
    ),
    true,
  );
  assert.equal(
    dialogueResponseIsCurrent(
      topic,
      4,
      "npc:town:inn:1",
      { session_id: topic.sessionId, revision: 3 },
    ),
    false,
  );
  const topicHandler = source
    .split("const chooseTopic =")
    .at(1)
    .split("const answerPrompt =")
    .at(0);
  assert.match(topicHandler, /dialogueResponseIsCurrent\(/);
  assert.match(topicHandler, /\.then\([\s\S]*dialogueResponseIsCurrent/);
  assert.match(topicHandler, /\.catch\([\s\S]*dialogueResponseIsCurrent/);
});

test("rapid provider-to-witness selection rejects stale topics and binds the witness session", () => {
  const hans = {
    sessionId: "dialogue:7:hans",
    topicId: "referred-testimony",
    revision: 0,
    selectionGeneration: 1,
    npcId: "npc:town:inn:0",
  };
  const agnes = {
    sessionId: "dialogue:7:agnes",
    topicId: "referred-testimony",
    revision: 2,
    selectionGeneration: 2,
    npcId: "npc:town:inn:1",
  };
  assert.equal(
    dialogueTopicPayload(hans, 2, "npc:town:inn:1", "stale-action"),
    null,
  );
  assert.deepEqual(
    dialogueTopicPayload(agnes, 2, "npc:town:inn:1", "agnes-action"),
    {
      session_id: "dialogue:7:agnes",
      topic_id: "referred-testimony",
      action_id: "agnes-action",
      expected_revision: 2,
    },
  );
  assert.doesNotMatch(source, /topicList\?\.replaceChildren\(\)/);
  assert.doesNotMatch(source, /if \(topicPane\) topicPane\.hidden = true/);
});

test("a delayed prompt answer cannot supersede a newly selected witness", () => {
  const providerAnswer = {
    sessionId: "dialogue:7:provider",
    revision: 3,
    selectionGeneration: 1,
    npcId: "npc:town:inn:0",
  };
  assert.equal(
    dialogueResponseIsCurrent(
      providerAnswer,
      1,
      "npc:town:inn:0",
      { session_id: "dialogue:7:provider", revision: 3 },
    ),
    true,
  );
  assert.equal(
    dialogueResponseIsCurrent(
      providerAnswer,
      2,
      "npc:town:inn:1",
      { session_id: "dialogue:7:witness", revision: 0 },
    ),
    false,
  );
  assert.equal(
    dialogueResponseIsCurrent(
      providerAnswer,
      1,
      "npc:town:inn:0",
      { session_id: "dialogue:7:provider", revision: 4 },
    ),
    false,
  );
  assert.match(source, /dialogueResponseIsCurrent\([\s\S]*answer dialogue prompt/);
});

test("only the same in-flight encounter start is deduplicated", () => {
  assert.match(source, /startInFlight\?\.key === key/);
  assert.match(source, /finally\(\(\) => \{ if \(startInFlight\?\.key === key\) startInFlight = null/);
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
