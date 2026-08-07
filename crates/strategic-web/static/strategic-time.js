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
      [0, [3, 6, 16], [8, 13, 29], 0.98, 22, 0],
      [5, [15, 18, 35], [28, 31, 48], 0.78, 26, 0.08],
      [6, [111, 48, 38], [48, 56, 83], 0.2, 42, 0.78],
      [7, [219, 106, 57], [79, 105, 146], 0.02, 62, 1],
      [9, [73, 143, 204], [30, 91, 166], 0, 75, 0.18],
      [12, [84, 166, 230], [35, 108, 194], 0, 82, 0.06],
      [15, [69, 145, 210], [29, 91, 170], 0, 78, 0.12],
      [17, [78, 116, 168], [34, 69, 126], 0.02, 68, 0.45],
      [18, [218, 102, 55], [101, 57, 89], 0.18, 52, 1],
      [19, [105, 40, 42], [39, 32, 57], 0.68, 34, 0.68],
      [21, [7, 10, 23], [11, 16, 35], 0.98, 24, 0.05],
      [24, [3, 6, 16], [8, 13, 29], 0.98, 22, 0],
    ];
    const right = stops.findIndex(([at]) => at >= hour);
    const left = Math.max(0, right - 1);
    const span = stops[right][0] - stops[left][0] || 1;
    const amount = (hour - stops[left][0]) / span;
    const daylight = hour >= 6 && hour < 18;
    const progress = daylight ? (hour - 6) / 12 : ((hour + 6) % 24) / 12;
    const arc = progress * 2 - 1;
    const glowX = 50 + 56 * Math.sign(arc) * Math.abs(arc) ** 1.45;
    const glowY = 10 + 54 * arc * arc;
    const twilight = Math.min(Math.abs(hour - 6), Math.abs(hour - 18));
    const glow = daylight
      ? (twilight < 2 ? "rgb(255 169 94 / 76%)" : "rgb(255 244 194 / 82%)")
      : "rgb(181 207 255 / 42%)";
    const building = stops[left][4] + (stops[right][4] - stops[left][4]) * amount;
    const light = Math.max(0, Math.min(1, (building - 22) / 60));
    const warmth = stops[left][5] + (stops[right][5] - stops[left][5]) * amount;
    return {
      low: rgb(mix(stops[left][1], stops[right][1], amount)),
      high: rgb(mix(stops[left][2], stops[right][2], amount)),
      stars: stops[left][3] + (stops[right][3] - stops[left][3]) * amount,
      building: Math.round(building),
      light,
      warmth,
      glowX,
      glowY,
      glow,
    };
  };

  const applyLighting = (minutes) => {
    const value = lighting(minutes);
    const root = document.documentElement.style;
    root.setProperty("--sky-low", value.low);
    root.setProperty("--sky-high", value.high);
    root.setProperty("--sky-glow", value.glow);
    root.setProperty("--sky-glow-x", `${value.glowX}%`);
    root.setProperty("--sky-glow-y", `${value.glowY}%`);
    root.setProperty("--star-opacity", value.stars.toFixed(2));
    root.setProperty("--building-light", `${value.building}%`);
    root.setProperty("--environment-light", value.light.toFixed(3));
    root.setProperty("--environment-warmth", value.warmth.toFixed(3));
    root.setProperty("--environment-tint", value.high);
    root.setProperty("--map-light", (0.62 + value.light * 0.38).toFixed(3));
    root.setProperty("--map-saturation", (0.7 + value.light * 0.3).toFixed(3));
    root.setProperty("--map-atmosphere-opacity", (0.3 - value.light * 0.22 + value.warmth * 0.05).toFixed(3));
    root.setProperty("--scene-atmosphere-opacity", ((0.3 - value.light * 0.22 + value.warmth * 0.05) * 0.65).toFixed(3));
    root.setProperty("--map-surface-mix", `${(62 + value.light * 38).toFixed(1)}%`);
    root.setProperty("--map-land-mix", `${(70 + value.light * 30).toFixed(1)}%`);
  };

  window.strategicTimeLighting = lighting;
  window.strategicTimeApplyLighting = applyLighting;

  const refreshTime = () => window.queueStrategicInitialLoad(() => window.strategicBackgroundFetch("strategic-time", "/time"))
    .then((response) => response.json())
    .then(({ character_minutes: characterMinutes, official_minutes: officialMinutes }) => {
      window.strategicCharacterMinutes = characterMinutes;
      const characterTime = format(characterMinutes);
      const officialTime = format(officialMinutes);
      document.querySelectorAll("[data-player-time]").forEach((element) => {
        element.textContent = characterTime;
        element.setAttribute("aria-label", characterTime);
        element.title = `Official time: ${officialTime}`;
      });
      applyLighting(characterMinutes);
      document.dispatchEvent(new CustomEvent("strategic-time-ready", {
        detail: { characterMinutes, officialMinutes },
      }));
    })
    .catch((error) => window.reportStrategicError(error, "strategic time"));
  refreshTime();
  document.addEventListener("strategic-page-mounted", refreshTime);
})();
