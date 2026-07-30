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
        const wrapper = choice.closest("label");
        const methodLabel = wrapper?.dataset.methodLabel || choice.value;
        const description = wrapper?.dataset.methodDescription || methodLabel;
        const ingredientName = ingredient?.dataset.itemId?.replaceAll("_", " ") || "the selected ingredient";
        const incompatibility = `${methodLabel} is not an authored preparation for ${ingredientName}.`;
        choice.disabled = !available;
        wrapper?.classList.toggle("disabled", !available);
        wrapper?.setAttribute("aria-disabled", String(!available));
        wrapper?.setAttribute("data-strategic-tooltip", available ? description : incompatibility);
        const status = wrapper?.querySelector("[data-herbal-method-status]");
        if (status) status.textContent = available ? description : incompatibility;
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
      const [output, duration, units, requirement, effect, risk, degraded] = encoded.split("|");
      preview.textContent =
        `${output} · ${duration} minutes · ${units} unit${units === "1" ? "" : "s"} · ` +
        `Requires: ${requirement} · ${effect} · ${risk}` +
        `${degraded === "true" ? " · Degradation warning" : ""}`;
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
