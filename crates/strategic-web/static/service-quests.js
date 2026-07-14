(() => {
  const services = document.querySelector("[data-settlement-id]");
  if (!services) return;

  const settlementId = services.dataset.settlementId;
  const chat = document.querySelector("[data-service-quest-settlement][data-service-quest-id]");

  const line = (kind, speaker, content) => {
    if (!chat) return null;
    const messages = chat.querySelector(".settlement-chat-messages");
    if (!messages) return null;
    const row = document.createElement("div");
    row.className = kind === "player" ? "chat-player-message" : "chat-npc-message";
    const timestamp = document.createElement("span");
    timestamp.className = "chat-timestamp";
    timestamp.textContent = "[--:--] ";
    row.append(timestamp);
    if (speaker) {
      const name = document.createElement("strong");
      name.textContent = `${speaker}: `;
      row.append(name);
    }
    if (typeof content === "string") row.append(document.createTextNode(content));
    else row.append(content);
    messages.append(row);
    messages.scrollTop = messages.scrollHeight;
    return row;
  };

  const link = (label, action) => {
    const anchor = document.createElement("a");
    anchor.href = "#";
    anchor.className = "chat-quest-link";
    anchor.textContent = label;
    anchor.addEventListener("click", (event) => {
      event.preventDefault();
      if (anchor.dataset.used) return;
      anchor.dataset.used = "true";
      action();
    });
    return anchor;
  };

  const updateTracker = (quest) => {
    const summary = document.querySelector("[data-current-quest]");
    if (!summary) return;
    const name = summary.querySelector("[data-current-quest-name]");
    const abandon = summary.querySelector("[data-current-quest-abandon]");
    if (name) {
      name.textContent = quest.title;
      name.href = `/quests/${encodeURIComponent(quest.quest_id)}`;
    }
    if (abandon) {
      abandon.action = `/quests/${encodeURIComponent(quest.quest_id)}/abandon`;
      abandon.hidden = false;
    }
    summary.hidden = false;
  };

  const beginConversation = (quest) => {
    const messages = chat.querySelector(".settlement-chat-messages");
    messages?.querySelector(".chat-npc-message")?.remove();
    const greeting = document.createDocumentFragment();
    greeting.append(document.createTextNode(`${quest.greeting} `));
    greeting.append(link(quest.problem, () => {
      line("player", "You", quest.follow_up);
      const details = document.createDocumentFragment();
      details.append(document.createTextNode(`${quest.details} `));
      details.append(link("interested", async () => {
        line("player", "You", "I'm interested.");
        if (!quest.can_accept) {
          line("npc", quest.npc_name, "I can only entrust this to a party leader who is free to take the work.");
          return;
        }
        const response = await fetch(`/api/quests/${encodeURIComponent(quest.id)}/accept`, {
          method: "POST",
          headers: { Accept: "application/json" },
        });
        const result = await response.json();
        if (!result.accepted) {
          line("npc", quest.npc_name, result.message || "I cannot give you this work just now.");
          return;
        }
        line("npc", quest.npc_name, quest.acceptance);
        updateTracker(result);
        const tab = services.querySelector(`[data-service-id="${CSS.escape(quest.service_id)}"]`);
        const badge = tab?.querySelector("[data-service-quest-badge]");
        if (badge) badge.hidden = true;
      }));
      line("npc", quest.npc_name, details);
    }));
    greeting.append(document.createTextNode("."));
    line("npc", quest.npc_name, greeting);
  };

  fetch(`/api/settlements/${encodeURIComponent(settlementId)}/service-quests`, {
    headers: { Accept: "application/json" },
  })
    .then((response) => (response.ok ? response.json() : []))
    .then((quests) => {
      quests.forEach((quest) => {
        const tab = services.querySelector(`[data-service-id="${CSS.escape(quest.service_id)}"]`);
        const badge = tab?.querySelector("[data-service-quest-badge]");
        if (badge) badge.hidden = false;
      });
      if (!chat || chat.dataset.serviceQuestSettlement !== settlementId) return;
      const quest = quests.find((entry) => entry.service_id === chat.dataset.serviceQuestId);
      if (quest) beginConversation(quest);
    })
    .catch(() => {});
})();
