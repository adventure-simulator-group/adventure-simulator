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

  if (typeof module !== "undefined" && module.exports) {
    module.exports = {
      dialogueCompletion,
      dialogueSubmission,
      dialogueResponseIsCurrent,
      dialogueTopicPayload,
      exactCandidate,
    };
  }
  if (typeof document === "undefined") return;

  let lifecycle;
  const mount = () => {
  lifecycle?.abort();
  lifecycle = new AbortController();
  const { signal } = lifecycle;
  const chat = document.querySelector("[data-local-chat-subject][data-dialogue-catalog-revision]");
  if (!chat) return;
  const messages = chat.querySelector(".settlement-chat-messages");
  const input = chat.querySelector(".settlement-chat-composer input");
  const send = chat.querySelector(".settlement-chat-composer button");
  const completion = chat.querySelector("[data-dialogue-completion]");
  const npcStrip = document.querySelector("[data-npc-strip]");
  const npcDescription = document.querySelector("[data-npc-description]");
  let currentView = null;
  let selectionGeneration = 0;
  let startInFlight = null;

  const actionId = () => globalThis.crypto?.randomUUID?.() || `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  const request = async (path, payload) => {
    const response = await window.strategicFetch(path, { method: "POST", headers: { "Content-Type": "application/json", Accept: "application/json" }, body: JSON.stringify(payload) });
    if (!response.ok) throw new Error(`Dialogue action failed (${response.status})`);
    return response.json();
  };
  const sourceLink = (source) => {
    if (!source?.edit_url) return null;
    const link = document.createElement("a"); link.className = "dialogue-source-link"; link.href = source.edit_url; link.target = "_blank"; link.rel = "noopener noreferrer"; link.hidden = !document.documentElement.hasAttribute("data-developer-mode"); link.setAttribute("aria-label", `Edit dialogue source at ${source.file} line ${source.line}`); link.title = `Edit ${source.file}:${source.line}`; const icon = document.createElement("span"); icon.className = "dialogue-source-icon"; icon.setAttribute("aria-hidden", "true"); link.append(icon); return link;
  };
  const topicAnchor = (topic, binding) => {
    const anchor = document.createElement("a"); anchor.href = "#"; anchor.className = "chat-quest-link"; anchor.textContent = topic.label; anchor.dataset.dialogueTopic = topic.id; anchor.dataset.dialogueSession = binding.sessionId; anchor.dataset.dialogueRevision = String(binding.revision); anchor.dataset.dialogueGeneration = String(binding.selectionGeneration); anchor.dataset.dialogueNpc = binding.npcId; const edit = sourceLink(topic.source); if (!edit) return anchor; const fragment = document.createDocumentFragment(); fragment.append(anchor, edit); return fragment;
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
    const binding = {
      sessionId: view.session_id,
      revision: view.revision,
      selectionGeneration,
      npcId: chat.dataset.localChatSubject || "",
    };
    messages?.querySelectorAll("[data-dialogue-scripted]").forEach((node) => node.remove());
    view.events.forEach((event) => {
      const row = document.createElement("div");
      row.className = event.speaker_is_player ? "chat-player-message" : "chat-npc-message";
      row.dataset.chatChannel = "local";
      row.dataset.dialogueScripted = "true";
      const timestamp = document.createElement("span"); timestamp.className = "chat-timestamp"; timestamp.textContent = "[--:--] ";
      const speaker = document.createElement("strong"); speaker.textContent = `${event.speaker_name}: `;
      row.append(timestamp, speaker);
      event.fragments.forEach(({ fragment, source }) => {
        if (fragment.kind === "text") { row.append(document.createTextNode(fragment.value)); const edit = sourceLink(source); if (edit) row.append(edit); }
        else if (fragment.kind === "period_claim") { const claim = document.createElement("q"); claim.className = "dialogue-period-claim"; claim.textContent = fragment.value; row.append(claim); const edit = sourceLink(source); if (edit) row.append(edit); }
        else if (fragment.kind === "authoritative_explanation") { const explanation = document.createElement("span"); explanation.className = "dialogue-authoritative-explanation"; explanation.dataset.reference = fragment.reference; explanation.textContent = fragment.value; row.append(explanation); const edit = sourceLink(source); if (edit) row.append(edit); }
        else if (fragment.kind === "topic") row.append(topicAnchor({ id: fragment.topic, label: fragment.label, source }, { ...binding, topicId: fragment.topic }));
      });
      messages.append(row);
    });
    renderPrompt(view.open_prompt);
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
      if (generation === selectionGeneration && chat.dataset.localChatSubject === actor) render(view);
    }).catch((error) => { if (generation === selectionGeneration) window.reportStrategicError(error, "start dialogue"); }).finally(() => { if (startInFlight?.key === key) startInFlight = null; });
    startInFlight = { key, promise };
    return promise;
  };
  const selectNpc = (npc, button) => {
    selectionGeneration += 1;
    chat.dataset.localChatSubject = npc.id;
    chat.dispatchEvent(new Event("local-chat-subject-changed"));
    currentView = null;
    messages?.querySelectorAll("[data-dialogue-scripted]").forEach((node) => node.remove());
    refreshCompletion();
    npcStrip?.querySelectorAll("button").forEach((candidate) => {
      const active = candidate === button;
      candidate.classList.toggle("active", active);
      candidate.setAttribute("aria-pressed", String(active));
      candidate.tabIndex = active ? 0 : -1;
    });
    if (npcDescription) {
      const placeholder = document.createElement("div"); placeholder.className = "visual-stage-placeholder"; placeholder.setAttribute("aria-hidden", "true"); placeholder.textContent = npc.initials || "?";
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
      const face = document.createElement("span"); face.className = "party-portrait-face"; face.textContent = npc.initials || "?";
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
