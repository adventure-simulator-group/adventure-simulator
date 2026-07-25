(() => {
  "use strict";

  const strip = document.querySelector("[data-evidence-strip]");
  if (!strip) return;
  const stage = document.querySelector("[data-evidence-description]");
  const messages = document.querySelector(".settlement-chat-messages");
  const input = document.querySelector(".settlement-chat-composer input");
  const send = document.querySelector(".settlement-chat-composer button");
  const caseSiteId = strip.dataset.evidenceCaseSite || "";
  let evidence = [];
  let selectedId = "";

  const actionId = () => globalThis.crypto?.randomUUID?.()
    || `${Date.now()}-${Math.random().toString(36).slice(2)}`;

  const request = async (path, options = {}) => {
    const response = await window.strategicFetch(path, {
      headers: { Accept: "application/json", ...(options.headers || {}) },
      ...options,
    });
    if (!response.ok) throw new Error(`Physical-evidence action failed (${response.status})`);
    return response.json();
  };

  const icon = (name, label) => {
    const image = document.createElement("img");
    image.className = "game-icon";
    image.src = `/static/icons/game/${encodeURIComponent(name || "awareness")}.svg`;
    image.alt = "";
    image.setAttribute("aria-hidden", "true");
    image.title = label;
    return image;
  };

  const narrationRow = (content, className = "") => {
    const row = document.createElement("div");
    row.className = `chat-npc-message evidence-narration ${className}`.trim();
    row.dataset.chatChannel = "local";
    row.dataset.evidenceScripted = "true";
    const timestamp = document.createElement("span");
    timestamp.className = "chat-timestamp";
    timestamp.textContent = "[--:--] ";
    const narration = document.createElement("em");
    narration.append(content);
    row.append(timestamp, narration);
    return row;
  };

  const bestiaryColor = (supportBps) => {
    const bounded = Math.max(0, Math.min(10000, Number(supportBps) || 0));
    if (bounded <= 5000) {
      return `rgb(255 ${Math.round(255 * bounded / 5000)} 0)`;
    }
    return `rgb(${Math.round(255 * (10000 - bounded) / 5000)} 255 0)`;
  };

  const bestiaryTooltip = (result) => [
    ["Typical signs", result.tendency],
    ["Common strengths", result.strengths],
    ["Considerations", result.considerations],
    ["Exceptions", result.exceptions],
    ["Confirmed combat mechanics", result.confirmed_mechanics],
    ["Folklore / unimplemented hypotheses", result.folklore],
  ].map(([label, value]) => `${label}: ${value}`).join("\n");

  const bestiaryResultsRow = (results) => {
    const row = narrationRow(document.createTextNode(""), "bestiary-check-results");
    const narration = row.querySelector("em");
    narration.replaceChildren();
    const heading = document.createElement("strong");
    heading.className = "bestiary-check-heading";
    heading.textContent = "Bestiary check(s) succeeded:";
    narration.append(heading);
    [...new Set(results.map((result) => result.interpretation))].forEach((interpretation) => {
      const line = document.createElement("span");
      line.className = "bestiary-interpretation";
      line.textContent = interpretation;
      narration.append(line);
    });
    const chips = document.createElement("span");
    chips.className = "bestiary-result-list";
    results.forEach((result) => {
      const percent = result.support_bps / 100;
      const chip = document.createElement("span");
      chip.className = "bestiary-result-chip";
      chip.tabIndex = 0;
      chip.setAttribute("role", "note");
      chip.dataset.bestiaryCategory = result.category;
      chip.style.backgroundColor = bestiaryColor(result.support_bps);
      chip.textContent = `${result.label} — ${result.support_label} (${percent}%)`;
      const accessible = `${result.label} Bestiary result: ${percent}%, ${result.support_label}.`;
      chip.setAttribute("aria-label", accessible);
      chip.dataset.strategicTooltip = bestiaryTooltip(result);
      chips.append(chip);
    });
    narration.append(chips);
    return row;
  };

  const renderConversation = (item) => {
    messages?.querySelectorAll("[data-evidence-scripted]").forEach((node) => node.remove());
    if (!messages) return;
    const introduction = document.createDocumentFragment();
    introduction.append(document.createTextNode(`${item.description} You can inspect `));
    item.topics.forEach((topic, index) => {
      if (index > 0) {
        introduction.append(document.createTextNode(index === item.topics.length - 1 ? " or " : ", "));
      }
      const anchor = document.createElement("a");
      anchor.href = "#";
      anchor.className = "chat-quest-link evidence-topic";
      anchor.dataset.evidenceTopic = topic.id;
      anchor.dataset.evidenceId = item.id;
      anchor.textContent = topic.label;
      introduction.append(anchor);
    });
    introduction.append(document.createTextNode("."));
    messages.append(narrationRow(introduction));
    item.inspections.forEach((attempt) => {
      messages.append(narrationRow(
        document.createTextNode(attempt.narration),
        attempt.stat_label ? (attempt.passed ? "evidence-check-passed" : "evidence-check-failed") : "",
      ));
      if (attempt.bestiary_results?.length) {
        messages.append(bestiaryResultsRow(attempt.bestiary_results));
      }
    });
    messages.scrollTop = messages.scrollHeight;
  };

  const selectEvidence = (item, button) => {
    selectedId = item.id;
    strip.querySelectorAll("button").forEach((candidate) => {
      const active = candidate === button;
      candidate.classList.toggle("active", active);
      candidate.setAttribute("aria-pressed", String(active));
      candidate.tabIndex = active ? 0 : -1;
    });
    if (stage) {
      const placeholder = document.createElement("div");
      placeholder.className = "visual-stage-placeholder evidence-portrait-large";
      placeholder.setAttribute("aria-hidden", "true");
      placeholder.append(icon(item.portrait_icon, item.label));
      const heading = document.createElement("h2");
      heading.textContent = item.label;
      const description = document.createElement("p");
      const emphasized = document.createElement("em");
      emphasized.textContent = item.description;
      description.append(emphasized);
      stage.replaceChildren(placeholder, heading, description);
    }
    renderConversation(item);
  };

  const renderStrip = () => {
    if (!evidence.length) {
      strip.hidden = true;
      return;
    }
    const buttons = evidence.map((item) => {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "party-portrait settlement-npc-portrait physical-evidence-portrait";
      button.dataset.evidenceId = item.id;
      button.setAttribute("aria-label", `Inspect ${item.label}`);
      button.setAttribute("aria-pressed", "false");
      button.tabIndex = -1;
      const portrait = document.createElement("span");
      portrait.className = "party-portrait-initial settlement-npc-initials";
      const face = document.createElement("span");
      face.className = "party-portrait-face";
      face.append(icon(item.portrait_icon, item.label));
      const name = document.createElement("span");
      name.className = "party-portrait-name settlement-npc-name";
      name.textContent = item.label;
      portrait.append(face, name);
      button.append(portrait);
      button.addEventListener("click", () => selectEvidence(item, button));
      button.addEventListener("keydown", (event) => {
        if (!["ArrowLeft", "ArrowRight"].includes(event.key)) return;
        event.preventDefault();
        const offset = event.key === "ArrowRight" ? 1 : -1;
        buttons[(buttons.indexOf(button) + offset + buttons.length) % buttons.length].focus();
      });
      return button;
    });
    strip.replaceChildren(...buttons);
    const selectedIndex = Math.max(0, evidence.findIndex((item) => item.id === selectedId));
    selectEvidence(evidence[selectedIndex], buttons[selectedIndex]);
  };

  const load = async () => {
    evidence = await request(`/api/evidence/case-sites/${encodeURIComponent(caseSiteId)}`);
    renderStrip();
  };

  messages?.addEventListener("click", async (event) => {
    const topic = event.target.closest("[data-evidence-topic]");
    if (!topic) return;
    event.preventDefault();
    try {
      const updated = await request("/api/evidence/inspect", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          evidence_id: topic.dataset.evidenceId,
          topic_id: topic.dataset.evidenceTopic,
          action_id: actionId(),
          case_site_id: caseSiteId,
        }),
      });
      evidence = evidence.map((item) => item.id === updated.id ? updated : item);
      const button = strip.querySelector(`[data-evidence-id="${CSS.escape(updated.id)}"]`);
      selectEvidence(updated, button);
    } catch (error) {
      window.reportStrategicError(error, "inspect physical evidence");
    }
  });

  // Physical evidence has clickable inspection topics rather than free-form
  // speech. Keep the ordinary chat composer visibly unavailable.
  if (input) {
    input.disabled = true;
    input.placeholder = "Select a detail in the description to inspect it";
  }
  if (send) send.disabled = true;
  load().catch((error) => window.reportStrategicError(error, "load physical evidence"));
})();
