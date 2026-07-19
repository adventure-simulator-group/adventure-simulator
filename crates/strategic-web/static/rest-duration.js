(() => {
  const DAY_MINUTES = 1440;

  function normalizeMinute(value) {
    return ((Math.round(Number(value)) % DAY_MINUTES) + DAY_MINUTES) % DAY_MINUTES;
  }

  function minutesUntilWake(currentMinutes, targetMinute) {
    const currentTod = normalizeMinute(currentMinutes);
    return DAY_MINUTES + ((normalizeMinute(targetMinute) - currentTod + DAY_MINUTES) % DAY_MINUTES);
  }

  function targetForDuration(currentMinutes, durationMinutes) {
    return normalizeMinute(Number(currentMinutes) + Number(durationMinutes));
  }

  function formatClock(minutes) {
    const value = normalizeMinute(minutes);
    return `${String(Math.floor(value / 60)).padStart(2, "0")}:${String(value % 60).padStart(2, "0")}`;
  }

  function formatDuration(minutes) {
    const value = Math.max(0, Math.round(Number(minutes)));
    return `${String(Math.floor(value / 60)).padStart(2, "0")}:${String(value % 60).padStart(2, "0")}`;
  }

  function parseDuration(value) {
    const match = String(value).trim().match(/^(\d+):([0-5]\d)$/);
    if (!match) return null;
    const hours = Number(match[1]);
    const minutes = Number(match[2]);
    if (!Number.isSafeInteger(hours)) return null;
    return hours * 60 + minutes;
  }

  if (typeof module !== "undefined") module.exports = {
    formatClock,
    formatDuration,
    minutesUntilWake,
    normalizeMinute,
    parseDuration,
    targetForDuration,
  };
  if (typeof document === "undefined") return;

  const mounted = new WeakSet();
  const dirty = new WeakSet();
  const states = new WeakMap();

  const mount = (control) => {
    if (mounted.has(control)) return;
    mounted.add(control);
    const form = control.closest("form");
    const duration = control.querySelector("[data-rest-duration-input]");
    const exact = control.querySelector("[data-rest-exact-minutes]");
    const slider = control.querySelector("[data-wake-time-slider]");
    const output = control.querySelector("[data-wake-time-output]");
    const panel = control.querySelector("[data-wake-time-panel]");
    const submit = form.querySelector("[data-rest-submit]");
    const radios = [...control.querySelectorAll("input[type=radio][name=unit]")];
    let characterMinutes = Number.isFinite(Number(window.strategicCharacterMinutes))
      ? Number(window.strategicCharacterMinutes)
      : null;
    const initiallyHours = radios.find((radio) => radio.checked)?.value === "hours";
    let daysValue = initiallyHours ? 1 : Math.max(1, Math.round(Number(duration.value) || 1));

    const selectedUnit = () => radios.find((radio) => radio.checked)?.value || "hours";
    const setTarget = (minute) => {
      slider.step = "1";
      slider.value = normalizeMinute(minute);
      const clock = formatClock(slider.value);
      output.value = clock;
      output.textContent = clock;
      slider.setAttribute("aria-valuetext", clock);
    };
    const setHoursMinutes = (minutes) => {
      const hoursMinutes = Math.max(DAY_MINUTES, Math.round(minutes));
      duration.value = formatDuration(hoursMinutes);
      exact.value = String(hoursMinutes);
      submit.disabled = false;
    };
    const applyUnit = () => {
      const hours = selectedUnit() === "hours";
      radios.forEach((radio) => radio.closest("label").classList.toggle("active", radio.checked));
      panel.setAttribute("aria-disabled", String(!hours));
      slider.disabled = !hours;
      exact.disabled = !hours;
      duration.type = hours ? "text" : "number";
      duration.inputMode = hours ? "text" : "numeric";
      duration.pattern = hours ? "[0-9]+:[0-5][0-9]" : "";
      duration.min = hours ? "" : "1";
      duration.max = hours ? "" : "365";
      duration.step = hours ? "" : "1";
      control.querySelector("[data-rest-unit-label]").textContent = hours ? "hours" : "days";
      if (hours) {
        if (characterMinutes === null) {
          duration.value = "24:00";
          exact.value = "";
          submit.disabled = true;
        } else {
          setHoursMinutes(minutesUntilWake(characterMinutes, slider.value));
        }
      } else {
        duration.value = String(daysValue);
        exact.value = "";
        submit.disabled = daysValue < 1;
      }
    };

    setTarget(480);
    slider.addEventListener("input", () => {
      dirty.add(control);
      const clock = formatClock(slider.value);
      output.value = clock;
      output.textContent = clock;
      slider.setAttribute("aria-valuetext", clock);
      if (characterMinutes !== null) setHoursMinutes(minutesUntilWake(characterMinutes, slider.value));
    });
    slider.addEventListener("pointerdown", () => { slider.step = "60"; });
    slider.addEventListener("keydown", (event) => {
      const direction = ["ArrowRight", "ArrowUp"].includes(event.key) ? 1
        : ["ArrowLeft", "ArrowDown"].includes(event.key) ? -1 : 0;
      if (!direction) return;
      event.preventDefault();
      const current = Number(slider.value);
      const next = direction > 0
        ? Math.floor(current / 60) * 60 + 60
        : Math.ceil(current / 60) * 60 - 60;
      slider.step = "1";
      slider.value = Math.min(1_380, Math.max(0, next));
      slider.dispatchEvent(new Event("input", { bubbles: true }));
    });
    duration.addEventListener("input", () => {
      dirty.add(control);
      if (selectedUnit() === "days") {
        const value = Number(duration.value);
        daysValue = Math.round(value);
        exact.value = "";
        submit.disabled = !Number.isFinite(value) || value < 1 || value > 365 || value !== daysValue;
        return;
      }
      const durationMinutes = parseDuration(duration.value);
      if (characterMinutes === null || durationMinutes === null
        || durationMinutes < DAY_MINUTES || durationMinutes > 365 * DAY_MINUTES) {
        exact.value = "";
        submit.disabled = true;
        return;
      }
      exact.value = String(durationMinutes);
      submit.disabled = false;
      setTarget(targetForDuration(characterMinutes, durationMinutes));
    });
    duration.addEventListener("change", () => {
      if (selectedUnit() === "hours") {
        const durationMinutes = parseDuration(duration.value);
        duration.value = formatDuration(durationMinutes === null ? DAY_MINUTES : Math.max(DAY_MINUTES, durationMinutes));
      }
      if (selectedUnit() === "days" && Number(duration.value) < 1) duration.value = "1";
      duration.dispatchEvent(new Event("input", { bubbles: true }));
    });
    radios.forEach((radio) => radio.addEventListener("change", () => {
      dirty.add(control);
      applyUnit();
    }));
    control.querySelectorAll("[data-rest-step]").forEach((button) => button.addEventListener("click", () => {
      dirty.add(control);
      if (selectedUnit() === "hours") {
        const current = parseDuration(duration.value) ?? DAY_MINUTES;
        const next = Math.min(365 * DAY_MINUTES, Math.max(DAY_MINUTES, current + Number(button.dataset.restStep) * 60));
        duration.value = formatDuration(next);
      } else {
        duration.value = String(Number(duration.value || duration.min) + Number(button.dataset.restStep));
      }
      duration.dispatchEvent(new Event("change", { bubbles: true }));
    }));
    states.set(control, {
      syncTime(minutes) {
        characterMinutes = Number(minutes);
        if (selectedUnit() === "hours") setHoursMinutes(minutesUntilWake(characterMinutes, slider.value));
      },
    });
    applyUnit();
  };

  const mountAll = (root = document) => root.querySelectorAll?.("[data-wake-time]").forEach(mount);
  const isDirty = (root = document) => [...root.querySelectorAll?.("[data-wake-time]") || []]
    .some((control) => dirty.has(control));
  const syncTime = (root, minutes) => {
    root.querySelectorAll?.("[data-wake-time]").forEach((control) => states.get(control)?.syncTime(minutes));
  };

  window.strategicRestDuration = { isDirty, mountAll };
  mountAll();
  document.addEventListener("strategic-time-ready", (event) => {
    mountAll();
    syncTime(document, event.detail.characterMinutes);
  });
  document.addEventListener("strategic-live-regions-refreshed", () => mountAll());
})();
