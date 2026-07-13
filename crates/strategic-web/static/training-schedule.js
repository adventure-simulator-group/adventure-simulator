(() => {
  const DAY = 1440;
  const STEP = 15;
  const leisureTip = 'It is strongly recommended to leave enough leisure time for sleep, and a moderate amount beyond that for morale.';

  function format(minutes) {
    const whole = Math.floor(minutes / 60);
    return `${whole}${['', '¼', '½', '¾'][(minutes % 60) / STEP]}h`;
  }

  function leisureColor(minutes) {
    const stops = [
      [480, [105, 168, 107]], // green
      [420, [214, 196, 83]],  // yellow
      [360, [217, 120, 53]],  // orange
      [300, [178, 59, 59]],   // red
      [0, [16, 16, 16]],      // black
    ];
    for (let index = 0; index < stops.length - 1; index += 1) {
      const [high, highColor] = stops[index];
      const [low, lowColor] = stops[index + 1];
      if (minutes >= low) {
        const ratio = Math.min(1, Math.max(0, (minutes - low) / (high - low)));
        const color = highColor.map((channel, channelIndex) => Math.round(lowColor[channelIndex] + (channel - lowColor[channelIndex]) * ratio));
        return `rgb(${color.join(' ')})`;
      }
    }
    return 'rgb(16 16 16)';
  }

  function distribute(values, names, amount) {
    const total = names.reduce((sum, name) => sum + values[name], 0);
    if (!total || !amount) return 0;
    const portions = names.map((name, index) => {
      const exact = values[name] * amount / total;
      return { name, index, amount: Math.min(values[name], Math.floor(exact)), remainder: exact % 1 };
    });
    let assigned = portions.reduce((sum, portion) => sum + portion.amount, 0);
    portions.sort((a, b) => b.remainder - a.remainder || a.index - b.index);
    for (const portion of portions) {
      if (assigned === amount) break;
      if (portion.amount < values[portion.name]) {
        portion.amount += 1;
        assigned += 1;
      }
    }
    portions.forEach(({ name, amount: reduction }) => { values[name] -= reduction; });
    return assigned;
  }

  function stateFor(root) {
    if (root._scheduleState) return root._scheduleState;
    const inputs = [...root.querySelectorAll('[data-schedule-input]')];
    inputs.forEach((input) => {
      input.value = Math.round(Number(input.value) / STEP) * STEP;
    });
    const state = {
      inputs: Object.fromEntries(inputs.map((input) => [input.name, input])),
      handles: Object.fromEntries([...root.querySelectorAll('[data-schedule-handle]')]
        .map((handle) => [handle.dataset.scheduleName, handle])),
    };
    root._scheduleState = state;
    render(root, state);
    bindEditing(root, state);
    return state;
  }

  function values(state) {
    return Object.fromEntries(Object.entries(state.inputs).map(([name, input]) => [name, Number(input.value)]));
  }

  function render(root, state) {
    const allocations = values(state);
    const leisure = Math.max(0, DAY - Object.values(allocations).reduce((sum, value) => sum + value, 0));
    Object.entries(state.handles).forEach(([name, handle]) => {
      const minutes = allocations[name];
      handle.style.left = `${minutes / DAY * 100}%`;
      handle.title = `${format(minutes)} per day`;
      handle.setAttribute('aria-valuenow', minutes);
      handle.setAttribute('aria-valuetext', format(minutes));
      root.querySelectorAll(`[data-schedule-value="${name}"] [data-schedule-display]`).forEach((output) => { output.textContent = format(minutes); });
    });
    root.querySelectorAll('[data-schedule-value="leisure_minutes"] [data-schedule-display]').forEach((output) => { output.textContent = format(leisure); });
    root.querySelectorAll('[data-leisure-fill]').forEach((fill) => {
      fill.style.width = `${leisure / DAY * 100}%`;
      fill.style.backgroundColor = leisureColor(leisure);
      fill.parentElement.title = leisureTip;
    });
  }

  function setValue(root, state, target, wanted) {
    const allocation = values(state);
    const current = allocation[target];
    const next = Math.max(0, Math.min(DAY, Math.round(wanted / STEP) * STEP));
    const delta = next - current;
    if (delta > 0) {
      const leisure = DAY - Object.values(allocation).reduce((sum, value) => sum + value, 0);
      const otherSkills = Object.keys(allocation).filter((name) => name !== target && name !== 'labor_minutes');
      const donors = target === 'labor_minutes'
        ? otherSkills
        : ['labor_minutes', ...otherSkills.filter((name) => name !== 'labor_minutes')];
      const capacity = Math.max(0, leisure) + donors.reduce((sum, name) => sum + allocation[name], 0);
      const accepted = Math.min(delta, capacity);
      allocation[target] += accepted;
      let remaining = Math.max(0, accepted - Math.max(0, leisure));
      if (target !== 'labor_minutes') {
        const fromLabor = Math.min(allocation.labor_minutes, remaining);
        allocation.labor_minutes -= fromLabor;
        remaining -= fromLabor;
      }
      if (remaining) distribute(allocation, otherSkills, remaining);
    } else {
      allocation[target] = next;
    }
    Object.entries(allocation).forEach(([name, minutes]) => { state.inputs[name].value = minutes; });
    render(root, state);
  }

  function save(root, delay = 0) {
    clearTimeout(root._scheduleSaveTimer);
    root._scheduleSaveTimer = setTimeout(() => {
      const body = new URLSearchParams(new FormData(root));
      fetch(root.action, { method: 'POST', body, headers: { Accept: 'text/plain' } })
        .catch(() => { root.dataset.scheduleSaveError = 'true'; });
    }, delay);
  }

  function nameFor(element) {
    return element.closest('.party-skill-row')?.querySelector('[data-schedule-handle]')?.dataset.scheduleName;
  }

  function bindEditing(root, state) {
    root.querySelectorAll('.skill-rank-bar, .party-skill-allocation').forEach((element) => {
      element.addEventListener('wheel', (evt) => {
        const name = nameFor(element);
        if (!name) return;
        evt.preventDefault();
        setValue(root, state, name, Number(state.inputs[name].value) + (evt.deltaY < 0 ? STEP : -STEP));
        save(root, 180);
      }, { passive: false });
    });
    root.querySelectorAll('[data-schedule-step]').forEach((button) => {
      button.addEventListener('click', () => {
        const name = nameFor(button);
        setValue(root, state, name, Number(state.inputs[name].value) + Number(button.dataset.scheduleStep));
        save(root);
      });
    });
    root.querySelectorAll('[data-schedule-display]').forEach((display) => {
      display.addEventListener('click', () => {
        const name = nameFor(display);
        if (!name || display.dataset.editing) return;
        display.dataset.editing = 'true';
        const input = document.createElement('input');
        input.className = 'schedule-time-input';
        input.type = 'number';
        input.min = '0';
        input.max = '24';
        input.step = '0.25';
        input.value = String(Number(state.inputs[name].value) / 60);
        const finish = (commit) => {
          if (commit && input.value !== '') {
            setValue(root, state, name, Number(input.value) * 60);
            save(root);
          }
          display.textContent = format(Number(state.inputs[name].value));
          delete display.dataset.editing;
          input.replaceWith(display);
        };
        input.addEventListener('keydown', (evt) => {
          if (evt.key === 'Enter') finish(true);
          if (evt.key === 'Escape') finish(false);
        });
        input.addEventListener('blur', () => finish(true), { once: true });
        display.replaceWith(input);
        input.focus();
        input.select();
      });
    });
  }

  window.scheduleDrag = {
    start(handle, evt) {
      evt.preventDefault();
      const root = handle.closest('[data-skill-schedule]');
      const state = stateFor(root);
      const name = handle.dataset.scheduleName;
      const move = (event) => {
        const track = handle.parentElement;
        const rect = track.getBoundingClientRect();
        setValue(root, state, name, (event.clientX - rect.left) / rect.width * DAY);
      };
      const finish = () => {
        handle.removeEventListener('pointermove', move);
        handle.removeEventListener('pointerup', finish);
        handle.removeEventListener('pointercancel', finish);
        save(root);
      };
      handle.setPointerCapture(evt.pointerId);
      handle.addEventListener('pointermove', move);
      handle.addEventListener('pointerup', finish);
      handle.addEventListener('pointercancel', finish);
      move(evt);
    },
    key(handle, evt) {
      const steps = { ArrowLeft: -STEP, ArrowDown: -STEP, ArrowRight: STEP, ArrowUp: STEP, PageDown: -60, PageUp: 60 };
      if (!(evt.key in steps) && evt.key !== 'Home' && evt.key !== 'End') return;
      evt.preventDefault();
      const root = handle.closest('[data-skill-schedule]');
      const state = stateFor(root);
      const current = Number(state.inputs[handle.dataset.scheduleName].value);
      setValue(root, state, handle.dataset.scheduleName, evt.key === 'Home' ? 0 : evt.key === 'End' ? DAY : current + steps[evt.key]);
      save(root);
    },
  };

  document.querySelectorAll('[data-skill-schedule]').forEach(stateFor);
})();
