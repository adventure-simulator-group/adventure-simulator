(() => {
  const setup = { "pan-fry": 5, stew: 12, roast: 7, bake: 15 };

  function mount(root = document) {
    root.querySelectorAll("[data-cooking-activity]").forEach((form) => {
      if (form.dataset.cookingMounted) return;
      form.dataset.cookingMounted = "true";
      const submit = form.querySelector("[data-cook-submit]");
      const ids = form.querySelector("[data-cooking-ids]");
      const quantities = form.querySelector("[data-cooking-quantities]");
      const update = () => {
        const method = form.querySelector("[data-cooking-method]:checked");
        const selected = [...form.querySelectorAll("[data-cooking-lot]:checked")];
        const values = selected.map((checkbox) => {
          const quantity = checkbox.closest("label").querySelector("[data-cooking-quantity]");
          const bounded = Math.max(1, Math.min(Number(quantity.max), Number(quantity.value) || 1));
          quantity.value = String(bounded);
          return { id: checkbox.dataset.cookingLot, quantity: bounded, mass: Number(checkbox.dataset.mass) * bounded, safety: Number(checkbox.dataset.safety) };
        });
        ids.value = values.map((value) => value.id).join(",");
        quantities.value = values.map((value) => value.quantity).join(",");
        let reason = "Select at least one ingredient";
        if (method && values.length) {
          const mass = values.reduce((sum, value) => sum + value.mass, 0);
          const slowest = Math.max(...values.map((value) => value.safety));
          const batch = Math.ceil(Math.sqrt(Math.max(0, mass - 0.5)) * 8);
          const duration = setup[method.value] + slowest + batch;
          reason = `Cooking time: ${duration} minutes`;
        }
        submit.disabled = !method || values.length === 0;
        submit.title = reason;
        submit.setAttribute("aria-label", submit.disabled ? `Cook unavailable. ${reason}` : `Cook. ${reason}`);
      };
      form.addEventListener("input", update);
      form.addEventListener("change", update);
      update();
    });
  }

  window.addEventListener("DOMContentLoaded", () => mount());
  window.addEventListener("strategic-live-regions-refreshed", () => mount());
  window.strategicCooking = { mount };
})();
