(() => {
  const services = document.querySelector("[data-settlement-id]");
  if (!services) return;

  const settlementId = services.dataset.settlementId;
  const chat = document.querySelector("[data-service-quest-settlement][data-service-quest-id]");
  const dialogueActions = new Map();
  let nextDialogueActionId = 0;

  const line = (kind, speaker, content) => {
    if (!chat) return null;
    const messages = chat.querySelector(".settlement-chat-messages");
    if (!messages) return null;
    const row = document.createElement("div");
    const body = typeof content === "string" ? content : content.textContent;
    row.className = kind === "player" ? "chat-player-message" : "chat-npc-message";
    row.dataset.chatChannel = "local";
    row.dataset.localChatBody = body || "";
    row.dataset.localChatSpeaker = speaker || "";
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
      window.strategicFetch(`/api/local-chat/npc/${encodeURIComponent(subject)}${suffix}`, {
        method: "POST",
        headers: { "Content-Type": "application/x-www-form-urlencoded" },
        body: form,
      }).catch((error) => window.reportStrategicError(error, "record quest dialogue"));
    }
    return row;
  };

  const link = (label, action) => {
    const anchor = document.createElement("a");
    anchor.href = "#";
    anchor.className = "chat-quest-link";
    anchor.textContent = label;
    const actionId = String(++nextDialogueActionId);
    anchor.dataset.questDialogueAction = actionId;
    dialogueActions.set(actionId, action);
    return anchor;
  };

  document.addEventListener("click", (event) => {
    const anchor = event.target.closest("[data-quest-dialogue-action]");
    if (!anchor) return;
    event.preventDefault();
    if (anchor.dataset.used) return;
    const action = dialogueActions.get(anchor.dataset.questDialogueAction);
    if (!action) return;
    anchor.dataset.used = "true";
    dialogueActions.delete(anchor.dataset.questDialogueAction);
    action();
  });

  const updateTracker = (quest) => {
    const summary = document.querySelector("[data-current-quest]");
    if (!summary) return;
    const name = summary.querySelector("[data-current-quest-name]");
    const status = summary.querySelector("[data-current-quest-status]");
    const abandon = summary.querySelector("[data-current-quest-abandon]");
    if (name) {
      name.textContent = quest.title;
    }
    if (status) {
      status.classList.remove("resolved");
      status.title = "Quest in progress";
      status.setAttribute("aria-label", status.title);
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

    const leftTemplate = document.createElement("template");
    const rightTemplate = document.createElement("template");
    leftTemplate.innerHTML = role.left_html;
    rightTemplate.innerHTML = role.right_html;
    left.append(leftTemplate.content);
    right.append(rightTemplate.content);
  };

  const recruitmentLink = (quest, role) => {
    const anchor = link(role.name, () => inspectRecruitingRole(quest, role));
    anchor.classList.add("service-role-link", `service-role-link-${role.match_level}`);
    anchor.title = role.requirements_summary;
    return anchor;
  };

  const openRecruitmentOffer = (quest) => {
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
  };

  const openQuestOffer = (quest) => {
    line("player", "You", quest.follow_up);
    const details = document.createDocumentFragment();
    details.append(document.createTextNode(`${quest.details} `));
    details.append(link("interested", async () => {
      line("player", "You", "I'm interested.");
      if (!quest.can_accept) {
        line("npc", quest.npc_name, "I can only entrust this to a party leader who is free to take the work.");
        return;
      }
      const response = await window.strategicFetch(`/api/quests/${encodeURIComponent(quest.id)}/accept`, {
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
  };

  const turnInQuest = async (quest) => {
    line("player", "You", "I've finished.");
    const response = await window.strategicFetch(`/api/quests/${encodeURIComponent(quest.id)}/turn-in`, {
      method: "POST",
      headers: { Accept: "application/json" },
    });
    const result = await response.json();
    if (!result.claimed) {
      line("npc", quest.npc_name, result.message || "I cannot settle this account just now.");
      return;
    }
    line("npc", quest.npc_name, quest.turn_in_response);
    window.strategicChat?.appendInfo(
      chat,
      `${result.reward} gold has been added to your party inventory.`,
    );
    clearTracker();
    const tab = services.querySelector(`[data-service-id="${CSS.escape(quest.service_id)}"]`);
    const badge = tab?.querySelector("[data-service-quest-badge]");
    if (badge) badge.hidden = true;
    const settlementBadge = document.querySelector("[data-settlement-turn-in-badge]");
    if (settlementBadge) settlementBadge.hidden = true;
  };

  const beginRecruitmentConversation = (quest) => {
    const messages = chat.querySelector(".settlement-chat-messages");
    messages?.querySelector(".chat-npc-message")?.remove();
    const greeting = document.createDocumentFragment();
    greeting.append(document.createTextNode(`${quest.greeting} `));
    greeting.append(link(quest.problem, () => openRecruitmentOffer(quest)));
    greeting.append(document.createTextNode("."));
    line("npc", quest.npc_name, greeting);
  };

  const beginConversation = (quest) => {
    const messages = chat.querySelector(".settlement-chat-messages");
    messages?.querySelector(".chat-npc-message")?.remove();
    const greeting = document.createDocumentFragment();
    greeting.append(document.createTextNode(`${quest.greeting} `));
    greeting.append(link(quest.problem, () => openQuestOffer(quest)));
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
    greeting.append(link("finished", () => turnInQuest(quest)));
    greeting.append(document.createTextNode("?"));
    line("npc", quest.npc_name, greeting);
  };

  const locateHerbalistMedication = (medicationName) => {
    const row = document.querySelector(
      `[data-herbalist-medication-name="${CSS.escape(medicationName)}"]`,
    );
    if (!row) return;
    row.scrollIntoView({ behavior: "smooth", block: "center" });
    row.querySelector("[data-merchant-buy]")?.focus();
  };

  const requestHerbalistExamination = async () => {
    line("player", "You", "I have been feeling ill.");
    const response = await window.strategicFetch(
      `/api/settlements/${encodeURIComponent(settlementId)}/herbalist/examination`,
      { method: "POST", headers: { Accept: "application/json" } },
    );
    const result = await response.json();
    if (!Array.isArray(result.diagnoses) || result.diagnoses.length === 0) {
      line("npc", "Herbalist", result.message || "I cannot name your illness with confidence.");
      return;
    }
    result.diagnoses.forEach((diagnosis) => {
      const recommendation = document.createDocumentFragment();
      recommendation.append(document.createTextNode(`You have ${diagnosis.disease_name}. I recommend `));
      recommendation.append(link(diagnosis.medication_name, () => {
        locateHerbalistMedication(diagnosis.medication_name);
      }));
      recommendation.append(document.createTextNode("."));
      line("npc", "Herbalist", recommendation);
    });
  };

  const beginHerbalistConversation = (quest) => {
    const messages = chat.querySelector(".settlement-chat-messages");
    messages?.querySelector(".chat-npc-message")?.remove();
    const greeting = document.createDocumentFragment();
    greeting.append(document.createTextNode("Greetings, traveler, what brings you to my humble shop? "));
    greeting.append(link("Feeling ill", requestHerbalistExamination));
    const examFee = Number(chat.dataset.herbalistExamFee);
    greeting.append(document.createTextNode(
      `? Or are you looking to purchase some ingredients? An examination costs ${examFee} gold.`,
    ));
    if (quest?.state === "available" || quest?.state === "recruiting") {
      greeting.append(document.createTextNode(" I could also use your help concerning "));
      const openQuest = quest.state === "recruiting" ? openRecruitmentOffer : openQuestOffer;
      greeting.append(link(quest.problem, () => openQuest(quest)));
      greeting.append(document.createTextNode("."));
    } else if (quest?.state === "ready") {
      greeting.append(document.createTextNode(" And have you "));
      greeting.append(link("finished the work we discussed", () => turnInQuest(quest)));
      greeting.append(document.createTextNode("?"));
    } else if (quest) {
      greeting.append(document.createTextNode(" I still await word of the work we discussed."));
    }
    line("npc", quest?.npc_name || "Herbalist", greeting);
  };

  const faithDetails = {
    western_church: {
      topic: "your place within Holy Church",
      name: "the Western Church",
      invitation: "This altar stands in communion with Holy Church. If you would enter her fellowship, speak freely and with a sincere conscience.",
      label: "Receive me into communion with Holy Church",
      reply: "Then let your profession be sincere. Make confession, hear Mass, and receive the sacraments worthily; I shall count you among the faithful.",
      already: "You are already in communion with Holy Church. Persevere in confession, the Mass, and works of mercy.",
    },
    reformed: {
      topic: "the evangelical faith preached in this church",
      name: "the Reformed faith",
      invitation: "In this church we confess the evangelical faith and place our trust in God's grace. If you would join this congregation, speak plainly.",
      label: "I would embrace the evangelical confession",
      reply: "Then hear the Word faithfully, pray for steadfastness, and let your life bear witness to the faith you have confessed.",
      already: "You already share the evangelical confession of this church. Remain steadfast in the Word and in charity toward your neighbors.",
    },
    old_faith: {
      topic: "the ancestral rites kept in this place",
      name: "the Old Faith",
      invitation: "We keep here the sacred customs handed down by our forebears. If you would bind yourself to them, make no idle promise.",
      label: "I will keep the faith and rites of your forebears",
      reply: "Then honor the old rites faithfully, keep your vows, and do not let hardship make your profession a hollow thing.",
      already: "You already keep the faith of this church. Honor the old rites and the obligations you have accepted.",
    },
  };

  const openFaithTopic = (religion) => {
    line("player", "You", "I would speak of my place within the Church.");
    if (!religion?.can_choose) {
      line("npc", "Priest", "I can receive such a profession only from one who stands before me in this church.");
      return;
    }
    const faith = faithDetails[religion.priest_religion_id];
    if (!faith) {
      line("npc", "Priest", "This church cannot receive a profession of faith just now.");
      return;
    }
    if (religion.religion_id === religion.priest_religion_id) {
      line("npc", "Priest", faith.already);
      return;
    }
    const invitation = document.createDocumentFragment();
    invitation.append(document.createTextNode(`${faith.invitation} `));
    let settled = false;
    invitation.append(link(faith.label, async () => {
      if (settled) return;
      settled = true;
      line("player", "You", `${faith.label}.`);
      const form = new URLSearchParams({ religion_id: religion.priest_religion_id });
      const response = await window.strategicFetch(`/api/settlements/${encodeURIComponent(settlementId)}/religion`, {
        method: "POST",
        headers: {
          Accept: "application/json",
          "Content-Type": "application/x-www-form-urlencoded",
        },
        body: form,
      });
      const result = await response.json();
      if (!result.changed) {
        settled = false;
        line("npc", "Priest", result.message || "I cannot receive your profession just now.");
        return;
      }
      religion.religion_id = result.religion_id;
      line("npc", "Priest", faith.reply);
    }));
    invitation.append(document.createTextNode("."));
    line("npc", "Priest", invitation);
  };

  const beginReligionConversation = (quest, religion) => {
    const messages = chat.querySelector(".settlement-chat-messages");
    messages?.querySelector(".chat-npc-message")?.remove();
    const faith = faithDetails[religion?.priest_religion_id];
    const greeting = document.createDocumentFragment();
    greeting.append(document.createTextNode("God give you peace, traveler. If your conscience is troubled, we may speak of "));
    greeting.append(link(faith?.topic || "the faith of this church", () => openFaithTopic(religion)));
    greeting.append(document.createTextNode("."));
    if (quest?.state === "available" || quest?.state === "recruiting") {
      greeting.append(document.createTextNode(" I must also ask your aid concerning "));
      const openQuest = quest.state === "recruiting" ? openRecruitmentOffer : openQuestOffer;
      greeting.append(link(quest.problem, () => openQuest(quest)));
      greeting.append(document.createTextNode("; prayer does not release us from the works of mercy."));
    } else if (quest?.state === "ready") {
      greeting.append(document.createTextNode(" And tell me: have you "));
      greeting.append(link("finished the work we discussed", () => turnInQuest(quest)));
      greeting.append(document.createTextNode("?"));
    } else if (quest) {
      greeting.append(document.createTextNode(" I continue to pray for your safe return from the work we discussed."));
    } else {
      greeting.append(document.createTextNode(" The church door remains open to every penitent."));
    }
    line("npc", "Priest", greeting);
  };

  let conversationSignature = "";
  const refreshServiceQuests = () => window.strategicBackgroundFetch("service-quests", `/api/settlements/${encodeURIComponent(settlementId)}/service-quests`, {
    headers: { Accept: "application/json" },
  })
    .then((response) => (response.ok ? response.json() : []))
    .then(async (quests) => {
      const religion = chat?.dataset.serviceQuestId === "religion"
        ? await window.strategicBackgroundFetch("religion-dialogue", `/api/settlements/${encodeURIComponent(settlementId)}/religion`, {
          headers: { Accept: "application/json" },
        }).then((response) => (response.ok ? response.json() : { religion_id: null, priest_religion_id: "", can_choose: false }))
        : null;
      return [quests, religion];
    })
    .then(([quests, religion]) => {
      services.querySelectorAll("[data-service-quest-badge]").forEach((badge) => {
        badge.hidden = true;
        badge.classList.remove("service-turn-in-badge", "service-recruitment-badge");
      });
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
      const nextConversationSignature = JSON.stringify(quest ? {
        id: quest.id,
        state: quest.state,
        can_accept: quest.can_accept,
        can_turn_in: quest.can_turn_in,
        recruitment: quest.recruitment,
      } : null);
      if (nextConversationSignature === conversationSignature) return;
      conversationSignature = nextConversationSignature;
      const showConversation = () => {
        if (chat.dataset.serviceQuestId === "herbalist") beginHerbalistConversation(quest);
        else if (chat.dataset.serviceQuestId === "religion") beginReligionConversation(quest, religion);
        else if (quest?.state === "available") beginConversation(quest);
        else if (quest?.state === "recruiting") beginRecruitmentConversation(quest);
        else if (quest) beginReturnConversation(quest);
      };
      if (chat.dataset.localChatReady === "true") showConversation();
      else chat.addEventListener("local-chat-ready", showConversation, { once: true });
    })
    .catch((error) => window.reportStrategicError(error, "service quests"));
  window.queueStrategicInitialLoad(refreshServiceQuests);
  document.addEventListener("strategic-live-update", refreshServiceQuests);
})();
