function changeTradeDraftCount(row, change) {
  const count = row.querySelector(".inventory-count");
  count.dataset.base ||= count.textContent.trim();
  setTradeDraftCount(row, Number(count.dataset.tradeDraftChange || 0) + change);
}

function mountInventoryBulkControls() {
  document.querySelectorAll(".inventory-footer-actions").forEach((actions) => {
    const section = actions.closest(".sidebar-section");
    const headerRow = section?.querySelector(".trade-inventory-table thead tr");
    if (headerRow) headerRow.append(actions);
  });
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
  const playerSidebar = document.querySelector('[data-inventory-pane]:not([hidden])') || document.querySelector(".right-sidebar");
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
  form.querySelectorAll("input:not([name='return_to']):not([name='inventory_scope'])").forEach((input) => input.remove());
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
  window.inventoryDiscardDraft = new Map();
  document.querySelectorAll("[data-generated-discard-row]").forEach((row) => row.remove());
  const discardTable = document.querySelector("[data-discard-table]");
  const discardEmpty = document.querySelector("[data-discard-empty]");
  if (discardTable) discardTable.hidden = true;
  if (discardEmpty) discardEmpty.hidden = false;
  form.querySelectorAll("input:not([name='return_to']):not([name='inventory_scope'])").forEach((input) => input.remove());
  form.hidden = true;
  form.querySelector("[type='submit']").disabled = true;
}

function updateDiscardForm() {
  const form = document.querySelector("#inventory-discard");
  if (!form) return;
  form.querySelectorAll("input").forEach((input) => input.remove());
  const draft = window.inventoryDiscardDraft ||= new Map();
  const fields = {
    inventory_item_ids: [...draft.keys()],
    quantities: [...draft.values()],
  };
  Object.entries(fields).forEach(([name, values]) => {
    const input = document.createElement("input");
    input.type = "hidden";
    input.name = name;
    input.value = values.join(",");
    form.append(input);
  });
  const hasDraft = draft.size > 0;
  form.hidden = !hasDraft;
  form.querySelector("[type='submit']").disabled = !hasDraft;
  const table = document.querySelector("[data-discard-table]");
  const empty = document.querySelector("[data-discard-empty]");
  if (table) table.hidden = !hasDraft;
  if (empty) empty.hidden = hasDraft;
}

