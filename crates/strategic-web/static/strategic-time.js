(() => {
  const format = (minutes) => {
    const day = Math.floor(minutes / 1440) % 365 + 1;
    const hour = Math.floor(minutes / 60) % 24;
    const minute = minutes % 60;
    return `Day ${day} · ${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
  };

  const mix = (a, b, amount) => a.map((value, index) => Math.round(value + (b[index] - value) * amount));
  const rgb = (value) => `rgb(${value.join(" ")})`;
  const lighting = (minutes) => {
    const hour = (minutes % 1440) / 60;
    const stops = [
      [0, [3, 5, 13], [7, 10, 22], 0.98, 22],
      [5, [8, 10, 20], [19, 20, 31], 0.8, 25],
      [7, [50, 28, 23], [25, 35, 52], 0.05, 62],
      [12, [18, 43, 72], [8, 24, 48], 0, 78],
      [17, [39, 33, 42], [15, 27, 46], 0.05, 72],
      [19, [48, 20, 18], [17, 18, 31], 0.65, 38],
      [21, [5, 7, 16], [8, 11, 24], 0.98, 24],
      [24, [3, 5, 13], [7, 10, 22], 0.98, 22],
    ];
    const right = stops.findIndex(([at]) => at >= hour);
    const left = Math.max(0, right - 1);
    const span = stops[right][0] - stops[left][0] || 1;
    const amount = (hour - stops[left][0]) / span;
    const sunProgress = Math.min(1, Math.max(0, (hour - 6) / 12));
    return {
      low: rgb(mix(stops[left][1], stops[right][1], amount)),
      high: rgb(mix(stops[left][2], stops[right][2], amount)),
      stars: stops[left][3] + (stops[right][3] - stops[left][3]) * amount,
      building: Math.round(stops[left][4] + (stops[right][4] - stops[left][4]) * amount),
      glowX: Math.round(8 + sunProgress * 84),
      glow: hour >= 6 && hour < 18 ? "rgb(143 103 54 / 45%)" : "rgb(112 132 170 / 24%)",
    };
  };

  const applyLighting = (minutes) => {
    const value = lighting(minutes);
    const root = document.documentElement.style;
    root.setProperty("--sky-low", value.low);
    root.setProperty("--sky-high", value.high);
    root.setProperty("--sky-glow", value.glow);
    root.setProperty("--sky-glow-x", `${value.glowX}%`);
    root.setProperty("--star-opacity", value.stars.toFixed(2));
    root.setProperty("--building-light", `${value.building}%`);
  };

  window.strategicTimeLighting = lighting;

  window.queueStrategicInitialLoad(() => window.strategicBackgroundFetch("strategic-time", "/time"))
    .then((response) => response.json())
    .then(({ character_minutes: characterMinutes, official_minutes: officialMinutes }) => {
      document.querySelectorAll("[data-player-time]").forEach((element) => {
        element.textContent = format(characterMinutes);
        element.title = `Official time: ${format(officialMinutes)}`;
      });
      applyLighting(characterMinutes);
    })
    .catch((error) => window.reportStrategicError(error, "strategic time"));
})();
