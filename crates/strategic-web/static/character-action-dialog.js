(() => {
  const FOCUSABLE = [
    "a[href]", "button:not(:disabled)", "input:not([type='hidden']):not(:disabled)",
    "select:not(:disabled)", "textarea:not(:disabled)", "[tabindex]:not([tabindex='-1'])",
  ].join(",");

  const visible = (root) => [...root.querySelectorAll(FOCUSABLE)]
    .filter((element) => !element.hidden && !element.closest("[hidden]") && element.getAttribute("aria-hidden") !== "true");

  const wrappedFocusIndex = (length, current, backwards) => {
    if (length <= 0) return -1;
    if (current < 0) return backwards ? length - 1 : 0;
    if (backwards && current === 0) return length - 1;
    if (!backwards && current === length - 1) return 0;
    return current + (backwards ? -1 : 1);
  };

  if (typeof module !== "undefined") module.exports = { wrappedFocusIndex };
  if (typeof document === "undefined") return;

  const restoreKey = "adventuresim-character-dialog-opener";
  document.addEventListener("click", (event) => {
    const opener = event.target.closest?.("[aria-haspopup='dialog']");
    if (opener?.getAttribute("aria-label")) {
      sessionStorage.setItem(restoreKey, opener.getAttribute("aria-label"));
    }
    if (event.target.closest?.(".character-action-dialog-close, .character-action-backdrop")) {
      sessionStorage.setItem(`${restoreKey}-pending`, "true");
    }
  });

  const overlays = [...document.querySelectorAll("[data-character-action-dialog]")];
  const overlay = overlays[0];
  overlays.slice(1).forEach((extra) => { extra.hidden = true; });
  if (!overlay) {
    document.body.classList.remove("character-action-dialog-open");
    if (sessionStorage.getItem(`${restoreKey}-pending`) === "true") {
      sessionStorage.removeItem(`${restoreKey}-pending`);
      const label = sessionStorage.getItem(restoreKey);
      if (label) {
        requestAnimationFrame(() => [...document.querySelectorAll("[aria-haspopup='dialog']")]
          .find((element) => element.getAttribute("aria-label") === label)?.focus());
      }
    }
    return;
  }

  document.body.classList.add("character-action-dialog-open");
  const dialog = overlay.querySelector("[role='dialog']");
  const close = overlay.querySelector(".character-action-dialog-close");
  requestAnimationFrame(() => {
    const preferred = overlay.dataset.initialFocus && dialog.querySelector(overlay.dataset.initialFocus);
    (preferred || visible(dialog)[0] || dialog).focus?.();
  });

  overlay.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && close) {
      event.preventDefault();
      sessionStorage.setItem(`${restoreKey}-pending`, "true");
      close.click();
      return;
    }
    if (event.key !== "Tab") return;
    const focusables = visible(dialog);
    const current = focusables.indexOf(document.activeElement);
    const next = wrappedFocusIndex(focusables.length, current, event.shiftKey);
    if (next >= 0 && (current < 0 || next !== current + (event.shiftKey ? -1 : 1))) {
      event.preventDefault();
      focusables[next].focus();
    }
  });
})();
