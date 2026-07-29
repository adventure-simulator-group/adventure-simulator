(() => {
  let generation = 0;

  const percentage = (value) => `${Math.round(Math.max(0, value) * 100)}%`;

  const renderWheel = (wheel, condition) => {
    if (!condition) {
      wheel.style.background = "transparent";
      wheel.title = "Strategic condition unavailable";
      wheel.setAttribute("aria-label", wheel.title);
      return;
    }

    const styles = getComputedStyle(wheel);
    const color = (name, fallback) => styles.getPropertyValue(`--incap-${name}`).trim() || fallback;
    const components = [
      ["pain", condition.pain, color("pain", "#d973a2")],
      ["blood loss", condition.blood_loss, color("blood", "#c84747")],
      ["fear", condition.fear, color("fear", "#4f83cc")],
      ["fatigue", condition.fatigue, color("fatigue", "#202020")],
      ["hunger", condition.hunger, color("hunger", "#b57a35")],
      ["thirst", condition.thirst, color("thirst", "#3f9fa8")],
      ["temperature", condition.thermal, color("thermal", "#7d8ee8")],
    ];
    let cursor = 0;
    const stops = [];
    for (const [, rawValue, color] of components) {
      const start = cursor;
      cursor = Math.min(360, cursor + Math.max(0, rawValue) * 360);
      if (cursor > start) stops.push(`${color} ${start}deg ${cursor}deg`);
    }
    if (cursor < 360) stops.push(`transparent ${cursor}deg 360deg`);
    wheel.style.background = stops.length
      ? `conic-gradient(from -90deg, ${stops.join(", ")})`
      : "transparent";
    wheel.dataset.conditionStatus = condition.status;
    const detail = components.map(([name, value]) => `${name} ${percentage(value)}`).join(", ");
    wheel.title = `${condition.status}: ${percentage(condition.incapacitation)} incapacitation; morale ${condition.morale.toFixed(1)}; ${detail}`;
    wheel.setAttribute("aria-label", wheel.title);
  };

  const refresh = async () => {
    const currentGeneration = ++generation;
    const wheels = [...document.querySelectorAll("[data-strategic-condition-wheel]")];
    const ids = [...new Set(wheels.map((wheel) => wheel.dataset.strategicConditionWheel))];
    const entries = await Promise.all(ids.map(async (id) => {
      const response = await window.strategicBackgroundFetch(
        `strategic-condition-${id}`,
        `/api/characters/${encodeURIComponent(id)}/condition`,
        { headers: { Accept: "application/json" } },
      );
      return [id, response.ok ? await response.json() : null];
    }));
    if (currentGeneration !== generation) return;
    const conditions = new Map(entries);
    wheels.forEach((wheel) => renderWheel(
      wheel,
      conditions.get(wheel.dataset.strategicConditionWheel),
    ));
  };

  const schedule = () => window.setTimeout(() => {
    refresh().catch((error) => window.reportStrategicError(error, "strategic condition"));
  }, 0);

  document.addEventListener("DOMContentLoaded", schedule);
  document.addEventListener("strategic-live-update", schedule);
  document.addEventListener("strategic-live-regions-refreshed", schedule);
})();
