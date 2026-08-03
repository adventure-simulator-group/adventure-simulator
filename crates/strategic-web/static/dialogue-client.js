(() => {
  "use strict";

  const normalize = (value) => value.trim().toLocaleLowerCase();
  const exactCandidate = (value, candidates) => {
    const token = normalize(value);
    if (!token) return null;
    return candidates.find((candidate) => normalize(candidate.label) === token || normalize(candidate.id) === token) || null;
  };
  const dialogueCompletion = (value, candidates, multi = false) => {
    const comma = multi ? value.lastIndexOf(",") : -1;
    const prefix = comma >= 0 ? value.slice(0, comma + 1) : "";
    const token = value.slice(comma + 1).trimStart();
    const normalized = normalize(token);
    if (!normalized || exactCandidate(token, candidates)) return null;
    const selected = multi && comma >= 0
      ? value.slice(0, comma).split(",").map((part) => exactCandidate(part, candidates)?.id).filter(Boolean)
      : [];
    const matches = candidates.filter((candidate) => !selected.includes(candidate.id)
      && normalize(candidate.label).startsWith(normalized));
    if (matches.length !== 1) return null;
    const spacing = comma >= 0 && /^\s/.test(value.slice(comma + 1)) ? " " : "";
    return `${prefix}${spacing}${matches[0].label}`;
  };
  const dialogueSubmission = (value, candidates, multi = false) => {
    const parts = multi ? value.split(",") : [value];
    if (!parts.length || parts.some((part) => !part.trim())) return null;
    const matches = parts.map((part) => exactCandidate(part, candidates));
    if (matches.some((match) => !match)) return null;
    const ids = [...new Set(matches.map((match) => match.id))];
    return ids.length === matches.length ? ids : null;
  };
  const dialogueTopicPayload = (binding, currentGeneration, currentNpcId, actionId) => {
    if (!binding
      || binding.selectionGeneration !== currentGeneration
      || binding.npcId !== currentNpcId
      || !binding.sessionId
      || !binding.topicId) return null;
    return {
      session_id: binding.sessionId,
      topic_id: binding.topicId,
      action_id: actionId,
      expected_revision: binding.revision,
    };
  };
  const dialogueResponseIsCurrent = (binding, currentGeneration, currentNpcId, currentView) => Boolean(
    binding
    && binding.selectionGeneration === currentGeneration
    && binding.npcId === currentNpcId
    && binding.sessionId === currentView?.session_id
    && binding.revision === currentView?.revision
  );
  const createRetriableAction = (createId) => {
    let pendingActionId = null;
    return {
      run(send) {
        pendingActionId ||= createId();
        const attemptedActionId = pendingActionId;
        return Promise.resolve()
          .then(() => send(attemptedActionId))
          .then((result) => {
            pendingActionId = null;
            return result;
          })
          .catch((error) => {
            if (error?.status === 409 || error?.status === 422) pendingActionId = null;
            throw error;
          });
      },
    };
  };
  const relationshipLabel = (value) => String(value || "uncertain").replaceAll("_", " ");
  const relationshipLevel = (kind, value) => {
    const bands = {
      affinity: ["hostile", "reserved", "warm", "trusted"],
      familiarity: ["new", "known", "familiar", "well_known"],
      morale: ["distressed", "guarded", "settled"],
    }[kind] || [];
    const index = bands.indexOf(String(value || ""));
    return index < 0 ? 0 : (index + 1) / bands.length;
  };
  const socialDurationChoices = Object.freeze([
    { id: "brief", minutes: 15, icon: "conversation", label: "Hast thou a moment to speak?", detail: "A brief fifteen-minute conversation." },
    { id: "visit", minutes: 60, icon: "sun", label: "I would tarry and speak with thee awhile.", detail: "An unhurried one-hour visit." },
    { id: "evening", minutes: 240, icon: "calendar", label: "Shall we pass the evening together?", detail: "A long four-hour conversation." },
  ]);
  const romanticResponse = (action) => ({
    formal_courtship: { icon: "rose", label: "Ask for a formal courtship", line: "I would seek thy family's leave to court thee." },
    informal_courtship: { icon: "lockpicks", label: "Propose a private courtship", line: "Wilt thou court me, though we keep it between ourselves?" },
    schedule_wedding: { icon: "calendar", label: "Plan the wedding", line: "Let us appoint the day and plight our troth." },
    cancel_wedding: { icon: "broken-heart", label: "Cancel the wedding", line: "I cannot go forward with our wedding as appointed." },
  })[action] || null;
  const courtshipPresentation = (kind, exposed) => {
    const informal = kind === "informal";
    if (!informal) return { icon: "rose", label: `${relationshipLabel(kind)} courtship; formal and public` };
    return exposed
      ? { icon: "eye-target", label: "informal courtship; known to family" }
      : { icon: "lockpicks", label: "informal courtship; private" };
  };
  const contextualMutationIsCurrent = (binding, activeToken, currentGeneration, currentNpcId, currentPath, currentView) => Boolean(
    binding
    && binding.token === activeToken
    && binding.selectionGeneration === currentGeneration
    && binding.npcId === currentNpcId
    && binding.path === currentPath
    && binding.sessionId === currentView?.session_id
    && binding.revision === currentView?.revision
  );
  const affinityPresentation = (value) => {
    const score = Number.isFinite(Number(value)) ? Number(value) : 0;
    if (score >= 50) return { band: "very-warm", label: "Very warm regard", face: "☺" };
    if (score >= 15) return { band: "warm", label: "Warm regard", face: "🙂" };
    if (score <= -50) return { band: "hostile", label: "Hostile regard", face: "☹" };
    if (score <= -15) return { band: "cold", label: "Cold regard", face: "🙁" };
    return { band: "neutral", label: "Neutral regard", face: "😐" };
  };
  const moraleTopicPresentation = (value) => {
    const score = Math.max(-5, Math.min(5, Number(value) || 0));
    const direction = score < 0 ? "negative" : score > 0 ? "positive" : "neutral";
    const endpoint = score < 0 ? "#cf4f4f" : score > 0 ? "#4fae67" : "#d7b650";
    const yellow = score === 0 ? 100 : Math.max(10, 100 - Math.abs(score) * 18);
    return { direction, label: `${direction[0].toUpperCase()}${direction.slice(1)} morale, ${score >= 0 ? "+" : ""}${score.toFixed(1)}`, color: `color-mix(in srgb, #d7b650 ${yellow}%, ${endpoint})` };
  };

  if (typeof module !== "undefined" && module.exports) {
    module.exports = {
      createRetriableAction,
      dialogueCompletion,
      dialogueSubmission,
      dialogueResponseIsCurrent,
      dialogueTopicPayload,
      exactCandidate,
      relationshipLabel,
      relationshipLevel,
      romanticResponse,
      socialDurationChoices,
      contextualMutationIsCurrent,
      courtshipPresentation,
      affinityPresentation,
      moraleTopicPresentation,
    };
  }
  if (typeof document === "undefined") return;

  let lifecycle;
  const mount = () => {
  lifecycle?.abort();
  lifecycle = new AbortController();
  const { signal } = lifecycle;
  document.querySelectorAll("[data-dialogue-category-tabs]").forEach((tablist) => {
    const dockChat = tablist.closest(".settlement-chat");
    const tabs = [...tablist.querySelectorAll("[role='tab']")];
    const activate = (tab, focus = false) => {
      tabs.forEach((candidate) => {
        const selected = candidate === tab;
        candidate.setAttribute("aria-selected", String(selected));
        candidate.tabIndex = selected ? 0 : -1;
        const panel = document.getElementById(candidate.getAttribute("aria-controls"));
        if (panel) panel.hidden = !selected;
      });
      if (focus) tab.focus();
    };
    tabs.forEach((tab, index) => {
      tab.addEventListener("click", async () => {
        if (tab.dataset.dialogueCategory === "tidings" && dockChat?.dataset.partySocialHref) {
          const response = await window.strategicFetch(dockChat.dataset.partySocialHref, { headers: { Accept: "text/html" } });
          if (!response.ok) { window.reportStrategicError(new Error(`Recent Tidings failed (${response.status})`), "open Recent Tidings"); return; }
          const page = new DOMParser().parseFromString(await response.text(), "text/html");
          const replacement = page.querySelector("[data-social-conversation]");
          if (!replacement) { window.reportStrategicError(new Error("Recent Tidings response was incomplete"), "open Recent Tidings"); return; }
          dockChat.replaceWith(replacement); document.dispatchEvent(new Event("strategic-page-mounted")); return;
        }
        activate(tab);
      }, { signal });
      tab.addEventListener("keydown", (event) => {
        if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
        event.preventDefault();
        const next = event.key === "Home" ? 0 : event.key === "End" ? tabs.length - 1 : (index + (event.key === "ArrowRight" ? 1 : -1) + tabs.length) % tabs.length;
        activate(tabs[next], true);
      }, { signal });
    });
  });
  document.querySelectorAll("[data-social-conversation]").forEach((dock) => {
    const tabs = [...dock.querySelectorAll("[role='tab'][data-conversation-tab]")];
    const activate = (tab, focus = false) => {
      tabs.forEach((candidate) => {
        const selected = candidate === tab;
        candidate.setAttribute("aria-selected", String(selected));
        candidate.tabIndex = selected ? 0 : -1;
        const panel = dock.querySelector(`#${CSS.escape(candidate.getAttribute("aria-controls"))}`);
        if (panel) panel.hidden = !selected;
      });
      if (focus) tab.focus();
    };
    tabs.forEach((tab, index) => {
      tab.addEventListener("click", () => activate(tab), { signal });
      tab.addEventListener("keydown", (event) => {
        if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
        event.preventDefault();
        const next = event.key === "Home" ? 0 : event.key === "End" ? tabs.length - 1
          : (index + (event.key === "ArrowRight" ? 1 : -1) + tabs.length) % tabs.length;
        activate(tabs[next], true);
      }, { signal });
    });
    const popover = dock.querySelector("[data-affinity-popover]");
    const trigger = popover?.querySelector("[data-affinity-trigger]");
    const closeAffinity = () => {
      if (!popover) return;
      popover.classList.remove("is-pinned");
      popover.classList.add("is-closing");
      trigger?.setAttribute("aria-expanded", "false");
    };
    popover?.addEventListener("pointerenter", () => popover.classList.remove("is-closing"), { signal });
    popover?.addEventListener("focusin", () => popover.classList.remove("is-closing"), { signal });
    trigger?.addEventListener("click", (event) => {
      event.stopPropagation();
      popover.classList.remove("is-closing");
      const pinned = popover.classList.toggle("is-pinned");
      if (!pinned) popover.classList.add("is-closing");
      trigger.setAttribute("aria-expanded", String(pinned));
    }, { signal });
    dock.addEventListener("keydown", (event) => {
      if (event.key !== "Escape" || !popover?.classList.contains("is-pinned")) return;
      event.preventDefault(); trigger?.focus(); closeAffinity();
    }, { signal });
    document.addEventListener("click", (event) => {
      if (popover && !popover.contains(event.target)) closeAffinity();
    }, { signal });
    dock.querySelectorAll("[data-about-question]").forEach((topic) => topic.addEventListener("click", () => {
      const panel = topic.closest("[data-about-person]");
      const stream = dock.querySelector("[data-social-message-stream]");
      stream?.querySelector("[data-about-exchange]")?.remove();
      const exchange = document.createElement("div");
      exchange.dataset.aboutExchange = "true";
      exchange.className = "about-person-exchange";
      const question = document.createElement("p"); question.className = "chat-player-message"; question.textContent = topic.dataset.aboutQuestion;
      const answer = document.createElement("p"); answer.className = "chat-npc-message"; answer.textContent = topic.dataset.aboutAnswer;
      exchange.append(question, answer); stream?.append(exchange);
    }, { signal }));
  });
  const chat = document.querySelector("[data-local-chat-subject][data-dialogue-catalog-revision]");
  if (!chat) return;
  const messages = chat.querySelector(".settlement-chat-messages");
  const input = chat.querySelector(".settlement-chat-composer input");
  const send = chat.querySelector(".settlement-chat-composer button");
  const completion = chat.querySelector("[data-dialogue-completion]");
  const npcStrip = document.querySelector("[data-npc-strip]");
  const npcDescription = document.querySelector("[data-npc-description]");
  let currentView = null;
  let currentSocial = null;
  let selectionGeneration = 0;
  let startInFlight = null;
  let contextualMutation = null;

  const actionId = () => globalThis.crypto?.randomUUID?.() || `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  const request = async (path, payload) => {
    const response = await window.strategicFetch(path, { method: "POST", headers: { "Content-Type": "application/json", Accept: "application/json" }, body: JSON.stringify(payload) });
    if (!response.ok) {
      const error = new Error(`Dialogue action failed (${response.status})`);
      error.status = response.status;
      throw error;
    }
    return response.json();
  };
  const sourceLink = (source) => {
    if (!source?.edit_url) return null;
    const link = document.createElement("a"); link.className = "dialogue-source-link"; link.href = source.edit_url; link.target = "_blank"; link.rel = "noopener noreferrer"; link.hidden = !document.documentElement.hasAttribute("data-developer-mode"); link.setAttribute("aria-label", `Edit dialogue source at ${source.file} line ${source.line}`); link.title = `Edit ${source.file}:${source.line}`; const icon = document.createElement("span"); icon.className = "dialogue-source-icon"; icon.setAttribute("aria-hidden", "true"); link.append(icon); return link;
  };
  const socialPath = () => {
    const npcId = chat.dataset.localChatSubject || "";
    const settlement = npcStrip?.dataset.npcSettlement || "";
    const location = npcStrip?.dataset.npcLocation || "";
    if (!npcId || !settlement || !location) return null;
    return `/api/settlements/${encodeURIComponent(settlement)}/locations/${encodeURIComponent(location)}/npcs/${encodeURIComponent(npcId)}/social`;
  };
  const contextualRow = (speaker, body, player = false) => {
    const row = document.createElement("div");
    row.className = player ? "chat-player-message" : "chat-npc-message";
    row.dataset.chatChannel = "local";
    row.dataset.dialogueContextual = "true";
    const timestamp = document.createElement("span"); timestamp.className = "chat-timestamp"; timestamp.textContent = "[--:--] ";
    const name = document.createElement("strong"); name.textContent = `${speaker}: `;
    row.append(timestamp, name, document.createTextNode(body));
    return row;
  };
  const renderRelationshipVitals = () => {
    if (!npcDescription) return;
    if (!currentSocial) return;
    // Relationship state is spoken under Of Thee. The header retains only a
    // qualitative, non-authoritative regard face as an at-a-glance invitation.
    const header = chat.querySelector(".conversation-dock-header");
    header?.querySelector("[data-npc-affinity-popover]")?.remove();
    if (header) {
      const popover = document.createElement("div"); popover.dataset.npcAffinityPopover = "true"; popover.className = `affinity-popover affinity-${currentSocial.affinity || "neutral"}`;
      const face = document.createElement("button"); face.type = "button"; face.dataset.npcAffinityFace = "true"; face.className = "affinity-face";
      const presentation = ({ hostile: ["☹", "Hostile regard"], reserved: ["🙁", "Reserved regard"], warm: ["🙂", "Warm regard"], trusted: ["☺", "Very warm regard"] })[currentSocial.affinity] || ["😐", "Neutral regard"];
      const details = document.createElement("section"); details.className = "affinity-details"; details.id = `npc-affinity-${currentSocial.resident_character_id}`; details.dataset.affinityDetails = "true";
      face.textContent = presentation[0]; face.setAttribute("aria-label", presentation[1]); face.title = `${presentation[1]}; ask under Of Thee for more`; face.setAttribute("aria-expanded", "false"); face.setAttribute("aria-controls", details.id);
      const heading = document.createElement("h3"); heading.textContent = "Thy impression";
      const familiarity = document.createElement("p"); familiarity.textContent = `Familiarity: ${relationshipLabel(currentSocial.familiarity)}.`;
      const unavailable = document.createElement("p"); unavailable.className = "text-muted small-copy"; unavailable.textContent = "No observer-safe trait impression is available.";
      details.append(heading, familiarity, unavailable); popover.append(face, details);
      const close = () => { popover.classList.remove("is-pinned"); popover.classList.add("is-closing"); face.setAttribute("aria-expanded", "false"); };
      face.addEventListener("click", (event) => { event.stopPropagation(); popover.classList.remove("is-closing"); const pinned = popover.classList.toggle("is-pinned"); if (!pinned) popover.classList.add("is-closing"); face.setAttribute("aria-expanded", String(pinned)); }, { signal });
      popover.addEventListener("pointerenter", () => popover.classList.remove("is-closing"), { signal });
      popover.addEventListener("focusin", () => popover.classList.remove("is-closing"), { signal });
      document.addEventListener("click", (event) => { if (!popover.contains(event.target)) close(); }, { signal });
      popover.addEventListener("keydown", (event) => { if (event.key === "Escape") { event.preventDefault(); face.focus(); close(); } }, { signal });
      header.insertBefore(popover, header.querySelector("[data-dialogue-category-tabs]"));
    }
  };
  const topicAnchor = (topic, binding) => {
    const anchor = document.createElement("a"); anchor.href = "#"; anchor.className = "chat-quest-link"; anchor.textContent = topic.label; anchor.dataset.dialogueTopic = topic.id; anchor.dataset.dialogueSession = binding.sessionId; anchor.dataset.dialogueRevision = String(binding.revision); anchor.dataset.dialogueGeneration = String(binding.selectionGeneration); anchor.dataset.dialogueNpc = binding.npcId; const edit = sourceLink(topic.source); if (!edit) return anchor; const fragment = document.createDocumentFragment(); fragment.append(anchor, edit); return fragment;
  };
  const renderCategoryTopics = (view, binding) => {
    document.querySelectorAll("[data-dialogue-category-panel]").forEach((panel) => {
      panel.replaceChildren();
      const category = panel.dataset.dialogueCategoryPanel;
      if (category === "tidings") {
        const unavailable = document.createElement("p"); unavailable.className = "conversation-empty";
        unavailable.textContent = "Their private recent tidings are not thine to inspect."; panel.append(unavailable); return;
      }
      if (category === "about") {
        const topics = document.createElement("div"); topics.className = "about-person-topics";
        const questions = currentSocial?.about_topics || [];
        questions.forEach(({ question, answer }) => {
          const button = document.createElement("button"); button.type = "button"; button.className = "about-person-topic"; button.textContent = question;
          button.dataset.socialRevision = currentSocial.social_revision; button.dataset.socialSubject = currentSocial.resident_character_id;
          button.addEventListener("click", () => {
            if (button.dataset.socialRevision !== currentSocial?.social_revision || button.dataset.socialSubject !== chat.dataset.localChatSubject) return;
            appendContextExchange(question, answer);
          }, { signal }); topics.append(button);
        });
        if (topics.childElementCount) panel.append(topics);
        else { const empty = document.createElement("p"); empty.className = "conversation-empty"; empty.textContent = "No private answer is available."; panel.append(empty); }
        return;
      }
      const known = (view.topics || []).filter((topic) => (topic.category || "lore") === category);
      if (!known.length) { const empty = document.createElement("p"); empty.className = "conversation-empty"; empty.textContent = `No discovered ${category} topics are ready to discuss.`; panel.append(empty); return; }
      const list = document.createElement("ul"); list.className = "dialogue-category-topic-list";
      known.forEach((topic) => { const item = document.createElement("li"); item.append(topicAnchor(topic, { ...binding, topicId: topic.id })); list.append(item); });
      panel.append(list);
    });
  };
  const renderPrompt = (prompt) => {
    if (!prompt) return;
    const form = document.createElement("form"); form.className = "dialogue-prompt"; form.dataset.dialoguePrompt = prompt.id; form.dataset.dialogueScripted = "true";
    const group = document.createElement("fieldset"); const legend = document.createElement("legend"); legend.textContent = "Choose a response"; group.append(legend);
    if (prompt.mode === "YesNo") prompt.choices.forEach((choice) => { const button = document.createElement("button"); button.type = "submit"; button.name = "choice"; button.value = choice.id; button.className = "btn btn-small"; button.textContent = choice.label; const edit = sourceLink(choice.source); group.append(button); if (edit) group.append(edit); });
    else prompt.choices.forEach((choice) => { const label = document.createElement("label"); const choiceInput = document.createElement("input"); choiceInput.type = prompt.mode === "Multi" ? "checkbox" : "radio"; choiceInput.name = "choice"; choiceInput.value = choice.id; if (prompt.mode !== "Multi" && prompt.min_choices > 0) choiceInput.required = true; label.append(choiceInput, document.createTextNode(choice.label)); const edit = sourceLink(choice.source); if (edit) label.append(edit); group.append(label); });
    if (prompt.mode !== "YesNo") { const submit = document.createElement("button"); submit.type = "submit"; submit.className = "btn btn-small"; submit.textContent = "Answer"; group.append(submit); }
    form.append(group); messages.append(form);
  };
  const removeContextPrompt = () => messages?.querySelector("[data-dialogue-context-prompt]")?.remove();
  const setContextControlsDisabled = (disabled) => messages?.querySelectorAll("[data-dialogue-context-topics] button, [data-dialogue-context-prompt] button").forEach((button) => { button.disabled = disabled; });
  const beginContextMutation = () => {
    const path = socialPath();
    if (!path || contextualMutation || !currentView?.session_id) return null;
    const binding = { token: actionId(), selectionGeneration, npcId: chat.dataset.localChatSubject || "", path, sessionId: currentView.session_id, revision: currentView.revision };
    contextualMutation = binding; setContextControlsDisabled(true); return binding;
  };
  const contextMutationCurrent = (binding) => contextualMutationIsCurrent(binding, contextualMutation?.token, selectionGeneration, chat.dataset.localChatSubject || "", socialPath(), currentView);
  const finishContextMutation = (binding) => {
    if (!contextMutationCurrent(binding)) return;
    contextualMutation = null; setContextControlsDisabled(false); renderContextTopics();
  };
  const responseButton = ({ icon, label, detail }, onChoose) => {
    const button = document.createElement("button");
    button.type = "button"; button.className = "dialogue-context-response";
    button.title = detail || label; button.setAttribute("aria-label", `${label}. ${detail || ""}`.trim());
    const glyph = document.createElement("span"); glyph.className = "game-icon"; glyph.style.setProperty("--game-icon", `url('/static/icons/game/${icon}.svg')`); glyph.setAttribute("aria-hidden", "true");
    const line = document.createElement("span"); line.textContent = label;
    button.append(glyph, line); button.addEventListener("click", onChoose, { signal });
    return button;
  };
  const appendContextExchange = (playerLine, npcLine) => {
    messages.append(contextualRow("You", playerLine, true), contextualRow(currentSocial?.name || "Resident", npcLine));
    messages.scrollTop = messages.scrollHeight;
  };
  const refreshSocialContext = async (generation = selectionGeneration) => {
    const path = socialPath(); if (!path) return null;
    const response = await window.strategicBackgroundFetch(`dialogue-social:${path}`, path, { headers: { Accept: "application/json" } });
    if (!response.ok) return null;
    const social = await response.json();
    if (generation !== selectionGeneration || social.resident_character_id !== chat.dataset.localChatSubject) return null;
    currentSocial = social;
    if (currentView) renderCategoryTopics(currentView, { sessionId: currentView.session_id, revision: currentView.revision, selectionGeneration, npcId: chat.dataset.localChatSubject || "" });
    renderRelationshipVitals(); renderContextTopics(); return social;
  };
  const chooseSocialResponse = async (choice) => {
    const binding = beginContextMutation(); if (!binding) return;
    try {
      const response = await window.strategicFetch(binding.path, { method: "POST", headers: { "Content-Type": "application/json", Accept: "application/json" }, body: JSON.stringify({ requested_minutes: choice.minutes, action_id: binding.token }) });
      if (!response.ok) throw new Error(`Conversation failed (${response.status})`);
      const social = await response.json(); if (!contextMutationCurrent(binding)) return;
      currentSocial = social;
      const reaction = ({ positive: "I am glad we passed this time together.", mixed: "Our words stumbled at times, yet I am glad we spoke.", negative: "Let us leave our speech here, lest it sour further." })[currentSocial.last_outcome] || "I thank thee for speaking with me.";
      removeContextPrompt(); appendContextExchange(choice.label, reaction); renderRelationshipVitals(); renderContextTopics();
    } catch (error) { if (contextMutationCurrent(binding)) window.reportStrategicError(error, "choose conversation response"); }
    finally { finishContextMutation(binding); }
  };
  const chooseRomanticResponse = async (action, responseView) => {
    const binding = beginContextMutation(); if (!binding) return;
    try {
      const response = await window.strategicFetch(binding.path.replace(/\/social$/, `/romance/${encodeURIComponent(action)}`), { method: "POST", headers: { Accept: "application/json" } });
      if (!response.ok) throw new Error(`Relationship response failed (${response.status})`);
      const result = await response.json(); if (!contextMutationCurrent(binding)) return;
      if (!result || typeof result.ok !== "boolean" || typeof result.message !== "string" || !result.view) throw new Error("Relationship response returned an invalid result");
      currentSocial = result.view; removeContextPrompt(); appendContextExchange(responseView.line, result.message); renderRelationshipVitals(); renderContextTopics();
    } catch (error) { if (contextMutationCurrent(binding)) window.reportStrategicError(error, "choose relationship response"); }
    finally { finishContextMutation(binding); }
  };
  const renderContextPrompt = (kind) => {
    removeContextPrompt(); if (!messages || !currentSocial || currentView?.open_prompt || contextualMutation) return;
    const panel = document.createElement("section"); panel.className = "dialogue-context-prompt"; panel.dataset.dialogueContextPrompt = kind;
    panel.setAttribute("aria-label", kind === "social" ? "Ways to spend time together" : "Relationship responses");
    const choices = document.createElement("div"); choices.className = "dialogue-context-responses";
    if (kind === "social") socialDurationChoices.forEach((choice) => choices.append(responseButton(choice, () => chooseSocialResponse(choice))));
    else (currentSocial.romantic_actions || []).forEach((action) => { const view = romanticResponse(action); if (view) choices.append(responseButton({ ...view, detail: view.line }, () => chooseRomanticResponse(action, view))); });
    panel.append(choices); messages.append(panel); panel.querySelector("button")?.focus(); messages.scrollTop = messages.scrollHeight;
  };
  const contextTopicButton = (kind, icon, label) => {
    const button = document.createElement("button"); button.type = "button"; button.className = `dialogue-context-topic dialogue-context-topic-${kind}`; button.title = label; button.setAttribute("aria-label", label);
    const glyph = document.createElement("span"); glyph.className = "game-icon"; glyph.style.setProperty("--game-icon", `url('/static/icons/game/${icon}.svg')`); glyph.setAttribute("aria-hidden", "true"); button.append(glyph);
    button.addEventListener("click", () => renderContextPrompt(kind), { signal }); return button;
  };
  const renderContextTopics = () => {
    messages?.querySelector("[data-dialogue-context-topics]")?.remove();
    if (!messages || !currentSocial || !currentView?.session_id || currentView.open_prompt || contextualMutation) return;
    const topics = document.createElement("nav"); topics.className = "dialogue-context-topics"; topics.dataset.dialogueContextTopics = "true"; topics.setAttribute("aria-label", `Conversation topics for ${currentSocial.name}`);
    topics.append(contextTopicButton("social", "conversation", "Spend time talking"));
    if ((currentSocial.romantic_actions || []).length) topics.append(contextTopicButton("romance", "rose", "Courtship and marriage"));
    messages.append(topics);
  };
  let expandedClaimToken = null;
  const affinityFeedback = (delta) => {
    if (Math.abs(delta) < 0.05) return "Affinity: no change";
    const sign = delta > 0 ? "+" : "−";
    return `Affinity ${sign}${Math.abs(delta).toFixed(1)}`;
  };
  const claimResponsePanel = (claim, binding) => {
    const panel = document.createElement("section");
    panel.className = "dialogue-claim-responses";
    panel.dataset.claimPanel = claim.challenge_token;
    panel.id = `dialogue-claim-panel-${claim.challenge_token}`;
    panel.setAttribute("aria-label", `Responses concerning ${claim.value}`);
    const echo = document.createElement("p");
    echo.className = "dialogue-claim-echo";
    echo.textContent = `You: “${claim.value}?”`;
    panel.append(echo);
    if (claim.resolved) {
      const feedback = document.createElement("p");
      feedback.className = "dialogue-claim-feedback";
      feedback.setAttribute("role", "status");
      feedback.textContent = `${claim.outcome === "useful_answer" ? "They offer a useful answer." : "They do not yield on that point."} ${affinityFeedback(claim.affinity_delta)}`;
      panel.append(feedback);
      return panel;
    }
    const controls = document.createElement("div");
    controls.className = "dialogue-claim-actions";
    const approaches = [
      claim.charm_response && { approach: "charm", label: "Charm", icon: "rose", description: "A low-risk, low-leverage appeal.", line: claim.charm_response },
      claim.command_response && { approach: "command", label: "Command", icon: "crown", description: "A medium-risk demand that always strains affinity.", line: claim.command_response },
      claim.bluff_response && { approach: "bluff", label: "Bluff", icon: "conversation", description: "A high-risk, high-leverage deception.", line: claim.bluff_response },
    ].filter(Boolean);
    approaches.forEach(({ approach, label, icon, description, line }) => {
      const control = document.createElement("button");
      control.type = "button";
      control.className = "social-action dialogue-claim-action";
      control.dataset.strategicTooltip = `${description} Takes 5 minutes.`;
      control.setAttribute("aria-label", `${label}. ${line} ${description} Takes 5 minutes.`);
      const iconMask = document.createElement("span");
      iconMask.className = "game-icon";
      iconMask.style.setProperty("--game-icon", `url('/static/icons/game/${icon}.svg')`);
      iconMask.setAttribute("aria-hidden", "true");
      const shortLabel = document.createElement("strong");
      shortLabel.textContent = label;
      const responseLine = document.createElement("span");
      responseLine.className = "dialogue-claim-response-line";
      responseLine.textContent = line;
      control.append(iconMask, shortLabel, responseLine);
      control.addEventListener("click", () => {
        controls.querySelectorAll("button").forEach((button) => { button.disabled = true; });
        request("/api/dialogue/claim-response", {
          session_id: binding.sessionId,
          challenge_token: claim.challenge_token,
          approach,
          action_id: actionId(),
          expected_revision: binding.revision,
        }).then((view) => {
          if (dialogueResponseIsCurrent(binding, selectionGeneration, chat.dataset.localChatSubject || "", currentView)) render(view);
        }).catch((error) => {
          if (dialogueResponseIsCurrent(binding, selectionGeneration, chat.dataset.localChatSubject || "", currentView)) {
            window.reportStrategicError(error, `${label.toLowerCase()} claim response`);
            controls.querySelectorAll("button").forEach((button) => { button.disabled = false; });
          }
        });
      }, { signal });
      controls.append(control);
    });
    panel.append(controls);
    return panel;
  };
  const claimControl = (claim, binding, row) => {
    const control = document.createElement("button");
    control.type = "button";
    control.className = `dialogue-claim dialogue-claim-${claim.assessment_direction}`;
    const strength = Math.max(0, Math.min(1, claim.assessment_strength));
    control.style.setProperty("--claim-mix", `${55 + strength * 45}%`);
    control.style.setProperty("--claim-bg-mix", `${10 + strength * 12}%`);
    const state = ({ unknown: "Insight is uncertain", likely_false: "Insight leans untrue", likely_true: "Insight leans true" })[claim.assessment_direction] || "Insight is uncertain";
    control.setAttribute("aria-label", `${claim.value}. ${state}. Open responses.`);
    control.setAttribute("aria-controls", `dialogue-claim-panel-${claim.challenge_token}`);
    control.setAttribute("aria-expanded", String(expandedClaimToken === claim.challenge_token));
    control.textContent = claim.value;
    const open = () => {
      document.querySelectorAll(".dialogue-claim-responses").forEach((panel) => panel.remove());
      expandedClaimToken = expandedClaimToken === claim.challenge_token ? null : claim.challenge_token;
      document.querySelectorAll(".dialogue-claim").forEach((candidate) => candidate.setAttribute("aria-expanded", "false"));
      if (expandedClaimToken) {
        control.setAttribute("aria-expanded", "true");
        row.append(claimResponsePanel(claim, binding));
      }
    };
    control.addEventListener("click", open, { signal });
    return control;
  };
  // Topics are exposed by highlighted phrases in dialogue, not by guessing
  // hidden topic labels in the free-text box.
  const activeCandidates = () => currentView?.open_prompt?.choices || [];
  const isMultiPrompt = () => currentView?.open_prompt?.mode === "Multi";
  const refreshCompletion = () => {
    if (!input || !completion) return;
    const suggestion = dialogueCompletion(input.value, activeCandidates(), isMultiPrompt());
    if (suggestion) {
      const typed = document.createElement("span");
      typed.className = "settlement-chat-completion-prefix";
      typed.textContent = input.value;
      completion.replaceChildren(typed, document.createTextNode(suggestion.slice(input.value.length)));
    } else completion.replaceChildren();
    input.setAttribute("aria-autocomplete", suggestion ? "inline" : "none");
    input.dataset.dialogueSuggestion = suggestion || "";
  };
  const render = (view) => {
    currentView = view;
    if (contextualMutation && !contextMutationCurrent(contextualMutation)) contextualMutation = null;
    removeContextPrompt();
    const binding = {
      sessionId: view.session_id,
      revision: view.revision,
      selectionGeneration,
      npcId: chat.dataset.localChatSubject || "",
    };
    renderCategoryTopics(view, binding);
    messages?.querySelectorAll("[data-dialogue-scripted]").forEach((node) => node.remove());
    view.events.forEach((event) => {
      const row = document.createElement("div");
      row.className = event.speaker_is_player ? "chat-player-message" : "chat-npc-message";
      row.dataset.chatChannel = "local";
      row.dataset.dialogueScripted = "true";
      const timestamp = document.createElement("span"); timestamp.className = "chat-timestamp"; timestamp.textContent = "[--:--] ";
      const speaker = document.createElement("strong"); speaker.textContent = `${event.speaker_name}: `;
      row.append(timestamp, speaker);
      event.fragments.forEach(({ fragment, source, claim }) => {
        if (fragment.kind === "text") {
          row.append(document.createTextNode(fragment.value));
          const edit = sourceLink(source); if (edit) row.append(edit);
        }
        else if (fragment.kind === "claim") {
          row.append(claim ? claimControl({ ...claim, value: fragment.value }, binding, row) : document.createTextNode(fragment.value));
          const edit = sourceLink(source); if (edit) row.append(edit);
        }
        else if (fragment.kind === "period_claim") { const claim = document.createElement("q"); claim.className = "dialogue-period-claim"; claim.textContent = fragment.value; row.append(claim); const edit = sourceLink(source); if (edit) row.append(edit); }
        else if (fragment.kind === "authoritative_explanation") { const explanation = document.createElement("span"); explanation.className = "dialogue-authoritative-explanation"; explanation.dataset.reference = fragment.reference; explanation.textContent = fragment.value; row.append(explanation); const edit = sourceLink(source); if (edit) row.append(edit); }
        else if (fragment.kind === "topic") row.append(topicAnchor({ id: fragment.topic, label: fragment.label, source }, { ...binding, topicId: fragment.topic }));
      });
      messages.append(row);
      const expanded = event.fragments
        .map((entry) => entry.fragment.kind === "claim" && entry.claim ? { ...entry.claim, value: entry.fragment.value } : null)
        .find((claim) => claim?.challenge_token === expandedClaimToken);
      if (expanded) row.append(claimResponsePanel(expanded, binding));
    });
    renderPrompt(view.open_prompt);
    if (view.order_errantry_offer) {
      const row = document.createElement("div");
      row.className = "chat-system-message";
      row.dataset.chatChannel = "info";
      row.dataset.dialogueScripted = "true";
      const button = document.createElement("button");
      button.type = "button";
      button.className = "btn btn-primary";
      button.textContent = "Accept the Order's errantry";
      const acceptance = createRetriableAction(actionId);
      button.addEventListener("click", () => {
        button.disabled = true;
        acceptance.run((acceptanceActionId) => request("/api/dialogue/accept-order-errantry", {
          session_id: view.session_id,
          action_id: acceptanceActionId,
        })).then((result) => {
          window.location.assign(result.redirect);
        }).catch((error) => {
          button.disabled = false;
          window.reportStrategicError(error, "accept Order errantry");
        });
      }, { signal });
      row.append(button);
      messages.append(row);
    }
    renderContextTopics();
    refreshCompletion();
    messages.scrollTop = messages.scrollHeight;
  };
  const chooseTopic = (binding) => {
    const payload = dialogueTopicPayload(
      binding,
      selectionGeneration,
      chat.dataset.localChatSubject || "",
      actionId(),
    );
    if (!payload) return null;
    return request("/api/dialogue/topic", payload).then((view) => {
      if (dialogueResponseIsCurrent(
        binding,
        selectionGeneration,
        chat.dataset.localChatSubject || "",
        currentView,
      )
        && binding.sessionId === view.session_id) render(view);
    }).catch((error) => {
      if (dialogueResponseIsCurrent(
        binding,
        selectionGeneration,
        chat.dataset.localChatSubject || "",
        currentView,
      )) {
        window.reportStrategicError(error, "choose dialogue topic");
      }
    });
  };
  const answerPrompt = (choices) => {
    const prompt = currentView.open_prompt;
    if (!prompt || choices.length < prompt.min_choices || choices.length > prompt.max_choices) {
      window.reportStrategicError(new Error(`Choose between ${prompt?.min_choices ?? 0} and ${prompt?.max_choices ?? 0} responses.`), "answer dialogue prompt");
      return;
    }
    const binding = {
      sessionId: currentView.session_id,
      revision: currentView.revision,
      selectionGeneration,
      npcId: chat.dataset.localChatSubject || "",
    };
    return request("/api/dialogue/answer", { session_id: binding.sessionId, prompt_row_id: prompt.id, choice_ids: choices, action_id: actionId(), expected_revision: binding.revision }).then((view) => {
      if (dialogueResponseIsCurrent(
        binding,
        selectionGeneration,
        chat.dataset.localChatSubject || "",
        currentView,
      ) && binding.sessionId === view.session_id) render(view);
    }).catch((error) => {
      if (dialogueResponseIsCurrent(
        binding,
        selectionGeneration,
        chat.dataset.localChatSubject || "",
        currentView,
      )) window.reportStrategicError(error, "answer dialogue prompt");
    });
  };
  const submitTypedAction = () => {
    if (!currentView || !input) return false;
    const ids = dialogueSubmission(input.value, activeCandidates(), isMultiPrompt());
    if (!ids) return false;
    input.value = "";
    refreshCompletion();
    if (currentView.open_prompt) answerPrompt(ids);
    else chooseTopic({
      sessionId: currentView.session_id,
      topicId: ids[0],
      revision: currentView.revision,
      selectionGeneration,
      npcId: chat.dataset.localChatSubject || "",
    });
    return true;
  };

  input?.addEventListener("input", refreshCompletion, { signal });
  input?.addEventListener("keydown", (event) => {
    if (event.key === "Tab" && input.dataset.dialogueSuggestion) {
      event.preventDefault();
      input.value = input.dataset.dialogueSuggestion;
      refreshCompletion();
      return;
    }
    if (event.key === "Enter" && submitTypedAction()) {
      event.preventDefault();
      event.stopImmediatePropagation();
    }
  }, { capture: true, signal });
  send?.addEventListener("click", (event) => {
    if (!submitTypedAction()) return;
    event.preventDefault();
    event.stopImmediatePropagation();
  }, { capture: true, signal });
  document.addEventListener("click", (event) => { const topic = event.target.closest("[data-dialogue-topic]"); if (!topic || event.target.closest(".dialogue-source-link")) return; event.preventDefault(); chooseTopic({ sessionId: topic.dataset.dialogueSession, topicId: topic.dataset.dialogueTopic, revision: Number(topic.dataset.dialogueRevision), selectionGeneration: Number(topic.dataset.dialogueGeneration), npcId: topic.dataset.dialogueNpc }); }, { signal });
  document.addEventListener("submit", (event) => { const form = event.target.closest("[data-dialogue-prompt]"); if (!form) return; event.preventDefault(); const submitter = event.submitter; const choices = submitter?.name === "choice" ? [submitter.value] : Array.from(new FormData(form).getAll("choice"), String); answerPrompt(choices); }, { signal });
  const begin = () => {
    if (!chat.dataset.localChatSubject) return;
    const generation = selectionGeneration;
    const actor = chat.dataset.localChatSubject;
    const location = npcStrip?.dataset.npcLocation || "";
    const key = `${actor}:${location}:${generation}`;
    if (startInFlight?.key === key) return startInFlight.promise;
    const promise = request("/api/dialogue/start", { npc_actor_id: actor, location_id: location }).then((view) => {
      if (generation === selectionGeneration && chat.dataset.localChatSubject === actor) {
        render(view);
        return refreshSocialContext(generation);
      }
      return null;
    }).catch((error) => { if (generation === selectionGeneration) window.reportStrategicError(error, "start dialogue"); }).finally(() => { if (startInFlight?.key === key) startInFlight = null; });
    startInFlight = { key, promise };
    return promise;
  };
  const selectNpc = (npc, button) => {
    selectionGeneration += 1;
    contextualMutation = null;
    chat.dataset.localChatSubject = npc.id;
    chat.dispatchEvent(new Event("local-chat-subject-changed"));
    currentView = null;
    currentSocial = null;
    chat.querySelector("[data-npc-affinity-popover]")?.remove();
    document.querySelectorAll("[data-dialogue-category-panel]").forEach((panel) => {
      panel.replaceChildren(); const loading = document.createElement("p"); loading.className = "conversation-empty"; loading.textContent = "Loading this person's conversation…"; panel.append(loading);
    });
    messages?.querySelectorAll("[data-dialogue-scripted], [data-dialogue-contextual], [data-dialogue-context-prompt], [data-dialogue-context-topics]").forEach((node) => node.remove());
    refreshCompletion();
    npcStrip?.querySelectorAll(".settlement-npc-portrait").forEach((candidate) => {
      const active = candidate === button;
      candidate.classList.toggle("active", active);
      candidate.setAttribute("aria-pressed", String(active));
      candidate.tabIndex = active ? 0 : -1;
    });
    if (npcDescription) {
      const placeholder = document.createElement("div"); placeholder.className = npc.initials ? "visual-stage-placeholder" : "visual-stage-placeholder npc-portrait-silhouette"; placeholder.setAttribute("aria-hidden", "true"); placeholder.textContent = npc.initials || "";
      const heading = document.createElement("h2"); heading.textContent = npc.name;
      const description = document.createElement("p"); description.textContent = npc.description;
      npcDescription.replaceChildren(placeholder, heading, description);
    }
    begin();
  };
  const loadPeople = async () => {
    if (!npcStrip) { begin(); return; }
    const path = `/api/settlements/${encodeURIComponent(npcStrip.dataset.npcSettlement)}/locations/${encodeURIComponent(npcStrip.dataset.npcLocation)}/npcs`;
    const response = await window.strategicFetch(path, { headers: { Accept: "application/json" } });
    if (!response.ok) throw new Error(`Could not load people here (${response.status})`);
    const people = await response.json();
    if (!people.length) { npcStrip.textContent = "Nobody is available here just now."; return; }
    const buttons = people.map((npc) => {
      const button = document.createElement("button"); button.type = "button"; button.className = "party-portrait settlement-npc-portrait"; button.dataset.npcId = npc.id; button.setAttribute("aria-label", `Talk to ${npc.name}`); button.setAttribute("aria-pressed", "false"); button.tabIndex = -1;
      const portrait = document.createElement("span"); portrait.className = "party-portrait-initial settlement-npc-initials";
      const face = document.createElement("span"); face.className = npc.initials ? "party-portrait-face" : "party-portrait-face npc-portrait-silhouette"; face.setAttribute("aria-hidden", "true"); face.textContent = npc.initials || "";
      const name = document.createElement("span"); name.className = "party-portrait-name settlement-npc-name"; name.textContent = npc.name;
      portrait.append(face, name); button.append(portrait); button.addEventListener("click", () => selectNpc(npc, button));
      button.addEventListener("keydown", (event) => { if (!['ArrowLeft', 'ArrowRight'].includes(event.key)) return; event.preventDefault(); const offset = event.key === 'ArrowRight' ? 1 : -1; buttons[(buttons.indexOf(button) + offset + buttons.length) % buttons.length].focus(); });
      return button;
    });
    npcStrip.replaceChildren(...buttons);
    const defaultIndex = Math.max(0, people.findIndex((npc) => npc.is_default));
    selectNpc(people[defaultIndex], buttons[defaultIndex]);
  };
  loadPeople().catch((error) => window.reportStrategicError(error, "load settlement NPCs"));
  };
  mount();
  document.addEventListener("strategic-page-mounted", mount);
  document.addEventListener("strategic-page-unmounting", () => lifecycle?.abort());
})();
