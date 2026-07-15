(() => {
  const GAME_MINUTE_NUMERATOR = 73;
  const GAME_MINUTE_DENOMINATOR_MS = 84_000;

  const format = (minutes) => {
    const day = Math.floor(minutes / 1440) % 365 + 1;
    const hour = Math.floor(minutes / 60) % 24;
    const minute = minutes % 60;
    return `Day ${day} · ${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
  };

  window.queueStrategicInitialLoad(() => window.strategicBackgroundFetch("strategic-time", "/time"))
    .then((response) => response.json())
    .then(({ character_minutes: characterMinutes, official_minutes: officialMinutes }) => {
      const initializedAt = Date.now();
      let lastElapsedMinutes = -1;
      const render = () => {
        const elapsedMinutes = Math.floor(
          ((Date.now() - initializedAt) * GAME_MINUTE_NUMERATOR) / GAME_MINUTE_DENOMINATOR_MS,
        );
        if (elapsedMinutes === lastElapsedMinutes) return;
        lastElapsedMinutes = elapsedMinutes;
        document.querySelectorAll("[data-player-time]").forEach((element) => {
          element.textContent = format(characterMinutes + elapsedMinutes);
          element.title = `Official time: ${format(officialMinutes + elapsedMinutes)}`;
        });
      };
      render();
      window.setInterval(render, 1_000);
    })
    .catch(() => {});
})();
