(() => {
  if (typeof document === "undefined") return;
  const returnFocus = new WeakMap();
  let pinnedPoint = null;

  const tooltipFor = (point) => {
    const id = point?.dataset?.physiologyTooltipId;
    const dialog = point?.closest?.("[data-physiology-dialog]");
    return id && dialog ? dialog.querySelector(`#${CSS.escape(id)}`) : null;
  };

  const hidePoint = (point) => {
    if (!point) return;
    const tooltip = tooltipFor(point);
    if (tooltip) tooltip.hidden = true;
    point.setAttribute("aria-expanded", "false");
  };

  const showPoint = (point, pin = false) => {
    const dialog = point?.closest?.("[data-physiology-dialog]");
    const tooltip = tooltipFor(point);
    if (!dialog || !tooltip) return;
    dialog.querySelectorAll("[data-physiology-reading-point][aria-expanded='true']")
      .forEach((other) => {
        if (other !== point) hidePoint(other);
      });
    tooltip.hidden = false;
    point.setAttribute("aria-expanded", "true");
    if (pin) pinnedPoint = point;
  };

  const clearPinnedPoint = () => {
    const point = pinnedPoint;
    pinnedPoint = null;
    if (point && document.activeElement !== point && !point.matches(":hover")) hidePoint(point);
  };

  const openDialog = (button) => {
    const dialog = document.getElementById(button.dataset.physiologyDialogOpen);
    if (!(dialog instanceof HTMLDialogElement)) return;
    returnFocus.set(dialog, button);
    button.setAttribute("aria-expanded", "true");
    dialog.showModal();
    requestAnimationFrame(() => dialog.querySelector("[data-physiology-dialog-close]")?.focus());
  };

  const closeDialog = (dialog) => {
    if (!(dialog instanceof HTMLDialogElement)) return;
    dialog.close();
  };

  document.addEventListener("click", (event) => {
    const opener = event.target.closest?.("[data-physiology-dialog-open]");
    if (opener) {
      openDialog(opener);
      return;
    }
    const close = event.target.closest?.("[data-physiology-dialog-close]");
    if (close) {
      closeDialog(close.closest("dialog"));
      return;
    }
    const point = event.target.closest?.("[data-physiology-reading-point]");
    if (point) {
      if (pinnedPoint === point) {
        clearPinnedPoint();
      } else {
        clearPinnedPoint();
        showPoint(point, true);
      }
      return;
    }
    if (event.target.matches?.("[data-physiology-dialog]")) closeDialog(event.target);
  });

  document.addEventListener("pointerover", (event) => {
    const point = event.target.closest?.("[data-physiology-reading-point]");
    if (point && !pinnedPoint) showPoint(point);
  });

  document.addEventListener("pointerout", (event) => {
    const point = event.target.closest?.("[data-physiology-reading-point]");
    if (!point || point.contains(event.relatedTarget)) return;
    if (pinnedPoint !== point && document.activeElement !== point) hidePoint(point);
  });

  document.addEventListener("focusin", (event) => {
    const point = event.target.closest?.("[data-physiology-reading-point]");
    if (point) showPoint(point);
  });

  document.addEventListener("focusout", (event) => {
    const point = event.target.closest?.("[data-physiology-reading-point]");
    if (point && pinnedPoint !== point) hidePoint(point);
  });

  document.addEventListener("keydown", (event) => {
    const point = event.target.closest?.("[data-physiology-reading-point]");
    if (point && (event.key === "Enter" || event.key === " ")) {
      event.preventDefault();
      point.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      return;
    }
    if (event.key === "Escape" && pinnedPoint) clearPinnedPoint();
  });

  document.addEventListener("close", (event) => {
    const dialog = event.target;
    if (!dialog.matches?.("[data-physiology-dialog]")) return;
    dialog.querySelectorAll("[data-physiology-reading-point][aria-expanded='true']")
      .forEach(hidePoint);
    pinnedPoint = null;
    const opener = returnFocus.get(dialog);
    opener?.setAttribute("aria-expanded", "false");
    if (opener?.isConnected) opener.focus();
    returnFocus.delete(dialog);
  }, true);
})();
