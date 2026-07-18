const hosts = document.querySelectorAll('[data-render-config]:not([data-render-started])');
for (const host of hosts) {
  host.dataset.renderStarted = 'true';
  const canvas = host.querySelector('[data-renderer-canvas]');
  const status = host.querySelector('[data-renderer-status]');
  if (!navigator.gpu) { status.textContent = 'WebGPU is unavailable; using the accessible paper fallback.'; continue; }
  try {
    const renderer = await import('/tactical/wasm/adventuresim_tactical_client.js');
    await renderer.default();
    await renderer.wasm_run_config(host.dataset.renderConfig);
    canvas.hidden = false;
    host.querySelector('[data-renderer-fallback]').hidden = true;
    status.textContent = 'Interactive renderer ready. Drag to pan and use the wheel to zoom.';
    document.addEventListener('visibilitychange', () => renderer.wasm_set_suspended(document.hidden));
  } catch (error) {
    console.error('Strategic renderer initialization failed', error);
    canvas.hidden = true;
    status.textContent = 'Interactive renderer could not start; using the accessible paper fallback.';
  }
}
