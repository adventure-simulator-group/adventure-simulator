const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const source = fs.readFileSync(path.join(__dirname, "..", "static", "dialogue-client.js"), "utf8");
const dialogueCss = fs.readFileSync(path.join(__dirname, "..", "static", "css", "strategic.css"), "utf8");
const {
  createRetriableAction,
  dialogueCompletion,
  dialogueResponseIsCurrent,
  dialogueSubmission,
  dialogueTopicPayload,
  relationshipLabel,
  relationshipLevel,
  romanticResponse,
  socialDurationChoices,
  contextualMutationIsCurrent,
  courtshipPresentation,
  affinityPresentation,
  moraleTopicPresentation,
} = require("../static/dialogue-client.js");

test("errantry acceptance reuses its action ID after a lost response", async () => {
  const generated = ["acceptance-1", "acceptance-2"];
  const acceptance = createRetriableAction(() => generated.shift());
  const sent = [];
  await assert.rejects(
    acceptance.run(async (actionId) => {
      sent.push(actionId);
      throw new TypeError("fetch failed after commit");
    }),
  );
  const result = await acceptance.run(async (actionId) => {
    sent.push(actionId);
    return { redirect: "/quests" };
  });
  assert.deepEqual(sent, ["acceptance-1", "acceptance-1"]);
  assert.equal(result.redirect, "/quests");
  await acceptance.run(async (actionId) => {
    sent.push(actionId);
    return { redirect: "/quests" };
  });
  assert.deepEqual(sent, ["acceptance-1", "acceptance-1", "acceptance-2"]);
});

test("errantry acceptance replaces its action ID after a definitive conflict", async () => {
  const generated = ["conflict-1", "conflict-2"];
  const acceptance = createRetriableAction(() => generated.shift());
  const sent = [];
  await assert.rejects(
    acceptance.run(async (actionId) => {
      sent.push(actionId);
      const error = new Error("conflict");
      error.status = 422;
      throw error;
    }),
  );
  await acceptance.run(async (actionId) => {
    sent.push(actionId);
    return { redirect: "/quests" };
  });
  assert.deepEqual(sent, ["conflict-1", "conflict-2"]);
});

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

