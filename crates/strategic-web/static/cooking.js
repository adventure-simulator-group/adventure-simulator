(() => {
  const setup = { "pan-fry": 5, stew: 12, roast: 7, bake: 15 };

  const formatNumber = (value) => Number(value).toFixed(2).replace(/\.00$/, "").replace(/(\.\d)0$/, "$1");

  function refreshInventory(element) {
    window.strategicInventoryBrowser?.refresh?.(element);
    window.strategicTradeUi?.mountInventoryBulkControls?.(element);
  }

  function transferButton(direction, id, name, count) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `trade-transfer trade-transfer-${direction}`;
    button.dataset.cookingUnstage = id;
    button.dataset.dynamicTransfer = "";
    button.dataset.defaultTransferMode = "one";
    button.dataset.transferMode = "one";
    button.dataset.count = String(count);
    button.dataset.labelOne = `Return one ${name} to inventory`;
    button.dataset.labelTarget = `Return ${name} to inventory`;
    button.dataset.labelAll = `Return all ${name} to inventory`;
    button.title = button.dataset.labelOne;
    button.setAttribute("aria-label", button.dataset.labelOne);
    const glyph = document.createElement("span");
    glyph.className = "inventory-transfer-glyph arrows-1";
    glyph.setAttribute("aria-hidden", "true");
    glyph.append(document.createElement("i"));
    button.append(glyph);
    return button;
  }

  function mount(root = document) {
    root.querySelectorAll("[data-cooking-activity]").forEach((form) => {
      if (form.dataset.cookingMounted) return;
      form.dataset.cookingMounted = "true";

      const submit = form.querySelector("[data-cook-submit]");
      const ids = form.querySelector("[data-cooking-ids]");
      const quantities = form.querySelector("[data-cooking-quantities]");
      const empty = form.querySelector("[data-cooking-pot-empty]");
      const potBrowser = form.querySelector('[data-inventory-browser="cooking-pot-left"]');
      const potBody = potBrowser?.querySelector("tbody");
      const inventoryBrowser = form.querySelector('[data-inventory-browser="cooking-inventory-right"]');
      const staged = new Map();

      const update = () => {
        const method = form.querySelector("[data-cooking-method]:checked");
        const values = [...staged.values()].filter((value) => value.quantity > 0);
        ids.value = values.map((value) => value.id).join(",");
        quantities.value = values.map((value) => value.quantity).join(",");
        if (empty) empty.hidden = values.length > 0;

        let reason = "Transfer at least one ingredient to the pot";
        if (method && values.length) {
          const mass = values.reduce((sum, value) => sum + value.mass * value.quantity, 0);
          const slowest = Math.max(...values.map((value) => value.safety));
          const batch = Math.ceil(Math.sqrt(Math.max(0, mass - 0.5)) * 8);
          const duration = setup[method.value] + slowest + batch;
          reason = `Cooking time: ${duration} minutes`;
        }
        submit.disabled = !method || values.length === 0;
        submit.title = reason;
        submit.setAttribute("aria-label", submit.disabled ? `Cook unavailable. ${reason}` : `Cook. ${reason}`);
      };

      const updateSourceRow = (entry) => {
        const count = entry.sourceRow.querySelector(".inventory-count");
        if (!count) return;
        count.dataset.base ||= String(entry.available);
        if (entry.quantity === 0) {
          delete count.dataset.tradeDraftChange;
          count.textContent = String(entry.available);
          entry.sourceRow.classList.remove("party-trade-changed");
        } else {
          count.dataset.tradeDraftChange = String(-entry.quantity);
          count.innerHTML = `${entry.available} <span class="trade-delta negative">-${entry.quantity}</span>`;
          entry.sourceRow.classList.add("party-trade-changed");
        }
        const stage = entry.sourceRow.querySelector("[data-cooking-stage]");
        if (stage) stage.disabled = entry.quantity >= entry.available;
      };

      const ensurePotRow = (entry) => {
        let row = potBody?.querySelector(`[data-cooking-pot-id="${CSS.escape(entry.id)}"]`);
        if (row || !potBody) return row;
        row = entry.sourceRow.cloneNode(true);
        row.hidden = false;
        row.removeAttribute("aria-expanded");
        row.classList.remove("food-component-row", "food-parent-row", "party-trade-changed");
        row.classList.remove("trade-row-player");
        row.classList.add("trade-row-merchant");
        row.dataset.cookingPotId = entry.id;
        row.dataset.inventoryQuantity = "0";
        delete row.dataset.cookingSource;
        row.querySelector(":scope > .inventory-target")?.remove();
        const actions = row.querySelector(".inventory-row-actions");
        if (actions) {
          actions.replaceChildren(transferButton("right", entry.id, entry.name, entry.quantity));
        }
        potBody.append(row);
        return row;
      };

      const updatePotRow = (entry) => {
        if (entry.quantity === 0) {
          potBody?.querySelector(`[data-cooking-pot-id="${CSS.escape(entry.id)}"]`)?.remove();
          return;
        }
        const row = ensurePotRow(entry);
        if (!row) return;
        row.dataset.inventoryQuantity = String(entry.quantity);
        const count = row.querySelector(".inventory-count");
        if (count) {
          count.textContent = String(entry.quantity);
          count.dataset.base = String(entry.quantity);
          delete count.dataset.tradeDraftChange;
        }
        const weight = row.querySelector(".inventory-weight");
        if (weight) {
          weight.textContent = formatNumber(entry.mass * entry.quantity);
          weight.dataset.sortValue = String(entry.mass * entry.quantity);
        }
        const value = row.querySelector(".inventory-gold");
        if (value) {
          value.textContent = formatNumber(entry.value * entry.quantity);
          value.dataset.sortValue = String(entry.value * entry.quantity);
        }
        const unstage = row.querySelector("[data-cooking-unstage]");
        if (unstage) unstage.dataset.count = String(entry.quantity);
      };

      const redraw = (entry) => {
        updateSourceRow(entry);
        updatePotRow(entry);
        refreshInventory(inventoryBrowser);
        refreshInventory(potBrowser);
        update();
      };

      form.addEventListener("click", (event) => {
        const stage = event.target.closest?.("[data-cooking-stage]");
        if (stage) {
          event.preventDefault();
          const sourceRow = stage.closest("tr");
          const id = stage.dataset.cookingStage;
          const available = Math.max(0, Number(stage.dataset.count) || 0);
          const entry = staged.get(id) || {
            id,
            name: stage.dataset.cookingName,
            available,
            mass: Math.max(0, Number(stage.dataset.mass) || 0),
            safety: Math.max(0, Number(stage.dataset.safety) || 0),
            value: Math.max(0, Number(sourceRow.querySelector(".inventory-gold")?.textContent) || 0),
            quantity: 0,
            sourceRow,
          };
          entry.quantity = stage.dataset.transferMode === "all"
            ? entry.available
            : Math.min(entry.available, entry.quantity + 1);
          staged.set(id, entry);
          redraw(entry);
          return;
        }

        const unstage = event.target.closest?.("[data-cooking-unstage]");
        if (unstage) {
          event.preventDefault();
          const entry = staged.get(unstage.dataset.cookingUnstage);
          if (!entry) return;
          entry.quantity = unstage.dataset.transferMode === "all" ? 0 : Math.max(0, entry.quantity - 1);
          if (entry.quantity === 0) staged.delete(entry.id);
          redraw(entry);
        }
      });

      form.addEventListener("change", (event) => {
        if (event.target.matches("[data-cooking-method]")) update();
      });
      update();
    });
  }

  window.addEventListener("DOMContentLoaded", () => mount());
  window.addEventListener("strategic-live-regions-refreshed", () => mount());
  window.strategicCooking = { mount };
})();
