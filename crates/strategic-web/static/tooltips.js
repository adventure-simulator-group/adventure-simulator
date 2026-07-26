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
      pinnedTarget = null;
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
      if (pinnedTarget) return;
      if (!activeTarget || activeTarget.contains(event.relatedTarget)) return;
      if (tooltip.contains(event.relatedTarget)) return;
      if (activeTarget.contains(event.target)) hide();
    });
    documentRoot.addEventListener('focusin', (event) => {
      if (Date.now() < suppressFocusUntil) return;
      show(tooltipTarget(event.target));
    });
    documentRoot.addEventListener('focusout', (event) => {
      if (pinnedTarget) return;
      if (activeTarget?.contains(event.target) && !activeTarget.contains(event.relatedTarget)) hide();
    });
    documentRoot.addEventListener('pointerdown', (event) => {
      if (event.pointerType !== 'touch') return;
      const target = tooltipTarget(event.target);
      if (!target) {
        if (pinnedTarget) hide();
        return;
      }
      suppressFocusUntil = Date.now() + 800;
      if (pinnedTarget === target) {
        hide();
      } else {
        show(target);
        pinnedTarget = target;
      }
    });
    documentRoot.addEventListener('click', (event) => {
      if (Date.now() < suppressFocusUntil) return;
      const target = tooltipTarget(event.target);
      if (!target || event.detail === 0) return;
      if (pinnedTarget === target) hide();
      else {
        show(target);
        pinnedTarget = target;
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
