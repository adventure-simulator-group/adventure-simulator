(() => {
  let observer;
  const mount = () => {
    observer?.disconnect();
    const page = document.querySelector("#strategic-page");
    const nav = page?.querySelector("[data-settlement-id]");
    if (!nav) return;
    const current = new URL(location.href);
    const requested = current.searchParams.get("building");
    const tabs = [...nav.querySelectorAll("[data-building-id]")];
    const buildings = new Set(tabs.map((tab) => tab.dataset.buildingId).filter(Boolean));
    const serverActive = nav.querySelector(".nav-tab.active")?.dataset.buildingId;
    const partyInspection = current.pathname.startsWith("/locations/") && current.pathname.includes("/party");
    const building = partyInspection && buildings.has(requested)
      ? requested : (buildings.has(serverActive) ? serverActive : (buildings.has("map") ? "map" : tabs[0]?.dataset.buildingId));
    if (requested && (!buildings.has(requested) || !partyInspection)) {
      current.searchParams.delete("building");
      history.replaceState(history.state, "", current);
    }
    tabs.forEach((tab) => {
      const selected = tab.dataset.buildingId === building;
      tab.classList.toggle("active", selected);
      tab.setAttribute("aria-current", selected ? "page" : "false");
      if (selected) document.documentElement.style.setProperty("--active-building-tint", tab.style.getPropertyValue("--building-tint"));
    });
    const syncPartyLinks = (root = page) => root.querySelectorAll?.("a[href], form[action]").forEach((node) => {
      const attribute = node.matches("form") ? "action" : "href";
      const raw = node.getAttribute(attribute);
      if (!raw || !raw.startsWith("/locations/")) return;
      const url = new URL(raw, location.origin);
      if (!url.pathname.includes("/party")) return;
      if (building) url.searchParams.set("building", building);
      else url.searchParams.delete("building");
      node.setAttribute(attribute, `${url.pathname}${url.search}${url.hash}`);
    });
    syncPartyLinks();
    observer = new MutationObserver((mutations) => mutations.forEach((mutation) =>
      mutation.addedNodes.forEach((node) => node.nodeType === Node.ELEMENT_NODE && syncPartyLinks(node.matches("a[href], form[action]") ? node.parentElement : node)),
    ));
    observer.observe(page, { childList: true, subtree: true });
  };
  mount();
  document.addEventListener("strategic-page-mounted", mount);
  document.addEventListener("strategic-page-unmounting", () => observer?.disconnect());
})();
