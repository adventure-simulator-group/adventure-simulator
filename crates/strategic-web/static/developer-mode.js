(() => {
  "use strict";
  const STORAGE_KEY = "adventuresim.developer-mode";
  const read = () => {
    try { return localStorage.getItem(STORAGE_KEY) === "on"; } catch (_) { return false; }
  };
  const write = (enabled) => {
    try { localStorage.setItem(STORAGE_KEY, enabled ? "on" : "off"); } catch (_) { /* unavailable storage remains session-only */ }
  };
  const sourceLink = (url) => {
    const link = document.createElement("a");
    link.className = "dialogue-source-link";
    link.href = url;
    link.target = "_blank";
    link.rel = "noopener noreferrer";
    link.textContent = "Edit";
    link.setAttribute("aria-label", "Edit dialogue source on GitHub");
    return link;
  };
  const decorateDialogue = () => {
    document.querySelectorAll("[data-dialogue-source-url]").forEach((container) => {
      const url = container.dataset.dialogueSourceUrl;
      if (!url || !url.startsWith("https://github.com/")) return;
      container.querySelectorAll(".chat-npc-message:not([data-dialogue-source-decorated])").forEach((line) => {
        line.dataset.dialogueSourceDecorated = "true";
        line.append(sourceLink(url));
      });
    });
  };
  const apply = (enabled) => {
    document.documentElement.toggleAttribute("data-developer-mode", enabled);
    document.querySelectorAll("[data-developer-mode-toggle]").forEach((button) => {
      button.setAttribute("aria-pressed", String(enabled));
      button.setAttribute("aria-label", enabled ? "Disable developer mode" : "Enable developer mode");
    });
    document.querySelectorAll(".dialogue-source-link").forEach((link) => { link.hidden = !enabled; });
    if (enabled) decorateDialogue();
  };
  let enabled = read();
  document.addEventListener("click", (event) => {
    if (!event.target.closest("[data-developer-mode-toggle]")) return;
    enabled = !enabled; write(enabled); apply(enabled);
  });
  new MutationObserver(() => apply(enabled)).observe(document.documentElement, { childList: true, subtree: true });
  apply(enabled);
  if (typeof module !== "undefined") module.exports = { STORAGE_KEY };
})();
