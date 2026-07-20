(() => {
  const active = new Map();
  let leavingPage = false;
  let initialQueue = Promise.resolve();

  const reportError = (error, context) => {
    if (error?.name === "AbortError") return;
    console.error(`Strategic request failed (${context})`, error);
    document.dispatchEvent(new CustomEvent("strategic-request-error", { detail: { error, context } }));
  };
  window.reportStrategicError = reportError;

  // Shared return-navigation contract for multi-page workflows. Callers may
  // carry a local path (including its query and fragment) in `return_to`.
  window.strategicLocalReturnUrl = (value) => {
    if (typeof value !== "string" || !value.startsWith("/") || value.startsWith("//") || value.includes("\\")) return null;
    try {
      const parsed = new URL(value, location.origin);
      return parsed.origin === location.origin ? `${parsed.pathname}${parsed.search}${parsed.hash}` : null;
    } catch (_) {
      return null;
    }
  };
  window.strategicApplyReturnNavigation = (root = document) => {
    const returnTo = window.strategicLocalReturnUrl(new URLSearchParams(location.search).get("return_to"));
    if (!returnTo) return;
    root.querySelectorAll?.('input[name="return_to"]').forEach((input) => { input.value = returnTo; });
  };
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => window.strategicApplyReturnNavigation(), { once: true });
  } else {
    window.strategicApplyReturnNavigation();
  }
  document.addEventListener("strategic-live-regions-refreshed", (event) => {
    window.strategicApplyReturnNavigation(event.target);
  });

  window.strategicFetch = async (input, init = {}) => {
    try {
      const response = await fetch(input, init);
      if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
      return response;
    } catch (error) {
      reportError(error, typeof input === "string" ? input : input.url);
      throw error;
    }
  };

  const abortAll = () => {
    if (leavingPage) return;
    leavingPage = true;
    active.forEach((controller) => controller.abort());
    active.clear();
  };

  window.strategicBackgroundFetch = (key, input, init = {}) => {
    active.get(key)?.abort();
    const controller = new AbortController();
    active.set(key, controller);

    if (init.signal) {
      if (init.signal.aborted) controller.abort();
      else init.signal.addEventListener("abort", () => controller.abort(), { once: true });
    }

    return window.strategicFetch(input, { ...init, signal: controller.signal }).finally(() => {
      if (active.get(key) === controller) active.delete(key);
    });
  };

  window.queueStrategicInitialLoad = (task) => {
    const result = initialQueue.then(() => (leavingPage ? undefined : task()));
    initialQueue = result.catch((error) => reportError(error, "initial load"));
    return result;
  };

  document.addEventListener("strategic-navigation-start", abortAll);
  window.addEventListener("pagehide", abortAll, { once: true });
})();
