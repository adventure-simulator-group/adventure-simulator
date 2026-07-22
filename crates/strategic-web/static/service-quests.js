const serviceQuestTabState = (serviceQuests) => {
  if (serviceQuests.some((quest) => quest.state === "ready")) return "quest ready to report";
  if (serviceQuests.some((quest) => quest.state === "available")) return "quest available";
  if (serviceQuests.some((quest) => quest.state === "recruiting")) return "recruitment available";
  return null;
};

if (typeof module !== "undefined") module.exports = { serviceQuestTabState };

(() => {
  if (typeof document === "undefined") return;
  const services = document.querySelector("[data-settlement-id]");
  if (!services) return;
  const settlementId = services.dataset.settlementId;
  const mapTab = services.querySelector('[data-service-id="map"]');
  const mapQuestBadge = services.querySelector("[data-map-quest-badge]");
  const setMapQuestActive = (active, description = "") => {
    if (mapQuestBadge) mapQuestBadge.hidden = !active;
    if (mapTab) {
      mapTab.setAttribute("aria-label", active ? "Map, active quest" : "Map");
      mapTab.setAttribute("title", active && description ? description : "Map");
    }
  };
  const refreshMapQuestMarker = () => window.strategicBackgroundFetch(
    "active-quest-marker", "/api/active-quest-marker", { headers: { Accept: "application/json" } },
  ).then((response) => (response.ok ? response.json() : { active: false }))
    .then((marker) => setMapQuestActive(marker.active === true, marker.description || ""))
    .catch((error) => window.reportStrategicError(error, "active quest marker"));
  const refreshServiceQuests = () => window.strategicBackgroundFetch(
    "service-quests", `/api/settlements/${encodeURIComponent(settlementId)}/service-quests`,
    { headers: { Accept: "application/json" } },
  ).then((response) => (response.ok ? response.json() : []))
    .then((quests) => {
      services.querySelectorAll("[data-service-quest-badge]").forEach((badge) => {
        const tab = badge.closest("[data-service-id]");
        const serviceQuests = quests.filter((quest) => quest.service_id === tab?.dataset.serviceId);
        const state = serviceQuestTabState(serviceQuests);
        const baseLabel = tab?.dataset.serviceLabel || tab?.title || "Service";
        badge.hidden = state === null;
        badge.classList.toggle("service-turn-in-badge", serviceQuests.some((quest) => quest.state === "ready"));
        badge.classList.toggle("service-available-quest-badge", serviceQuests.some((quest) => quest.state === "available"));
        badge.classList.toggle("service-recruitment-badge", serviceQuests.some((quest) => quest.state === "recruiting"));
        tab?.setAttribute("aria-label", state ? `${baseLabel}, ${state}` : baseLabel);
      });
    }).catch((error) => window.reportStrategicError(error, "service quests"));
  window.queueStrategicInitialLoad(refreshServiceQuests);
  window.queueStrategicInitialLoad(refreshMapQuestMarker);
  document.addEventListener("strategic-live-update", refreshServiceQuests);
  document.addEventListener("strategic-live-update", refreshMapQuestMarker);
})();
