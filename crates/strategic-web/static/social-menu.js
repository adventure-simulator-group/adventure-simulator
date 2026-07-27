(() => {
  const formatDuration = (minutes) => {
    const hours = Math.floor(minutes / 60);
    const remainder = minutes % 60;
    if (!hours) return `${remainder} minutes`;
    if (!remainder) return `${hours} ${hours === 1 ? "hour" : "hours"}`;
    return `${hours} ${hours === 1 ? "hour" : "hours"} ${remainder} minutes`;
  };
  const actionId = () => globalThis.crypto?.randomUUID?.()
    || `chat-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  const relationshipLabel = (value) => String(value || "uncertain").replaceAll("_", " ");
  const FOCUSABLE = [
    "a[href]", "button:not(:disabled)", "input:not([type='hidden']):not(:disabled)",
    "select:not(:disabled)", "textarea:not(:disabled)", "[tabindex]:not([tabindex='-1'])",
  ].join(",");
  const visible = (root) => [...root.querySelectorAll(FOCUSABLE)]
    .filter((element) => !element.hidden && !element.closest("[hidden]")
      && element.getAttribute("aria-hidden") !== "true");
  if (typeof module !== "undefined") {
    module.exports = { formatDuration, relationshipLabel };
  }
  if (typeof window === "undefined" || typeof document === "undefined") return;
  let activeOverlay = null;
  let openController = null;

  const bindDuration = (form) => {
    if (form.dataset.socialChatBound === "true") return;
    form.dataset.socialChatBound = "true";
    const slider = form.querySelector("[data-social-chat-duration]");
    const output = form.querySelector("[data-social-chat-output]");
    const submit = form.querySelector("[data-social-chat-submit]");
    if (!slider || !output || !submit) return;
    const render = () => {
      const minutes = Number(slider.value);
      const duration = formatDuration(minutes);
      output.textContent = duration;
      slider.setAttribute("aria-valuetext", duration);
      submit.textContent = `Chat for ${duration}`;
    };
    slider.addEventListener("input", render);
    render();
  };

  const closeActiveOverlay = (restoreFocus = true) => {
    if (!activeOverlay) {
      document.body.classList.remove("activity-modal-open");
      return;
    }
    const { overlay, opener, controller } = activeOverlay;
    activeOverlay = null;
    controller.abort();
    overlay.remove();
    document.body.classList.remove("activity-modal-open");
    if (restoreFocus && opener?.isConnected) opener.focus();
  };

  const socialPath = (npcId) => {
    const strip = document.querySelector("[data-npc-strip]");
    if (!strip) return null;
    return `/api/settlements/${encodeURIComponent(strip.dataset.npcSettlement)}/locations/${encodeURIComponent(strip.dataset.npcLocation)}/npcs/${encodeURIComponent(npcId)}/social`;
  };

  const renderNpcSocial = (view, path, opener) => {
    closeActiveOverlay(false);
    const controller = new AbortController();
    const { signal } = controller;
    const overlay = document.createElement("div");
    overlay.className = "character-action-overlay";
    overlay.dataset.npcSocialOverlay = "true";
    const backdrop = document.createElement("button");
    backdrop.type = "button";
    backdrop.className = "character-action-backdrop";
    backdrop.setAttribute("aria-label", "Close social dialog");
    const dialog = document.createElement("section");
    dialog.className = "character-action-dialog social-dialog";
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    dialog.setAttribute("aria-labelledby", "npc-social-dialog-title");
    dialog.tabIndex = -1;
    const header = document.createElement("header");
    header.className = "character-action-dialog-header";
    const title = document.createElement("h2");
    title.id = "npc-social-dialog-title";
    title.textContent = `Social — ${view.name}`;
    const close = document.createElement("button");
    close.type = "button";
    close.className = "character-action-dialog-close";
    close.setAttribute("aria-label", "Close social dialog");
    close.textContent = "×";
    header.append(title, close);

    const rail = document.createElement("div");
    rail.className = "social-rail";
    const relationship = document.createElement("dl");
    relationship.className = "social-biography";
    [
      ["Affinity", relationshipLabel(view.affinity)],
      ["Familiarity", relationshipLabel(view.familiarity)],
      ["Demeanor", relationshipLabel(view.morale)],
    ].forEach(([label, value]) => {
      const row = document.createElement("div");
      const term = document.createElement("dt");
      const detail = document.createElement("dd");
      term.textContent = label;
      detail.textContent = value;
      row.append(term, detail);
      relationship.append(row);
    });
    const section = document.createElement("section");
    section.className = "sidebar-section";
    const heading = document.createElement("h3");
    heading.textContent = "Spend time together";
    const form = document.createElement("form");
    form.className = "social-chat-activity";
    form.dataset.socialChatForm = "true";
    const label = document.createElement("label");
    label.textContent = "Chat";
    const help = document.createElement("span");
    help.className = "text-muted small-copy";
    help.textContent = "An ordinary conversation can strengthen or strain the relationship.";
    label.append(help);
    const controls = document.createElement("div");
    controls.className = "social-chat-duration";
    const slider = document.createElement("input");
    slider.type = "range";
    slider.name = "requested_minutes";
    slider.min = "15";
    slider.max = "480";
    slider.step = "15";
    slider.value = "30";
    slider.dataset.socialChatDuration = "true";
    slider.setAttribute("aria-label", `Time spent chatting with ${view.name}`);
    const output = document.createElement("output");
    output.dataset.socialChatOutput = "true";
    const submit = document.createElement("button");
    submit.type = "submit";
    submit.className = "btn btn-primary btn-small";
    submit.dataset.socialChatSubmit = "true";
    controls.append(slider, output);
    form.append(label, controls, submit);
    const feedback = document.createElement("p");
    feedback.className = "social-feedback";
    feedback.setAttribute("role", "status");
    if (view.last_outcome) {
      feedback.textContent = ({
        positive: "The conversation brings you closer.",
        mixed: "The conversation has warm moments and awkward ones.",
        negative: "The conversation leaves some friction between you.",
      })[view.last_outcome] || "You spend some time talking.";
    }
    section.append(heading, form, feedback);
    rail.append(relationship, section);
    dialog.append(header, rail);
    overlay.append(backdrop, dialog);
    document.body.append(overlay);
    document.body.classList.add("activity-modal-open");
    activeOverlay = { overlay, opener, controller };
    bindDuration(form);
    const submissionActionId = actionId();
    const dismiss = () => closeActiveOverlay(true);
    backdrop.addEventListener("click", dismiss, { signal });
    close.addEventListener("click", dismiss, { signal });
    overlay.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        dismiss();
        return;
      }
      if (event.key !== "Tab") return;
      const focusables = visible(dialog);
      if (!focusables.length) {
        event.preventDefault();
        dialog.focus();
        return;
      }
      const current = focusables.indexOf(document.activeElement);
      if (event.shiftKey && current <= 0) {
        event.preventDefault();
        focusables[focusables.length - 1].focus();
      } else if (!event.shiftKey && (current < 0 || current === focusables.length - 1)) {
        event.preventDefault();
        focusables[0].focus();
      }
    }, { signal });
    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      submit.disabled = true;
      try {
        const response = await window.strategicFetch(path, {
          method: "POST",
          headers: { "Content-Type": "application/json", Accept: "application/json" },
          body: JSON.stringify({
            requested_minutes: Number(slider.value),
            action_id: submissionActionId,
          }),
          signal,
        });
        if (!response.ok) throw new Error(`Conversation failed (${response.status})`);
        if (signal.aborted) return;
        const updatedView = await response.json();
        if (signal.aborted) return;
        renderNpcSocial(updatedView, path, opener);
      } catch (error) {
        if (signal.aborted || error?.name === "AbortError") return;
        feedback.className = "social-feedback social-feedback-error";
        feedback.textContent = "The conversation could not be completed right now.";
        window.reportStrategicError(error, "chat with local resident");
        submit.disabled = false;
      }
    }, { signal });
    dialog.focus();
  };

  const openNpcSocial = async (opener) => {
    const path = socialPath(opener.dataset.openNpcSocial);
    if (!path) return;
    openController?.abort();
    const controller = new AbortController();
    openController = controller;
    opener.disabled = true;
    try {
      const response = await window.strategicFetch(path, {
        headers: { Accept: "application/json" },
        signal: controller.signal,
      });
      if (!response.ok) throw new Error(`Social menu unavailable (${response.status})`);
      if (controller.signal.aborted) return;
      const view = await response.json();
      if (controller.signal.aborted) return;
      renderNpcSocial(view, path, opener);
    } catch (error) {
      if (controller.signal.aborted || error?.name === "AbortError") return;
      window.reportStrategicError(error, "open social menu");
    } finally {
      if (openController === controller) openController = null;
      if (opener.isConnected) opener.disabled = false;
    }
  };

  const mount = () => {
    document.querySelectorAll("[data-social-chat-form]").forEach(bindDuration);
  };
  document.addEventListener("click", (event) => {
    const opener = event.target.closest?.("[data-open-npc-social]");
    if (!opener) return;
    event.preventDefault();
    openNpcSocial(opener);
  });
  mount();
  document.addEventListener("strategic-page-unmounting", () => {
    openController?.abort();
    openController = null;
    closeActiveOverlay(false);
    document.body.classList.remove("activity-modal-open");
  });
  document.addEventListener("strategic-page-mounted", mount);
})();
