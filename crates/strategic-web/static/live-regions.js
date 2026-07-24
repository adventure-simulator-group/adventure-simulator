(() => {
  const createDirtyRetryPolicy = ({ setTimer, clearTimer, maxAttempts = 6 }) => {
    let attempts = 0;
    let timer;
    return {
      schedule(callback) {
        if (attempts >= maxAttempts) return false;
        clearTimer(timer);
        const delay = Math.min(4000, 250 * (2 ** attempts));
        attempts += 1;
        timer = setTimer(callback, delay);
        return true;
      },
      reset() {
        attempts = 0;
        clearTimer(timer);
        timer = undefined;
      },
      attempts: () => attempts,
    };
  };
  window.strategicLiveRetryPolicyFactory = createDirtyRetryPolicy;
  const liveRefreshUrl = (root = document, currentLocation = location) => (
    root.querySelector("[data-live-refresh-url]")?.dataset.liveRefreshUrl
      || `${currentLocation.pathname}${currentLocation.search}`
  );
  window.strategicLiveRefreshUrl = liveRefreshUrl;
  if (!document.querySelector("#strategic-live-revision")) return;

  let generation = 0;
  let refreshTimer;
  let navigating = false;
  let dirtyRetryPending = false;
  const dirtyRetry = createDirtyRetryPolicy({
    setTimer: window.setTimeout.bind(window),
    clearTimer: window.clearTimeout.bind(window),
  });

  const draftMaps = ["merchantDraft", "merchantSells", "partyTradeDraft",
    "inventoryDiscardDraft", "lootTransferDraft", "poolTransferDraft"];

  const hasStagedInventoryChanges = () => draftMaps.some(
    (name) => window.strategicTradeUi?.state?.[name]?.size > 0,
  )
    || [...document.querySelectorAll("form.party-offer, #inventory-discard")]
      .some((form) => !form.hidden && !form.hasAttribute("hidden"));

  const sidebarsAreBusy = () => {
    const active = document.activeElement?.closest?.(".left-sidebar, .right-sidebar")
      ? document.activeElement
      : null;
    const editing = active?.matches?.("input, textarea, select, [contenteditable='true'], [role='slider']");
    return hasStagedInventoryChanges()
      || Boolean(window.strategicRestDuration?.isDirty?.(document))
      || Boolean(document.querySelector("dialog[open], [data-role-inspection-panel], [data-service-role-inspection], [data-activity-modal]:not([hidden])"))
      || Boolean(document.querySelector('.numeric-editor'))
      || Boolean(editing);
  };

  const selectedInventoryTab = () => document.querySelector("[data-inventory-tab].active")?.dataset.inventoryTab;
  const scheduleEditorIsPending = () => Boolean(
    document.querySelector('[data-skill-schedule][data-schedule-pending]'),
  );

  const restoreInventoryTab = (name) => {
    if (!name) return;
    document.querySelectorAll("[data-inventory-tabs]").forEach((root) => {
      root.querySelectorAll("[data-inventory-tab]").forEach((tab) => {
        tab.classList.toggle("active", tab.dataset.inventoryTab === name);
      });
      root.querySelectorAll("[data-inventory-pane]").forEach((pane) => {
        pane.hidden = pane.dataset.inventoryPane !== name;
      });
    });
    const scope = document.querySelector("#merchant-offer [name='inventory_scope']");
    if (scope) scope.value = name;
  };

  const replaceIfChanged = (selector, nextDocument) => {
    const current = document.querySelector(selector);
    const next = nextDocument.querySelector(selector);
    if (!current || !next || current.outerHTML === next.outerHTML) return false;
    current.replaceWith(next);
    return true;
  };

  const scrollOffsets = (selector) => {
    const region = document.querySelector(selector);
    return region ? { left: region.scrollLeft, top: region.scrollTop } : null;
  };

  const restoreScrollOffsets = (selector, offsets) => {
    if (!offsets) return;
    const region = document.querySelector(selector);
    if (!region) return;
    region.scrollLeft = offsets.left;
    region.scrollTop = offsets.top;
  };

  const refresh = async () => {
    if (navigating) return;
    const currentGeneration = ++generation;
    const schedulePendingAtStart = scheduleEditorIsPending();
    const response = await window.strategicBackgroundFetch("live-regions", liveRefreshUrl(), {
      headers: { Accept: "text/html", "X-Strategic-Live-Region": "true" },
    });
    if (!response.ok) return;
    const nextDocument = new DOMParser().parseFromString(await response.text(), "text/html");
    if (currentGeneration !== generation) return;

    // Match the post-mount table structure before comparing it with the live DOM.
    window.strategicTradeUi?.mountInventoryBulkControls?.(nextDocument);
    window.strategicInventoryBrowser?.mountAll?.(nextDocument);
    window.strategicRestDuration?.mountAll?.(nextDocument);
    const inventoryTab = selectedInventoryTab();
    const leftSidebarScroll = scrollOffsets(".left-sidebar");
    const rightSidebarScroll = scrollOffsets(".right-sidebar");
    const replaced = [];

    if (replaceIfChanged("[data-party-portrait-members]", nextDocument)) {
      replaced.push("party-portraits");
    }
    if (!sidebarsAreBusy()) {
      dirtyRetryPending = false;
      dirtyRetry.reset();
      if (!schedulePendingAtStart && !scheduleEditorIsPending()
        && replaceIfChanged(".left-sidebar", nextDocument)) replaced.push("left-sidebar");
      if (replaceIfChanged(".right-sidebar", nextDocument)) replaced.push("right-sidebar");
    } else {
      // Do not lose the invalidation while an editor protects its local draft.
      // A bounded timer keeps retrying until the region is safe to reconcile.
      dirtyRetryPending = true;
      dirtyRetry.schedule(() => {
        if (dirtyRetryPending) refresh().catch((error) => window.reportStrategicError(error, "live regions"));
      });
    }

    if (!replaced.length) return;
    restoreInventoryTab(inventoryTab);
    if (replaced.includes("left-sidebar")) restoreScrollOffsets(".left-sidebar", leftSidebarScroll);
    if (replaced.includes("right-sidebar")) restoreScrollOffsets(".right-sidebar", rightSidebarScroll);
    document.dispatchEvent(new CustomEvent("strategic-live-regions-refreshed", {
      detail: { regions: replaced },
    }));
  };

  const scheduleRefresh = () => {
    if (navigating) return;
    window.clearTimeout(refreshTimer);
    refreshTimer = window.setTimeout(() => refresh().catch((error) => window.reportStrategicError(error, "live regions")), 40);
  };

  const reconcileDirtyWhenIdle = () => {
    if (!dirtyRetryPending || sidebarsAreBusy()) return;
    dirtyRetry.reset();
    scheduleRefresh();
  };

  document.addEventListener("strategic-navigation-start", () => {
    navigating = true;
    generation += 1;
    window.clearTimeout(refreshTimer);
    dirtyRetry.reset();
  });

  document.addEventListener("strategic-live-update", scheduleRefresh);
  document.addEventListener("strategic-live-refresh-requested", scheduleRefresh);
  document.addEventListener("focusout", reconcileDirtyWhenIdle);
  document.addEventListener("change", reconcileDirtyWhenIdle);
  document.addEventListener("strategic-editor-idle", reconcileDirtyWhenIdle);
  document.addEventListener("strategic-trade-draft-changed", reconcileDirtyWhenIdle);
})();
