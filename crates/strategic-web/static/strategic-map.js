(() => {
  "use strict";

  const PIN_REFERENCE_WIDTH = 390;
  const MIN_VIEW_WIDTH = 10;
  const MIN_VIEW_HEIGHT = MIN_VIEW_WIDTH / 1.5;
  const SVG_NS = "http://www.w3.org/2000/svg";
  const parseViewBox = (svg) => svg.getAttribute("viewBox").trim().split(/\s+/).map(Number);
  const scalePins = (svg, width) => {
    const scale = width / PIN_REFERENCE_WIDTH;
    svg.querySelectorAll("[data-map-pin-symbol]").forEach((symbol) => {
      symbol.setAttribute("transform", `scale(${scale.toFixed(5)})`);
    });
  };
  const labelPriorityThreshold = (viewWidth) => {
    if (viewWidth > 700) return 80;
    if (viewWidth > 400) return 70;
    if (viewWidth > 250) return 60;
    if (viewWidth > 140) return 50;
    if (viewWidth > 70) return 40;
    return 20;
  };
  const boxesOverlap = (a, b, padding = 3) => !(
    a.right + padding <= b.left || a.left >= b.right + padding
    || a.bottom + padding <= b.top || a.top >= b.bottom + padding
  );
  const layoutLabels = (svg, [viewX, viewY, viewWidth, viewHeight]) => {
    const labels = [...svg.querySelectorAll("[data-map-label]")];
    if (!labels.length) return;
    const rect = svg.getBoundingClientRect();
    const pixelWidth = rect.width || 1200;
    const pixelHeight = rect.height || pixelWidth / 1.5;
    const worldPerPixelX = viewWidth / pixelWidth;
    const worldPerPixelY = viewHeight / pixelHeight;
    const threshold = labelPriorityThreshold(viewWidth);
    const candidates = labels.map((label) => {
      const x = Number(label.dataset.mapX);
      const y = Number(label.dataset.mapY);
      const priority = Number(label.dataset.mapLabelPriority);
      const width = Number(label.dataset.mapLabelWidth) || 80;
      return {
        label, x, y, priority, width,
        essential: label.dataset.mapLabelEssential === "true",
        screenX: (x - viewX) / viewWidth * pixelWidth,
        screenY: (y - viewY) / viewHeight * pixelHeight,
      };
    }).filter(({ x, y, priority, essential }) => Number.isFinite(x) && Number.isFinite(y)
      && (essential || priority >= threshold)
      && x >= viewX && x <= viewX + viewWidth && y >= viewY && y <= viewY + viewHeight)
      .sort((a, b) => b.priority - a.priority || a.y - b.y || a.x - b.x);
    const placed = [];
    const visible = new Set();
    const labelGap = 22;
    for (const candidate of candidates) {
      const preferLeft = candidate.screenX + candidate.width + 14 > pixelWidth;
      let placement = null;
      for (const left of preferLeft ? [true, false] : [false, true]) {
        const box = left
          ? { left: candidate.screenX - candidate.width - labelGap, right: candidate.screenX - labelGap, top: candidate.screenY - 15, bottom: candidate.screenY + 3 }
          : { left: candidate.screenX + labelGap, right: candidate.screenX + candidate.width + labelGap, top: candidate.screenY - 15, bottom: candidate.screenY + 3 };
        if (box.left < 2 || box.right > pixelWidth - 2 || box.top < 2 || box.bottom > pixelHeight - 2) continue;
        if (!placed.some((other) => boxesOverlap(box, other))) { placement = { left, box }; break; }
      }
      if (!placement && candidate.essential) {
        const left = preferLeft;
        placement = { left, box: left
          ? { left: candidate.screenX - candidate.width - labelGap, right: candidate.screenX - labelGap, top: candidate.screenY - 15, bottom: candidate.screenY + 3 }
          : { left: candidate.screenX + labelGap, right: candidate.screenX + candidate.width + labelGap, top: candidate.screenY - 15, bottom: candidate.screenY + 3 } };
      }
      if (!placement) continue;
      candidate.label.setAttribute("transform", `translate(${((placement.left ? -labelGap : labelGap) * worldPerPixelX).toFixed(3)} ${(-3 * worldPerPixelY).toFixed(3)})`);
      candidate.label.style.setProperty("--map-label-font-size", `${(12 * worldPerPixelY).toFixed(3)}px`);
      candidate.label.style.setProperty("--map-label-stroke-width", `${(3.5 * worldPerPixelY).toFixed(3)}px`);
      candidate.label.querySelector("text")?.setAttribute("text-anchor", placement.left ? "end" : "start");
      placed.push(placement.box);
      visible.add(candidate.label);
    }
    labels.forEach((label) => label.setAttribute("display", visible.has(label) ? "inline" : "none"));
  };
  const writeViewBox = (svg, view) => {
    svg.setAttribute("viewBox", view.map((value) => value.toFixed(2)).join(" "));
    scalePins(svg, view[2]);
    layoutLabels(svg, view);
  };
  const tileZoom = (viewWidth, pixelWidth, pixelRatio, maxZoom) => {
    const density = Math.max(1, pixelWidth * pixelRatio / viewWidth);
    return Math.min(maxZoom, Math.max(0, Math.ceil(Math.log2(density))));
  };
  const visibleTileRange = ([x, y, width, height], tileSize, zoom) => {
    const span = tileSize / 2 ** zoom;
    const maxX = Math.ceil(1200 / span) - 1;
    const maxY = Math.ceil(800 / span) - 1;
    return {
      span,
      minX: Math.max(0, Math.floor(x / span)),
      maxX: Math.min(maxX, Math.ceil((x + width) / span) - 1),
      minY: Math.max(0, Math.floor(y / span)),
      maxY: Math.min(maxY, Math.ceil((y + height) / span) - 1),
    };
  };
  const renderTiles = (map, svg, view, theme = map.dataset.mapTheme) => {
    const layer = svg.querySelector("[data-map-tile-layer]");
    if (!layer) return;
    const tileSize = Number(map.dataset.mapTileSize);
    const maxZoom = Number(map.dataset.mapMaxTileZoom);
    const gutter = Number(map.dataset.mapTileGutter || 0);
    const version = map.dataset.mapTileVersion;
    const root = map.dataset.mapTileRoot;
    if (!tileSize || !Number.isFinite(maxZoom) || !version || !root) return;
    const rect = svg.getBoundingClientRect();
    const zoom = tileZoom(view[2], rect.width || 768, globalThis.devicePixelRatio || 1, maxZoom);
    const range = visibleTileRange(view, tileSize, zoom);
    const tileGutter = gutter / 2 ** zoom;
    const key = [theme, zoom, range.minX, range.maxX, range.minY, range.maxY].join(":");
    if (layer.dataset.tileKey === key) return;
    const tiles = [];
    for (let y = range.minY; y <= range.maxY; y += 1) {
      for (let x = range.minX; x <= range.maxX; x += 1) {
        const image = document.createElementNS(SVG_NS, "image");
        image.setAttribute("x", (x * range.span - tileGutter).toFixed(3));
        image.setAttribute("y", (y * range.span - tileGutter).toFixed(3));
        image.setAttribute("width", (range.span + 2 * tileGutter).toFixed(3));
        image.setAttribute("height", (range.span + 2 * tileGutter).toFixed(3));
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

  const initializeMap = (map) => {
    if (map.dataset.mapReady === "true") return;
    map.dataset.mapReady = "true";
    const svg = map.querySelector("[data-map-svg]");
    if (!svg) return;
    const initial = parseViewBox(svg);
    scalePins(svg, initial[2]);
    layoutLabels(svg, initial);
    renderTiles(map, svg, initial, "paper");

    const updateView = (view) => {
      writeViewBox(svg, view);
      renderTiles(map, svg, view);
    };
    const zoom = (factor) => updateView(zoomedView(parseViewBox(svg), factor));
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

    const pointers = new Map();
    let drag = null, pinch = null;
    const pointerMidpoint = ([first, second]) => ({ x: (first.x + second.x) / 2, y: (first.y + second.y) / 2 });
    const pointerDistance = ([first, second]) => Math.hypot(second.x - first.x, second.y - first.y);
    svg.addEventListener("pointerdown", (event) => {
      if (event.target.closest?.("a")) return;
      pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
      svg.setPointerCapture?.(event.pointerId);
      if (pointers.size === 1) {
        drag = { x: event.clientX, y: event.clientY, view: parseViewBox(svg) };
      } else if (pointers.size === 2) {
        const points = [...pointers.values()];
        pinch = { midpoint: pointerMidpoint(points), distance: pointerDistance(points), view: parseViewBox(svg) };
        drag = null;
      }
    });
    svg.addEventListener("pointermove", (event) => {
      if (!pointers.has(event.pointerId)) return;
      pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
      if (pinch && pointers.size >= 2) {
        const points = [...pointers.values()].slice(0, 2);
        const midpoint = pointerMidpoint(points);
        const distance = pointerDistance(points);
        if (!distance || !pinch.distance) return;
        const rect = svg.getBoundingClientRect();
        const [x, y, width, height] = pinch.view;
        const focusX = x + (pinch.midpoint.x - rect.left) / rect.width * width;
        const focusY = y + (pinch.midpoint.y - rect.top) / rect.height * height;
        const next = zoomedView(pinch.view, pinch.distance / distance, focusX, focusY);
        updateView(pannedView(next, -(midpoint.x - pinch.midpoint.x) * next[2] / rect.width, -(midpoint.y - pinch.midpoint.y) * next[3] / rect.height));
        return;
      }
      if (!drag) return;
      const [, , width, height] = drag.view;
      const rect = svg.getBoundingClientRect();
      updateView(pannedView(drag.view, -(event.clientX - drag.x) * width / rect.width, -(event.clientY - drag.y) * height / rect.height));
    });
    const endPointer = (event) => {
      pointers.delete(event.pointerId);
      pinch = null;
      const remaining = [...pointers.values()][0];
      drag = remaining ? { x: remaining.x, y: remaining.y, view: parseViewBox(svg) } : null;
    };
    svg.addEventListener("pointerup", endPointer);
    svg.addEventListener("pointercancel", endPointer);
  };

  const initializeStrategicMaps = (root = document) => root.querySelectorAll("[data-strategic-map]").forEach((map) => initializeMap(map));
  globalThis.StrategicMap = { boxesOverlap, initializeMap, labelPriorityThreshold, layoutLabels, parseViewBox, pannedView, renderTiles, tileZoom, visibleTileRange, zoomedView };
  initializeStrategicMaps();
  document.addEventListener("strategic-live-regions-refreshed", () => initializeStrategicMaps());
})();
