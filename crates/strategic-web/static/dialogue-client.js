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

  if (typeof module !== "undefined" && module.exports) {
    module.exports = { dialogueCompletion, dialogueSubmission, exactCandidate };
  }
  if (typeof document === "undefined") return;

  const chat = document.querySelector("[data-local-chat-subject][data-dialogue-catalog-revision]");
  if (!chat) return;
  const messages = chat.querySelector(".settlement-chat-messages");
  const topicPane = chat.querySelector("[data-dialogue-topic-pane]");
  const topicList = chat.querySelector("[data-dialogue-topic-list]");
  const input = chat.querySelector(".settlement-chat-composer input");
  const send = chat.querySelector(".settlement-chat-composer button");
  const completion = chat.querySelector("[data-dialogue-completion]");
  let currentView = null;

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
  const topicAnchor = (topic) => {
    const anchor = document.createElement("a"); anchor.href = "#"; anchor.className = "chat-quest-link"; anchor.textContent = topic.label; anchor.dataset.dialogueTopic = topic.id; const edit = sourceLink(topic.source); if (edit) anchor.append(edit); return anchor;
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
  const renderExamination = (examination) => {
    if (!examination) return;
    const diagnoses = examination.diagnoses.length ? examination.diagnoses : [{ disease_name: examination.message, medication_name: "" }];
    diagnoses.forEach((diagnosis) => { const row = document.createElement("div"); row.className = "chat-npc-message"; row.dataset.chatChannel = "local"; row.dataset.dialogueScripted = "true"; const timestamp = document.createElement("span"); timestamp.className = "chat-timestamp"; timestamp.textContent = "[--:--] "; const speaker = document.createElement("strong"); speaker.textContent = "Herbalist: "; row.append(timestamp, speaker, document.createTextNode(diagnosis.medication_name ? `You have ${diagnosis.disease_name}. I recommend ` : diagnosis.disease_name)); if (diagnosis.medication_name) { const medication = document.createElement("button"); medication.type = "button"; medication.className = "chat-quest-link"; medication.dataset.dialogueMedication = diagnosis.medication_name; medication.textContent = diagnosis.medication_name; row.append(medication, document.createTextNode(".")); } messages.append(row); });
  };
  const activeCandidates = () => currentView?.open_prompt?.choices || currentView?.topics || [];
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
        else if (fragment.kind === "topic") row.append(topicAnchor({ id: fragment.topic, label: fragment.label, source }));
      });
      messages.append(row);
    });
    renderPrompt(view.open_prompt);
    renderExamination(view.examination);
    if (topicPane && topicList) {
      topicList.replaceChildren(...view.topics.map((topic) => { const item = document.createElement("li"); item.append(topicAnchor(topic)); return item; }));
      topicPane.hidden = false;
    }
    refreshCompletion();
    messages.scrollTop = messages.scrollHeight;
  };
  const chooseTopic = (topicId) => request("/api/dialogue/topic", { session_id: currentView.session_id, topic_id: topicId, action_id: actionId(), expected_revision: currentView.revision }).then(render).catch((error) => window.reportStrategicError(error, "choose dialogue topic"));
  const answerPrompt = (choices) => {
    const prompt = currentView.open_prompt;
    if (!prompt || choices.length < prompt.min_choices || choices.length > prompt.max_choices) {
      window.reportStrategicError(new Error(`Choose between ${prompt?.min_choices ?? 0} and ${prompt?.max_choices ?? 0} responses.`), "answer dialogue prompt");
      return;
    }
    return request("/api/dialogue/answer", { session_id: currentView.session_id, prompt_row_id: prompt.id, choice_ids: choices, action_id: actionId(), expected_revision: currentView.revision }).then(render).catch((error) => window.reportStrategicError(error, "answer dialogue prompt"));
  };
  const submitTypedAction = () => {
    if (!currentView || !input) return false;
    const ids = dialogueSubmission(input.value, activeCandidates(), isMultiPrompt());
    if (!ids) return false;
    input.value = "";
    refreshCompletion();
    if (currentView.open_prompt) answerPrompt(ids);
    else chooseTopic(ids[0]);
    return true;
  };

  input?.addEventListener("input", refreshCompletion);
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
  }, true);
  send?.addEventListener("click", (event) => {
    if (!submitTypedAction()) return;
    event.preventDefault();
    event.stopImmediatePropagation();
  }, true);
  document.addEventListener("click", (event) => { const topic = event.target.closest("[data-dialogue-topic]"); if (!topic || event.target.closest(".dialogue-source-link")) return; event.preventDefault(); chooseTopic(topic.dataset.dialogueTopic); });
  document.addEventListener("click", (event) => { const medication = event.target.closest("[data-dialogue-medication]"); if (!medication) return; const rows = Array.from(document.querySelectorAll("[data-herbalist-medication-name]")); const row = rows.find((candidate) => candidate.dataset.herbalistMedicationName === medication.dataset.dialogueMedication); row?.scrollIntoView({ behavior: "smooth", block: "center" }); row?.querySelector("[data-merchant-buy]")?.focus(); });
  document.addEventListener("submit", (event) => { const form = event.target.closest("[data-dialogue-prompt]"); if (!form) return; event.preventDefault(); const submitter = event.submitter; const choices = submitter?.name === "choice" ? [submitter.value] : Array.from(new FormData(form).getAll("choice"), String); answerPrompt(choices); });
  const begin = () => request("/api/dialogue/start", { npc_actor_id: chat.dataset.localChatSubject }).then(render).catch((error) => window.reportStrategicError(error, "start dialogue"));
  if (chat.dataset.localChatReady === "true") begin(); else chat.addEventListener("local-chat-ready", begin, { once: true });
})();
