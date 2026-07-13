function changeTradeDraftCount(row, change) {
  const count = row.querySelector(".inventory-count");
  count.dataset.base ||= count.textContent.trim();
  setTradeDraftCount(row, Number(count.dataset.tradeDraftChange || 0) + change);
}

function setTradeDraftCount(row, draftChange) {
  const count = row.querySelector(".inventory-count");
  count.dataset.base ||= count.textContent.trim();
  count.dataset.tradeDraftChange = String(draftChange);
  row.classList.toggle("party-trade-changed", draftChange !== 0);
  if (draftChange === 0) {
    count.textContent = count.dataset.base;
    return;
  }

  const sign = draftChange > 0 ? "+" : "";
  const direction = draftChange > 0 ? "positive" : "negative";
  count.innerHTML = `${count.dataset.base} <span class="trade-delta ${direction}">${sign}${draftChange}</span>`;
}

function merchantRow(itemId, sidebar) {
  return sidebar.querySelector(`tr[data-merchant-item="${CSS.escape(itemId)}"]`);
}

function ensureMerchantPlayerRow(itemId, sourceRow) {
  const playerSidebar = document.querySelector(".right-sidebar");
  let row = [...playerSidebar.querySelectorAll(`tr[data-merchant-item="${CSS.escape(itemId)}"]`)]
    .find((candidate) => candidate.dataset.merchantEquipped !== "true");
  if (row) return row;

  row = sourceRow.cloneNode(true);
  row.classList.remove("trade-row-merchant");
  row.classList.add("trade-row-player");
  row.dataset.merchantEquipped = "false";
  row.querySelector(".trade-transfer")?.remove();
  const count = row.querySelector(".inventory-count");
  count.textContent = "0";
  delete count.dataset.base;
  delete count.dataset.tradeDraftChange;
  row.classList.remove("party-trade-changed");
  const equipped = document.createElement("td");
  equipped.className = "inventory-equipped";
  equipped.innerHTML = '<input type="checkbox" disabled>';
  count.after(equipped);
  row.querySelector(".inventory-gold").textContent = sourceRow.dataset.merchantSellPrice;
  row.dataset.generatedMerchantRow = "true";
  const name = row.querySelector(".inventory-item-name");
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.className = "trade-transfer trade-transfer-left";
  cancel.dataset.merchantCancelBuy = itemId;
  cancel.setAttribute("aria-label", `Cancel buying ${itemId}`);
  cancel.title = "Remove one item from this purchase";
  name.append(cancel);
  playerSidebar.querySelector(".trade-inventory-table tbody").append(row);
  return row;
}

function updateMerchantOfferForm() {
  const form = document.querySelector("#merchant-offer");
  form.querySelectorAll("input:not([name='return_to'])").forEach((input) => input.remove());
  const buys = window.merchantDraft || new Map();
  const sells = window.merchantSells || new Map();
  const fields = {
    buy_item_ids: [...buys.keys()],
    buy_quantities: [...buys.values()],
    sell_inventory_ids: [...sells.keys()],
    sell_quantities: [...sells.values()],
  };
  Object.entries(fields).forEach(([name, values]) => {
    const input = document.createElement("input");
    input.type = "hidden";
    input.name = name;
    input.value = values.join(",");
    form.append(input);
  });
  const hasDraft = buys.size > 0 || sells.size > 0;
  form.hidden = !hasDraft;
  form.querySelector("[type='submit']").disabled = !hasDraft;
}

function resetTradeDraft(form) {
  document.querySelectorAll(".trade-inventory-row").forEach((row) => setTradeDraftCount(row, 0));
  document.querySelectorAll("tr[data-generated-merchant-row], tr[data-generated-offer-row]").forEach((row) => row.remove());
  window.merchantDraft = new Map();
  window.merchantSells = new Map();
  window.merchantSaleDetails = new Map();
  window.merchantBuyPrices = new Map();
  window.partyTradeDraft = new Map();
  form.querySelectorAll("input:not([name='return_to'])").forEach((input) => input.remove());
  form.hidden = true;
  form.querySelector("[type='submit']").disabled = true;
}

function updateMerchantGoldDraft() {
  const netQuantities = new Map();
  const buys = window.merchantDraft || new Map();
  const sales = window.merchantSells || new Map();
  const saleDetails = window.merchantSaleDetails || new Map();
  const buyPrices = window.merchantBuyPrices || new Map();
  buys.forEach((quantity, itemId) => netQuantities.set(itemId, (netQuantities.get(itemId) || 0) + quantity));
  sales.forEach((quantity, inventoryId) => {
    const itemId = saleDetails.get(inventoryId).itemId;
    netQuantities.set(itemId, (netQuantities.get(itemId) || 0) - quantity);
  });

  let goldChange = 0;
  netQuantities.forEach((quantity, itemId) => {
    if (quantity > 0) goldChange -= buyPrices.get(itemId) * quantity;
    if (quantity < 0) goldChange += saleDetails.get([...sales.keys()].find((inventoryId) => saleDetails.get(inventoryId).itemId === itemId)).price * -quantity;
  });
  const goldRow = merchantRow("gold_coin", document.querySelector(".right-sidebar"));
  if (goldRow) setTradeDraftCount(goldRow, goldChange);
}

