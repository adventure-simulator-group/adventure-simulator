(() => {
  const DAY = 1440;
  const STEP = 15;
  const leisureTip = 'It is strongly recommended to leave enough leisure time for sleep, and a moderate amount beyond that for morale.';

  function format(minutes) {
    const snapped = Math.round(Number(minutes) / STEP) * STEP;
    const whole = Math.floor(snapped / 60);
    return `${whole}${['', '¼', '½', '¾'][(snapped % 60) / STEP]}h`;
  }

  function formatClock(minutes) {
    const snapped = Math.round(Number(minutes) / STEP) * STEP;
    return `${String(Math.floor(snapped / 60)).padStart(2, '0')}:${String(snapped % 60).padStart(2, '0')}`;
  }

  function parseClock(value) {
    const normalized = String(value).trim();
    let hours;
    let minutes;
    const clock = /^(\d{1,2}):([0-5]\d)$/.exec(normalized);
    if (clock) {
      hours = Number(clock[1]);
      minutes = Number(clock[2]);
    } else if (/^\d{1,2}$/.test(normalized)) {
      hours = Number(normalized);
      minutes = 0;
    } else if (/^\d{3,4}$/.test(normalized)) {
      hours = Number(normalized.slice(0, -2));
      minutes = Number(normalized.slice(-2));
      if (minutes > 59) return null;
    } else {
      return null;
    }
    if (hours > 24 || (hours === 24 && minutes !== 0)) return null;
    return hours * 60 + minutes;
  }

  function createLatestSaveQueue(send, { onState = () => {}, onDrained = () => {} } = {}) {
    let queued = null;
    let ready = false;
    let inFlight = false;
    let halted = false;
    let error = null;

    const status = () => ({
      dirty: inFlight || queued !== null,
      error,
      inFlight,
      pending: inFlight || queued !== null,
    });
    const notify = () => onState(status());

    const pump = async () => {
      if (inFlight || halted || !ready || queued === null) return;
      const snapshot = queued;
      queued = null;
      ready = false;
      inFlight = true;
      error = null;
      notify();
      try {
        await send(snapshot);
      } catch (caught) {
        const hasNewerSnapshot = queued !== null;
        if (!hasNewerSnapshot) queued = snapshot;
        inFlight = false;
        halted = !hasNewerSnapshot;
        error = caught;
        notify();
        if (hasNewerSnapshot && ready) void pump();
        return;
      }
      inFlight = false;
      notify();
      if (queued !== null) {
        if (ready) void pump();
        return;
      }
      onDrained();
    };

    return {
      flush() {
        ready = true;
        void pump();
      },
      retry() {
        if (queued === null) return;
        halted = false;
        error = null;
        ready = true;
        notify();
        void pump();
      },
      stage(snapshot) {
        queued = snapshot;
        halted = false;
        error = null;
        notify();
      },
      status,
    };
  }

  function leisureColor(minutes) {
    const stops = [
      [480, [105, 168, 107]],
      [420, [214, 196, 83]],
      [360, [217, 120, 53]],
      [300, [178, 59, 59]],
      [0, [16, 16, 16]],
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

  function signedEffect(kind, value) {
    if (Math.abs(value) < 0.0005) return '0';
    const rounded = kind === 'gold' ? Math.round(value).toString() : value.toFixed(1);
    return value > 0 ? `+${rounded}` : rounded;
  }

  function renderActivityPreview(row, minutes) {
    const hours = minutes / 60;
    const effects = {
      gold: hours * Number(row.dataset.goldRate || 0),
      virtue: hours * Number(row.dataset.virtueRate || 0),
      morale: row.dataset.prayerMorale === 'true'
        ? Number(row.dataset.prayerMoraleLimit) * (1 - Math.exp(-minutes / Number(row.dataset.prayerMoraleScale)))
        : hours * Number(row.dataset.moraleRate || 0),
      fatigue: hours * Number(row.dataset.fatigueRate || 0),
    };
    Object.entries(effects).forEach(([kind, value]) => {
      const cell = row.querySelector(`[data-activity-effect="${kind}"]`);
      if (!cell) return;
      cell.textContent = signedEffect(kind, value);
      cell.classList.toggle('schedule-effect-positive', value > 0.0005);
      cell.classList.toggle('schedule-effect-negative', value < -0.0005);
      cell.classList.toggle('schedule-effect-neutral', Math.abs(value) <= 0.0005);
    });
  }

  function drainFromBottom(allocation, names, amount) {
    let remaining = amount;
    for (const name of [...names].reverse()) {
      if (!remaining) break;
      const drained = Math.min(allocation[name], remaining);
      allocation[name] -= drained;
      remaining -= drained;
    }
  }

  function stateFor(root) {
    if (root._scheduleState) return root._scheduleState;
    const inputs = [...root.querySelectorAll('[data-schedule-input]')];
    inputs.forEach((input) => { input.value = Math.round(Number(input.value) / STEP) * STEP; });
    const state = { inputs: Object.fromEntries(inputs.map((input) => [input.name, input])) };
    state.saveQueue = createLatestSaveQueue(
      (snapshot) => window.strategicFetch(root.action, {
        method: 'POST',
        body: new URLSearchParams(snapshot),
        headers: { Accept: 'text/plain' },
      }),
      {
        onState(queueState) {
          root.toggleAttribute('data-schedule-dirty', queueState.dirty);
          root.toggleAttribute('data-schedule-pending', queueState.pending);
          root.toggleAttribute('data-schedule-save-in-flight', queueState.inFlight);
          root.toggleAttribute('data-schedule-save-error', Boolean(queueState.error));
          const saveStatus = root.querySelector('[data-schedule-save-status]');
          if (saveStatus) saveStatus.hidden = !queueState.error;
        },
        onDrained() {
          document.dispatchEvent(new Event('strategic-live-refresh-requested'));
        },
      },
    );
    root._scheduleState = state;
    render(root, state);
    return state;
  }

  function values(state) {
    return Object.fromEntries(Object.entries(state.inputs).map(([name, input]) => [name, Number(input.value)]));
  }

  function render(root, state) {
    const allocation = values(state);
    Object.entries(allocation).forEach(([name, minutes]) => {
      root.querySelectorAll(`[data-schedule-value="${name}"] [data-schedule-display]`).forEach((output) => {
        output.textContent = format(minutes);
        output.setAttribute('aria-label', `Daily allocation ${formatClock(minutes)}; click to edit`);
      });
    });
    const leisure = Math.max(0, DAY - Object.values(allocation).reduce((sum, value) => sum + value, 0));
    root.querySelectorAll('[data-schedule-value="leisure_minutes"] [data-schedule-display]').forEach((output) => {
      output.textContent = format(leisure);
    });
    root.querySelectorAll('[data-activity-row]').forEach((row) => {
      const name = row.dataset.activityAllocation;
      renderActivityPreview(row, name === 'leisure_minutes' ? leisure : (allocation[name] || 0));
    });
    root.querySelector('[data-schedule-value="leisure_minutes"]')?.style.setProperty('--leisure-color', leisureColor(leisure));
    root.querySelector('[data-schedule-value="leisure_minutes"]')?.setAttribute('title', leisureTip);
  }

  function setValue(root, state, target, wanted) {
    const allocation = values(state);
    const names = Object.keys(allocation);
    const current = allocation[target];
    const next = Math.max(0, Math.min(DAY, Math.round(wanted / STEP) * STEP));
    const delta = next - current;
    if (delta > 0) {
      const leisure = DAY - names.reduce((sum, name) => sum + allocation[name], 0);
      const otherSkills = names.filter((name) => name !== target && name !== 'labor_minutes');
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
      if (remaining) drainFromBottom(allocation, otherSkills, remaining);
    } else {
      allocation[target] = next;
    }
    Object.entries(allocation).forEach(([name, minutes]) => { state.inputs[name].value = minutes; });
    render(root, state);
  }

  function save(root, delay = 0) {
    clearTimeout(root._scheduleSaveTimer);
    const state = stateFor(root);
    state.saveQueue.stage(new URLSearchParams(new FormData(root)).toString());
    root._scheduleSaveTimer = setTimeout(() => state.saveQueue.flush(), delay);
  }

  function nameFor(element) {
    return element.closest('[data-schedule-value]')?.dataset.scheduleValue;
  }

  function beginEditing(root, state, display) {
    const name = nameFor(display);
    if (!name || !state.inputs[name] || display.dataset.editing) return;
    display.dataset.editing = 'true';
    const input = document.createElement('input');
    input.className = 'schedule-time-input';
    input.type = 'text';
    input.inputMode = 'numeric';
    input.placeholder = 'hh:mm';
    input.setAttribute('aria-label', 'Daily allocation in hours and minutes');
    input.value = formatClock(state.inputs[name].value);
    let finished = false;
    const finish = (commit) => {
      if (finished) return;
      finished = true;
      const parsed = parseClock(input.value);
      if (commit && parsed !== null) {
        setValue(root, state, name, parsed);
        save(root);
      }
      delete display.dataset.editing;
      input.replaceWith(display);
      render(root, state);
    };
    input.addEventListener('keydown', (event) => {
      if (event.key === 'Enter') finish(true);
      if (event.key === 'Escape') finish(false);
    });
    input.addEventListener('blur', () => finish(true), { once: true });
    display.replaceWith(input);
    input.focus();
    input.select();
  }

  if (typeof module !== 'undefined') module.exports = { createLatestSaveQueue, parseClock };
  if (typeof document === 'undefined') return;

  function mountSchedules(root = document) {
    root.querySelectorAll('[data-skill-schedule]').forEach(stateFor);
  }

  mountSchedules();
  document.addEventListener('strategic-live-regions-refreshed', (event) => {
    if (!event.detail?.regions || event.detail.regions.includes('left-sidebar')) mountSchedules();
  });
  document.addEventListener('wheel', (event) => {
    const cell = event.target.closest?.('.party-skill-allocation');
    const root = cell?.closest('[data-skill-schedule]');
    if (!root) return;
    const state = stateFor(root);
    const name = nameFor(cell);
    if (!name || !state.inputs[name]) return;
    event.preventDefault();
    setValue(root, state, name, Number(state.inputs[name].value) + (event.deltaY < 0 ? STEP : -STEP));
    save(root, 180);
  }, { passive: false });
  document.addEventListener('click', (event) => {
    const retry = event.target.closest?.('[data-schedule-retry]');
    if (retry) {
      const root = retry.closest('[data-skill-schedule]');
      stateFor(root).saveQueue.retry();
      return;
    }
    const step = event.target.closest?.('[data-schedule-step]');
    if (step) {
      const root = step.closest('[data-skill-schedule]');
      const state = stateFor(root);
      const name = nameFor(step);
      setValue(root, state, name, Number(state.inputs[name].value) + Number(step.dataset.scheduleStep));
      save(root);
      return;
    }
    const display = event.target.closest?.('[data-schedule-display][role="button"]');
    const root = display?.closest('[data-skill-schedule]');
    if (root) beginEditing(root, stateFor(root), display);
  });
  document.addEventListener('keydown', (event) => {
    const display = event.target.closest?.('[data-schedule-display][role="button"]');
    const root = display?.closest('[data-skill-schedule]');
    if (root && (event.key === 'Enter' || event.key === ' ')) {
      event.preventDefault();
      beginEditing(root, stateFor(root), display);
    }
  });
})();
