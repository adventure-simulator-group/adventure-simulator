const strategicTradeUi = window.strategicTradeUi ||= { state: {} };
const refreshInventoryPanel = (element) => {
  if (!element) return;
  if (element === document) {
    const grid = document.querySelector(".main-grid");
    ["left", "right"].forEach((side) => {
      const sidebar = grid?.querySelector(`.${side}-sidebar`);
      const hasVisibleBrowser = [...(sidebar?.querySelectorAll("[data-inventory-browser]") || [])]
        .some((browser) => !browser.closest("[hidden]"));
      if (!hasVisibleBrowser) grid?.style.removeProperty(`--inventory-${side}-width`);
    });
  }
  window.strategicInventoryBrowser?.refresh?.(element);
  mountInventoryBulkControls(element);
};

function changeTradeDraftCount(row, change) {
  const count = row.querySelector(".inventory-count");
  count.dataset.base ||= count.textContent.trim();
  setTradeDraftCount(row, Number(count.dataset.tradeDraftChange || 0) + change);
}

function mountInventoryBulkControls(root = document) {
  if (!root?.querySelectorAll) root = document;
  const browsers = root.matches?.("[data-inventory-browser]") ? [root] : [...root.querySelectorAll("[data-inventory-browser]")];
  const closestBrowser = root.closest?.("[data-inventory-browser]");
  if (closestBrowser && !browsers.includes(closestBrowser)) browsers.push(closestBrowser);

  browsers.forEach((browser) => {
    const placeAtStart = Boolean(browser.closest(".right-sidebar"));
    const headerRow = browser.querySelector(".trade-inventory-table thead tr");
    const headerCell = headerRow?.querySelector(":scope > .inventory-actions-header");
    const actionColumn = browser.querySelector(".trade-inventory-table colgroup .inventory-column-actions");
    if (headerCell) headerRow[placeAtStart ? "prepend" : "append"](headerCell);
    if (actionColumn) actionColumn.parentElement[placeAtStart ? "prepend" : "append"](actionColumn);

    browser.querySelectorAll("tbody > tr.trade-inventory-row:not(.inventory-detail-row)").forEach((row) => {
      let cell = row.querySelector(":scope > .inventory-actions-cell");
      if (!cell) {
        cell = document.createElement("td");
        cell.className = "inventory-actions-cell";
        cell.setAttribute("aria-label", "Item actions");
      }
      const actions = row.querySelector(".inventory-row-actions");
      if (actions && actions.parentElement !== cell) cell.append(actions);
      row[placeAtStart ? "prepend" : "append"](cell);
    });
  });

  root.querySelectorAll(".inventory-footer-actions").forEach((actions) => {
    if (actions.closest(".inventory-actions-header")) return;
    const inventoryRegion = actions.closest(".encumbrance-inventory-rail, .smith-wares-scroll, .sidebar-section");
    const headerCell = inventoryRegion?.querySelector("[data-inventory-browser] .inventory-actions-header");
    if (headerCell) headerCell.append(actions);
  });
  applyDynamicTransferModifiers();
}
strategicTradeUi.mountInventoryBulkControls = mountInventoryBulkControls;

function setDynamicTransferButton(button, shiftKey, controlKey) {
  const defaultMode = button.dataset.defaultTransferMode || "one";
  const mode = controlKey ? "all" : shiftKey && defaultMode === "one" ? "target" : defaultMode;
  button.dataset.transferMode = mode;
  const label = button.dataset[`label${mode[0].toUpperCase()}${mode.slice(1)}`];
  if (label) {
    button.title = label;
    button.setAttribute("aria-label", label);
  }
  const glyph = button.querySelector(".inventory-transfer-glyph");
  if (glyph) {
    const count = mode === "all" ? 3 : mode === "target" ? 2 : 1;
    glyph.className = `inventory-transfer-glyph arrows-${count}`;
    glyph.replaceChildren(...Array.from({ length: count }, () => document.createElement("i")));
  }

  const form = button.closest("[data-repair-retrieve-form]");
  if (!form) return;
  const useSingle = mode === "one" && form.dataset.singleAction;
  form.action = useSingle ? form.dataset.singleAction : form.dataset.bulkAction;
  form.querySelectorAll("[name='item_id'], [name='limit']").forEach((input) => { input.disabled = Boolean(useSingle); });
  const limit = form.querySelector("[name='limit']");
  if (limit && !useSingle) limit.value = mode === "all" ? "4294967295" : "2";
}

