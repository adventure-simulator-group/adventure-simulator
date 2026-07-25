(() => {
  const button = document.querySelector("[data-journal-tab]");
  const grid = document.querySelector(".main-grid");
  if (!button || !grid) return;

  let originalLeft = null;
  let originalRight = null;
  let journalLeft = null;
  let journalRight = null;
  let loading = false;

  const selectCase = (caseId) => {
    journalLeft?.querySelectorAll("[data-journal-case-select]").forEach((tab) => {
      const selected = tab.dataset.journalCaseSelect === caseId;
      tab.classList.toggle("active", selected);
      tab.setAttribute("aria-selected", String(selected));
    });
    journalRight?.querySelectorAll("[data-journal-case-panel]").forEach((panel) => {
      panel.hidden = panel.dataset.journalCasePanel !== caseId;
    });
  };

  const wireCaseTabs = () => {
    journalLeft?.querySelectorAll("[data-journal-case-select]").forEach((tab) => {
      tab.addEventListener("click", () => selectCase(tab.dataset.journalCaseSelect));
    });
  };

  const closeJournal = ({ restoreFocus = true } = {}) => {
    if (!originalLeft || !originalRight || !journalLeft || !journalRight) return;
    journalLeft.replaceWith(originalLeft);
    journalRight.replaceWith(originalRight);
    journalLeft = null;
    journalRight = null;
    button.classList.remove("active");
    button.setAttribute("aria-pressed", "false");
    button.setAttribute("aria-label", "Open journal");
    if (restoreFocus) button.focus();
  };

  const openJournal = async () => {
    if (loading) return;
    loading = true;
    button.setAttribute("aria-busy", "true");
    try {
      const response = await window.strategicFetch("/quests", {
        headers: { Accept: "text/html" },
      });
      if (!response.ok) throw new Error(`Could not open journal (${response.status})`);
      const responseDocument = new DOMParser().parseFromString(await response.text(), "text/html");
      const nextLeft = responseDocument.querySelector("[data-journal-case-index]");
      const nextRight = responseDocument.querySelector("[data-journal-case-log]");
      if (!nextLeft || !nextRight) {
        throw new Error("The journal response did not contain both journal rails.");
      }

      originalLeft = grid.querySelector(":scope > .left-sidebar");
      originalRight = grid.querySelector(":scope > .right-sidebar");
      if (!originalLeft || !originalRight) {
        throw new Error("The current location does not contain replaceable side rails.");
      }

      journalLeft = document.importNode(nextLeft, true);
      journalRight = document.importNode(nextRight, true);
      originalLeft.replaceWith(journalLeft);
      originalRight.replaceWith(journalRight);
      wireCaseTabs();
      button.classList.add("active");
      button.setAttribute("aria-pressed", "true");
      button.setAttribute("aria-label", "Close journal");
      journalLeft.querySelector("[data-journal-case-select]")?.focus();
    } catch (error) {
      window.reportStrategicError?.(error, "open journal");
    } finally {
      loading = false;
      button.removeAttribute("aria-busy");
    }
  };

  button.addEventListener("click", (event) => {
    event.preventDefault();
    if (journalLeft) closeJournal();
    else openJournal();
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && journalLeft) closeJournal();
  });
})();
