(() => {
  const DAY_MINUTES = 1440;

  function normalizeMinute(value) {
    return ((Math.round(Number(value)) % DAY_MINUTES) + DAY_MINUTES) % DAY_MINUTES;
  }

  function minutesUntilWake(currentMinutes, targetMinute) {
    const currentTod = normalizeMinute(currentMinutes);
    return DAY_MINUTES + ((normalizeMinute(targetMinute) - currentTod + DAY_MINUTES) % DAY_MINUTES);
  }

  function targetForDuration(currentMinutes, durationHours) {
    const durationMinutes = Math.round(Number(durationHours) * 60);
    return normalizeMinute(Number(currentMinutes) + durationMinutes);
  }

  function formatClock(minutes) {
    const value = normalizeMinute(minutes);
    return `${String(Math.floor(value / 60)).padStart(2, "0")}:${String(value % 60).padStart(2, "0")}`;
  }

  function formatHours(minutes) {
    const hours = minutes / 60;
    return Number.isInteger(hours) ? String(hours) : String(Number(hours.toFixed(2)));
  }

  if (typeof module !== "undefined") module.exports = {
    formatClock,
    minutesUntilWake,
    normalizeMinute,
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
      slider.value = normalizeMinute(minute);
      const clock = formatClock(slider.value);
      output.value = clock;
      output.textContent = clock;
      slider.setAttribute("aria-valuetext", clock);
    };
    const setHoursMinutes = (minutes) => {
      const hoursMinutes = Math.max(DAY_MINUTES, Math.round(minutes));
      duration.value = formatHours(hoursMinutes);
      exact.value = String(hoursMinutes);
      submit.disabled = false;
    };
    const applyUnit = () => {
      const hours = selectedUnit() === "hours";
      radios.forEach((radio) => radio.closest("label").classList.toggle("active", radio.checked));
      panel.setAttribute("aria-disabled", String(!hours));
      slider.disabled = !hours;
      exact.disabled = !hours;
      duration.min = hours ? "24" : "1";
      duration.max = hours ? "8760" : "365";
      duration.step = hours ? "0.01" : "1";
      control.querySelector("[data-rest-unit-label]").textContent = hours ? "hours" : "days";
      if (hours) {
        if (characterMinutes === null) {
          duration.value = "24";
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
      setTarget(slider.value);
      if (characterMinutes !== null) setHoursMinutes(minutesUntilWake(characterMinutes, slider.value));
    });
    duration.addEventListener("input", () => {
      dirty.add(control);
      const value = Number(duration.value);
      if (selectedUnit() === "days") {
        daysValue = Math.round(value);
        exact.value = "";
        submit.disabled = !Number.isFinite(value) || value < 1 || value > 365 || value !== daysValue;
        return;
      }
      if (characterMinutes === null || !Number.isFinite(value) || value < 24 || value > 8760) {
        exact.value = "";
        submit.disabled = true;
        return;
      }
      setHoursMinutes(Math.round(value * 60));
      setTarget(targetForDuration(characterMinutes, value));
    });
    duration.addEventListener("change", () => {
      if (selectedUnit() === "hours" && Number(duration.value) < 24) duration.value = "24";
      if (selectedUnit() === "days" && Number(duration.value) < 1) duration.value = "1";
      duration.dispatchEvent(new Event("input", { bubbles: true }));
    });
    radios.forEach((radio) => radio.addEventListener("change", () => {
      dirty.add(control);
      applyUnit();
    }));
    control.querySelectorAll("[data-rest-step]").forEach((button) => button.addEventListener("click", () => {
      dirty.add(control);
      duration.value = String(Number(duration.value || duration.min) + Number(button.dataset.restStep));
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
