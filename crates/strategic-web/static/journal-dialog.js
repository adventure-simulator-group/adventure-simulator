(() => {
  const dialog = document.querySelector("[data-journal-dialog]");
  const content = dialog?.querySelector("[data-journal-dialog-content]");
  const openers = [...document.querySelectorAll("[data-journal-open]")];
  if (!dialog || !content || !openers.length) return;

  let activeOpener = null;
  let loaded = false;

  const loadJournal = async () => {
    content.setAttribute("aria-busy", "true");
    try {
      const response = await window.strategicFetch("/quests", {
        headers: { Accept: "text/html" },
      });
      if (!response.ok) throw new Error(`Could not open journal (${response.status})`);
      const documentCopy = new DOMParser().parseFromString(await response.text(), "text/html");
      const journal = documentCopy.querySelector("[data-investigation-journal]");
      if (!journal) throw new Error("The journal response did not contain journal entries.");
      journal.querySelector(":scope > header")?.remove();
      content.replaceChildren(...journal.childNodes);
      loaded = true;
    } catch (error) {
      content.textContent = "The journal could not be opened.";
      window.reportStrategicError?.(error, "open journal");
    } finally {
      content.removeAttribute("aria-busy");
    }
  };

  openers.forEach((opener) => opener.addEventListener("click", (event) => {
    event.preventDefault();
    activeOpener = opener;
    opener.setAttribute("aria-expanded", "true");
    if (!dialog.open) dialog.showModal();
    dialog.focus();
    if (!loaded) loadJournal();
  }));

  dialog.addEventListener("click", (event) => {
    if (event.target === dialog) dialog.close();
  });

  dialog.addEventListener("close", () => {
    openers.forEach((opener) => opener.setAttribute("aria-expanded", "false"));
    activeOpener?.focus();
    activeOpener = null;
  });
})();
