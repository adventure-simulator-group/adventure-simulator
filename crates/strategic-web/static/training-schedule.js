(() => {
  const MINUTES_PER_DAY = 24 * 60;
  const MINIMUM_LEISURE = 6 * 60;

  function formatMinutes(minutes) {
    const hours = Math.floor(minutes / 60);
    return `${hours}h ${minutes % 60}m`;
  }

  function distributeReduction(sliders, amount) {
    let remaining = amount;
    const donors = sliders.filter((slider) => Number(slider.value) > 0);
    const total = donors.reduce((sum, slider) => sum + Number(slider.value), 0);
    if (!total) return remaining;

    const reductions = donors.map((slider, index) => {
      const exact = (Number(slider.value) * amount) / total;
      return { slider, index, amount: Math.min(Number(slider.value), Math.floor(exact)), remainder: exact % 1 };
    });
    let assigned = reductions.reduce((sum, reduction) => sum + reduction.amount, 0);
    reductions.sort((a, b) => b.remainder - a.remainder || a.index - b.index);
    for (const reduction of reductions) {
      if (assigned >= amount) break;
      if (reduction.amount < Number(reduction.slider.value)) {
        reduction.amount += 1;
        assigned += 1;
      }
    }
    for (const reduction of reductions) {
      reduction.slider.value = String(Number(reduction.slider.value) - reduction.amount);
    }
    remaining -= assigned;
    return remaining;
  }

  function initialize(schedule) {
    const sliders = [...schedule.querySelectorAll('[data-schedule-slider]')];
    const leisureValue = schedule.querySelector('[data-leisure-value]');
    const leisureBar = schedule.querySelector('[data-leisure-bar]');
    const warning = schedule.querySelector('[data-leisure-warning]');
    let previous = new Map(sliders.map((slider) => [slider, Number(slider.value)]));

    function allocated() {
      return sliders.reduce((sum, slider) => sum + Number(slider.value), 0);
    }

    function render() {
      const leisure = Math.max(0, MINUTES_PER_DAY - allocated());
      for (const slider of sliders) {
        const value = Number(slider.value);
        const output = slider.parentElement.querySelector('[data-schedule-value]');
        output.textContent = formatMinutes(value);
        slider.title = `${value} minutes per day`;
      }
      leisureValue.textContent = formatMinutes(leisure);
      leisureBar.style.width = `${(leisure / MINUTES_PER_DAY) * 100}%`;
      warning.hidden = leisure >= MINIMUM_LEISURE;
    }

    for (const slider of sliders) {
      slider.addEventListener('input', () => {
        const oldValue = previous.get(slider) ?? 0;
        const desired = Number(slider.value);
        const delta = desired - oldValue;
        if (delta > 0) {
          const leisure = Math.max(0, MINUTES_PER_DAY - (allocated() - delta));
          let toTake = Math.max(0, delta - leisure);
          // Labor is sacrificed after leisure, then the other skills share the remainder.
          const labor = sliders.find((candidate) => candidate.name === 'labor_minutes');
          if (labor && labor !== slider) {
            const laborReduction = Math.min(Number(labor.value), toTake);
            labor.value = String(Number(labor.value) - laborReduction);
            toTake -= laborReduction;
          }
          if (toTake > 0) {
            distributeReduction(sliders.filter((candidate) => candidate !== slider && candidate.name !== 'labor_minutes'), toTake);
          }
        }
        previous = new Map(sliders.map((candidate) => [candidate, Number(candidate.value)]));
        render();
      });
    }
    render();
  }

  document.querySelectorAll('[data-training-schedule]').forEach(initialize);
})();
