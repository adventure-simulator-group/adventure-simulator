const wrappedDialogFocusIndex = (length, current, backwards) => {
  if (length <= 0) return -1;
  if (current < 0) return backwards ? length - 1 : 0;
  if (backwards && current === 0) return length - 1;
  if (!backwards && current === length - 1) return 0;
  return current + (backwards ? -1 : 1);
};

if (typeof module !== "undefined") module.exports = { wrappedDialogFocusIndex };

(() => {
  if (typeof document === "undefined") return;
  let lastFocused = null;
  const focusableSelector = [
    "a[href]", "button:not(:disabled)", "input:not(:disabled)",
    "select:not(:disabled)", "textarea:not(:disabled)", "[tabindex]:not([tabindex='-1'])",
  ].join(",");
  const visibleFocusables = (dialog) => [...dialog.querySelectorAll(focusableSelector)]
    .filter((element) => !element.hidden && !element.closest("[hidden]") && element.getAttribute("aria-hidden") !== "true");

  const focusDialog = (dialog) => {
    if (!dialog || dialog.hidden) return;
    lastFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    requestAnimationFrame(() => {
      const target = visibleFocusables(dialog)[0] || dialog;
      target.focus?.();
    });
  };

  const restoreFocus = () => {
    if (lastFocused?.isConnected) lastFocused.focus();
    lastFocused = null;
  };

  document.querySelectorAll(".party-offer[role='dialog']").forEach((dialog) => {
    new MutationObserver(() => {
      if (dialog.hidden) restoreFocus();
      else focusDialog(dialog);
    }).observe(dialog, { attributes: true, attributeFilter: ["hidden"] });
  });

  const examination = document.querySelector("[data-medical-examination]");
  const restSummary = document.querySelector(".rest-summary-overlay[role='dialog']");
  if (examination) {
    document.body.classList.add("character-action-dialog-open");
    focusDialog(examination);
  }
  else if (restSummary) focusDialog(restSummary);

  document.addEventListener("keydown", (event) => {
    const openConfirmation = [...document.querySelectorAll(".party-offer[role='dialog']")]
      .find((dialog) => !dialog.hidden);
    const activeDialog = openConfirmation || examination || restSummary;
    if (!activeDialog) return;
    if (event.key === "Tab") {
      const focusables = visibleFocusables(activeDialog);
      const current = focusables.indexOf(document.activeElement);
      const next = wrappedDialogFocusIndex(focusables.length, current, event.shiftKey);
      if (next >= 0 && (current < 0 || next !== current + (event.shiftKey ? -1 : 1))) {
        event.preventDefault();
        focusables[next].focus();
      }
      return;
    }
    if (event.key !== "Escape") return;
    const close = activeDialog.querySelector(
      ".party-offer-cancel, [data-cancel-loot], [data-cancel-pool], .medical-examination-close, .rest-summary-close",
    );
    if (!close) return;
    event.preventDefault();
    close.click();
  });

  if (!examination) return;
  let resolved = false;
  examination.querySelectorAll("form").forEach((form) => {
    form.addEventListener("submit", () => { resolved = true; });
  });
  window.addEventListener("pagehide", () => {
    if (!resolved && examination.dataset.dismissUrl) {
      navigator.sendBeacon(examination.dataset.dismissUrl, new Blob([], {
        type: "application/x-www-form-urlencoded",
      }));
    }
  }, { once: true });
})();
