(() => {
  "use strict";

  const methodAttribute = (method) => `data-${method.replaceAll("_", "-")}`;

  function mount(root) {
    if (!root || root.dataset.herbalismMounted === "true") return;
    root.dataset.herbalismMounted = "true";
    const form = root.querySelector("[data-herbalism-form]");
    const preview = root.querySelector("[data-herbal-preview]");
    const submit = root.querySelector("[data-herbal-submit]");
    const methods = [...root.querySelectorAll("[data-herbal-method]")];

    function refresh() {
      const ingredient = root.querySelector("[data-herbal-ingredient]:checked");
      const method = root.querySelector("[data-herbal-method]:checked");
      methods.forEach((choice) => {
        const available = ingredient?.getAttribute(methodAttribute(choice.value));
        choice.disabled = !available;
        choice.closest("label")?.classList.toggle("disabled", !available);
        choice.closest("label")?.setAttribute(
          "data-strategic-tooltip",
          available ? choice.closest("label").textContent.trim() : "Not an authored preparation for this ingredient",
        );
      });
      if (!ingredient || !method || method.disabled) {
        preview.textContent = ingredient
          ? "Choose a compatible preparation method."
          : "Select one ingredient and one method.";
        submit.disabled = true;
        return;
      }
      const encoded = ingredient.getAttribute(methodAttribute(method.value));
      if (!encoded) {
        preview.textContent = "This ingredient and method are incompatible.";
        submit.disabled = true;
        return;
      }
      const [output, duration, units, effect, risk, degraded] = encoded.split("|");
      preview.textContent =
        `${output} · ${duration} minutes · ${units} unit${units === "1" ? "" : "s"} · ` +
        `${effect} · Risk: ${risk}${degraded === "true" ? " · Degradation warning" : ""}`;
      submit.disabled = false;
    }
    form.addEventListener("change", refresh);
    refresh();
  }

  function mountAll(scope = document) {
    scope.querySelectorAll("[data-herbalism-activity]").forEach(mount);
  }

  document.addEventListener("DOMContentLoaded", () => mountAll());
  document.addEventListener("datastar-patch", (event) => mountAll(event.target || document));
  window.strategicHerbalism = { mount, mountAll };
})();
