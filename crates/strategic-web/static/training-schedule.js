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

  function stepClockValue(value, delta, fallback = 0) {
    const parsed = parseClock(value);
    const base = parsed === null ? Number(fallback) : parsed;
    return formatClock(Math.max(0, Math.min(DAY, base + Number(delta))));
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

  function roundedEffectValue(kind, value) {
    return kind === 'gold' ? Math.round(value) : Number(value.toFixed(1));
  }

  function signedEffect(kind, value) {
    const rounded = roundedEffectValue(kind, value);
    if (rounded === 0) return '0';
    const formatted = kind === 'gold' ? rounded.toString() : rounded.toFixed(1);
    return rounded > 0 ? `+${formatted}` : formatted;
  }

  function renderEffect(row, kind, value) {
    const cell = row.querySelector(`[data-activity-effect="${kind}"]`);
    if (!cell) return;
    const rounded = roundedEffectValue(kind, value);
    cell.textContent = signedEffect(kind, value);
    cell.classList.toggle('schedule-effect-positive', rounded > 0);
    cell.classList.toggle('schedule-effect-negative', rounded < 0);
    cell.classList.toggle('schedule-effect-neutral', rounded === 0);
  }

  function renderActivityPreview(row, minutes) {
    const hours = minutes / 60;
    const effects = {
      gold: hours * Number(row.dataset.goldRate || 0),
      virtue: hours * Number(row.dataset.virtueRate || 0),
      morale: row.dataset.prayerMorale === 'true'
        ? Number(row.dataset.prayerMoraleMultiplier || 1) * Number(row.dataset.prayerMoraleLimit)
          * (1 - Math.exp(-minutes / Number(row.dataset.prayerMoraleScale)))
        : hours * Number(row.dataset.moraleRate || 0),
      fatigue: hours * Number(row.dataset.fatigueRate || 0),
    };
    Object.entries(effects).forEach(([kind, value]) => renderEffect(row, kind, value));
    const training = row.querySelector('[data-activity-effect="training"]');
    if (training) {
      const rates = (training.dataset.trainingRates || '').split('|').filter(Boolean)
        .map((entry) => {
          const [skill, rate] = entry.split('=');
          return [skill, Number(rate)];
        });
      const trained = rates.map(([skill, rate]) => [skill, hours * rate]);
      const total = trained.reduce((sum, [, value]) => sum + value, 0);
      training.textContent = total > 0 ? `+${total.toFixed(2)}h` : '—';
      training.title = trained.length
        ? trained.map(([skill, value]) => `${skill}: +${value.toFixed(2)}h`).join('; ')
        : 'No skill training';
      training.setAttribute('aria-label', `Effective skill training: ${total.toFixed(2)} hours`);
    }
  }

  function calculateLeisurePreview({
    baselineFatigue,
    currentFatigue,
    fatiguePreviewDivisor,
    laborFatigueRate,
    laborMinutes,
    leisureMinutes,
    moraleLimit,
    moraleScale,
    recoveryRate,
  }) {
    const laborFatigue = laborMinutes / 60 * laborFatigueRate;
    const recovery = leisureMinutes / 60 * recoveryRate;
    const fatigueBeforeRecovery = Math.max(0, currentFatigue) + baselineFatigue + laborFatigue;
    const fatigueAfter = Math.max(0, fatigueBeforeRecovery - recovery);
    const fatigueDelta = fatigueAfter - Math.max(0, currentFatigue);
    const surplusRecoveryRate = Math.max(0, recovery - baselineFatigue - laborFatigue);
    const timeToClearFatigue = surplusRecoveryRate > 0
      ? Math.max(0, currentFatigue) / surplusRecoveryRate
      : Number.POSITIVE_INFINITY;
    const qualifyingDays = Math.max(0, 1 - timeToClearFatigue);
    const dailyMoraleQuality = moraleLimit
      * (1 - Math.exp(-surplusRecoveryRate / Math.max(moraleScale, Number.EPSILON)));
    return {
      fatigueDelta,
      leisureFatigue: (fatigueDelta - laborFatigue) / Math.max(fatiguePreviewDivisor, Number.EPSILON),
      morale: qualifyingDays * dailyMoraleQuality,
    };
  }

  function renderLeisurePreview(row, leisureMinutes, allocation) {
    const preview = calculateLeisurePreview({
      baselineFatigue: Number(row.dataset.leisureBaselineFatigue || 0),
      currentFatigue: Number(row.dataset.leisureCurrentFatigue || 0),
      fatiguePreviewDivisor: Number(row.dataset.leisureFatiguePreviewDivisor || 1),
      laborFatigueRate: Number(row.dataset.leisureLaborFatigueRate || 0),
      laborMinutes: Number(allocation.labor_minutes || 0),
      leisureMinutes,
      moraleLimit: Number(row.dataset.leisureMoraleLimit || 0),
      moraleScale: Number(row.dataset.leisureMoraleScale || 0),
      recoveryRate: Number(row.dataset.leisureRecoveryRate || 0),
    });
    renderEffect(row, 'morale', preview.morale);
    renderEffect(row, 'fatigue', preview.leisureFatigue);
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
    syncReligionControls(root, root.querySelector('[data-religion-auto-toggle]')?.checked ?? true);
    syncCombatControls(root, root.querySelector('[data-combat-auto-toggle]')?.checked ?? true);
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

  function metaInputActive(input, root) {
    const religionAuto = root.querySelector('[data-religion-auto-toggle]')?.checked ?? true;
    const combatAuto = root.querySelector('[data-combat-auto-toggle]')?.checked ?? true;
    if ('religionAutoBudget' in input.dataset) return religionAuto;
    if ('religionManualBudget' in input.dataset) return !religionAuto;
    if ('combatAutoBudget' in input.dataset) return combatAuto;
    if ('combatManualBudget' in input.dataset) return !combatAuto;
    return true;
  }

  function values(root, state, activeOnly = true) {
    return Object.fromEntries(Object.entries(state.inputs)
      .filter(([, input]) => !activeOnly || metaInputActive(input, root))
      .map(([name, input]) => [name, Number(input.value)]));
  }

  function syncReligionControls(root, autoTrain) {
    root.querySelectorAll('[data-religion-auto-budget]').forEach((input) => {
      const allocation = input.closest('[data-schedule-value]');
      allocation?.classList.toggle('religion-allocation-inactive', !autoTrain);
      allocation?.querySelectorAll('button, [role="button"]')
        .forEach((control) => { control.toggleAttribute('disabled', !autoTrain); control.setAttribute('aria-disabled', String(!autoTrain)); });
    });
    root.querySelectorAll('[data-religion-manual-budget]').forEach((input) => {
      const allocation = input.closest('[data-schedule-value]');
      allocation?.classList.toggle('religion-allocation-inactive', autoTrain);
      allocation?.querySelectorAll('button, [role="button"]')
        .forEach((control) => { control.toggleAttribute('disabled', autoTrain); control.setAttribute('aria-disabled', String(autoTrain)); });
    });
    root.querySelectorAll('[data-religion-auto-control]').forEach((control) => { control.hidden = !autoTrain; });
    root.querySelectorAll('[data-religion-primary-manual-control]').forEach((control) => { control.hidden = autoTrain; });
  }

  function setAllocationInteractive(allocation, active) {
    allocation?.classList.toggle('religion-allocation-inactive', !active);
    allocation?.querySelectorAll('[data-schedule-display]').forEach((control) => {
      control.setAttribute('aria-disabled', String(!active));
      control.setAttribute('tabindex', active ? '0' : '-1');
    });
  }

  function syncCombatControls(root, autoTrain) {
    root.querySelectorAll('[data-combat-auto-budget]').forEach((input) => {
      const allocation = input.closest('[data-schedule-value]');
      setAllocationInteractive(allocation, autoTrain);
    });
    root.querySelectorAll('[data-combat-manual-budget]').forEach((input) => {
      const allocation = input.closest('[data-schedule-value]');
      setAllocationInteractive(allocation, !autoTrain);
    });
  }

  function religionAllocationTotal(allocation, autoTrain) {
    if (autoTrain) return Number(allocation.religion_minutes || 0);
    return Object.entries(allocation)
      .filter(([name]) => name.startsWith('religion_') && name.endsWith('_minutes') && name !== 'religion_minutes')
      .reduce((sum, [, minutes]) => sum + Number(minutes), 0);
  }

  function render(root, state) {
    const allValues = values(root, state, false);
    const allocation = values(root, state);
    Object.entries(allValues).forEach(([name, minutes]) => {
      root.querySelectorAll(`[data-schedule-value="${name}"] [data-schedule-display]`).forEach((output) => {
        output.textContent = format(minutes);
        output.setAttribute('aria-label', `Daily allocation ${formatClock(minutes)}; click to edit`);
      });
    });
    const auto = root.querySelector('[data-religion-auto-toggle]')?.checked ?? true;
    if (!auto) {
      const religionTotal = religionAllocationTotal(allValues, false);
      root.querySelectorAll('[data-schedule-value="religion_minutes"] [data-schedule-display]').forEach((output) => {
        output.textContent = format(religionTotal);
        output.setAttribute('aria-label', `Total Religion allocation ${formatClock(religionTotal)}`);
      });
    }
    const combatAuto = root.querySelector('[data-combat-auto-toggle]')?.checked ?? true;
    if (!combatAuto) {
      const combatTotal = ['melee_minutes', 'ranged_minutes', 'dodge_minutes', 'block_minutes']
        .reduce((sum, name) => sum + Number(allValues[name] || 0), 0);
      root.querySelectorAll('[data-schedule-value="combat_minutes"] [data-schedule-display]').forEach((output) => {
        output.textContent = format(combatTotal);
        output.setAttribute('aria-label', `Total Combat allocation ${formatClock(combatTotal)}`);
      });
    }
    const leisure = Math.max(0, DAY - Object.values(allocation).reduce((sum, value) => sum + value, 0));
    root.querySelectorAll('[data-schedule-value="leisure_minutes"] [data-schedule-display]').forEach((output) => {
      output.textContent = format(leisure);
    });
    root.querySelectorAll('[data-activity-row]').forEach((row) => {
      const name = row.dataset.activityAllocation;
      if (name === 'leisure_minutes') renderLeisurePreview(row, leisure, allocation);
      else renderActivityPreview(row, allocation[name] || 0);
    });
    root.querySelector('[data-schedule-value="leisure_minutes"]')?.style.setProperty('--leisure-color', leisureColor(leisure));
    root.querySelector('[data-schedule-value="leisure_minutes"]')?.setAttribute('title', leisureTip);
  }

  function setValue(root, state, target, wanted) {
    if (!state.inputs[target] || !metaInputActive(state.inputs[target], root)) return;
    const allocation = values(root, state);
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
    if (!name || !state.inputs[name] || !metaInputActive(state.inputs[name], root)
      || display.dataset.editing || !window.StrategicNumericEditor) return;
    const originalMinutes = Number(state.inputs[name].value);
    const anchor = display.closest('.party-skill-allocation');
    const rail = display.closest('.left-sidebar');
    window.StrategicNumericEditor.open({
      display,
      initialValue: originalMinutes,
      parse: parseClock,
      format: formatClock,
      step: STEP,
      minimum: 0,
      maximum: DAY,
      anchor,
      rail,
      groupLabel: 'Edit daily allocation',
      inputLabel: 'Daily allocation in hours and minutes',
      increaseLabel: 'Increase daily allocation by 15 minutes',
      decreaseLabel: 'Decrease daily allocation by 15 minutes',
      saveLabel: 'Save daily allocation',
      cancelLabel: 'Cancel daily allocation edit',
      onCommit: (parsed) => {
        setValue(root, state, name, parsed);
        save(root);
        render(root, state);
      },
    });
  }

  if (typeof module !== 'undefined') module.exports = {
    calculateLeisurePreview,
    createLatestSaveQueue,
    metaInputActive,
    setAllocationInteractive,
    parseClock,
    religionInputActive: (input, autoTrain) => {
      if ('religionAutoBudget' in input.dataset) return autoTrain;
      if ('religionManualBudget' in input.dataset) return !autoTrain;
      return true;
    },
    religionAllocationTotal,
    signedEffect,
    stepClockValue,
  };
  if (typeof document === 'undefined') return;

  function mountSchedules(root = document) {
    root.querySelectorAll('[data-skill-schedule]').forEach(stateFor);
  }

  mountSchedules();
  document.addEventListener('strategic-live-regions-refreshed', (event) => {
    if (!event.detail?.regions || event.detail.regions.includes('left-sidebar')) mountSchedules();
  });
  document.addEventListener('click', (event) => {
    const expand = event.target.closest?.('[data-religion-expand]');
    if (expand) {
      const root = expand.closest('[data-skill-schedule]') || expand.closest('table');
      const expanded = expand.getAttribute('aria-expanded') !== 'true';
      expand.setAttribute('aria-expanded', String(expanded));
      root.querySelectorAll('.religion-detail-row').forEach((row) => { row.hidden = !expanded; });
      return;
    }
    const combatExpand = event.target.closest?.('[data-combat-expand]');
    if (combatExpand) {
      const root = combatExpand.closest('[data-skill-schedule]') || combatExpand.closest('table');
      const expanded = combatExpand.getAttribute('aria-expanded') !== 'true';
      combatExpand.setAttribute('aria-expanded', String(expanded));
      root.querySelectorAll('.combat-detail-row').forEach((row) => { row.hidden = !expanded; });
      return;
    }
    const socialExpand = event.target.closest?.('[data-social-expand]');
    if (socialExpand) {
      const root = socialExpand.closest('[data-skill-schedule]') || socialExpand.closest('table');
      const expanded = socialExpand.getAttribute('aria-expanded') !== 'true';
      socialExpand.setAttribute('aria-expanded', String(expanded));
      root.querySelectorAll('.social-detail-row').forEach((row) => { row.hidden = !expanded; });
      return;
    }
    const retry = event.target.closest?.('[data-schedule-retry]');
    if (retry) {
      const root = retry.closest('[data-skill-schedule]');
      stateFor(root).saveQueue.retry();
      return;
    }
    const display = event.target.closest?.('[data-schedule-display][role="button"]');
    const root = display?.closest('[data-skill-schedule]');
    if (root) beginEditing(root, stateFor(root), display);
  });
  document.addEventListener('change', (event) => {
    const toggle = event.target.closest?.('[data-religion-auto-toggle]');
    const combatToggle = event.target.closest?.('[data-combat-auto-toggle]');
    if (!toggle && !combatToggle) return;
    const root = (toggle || combatToggle).closest('[data-skill-schedule]');
    if (toggle) syncReligionControls(root, toggle.checked);
    if (combatToggle) syncCombatControls(root, combatToggle.checked);
    render(root, stateFor(root));
    save(root);
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
