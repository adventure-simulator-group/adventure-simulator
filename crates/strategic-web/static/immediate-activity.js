(() => {
  const states = new WeakMap();
  let lastSnapshot = Number(window.strategicCharacterMinutes || 0);

  const clock = (minutes) => {
    const wrapped = ((minutes % 1440) + 1440) % 1440;
    return `${String(Math.floor(wrapped / 60)).padStart(2, '0')}:${String(wrapped % 60).padStart(2, '0')}`;
  };
  const rounded = (kind, value) => kind === 'gold' ? Math.round(value) : Number(value.toFixed(1));
  const signed = (kind, value) => {
    const result = rounded(kind, value);
    if (result === 0) return '0';
    const shown = kind === 'gold' ? String(result) : result.toFixed(1);
    return result > 0 ? `+${shown}` : shown;
  };
  const activityKind = (allocation) => allocation.replace(/_minutes$/, '');

  function copyPreview(source, target, hours) {
    const minutes = hours * 60;
    const values = {
      gold: hours * Number(source.dataset.goldRate || 0),
      virtue: hours * Number(source.dataset.virtueRate || 0),
      morale: source.dataset.prayerMorale === 'true'
        ? Number(source.dataset.prayerMoraleMultiplier || 1)
          * Number(source.dataset.prayerMoraleLimit || 0)
          * (1 - Math.exp(-minutes / Number(source.dataset.prayerMoraleScale || 1)))
        : hours * Number(source.dataset.moraleRate || 0),
      fatigue: hours * Number(source.dataset.fatigueRate || 0),
    };
    Object.entries(values).forEach(([kind, value]) => {
      const cell = target.querySelector(`[data-activity-effect="${kind}"]`);
      const result = rounded(kind, value);
      cell.textContent = signed(kind, value);
      cell.classList.toggle('schedule-effect-positive', result > 0);
      cell.classList.toggle('schedule-effect-negative', result < 0);
      cell.classList.toggle('schedule-effect-neutral', result === 0);
    });
    const sourceTraining = source.querySelector('[data-activity-effect="training"]');
    const targetTraining = target.querySelector('[data-activity-effect="training"]');
    const trained = (sourceTraining?.dataset.trainingRates || '').split('|').filter(Boolean)
      .map((entry) => {
        const [skill, rate] = entry.split('=');
        return [skill, Number(rate) * hours];
      });
    const total = trained.reduce((sum, [, value]) => sum + value, 0);
    targetTraining.textContent = total > 0 ? `+${total.toFixed(2)}h` : 'â€”';
    targetTraining.title = trained.length
      ? trained.map(([skill, value]) => `${skill}: +${value.toFixed(2)}h`).join('; ')
      : 'No skill training';
  }

  function mount(modal) {
    if (states.has(modal)) return;
    const panel = modal.querySelector('[data-activity-form]');
    const slider = modal.querySelector('[data-activity-duration]');
    const state = { opener: null, source: null, start: lastSnapshot };
    states.set(modal, state);

    const render = () => {
      const hours = Number(slider.value);
      modal.querySelector('[data-activity-minutes]').value = String(hours * 60);
      modal.querySelector('[data-activity-end]').textContent = `Ends at ${clock(state.start + hours * 60)}`;
      modal.querySelector('[data-activity-hours]').textContent = `${hours} h spent`;
      slider.setAttribute('aria-valuetext', `${hours} hours; ends at ${clock(state.start + hours * 60)}`);
      modal.querySelector('[data-activity-submit]').textContent = `Spend ${hours} ${hours === 1 ? 'hour' : 'hours'}`;
      if (state.source) copyPreview(state.source, modal.querySelector('[data-activity-preview-row]'), hours);
    };
    const close = () => {
      modal.hidden = true;
      document.body.classList.remove('activity-modal-open');
      state.opener?.focus();
    };
    const open = (button) => {
      state.opener = button;
      state.source = button.closest('[data-activity-row]');
      state.start = Number(window.strategicCharacterMinutes ?? lastSnapshot);
      slider.value = '1';
      const allocation = button.dataset.activityOpen;
      const kind = activityKind(allocation);
      const label = state.source.querySelector('.sr-only')?.textContent?.trim() || 'Activity';
      modal.querySelector('[data-activity-title]').textContent = `Perform ${label}`;
      modal.querySelector('[data-activity-preview-label]').textContent = label;
      modal.querySelector('[data-activity-kind]').value = kind;
      const schedule = button.closest('[data-skill-schedule]')
        || modal.parentElement?.querySelector('[data-skill-schedule]');
      const serviceName = kind === 'apprenticeship'
        ? 'apprenticeship_service_id'
        : kind === 'profession_practice' ? 'profession_service_id' : '';
      modal.querySelector('[data-activity-service]').value = serviceName
        ? schedule?.elements.namedItem(serviceName)?.value || '' : '';
      modal.hidden = false;
      document.body.classList.add('activity-modal-open');
      render();
      panel.focus();
    };
    slider.addEventListener('input', render);
    modal.querySelectorAll('[data-activity-close]').forEach((button) => button.addEventListener('click', close));
    modal.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        close();
      }
      if (event.key === 'Tab') {
        const focusable = [...panel.querySelectorAll('button:not(:disabled), input:not(:disabled)')];
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
      }
    });
    modal.closest('.left-sidebar, .right-sidebar, body')?.addEventListener('click', (event) => {
      const button = event.target.closest?.('[data-activity-open]');
      if (button) open(button);
    });
  }

  const mountAll = (root = document) => root.querySelectorAll?.('[data-activity-modal]').forEach(mount);
  mountAll();
  document.addEventListener('strategic-live-regions-refreshed', (event) => {
    if (!event.detail?.regions || event.detail.regions.includes('right-sidebar')) mountAll();
  });
  document.addEventListener('strategic-time-ready', (event) => {
    lastSnapshot = Number(event.detail?.characterMinutes || 0);
  });

  if (typeof module !== 'undefined') module.exports = { activityKind, clock, signed };
})();
