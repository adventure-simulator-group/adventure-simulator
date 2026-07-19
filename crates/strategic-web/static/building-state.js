(() => {
  const services = new Set(["map", "merchants", "weapons", "armor", "clothing", "inn", "religion"]);
  const nav = document.querySelector("[data-settlement-id]");
  if (!nav) return;
  const current = new URL(window.location.href);
  const requested = current.searchParams.get("building");
  const serverActive = nav.querySelector(".nav-tab.active")?.dataset.serviceId;
  const building = services.has(requested) ? requested : (services.has(serverActive) ? serverActive : "map");
  if (requested && !services.has(requested)) {
    current.searchParams.delete("building");
    history.replaceState(null, "", current);
  }
  nav.querySelectorAll("[data-service-id]").forEach((tab) => {
    const selected = tab.dataset.serviceId === building;
    tab.classList.toggle("active", selected);
    tab.setAttribute("aria-current", selected ? "page" : "false");
    if (selected) document.documentElement.style.setProperty("--active-building-tint", tab.style.getPropertyValue("--building-tint"));
  });
  document.querySelectorAll("a[href], form[action]").forEach((node) => {
    const attribute = node.matches("form") ? "action" : "href";
    const raw = node.getAttribute(attribute);
    if (!raw || !raw.startsWith("/locations/")) return;
    const url = new URL(raw, window.location.origin);
    if (!url.pathname.includes("/party")) return;
    url.searchParams.set("building", building);
    node.setAttribute(attribute, `${url.pathname}${url.search}${url.hash}`);
  });
})();
