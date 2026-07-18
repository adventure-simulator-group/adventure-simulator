const hosts = [...document.querySelectorAll('[data-render-config]:not([data-render-started])')];
const { consumeMarkerSelection } = window.strategicRendererHelpers;

function setStatus(status, message) {
  if (status) status.textContent = message;
}

async function pollHandoff(url, status, fallback, signal) {
  while (!signal.aborted) {
    const response = await fetch(url, { credentials: 'same-origin', headers: { Accept: 'application/json' } });
    const outcome = await response.json();
    if (!response.ok) throw new Error(outcome.status === 'unauthorized' ? 'Mission handoff is not authorized.' : 'Mission status is unavailable.');
    if (outcome.fallback_url) {
      fallback.href = outcome.fallback_url;
      fallback.hidden = false;
    }
    if (outcome.status === 'ready') return outcome;
    if (outcome.status === 'failed' || outcome.status === 'ended') throw new Error(`Mission ${outcome.status}.`);
    setStatus(status, 'Allocating a tactical server…');
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new DOMException('Handoff canceled', 'AbortError');
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
  let busy = false;
  let handedOff = false;
  form.addEventListener('submit', async (event) => {
    if (!host.classList.contains('renderer-enhanced')) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    if (busy) return;
    busy = true;
    submit.disabled = true;
    renderer.wasm_set_allocating(true);
    setStatus(status, 'Requesting tactical combat…');
    const controller = new AbortController();
    try {
      const response = await fetch(form.action, {
        method: 'POST', credentials: 'same-origin',
        headers: { Accept: 'application/json' }, signal: controller.signal,
      });
      const outcome = await response.json();
      if (outcome.outcome === 'approval_required') {
        setStatus(status, outcome.message);
        return;
      }
      if (!response.ok || outcome.outcome !== 'executed') throw new Error(outcome.message || 'Combat could not be started.');
      fallback.href = outcome.status_url;
      fallback.hidden = false;
      const ready = await pollHandoff(outcome.handoff_url, status, fallback, controller.signal);
      renderer.wasm_tactical_handoff(JSON.stringify({
        handoff_schema: 1, player_id: ready.player_id, server_url: ready.server_url,
      }));
      handedOff = true;
      setStatus(status, 'Connecting to the tactical server…');
      const statusTimer = setInterval(() => {
        const phase = renderer.wasm_renderer_status();
        if (phase === 'tactical_connected') {
          clearInterval(statusTimer);
          setStatus(status, 'Connected. Tactical combat is active.');
        } else if (phase === 'tactical_failed') {
          clearInterval(statusTimer);
          setStatus(status, 'The tactical connection failed. The strategic scene has been restored; use mission status to retry.');
        }
      }, 200);
    } catch (error) {
      console.error('Live tactical handoff failed', error);
      setStatus(status, `${error.message} Use the normal mission status link to continue.`);
    } finally {
      if (!handedOff) {
        renderer.wasm_set_allocating(false);
        submit.disabled = false;
        busy = false;
      }
    }
  }, { capture: true });
}

if (hosts.length > 1) {
  for (const host of hosts) setStatus(host.querySelector('[data-renderer-status]'), 'Only one interactive renderer can run on this page; using the accessible fallback.');
} else for (const host of hosts) {
  host.dataset.renderStarted = 'true';
  const canvas = host.querySelector('[data-renderer-canvas]');
  const status = host.querySelector('[data-renderer-status]');
  if (!navigator.gpu) { setStatus(status, 'WebGPU is unavailable; using the accessible paper fallback.'); continue; }
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
        if (JSON.stringify(compiledPackage.bounds) !== JSON.stringify(manifest.bounds)) throw new Error('map package bounds do not match its manifest');
        config.startup.package = compiledPackage;
      } catch (artifactError) { console.warn('Compiled map artifacts unavailable; using embedded map package', artifactError); }
    }
    renderer.wasm_set_suspended(document.hidden);
    await renderer.wasm_run_config(JSON.stringify(config));
    canvas.hidden = false;
    host.classList.add('renderer-enhanced');
    setStatus(status, config.startup.mode === 'strategic_map' ? 'Interactive renderer ready. Drag to pan and use the wheel to zoom.' : 'Interactive strategic scene ready.');
    document.addEventListener('visibilitychange', () => renderer.wasm_set_suspended(document.hidden));
    setInterval(() => consumeMarkerSelection(renderer, document, location.href), 100);
    installHandoff(renderer, host);
  } catch (error) {
    console.error('Strategic renderer initialization failed', error);
    canvas.hidden = true;
    setStatus(status, 'Interactive renderer could not start; using the accessible paper fallback.');
  }
}
