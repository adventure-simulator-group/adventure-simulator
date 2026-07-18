const hosts = document.querySelectorAll('[data-render-config]:not([data-render-started])');
for (const host of hosts) {
  host.dataset.renderStarted = 'true';
  const canvas = host.querySelector('[data-renderer-canvas]');
  const status = host.querySelector('[data-renderer-status]');
  if (!navigator.gpu) { status.textContent = 'WebGPU is unavailable; using the accessible paper fallback.'; continue; }
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
    status.textContent = 'Interactive renderer ready. Drag to pan and use the wheel to zoom.';
    document.addEventListener('visibilitychange', () => renderer.wasm_set_suspended(document.hidden));
  } catch (error) {
    console.error('Strategic renderer initialization failed', error);
    canvas.hidden = true;
    status.textContent = 'Interactive renderer could not start; using the accessible paper fallback.';
  }
}
