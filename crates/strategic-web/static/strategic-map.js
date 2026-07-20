(() => {
  "use strict";

  const THEME_KEY = "adventuresim.map-theme";
  const PIN_REFERENCE_WIDTH = 390;
  const MIN_VIEW_WIDTH = 20;
  const MIN_VIEW_HEIGHT = MIN_VIEW_WIDTH / 1.5;
  const THEMES = new Set(["paper", "atlas"]);
  const SVG_NS = "http://www.w3.org/2000/svg";
  const parseViewBox = (svg) => svg.getAttribute("viewBox").trim().split(/\s+/).map(Number);
  const scalePins = (svg, width) => {
    const scale = width / PIN_REFERENCE_WIDTH;
    svg.querySelectorAll("[data-map-pin-symbol]").forEach((symbol) => {
      symbol.setAttribute("transform", `scale(${scale.toFixed(5)})`);
    });
  };
  const writeViewBox = (svg, view) => {
    svg.setAttribute("viewBox", view.map((value) => value.toFixed(2)).join(" "));
    scalePins(svg, view[2]);
  };
  const tileZoom = (viewWidth, pixelWidth, pixelRatio, maxZoom) => {
    const density = Math.max(1, pixelWidth * pixelRatio / viewWidth);
    return Math.min(maxZoom, Math.max(0, Math.ceil(Math.log2(density))));
  };
  const visibleTileRange = ([x, y, width, height], tileSize, zoom, padding = 1) => {
    const span = tileSize / 2 ** zoom;
    const maxX = Math.ceil(1200 / span) - 1;
    const maxY = Math.ceil(800 / span) - 1;
    return {
      span,
      minX: Math.max(0, Math.floor(x / span) - padding),
      maxX: Math.min(maxX, Math.floor((x + width) / span) + padding),
      minY: Math.max(0, Math.floor(y / span) - padding),
      maxY: Math.min(maxY, Math.floor((y + height) / span) + padding),
    };
  };
  const renderTiles = (map, svg, view, theme = map.dataset.mapTheme) => {
    const layer = svg.querySelector("[data-map-tile-layer]");
    if (!layer) return;
    const tileSize = Number(map.dataset.mapTileSize);
    const maxZoom = Number(map.dataset.mapMaxTileZoom);
    const version = map.dataset.mapTileVersion;
    const root = map.dataset.mapTileRoot;
    if (!tileSize || !Number.isFinite(maxZoom) || !version || !root) return;
    const rect = svg.getBoundingClientRect();
    const zoom = tileZoom(view[2], rect.width || 768, globalThis.devicePixelRatio || 1, maxZoom);
    const range = visibleTileRange(view, tileSize, zoom);
    const key = [theme, zoom, range.minX, range.maxX, range.minY, range.maxY].join(":");
    if (layer.dataset.tileKey === key) return;
    const tiles = [];
    for (let y = range.minY; y <= range.maxY; y += 1) {
      for (let x = range.minX; x <= range.maxX; x += 1) {
        const image = document.createElementNS(SVG_NS, "image");
        image.setAttribute("x", (x * range.span).toFixed(3));
        image.setAttribute("y", (y * range.span).toFixed(3));
        image.setAttribute("width", (range.span + 0.02).toFixed(3));
        image.setAttribute("height", (range.span + 0.02).toFixed(3));
        image.setAttribute("preserveAspectRatio", "none");
        image.setAttribute("href", `${root}${theme}/${zoom}/${x}/${y}.avif?v=${version}`);
        tiles.push(image);
      }
    }
    layer.replaceChildren(...tiles);
    layer.dataset.tileKey = key;
  };
  const zoomedView = ([x, y, width, height], factor, focusX = x + width / 2, focusY = y + height / 2) => {
    const nextWidth = Math.min(1200, Math.max(MIN_VIEW_WIDTH, width * factor));
    const nextHeight = Math.min(800, Math.max(MIN_VIEW_HEIGHT, height * factor));
    const ratioX = (focusX - x) / width;
    const ratioY = (focusY - y) / height;
    return [focusX - nextWidth * ratioX, focusY - nextHeight * ratioY, nextWidth, nextHeight];
  };
  const pannedView = ([x, y, width, height], dx, dy) => [x + dx, y + dy, width, height];

  const applyTheme = (map, theme, storage = globalThis.localStorage) => {
    const chosen = THEMES.has(theme) ? theme : "atlas";
    map.dataset.mapTheme = chosen;
    map.querySelectorAll("[data-map-theme-choice]").forEach((button) => {
      button.setAttribute("aria-pressed", String(button.dataset.mapThemeChoice === chosen));
    });
    try { storage?.setItem(THEME_KEY, chosen); } catch (_) { /* storage may be disabled */ }
    return chosen;
  };

  const initializeMap = (map, storage = globalThis.localStorage) => {
    if (map.dataset.mapReady === "true") return;
    map.dataset.mapReady = "true";
    const svg = map.querySelector("[data-map-svg]");
    if (!svg) return;
    const initial = parseViewBox(svg);
    scalePins(svg, initial[2]);
    let stored = "atlas";
    try { stored = storage?.getItem(THEME_KEY) || "atlas"; } catch (_) { /* use default */ }
    const initialTheme = applyTheme(map, stored, storage);
    renderTiles(map, svg, initial, initialTheme);

    const updateView = (view) => {
      writeViewBox(svg, view);
      renderTiles(map, svg, view);
    };
    const zoom = (factor) => updateView(zoomedView(parseViewBox(svg), factor));
    map.addEventListener("click", (event) => {
      const theme = event.target.closest?.("[data-map-theme-choice]");
      if (theme) {
        const chosen = applyTheme(map, theme.dataset.mapThemeChoice, storage);
        renderTiles(map, svg, parseViewBox(svg), chosen);
      }
      const control = event.target.closest?.("[data-map-zoom]");
      if (control) zoom(control.dataset.mapZoom === "in" ? 0.8 : 1.25);
      if (event.target.closest?.("[data-map-reset]")) updateView(initial);
    });
    svg.addEventListener("wheel", (event) => { event.preventDefault(); zoom(event.deltaY < 0 ? 0.85 : 1.18); }, { passive: false });
    svg.addEventListener("keydown", (event) => {
      const [, , width, height] = parseViewBox(svg);
      const amountX = width * 0.08, amountY = height * 0.08;
      const moves = { ArrowLeft: [-amountX, 0], ArrowRight: [amountX, 0], ArrowUp: [0, -amountY], ArrowDown: [0, amountY] };
      if (moves[event.key]) { event.preventDefault(); updateView(pannedView(parseViewBox(svg), ...moves[event.key])); }
      if (event.key === "+" || event.key === "=") { event.preventDefault(); zoom(0.8); }
      if (event.key === "-") { event.preventDefault(); zoom(1.25); }
      if (event.key === "Home") { event.preventDefault(); updateView(initial); }
    });

    let drag = null;
    svg.addEventListener("pointerdown", (event) => { if (!event.target.closest?.("a")) { drag = { x: event.clientX, y: event.clientY, view: parseViewBox(svg) }; svg.setPointerCapture?.(event.pointerId); } });
    svg.addEventListener("pointermove", (event) => {
      if (!drag) return;
      const [, , width, height] = drag.view;
      const rect = svg.getBoundingClientRect();
      updateView(pannedView(drag.view, -(event.clientX - drag.x) * width / rect.width, -(event.clientY - drag.y) * height / rect.height));
    });
    const endDrag = () => { drag = null; };
    svg.addEventListener("pointerup", endDrag);
    svg.addEventListener("pointercancel", endDrag);
  };

  const initializeStrategicMaps = (root = document) => root.querySelectorAll("[data-strategic-map]").forEach((map) => initializeMap(map));
  globalThis.StrategicMap = { applyTheme, initializeMap, parseViewBox, pannedView, renderTiles, tileZoom, visibleTileRange, zoomedView };
  initializeStrategicMaps();
  document.addEventListener("strategic-live-regions-refreshed", () => initializeStrategicMaps());
})();
