(() => {
  const active = new Map();
  let leavingPage = false;
  let initialQueue = Promise.resolve();

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

    return fetch(input, { ...init, signal: controller.signal }).finally(() => {
      if (active.get(key) === controller) active.delete(key);
    });
  };

  window.queueStrategicInitialLoad = (task) => {
    const result = initialQueue.then(() => (leavingPage ? undefined : task()));
    initialQueue = result.catch(() => {});
    return result;
  };

  document.addEventListener("strategic-navigation-start", abortAll);
  window.addEventListener("pagehide", abortAll, { once: true });
})();
