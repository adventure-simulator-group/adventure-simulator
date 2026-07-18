const hosts = [...document.querySelectorAll('[data-render-config]:not([data-render-started])')];
const {
  consumeMarkerSelection,
  createAttemptState,
  pollForReady,
  monitorTacticalMission,
  canonicalNavigationPath,
} = window.strategicRendererHelpers;

function setStatus(status, message) {
  if (status) status.textContent = message;
}

async function requestJson(url, options, signal) {
  const response = await fetch(url, { ...options, signal });
  const outcome = await response.json();
  if (!response.ok && !outcome.status && !outcome.outcome) {
    throw new Error(outcome.status === 'unauthorized'
      ? 'Mission handoff is not authorized.'
      : 'Mission status is unavailable.');
  }
  return outcome;
}

function installHandoff(renderer, host) {
  const form = document.querySelector('form[data-live-tactical-handoff]');
  if (!form) return;
  const submit = form.querySelector('[data-tactical-submit]');
  const status = form.querySelector('[data-tactical-status]');
  const fallback = document.createElement('a');
  fallback.className = 'btn btn-primary mt-1';
  fallback.textContent = 'Open tactical mission status';
  fallback.hidden = true;
  status.after(fallback);
  form.dataset.handoffInterceptReady = 'true';

  const updateFallback = (outcome) => {
    if (!outcome?.fallback_url) return;
    fallback.href = outcome.fallback_url;
    fallback.hidden = false;
  };
  const attempt = createAttemptState(({ active, handedOff }) => {
    submit.disabled = active;
    renderer.wasm_set_allocating(active && !handedOff);
  });
  const cancelAttempt = () => attempt.reset();
  window.addEventListener('pagehide', cancelAttempt, { once: true });
  document.addEventListener('strategic-navigation-start', cancelAttempt);
  const teardownObserver = new MutationObserver(() => {
    if (!host.isConnected) {
      cancelAttempt();
      teardownObserver.disconnect();
    }
  });
  teardownObserver.observe(document.documentElement, { childList: true, subtree: true });

  form.addEventListener('submit', async (event) => {
    if (!host.classList.contains('renderer-enhanced')) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    if (!attempt.begin()) return;
    const signal = attempt.snapshot().signal;
    setStatus(status, 'Requesting tactical combat…');
    try {
      const outcome = await requestJson(form.action, {
        method: 'POST',
        credentials: 'same-origin',
        headers: { Accept: 'application/json' },
      }, signal);
      if (outcome.outcome === 'approval_required') {
        setStatus(status, outcome.message);
        attempt.reset();
        return;
      }
      if (outcome.outcome !== 'executed') {
        throw new Error(outcome.message || 'Combat could not be started.');
      }
      fallback.href = outcome.status_url;
      fallback.hidden = false;
      const requestStatus = (pollSignal) => requestJson(outcome.handoff_url, {
        credentials: 'same-origin',
        headers: { Accept: 'application/json' },
      }, pollSignal);
      const ready = await pollForReady({
        requestStatus,
        signal,
        onOutcome: (handoff) => {
          updateFallback(handoff);
          if (handoff.status === 'pending') {
            setStatus(status, 'Allocating a tactical server…');
          }
        },
      });
      renderer.wasm_tactical_handoff(JSON.stringify({
        handoff_schema: 1,
        player_id: ready.player_id,
        server_url: ready.server_url,
      }));
      attempt.markHandedOff();
      setStatus(status, 'Connecting to the tactical server…');
      await monitorTacticalMission({
        requestStatus,
        rendererStatus: () => renderer.wasm_renderer_status(),
        signal,
        onConnected: () => setStatus(status, 'Connected. Tactical combat is active.'),
        onFailed: (handoff) => {
          updateFallback(handoff);
          setStatus(status, 'The tactical connection failed. The strategic scene has been restored; use mission status to retry.');
          attempt.reset();
        },
        onEnded: (handoff) => {
          updateFallback(handoff);
          const path = canonicalNavigationPath(handoff.fallback_url, location.href);
          if (!path) {
            setStatus(status, 'Mission ended. Use the mission status link to continue.');
            attempt.reset();
            return;
          }
          attempt.reset();
          document.dispatchEvent(new CustomEvent('strategic-navigation-start'));
          location.assign(path);
        },
      });
    } catch (error) {
      if (error.name !== 'AbortError') {
        console.error('Live tactical handoff failed', error);
        setStatus(status, `${error.message} Use the normal mission status link to continue.`);
      }
      if (attempt.snapshot().active) attempt.reset();
    }
  }, { capture: true });
}

if (hosts.length > 1) {
  for (const host of hosts) {
    setStatus(host.querySelector('[data-renderer-status]'), 'Only one interactive renderer can run on this page; using the accessible fallback.');
  }
} else for (const host of hosts) {
  host.dataset.renderStarted = 'true';
  const canvas = host.querySelector('[data-renderer-canvas]');
  const status = host.querySelector('[data-renderer-status]');
  if (!navigator.gpu) {
    setStatus(status, 'WebGPU is unavailable; using the accessible paper fallback.');
    continue;
  }
  try {
    const renderer = await import('/tactical/wasm/adventuresim-tactical-client.js');
    await renderer.default();
    const config = JSON.parse(host.dataset.renderConfig);
    if (config.startup.mode === 'strategic_map' && config.startup.package_url) {
      try {
        const manifestResponse = await fetch(config.startup.package_url, { credentials: 'same-origin', cache: 'no-cache' });
        if (!manifestResponse.ok) throw new Error(`map manifest returned ${manifestResponse.status}`);
        const manifest = await manifestResponse.json();
        renderer.wasm_validate_manifest(JSON.stringify(manifest));
        const packageResponse = await fetch(manifest.package_url, { credentials: 'same-origin', cache: 'force-cache' });
        if (!packageResponse.ok) throw new Error(`map package returned ${packageResponse.status}`);
        const compiledPackage = await packageResponse.json();
        if (JSON.stringify(compiledPackage.bounds) !== JSON.stringify(manifest.bounds)) {
          throw new Error('map package bounds do not match its manifest');
        }
        config.startup.package = compiledPackage;
      } catch (artifactError) {
        console.warn('Compiled map artifacts unavailable; using embedded map package', artifactError);
      }
    }
    renderer.wasm_set_suspended(document.hidden);
    await renderer.wasm_run_config(JSON.stringify(config));
    canvas.hidden = false;
    host.classList.add('renderer-enhanced');
    setStatus(status, config.startup.mode === 'strategic_map'
      ? 'Interactive renderer ready. Drag to pan and use the wheel to zoom.'
      : 'Interactive strategic scene ready.');
    document.addEventListener('visibilitychange', () => renderer.wasm_set_suspended(document.hidden));
    const markerTimer = setInterval(() => consumeMarkerSelection(renderer, document, location.href), 100);
    const stopMarkerTimer = () => clearInterval(markerTimer);
    window.addEventListener('pagehide', stopMarkerTimer, { once: true });
    document.addEventListener('strategic-navigation-start', stopMarkerTimer, { once: true });
    installHandoff(renderer, host);
  } catch (error) {
    console.error('Strategic renderer initialization failed', error);
    canvas.hidden = true;
    setStatus(status, 'Interactive renderer could not start; using the accessible paper fallback.');
  }
}
