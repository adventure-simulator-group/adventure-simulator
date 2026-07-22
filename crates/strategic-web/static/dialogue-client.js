(() => {
  "use strict";
  if (typeof document === "undefined") return;
  const chat = document.querySelector("[data-local-chat-subject][data-dialogue-catalog-revision]");
  if (!chat) return;
  const messages = chat.querySelector(".settlement-chat-messages");
  let currentView = null;
  const actionId = () => globalThis.crypto?.randomUUID?.() || `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  const request = async (path, payload) => {
    const response = await window.strategicFetch(path, { method: "POST", headers: { "Content-Type": "application/json", Accept: "application/json" }, body: JSON.stringify(payload) });
    if (!response.ok) throw new Error(`Dialogue action failed (${response.status})`);
    return response.json();
  };
  const sourceLink = (source) => {
    if (!source?.edit_url) return null;
    const link = document.createElement("a"); link.className = "dialogue-source-link"; link.href = source.edit_url; link.target = "_blank"; link.rel = "noopener noreferrer"; link.textContent = "Edit"; link.hidden = !document.documentElement.hasAttribute("data-developer-mode"); link.setAttribute("aria-label", `Edit dialogue source at ${source.file} line ${source.line}`); return link;
  };
  const topicAnchor = (topic) => {
    const anchor = document.createElement("a"); anchor.href = "#"; anchor.className = "chat-quest-link"; anchor.textContent = topic.label; anchor.dataset.dialogueTopic = topic.id; const edit=sourceLink(topic.source); if(edit) anchor.append(edit); return anchor;
  };
  const renderPrompt = (prompt) => {
    if (!prompt) return;
    const form=document.createElement("form");form.className="dialogue-prompt";form.dataset.dialoguePrompt=prompt.id;form.dataset.dialogueScripted="true";
    const group=document.createElement("fieldset");const legend=document.createElement("legend");legend.textContent="Choose a response";group.append(legend);
    if (prompt.mode === "YesNo") prompt.choices.forEach((choice)=>{const button=document.createElement("button");button.type="submit";button.name="choice";button.value=choice.id;button.className="btn btn-small";button.textContent=choice.label;const edit=sourceLink(choice.source);group.append(button);if(edit)group.append(edit);});
    else prompt.choices.forEach((choice)=>{const label=document.createElement("label");const input=document.createElement("input");input.type=prompt.mode==="Multi"?"checkbox":"radio";input.name="choice";input.value=choice.id;if(prompt.mode!=="Multi"&&prompt.min_choices>0)input.required=true;label.append(input,document.createTextNode(choice.label));const edit=sourceLink(choice.source);if(edit)label.append(edit);group.append(label);});
    if(prompt.mode!=="YesNo"){const submit=document.createElement("button");submit.type="submit";submit.className="btn btn-small";submit.textContent="Answer";group.append(submit);} form.append(group);messages.append(form);
  };
  const renderExamination = (examination) => {
    if (!examination) return;
    const diagnoses=examination.diagnoses.length?examination.diagnoses:[{disease_name:examination.message,medication_name:""}];
    diagnoses.forEach((diagnosis)=>{const row=document.createElement("div");row.className="chat-npc-message";row.dataset.chatChannel="local";row.dataset.dialogueScripted="true";const timestamp=document.createElement("span");timestamp.className="chat-timestamp";timestamp.textContent="[--:--] ";const speaker=document.createElement("strong");speaker.textContent="Herbalist: ";row.append(timestamp,speaker,document.createTextNode(diagnosis.medication_name?`You have ${diagnosis.disease_name}. I recommend `:diagnosis.disease_name));if(diagnosis.medication_name){const medication=document.createElement("button");medication.type="button";medication.className="chat-quest-link";medication.dataset.dialogueMedication=diagnosis.medication_name;medication.textContent=diagnosis.medication_name;row.append(medication,document.createTextNode("."));}messages.append(row);});
  };
  const render = (view) => {
    currentView=view;messages?.querySelectorAll("[data-dialogue-scripted]").forEach((node)=>node.remove());
    view.events.forEach((event) => {
      const row = document.createElement("div");
      row.className = event.speaker_is_player ? "chat-player-message" : "chat-npc-message";
      row.dataset.chatChannel = "local";
      row.dataset.dialogueScripted = "true";
      const timestamp = document.createElement("span");
      timestamp.className = "chat-timestamp";
      timestamp.textContent = "[--:--] ";
      const speaker = document.createElement("strong");
      speaker.textContent = `${event.speaker_name}: `;
      row.append(timestamp, speaker);
      event.fragments.forEach(({ fragment, source }) => {
        if (fragment.kind === "text") {
          row.append(document.createTextNode(fragment.value));
          const edit = sourceLink(source);
          if (edit) row.append(edit);
        } else if (fragment.kind === "topic") {
          row.append(topicAnchor({ id: fragment.topic, label: fragment.label, source }));
        }
      });
      messages.append(row);
    });
    renderPrompt(view.open_prompt);
    renderExamination(view.examination);
    const rail=document.querySelector(".right-sidebar");if(rail){rail.querySelector("[data-dialogue-topic-rail]")?.remove();const section=document.createElement("section");section.className="sidebar-section";section.dataset.dialogueTopicRail="true";const heading=document.createElement("h3");heading.className="sidebar-header";heading.textContent="Topics";const list=document.createElement("ul");view.topics.forEach((topic)=>{const item=document.createElement("li");item.append(topicAnchor(topic));list.append(item);});section.append(heading,list);rail.prepend(section);} messages.scrollTop=messages.scrollHeight;
  };
  document.addEventListener("click",(event)=>{const topic=event.target.closest("[data-dialogue-topic]");if(!topic||event.target.closest(".dialogue-source-link"))return;event.preventDefault();request("/api/dialogue/topic",{session_id:currentView.session_id,topic_id:topic.dataset.dialogueTopic,action_id:actionId(),expected_revision:currentView.revision}).then(render).catch((error)=>window.reportStrategicError(error,"choose dialogue topic"));});
  document.addEventListener("click",(event)=>{const medication=event.target.closest("[data-dialogue-medication]");if(!medication)return;const rows=Array.from(document.querySelectorAll("[data-herbalist-medication-name]"));const row=rows.find((candidate)=>candidate.dataset.herbalistMedicationName===medication.dataset.dialogueMedication);row?.scrollIntoView({behavior:"smooth",block:"center"});row?.querySelector("[data-merchant-buy]")?.focus();});
  document.addEventListener("submit",(event)=>{const form=event.target.closest("[data-dialogue-prompt]");if(!form)return;event.preventDefault();const submitter=event.submitter;const choices=submitter?.name==="choice"?[submitter.value]:Array.from(new FormData(form).getAll("choice"),String);const prompt=currentView.open_prompt;if(choices.length<prompt.min_choices||choices.length>prompt.max_choices){window.reportStrategicError(new Error(`Choose between ${prompt.min_choices} and ${prompt.max_choices} responses.`),"answer dialogue prompt");return;}request("/api/dialogue/answer",{session_id:currentView.session_id,prompt_row_id:form.dataset.dialoguePrompt,choice_ids:choices,action_id:actionId(),expected_revision:currentView.revision}).then(render).catch((error)=>window.reportStrategicError(error,"answer dialogue prompt"));});
  const begin=()=>request("/api/dialogue/start",{npc_actor_id:chat.dataset.localChatSubject}).then(render).catch((error)=>window.reportStrategicError(error,"start dialogue"));
  if(chat.dataset.localChatReady==="true")begin();else chat.addEventListener("local-chat-ready",begin,{once:true});
})();
