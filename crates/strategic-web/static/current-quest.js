(() => {
  const summary = document.querySelector("[data-current-quest]");
  if (!summary) return;

  const refreshCurrentQuest = () => window.strategicBackgroundFetch("current-quest", "/api/current-quest", {
    headers: { Accept: "application/json" },
  })
    .then((response) => (response.ok ? response.json() : null))
    .then((quest) => {
      if (!quest) {
        summary.hidden = true;
        return;
      }
      const name = summary.querySelector("[data-current-quest-name]");
      const status = summary.querySelector("[data-current-quest-status]");
      const abandon = summary.querySelector("[data-current-quest-abandon]");
      if (!name || !status || !abandon) return;
      name.textContent = quest.title;
      status.classList.toggle("resolved", quest.resolved);
      status.title = quest.resolved ? "Quest resolved" : "Quest in progress";
      status.setAttribute("aria-label", status.title);
      abandon.action = `/quests/${encodeURIComponent(quest.id)}/abandon`;
      abandon.hidden = !quest.can_abandon;
      summary.hidden = false;
    })
    .catch((error) => window.reportStrategicError(error, "current quest"));
  window.queueStrategicInitialLoad(refreshCurrentQuest);
  document.addEventListener("strategic-live-update", refreshCurrentQuest);
})();
