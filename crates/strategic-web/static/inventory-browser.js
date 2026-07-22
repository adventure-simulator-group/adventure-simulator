(function inventoryBrowserModule(global) {
  "use strict";

  const OPTIONAL_COLUMNS = {
    accuracy: ["Precision", "accuracy"], reach: ["Reach m", "reach"],
    penetration: ["Penetration", "penetration"], damage: ["Damage types", "damage"],
    block: ["Block", "block"], coverage: ["Coverage", "coverage"],
    resistance: ["Resistance J", "resistance"], padding: ["Padding J", "padding"],
    flexibility: ["Flexibility", "flexibility"],
    "range-of-motion": ["Range of motion", "rangeOfMotion"],
  };
  const DETAIL_ICONS = {
    Slot: "knapsack",
    Balance: "scales",
    Mode: "crossed-swords",
    Precision: "bullseye",
    "Reach m": "spear-hook",
    Penetration: "piercing-sword",
    "Damage types": "saber-slash",
    Block: "shield",
    Coverage: "armor-vest",
    "Resistance J": "bordered-shield",
    "Padding J": "layered-armor",
    Flexibility: "dodge",
    "Range of motion": "acrobatic",
  };
  const VALID_SORTS = new Set(["type", "name", "quantity", "target", "equipped", "durability", "weight", "value", ...Object.keys(OPTIONAL_COLUMNS)]);

  const prefix = (namespace) => `inv.${namespace}.`;
  function parsePanelState(search, namespace, available = []) {
    const params = new URLSearchParams(search);
    const p = prefix(namespace);
    const sort = params.get(`${p}sort`);
    const direction = params.get(`${p}dir`);
    const allowed = new Set(available);
    return {
      query: params.get(`${p}q`) || "",
      sort: VALID_SORTS.has(sort) ? sort : "name",
      direction: direction === "desc" ? "desc" : "asc",
      columns: (params.get(`${p}cols`) || "").split(",").filter((column) => allowed.has(column)),
    };
  }

  function serializePanelState(search, namespace, state) {
    const params = new URLSearchParams(search);
    const p = prefix(namespace);
    [["q", state.query], ["sort", state.sort], ["dir", state.direction], ["cols", state.columns.join(",")]].forEach(([key, value]) => {
      if (value && !(key === "sort" && value === "name") && !(key === "dir" && value === "asc")) params.set(`${p}${key}`, value);
      else params.delete(`${p}${key}`);
    });
    const encoded = params.toString();
    return encoded ? `?${encoded}` : "";
  }

  function compareValues(left, right, direction = "asc") {
    const blankLeft = left === "" || left == null || Number.isNaN(left);
    const blankRight = right === "" || right == null || Number.isNaN(right);
    if (blankLeft !== blankRight) return blankLeft ? 1 : -1;
    if (blankLeft) return 0;
    const result = typeof left === "number" && typeof right === "number"
      ? left - right
      : String(left).localeCompare(String(right), undefined, { sensitivity: "base", numeric: true });
    return direction === "desc" ? -result : result;
  }

  const textNumber = (text) => {
    const trimmed = String(text || "").trim();
    if (!trimmed || trimmed === "—") return "";
    const number = Number(trimmed.replace(/[^0-9+.-]/g, ""));
    return Number.isFinite(number) ? number : trimmed;
  };
  function normalizeSortValue(value, type = "number") {
    if (type === "text") return String(value || "").trim();
    return textNumber(value);
  }
  function rowValue(row, key) {
    const label = row.querySelector?.("[data-item-name]");
    if (key === "name") return label?.dataset.itemName || label?.textContent.trim() || "";
    if (key === "type") return label?.dataset.itemKind || "";
    if (OPTIONAL_COLUMNS[key]) {
      const value = label?.dataset[`stat${OPTIONAL_COLUMNS[key][1][0].toUpperCase()}${OPTIONAL_COLUMNS[key][1].slice(1)}`];
      return normalizeSortValue(value, key === "damage" ? "text" : "number");
    }
    const selectors = { quantity: ".inventory-count", target: ".inventory-target-value", equipped: ".inventory-equipped input", durability: ".inventory-durability", weight: ".inventory-weight", value: ".inventory-gold" };
    if (key === "equipped") return row.querySelector(selectors[key])?.checked ? 1 : 0;
    const cell = row.querySelector(selectors[key]);
    if (key === "target") return normalizeSortValue(cell?.textContent || row.querySelector(".inventory-target")?.textContent);
    if (key === "durability") return normalizeSortValue(cell?.querySelector("[data-sort-value]")?.dataset.sortValue || cell?.dataset.sortValue || cell?.textContent);
    return normalizeSortValue(cell?.dataset.sortValue || cell?.textContent);
  }

  function ensureQuantityTargetSplit(row) {
    const count = row.querySelector(":scope > .inventory-count");
    if (!count || row.querySelector(":scope > .inventory-target")) return;
    const control = count.querySelector("[data-target-control]");
    const target = document.createElement("td");
    target.className = "inventory-target";
    if (control) {
      target.append(control);
      const quantity = control.dataset.quantity || row.dataset.inventoryQuantity || "0";
      row.dataset.inventoryQuantity = quantity;
      count.textContent = quantity;
    } else {
      const targetValue = row.dataset.target || row.querySelector("[data-target]")?.dataset.target;
      target.textContent = targetValue == null ? "—" : targetValue;
    }
    count.after(target);
  }

  function normalizeDestinationRow(row, browser) {
    const wantsQuantities = Boolean(browser.querySelector("thead .inventory-column-count"));
    if (wantsQuantities) {
      const count = row.querySelector(":scope > .inventory-count");
      if (count) count.hidden = false;
      ensureQuantityTargetSplit(row);
    }
    else {
      const count = row.querySelector(":scope > .inventory-count");
      if (count) count.hidden = true;
      row.querySelector(":scope > .inventory-target")?.remove();
    }
    const target = row.querySelector(":scope > .inventory-target");
    let cursor = target || row.querySelector(":scope > .inventory-item-name");
    const wantsEquipped = Boolean(browser.querySelector("thead .inventory-column-equipped"));
    let equipped = row.querySelector(":scope > .inventory-equipped");
    if (!wantsEquipped && equipped) { equipped.remove(); equipped = null; }
    if (wantsEquipped && !equipped) {
      equipped = document.createElement("td"); equipped.className = "inventory-equipped";
      equipped.innerHTML = '<input type="checkbox" disabled aria-label="Generated item is not equipped">';
      cursor?.after(equipped);
    }
    if (equipped && (row.dataset.generatedOfferRow === "true" || row.dataset.generatedMerchantRow === "true")) {
      const toggle = equipped.querySelector("input");
      if (toggle) { toggle.disabled = true; delete toggle.dataset.equipmentToggle; toggle.setAttribute("aria-label", "Equipment can be changed after applying the transfer"); }
    }
    if (equipped) cursor = equipped;
    const wantsDurability = Boolean(browser.querySelector("thead .inventory-column-durability"));
    let durability = row.querySelector(":scope > .inventory-durability");
    if (!wantsDurability && durability) { durability.remove(); durability = null; }
    if (wantsDurability && !durability) {
      durability = document.createElement("td"); durability.className = "inventory-durability"; durability.textContent = "—";
      cursor?.after(durability);
    }
  }

  function optionalCell(row, column) {
    let cell = row.querySelector(`[data-inventory-column="${column}"]`);
    if (cell) return cell;
    cell = document.createElement("td");
    cell.dataset.inventoryColumn = column;
    const dataKey = OPTIONAL_COLUMNS[column][1];
    const property = `stat${dataKey[0].toUpperCase()}${dataKey.slice(1)}`;
    const label = row.querySelector("[data-item-name]");
    const kind = label?.dataset.itemKind;
    const weaponColumn = ["accuracy", "reach", "penetration", "damage", "block"].includes(column);
    const applicable = weaponColumn ? ["weapon", "shield"].includes(kind) : kind === "armor";
    cell.textContent = applicable ? (label?.dataset[property] || "—") : "—";
    const actionCell = row.querySelector(":scope > .inventory-actions-cell");
    if (actionCell && actionCell === row.lastElementChild) row.insertBefore(cell, actionCell);
    else row.append(cell);
    return cell;
  }

  function updateHistory(browser, state, replace = false) {
    const query = serializePanelState(global.location.search, browser.dataset.inventoryBrowser, state);
    const url = `${global.location.pathname}${query}${global.location.hash}`;
    global.history[replace ? "replaceState" : "pushState"]({}, "", url);
  }

  function syncPanelWidth(browser) {
    if (!global.getComputedStyle || browser.closest("[hidden]")) return;
    const aside = browser.closest(".left-sidebar, .right-sidebar");
    const grid = aside?.closest(".main-grid");
    if (!aside || !grid) return;
    const styles = global.getComputedStyle(aside);
    const frameWidth = (Number.parseFloat(styles.paddingLeft) || 0) + (Number.parseFloat(styles.paddingRight) || 0);
    const table = browser.querySelector(".trade-inventory-table");
    const tableWidth = table?.getBoundingClientRect?.().width || table?.clientWidth || 0;
    const browserWidth = browser.getBoundingClientRect?.().width || browser.clientWidth || 0;
    const contentWidth = Math.ceil(Math.max(browserWidth, tableWidth));
    const side = aside.classList.contains("left-sidebar") ? "left" : "right";
    grid.style.setProperty(`--inventory-${side}-width`, `${contentWidth + frameWidth}px`);
  }

  function currencyNumber(cell) {
    const value = Number(String(cell?.dataset.sortValue || cell?.textContent || "0").replace(/[^0-9+.-]/g, ""));
    return Number.isFinite(value) ? value : 0;
  }

  function currencyRowQuantity(row) {
    const count = row.querySelector(".inventory-count");
    const base = Number(row.dataset.inventoryQuantity
      ?? count?.dataset.base
      ?? row.querySelector("[data-target-control]")?.dataset.quantity
      ?? row.querySelector("[data-count]")?.dataset.count
      ?? currencyNumber(count));
    const change = Number(count?.dataset.tradeDraftChange || 0);
    return Math.max(0, (Number.isFinite(base) ? base : 0) + (Number.isFinite(change) ? change : 0));
  }

  function currencyRowTarget(row) {
    return Math.max(0, Number(row.dataset.target
      ?? row.querySelector("[data-target-value]")?.textContent
      ?? row.querySelector("[data-target]")?.dataset.target
      ?? 0) || 0);
  }

  function groupCurrencyRows(browser) {
    const body = browser.querySelector("tbody");
    if (!body) return;
    const previousParent = body.querySelector(":scope > .currency-parent-row");
    const wasExpanded = previousParent?.getAttribute("aria-expanded") === "true";
    const parentDraftChange = Number(previousParent?.querySelector(".inventory-count")?.dataset.tradeDraftChange || 0);
    previousParent?.remove();
    const components = [...body.querySelectorAll(":scope > tr.trade-inventory-row")]
      .filter((row) => !row.classList.contains("currency-parent-row") && row.querySelector('[data-item-kind="currency"]'));
    if (!components.length) return;
    const first = components[0];
    const parent = first.cloneNode(true);
    parent.classList.add("currency-parent-row");
    parent.classList.remove("currency-component-row");
    parent.dataset.itemKey = "coin";
    parent.dataset.merchantItem = "coin";
    const labels = parent.querySelectorAll("[data-item-name]");
    labels.forEach((label) => { label.dataset.itemName = "Coin"; label.textContent = "Coin"; delete label.dataset.currencyName; });
    const componentTotal = components.reduce((sum, row) => sum + currencyRowQuantity(row), 0);
    const total = Math.max(0, componentTotal + (Number.isFinite(parentDraftChange) ? parentDraftChange : 0));
    const target = components.reduce((sum, row) => sum + currencyRowTarget(row), 0);
    const componentWeight = components.reduce((sum, row) => {
      const quantity = currencyRowQuantity(row);
      return sum + quantity * currencyNumber(row.querySelector(".inventory-weight"));
    }, 0);
    const unitWeight = currencyNumber(first.querySelector(".inventory-weight"));
    const weight = Math.max(0, componentWeight + parentDraftChange * unitWeight);
    parent.querySelector("[data-target-control]")?.remove();
    const count = parent.querySelector(".inventory-count");
    if (count) {
      count.textContent = String(total);
      count.dataset.base = String(componentTotal);
      if (parentDraftChange) count.dataset.tradeDraftChange = String(parentDraftChange);
      else delete count.dataset.tradeDraftChange;
    }
    parent.dataset.inventoryQuantity = String(componentTotal);
    parent.dataset.target = String(target);
    const targetCell = parent.querySelector(":scope > .inventory-target");
    if (targetCell) targetCell.textContent = String(target);
    const weightCell = parent.querySelector(".inventory-weight");
    if (weightCell) { weightCell.textContent = weight.toFixed(2).replace(/\.00$/, ""); weightCell.dataset.sortValue = String(weight); }
    const valueCell = parent.querySelector(".inventory-gold");
    if (valueCell) { valueCell.textContent = String(total); valueCell.dataset.sortValue = String(total); }
    parent.querySelectorAll("button,input,select").forEach((control) => {
      if (control.matches("button.trade-transfer")) {
        ["data-item", "data-item-name", "data-key", "data-from", "data-to", "data-discard-item", "data-pool-stage", "data-loot-stage", "data-merchant-sell"].forEach((attribute) => control.removeAttribute(attribute));
        control.dataset.coinAction = "true";
        control.dataset.count = String(total);
        control.dataset.target = String(target);
        control.dataset.labelOne = "Transfer one Coin";
        control.dataset.labelTarget = "Transfer Coin to target";
        control.dataset.labelAll = "Transfer all Coin";
        control.setAttribute("aria-label", "Transfer Coin");
        control.title = "Transfer Coin";
      } else if (!control.matches('[data-coin-toggle]')) control.remove();
    });
    const nameCell = parent.querySelector(".inventory-item-name");
    if (nameCell) {
      const toggle = document.createElement("button");
      toggle.type = "button"; toggle.className = "currency-disclosure"; toggle.dataset.coinToggle = "true";
      toggle.setAttribute("aria-label", "Show currency denominations"); toggle.setAttribute("aria-expanded", "false");
      toggle.textContent = "›";
      const coinLabel = nameCell.querySelector("[data-item-name]");
      if (coinLabel) coinLabel.after(toggle);
      else nameCell.append(toggle);
    }
    parent.querySelectorAll(".game-icon").forEach((icon) => {
      icon.setAttribute("aria-label", "Item type: Coin");
      icon.setAttribute("title", "Item type: Coin");
    });
    components.forEach((row) => {
      row.classList.add("currency-component-row"); row.hidden = true; row.tabIndex = -1;
      const label = row.querySelector("[data-currency-name]");
      if (label) { label.textContent = label.dataset.currencyName; label.dataset.itemName = label.dataset.currencyName; }
      const componentCount = row.querySelector(".inventory-count");
      if (componentCount) componentCount.textContent = String(currencyRowQuantity(row));
    });
    first.before(parent);
    parent._currencyComponents = components;
    parent.setAttribute("aria-expanded", String(Boolean(wasExpanded)));
    parent.querySelector("[data-coin-toggle]")?.setAttribute("aria-expanded", String(Boolean(wasExpanded)));
    components.forEach((component) => { component.hidden = !wasExpanded; });
  }

  function groupAlcoholRows(browser) {
    const body = browser.querySelector("tbody");
    if (!body) return;
    const previousParent = body.querySelector(":scope > .alcohol-parent-row");
    const wasExpanded = previousParent?.getAttribute("aria-expanded") === "true";
    previousParent?.remove();
    const components = [...body.querySelectorAll(":scope > tr.trade-inventory-row")]
      .filter((row) => !row.classList.contains("alcohol-parent-row") && row.querySelector('[data-item-group="alcohol"]'));
    if (!components.length) return;
    const first = components[0];
    const parent = first.cloneNode(true);
    parent.classList.add("alcohol-parent-row");
    parent.classList.remove("alcohol-component-row");
    parent.dataset.itemKey = "alcohol";
    parent.dataset.merchantItem = "alcohol";
    parent.querySelectorAll("[data-item-name]").forEach((label) => {
      label.dataset.itemName = "Alcohol";
      label.textContent = "Alcohol";
      delete label.dataset.itemGroup;
      delete label.dataset.groupName;
    });
    const total = components.reduce((sum, row) => sum + currencyRowQuantity(row), 0);
    const target = components.reduce((sum, row) => sum + currencyRowTarget(row), 0);
    const totalWeight = components.reduce((sum, row) => sum
      + currencyRowQuantity(row) * currencyNumber(row.querySelector(".inventory-weight")), 0);
    const totalValue = components.reduce((sum, row) => sum
      + currencyRowQuantity(row) * currencyNumber(row.querySelector(".inventory-gold")), 0);
    parent.querySelector("[data-target-control]")?.remove();
    const count = parent.querySelector(".inventory-count");
    if (count) count.textContent = String(total);
    const showsQuantity = !components.every((row) => row.dataset.groupSummary === "catalog");
    if (count && !showsQuantity) {
      count.hidden = true;
      count.setAttribute("hidden", "");
    }
    parent.dataset.inventoryQuantity = String(total);
    parent.dataset.target = String(target);
    const targetCell = parent.querySelector(":scope > .inventory-target");
    if (targetCell) targetCell.textContent = String(target);
    const weightCell = parent.querySelector(".inventory-weight");
    const valueCell = parent.querySelector(".inventory-gold");
    if (showsQuantity) {
      if (weightCell) { weightCell.textContent = totalWeight.toFixed(2).replace(/\.00$/, ""); weightCell.dataset.sortValue = String(totalWeight); }
      if (valueCell) { valueCell.textContent = String(totalValue); valueCell.dataset.sortValue = String(totalValue); }
    } else {
      if (weightCell) { weightCell.textContent = "—"; weightCell.dataset.sortValue = ""; }
      if (valueCell) { valueCell.textContent = "—"; valueCell.dataset.sortValue = ""; }
    }
    parent.querySelectorAll("button,input,select").forEach((control) => control.remove());
    const nameCell = parent.querySelector(".inventory-item-name");
    if (nameCell) {
      const toggle = document.createElement("button");
      toggle.type = "button";
      toggle.className = "currency-disclosure";
      toggle.dataset.alcoholToggle = "true";
      toggle.setAttribute("aria-label", "Show alcohol types");
      toggle.setAttribute("aria-expanded", "false");
      toggle.textContent = "›";
      const label = nameCell.querySelector("[data-item-name]");
      if (label) label.after(toggle);
      else nameCell.append(toggle);
    }
    parent.querySelectorAll(".game-icon").forEach((icon) => {
      icon.setAttribute("aria-label", "Item type: Alcohol");
      icon.setAttribute("title", "Item type: Alcohol");
    });
    components.forEach((row) => {
      row.classList.add("alcohol-component-row");
      row.hidden = true;
      row.tabIndex = -1;
    });
    first.before(parent);
    parent._alcoholComponents = components;
    parent.setAttribute("aria-expanded", String(Boolean(wasExpanded)));
    parent.querySelector("[data-alcohol-toggle]")?.setAttribute("aria-expanded", String(Boolean(wasExpanded)));
    components.forEach((component) => { component.hidden = !wasExpanded; });
  }

  function groupFoodRows(browser) {
    const body = browser.querySelector("tbody");
    if (!body) return;
    const previousParent = body.querySelector(":scope > .food-parent-row");
    const wasExpanded = previousParent?.getAttribute("aria-expanded") === "true";
    previousParent?.remove();
    const components = [...body.querySelectorAll(":scope > tr.trade-inventory-row")]
      .filter((row) => !row.classList.contains("food-parent-row") && row.querySelector('[data-item-kind="food"]'));
    if (!components.length) return;
    const first = components[0];
    const parent = first.cloneNode(true);
    parent.classList.add("food-parent-row");
    parent.classList.remove("food-component-row");
    parent.dataset.itemKey = "food";
    parent.dataset.merchantItem = "food";
    parent.querySelectorAll("[data-item-name]").forEach((label) => {
      label.dataset.itemName = "Food";
      label.textContent = "Food";
    });
    const total = components.reduce((sum, row) => sum + currencyRowQuantity(row), 0);
    const totalWeight = components.reduce((sum, row) => sum
      + currencyRowQuantity(row) * currencyNumber(row.querySelector(".inventory-weight")), 0);
    const totalValue = components.reduce((sum, row) => sum
      + currencyRowQuantity(row) * currencyNumber(row.querySelector(".inventory-gold")), 0);
    parent.querySelectorAll("button,input,select").forEach((control) => control.remove());
    const count = parent.querySelector(".inventory-count");
    const weight = parent.querySelector(".inventory-weight");
    const value = parent.querySelector(".inventory-gold");
    if (count) count.textContent = String(total);
    if (weight) { weight.textContent = totalWeight.toFixed(2).replace(/\.00$/, ""); weight.dataset.sortValue = String(totalWeight); }
    if (value) { value.textContent = String(totalValue); value.dataset.sortValue = String(totalValue); }
    const nameCell = parent.querySelector(".inventory-item-name");
    if (nameCell) {
      const toggle = document.createElement("button");
      toggle.type = "button";
      toggle.className = "currency-disclosure";
      toggle.dataset.foodToggle = "true";
      toggle.textContent = "›";
      toggle.setAttribute("aria-label", "Show food lots");
      toggle.setAttribute("aria-expanded", String(Boolean(wasExpanded)));
      const label = nameCell.querySelector("[data-item-name]");
      if (label) label.after(toggle);
      else nameCell.append(toggle);
    }
    components.forEach((row) => {
      row.classList.add("food-component-row");
      row.hidden = true;
      row.tabIndex = -1;
    });
    first.before(parent);
    parent._foodComponents = components;
    parent.setAttribute("aria-expanded", String(Boolean(wasExpanded)));
    components.forEach((component) => { component.hidden = !wasExpanded; });
  }

  function apply(browser, state) {
    groupCurrencyRows(browser);
    groupAlcoholRows(browser);
    groupFoodRows(browser);
    const body = browser.querySelector("tbody");
    if (!body) return;
    const rows = [...body.querySelectorAll(":scope > tr.trade-inventory-row:not(.inventory-detail-row):not(.currency-component-row):not(.alcohol-component-row):not(.food-component-row)")];
    rows.forEach((row) => normalizeDestinationRow(row, browser));
    body.querySelectorAll(":scope > tr.alcohol-component-row, :scope > tr.food-component-row")
      .forEach((row) => normalizeDestinationRow(row, browser));
    rows.forEach((row) => {
      row.tabIndex = 0;
      if (!row.hasAttribute("aria-expanded")) row.setAttribute("aria-expanded", "false");
      const name = String(rowValue(row, "name")).toLocaleLowerCase();
      row.hidden = !name.includes(state.query.toLocaleLowerCase());
      Object.keys(OPTIONAL_COLUMNS).forEach((column) => optionalCell(row, column).hidden = !state.columns.includes(column));
      if (row._inventoryDetail) { row._inventoryDetail.remove(); row._inventoryDetail = null; }
      if (row.getAttribute("aria-expanded") === "true" && !row._currencyComponents && !row._alcoholComponents && !row._foodComponents) createDetail(row, browser);
    });
    browser.querySelectorAll("thead [data-inventory-column]").forEach((header) => { header.hidden = !state.columns.includes(header.dataset.inventoryColumn); });
    rows.map((row, index) => ({ row, index })).sort((a, b) => {
      const groupRank = (row) => row.classList.contains("currency-parent-row") ? 0 : row.classList.contains("alcohol-parent-row") ? 1 : row.classList.contains("food-parent-row") ? 2 : 3;
      return groupRank(a.row) - groupRank(b.row) || compareValues(rowValue(a.row, state.sort), rowValue(b.row, state.sort), state.direction) || a.index - b.index;
    }).forEach(({ row }) => {
      body.append(row);
      if (row._currencyComponents) row._currencyComponents.forEach((component) => body.append(component));
      if (row._alcoholComponents) row._alcoholComponents.forEach((component) => body.append(component));
      if (row._foodComponents) row._foodComponents.forEach((component) => body.append(component));
      const detail = row._inventoryDetail;
      if (detail) body.append(detail);
    });
    browser.querySelectorAll("[data-inventory-sort]").forEach((button) => {
      const active = button.dataset.inventorySort === state.sort;
      button.closest("th").setAttribute("aria-sort", active ? (state.direction === "asc" ? "ascending" : "descending") : "none");
      const indicator = button.querySelector(".inventory-sort-indicator");
      if (indicator) indicator.textContent = active ? (state.direction === "asc" ? "▲" : "▼") : "";
    });
    syncPanelWidth(browser);
  }

  function createDetail(row, browser) {
    if (!row._inventoryDetail) {
      const detail = document.createElement("tr");
      detail.className = "inventory-detail-row";
      const cell = document.createElement("td");
      cell.colSpan = 20;
      const label = row.querySelector("[data-item-name]");
      const visible = new Set(parsePanelState(global.location.search, browser.dataset.inventoryBrowser, browser.dataset.optionalColumns.split(",").filter(Boolean)).columns);
      const entries = [["Slot", label?.dataset.detailSlot], ["Balance", label?.dataset.detailBalance], ["Mode", label?.dataset.detailMode], ...Object.entries(OPTIONAL_COLUMNS).filter(([key]) => !visible.has(key)).map(([key, [name, dataKey]]) => [name, label?.dataset[`stat${dataKey[0].toUpperCase()}${dataKey.slice(1)}`]])].filter(([, value]) => value && value !== "0" && value !== "—");
      if (entries.length) {
        const list = document.createElement("dl");
        entries.forEach(([name, value]) => {
          const group = document.createElement("div");
          group.className = "inventory-detail-stat";
          const icon = document.createElement("span");
          icon.className = "inventory-detail-icon";
          icon.setAttribute("aria-hidden", "true");
          icon.style.setProperty("--inventory-detail-icon", `url('/static/icons/game/${DETAIL_ICONS[name] || "help"}.svg')`);
          const term = document.createElement("dt"); term.textContent = name;
          const description = document.createElement("dd"); description.textContent = value; description.title = value;
          group.append(icon, term, description); list.append(group);
        });
        cell.append(list);
      } else {
        cell.className = "inventory-detail-empty";
        cell.textContent = "No additional details.";
      }
      detail.append(cell); row._inventoryDetail = detail; row.after(detail);
    }
    row._inventoryDetail.hidden = row.hidden;
  }

  function toggleExpanded(row, browser) {
    const open = row.getAttribute("aria-expanded") === "true";
    row.setAttribute("aria-expanded", String(!open));
    if (row._currencyComponents) {
      row.querySelector("[data-coin-toggle]")?.setAttribute("aria-expanded", String(!open));
      row._currencyComponents.forEach((component) => { component.hidden = open || row.hidden; });
      return;
    }
    if (row._alcoholComponents) {
      row.querySelector("[data-alcohol-toggle]")?.setAttribute("aria-expanded", String(!open));
      row._alcoholComponents.forEach((component) => { component.hidden = open || row.hidden; });
      return;
    }
    if (row._foodComponents) {
      row.querySelector("[data-food-toggle]")?.setAttribute("aria-expanded", String(!open));
      row._foodComponents.forEach((component) => { component.hidden = open || row.hidden; });
      return;
    }
    if (!open) createDetail(row, browser);
    else if (row._inventoryDetail) row._inventoryDetail.hidden = true;
  }

  function mount(browser) {
    if (browser.dataset.inventoryMounted) { // live refresh may preserve the wrapper but replace rows
      const refreshed = parsePanelState(global.location.search, browser.dataset.inventoryBrowser, browser.dataset.optionalColumns.split(",").filter(Boolean));
      Object.assign(browser._inventoryState, refreshed);
      const search = browser.querySelector("[data-inventory-search]"); if (search) search.value = refreshed.query;
      browser.querySelectorAll("[data-inventory-column-options] input").forEach((input) => { input.checked = refreshed.columns.includes(input.value); });
      apply(browser, browser._inventoryState);
      return;
    }
    browser.dataset.inventoryMounted = "true";
    const available = browser.dataset.optionalColumns.split(",").filter(Boolean);
    let state = parsePanelState(global.location.search, browser.dataset.inventoryBrowser, available);
    browser._inventoryState = state;
    const search = browser.querySelector("[data-inventory-search]"); search.value = state.query;
    const options = browser.querySelector("[data-inventory-column-options]");
    available.forEach((column) => {
      const label = document.createElement("label");
      label.innerHTML = `<input type="checkbox" value="${column}"> ${OPTIONAL_COLUMNS[column][0]}`;
      const input = label.querySelector("input"); input.checked = state.columns.includes(column);
      input.addEventListener("change", () => { state.columns = [...options.querySelectorAll("input:checked")].map((entry) => entry.value); updateHistory(browser, state); apply(browser, state); });
      options.append(label);
      const th = document.createElement("th"); th.scope = "col"; th.dataset.inventoryColumn = column; th.innerHTML = `<button type="button" data-inventory-sort="${column}" aria-label="Sort by ${OPTIONAL_COLUMNS[column][0]}">${OPTIONAL_COLUMNS[column][0]} <span class="inventory-sort-indicator" aria-hidden="true"></span></button>`;
      const headerRow = browser.querySelector("thead tr");
      const actionHeader = headerRow.querySelector(":scope > .inventory-actions-header");
      if (actionHeader && actionHeader === headerRow.lastElementChild) headerRow.insertBefore(th, actionHeader);
      else headerRow.append(th);
    });
    search.addEventListener("input", () => { state.query = search.value; updateHistory(browser, state, browser._searchEditing === true); browser._searchEditing = true; apply(browser, state); });
    search.addEventListener("blur", () => { browser._searchEditing = false; });
    browser.addEventListener("click", (event) => {
      const coinAction = event.target.closest("[data-coin-action]");
      if (coinAction) {
        event.preventDefault(); event.stopImmediatePropagation();
        const row = coinAction.closest(".currency-parent-row");
        const buttons = row?._currencyComponents?.map((component) => component.querySelector("button.trade-transfer:not([disabled])")).filter(Boolean) || [];
        const mode = coinAction.dataset.transferMode || "one";
        if (mode === "one") buttons.find((button) => currencyRowQuantity(button.closest("tr")) > 0)?.click();
        else if (mode === "all") buttons.forEach((button) => { button.dataset.transferMode = "all"; button.click(); });
        else {
          let remaining = Math.max(0, currencyRowQuantity(row) - Number(coinAction.dataset.target || 0));
          buttons.forEach((button) => {
            if (remaining === 0) return;
            const component = button.closest("tr");
            const quantity = currencyRowQuantity(component);
            const transfer = Math.min(quantity, remaining);
            const originalTarget = button.dataset.target;
            button.dataset.target = String(quantity - transfer);
            button.dataset.transferMode = "target";
            button.click();
            if (originalTarget == null) delete button.dataset.target;
            else button.dataset.target = originalTarget;
            remaining -= transfer;
          });
        }
        groupCurrencyRows(browser);
        return;
      }
      const coinToggle = event.target.closest("[data-coin-toggle]");
      if (coinToggle) { event.preventDefault(); toggleExpanded(coinToggle.closest(".currency-parent-row"), browser); return; }
      const alcoholToggle = event.target.closest("[data-alcohol-toggle]");
      if (alcoholToggle) { event.preventDefault(); toggleExpanded(alcoholToggle.closest(".alcohol-parent-row"), browser); return; }
      const foodToggle = event.target.closest("[data-food-toggle]");
      if (foodToggle) { event.preventDefault(); toggleExpanded(foodToggle.closest(".food-parent-row"), browser); return; }
      const sort = event.target.closest("[data-inventory-sort]");
      if (sort) { const key = sort.dataset.inventorySort; state.direction = state.sort === key && state.direction === "asc" ? "desc" : "asc"; state.sort = key; updateHistory(browser, state); apply(browser, state); return; }
      const row = event.target.closest("tr.trade-inventory-row");
      if (row && !event.target.closest("button,input,a,select,textarea,label,form,details,summary")) toggleExpanded(row, browser);
    });
    browser.addEventListener("keydown", (event) => {
      if ((event.key === "Enter" || event.key === " ") && event.target.matches("tr.trade-inventory-row")) {
        event.preventDefault(); toggleExpanded(event.target, browser);
      }
    });
    apply(browser, state);
  }

  function mountAll(root = document) { root.querySelectorAll?.("[data-inventory-browser]").forEach(mount); }
  function refresh(scope = document) {
    const browsers = scope.matches?.("[data-inventory-browser]") ? [scope] : [...(scope.querySelectorAll?.("[data-inventory-browser]") || [])];
    const closest = scope.closest?.("[data-inventory-browser]"); if (closest && !browsers.includes(closest)) browsers.push(closest);
    browsers.forEach((browser) => {
      if (!browser.dataset.inventoryMounted) mount(browser);
      else apply(browser, browser._inventoryState || parsePanelState(global.location.search, browser.dataset.inventoryBrowser, browser.dataset.optionalColumns.split(",").filter(Boolean)));
    });
  }
  const api = { parsePanelState, serializePanelState, compareValues, normalizeSortValue, rowValue, groupCurrencyRows, groupFoodRows, mountAll, refresh, syncPanelWidth };
  global.strategicInventoryBrowser = api;
  if (typeof module !== "undefined") module.exports = api;
  if (global.document) { global.addEventListener("DOMContentLoaded", () => mountAll()); global.addEventListener("popstate", () => mountAll()); }
})(typeof window === "undefined" ? globalThis : window);
