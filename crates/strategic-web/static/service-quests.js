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
  const chat = document.querySelector("[data-service-quest-id]");
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
      document.querySelector("[data-service-recruitment]")?.remove();
      const recruiting = quests.find((quest) => quest.service_id === chat?.dataset.serviceQuestId && quest.state === "recruiting" && quest.recruitment);
      const right = document.querySelector(".right-sidebar");
      if (recruiting && right) {
        const section = document.createElement("section");
        section.className = "sidebar-section";
        section.dataset.serviceRecruitment = "true";
        const heading = document.createElement("h3");
        heading.className = "sidebar-header";
        heading.textContent = "Recruitment";
        const leader = document.createElement("a");
        leader.href = `/locations/settlement/${encodeURIComponent(settlementId)}/players/${encodeURIComponent(recruiting.recruitment.leader_id)}`;
        leader.className = "chat-quest-link";
        leader.textContent = recruiting.recruitment.leader_name;
        leader.title = `Leader of ${recruiting.recruitment.party_name}`;
        const summary = document.createElement("p");
        summary.append(leader, document.createTextNode(" is seeking help."));
        const roles = document.createElement("ul");
        recruiting.recruitment.roles.forEach((role) => {
          const item = document.createElement("li");
          const button = document.createElement("button");
          button.type = "button";
          button.className = `service-role-link service-role-link-${role.match_level}`;
          button.textContent = role.name;
          button.title = role.requirements_summary;
          button.addEventListener("click", () => {
            document.querySelectorAll("[data-service-role-inspection]").forEach((node) => node.remove());
            const left = document.querySelector(".left-sidebar");
            const currentRight = document.querySelector(".right-sidebar");
            if (!left || !currentRight) return;
            const leftTemplate = document.createElement("template");
            const rightTemplate = document.createElement("template");
            leftTemplate.innerHTML = role.left_html;
            rightTemplate.innerHTML = role.right_html;
            Array.from(leftTemplate.content.children).forEach((node) => { node.dataset.serviceRoleInspection = "true"; });
            Array.from(rightTemplate.content.children).forEach((node) => { node.dataset.serviceRoleInspection = "true"; });
            left.append(leftTemplate.content);
            currentRight.append(rightTemplate.content);
          });
          item.append(button);
          roles.append(item);
        });
        section.append(heading, summary, roles);
        right.prepend(section);
      }
    }).catch((error) => window.reportStrategicError(error, "service quests"));
  window.queueStrategicInitialLoad(refreshServiceQuests);
  window.queueStrategicInitialLoad(refreshMapQuestMarker);
  document.addEventListener("strategic-live-update", refreshServiceQuests);
  document.addEventListener("strategic-live-update", refreshMapQuestMarker);
})();
