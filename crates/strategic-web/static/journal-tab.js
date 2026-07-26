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
    const {
      journalLeft, journalCenter, journalRight,
      originalLeft, originalCenter, originalRight,
    } = state;
    state.controller?.abort();
    journalLeft.replaceWith(originalLeft);
    journalCenter.replaceWith(originalCenter);
    journalRight.replaceWith(originalRight);
    state.button.classList.remove("active");
    state.button.setAttribute("aria-pressed", "false");
    state.button.setAttribute("aria-label", "Open journal");
    if (restoreFocus) state.button.focus();
    state = null;
  };
  const journalLayoutFor = (tab) => {
    const root = tab.closest(".main-grid") || document;
    return {
      index: tab.closest("[data-journal-case-index]") || root.querySelector("[data-journal-case-index]"),
      log: root.querySelector("[data-journal-case-log]"),
    };
  };
  const selectCase = (caseId, layout) => {
    layout.index?.querySelectorAll("[data-journal-case-select]").forEach((tab) => {
      const selected = tab.dataset.journalCaseSelect === caseId;
      tab.classList.toggle("active", selected);
      tab.setAttribute("aria-selected", String(selected));
      tab.tabIndex = selected ? 0 : -1;
    });
    layout.log?.querySelectorAll("[data-journal-case-panel]").forEach((panel) => {
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
      const nextCenter = responseDocument.querySelector("[data-journal-case-log]");
      const nextRight = responseDocument.querySelector("[data-journal-context]");
      const originalLeft = grid.querySelector(":scope > .left-sidebar");
      const originalCenter = grid.querySelector(":scope > main.center-content");
      const originalRight = grid.querySelector(":scope > .right-sidebar");
      if (!nextLeft || !nextCenter || !nextRight || !originalLeft || !originalCenter || !originalRight) {
        throw new Error("The journal response did not contain a replaceable strategic layout.");
      }
      const journalLeft = document.importNode(nextLeft, true);
      const journalCenter = document.importNode(nextCenter, true);
      const journalRight = document.importNode(nextRight, true);
      Object.assign(state, {
        originalLeft, originalCenter, originalRight,
        journalLeft,
        journalCenter,
        journalRight,
        loading: false,
      });
      originalLeft.replaceWith(journalLeft);
      originalCenter.replaceWith(journalCenter);
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
    if (caseTab) {
      event.preventDefault();
      selectCase(caseTab.dataset.journalCaseSelect, journalLayoutFor(caseTab));
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
    const tab = event.target.closest?.("[data-journal-case-select]");
    if (!tab) return;
    const layout = journalLayoutFor(tab);
    const tabs = [...layout.index.querySelectorAll("[data-journal-case-select]")];
    const current = tabs.indexOf(tab);
    const direction = {
      ArrowLeft: -1,
      ArrowUp: -1,
      ArrowRight: 1,
      ArrowDown: 1,
    }[event.key];
    if (direction === undefined && event.key !== "Home" && event.key !== "End") return;
    event.preventDefault();
    const next = event.key === "Home"
      ? 0
      : event.key === "End"
        ? tabs.length - 1
        : (current + direction + tabs.length) % tabs.length;
    const nextTab = tabs[next];
    if (!nextTab) return;
    selectCase(nextTab.dataset.journalCaseSelect, layout);
    nextTab.focus();
  });
  document.addEventListener("strategic-page-unmounting", () => {
    generation += 1;
    close({ restoreFocus: false });
  });
})();
