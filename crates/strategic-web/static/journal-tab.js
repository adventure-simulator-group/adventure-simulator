(() => {
  let state = null;
  let generation = 0;

  const close = ({ restoreFocus = true } = {}) => {
    if (!state) return;
    if (state.loading) {
      state.controller?.abort();
      state = null;
      return;
    }
    const { journalLeft, journalRight, originalLeft, originalRight } = state;
    state.controller?.abort();
    journalLeft.replaceWith(originalLeft);
    journalRight.replaceWith(originalRight);
    state.button.classList.remove("active");
    state.button.setAttribute("aria-pressed", "false");
    state.button.setAttribute("aria-label", "Open journal");
    if (restoreFocus) state.button.focus();
    state = null;
  };
  const selectCase = (caseId) => {
    state?.journalLeft.querySelectorAll("[data-journal-case-select]").forEach((tab) => {
      const selected = tab.dataset.journalCaseSelect === caseId;
      tab.classList.toggle("active", selected);
      tab.setAttribute("aria-selected", String(selected));
    });
    state?.journalRight.querySelectorAll("[data-journal-case-panel]").forEach((panel) => {
      panel.hidden = panel.dataset.journalCasePanel !== caseId;
    });
  };
  const open = async (button) => {
    if (state?.loading) return;
    const mine = generation;
    const grid = button.closest("#strategic-page")?.querySelector(".main-grid");
    if (!grid) return;
    const controller = new AbortController();
    state = { button, grid, loading: true, controller };
    button.setAttribute("aria-busy", "true");
    try {
      const response = await window.strategicFetch("/quests", {
        headers: { Accept: "text/html" },
        signal: controller.signal,
      });
      if (mine !== generation) return;
      const responseDocument = new DOMParser().parseFromString(await response.text(), "text/html");
      const nextLeft = responseDocument.querySelector("[data-journal-case-index]");
      const nextRight = responseDocument.querySelector("[data-journal-case-log]");
      const originalLeft = grid.querySelector(":scope > .left-sidebar");
      const originalRight = grid.querySelector(":scope > .right-sidebar");
      if (!nextLeft || !nextRight || !originalLeft || !originalRight) {
        throw new Error("The journal response did not contain replaceable rails.");
      }
      const journalLeft = document.importNode(nextLeft, true);
      const journalRight = document.importNode(nextRight, true);
      Object.assign(state, {
        originalLeft, originalRight,
        journalLeft,
        journalRight,
        loading: false,
      });
      originalLeft.replaceWith(journalLeft);
      originalRight.replaceWith(journalRight);
      button.classList.add("active");
      button.setAttribute("aria-pressed", "true");
      button.setAttribute("aria-label", "Close journal");
      state.journalLeft.querySelector("[data-journal-case-select]")?.focus();
    } catch (error) {
      if (mine === generation) state = null;
      window.reportStrategicError?.(error, "open journal");
    } finally {
      button.removeAttribute("aria-busy");
    }
  };

  document.addEventListener("click", (event) => {
    const currentButton = document.querySelector("[data-journal-tab]");
    const caseTab = event.target.closest("[data-journal-case-select]");
    if (caseTab && state) {
      event.preventDefault();
      selectCase(caseTab.dataset.journalCaseSelect);
      return;
    }
    const button = event.target.closest("[data-journal-tab]");
    if (button !== currentButton) return;
    if (!button) return;
    event.preventDefault();
    if (state) close();
    else open(button);
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && state) close();
  });
  document.addEventListener("strategic-page-unmounting", () => {
    generation += 1;
    close({ restoreFocus: false });
  });
})();
