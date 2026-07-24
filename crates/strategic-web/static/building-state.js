(() => {
  const services = new Set(["", "residences", "keep", "map", "merchants", "weapons", "armor", "clothing", "herbalist", "inn", "religion"]);
  const nav = document.querySelector("[data-settlement-id]");
  if (!nav) return;
  const current = new URL(window.location.href);
  const requested = current.searchParams.get("building");
  const serverActive = nav.querySelector(".nav-tab.active")?.dataset.serviceId;
  const partyInspection = current.pathname.startsWith("/locations/") && current.pathname.includes("/party");
  const building = partyInspection && services.has(requested)
    ? requested
    : (services.has(serverActive) ? serverActive : "map");
  if (requested && (!services.has(requested) || !partyInspection)) {
    current.searchParams.delete("building");
    history.replaceState(null, "", current);
  }
  nav.querySelectorAll("[data-service-id]").forEach((tab) => {
    const selected = tab.dataset.serviceId === building;
    tab.classList.toggle("active", selected);
    tab.setAttribute("aria-current", selected ? "page" : "false");
    if (selected) document.documentElement.style.setProperty("--active-building-tint", tab.style.getPropertyValue("--building-tint"));
  });
  const syncPartyLinks = (root = document) => {
    root.querySelectorAll("a[href], form[action]").forEach((node) => {
      const attribute = node.matches("form") ? "action" : "href";
      const raw = node.getAttribute(attribute);
      if (!raw || !raw.startsWith("/locations/")) return;
      const url = new URL(raw, window.location.origin);
      if (!url.pathname.includes("/party")) return;
      if (building) url.searchParams.set("building", building);
      else url.searchParams.delete("building");
      node.setAttribute(attribute, `${url.pathname}${url.search}${url.hash}`);
    });
  };
  syncPartyLinks();

  // Live regions replace party controls after this deferred script runs. Keep
  // the active building on links introduced by those server-driven updates.
  new MutationObserver((mutations) => {
    mutations.forEach((mutation) => {
      mutation.addedNodes.forEach((node) => {
        if (node.nodeType !== Node.ELEMENT_NODE) return;
        if (node.matches("a[href], form[action]")) {
          syncPartyLinks(node.parentElement || node);
        } else {
          syncPartyLinks(node);
        }
      });
    });
  }).observe(document.body, { childList: true, subtree: true });
})();
