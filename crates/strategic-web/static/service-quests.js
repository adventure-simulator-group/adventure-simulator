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
    const body = typeof content === "string" ? content : content.textContent;
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
    const subject = chat.dataset.localChatSubject;
    if (subject) {
      const form = new URLSearchParams({ body: body || "", speaker: speaker || "" });
      const suffix = kind === "player" ? "" : "/npc";
      fetch(`/api/local-chat/npc/${encodeURIComponent(subject)}${suffix}`, {
        method: "POST",
        headers: { "Content-Type": "application/x-www-form-urlencoded" },
        body: form,
      }).catch(() => {});
    }
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
    }
    if (abandon) {
      abandon.action = `/quests/${encodeURIComponent(quest.quest_id)}/abandon`;
      abandon.hidden = false;
    }
    summary.hidden = false;
  };

  const clearTracker = () => {
    const summary = document.querySelector("[data-current-quest]");
    if (summary) summary.hidden = true;
  };

  const clearRoleInspection = () => {
    document.querySelectorAll("[data-service-role-inspection]").forEach((panel) => panel.remove());
    document.querySelectorAll(".service-role-inspection-hidden").forEach((element) => {
      element.classList.remove("service-role-inspection-hidden");
    });
  };

  const inspectRecruitingRole = (quest, role) => {
    clearRoleInspection();
    const left = document.querySelector(".left-sidebar");
    const right = document.querySelector(".right-sidebar");
    if (!left || !right) return;
    [left, right].forEach((sidebar) => {
      Array.from(sidebar.children).forEach((child) => child.classList.add("service-role-inspection-hidden"));
    });

    const leftPanel = document.createElement("section");
    leftPanel.dataset.serviceRoleInspection = "true";
    leftPanel.className = "role-inspection-panel role-inspection-content";
    const leftHeading = document.createElement("h3");
    leftHeading.className = "sidebar-header";
    leftHeading.textContent = role.name;
    leftPanel.append(leftHeading);
    const list = document.createElement("div");
    list.className = "role-detail-list";
    const requirements = role.requirements.length ? role.requirements : ["No minimum recommendations"];
    requirements.forEach((requirement) => {
      const row = document.createElement("div");
      row.className = "role-detail-row";
      row.append(document.createTextNode(requirement));
      list.append(row);
    });
    leftPanel.append(list);

    const rightPanel = document.createElement("section");
    rightPanel.dataset.serviceRoleInspection = "true";
    rightPanel.className = "role-inspection-panel role-inspection-content";
    const rightHeading = document.createElement("h3");
    rightHeading.className = "sidebar-header";
    rightHeading.textContent = quest.recruitment.party_name;
    rightPanel.append(rightHeading);
    const leader = document.createElement("p");
    leader.textContent = `Led by ${quest.recruitment.leader_name}`;
    rightPanel.append(leader);
    const match = document.createElement("p");
    match.className = `small-copy service-role-match service-role-match-${role.match_level}`;
    match.textContent = role.match_summary;
    rightPanel.append(match);
    const availability = document.createElement("p");
    availability.className = "small-copy text-muted";
    availability.textContent = `${role.remaining} opening${role.remaining === 1 ? "" : "s"}`;
    rightPanel.append(availability);
    const form = document.createElement("form");
    form.method = "post";
    form.action = `/party-roles/${encodeURIComponent(role.id)}/join`;
    const button = document.createElement("button");
    button.type = "submit";
    button.className = "btn btn-primary btn-block mt-1";
    button.textContent = "Send request to join";
    button.disabled = !quest.can_accept;
    if (!quest.can_accept) button.title = "Only a free party leader at this settlement can request a merge";
    form.append(button);
    rightPanel.append(form);
    left.append(leftPanel);
    right.append(rightPanel);
  };

  const recruitmentLink = (quest, role) => {
    const anchor = link(role.name, () => inspectRecruitingRole(quest, role));
    anchor.classList.add("service-role-link", `service-role-link-${role.match_level}`);
    anchor.title = role.requirements_summary;
    return anchor;
  };

  const beginRecruitmentConversation = (quest) => {
    const messages = chat.querySelector(".settlement-chat-messages");
    messages?.querySelector(".chat-npc-message")?.remove();
    const greeting = document.createDocumentFragment();
    greeting.append(document.createTextNode(`${quest.greeting} `));
    greeting.append(link(quest.problem, () => {
      line("player", "You", quest.follow_up);
      const details = document.createDocumentFragment();
      const situation = quest.details.replace(/\s*Are you\s*$/, "");
      details.append(document.createTextNode(`${situation} `));
      const leader = document.createElement("a");
      leader.href = `/locations/settlement/${encodeURIComponent(settlementId)}/players/${encodeURIComponent(quest.recruitment.leader_id)}`;
      leader.className = "chat-quest-link";
      leader.textContent = quest.recruitment.leader_name;
      leader.title = `Leader of ${quest.recruitment.party_name}`;
      details.append(leader, document.createTextNode(" is looking for "));
      if (quest.recruitment.roles.length === 0) {
        details.replaceChildren();
        details.append(leader, document.createTextNode(" and their party are already helping me with the matter."));
        line("npc", quest.npc_name, details);
        return;
      }
      quest.recruitment.roles.forEach((role, index) => {
        if (index > 0) details.append(document.createTextNode(index + 1 === quest.recruitment.roles.length ? " and " : ", "));
        details.append(recruitmentLink(quest, role));
      });
      details.append(document.createTextNode(" to help."));
      line("npc", quest.npc_name, details);
    }));
    greeting.append(document.createTextNode("."));
    line("npc", quest.npc_name, greeting);
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

  const beginReturnConversation = (quest) => {
    const messages = chat.querySelector(".settlement-chat-messages");
    messages?.querySelector(".chat-npc-message")?.remove();
    if (quest.state === "underway") {
      line("npc", quest.npc_name, quest.waiting);
      return;
    }

    const greeting = document.createDocumentFragment();
    greeting.append(document.createTextNode("Welcome back. Have you "));
    greeting.append(link("finished", async () => {
      line("player", "You", "I've finished.");
      if (!quest.can_turn_in) {
        line("npc", quest.npc_name, "Your party leader should report the result and receive the reward.");
        return;
      }
      const response = await fetch(`/api/quests/${encodeURIComponent(quest.id)}/turn-in`, {
        method: "POST",
        headers: { Accept: "application/json" },
      });
      const result = await response.json();
      if (!result.claimed) {
        line("npc", quest.npc_name, result.message || "I cannot settle this account just now.");
        return;
      }
      line("npc", quest.npc_name, quest.turn_in_response);
      clearTracker();
      const tab = services.querySelector(`[data-service-id="${CSS.escape(quest.service_id)}"]`);
      const badge = tab?.querySelector("[data-service-quest-badge]");
      if (badge) badge.hidden = true;
      const settlementBadge = document.querySelector("[data-settlement-turn-in-badge]");
      if (settlementBadge) settlementBadge.hidden = true;
    }));
    greeting.append(document.createTextNode("?"));
    line("npc", quest.npc_name, greeting);
  };

  fetch(`/api/settlements/${encodeURIComponent(settlementId)}/service-quests`, {
    headers: { Accept: "application/json" },
  })
    .then((response) => (response.ok ? response.json() : []))
    .then((quests) => {
      const serviceIds = new Set(quests.map((quest) => quest.service_id));
      serviceIds.forEach((serviceId) => {
        const serviceQuests = quests.filter((quest) => quest.service_id === serviceId);
        const tab = services.querySelector(`[data-service-id="${CSS.escape(serviceId)}"]`);
        const badge = tab?.querySelector("[data-service-quest-badge]");
        if (badge) {
          badge.hidden = !serviceQuests.some((quest) => quest.state !== "underway");
          badge.classList.toggle(
            "service-turn-in-badge",
            serviceQuests.some((quest) => quest.state === "ready"),
          );
          badge.classList.toggle(
            "service-recruitment-badge",
            !serviceQuests.some((quest) => quest.state === "ready")
              && serviceQuests.some((quest) => quest.state === "recruiting"),
          );
        }
      });
      const settlementBadge = document.querySelector("[data-settlement-turn-in-badge]");
      if (settlementBadge) settlementBadge.hidden = !quests.some((quest) => quest.state === "ready");
      if (!chat || chat.dataset.serviceQuestSettlement !== settlementId) return;
      const serviceQuests = quests.filter((entry) => entry.service_id === chat.dataset.serviceQuestId);
      const quest = serviceQuests.find((entry) => entry.state === "ready")
        || serviceQuests.find((entry) => entry.state === "recruiting")
        || serviceQuests.find((entry) => entry.state === "available")
        || serviceQuests[0];
      const showConversation = () => {
        if (quest?.state === "available") beginConversation(quest);
        else if (quest?.state === "recruiting") beginRecruitmentConversation(quest);
        else if (quest) beginReturnConversation(quest);
      };
      if (chat.dataset.localChatReady === "true") showConversation();
      else chat.addEventListener("local-chat-ready", showConversation, { once: true });
    })
    .catch(() => {});
})();
