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

    const appendBestiarySection = (heading, lines, itemClass) => {
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
      tooltip.append(section);
    };

    const renderTooltip = (target) => {
      const strengths = (target.getAttribute('data-bestiary-strengths') || '')
        .split('\n')
        .map((line) => line.trim())
        .filter(Boolean);
      const weaknesses = (target.getAttribute('data-bestiary-weaknesses') || '')
        .split('\n')
        .map((line) => line.trim())
        .filter(Boolean);
      if (!strengths.length && !weaknesses.length) {
        tooltip.textContent = target.getAttribute(TOOLTIP_ATTRIBUTE);
        return;
      }
      tooltip.textContent = '';
      const name = (target.getAttribute('data-bestiary-name') || '').trim();
      if (name) {
        const title = documentRoot.createElement('strong');
        title.className = 'strategic-tooltip-title';
        title.textContent = name;
        tooltip.append(title);
      }
      appendBestiarySection('Strengths', strengths, 'strategic-tooltip-strength');
      appendBestiarySection('Weaknesses', weaknesses, 'strategic-tooltip-weakness');
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

    const hide = () => {
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
        if (activeTarget && !activeTarget.isConnected) hide();
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
      if (activeTarget !== target) {
        hide();
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

    const tooltipTarget = (eventTarget) => eventTarget?.closest?.(`[title], [${TOOLTIP_ATTRIBUTE}]`);

    documentRoot.addEventListener('pointerover', (event) => {
      if (event.pointerType === 'touch') return;
      show(tooltipTarget(event.target));
    });
    documentRoot.addEventListener('pointerout', (event) => {
      if (!activeTarget || activeTarget.contains(event.relatedTarget)) return;
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
      hide();
    });
    documentRoot.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') hide();
    });
    windowRoot.addEventListener('resize', position);
    windowRoot.addEventListener('scroll', position, true);

    return { tooltip, enhance, show, hide, position, get activeTarget() { return activeTarget; } };
  };

  if (typeof module !== 'undefined' && module.exports) {
    module.exports = { createTooltipSystem };
  } else if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => createTooltipSystem(), { once: true });
  } else {
    createTooltipSystem();
  }
})();