test("discovered authored topics use typed category tabs while contextual actions stay inline", () => {
  assert.match(source, /topic\.category \|\| "lore"/);
  assert.match(source, /data-dialogue-category-panel/);
  assert.match(source, /row\.append\(topicAnchor/);
  assert.match(source, /dialogue-context-topics/);
  assert.match(source, /contextTopicButton\("social", "conversation"/);
  assert.match(source, /contextTopicButton\("romance", "rose"/);
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

test("atomic witness claims use observer-safe projections and authoritative responses", () => {
  assert.match(source, /fragment\.kind === "claim"/);
  assert.match(source, /fragment\.kind === "claim"[\s\S]*sourceLink\(source\)/);
  assert.doesNotMatch(source, /claim_segments|split_once/);
  assert.doesNotMatch(source, /api\/dialogue\/spend-time/);
  assert.doesNotMatch(source, /api\/dialogue\/insight/);
  assert.match(source, /api\/dialogue\/claim-response/);
  assert.match(source, /challenge_token:\s*claim\.challenge_token/);
  assert.match(source, /expected_revision:\s*binding\.revision/);
  assert.match(dialogueCss, /dialogue-claim-unknown/);
  assert.match(dialogueCss, /dialogue-claim-likely_false/);
  assert.match(dialogueCss, /dialogue-claim-likely_true/);
  assert.match(source, /aria-expanded/);
  assert.match(source, /aria-controls/);
  assert.match(dialogueCss, /dialogue-claim-actions\s*\{[^}]*flex-direction:\s*column/s);
  assert.match(source, /Charm/);
  assert.match(source, /Command/);
  assert.match(source, /Bluff/);
  assert.match(source, /Takes 5 minutes/);
  assert.match(source, /Affinity \$\{sign\}/);
  assert.match(source, /claim\.charm_response/);
  assert.match(source, /claim\.command_response/);
  assert.match(source, /claim\.bluff_response/);
  assert.match(source, /\]\.filter\(Boolean\)/);
  assert.doesNotMatch(source, /Would you tell me more of|Speak plainly: was it truly|Others tell it differently/);
  assert.doesNotMatch(source, /pressure_cue|possible-pressure|cooldown|available_approaches/);
  assert.doesNotMatch(source, /has_bound_concern|diagnosis_correct|success_chance|target_transparency|proposition_id|reliability|truthful_text/);
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
  assert.doesNotMatch(source, /data\.openNpcSocial|dataset\.openNpcSocial/);
  assert.doesNotMatch(source, /settlement-npc-social-button/);
  assert.doesNotMatch(source, /npc-social-summary/);
});

test("socializing and romance are dialogue responses with a qualitative regard face", () => {
  assert.equal(relationshipLabel("well_known"), "well known");
  assert.equal(relationshipLevel("affinity", "trusted"), 1);
  assert.equal(relationshipLevel("morale", "guarded"), 2 / 3);
  assert.deepEqual(socialDurationChoices.map((choice) => choice.minutes), [15, 60, 240]);
  assert.equal(romanticResponse("formal_courtship").icon, "rose");
  assert.equal(romanticResponse("schedule_wedding").icon, "calendar");
  assert.deepEqual(courtshipPresentation("formal", false), { icon: "rose", label: "formal courtship; formal and public" });
  assert.deepEqual(courtshipPresentation("informal", false), { icon: "lockpicks", label: "informal courtship; private" });
  assert.deepEqual(courtshipPresentation("informal", true), { icon: "eye-target", label: "informal courtship; known to family" });
  assert.match(source, /dataset\.dialogueContextPrompt/);
  assert.match(source, /appendContextExchange/);
  assert.match(source, /face\.dataset\.npcAffinityFace/);
  assert.match(source, /requested_minutes: choice\.minutes/);
  assert.match(source, /ask under Of Thee for more/);
  assert.match(source, /currentView\.open_prompt \|\| contextualMutation/);
  assert.match(source, /const render = \(view\) => \{[\s\S]*?removeContextPrompt\(\);/);
  const socialMutation = source.slice(source.indexOf("const chooseSocialResponse"), source.indexOf("const chooseRomanticResponse"));
  const romanceMutation = source.slice(source.indexOf("const chooseRomanticResponse"), source.indexOf("const renderContextPrompt"));
  assert.match(socialMutation, /renderCategoryTopics\(currentView/);
  assert.match(romanceMutation, /renderCategoryTopics\(currentView/);
  assert.doesNotMatch(source, /success_chance|personality_fit|morale_delta/);
});

test("morale topics and affinity faces combine color with accessible qualitative wording", () => {
  assert.deepEqual(affinityPresentation(0), { band: "neutral", label: "Neutral regard", face: "😐" });
  assert.equal(affinityPresentation(70).band, "very-warm");
  assert.equal(affinityPresentation(-70).band, "hostile");
  assert.equal(moraleTopicPresentation(0).label, "Neutral morale, +0.0");
  assert.equal(moraleTopicPresentation(4).direction, "positive");
  assert.equal(moraleTopicPresentation(-4).direction, "negative");
  assert.notEqual(moraleTopicPresentation(1).color, moraleTopicPresentation(5).color);
  assert.match(dialogueCss, /affinity-popover\.is-pinned/);
  assert.match(source, /event\.key !== "Escape"/);
  assert.match(source, /is-closing/);
  assert.match(dialogueCss, /is-closing \.affinity-details/);
});

test("NPC switches clear subject-bound social state before asynchronous refresh", () => {
  const selectNpc = source.slice(source.indexOf("const selectNpc"), source.indexOf("const loadPeople"));
  assert.match(selectNpc, /currentSocial = null/);
  assert.match(selectNpc, /data-npc-affinity-popover/);
  assert.match(selectNpc, /data-dialogue-category-panel/);
  assert.match(selectNpc, /Loading this person's conversation/);
  assert.match(source, /button\.dataset\.socialRevision !== currentSocial\?\.social_revision/);
  assert.match(source, /button\.dataset\.socialSubject !== chat\.dataset\.localChatSubject/);
});

test("party portrait chat deep-loads the authorized social dock into the functional stream", () => {
  assert.match(source, /dockChat\.dataset\.partySocialHref/);
  assert.match(source, /const activateRequested = async \(tab, focus = false\)/);
  assert.match(source, /void activateRequested\(tabs\[next\], true\)/);
  assert.match(source, /querySelector\("\[data-social-conversation\]"\)/);
  assert.match(source, /dockChat\.replaceWith\(replacement\)/);
});

test("contextual mutations are bound to the active NPC, path, session, revision, and operation", () => {
  const binding = { token: "op-1", selectionGeneration: 3, npcId: "npc:anna", path: "/anna/social", sessionId: "dialogue:anna", revision: 4 };
  const view = { session_id: "dialogue:anna", revision: 4 };
  assert.equal(contextualMutationIsCurrent(binding, "op-1", 3, "npc:anna", "/anna/social", view), true);
  assert.equal(contextualMutationIsCurrent(binding, "op-2", 3, "npc:anna", "/anna/social", view), false);
  assert.equal(contextualMutationIsCurrent(binding, "op-1", 4, "npc:anna", "/anna/social", view), false);
  assert.equal(contextualMutationIsCurrent(binding, "op-1", 3, "npc:elsa", "/elsa/social", view), false);
  assert.equal(contextualMutationIsCurrent(binding, "op-1", 3, "npc:anna", "/anna/social", { ...view, revision: 5 }), false);
  assert.match(source, /if \(!contextMutationCurrent\(binding\)\) return/);
  assert.match(source, /contextualMutation = null/);
  assert.match(source, /setContextControlsDisabled\(true\)/);
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