function ensureDiscardRow(sourceRow, inventoryId) {
  let row = document.querySelector(`[data-discard-staged="${CSS.escape(inventoryId)}"]`);
  if (row) return row;
  row = sourceRow.cloneNode(true);
  row.dataset.discardStaged = inventoryId;
  row.dataset.generatedDiscardRow = "true";
  row.classList.add("discard-staged-row");
  delete row.dataset.discardSource;
  row.querySelector(".inventory-equipped")?.remove();
  const action = row.querySelector("[data-discard-item]");
  if (action) {
    delete action.dataset.discardItem;
    action.dataset.unstageDiscard = inventoryId;
    action.classList.remove("trade-transfer-left");
    action.classList.add("trade-transfer-right");
    action.title = "Remove one item from the discard list";
    action.setAttribute("aria-label", "Remove one item from the discard list");
  }
  const count = row.querySelector(".inventory-count");
  count.textContent = "0";
  delete count.dataset.base;
  delete count.dataset.tradeDraftChange;
  document.querySelector("[data-discard-list]").append(row);
  return row;
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
  const tab = event.target.closest("[data-inventory-tab]");
  if (tab) {
    const root = tab.closest("[data-inventory-tabs]");
    root.querySelectorAll("[data-inventory-tab]").forEach((entry) => entry.classList.toggle("active", entry === tab));
    root.querySelectorAll("[data-inventory-pane]").forEach((pane) => { pane.hidden = pane.dataset.inventoryPane !== tab.dataset.inventoryTab; });
    const scope = document.querySelector("#merchant-offer [name='inventory_scope']");
    if (scope) scope.value = tab.dataset.inventoryTab;
    resetTradeDraft(document.querySelector("#merchant-offer"));
    return;
  }
  const targetStep = event.target.closest("[data-target-step]");
  if (targetStep) {
    const control = targetStep.closest("[data-target-control]");
    const value = control.querySelector("[data-target-value]");
    const quantity = Math.max(0, Number(value.textContent) + Number(targetStep.dataset.targetStep));
    value.textContent = String(quantity);
    control.closest("tr").dataset.target = String(quantity);
    let down = control.querySelector('[data-target-step="-1"]');
    if (!down && quantity > 0) {
      down = targetStep.cloneNode(true); down.dataset.targetStep = "-1"; down.classList.replace("inventory-target-up", "inventory-target-down"); down.textContent = "⌄"; control.append(down);
    }
    if (down) down.hidden = quantity === 0;
    fetch("/api/inventory-target", { method: "POST", body: new URLSearchParams({ item_id: control.dataset.itemId, quantity: String(quantity), party_scope: control.dataset.partyScope }) });
    return;
  }
  const bulk = event.target.closest("[data-inventory-bulk]");
  if (bulk) {
    const panel = bulk.closest("aside, section, .sidebar-section") || document;
    const selector = bulk.dataset.inventoryBulk === "buy" ? "[data-merchant-buy]" : bulk.dataset.inventoryBulk === "loot" ? "[data-loot-stage]" : ["deposit", "withdraw"].includes(bulk.dataset.inventoryBulk) ? `[data-pool-stage][data-pool-direction="${bulk.dataset.inventoryBulk}"]` : bulk.dataset.inventoryBulk.startsWith("party-") ? ".party-draft-transfer" : "[data-merchant-sell]";
    panel.querySelectorAll(`${selector}[data-transfer-mode="${bulk.dataset.transferMode}"]`).forEach((button) => button.click());
    return;
  }
  const cancelLoot = event.target.closest("[data-cancel-loot]");
  if (cancelLoot) {
    window.lootTransferDraft = new Map();
    document.querySelectorAll("[data-loot-row]").forEach((row) => setTradeDraftCount(row, 0));
    const form = cancelLoot.closest("form"); form.querySelectorAll("input").forEach((input) => input.remove()); form.hidden = true;
    return;
  }
  const lootStage = event.target.closest("[data-loot-stage]");
  if (lootStage) {
    const row = lootStage.closest("tr");
    const draft = window.lootTransferDraft ||= new Map();
    const id = lootStage.dataset.lootStage;
    const staged = draft.get(id) || 0;
    const available = Number(row.dataset.count);
    const mode = lootStage.dataset.transferMode || "one";
    const amount = Math.max(0, Math.min(available - staged, mode === "all" ? available - staged : mode === "target" ? Number(row.dataset.target) - Number(row.dataset.current) - staged : 1));
    if (!amount) return;
    draft.set(id, staged + amount); setTradeDraftCount(row, -(staged + amount));
    const form = document.querySelector("#loot-transfer-offer");
    form.querySelectorAll("input").forEach((input) => input.remove());
    for (const [name, values] of Object.entries({ item_ids: [...draft.keys()], quantities: [...draft.values()] })) { const input = document.createElement("input"); input.type="hidden"; input.name=name; input.value=values.join(","); form.append(input); }
    form.hidden = false; form.querySelector('[type="submit"]').disabled = false;
    return;
  }
  const cancelPool = event.target.closest("[data-cancel-pool]");
  if (cancelPool) { window.poolTransferDraft = new Map(); document.querySelectorAll("[data-pool-stage]").forEach((button) => setTradeDraftCount(button.closest("tr"), 0)); const form=cancelPool.closest("form"); form.querySelectorAll("input").forEach((input)=>input.remove()); form.hidden=true; return; }
  const poolStage = event.target.closest("[data-pool-stage]");
  if (poolStage) {
    const form = document.querySelector("#pool-transfer-offer");
    if (form.dataset.direction && form.dataset.direction !== poolStage.dataset.poolDirection) { window.poolTransferDraft = new Map(); document.querySelectorAll("[data-pool-stage]").forEach((button) => setTradeDraftCount(button.closest("tr"), 0)); }
    form.dataset.direction = poolStage.dataset.poolDirection;
    form.action = `${location.pathname}/${poolStage.dataset.poolDirection}`;
    const draft = window.poolTransferDraft ||= new Map(); const id=poolStage.dataset.poolStage; const staged=draft.get(id)||0;
    const available=Number(poolStage.dataset.count); const mode=poolStage.dataset.transferMode||"one";
    const amount=Math.max(0,Math.min(available-staged,mode==="all"?available-staged:mode==="target"?Number(poolStage.dataset.target)-Number(poolStage.dataset.current)-staged:1));
    if (!amount) return; draft.set(id,staged+amount); setTradeDraftCount(poolStage.closest("tr"),-(staged+amount));
    form.querySelectorAll("input").forEach((input)=>input.remove());
    for (const [name, values] of Object.entries({item_id:[...draft.keys()],quantity:[...draft.values()]})) { const input=document.createElement("input");input.type="hidden";input.name=name;input.value=values.join(",");form.append(input); }
    form.hidden=false;form.querySelector('[type="submit"]').disabled=false;return;
  }
  const cancelTrade = event.target.closest("[data-cancel-trade]");
  if (cancelTrade) {
    resetTradeDraft(cancelTrade.closest("form"));
    return;
  }
  const unstageDiscard = event.target.closest("[data-unstage-discard]");
  if (unstageDiscard) {
    const id = unstageDiscard.dataset.unstageDiscard;
    const draft = window.inventoryDiscardDraft ||= new Map();
    const stagedRow = unstageDiscard.closest("tr");
    const quantity = draft.get(id) || Number(stagedRow.querySelector(".inventory-count").textContent) || 0;
    if (!quantity) return;
    const sourceRow = document.querySelector(`[data-discard-source="${CSS.escape(id)}"]`);
    if (sourceRow) setTradeDraftCount(sourceRow, -(quantity - 1));
    if (quantity === 1) {
      draft.delete(id);
      stagedRow.remove();
    } else {
      draft.set(id, quantity - 1);
      stagedRow.querySelector(".inventory-count").textContent = String(quantity - 1);
    }
    updateDiscardForm();
    return;
  }
  const discardItem = event.target.closest("[data-discard-item]");
  if (discardItem) {
    const id = discardItem.dataset.discardItem;
    const sourceRow = discardItem.closest("tr");
    const draft = window.inventoryDiscardDraft ||= new Map();
    const quantity = draft.get(id) || 0;
    if (quantity >= Number(discardItem.dataset.count)) return;
    draft.set(id, quantity + 1);
    setTradeDraftCount(sourceRow, -(quantity + 1));
    ensureDiscardRow(sourceRow, id).querySelector(".inventory-count").textContent = String(quantity + 1);
    updateDiscardForm();
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
    const available = Number(merchantSell.dataset.count || sourceRow.dataset.inventoryQuantity || sourceRow.querySelector(".inventory-count").dataset.base || sourceRow.querySelector(".inventory-count").textContent.trim());
    const target = Number(sourceRow.dataset.target || merchantSell.dataset.target || 0);
    const mode = merchantSell.dataset.transferMode || "one";
    const amount = Math.max(0, Math.min(available - currentDraft, mode === "all" ? available - currentDraft : mode === "target" ? available - target - currentDraft : 1));
    if (!amount) return;
    sells.set(id, currentDraft + amount);
    (window.merchantSaleDetails ||= new Map()).set(id, { itemId: merchantSell.dataset.itemName, price: Number(merchantSell.dataset.merchantSellPrice) });
    changeTradeDraftCount(sourceRow, -amount);
    changeTradeDraftCount(merchantRow(itemId, document.querySelector(".left-sidebar")), amount);
    updateMerchantGoldDraft();
    updateMerchantOfferForm();
    return;
  }
  const merchantButton = event.target.closest("[data-merchant-buy]");
  if (merchantButton) {
    const draft = window.merchantDraft ||= new Map();
    const item = merchantButton.dataset.merchantBuy;
    const currentDraft = draft.get(item) || 0;
    const activePane = document.querySelector('[data-inventory-pane]:not([hidden])');
    const destination = activePane?.querySelector(`tr[data-merchant-item="${CSS.escape(item)}"]`);
    const current = Number(destination?.dataset.inventoryQuantity || 0);
    const target = Number(destination?.dataset.target || merchantButton.dataset.target || 0);
    const available = Number(merchantButton.dataset.count || 999);
    const mode = merchantButton.dataset.transferMode || "one";
    const amount = Math.max(0, Math.min(available - currentDraft, mode === "all" ? available - currentDraft : mode === "target" ? target - current - currentDraft : 1));
    if (!amount) return;
    draft.set(item, currentDraft + amount);
    (window.merchantBuyPrices ||= new Map()).set(item, Number(merchantButton.dataset.merchantBuyPrice));
    const sourceRow = merchantButton.closest("tr");
    changeTradeDraftCount(sourceRow, -amount);
    changeTradeDraftCount(ensureMerchantPlayerRow(item, sourceRow), amount);
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
  const sourceSidebar = button.closest("aside");
  const targetSidebar = sourceSidebar.classList.contains("left-sidebar") ? document.querySelector(".right-sidebar") : document.querySelector(".left-sidebar");
  let targetRow = targetSidebar.querySelector(`tr[data-item-key="${CSS.escape(button.dataset.key)}"]`);
  const current = Number(targetRow?.querySelector(".inventory-count")?.dataset.base || targetRow?.querySelector(".inventory-count")?.textContent.trim() || 0);
  const available = Number(button.dataset.count); const mode=button.dataset.transferMode||"one";
  const amount=Math.max(0,Math.min(available-entry.quantity,mode==="all"?available-entry.quantity:mode==="target"?Number(button.dataset.target)-current-entry.quantity:1));
  if (!amount) return;
  entry.quantity += amount;
  draft.set(key, entry);
  setTradeDraftCount(button.closest("tr"), -entry.quantity);
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

document.addEventListener("wheel", (event) => {
  const control = event.target.closest("[data-target-control]");
  if (!control) return;
  event.preventDefault();
  const selector = event.deltaY < 0 ? '[data-target-step="1"]' : '[data-target-step="-1"]';
  control.querySelector(selector)?.click();
}, { passive: false });

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", mountInventoryBulkControls, { once: true });
} else {
  mountInventoryBulkControls();
}