function applyDynamicTransferModifiers(event) {
  const modifiers = strategicTradeUi.state.transferModifiers ||= { shiftKey: false, controlKey: false };
  if (event) {
    modifiers.shiftKey = event.shiftKey;
    modifiers.controlKey = event.ctrlKey;
  }
  document.querySelectorAll("[data-dynamic-transfer]").forEach((button) => {
    setDynamicTransferButton(button, modifiers.shiftKey, modifiers.controlKey);
  });
}
strategicTradeUi.applyDynamicTransferModifiers = applyDynamicTransferModifiers;

function saveInventoryTarget(control, quantity) {
  const value = control?.querySelector("[data-target-value]");
  const row = control?.closest("tr");
  if (!value || !row) return;

  value.textContent = String(quantity);
  row.dataset.target = String(quantity);
  control.title = `Carrying ${row.dataset.inventoryQuantity || 0}; target ${quantity}`;

  window.strategicFetch("/api/inventory-target", {
    method: "POST",
    body: new URLSearchParams({
      item_id: control.dataset.itemId,
      quantity: String(quantity),
      party_scope: control.dataset.partyScope,
    }),
  });
}

function editInventoryTarget(control) {
  const display = control?.querySelector("[data-target-value]");
  const initialValue = Number(display?.textContent);
  if (!display || !Number.isSafeInteger(initialValue) || !window.StrategicNumericEditor) return;
  window.StrategicNumericEditor.open({
    display,
    initialValue,
    parse(text) {
      const trimmed = String(text).trim();
      if (!/^\d+$/.test(trimmed)) return null;
      const parsed = Number(trimmed);
      return Number.isSafeInteger(parsed) ? parsed : null;
    },
    format: String,
    step: 1,
    minimum: 0,
    maximum: 4294967295,
    anchor: control,
    groupLabel: "Edit target quantity",
    inputLabel: `Target quantity for ${control.dataset.itemId}`,
    increaseLabel: "Increase target quantity by one",
    decreaseLabel: "Decrease target quantity by one",
    saveLabel: "Save target quantity",
    cancelLabel: "Cancel target quantity edit",
    onCommit: (quantity) => saveInventoryTarget(control, quantity),
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
  const inventoryRoot = sidebar?.matches?.(".right-sidebar")
    ? sidebar.querySelector('[data-inventory-pane]:not([hidden])') || sidebar
    : sidebar;
  return inventoryRoot?.querySelector(`tr[data-merchant-item="${CSS.escape(itemId)}"]`) || null;
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
  row.dataset.inventoryQuantity = "0";
  row.dataset.target = sourceRow.querySelector("[data-merchant-buy]")?.dataset.target || "0";
  row.querySelector(".trade-transfer")?.remove();
  const count = row.querySelector(".inventory-count");
  count.textContent = "0";
  delete count.dataset.base;
  delete count.dataset.tradeDraftChange;
  row.classList.remove("party-trade-changed");
  const equipped = document.createElement("td");
  equipped.className = "inventory-equipped";
  equipped.innerHTML = '<input type="checkbox" disabled>';
  (row.querySelector(".inventory-target") || count).after(equipped);
  row.querySelector(".inventory-gold").textContent = sourceRow.dataset.merchantSellPrice;
  row.dataset.generatedMerchantRow = "true";
  const actions = row.querySelector(".inventory-row-actions");
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.className = "trade-transfer trade-transfer-left";
  cancel.dataset.merchantCancelBuy = itemId;
  cancel.dataset.dynamicTransfer = "";
  cancel.dataset.defaultTransferMode = "one";
  cancel.dataset.transferMode = "one";
  cancel.dataset.labelOne = `Cancel one ${itemId}`;
  cancel.dataset.labelTarget = `Cancel ${itemId} down to target`;
  cancel.dataset.labelAll = `Cancel all ${itemId}`;
  cancel.setAttribute("aria-label", cancel.dataset.labelOne);
  cancel.title = cancel.dataset.labelOne;
  cancel.innerHTML = '<span class="inventory-transfer-glyph arrows-1" aria-hidden="true"><i></i></span>';
  actions.append(cancel);
  applyDynamicTransferModifiers();
  playerSidebar.querySelector(".trade-inventory-table tbody").append(row);
  refreshInventoryPanel(playerSidebar);
  return row;
}

function updateMerchantOfferForm() {
  const form = document.querySelector("#merchant-offer");
  form.querySelectorAll("input:not([name='return_to']):not([name='inventory_scope'])").forEach((input) => input.remove());
  const buys = strategicTradeUi.state.merchantDraft || new Map();
  const sells = strategicTradeUi.state.merchantSells || new Map();
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
  strategicTradeUi.state = {
    merchantDraft: new Map(),
    merchantSells: new Map(),
    merchantSaleDetails: new Map(),
    merchantBuyPrices: new Map(),
    partyTradeDraft: new Map(),
    inventoryDiscardDraft: new Map(),
  };
  document.querySelectorAll("[data-generated-discard-row]").forEach((row) => row.remove());
  const discardTable = document.querySelector("[data-discard-table]");
  const discardEmpty = document.querySelector("[data-discard-empty]");
  if (discardTable) discardTable.hidden = true;
  if (discardEmpty) discardEmpty.hidden = false;
  form.querySelectorAll("input:not([name='return_to']):not([name='inventory_scope'])").forEach((input) => input.remove());
  form.hidden = true;
  form.querySelector("[type='submit']").disabled = true;
  window.strategicInventoryBrowser?.refresh?.(document);
}

function updateDiscardForm() {
  const form = document.querySelector("#inventory-discard");
  if (!form) return;
  form.querySelectorAll("input").forEach((input) => input.remove());
  const draft = strategicTradeUi.state.inventoryDiscardDraft ||= new Map();
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
  const staged = [...draft.values()].reduce((total, quantity) => total + quantity, 0);
  const confirmation = form.querySelector("[data-discard-confirmation]");
  if (confirmation) confirmation.textContent = `Permanently discard ${staged} staged item${staged === 1 ? "" : "s"}?`;
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
    action.dataset.labelOne = "Restore one item from the discard list";
    action.dataset.labelTarget = "Restore this discard stack";
    action.dataset.labelAll = "Restore this discard stack";
    action.title = action.dataset.labelOne;
    action.setAttribute("aria-label", action.dataset.labelOne);
  }
  const count = row.querySelector(".inventory-count");
  count.textContent = "0";
  delete count.dataset.base;
  delete count.dataset.tradeDraftChange;
  document.querySelector("[data-discard-table] tbody").append(row);
  applyDynamicTransferModifiers();
  refreshInventoryPanel(row);
  return row;
}

function updateMerchantGoldDraft() {
  const netQuantities = new Map();
  const buys = strategicTradeUi.state.merchantDraft || new Map();
  const sales = strategicTradeUi.state.merchantSells || new Map();
  const saleDetails = strategicTradeUi.state.merchantSaleDetails || new Map();
  const buyPrices = strategicTradeUi.state.merchantBuyPrices || new Map();
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
  const goldRow = merchantRow("coin", document.querySelector(".right-sidebar"));
  if (goldRow) setTradeDraftCount(goldRow, goldChange);
}

function selectMerchantInventoryScope(tab) {
  if (!tab) return false;
  const root = tab.closest("[data-inventory-tabs]");
  if (!root) return false;
  root.querySelectorAll("[data-inventory-tab]").forEach((entry) => entry.classList.toggle("active", entry === tab));
  root.querySelectorAll("[data-inventory-pane]").forEach((pane) => { pane.hidden = pane.dataset.inventoryPane !== tab.dataset.inventoryTab; });
  const scope = document.querySelector("#merchant-offer [name='inventory_scope']");
  if (scope) scope.value = tab.dataset.inventoryTab;
  resetTradeDraft(document.querySelector("#merchant-offer"));
  refreshInventoryPanel(root.querySelector('[data-inventory-pane]:not([hidden])'));
  return true;
}

document.addEventListener("click", (event) => {
  const dynamicTransfer = event.target.closest?.("[data-dynamic-transfer]");
  const clickTarget = dynamicTransfer || event.target;
  if (event.isTrusted && dynamicTransfer) {
    applyDynamicTransferModifiers(event);
  }
  const tab = clickTarget.closest("[data-inventory-tab]");
  if (tab) {
    selectMerchantInventoryScope(tab);
    return;
  }
  const targetValue = clickTarget.closest("[data-target-value]");
  if (targetValue) {
    event.preventDefault();
    editInventoryTarget(targetValue.closest("[data-target-control]"));
    return;
  }
  const bulk = clickTarget.closest("[data-inventory-bulk]");
  if (bulk) {
    const panel = bulk.closest("aside, section, .sidebar-section") || document;
    const selector = bulk.dataset.inventoryBulk === "buy" ? "[data-merchant-buy]" : bulk.dataset.inventoryBulk === "loot" ? "[data-loot-stage]" : ["deposit", "withdraw"].includes(bulk.dataset.inventoryBulk) ? `[data-pool-stage][data-pool-direction="${bulk.dataset.inventoryBulk}"]` : bulk.dataset.inventoryBulk.startsWith("party-") ? ".party-draft-transfer" : "[data-merchant-sell]";
    panel.querySelectorAll(`${selector}[data-transfer-mode="${bulk.dataset.transferMode}"]`).forEach((button) => button.click());
    return;
  }
  const cancelLoot = clickTarget.closest("[data-cancel-loot]");
  if (cancelLoot) {
    strategicTradeUi.state.lootTransferDraft = new Map();
    document.querySelectorAll("[data-loot-row]").forEach((row) => setTradeDraftCount(row, 0));
    const form = cancelLoot.closest("form"); form.querySelectorAll("input").forEach((input) => input.remove()); form.hidden = true;
    const prompt = form.querySelector("[data-loot-transfer-prompt]");
    if (prompt) prompt.textContent = "Apply staged loot to the party inventory?";
    document.dispatchEvent(new Event("strategic-live-refresh-requested"));
    return;
  }
  const lootStage = clickTarget.closest("[data-loot-stage]");
  if (lootStage) {
    const row = lootStage.closest("tr");
    const draft = strategicTradeUi.state.lootTransferDraft ||= new Map();
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
    const prompt = form.querySelector("[data-loot-transfer-prompt]");
    const total = [...draft.values()].reduce((sum, quantity) => sum + quantity, 0);
    if (prompt) prompt.textContent = `Apply ${total} staged item${total === 1 ? "" : "s"} to the party inventory?`;
    form.hidden = false; form.querySelector('[type="submit"]').disabled = false;
    return;
  }
  const cancelPool = clickTarget.closest("[data-cancel-pool]");
  if (cancelPool) { strategicTradeUi.state.poolTransferDraft = new Map(); document.querySelectorAll("[data-pool-stage]").forEach((button) => setTradeDraftCount(button.closest("tr"), 0)); const form=cancelPool.closest("form"); form.querySelectorAll("input").forEach((input)=>input.remove()); form.hidden=true; document.dispatchEvent(new Event("strategic-live-refresh-requested")); return; }
  const poolStage = clickTarget.closest("[data-pool-stage]");
  if (poolStage) {
    const form = document.querySelector("#pool-transfer-offer");
    if (form.dataset.direction && form.dataset.direction !== poolStage.dataset.poolDirection) { strategicTradeUi.state.poolTransferDraft = new Map(); document.querySelectorAll("[data-pool-stage]").forEach((button) => setTradeDraftCount(button.closest("tr"), 0)); }
    form.dataset.direction = poolStage.dataset.poolDirection;
    form.action = `${location.pathname}/${poolStage.dataset.poolDirection}`;
    const draft = strategicTradeUi.state.poolTransferDraft ||= new Map(); const id=poolStage.dataset.poolStage; const staged=draft.get(id)||0;
    const available=Number(poolStage.dataset.count); const mode=poolStage.dataset.transferMode||"one";
    const amount=Math.max(0,Math.min(available-staged,mode==="all"?available-staged:mode==="target"?Number(poolStage.dataset.target)-Number(poolStage.dataset.current)-staged:1));
    if (!amount) return; draft.set(id,staged+amount); setTradeDraftCount(poolStage.closest("tr"),-(staged+amount));
    form.querySelectorAll("input").forEach((input)=>input.remove());
    for (const [name, values] of Object.entries({item_id:[...draft.keys()],quantity:[...draft.values()]})) { const input=document.createElement("input");input.type="hidden";input.name=name;input.value=values.join(",");form.append(input); }
    form.hidden=false;form.querySelector('[type="submit"]').disabled=false;return;
  }
  const cancelTrade = clickTarget.closest("[data-cancel-trade]");
  if (cancelTrade) {
    resetTradeDraft(cancelTrade.closest("form"));
    document.dispatchEvent(new Event("strategic-live-refresh-requested"));
    return;
  }
  const unstageDiscard = clickTarget.closest("[data-unstage-discard]");
  if (unstageDiscard) {
    const id = unstageDiscard.dataset.unstageDiscard;
    const draft = strategicTradeUi.state.inventoryDiscardDraft ||= new Map();
    const stagedRow = unstageDiscard.closest("tr");
    const quantity = draft.get(id) || Number(stagedRow.querySelector(".inventory-count").textContent) || 0;
    if (!quantity) return;
    const sourceRow = document.querySelector(`[data-discard-source="${CSS.escape(id)}"]`);
    const mode = unstageDiscard.dataset.transferMode || "one";
    const amount = mode === "one" ? 1 : quantity;
    if (sourceRow) setTradeDraftCount(sourceRow, -(quantity - amount));
    if (amount >= quantity) {
      draft.delete(id);
      stagedRow.remove();
    } else {
      draft.set(id, quantity - amount);
      stagedRow.querySelector(".inventory-count").textContent = String(quantity - amount);
    }
    refreshInventoryPanel(sourceRow || document.querySelector("[data-discard-table]"));
    refreshInventoryPanel(document.querySelector("[data-discard-table]"));
    updateDiscardForm();
    return;
  }
  const discardItem = clickTarget.closest("[data-discard-item]");
  if (discardItem) {
    const id = discardItem.dataset.discardItem;
    const sourceRow = discardItem.closest("tr");
    const draft = strategicTradeUi.state.inventoryDiscardDraft ||= new Map();
    const quantity = draft.get(id) || 0;
    const available = Number(discardItem.dataset.count);
    if (quantity >= available) return;
    const mode = discardItem.dataset.transferMode || "one";
    const amount = mode === "one" ? 1 : available - quantity;
    draft.set(id, quantity + amount);
    setTradeDraftCount(sourceRow, -(quantity + amount));
    const stagedRow = ensureDiscardRow(sourceRow, id);
    stagedRow.querySelector(".inventory-count").textContent = String(quantity + amount);
    refreshInventoryPanel(sourceRow);
    refreshInventoryPanel(stagedRow);
    updateDiscardForm();
    return;
  }
  const cancelBuy = clickTarget.closest("[data-merchant-cancel-buy]");
  if (cancelBuy) {
    const itemId = cancelBuy.dataset.merchantCancelBuy;
    const buys = strategicTradeUi.state.merchantDraft ||= new Map();
    const quantity = buys.get(itemId) || 0;
    if (quantity === 0) return;
    const playerRow = cancelBuy.closest("tr");
    const mode = cancelBuy.dataset.transferMode || "one";
    const current = Number(playerRow.dataset.inventoryQuantity || 0);
    const target = Number(playerRow.dataset.target || 0);
    const amount = mode === "all" ? quantity : mode === "target" ? Math.max(0, quantity - Math.max(0, target - current)) : 1;
    if (!amount) return;
    if (amount >= quantity) buys.delete(itemId); else buys.set(itemId, quantity - amount);
    changeTradeDraftCount(playerRow, -amount);
    const stockRow = merchantRow(itemId, document.querySelector(".left-sidebar"));
    changeTradeDraftCount(stockRow, amount);
    if (playerRow.dataset.generatedMerchantRow === "true" && Number(playerRow.querySelector(".inventory-count").dataset.tradeDraftChange || 0) === 0) playerRow.remove();
    refreshInventoryPanel(stockRow);
    refreshInventoryPanel(document.querySelector('[data-inventory-pane]:not([hidden])'));
    updateMerchantGoldDraft();
    updateMerchantOfferForm();
    return;
  }
  const merchantSell = clickTarget.closest("[data-merchant-sell]");
  if (merchantSell) {
    const itemId = merchantSell.dataset.itemName;
    const buys = strategicTradeUi.state.merchantDraft ||= new Map();
    const pendingPurchase = buys.get(itemId) || 0;
    const sourceRow = merchantSell.closest("tr");
    if (pendingPurchase > 0) {
      if (pendingPurchase === 1) buys.delete(itemId); else buys.set(itemId, pendingPurchase - 1);
      changeTradeDraftCount(sourceRow, -1);
      const stockRow = merchantRow(itemId, document.querySelector(".left-sidebar"));
      changeTradeDraftCount(stockRow, 1);
      refreshInventoryPanel(sourceRow); refreshInventoryPanel(stockRow);
      updateMerchantGoldDraft();
      updateMerchantOfferForm();
      return;
    }
    const sells = strategicTradeUi.state.merchantSells ||= new Map(); const id = merchantSell.dataset.merchantSell;
    const currentDraft = sells.get(id) || 0;
    const available = Number(merchantSell.dataset.count || sourceRow.dataset.inventoryQuantity || sourceRow.querySelector(".inventory-count").dataset.base || sourceRow.querySelector(".inventory-count").textContent.trim());
    const target = Number(sourceRow.dataset.target || merchantSell.dataset.target || 0);
    const mode = merchantSell.dataset.transferMode || "one";
    const amount = Math.max(0, Math.min(available - currentDraft, mode === "all" ? available - currentDraft : mode === "target" ? available - target - currentDraft : 1));
    if (!amount) return;
    sells.set(id, currentDraft + amount);
    (strategicTradeUi.state.merchantSaleDetails ||= new Map()).set(id, { itemId: merchantSell.dataset.itemName, price: Number(merchantSell.dataset.merchantSellPrice) });
    changeTradeDraftCount(sourceRow, -amount);
    const stockRow = merchantRow(itemId, document.querySelector(".left-sidebar"));
    if (stockRow) changeTradeDraftCount(stockRow, amount);
    refreshInventoryPanel(sourceRow); refreshInventoryPanel(stockRow);
    updateMerchantGoldDraft();
    updateMerchantOfferForm();
    return;
  }
  const merchantButton = clickTarget.closest("[data-merchant-buy]");
  if (merchantButton) {
    const draft = strategicTradeUi.state.merchantDraft ||= new Map();
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
    (strategicTradeUi.state.merchantBuyPrices ||= new Map()).set(item, Number(merchantButton.dataset.merchantBuyPrice));
    const sourceRow = merchantButton.closest("tr");
    changeTradeDraftCount(sourceRow, -amount);
    const playerRow = ensureMerchantPlayerRow(item, sourceRow);
    changeTradeDraftCount(playerRow, amount);
    refreshInventoryPanel(playerRow);
    refreshInventoryPanel(sourceRow);
    updateMerchantGoldDraft();
    updateMerchantOfferForm();
    return;
  }
  const button = clickTarget.closest(".party-draft-transfer");
  if (!button) return;
  const key = button.dataset.item;
  const draft = strategicTradeUi.state.partyTradeDraft ||= new Map();
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
  refreshInventoryPanel(targetRow);
  refreshInventoryPanel(button.closest("tr"));
  const form = document.querySelector("#party-offer");
  form.querySelectorAll("input").forEach((input) => input.remove());
  const fields = { from_character_ids: [], to_character_ids: [], inventory_item_ids: [], quantities: [] };
  draft.forEach((value, item) => { fields.from_character_ids.push(value.from); fields.to_character_ids.push(value.to); fields.inventory_item_ids.push(item); fields.quantities.push(value.quantity); });
  Object.entries(fields).forEach(([name, values]) => { const input = document.createElement("input"); input.type = "hidden"; input.name = name; input.value = values.join(","); form.append(input); });
  form.hidden = false;
  form.querySelector("[type='submit']").disabled = false;
});

document.addEventListener("keydown", (event) => {
  const targetValue = event.target.closest?.("[data-target-value]");
  if (targetValue && (event.key === "Enter" || event.key === " ")) {
    event.preventDefault();
    editInventoryTarget(targetValue.closest("[data-target-control]"));
    return;
  }
  if (event.key === "Shift" || event.key === "Control") applyDynamicTransferModifiers(event);
});

function initializeProvisioningDraft() {
  const params = new URLSearchParams(window.location.search);
  const parseQuantity = (name) => {
    const value = params.get(name);
    if (!/^(?:0|[1-9]\d{0,9})$/.test(value || "")) return 0;
    const quantity = Number(value);
    return Number.isSafeInteger(quantity) && quantity <= 4294967295 ? quantity : 0;
  };
  const requested = new Map([
    ["travel_ration", parseQuantity("provision_rations")],
    ["waterskin", parseQuantity("provision_waterskins")],
  ].filter(([, quantity]) => quantity > 0));
  if (!requested.size || params.get("inventory_scope") !== "party") return;

  const partyTab = document.querySelector('[data-inventory-tab="party"]');
  if (!selectMerchantInventoryScope(partyTab)) return;
  const draft = strategicTradeUi.state.merchantDraft ||= new Map();
  let stagedFoodRow = null;
  requested.forEach((quantity, itemId) => {
    const source = merchantRow(itemId, document.querySelector(".left-sidebar"));
    const button = source?.querySelector("[data-merchant-buy]");
    if (!source || !button) return;
    draft.set(itemId, quantity);
    (strategicTradeUi.state.merchantBuyPrices ||= new Map()).set(itemId, Number(button.dataset.merchantBuyPrice));
    setTradeDraftCount(source, -quantity);
    setTradeDraftCount(ensureMerchantPlayerRow(itemId, source), quantity);
    if (itemId === "travel_ration") stagedFoodRow = source;
  });
  const foodParent = stagedFoodRow?.closest("tbody")?.querySelector(":scope > .food-parent-row");
  if (foodParent?.getAttribute("aria-expanded") !== "true") {
    foodParent?.querySelector("[data-food-toggle]")?.click();
  }
  updateMerchantGoldDraft();
  updateMerchantOfferForm();
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", initializeProvisioningDraft, { once: true });
} else {
  initializeProvisioningDraft();
}
document.addEventListener("keyup", (event) => {
  if (event.key === "Shift" || event.key === "Control") applyDynamicTransferModifiers(event);
});
window.addEventListener("blur", () => {
  strategicTradeUi.state.transferModifiers = { shiftKey: false, controlKey: false };
  applyDynamicTransferModifiers();
});

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", () => mountInventoryBulkControls(), { once: true });
} else {
  mountInventoryBulkControls();
}
document.addEventListener("strategic-live-regions-refreshed", () => refreshInventoryPanel(document));
document.addEventListener("strategic-page-mounted", () => {
  initializeProvisioningDraft();
  mountInventoryBulkControls();
  refreshInventoryPanel(document);
});
