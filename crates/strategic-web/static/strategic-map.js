(() => {
  "use strict";

  const THEME_KEY = "adventuresim.map-theme";
  const THEMES = new Set(["paper", "atlas"]);
  const parseViewBox = (svg) => svg.getAttribute("viewBox").trim().split(/\s+/).map(Number);
  const writeViewBox = (svg, view) => svg.setAttribute("viewBox", view.map((value) => value.toFixed(2)).join(" "));
  const zoomedView = ([x, y, width, height], factor, focusX = x + width / 2, focusY = y + height / 2) => {
    const nextWidth = Math.min(1200, Math.max(80, width * factor));
    const nextHeight = Math.min(800, Math.max(53.33, height * factor));
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
    let stored = "atlas";
    try { stored = storage?.getItem(THEME_KEY) || "atlas"; } catch (_) { /* use default */ }
    applyTheme(map, stored, storage);

    const zoom = (factor) => writeViewBox(svg, zoomedView(parseViewBox(svg), factor));
    map.addEventListener("click", (event) => {
      const theme = event.target.closest?.("[data-map-theme-choice]");
      if (theme) applyTheme(map, theme.dataset.mapThemeChoice, storage);
      const control = event.target.closest?.("[data-map-zoom]");
      if (control) zoom(control.dataset.mapZoom === "in" ? 0.8 : 1.25);
      if (event.target.closest?.("[data-map-reset]")) writeViewBox(svg, initial);
    });
    svg.addEventListener("wheel", (event) => { event.preventDefault(); zoom(event.deltaY < 0 ? 0.85 : 1.18); }, { passive: false });
    svg.addEventListener("keydown", (event) => {
      const [, , width, height] = parseViewBox(svg);
      const amountX = width * 0.08, amountY = height * 0.08;
      const moves = { ArrowLeft: [-amountX, 0], ArrowRight: [amountX, 0], ArrowUp: [0, -amountY], ArrowDown: [0, amountY] };
      if (moves[event.key]) { event.preventDefault(); writeViewBox(svg, pannedView(parseViewBox(svg), ...moves[event.key])); }
      if (event.key === "+" || event.key === "=") { event.preventDefault(); zoom(0.8); }
      if (event.key === "-") { event.preventDefault(); zoom(1.25); }
      if (event.key === "Home") { event.preventDefault(); writeViewBox(svg, initial); }
    });

    let drag = null;
    svg.addEventListener("pointerdown", (event) => { if (!event.target.closest?.("a")) { drag = { x: event.clientX, y: event.clientY, view: parseViewBox(svg) }; svg.setPointerCapture?.(event.pointerId); } });
    svg.addEventListener("pointermove", (event) => {
      if (!drag) return;
      const [, , width, height] = drag.view;
      const rect = svg.getBoundingClientRect();
      writeViewBox(svg, pannedView(drag.view, -(event.clientX - drag.x) * width / rect.width, -(event.clientY - drag.y) * height / rect.height));
    });
    const endDrag = () => { drag = null; };
    svg.addEventListener("pointerup", endDrag);
    svg.addEventListener("pointercancel", endDrag);
  };

  const initializeStrategicMaps = (root = document) => root.querySelectorAll("[data-strategic-map]").forEach((map) => initializeMap(map));
  globalThis.StrategicMap = { applyTheme, initializeMap, parseViewBox, pannedView, zoomedView };
  initializeStrategicMaps();
  document.addEventListener("strategic-live-regions-refreshed", () => initializeStrategicMaps());
})();