document.addEventListener("click", (event) => {
  const cancelTrade = event.target.closest("[data-cancel-trade]");
  if (cancelTrade) {
    resetTradeDraft(cancelTrade.closest("form"));
    return;
  }
  const cancelBuy = event.target.closest("[data-merchant-cancel-buy]");
  if (cancelBuy) {
    const itemId = cancelBuy.dataset.merchantCancelBuy;
    const buys = window.merchantDraft ||= new Map();
    const quantity = buys.get(itemId) || 0;
    if (quantity === 0) return;
    if (quantity === 1) buys.delete(itemId); else buys.set(itemId, quantity - 1);
    const playerRow = cancelBuy.closest("tr");
    changeTradeDraftCount(playerRow, -1);
    changeTradeDraftCount(merchantRow(itemId, document.querySelector(".left-sidebar")), 1);
    if (playerRow.dataset.generatedMerchantRow === "true" && Number(playerRow.querySelector(".inventory-count").dataset.tradeDraftChange || 0) === 0) playerRow.remove();
    updateMerchantGoldDraft();
    updateMerchantOfferForm();
    return;
  }
  const merchantSell = event.target.closest("[data-merchant-sell]");
  if (merchantSell) {
    const itemId = merchantSell.dataset.itemName;
    const buys = window.merchantDraft ||= new Map();
    const pendingPurchase = buys.get(itemId) || 0;
    const sourceRow = merchantSell.closest("tr");
    if (pendingPurchase > 0) {
      if (pendingPurchase === 1) buys.delete(itemId); else buys.set(itemId, pendingPurchase - 1);
      changeTradeDraftCount(sourceRow, -1);
      changeTradeDraftCount(merchantRow(itemId, document.querySelector(".left-sidebar")), 1);
      updateMerchantGoldDraft();
      updateMerchantOfferForm();
      return;
    }
    const sells = window.merchantSells ||= new Map(); const id = merchantSell.dataset.merchantSell;
    const currentDraft = sells.get(id) || 0;
    if (currentDraft >= Number(sourceRow.querySelector(".inventory-count").dataset.base || sourceRow.querySelector(".inventory-count").textContent.trim())) return;
    sells.set(id, (sells.get(id) || 0) + 1);
    (window.merchantSaleDetails ||= new Map()).set(id, { itemId: merchantSell.dataset.itemName, price: Number(merchantSell.dataset.merchantSellPrice) });
    changeTradeDraftCount(sourceRow, -1);
    changeTradeDraftCount(merchantRow(itemId, document.querySelector(".left-sidebar")), 1);
    updateMerchantGoldDraft();
    updateMerchantOfferForm();
    return;
  }
  const merchantButton = event.target.closest("[data-merchant-buy]");
  if (merchantButton) {
    const draft = window.merchantDraft ||= new Map();
    const item = merchantButton.dataset.merchantBuy;
    draft.set(item, (draft.get(item) || 0) + 1);
    (window.merchantBuyPrices ||= new Map()).set(item, Number(merchantButton.dataset.merchantBuyPrice));
    const sourceRow = merchantButton.closest("tr");
    changeTradeDraftCount(sourceRow, -1);
    changeTradeDraftCount(ensureMerchantPlayerRow(item, sourceRow), 1);
    updateMerchantGoldDraft();
    updateMerchantOfferForm();
    return;
  }
  const button = event.target.closest(".party-draft-transfer");
  if (!button) return;
  const key = button.dataset.item;
  const draft = window.partyTradeDraft ||= new Map();
  const entry = draft.get(key) || { from: button.dataset.from, to: button.dataset.to, quantity: 0 };
  if (entry.quantity >= Number(button.dataset.count)) return;
  entry.quantity += 1;
  draft.set(key, entry);
  setTradeDraftCount(button.closest("tr"), -entry.quantity);
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
  setTradeDraftCount(targetRow, entry.quantity);
  const form = document.querySelector("#party-offer");
  form.querySelectorAll("input").forEach((input) => input.remove());
  const fields = { from_character_ids: [], to_character_ids: [], inventory_item_ids: [], quantities: [] };
  draft.forEach((value, item) => { fields.from_character_ids.push(value.from); fields.to_character_ids.push(value.to); fields.inventory_item_ids.push(item); fields.quantities.push(value.quantity); });
  Object.entries(fields).forEach(([name, values]) => { const input = document.createElement("input"); input.type = "hidden"; input.name = name; input.value = values.join(","); form.append(input); });
  form.hidden = false;
  form.querySelector("[type='submit']").disabled = false;
});
