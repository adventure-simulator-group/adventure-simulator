(() => {
  const TOOLTIP_ID = 'strategic-tooltip';
  const TOOLTIP_ATTRIBUTE = 'data-strategic-tooltip';
  const VIEWPORT_MARGIN = 8;
  const ANCHOR_GAP = 8;

  const hasAccessibleName = (element) => {
    if (element.hasAttribute('aria-label') || element.hasAttribute('aria-labelledby')) return true;
    if (element.matches('img, area') && element.hasAttribute('alt')) return true;
    if (element.matches('input[type="button"], input[type="submit"], input[type="reset"]') && element.value) return true;
    return Boolean(element.textContent?.trim());
  };

  const createTooltipSystem = (documentRoot = document, windowRoot = window) => {
    const documentElement = documentRoot.documentElement;
    let tooltip = documentRoot.getElementById(TOOLTIP_ID);
    if (!tooltip) {
      tooltip = documentRoot.createElement('div');
      tooltip.id = TOOLTIP_ID;
      tooltip.className = 'strategic-tooltip';
      tooltip.setAttribute('role', 'tooltip');
      tooltip.hidden = true;
      tooltip.setAttribute('aria-hidden', 'true');
      documentRoot.body.append(tooltip);
    }

    let activeTarget = null;
    let descriptionWasLinked = false;
    let suppressFocusUntil = 0;
    let pinnedTarget = null;

    const appendBestiarySection = (container, heading, lines, itemClass) => {
      const section = documentRoot.createElement('section');
      section.className = 'strategic-tooltip-section';
      const title = documentRoot.createElement('strong');
      title.className = 'strategic-tooltip-heading';
      title.textContent = heading;
      const list = documentRoot.createElement('ul');
      list.className = 'strategic-tooltip-list';
      for (const line of lines) {
        const item = documentRoot.createElement('li');
        item.className = itemClass;
        item.textContent = line;
        list.append(item);
      }
      section.append(title, list);
      container.append(section);
    };

    const renderEnemyDetails = (enemy, container) => {
      container.textContent = '';
      const title = documentRoot.createElement('strong');
      title.className = 'strategic-tooltip-enemy-title';
      title.textContent = enemy.name;
      container.append(title);
      if (enemy.strengths?.length) {
        appendBestiarySection(container, 'Strengths', enemy.strengths, 'strategic-tooltip-strength');
      }
      if (enemy.weaknesses?.length) {
        appendBestiarySection(container, 'Weaknesses', enemy.weaknesses, 'strategic-tooltip-weakness');
      }
    };

    const renderBestiaryEnemies = (target) => {
      if (!target.hasAttribute('data-bestiary-enemies')) return false;
      tooltip.classList.add('strategic-tooltip-interactive');
      tooltip.setAttribute('role', 'dialog');
      tooltip.setAttribute('aria-modal', 'false');
      let enemies = [];
      try {
        const parsed = JSON.parse(target.getAttribute('data-bestiary-enemies') || '[]');
        if (Array.isArray(parsed)) enemies = parsed;
      } catch {
        enemies = [];
      }

      tooltip.textContent = '';
      const name = (target.getAttribute('data-bestiary-name') || '').trim();
      if (name) tooltip.setAttribute('aria-label', name);
      else tooltip.removeAttribute('aria-label');
      if (name) {
        const title = documentRoot.createElement('strong');
        title.className = 'strategic-tooltip-title';
        title.textContent = name;
        tooltip.append(title);
      }
      if (!enemies.length) {
        const empty = documentRoot.createElement('span');
        empty.className = 'strategic-tooltip-empty';
        empty.textContent = 'No current enemy types';
        tooltip.append(empty);
        return true;
      }

      const list = documentRoot.createElement('div');
      list.className = 'strategic-tooltip-enemy-list';
      const details = documentRoot.createElement('section');
      details.className = 'strategic-tooltip-enemy-details';
      details.hidden = true;
      for (const enemy of enemies) {
        const item = documentRoot.createElement('button');
        item.type = 'button';
        item.className = 'strategic-tooltip-enemy';
        item.textContent = enemy.name;
        const showDetails = () => {
          details.hidden = false;
          renderEnemyDetails(enemy, details);
          position();
        };
        item.addEventListener('pointerover', showDetails);
        item.addEventListener('focus', showDetails);
        list.append(item);
      }
      tooltip.append(list, details);
      return true;
    };

    const renderTooltip = (target) => {
      if (renderBestiaryEnemies(target)) return;
      tooltip.classList.remove('strategic-tooltip-interactive');
      tooltip.setAttribute('role', 'tooltip');
      tooltip.removeAttribute('aria-modal');
      tooltip.removeAttribute('aria-label');
      tooltip.textContent = target.getAttribute(TOOLTIP_ATTRIBUTE);
    };

    const enhance = (target) => {
      if (!target?.getAttribute) return null;
      const title = target.getAttribute('title');
      if (title !== null) {
        const text = title.trim();
        target.removeAttribute('title');
        if (!text) return null;
        target.setAttribute(TOOLTIP_ATTRIBUTE, text);
        if (!hasAccessibleName(target)) {
          target.setAttribute('aria-label', text);
          target.setAttribute('data-strategic-tooltip-generated-label', '');
        }
      }
      return target.hasAttribute(TOOLTIP_ATTRIBUTE) ? target : null;
    };

    const unlinkDescription = () => {
      if (!activeTarget || descriptionWasLinked) return;
      const ids = (activeTarget.getAttribute('aria-describedby') || '')
        .split(/\s+/)
        .filter((id) => id && id !== TOOLTIP_ID);
      if (ids.length) activeTarget.setAttribute('aria-describedby', ids.join(' '));
      else activeTarget.removeAttribute('aria-describedby');
    };

    const setPinned = (target) => {
      if (pinnedTarget && pinnedTarget !== target) {
        pinnedTarget.setAttribute('aria-pressed', 'false');
      }
      pinnedTarget = target;
      if (pinnedTarget) pinnedTarget.setAttribute('aria-pressed', 'true');
    };

    const hide = (force = false) => {
      if (pinnedTarget && force !== true) return;
      activeTarget?.removeEventListener('pointerleave', hide);
      activeTarget?.removeEventListener('blur', hide);
      unlinkDescription();
      activeTarget = null;
      descriptionWasLinked = false;
      tooltip.hidden = true;
      tooltip.setAttribute('aria-hidden', 'true');
    };

    const position = () => {
      if (!activeTarget || tooltip.hidden || !activeTarget.isConnected) {
        if (activeTarget && !activeTarget.isConnected) {
          setPinned(null);
          hide(true);
        }
        return;
      }

      const anchor = activeTarget.getBoundingClientRect();
      const box = tooltip.getBoundingClientRect();
      const viewportWidth = windowRoot.innerWidth || documentElement.clientWidth;
      const viewportHeight = windowRoot.innerHeight || documentElement.clientHeight;
      const maximumLeft = Math.max(VIEWPORT_MARGIN, viewportWidth - box.width - VIEWPORT_MARGIN);
      const left = Math.min(maximumLeft, Math.max(VIEWPORT_MARGIN, anchor.left + anchor.width / 2 - box.width / 2));
      let top = anchor.top - box.height - ANCHOR_GAP;
      let placement = 'top';

      if (top < VIEWPORT_MARGIN) {
        top = anchor.bottom + ANCHOR_GAP;
        placement = 'bottom';
      }
      top = Math.min(
        Math.max(VIEWPORT_MARGIN, viewportHeight - box.height - VIEWPORT_MARGIN),
        Math.max(VIEWPORT_MARGIN, top),
      );

      tooltip.dataset.placement = placement;
      tooltip.style.left = `${Math.round(left)}px`;
      tooltip.style.top = `${Math.round(top)}px`;
    };

    const show = (target) => {
      target = enhance(target);
      if (!target) return;
      if (pinnedTarget && pinnedTarget !== target) return;
      if (activeTarget !== target) {
        hide(true);
        activeTarget = target;
        target.addEventListener('pointerleave', hide);
        target.addEventListener('blur', hide);
        const tooltipText = target.getAttribute(TOOLTIP_ATTRIBUTE).trim();
        const describedBy = (target.getAttribute('aria-describedby') || '').split(/\s+/).filter(Boolean);
        const repeatsAccessibleName = target.getAttribute('aria-label')?.trim() === tooltipText;
        descriptionWasLinked = describedBy.includes(TOOLTIP_ID) || repeatsAccessibleName;
        if (!descriptionWasLinked) target.setAttribute('aria-describedby', [...describedBy, TOOLTIP_ID].join(' '));
      }
      renderTooltip(target);
      tooltip.hidden = false;
      tooltip.setAttribute('aria-hidden', 'false');
      position();
    };

    const togglePinned = (target, focusInside = false) => {
      if (pinnedTarget === target) {
        setPinned(null);
        hide(true);
        return;
      }
      setPinned(null);
      show(target);
      setPinned(target);
      if (focusInside) tooltip.querySelector('.strategic-tooltip-enemy')?.focus();
    };

    const tooltipTarget = (eventTarget) => eventTarget?.closest?.(`[title], [${TOOLTIP_ATTRIBUTE}]`);

    documentRoot.addEventListener('pointerover', (event) => {
      if (event.pointerType === 'touch') return;
      show(tooltipTarget(event.target));
    });
    documentRoot.addEventListener('pointerout', (event) => {
      if (!activeTarget || activeTarget.contains(event.relatedTarget)) return;
      if (tooltip.contains(event.relatedTarget)) return;
      if (activeTarget.contains(event.target)) hide();
    });
    documentRoot.addEventListener('focusin', (event) => {
      if (Date.now() < suppressFocusUntil) return;
      show(tooltipTarget(event.target));
    });
    documentRoot.addEventListener('focusout', (event) => {
      if (activeTarget?.contains(event.target) && !activeTarget.contains(event.relatedTarget)) hide();
    });
    documentRoot.addEventListener('pointerdown', (event) => {
      if (event.pointerType !== 'touch') return;
      suppressFocusUntil = Date.now() + 800;
      setPinned(null);
      hide(true);
    });
    documentRoot.addEventListener('click', (event) => {
      const target = event.target?.closest?.('[data-tooltip-pinnable]');
      if (target) {
        togglePinned(target);
        return;
      }
      if (pinnedTarget && !tooltip.contains(event.target)) {
        setPinned(null);
        hide(true);
      }
    });
    documentRoot.addEventListener('keydown', (event) => {
      const target = event.target?.closest?.('[data-tooltip-pinnable]');
      if (target && (event.key === 'Enter' || event.key === ' ')) {
        event.preventDefault();
        togglePinned(target, true);
        return;
      }
      if (event.key === 'Escape') {
        const returnTarget = pinnedTarget;
        setPinned(null);
        hide(true);
        if (returnTarget?.isConnected) {
          suppressFocusUntil = Date.now() + 100;
          returnTarget.focus();
        }
      }
    });
    tooltip.addEventListener('pointerleave', () => hide());
    windowRoot.addEventListener('resize', position);
    windowRoot.addEventListener('scroll', position, true);

    return {
      tooltip,
      enhance,
      show,
      hide,
      position,
      get activeTarget() { return activeTarget; },
      get pinnedTarget() { return pinnedTarget; },
    };
  };

  if (typeof module !== 'undefined' && module.exports) {
    module.exports = { createTooltipSystem };
  } else if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => createTooltipSystem(), { once: true });
  } else {
    createTooltipSystem();
  }
})();
