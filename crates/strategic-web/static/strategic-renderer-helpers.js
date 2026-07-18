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

  const api = { validMarkerId, canonicalMarkerAnchor, consumeMarkerSelection };
  if (typeof window !== 'undefined') window.strategicRendererHelpers = api;
  if (typeof module !== 'undefined') module.exports = api;
})();
