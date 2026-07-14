(() => {
  const host = document.querySelector("#strategic-live-stream");
  let marker = document.querySelector("#strategic-live-revision");
  if (!marker) return;

  let revision = marker.dataset.liveRevision;
  let navigating = false;

  const locationMatches = ({ kind, id }) => {
    // Character selection is an intentional escape from the current location.
    if (location.pathname.startsWith("/characters")) return true;
    if (!kind || !id) return location.pathname === "/characters";
    const encoded = encodeURIComponent(id);
    if (kind === "quest") return location.pathname.startsWith(`/locations/quest/${encoded}`);
    return location.pathname.startsWith(`/locations/settlement/${encoded}`)
      || location.pathname.startsWith(`/settlements/${encoded}`);
  };

  const synchronize = async () => {
    document.dispatchEvent(new CustomEvent("strategic-live-update", {
      detail: { revision: marker.dataset.liveRevision },
    }));
    document.querySelectorAll("[data-live-refresh]").forEach((element) => {
      element.dispatchEvent(new CustomEvent("strategic-live-update"));
    });
    if (navigating) return;
    const response = await fetch("/api/live/navigation", { headers: { Accept: "application/json" } });
    if (!response.ok) return;
    const state = await response.json();
    if (!locationMatches(state)) {
      navigating = true;
      location.assign(state.path);
    }
  };

  new MutationObserver(() => {
    marker = document.querySelector("#strategic-live-revision");
    if (!marker) return;
    if (marker.dataset.liveRevision === revision) return;
    revision = marker.dataset.liveRevision;
    synchronize().catch(() => {});
  }).observe(host, { attributes: true, childList: true, subtree: true, attributeFilter: ["data-live-revision"] });
})();
