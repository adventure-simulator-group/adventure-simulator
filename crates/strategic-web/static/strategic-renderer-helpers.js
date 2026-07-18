(() => {
  function validMarkerId(id) {
    return typeof id === 'string' && id.length > 0 && id.length <= 512 && !/[\u0000-\u001f\u007f]/.test(id);
  }

  function canonicalMarkerAnchor(id, root, currentHref) {
    if (!validMarkerId(id)) return null;
    const anchor = [...root.querySelectorAll('a[data-map-marker-id]')]
      .find((candidate) => candidate.dataset.mapMarkerId === id);
    if (!anchor) return null;
    let destination;
    let current;
    try {
      current = new URL(currentHref);
      destination = new URL(anchor.href, current);
    } catch { return null; }
    if (destination.origin !== current.origin || !anchor.getAttribute('href')?.startsWith('/')) return null;
    return anchor;
  }

  function consumeMarkerSelection(renderer, root, currentHref) {
    const id = renderer.wasm_take_marker_selection();
    const anchor = canonicalMarkerAnchor(id, root, currentHref);
    if (anchor) anchor.click();
    return Boolean(anchor);
  }

  function abortError() {
    const error = new Error('Operation canceled');
    error.name = 'AbortError';
    return error;
  }

  function waitWithSignal(milliseconds, signal) {
    if (signal.aborted) return Promise.reject(abortError());
    return new Promise((resolve, reject) => {
      const onAbort = () => {
        clearTimeout(timer);
        reject(abortError());
      };
      const timer = setTimeout(() => {
        signal.removeEventListener('abort', onAbort);
        resolve();
      }, milliseconds);
      signal.addEventListener('abort', onAbort, { once: true });
    });
  }

  function createAttemptState(onChange, AbortControllerType = AbortController) {
    let state = { active: false, handedOff: false, controller: null };
    const publish = () => onChange?.({
      active: state.active,
      handedOff: state.handedOff,
      signal: state.controller?.signal ?? null,
    });
    return {
      begin() {
        if (state.active) return false;
        state = { active: true, handedOff: false, controller: new AbortControllerType() };
        publish();
        return true;
      },
      markHandedOff() {
        if (!state.active) return false;
        state.handedOff = true;
        publish();
        return true;
      },
      reset({ abort = true } = {}) {
        if (abort) state.controller?.abort();
        state = { active: false, handedOff: false, controller: null };
        publish();
      },
      snapshot() {
        return {
          active: state.active,
          handedOff: state.handedOff,
          signal: state.controller?.signal ?? null,
        };
      },
    };
  }

  async function pollForReady({
    requestStatus,
    signal,
    wait = waitWithSignal,
    onOutcome,
    initialDelay = 500,
    maximumDelay = 4_000,
  }) {
    let delay = initialDelay;
    while (!signal.aborted) {
      let outcome;
      try {
        outcome = await requestStatus(signal);
      } catch (error) {
        if (signal.aborted || error.name === 'AbortError') throw error;
        await wait(delay, signal);
        delay = Math.min(delay * 2, maximumDelay);
        continue;
      }
      onOutcome?.(outcome);
      if (outcome.status === 'ready') return outcome;
      if (outcome.status === 'failed' || outcome.status === 'ended' || outcome.status === 'unauthorized') {
        throw new Error(`Mission ${outcome.status}.`);
      }
      await wait(delay, signal);
      delay = Math.min(delay * 2, maximumDelay);
    }
    throw abortError();
  }

  async function monitorTacticalMission({
    requestStatus,
    rendererStatus,
    signal,
    wait = waitWithSignal,
    interval = 5_000,
    onConnected,
    onEnded,
    onFailed,
  }) {
    let connectedReported = false;
    while (!signal.aborted) {
      const phase = rendererStatus();
      if (phase === 'tactical_failed') {
        onFailed?.({ status: 'failed' });
        return 'failed';
      }
      if (phase === 'tactical_connected' && !connectedReported) {
        connectedReported = true;
        onConnected?.();
      }
      await wait(interval, signal);
      let outcome;
      try {
        outcome = await requestStatus(signal);
      } catch (error) {
        if (signal.aborted || error.name === 'AbortError') throw error;
        continue;
      }
      if (outcome.status === 'ended') {
        onEnded?.(outcome);
        return 'ended';
      }
      if (outcome.status === 'failed' || outcome.status === 'unauthorized') {
        onFailed?.(outcome);
        return 'failed';
      }
    }
    throw abortError();
  }

  function canonicalNavigationPath(path, currentHref) {
    if (typeof path !== 'string' || !path.startsWith('/') || path.startsWith('//') || path.includes('\\')) return null;
    try {
      const current = new URL(currentHref);
      const destination = new URL(path, current);
      return destination.origin === current.origin ? `${destination.pathname}${destination.search}${destination.hash}` : null;
    } catch { return null; }
  }

  const api = {
    validMarkerId,
    canonicalMarkerAnchor,
    consumeMarkerSelection,
    waitWithSignal,
    createAttemptState,
    pollForReady,
    monitorTacticalMission,
    canonicalNavigationPath,
  };
  if (typeof window !== 'undefined') window.strategicRendererHelpers = api;
  if (typeof module !== 'undefined') module.exports = api;
})();
