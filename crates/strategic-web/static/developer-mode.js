(() => {
  "use strict";
  const STORAGE_KEY = "adventuresim.developer-mode";
  const read = () => {
    try { return localStorage.getItem(STORAGE_KEY) === "on"; } catch (_) { return false; }
  };
  const write = (enabled) => {
    try { localStorage.setItem(STORAGE_KEY, enabled ? "on" : "off"); } catch (_) { /* unavailable storage remains session-only */ }
  };
  const apply = (enabled) => {
    document.documentElement.toggleAttribute("data-developer-mode", enabled);
    document.querySelectorAll("[data-developer-mode-toggle]").forEach((button) => {
      button.setAttribute("aria-pressed", String(enabled));
      button.setAttribute("aria-label", enabled ? "Disable developer mode" : "Enable developer mode");
    });
    document.querySelectorAll(".dialogue-source-link").forEach((link) => { link.hidden = !enabled; });
    document.querySelectorAll("[data-developer-mode-input]").forEach((input) => { input.disabled = !enabled; });
  };
  let enabled = read();
  document.addEventListener("click", (event) => {
    if (!event.target.closest("[data-developer-mode-toggle]")) return;
    enabled = !enabled; write(enabled); apply(enabled);
  });
  apply(enabled);
  new MutationObserver((records) => {
    for (const record of records) {
      for (const node of record.addedNodes) {
        if (!(node instanceof Element)) continue;
        if (node.matches("[data-developer-mode-input]")) node.disabled = !enabled;
        node.querySelectorAll("[data-developer-mode-input]").forEach((input) => { input.disabled = !enabled; });
        if (node.matches(".dialogue-source-link")) node.hidden = !enabled;
        node.querySelectorAll(".dialogue-source-link").forEach((link) => { link.hidden = !enabled; });
      }
    }
  }).observe(document.documentElement, { childList: true, subtree: true });
  if (typeof module !== "undefined") module.exports = { STORAGE_KEY };
})();
