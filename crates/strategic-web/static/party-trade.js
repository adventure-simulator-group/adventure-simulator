document.addEventListener("click", (event) => {
  const button = event.target.closest(".party-draft-transfer");
  if (!button) return;
  const key = button.dataset.item;
  const draft = window.partyTradeDraft ||= new Map();
  const entry = draft.get(key) || { from: button.dataset.from, to: button.dataset.to, quantity: 0 };
  if (entry.quantity >= Number(button.dataset.count)) return;
  entry.quantity += 1;
  draft.set(key, entry);
  button.closest("tr").classList.add("party-trade-changed");
  const count = button.closest("tr").querySelector(".inventory-count");
  count.dataset.base ||= count.textContent.trim();
  count.innerHTML = `${Number(count.dataset.base) - entry.quantity} <span class="trade-delta negative">-${entry.quantity}</span>`;
  const sourceSidebar = button.closest("aside");
  const targetSidebar = sourceSidebar.classList.contains("left-sidebar") ? document.querySelector(".right-sidebar") : document.querySelector(".left-sidebar");
  let targetRow = targetSidebar.querySelector(`tr[data-item-key="${CSS.escape(button.dataset.key)}"]`);
  if (!targetRow) {
    targetRow = button.closest("tr").cloneNode(true);
    targetRow.dataset.generatedOfferRow = "true";
    targetRow.querySelector(".party-draft-transfer")?.remove();
    const provisionalCount = targetRow.querySelector(".inventory-count");
    provisionalCount.textContent = "0";
    delete provisionalCount.dataset.base;
    targetSidebar.querySelector(".trade-inventory-table tbody").append(targetRow);
  }
  const otherCount = targetRow.querySelector(".inventory-count");
  otherCount.dataset.base ||= otherCount.textContent.trim();
  targetRow.classList.add("party-trade-changed");
  otherCount.innerHTML = `${Number(otherCount.dataset.base) + entry.quantity} <span class="trade-delta positive">+${entry.quantity}</span>`;
  const form = document.querySelector("#party-offer");
  form.querySelectorAll("input").forEach((input) => input.remove());
  const fields = { from_character_ids: [], to_character_ids: [], inventory_item_ids: [], quantities: [] };
  draft.forEach((value, item) => { fields.from_character_ids.push(value.from); fields.to_character_ids.push(value.to); fields.inventory_item_ids.push(item); fields.quantities.push(value.quantity); });
  Object.entries(fields).forEach(([name, values]) => { const input = document.createElement("input"); input.type = "hidden"; input.name = name; input.value = values.join(","); form.append(input); });
  form.querySelector("button").disabled = false;
});
