const serviceQuestTabState = (activity) => {
  if (activity.recruitment?.length) return "recruitment available";
  if (activity.quests?.some((quest) => quest.state === "ready")) return "quest ready to report";
  if (activity.quests?.some((quest) => quest.state === "available")) return "quest available";
  return null;
};

if (typeof module !== "undefined") module.exports = { serviceQuestTabState };

(() => {
  if (typeof document === "undefined") return;
  const refreshServiceQuests = () => {
    const services = document.querySelector("#strategic-page [data-settlement-id]");
    if (!services) return Promise.resolve();
    const settlementId = services.dataset.settlementId;
    const chat = document.querySelector("#strategic-page [data-service-quest-id]");
    return window.strategicBackgroundFetch(
    "service-quests", `/api/settlements/${encodeURIComponent(settlementId)}/service-quests`,
    { headers: { Accept: "application/json" } },
  ).then((response) => (response.ok ? response.json() : { quests: [], recruitment: [] }))
    .then((activity) => {
      document.querySelector("[data-service-recruitment]")?.remove();
      const recruiting = activity.recruitment.find((company) => company.service_id === chat?.dataset.serviceQuestId);
      const right = document.querySelector(".right-sidebar");
      if (recruiting && right) {
        const section = document.createElement("section");
        section.className = "sidebar-section";
        section.dataset.serviceRecruitment = "true";
        const heading = document.createElement("h3");
        heading.className = "sidebar-header";
        heading.textContent = "Recruitment";
        const leader = document.createElement("a");
        leader.href = `/locations/settlement/${encodeURIComponent(settlementId)}/players/${encodeURIComponent(recruiting.leader_id)}`;
        leader.className = "chat-quest-link";
        leader.textContent = recruiting.leader_name;
        leader.title = `Leader of ${recruiting.party_name}`;
        const summary = document.createElement("p");
        summary.append(leader, document.createTextNode(" is seeking help."));
        const roles = document.createElement("ul");
        recruiting.roles.forEach((role) => {
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
  };
  window.queueStrategicInitialLoad(refreshServiceQuests);
  document.addEventListener("strategic-live-update", refreshServiceQuests);
  document.addEventListener("strategic-page-mounted", refreshServiceQuests);
})();
