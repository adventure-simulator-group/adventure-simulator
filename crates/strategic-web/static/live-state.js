(() => {
  const host = document.querySelector("#strategic-live-stream");
  let marker = document.querySelector("#strategic-live-revision");
  if (!marker) return;

  let revision = marker.dataset.liveRevision;
  let navigating = false;
  let synchronizing = false;
  let synchronizationPending = false;
  let baselineReceived = revision !== "0";

  const beginNavigation = () => {
    if (navigating) return;
    navigating = true;
    document.dispatchEvent(new CustomEvent("strategic-navigation-start"));
  };

  const locationMatches = ({ kind, id }) => {
    // Character selection is an intentional escape from the current location.
    if (location.pathname.startsWith("/characters")) return true;
    if (!kind || !id) return location.pathname === "/characters";
    const encoded = encodeURIComponent(id);
    if (kind === "quest") return location.pathname.startsWith(`/locations/quest/${encoded}`);
    return location.pathname.startsWith(`/locations/settlement/${encoded}`)
      || location.pathname.startsWith(`/settlements/${encoded}`);
  };

  const dispatchUpdate = () => {
    document.dispatchEvent(new CustomEvent("strategic-live-update", {
      detail: { revision },
    }));
    document.querySelectorAll("[data-live-refresh]").forEach((element) => {
      element.dispatchEvent(new CustomEvent("strategic-live-update"));
    });
  };

  const synchronize = async () => {
    if (navigating) return;
    if (synchronizing) {
      synchronizationPending = true;
      return;
    }
    synchronizing = true;
    do {
      synchronizationPending = false;
      const response = await window.strategicBackgroundFetch("live-navigation", "/api/live/navigation", {
        headers: { Accept: "application/json" },
      });
      if (!response.ok) continue;
      const state = await response.json();
      if (!locationMatches(state)) {
        beginNavigation();
        location.assign(state.path);
        return;
      }
      dispatchUpdate();
    } while (synchronizationPending && !navigating);
    synchronizing = false;
  };

  document.addEventListener("submit", (event) => {
    if (event.defaultPrevented || event.target.target === "_blank") return;
    beginNavigation();
  });

  document.addEventListener("click", (event) => {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    const link = event.target.closest("a[href]");
    if (!link || event.defaultPrevented || link.target === "_blank") return;
    const href = link.getAttribute("href");
    if (!href || href.startsWith("#") || link.hasAttribute("download")) return;
    const destination = new URL(link.href, location.href);
    if (destination.origin === location.origin && destination.href !== location.href) {
      beginNavigation();
    }
  });

  new MutationObserver(() => {
    marker = document.querySelector("#strategic-live-revision");
    if (!marker) return;
    if (marker.dataset.liveRevision === revision) return;
    revision = marker.dataset.liveRevision;
    // The stream's first patch establishes its baseline. Refreshing here would
    // immediately duplicate every request that rendered the new page.
    if (!baselineReceived) {
      baselineReceived = true;
      return;
    }
    synchronize().catch(() => { synchronizing = false; });
  }).observe(host, {
    attributes: true,
    childList: true,
    subtree: true,
    attributeFilter: ["data-live-revision"],
  });
})();
