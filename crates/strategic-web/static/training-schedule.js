(() => {
  const DAY = 1440;
  const MIN_LEISURE = 360;

  const format = (minutes) => `${Math.floor(minutes / 60)}h ${minutes % 60}m`;

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
    const state = {
      inputs: Object.fromEntries(inputs.map((input) => [input.name, input])),
      handles: Object.fromEntries([...root.querySelectorAll('[data-schedule-handle]')]
        .map((handle) => [handle.dataset.scheduleName, handle])),
    };
    root._scheduleState = state;
    render(root, state);
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
      handle.title = `${minutes} minutes per day`;
      handle.setAttribute('aria-valuenow', minutes);
      handle.setAttribute('aria-valuetext', format(minutes));
      root.querySelectorAll(`[data-schedule-value="${name}"]`).forEach((output) => { output.textContent = format(minutes); });
    });
    root.querySelectorAll('[data-schedule-value="leisure_minutes"]').forEach((output) => { output.textContent = format(leisure); });
    root.querySelectorAll('[data-leisure-fill]').forEach((fill) => { fill.style.width = `${leisure / DAY * 100}%`; });
    root.querySelectorAll('[data-leisure-warning]').forEach((warning) => { warning.hidden = leisure >= MIN_LEISURE; });
  }

  function setValue(root, state, target, wanted) {
    const allocation = values(state);
    const current = allocation[target];
    const next = Math.max(0, Math.min(DAY, Math.round(wanted)));
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
      };
      handle.setPointerCapture(evt.pointerId);
      handle.addEventListener('pointermove', move);
      handle.addEventListener('pointerup', finish);
      handle.addEventListener('pointercancel', finish);
      move(evt);
    },
    key(handle, evt) {
      const steps = { ArrowLeft: -15, ArrowDown: -15, ArrowRight: 15, ArrowUp: 15, PageDown: -60, PageUp: 60 };
      if (!(evt.key in steps) && evt.key !== 'Home' && evt.key !== 'End') return;
      evt.preventDefault();
      const root = handle.closest('[data-skill-schedule]');
      const state = stateFor(root);
      const current = Number(state.inputs[handle.dataset.scheduleName].value);
      setValue(root, state, handle.dataset.scheduleName, evt.key === 'Home' ? 0 : evt.key === 'End' ? DAY : current + steps[evt.key]);
    },
  };

  document.querySelectorAll('[data-skill-schedule]').forEach(stateFor);
})();
