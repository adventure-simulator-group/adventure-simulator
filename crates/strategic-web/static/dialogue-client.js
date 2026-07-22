(() => {
  "use strict";
  if (typeof document === "undefined") return;
  const chat = document.querySelector("[data-dialogue-conversation][data-local-chat-subject]");
  if (!chat) return;
  const messages = chat.querySelector(".settlement-chat-messages");
  const conversationId = chat.dataset.dialogueConversation;
  const revision = chat.dataset.dialogueCatalogRevision;
  let sessionId = "";

  const request = async (path, payload) => {
    const response = await window.strategicFetch(path, {
      method: "POST",
      headers: { "Content-Type": "application/json", Accept: "application/json" },
      body: JSON.stringify(payload),
    });
    if (!response.ok) throw new Error(`Dialogue action failed (${response.status})`);
    return response.status === 204 ? null : response.json();
  };
  const label = (role) => role.split("_").map((word) => word[0]?.toUpperCase() + word.slice(1)).join(" ");
  const topicAnchor = (topic, text) => {
    const anchor = document.createElement("a");
    anchor.href = "#";
    anchor.className = "chat-quest-link";
    anchor.textContent = text;
    anchor.dataset.dialogueTopic = topic;
    anchor.dataset.dialogueSession = sessionId;
    return anchor;
  };
  const renderTurn = (turn) => {
    const row = document.createElement("div");
    const player = turn.speaker.toLowerCase().includes("player") || turn.speaker === "customers" || turn.speaker === "patient";
    row.className = player ? "chat-player-message" : "chat-npc-message";
    row.dataset.chatChannel = "local";
    const timestamp = document.createElement("span");
    timestamp.className = "chat-timestamp";
    timestamp.textContent = "[--:--] ";
    const speaker = document.createElement("strong");
    speaker.textContent = `${player ? "You" : label(turn.speaker)}: `;
    row.append(timestamp, speaker);
    turn.fragments.forEach((fragment) => {
      if (fragment.kind === "text") row.append(document.createTextNode(fragment.value));
      else if (fragment.kind === "topic") row.append(topicAnchor(fragment.topic, fragment.label));
    });
    messages?.append(row);
  };
  const renderPrompt = (prompt) => {
    const form = document.createElement("form");
    form.className = "dialogue-prompt";
    form.dataset.dialoguePrompt = `${sessionId}:prompt:${prompt.id}`;
    prompt.choices.forEach((choice) => {
      const button = document.createElement("button");
      button.type = "submit";
      button.name = "choice";
      button.value = choice.id;
      button.className = "btn btn-small";
      button.textContent = choice.label;
      form.append(button);
    });
    messages?.append(form);
  };
  const showTopic = async (topicId) => {
    const response = await request("/api/dialogue/topic", {
      session_id: sessionId, conversation_id: conversationId, topic_id: topicId, revision,
    });
    response.turns.forEach(renderTurn);
    if (response.prompt) renderPrompt(response.prompt);
    messages.scrollTop = messages.scrollHeight;
  };
  const renderRail = (topics) => {
    const rail = document.querySelector(".right-sidebar");
    if (!rail) return;
    rail.querySelector("[data-dialogue-topic-rail]")?.remove();
    const section = document.createElement("section");
    section.className = "sidebar-section";
    section.dataset.dialogueTopicRail = "true";
    const heading = document.createElement("h3");
    heading.className = "sidebar-header";
    heading.textContent = "Topics";
    const list = document.createElement("ul");
    topics.forEach((topic) => { const item = document.createElement("li"); item.append(topicAnchor(topic.id, topic.label)); list.append(item); });
    section.append(heading, list);
    rail.prepend(section);
  };
  const begin = async () => {
    const catalogResponse = await window.strategicFetch("/api/dialogue/catalog", { headers: { Accept: "application/json" } });
    if (!catalogResponse.ok) throw new Error("Dialogue catalog is unavailable");
    const catalog = await catalogResponse.json();
    const conversation = catalog.conversations.flatMap((document) => document.conversations).find((entry) => entry.id === conversationId);
    if (!conversation) throw new Error(`Unknown dialogue conversation ${conversationId}`);
    const started = await request("/api/dialogue/start", {
      conversation_id: conversationId, npc_actor_id: chat.dataset.localChatSubject, revision,
    });
    sessionId = started.session_id;
    const known = conversation.topics.filter((topic) => topic.initially_known || catalog.known_topics.includes(topic.id));
    renderRail(known);
    if (known[0]) await showTopic(known[0].id);
  };

  document.addEventListener("click", (event) => {
    const topic = event.target.closest("[data-dialogue-topic]");
    if (!topic) return;
    event.preventDefault();
    if (topic.getAttribute("aria-disabled") === "true") return;
    topic.setAttribute("aria-disabled", "true");
    topic.removeAttribute("href");
    showTopic(topic.dataset.dialogueTopic).catch((error) => window.reportStrategicError(error, "choose dialogue topic"));
  });
  document.addEventListener("submit", (event) => {
    const form = event.target.closest("[data-dialogue-prompt]");
    if (!form) return;
    event.preventDefault();
    const submitter = event.submitter;
    const choices = submitter?.value ? [submitter.value] : Array.from(new FormData(form).getAll("choice"), String);
    request("/api/dialogue/answer", { prompt_row_id: form.dataset.dialoguePrompt, choice_ids: choices, revision })
      .then(() => { form.querySelectorAll("button").forEach((button) => { button.disabled = true; }); })
      .catch((error) => window.reportStrategicError(error, "answer dialogue prompt"));
  });
  const startWhenReady = () => begin().catch((error) => window.reportStrategicError(error, "start dialogue"));
  if (chat.dataset.localChatReady === "true") startWhenReady();
  else chat.addEventListener("local-chat-ready", startWhenReady, { once: true });
})();
