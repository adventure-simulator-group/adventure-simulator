(function inventoryBrowserModule(global) {
  "use strict";

  const OPTIONAL_COLUMNS = {
    accuracy: ["Ranged precision", "accuracy"],
    swingPrecision: ["Swing precision", "swing-precision"],
    stabPrecision: ["Stab precision", "stab-precision"],
    reach: ["Reach m", "reach"],
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
    const weaponColumn = ["accuracy", "swingPrecision", "stabPrecision", "reach", "penetration", "damage", "block"].includes(column);
    const applicable = weaponColumn ? ["weapon", "shield"].includes(kind) : kind === "armor";
    const styleApplicable = column === "accuracy"
      ? label?.dataset.itemRanged === "true"
      : ["swingPrecision", "stabPrecision"].includes(column)
        ? label?.dataset.itemMelee === "true"
        : true;
    cell.textContent = applicable && styleApplicable ? (label?.dataset[property] || "—") : "—";
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
    const wasExpanded = previousParent
      ? previousParent.getAttribute("aria-expanded") === "true"
      : browser.dataset.inventoryBrowser === "cooking-inventory-right";
    previousParent?.remove();
    const components = [...body.querySelectorAll(":scope > tr.trade-inventory-row")]
      .filter((row) => !row.classList.contains("food-parent-row") && row.querySelector('[data-item-kind="food"], [data-food-lot="true"]'));
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
    const showsQuantity = !components.every((row) => row.dataset.groupSummary === "catalog");
    if (count && !showsQuantity) {
      count.hidden = true;
      count.setAttribute("hidden", "");
    }
    if (showsQuantity) {
      if (weight) { weight.textContent = totalWeight.toFixed(2).replace(/\.00$/, ""); weight.dataset.sortValue = String(totalWeight); }
      if (value) { value.textContent = String(totalValue); value.dataset.sortValue = String(totalValue); }
    } else {
      if (weight) { weight.textContent = "—"; weight.dataset.sortValue = ""; }
      if (value) { value.textContent = "—"; value.dataset.sortValue = ""; }
    }
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
      const visible = new Set(parsePanelState(global.location.search, browser.dataset.inventoryBrowser, (browser.dataset.optionalColumns || "").split(",").filter(Boolean)).columns);
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
        const empty = document.createElement("span");
        empty.textContent = "No additional details.";
        cell.append(empty);
      }
      const editUrl = label?.dataset.itemEditUrl;
      if (editUrl) {
        const link = document.createElement("a");
        link.className = "inventory-source-link";
        link.href = editUrl;
        link.target = "_blank";
        link.rel = "noopener noreferrer";
        link.dataset.developerOnly = "";
        link.setAttribute("aria-label", `Edit ${label.dataset.itemName || "item"} YAML definition`);
        link.title = "Edit YAML definition on GitHub";
        const icon = document.createElement("span");
        icon.className = "inventory-source-icon";
        icon.setAttribute("aria-hidden", "true");
        const text = document.createElement("span");
        text.textContent = "Edit YAML";
        link.append(icon, text);
        cell.append(link);
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
    if (row.dataset.containerObjectId) {
      const selector = open
        ? `tr[data-container-ancestor~="${CSS.escape(row.dataset.containerObjectId)}"]`
        : `tr[data-container-parent-object-id="${CSS.escape(row.dataset.containerObjectId)}"]`;
      browser.querySelectorAll(selector).forEach((child) => {
        child.hidden = open || row.hidden;
        child.setAttribute("aria-hidden", String(open || row.hidden));
      });
      row.querySelector("[data-container-toggle]")?.setAttribute("aria-expanded", String(!open));
      return;
    }
    if (!open) createDetail(row, browser);
    else if (row._inventoryDetail) row._inventoryDetail.hidden = true;
  }

  function ensureRowActionRail(row) {
    let cell = row.querySelector(":scope > .inventory-actions-cell");
    if (!cell) {
      cell = document.createElement("td");
      cell.className = "inventory-actions-cell";
      cell.setAttribute("aria-label", "Item actions");
      row.append(cell);
    }
    let actions = row.querySelector(".inventory-row-actions");
    if (!actions) {
      actions = document.createElement("span");
      actions.className = "inventory-row-actions";
    }
    if (actions.parentElement !== cell) cell.prepend(actions);
    return { cell, actions };
  }

  function bindContainerRowDragDrop(browser, row, open) {
    row.draggable = true;
    if (!row.dataset.containerDragBound) {
      row.dataset.containerDragBound = "true";
      row.addEventListener("dragstart", (event) => event.dataTransfer?.setData(
        "application/x-adventuresim-inventory-object",
        row.dataset.containerObjectId || row.dataset.personalInventoryId || row.dataset.partyInventoryId || "",
      ));
    }
    if (row.dataset.containerDropBound) return;
    row.dataset.containerDropBound = "true";
    row.addEventListener("dragover", (event) => { event.preventDefault(); row.classList.add("inventory-container-drop-target"); });
    row.addEventListener("dragleave", () => row.classList.remove("inventory-container-drop-target"));
    row.addEventListener("drop", (event) => {
      event.preventDefault();
      event.stopPropagation();
      row.classList.remove("inventory-container-drop-target");
      const child = event.dataTransfer?.getData("application/x-adventuresim-inventory-object");
      if (child) browser.dispatchEvent(new CustomEvent("inventory-container-move", {
        bubbles: true,
        detail: { child, parent: open.dataset.containerOpen },
      }));
    });
  }

  function decorateContainers(browser) {
    browser.querySelectorAll("tr.trade-inventory-row").forEach((row) => {
      const label = row.querySelector("[data-container-capacity-ml]");
      if ((!row.dataset.containerObjectId && !rowInventoryKey(row)) || !label
          || !["cooking_pan", "cooking_pot", "portable_oven"].includes(label.dataset.itemName)) return;
      if (row.dataset.containerDecorated) {
        const used = Number(row.dataset.containerUsedMl || 0);
        const capacity = Number(label.dataset.containerCapacityMl || 0);
        const meter = row.querySelector(".inventory-container-capacity");
        if (meter) {
          meter.textContent = `${used / 1000} / ${capacity / 1000} L`;
          meter.setAttribute("aria-label", `${used} of ${capacity} milliliters used`);
        }
        const open = row.querySelector("[data-container-open]");
        if (open && row.dataset.containerObjectId) open.dataset.containerOpen = row.dataset.containerObjectId;
        if (open) bindContainerRowDragDrop(browser, row, open);
        return;
      }
      row.dataset.containerDecorated = "true";
      const capacity = Number(label.dataset.containerCapacityMl || 0);
      const used = Number(row.dataset.containerUsedMl || 0);
      const meter = document.createElement("span");
      meter.className = "inventory-container-capacity";
      meter.textContent = `${used / 1000} / ${capacity / 1000} L`;
      meter.setAttribute("aria-label", `${used} of ${capacity} milliliters used`);
      row.querySelector(".inventory-item-name")?.append(meter);
      const { cell: actions } = ensureRowActionRail(row);
      const open = document.createElement("button");
      open.type = "button";
      open.className = "inventory-container-open";
      open.dataset.containerOpen = row.dataset.containerObjectId || rowInventoryKey(row) || "";
      open.textContent = "Open";
      open.setAttribute("aria-label", `Open ${label.dataset.itemName || "container"}`);
      actions?.append(open);
      bindContainerRowDragDrop(browser, row, open);
    });
  }

  function rowInventoryKey(row) {
    if (row.dataset.personalInventoryId) return `personal:${row.dataset.personalInventoryId}`;
    if (row.dataset.partyInventoryId) return `party:${row.dataset.partyInventoryId}`;
    return null;
  }

  function ensureMoveIntoControl(row, rowRef, snapshot) {
    const { actions } = ensureRowActionRail(row);
    if (!actions || actions.querySelector("[data-container-move-into]")) return;
    const candidates = snapshot.objects.filter((object) =>
      ["cooking_pan", "cooking_pot", "portable_oven"].includes(object.item_id)
        && String(object.id) !== String(rowRef) && object.location_kind !== "fireplace")
      .map((object) => ({ id: String(object.id), item_id: object.item_id }));
    document.querySelectorAll("tr.trade-inventory-row").forEach((candidateRow) => {
      const label = candidateRow.querySelector("[data-container-capacity-ml]");
      const id = candidateRow.dataset.containerObjectId || rowInventoryKey(candidateRow);
      if (id && id !== String(rowRef) && label
          && ["cooking_pan", "cooking_pot", "portable_oven"].includes(label.dataset.itemName)
          && !candidates.some((candidate) => candidate.id === id)) {
        candidates.push({ id, item_id: label.dataset.itemName });
      }
    });
    if (!candidates.length) return;
    const select = document.createElement("select");
    select.setAttribute("aria-label", "Destination container");
    candidates.forEach((container) => {
      const option = document.createElement("option"); option.value = String(container.id);
      option.textContent = container.item_id.replaceAll("_", " "); select.append(option);
    });
    const move = document.createElement("button"); move.type = "button";
    move.dataset.containerMoveInto = String(rowRef); move.textContent = "Move into";
    move.setAttribute("aria-label", "Move item into selected container");
    move.addEventListener("click", () => row.closest("[data-inventory-browser]")?.dispatchEvent(
      new CustomEvent("inventory-container-move", { bubbles: true, detail: { child: rowRef, parent: select.value } }),
    ));
    actions.classList.add("inventory-container-move-actions");
    actions.append(select, move);
  }

  const containerHydrationGenerations = new WeakMap();
  let authoritativeContainerSnapshot = null;
  async function hydrateContainerState(root = document) {
    if (!global.fetch) return;
    const generation = (containerHydrationGenerations.get(root) || 0) + 1;
    containerHydrationGenerations.set(root, generation);
    let snapshot;
    try {
      snapshot = await global.fetch("/api/inventory/containers", { headers: { Accept: "application/json" } }).then((response) => {
        if (!response.ok) throw new Error("container snapshot unavailable");
        return response.json();
      });
    } catch (_) { return; }
    if (generation !== containerHydrationGenerations.get(root)) return;
    authoritativeContainerSnapshot = snapshot;
    const objects = new Map(snapshot.objects.map((object) => [object.id, object]));
    const parents = new Map(snapshot.edges.map((edge) => [edge.child_object_id, edge.parent_object_id]));
    const liquids = new Map(snapshot.liquids.map((liquid) => [liquid.container_object_id, Number(liquid.water_ml)]));
    const byRow = new Map(snapshot.objects.map((object) => [`${object.location_kind}:${object.inventory_row_id}`, object]));
    root.querySelectorAll?.("tr.trade-inventory-row").forEach((row) => {
      const object = byRow.get(rowInventoryKey(row));
      const rowKey = rowInventoryKey(row);
      if (!object) {
        if (rowKey && !row.dataset.containerDragBound) {
          row.draggable = true; row.dataset.containerDragBound = "true";
          row.addEventListener("dragstart", (event) => event.dataTransfer?.setData(
            "application/x-adventuresim-inventory-object", rowKey,
          ));
        }
        if (rowKey) ensureMoveIntoControl(row, rowKey, snapshot);
        return;
      }
      row.dataset.containerObjectId = String(object.id);
      ensureMoveIntoControl(row, String(object.id), snapshot);
      row.tabIndex = 0;
      row.draggable = true;
      if (!row.dataset.containerDragBound) {
        row.dataset.containerDragBound = "true";
        row.addEventListener("dragstart", (event) => event.dataTransfer?.setData(
          "application/x-adventuresim-inventory-object", String(object.id),
        ));
      }
      const ancestors = [];
      let parent = parents.get(object.id);
      while (parent != null && ancestors.length <= 16) {
        ancestors.push(parent); parent = parents.get(parent);
      }
      if (ancestors.length) {
        row.dataset.containerParentObjectId = String(ancestors[0]);
        row.dataset.containerAncestor = ancestors.join(" ");
        const openPanel = row.closest("[data-open-container-object-id]");
        row.hidden = openPanel
          ? row.dataset.containerParentObjectId !== openPanel.dataset.openContainerObjectId
          : true;
        const actions = row.querySelector(".inventory-row-actions") || row.querySelector(".inventory-item-name");
        if (actions && !actions.querySelector("[data-container-remove]")) {
          const remove = document.createElement("button");
          remove.type = "button"; remove.className = "inventory-container-remove";
          remove.dataset.containerRemove = String(object.id); remove.textContent = "Remove";
          remove.setAttribute("aria-label", "Remove item from container"); actions.append(remove);
        }
      }
      let used = liquids.get(object.id) || 0;
      snapshot.edges.filter((edge) => edge.parent_object_id === object.id).forEach((edge) => {
        const child = objects.get(edge.child_object_id);
        const childRow = child && root.querySelector(`tr[data-${child.location_kind}-inventory-id="${child.inventory_row_id}"]`);
        used += Number(childRow?.querySelector("[data-exterior-volume-ml]")?.dataset.exteriorVolumeMl || 0);
      });
      row.dataset.containerUsedMl = String(used);
      if (snapshot.edges.some((edge) => edge.parent_object_id === object.id)) {
        const actions = row.querySelector(".inventory-row-actions") || row.querySelector(".inventory-item-name");
        if (actions && !actions.querySelector("[data-container-toggle]")) {
          const toggle = document.createElement("button");
          toggle.type = "button"; toggle.dataset.containerToggle = "true";
          toggle.textContent = "Expand"; toggle.setAttribute("aria-expanded", "false");
          toggle.setAttribute("aria-label", "Expand container contents"); actions.append(toggle);
        }
      }
    });
    root.querySelectorAll?.("[data-inventory-browser]").forEach(decorateContainers);
  }

  async function postContainer(path, values) {
    const body = new URLSearchParams(values);
    const response = await global.fetch(path, { method: "POST", body, headers: { "Content-Type": "application/x-www-form-urlencoded" } });
    if (!response.ok) throw new Error(await response.text());
    global.location.reload();
  }

  function openContainer(browser, button) {
    const id = button.dataset.containerOpen;
    if (!id) return;
    const paired = browser.dataset.inventoryCounterpart
      ? document.querySelector(`[data-inventory-browser="${CSS.escape(browser.dataset.inventoryCounterpart)}"]`)
      : null;
    let counterpart = browser.dataset.openContainerObjectId ? browser
      : paired && !paired.closest("[hidden]") ? paired
      : [...document.querySelectorAll("[data-inventory-browser]")]
        .find((candidate) => candidate !== browser && !candidate.hidden && !candidate.closest("[hidden]"));
    if (!counterpart) {
      const host = [...document.querySelectorAll(".left-sidebar,.right-sidebar,.center-content")]
        .find((candidate) => !candidate.contains(browser));
      if (!host) return;
      const hidden = [...host.children].map((child) => [child, child.hidden]);
      hidden.forEach(([child]) => { child.hidden = true; });
      counterpart = document.createElement("section");
      counterpart.dataset.inventoryBrowser = `container-${id}`;
      counterpart.innerHTML = '<label>Search container <input type="search" data-inventory-search></label><table class="trade-inventory-table"><tbody></tbody></table>';
      counterpart._containerHost = { host, hidden };
      host.append(counterpart); mount(counterpart);
    }
    counterpart._containerPanelStack ||= [];
    const sourceRows = [...document.querySelectorAll(`tr[data-container-ancestor~="${CSS.escape(id)}"]`)]
      .filter((source) => !counterpart.contains(source));
    const tbody = counterpart.querySelector("tbody");
    const panelRows = tbody ? [...tbody.children] : [];
    counterpart._containerPanelStack.push({
      active: document.activeElement,
      rowHidden: panelRows.map((row) => [row, row.hidden]),
      previousId: counterpart.dataset.openContainerObjectId || "",
      rows: panelRows,
    });
    tbody?.replaceChildren();
    sourceRows.forEach((source) => {
      if (tbody) {
        const clone = source.cloneNode(true);
        clone.dataset.containerTransient = "true";
        delete clone.dataset.containerDragBound;
        delete clone.dataset.containerDropBound;
        clone.hidden = false;
        tbody.append(clone);
      }
    });
    if (tbody && authoritativeContainerSnapshot) {
      const parents = new Map(authoritativeContainerSnapshot.edges.map((edge) => [edge.child_object_id, edge.parent_object_id]));
      const belongsToOpenTree = (objectId) => {
        let cursor = parents.get(objectId);
        for (let depth = 0; cursor != null && depth <= 16; depth += 1) {
          if (String(cursor) === id) return true;
          cursor = parents.get(cursor);
        }
        return false;
      };
      authoritativeContainerSnapshot.presentations.filter((item) => belongsToOpenTree(item.object_id)).forEach((item) => {
        if (tbody.querySelector(`[data-container-object-id="${item.object_id}"]`)) return;
        const row = document.createElement("tr");
        row.className = "trade-inventory-row inventory-container-snapshot-row";
        row.dataset.containerTransient = "true"; row.dataset.containerObjectId = String(item.object_id);
        row.dataset.containerParentObjectId = String(parents.get(item.object_id));
        const ancestors = []; let cursor = parents.get(item.object_id);
        while (cursor != null && ancestors.length <= 16) { ancestors.push(cursor); cursor = parents.get(cursor); }
        row.dataset.containerAncestor = ancestors.join(" "); row.tabIndex = 0; row.draggable = true;
        row.addEventListener("dragstart", (event) => event.dataTransfer?.setData(
          "application/x-adventuresim-inventory-object", String(item.object_id),
        ));
        row.hidden = row.dataset.containerParentObjectId !== id;
        const name = document.createElement("td"); name.className = "inventory-item-name";
        const label = document.createElement("span"); label.className = "inventory-item-label";
        label.dataset.itemName = item.item_id; label.dataset.exteriorVolumeMl = String(item.exterior_volume_ml);
        if (item.container_capacity_ml > 0) label.dataset.containerCapacityMl = String(item.container_capacity_ml);
        label.textContent = item.display_name;
        const actions = document.createElement("span"); actions.className = "inventory-row-actions";
        const remove = document.createElement("button"); remove.type = "button";
        remove.dataset.containerRemove = String(item.object_id); remove.textContent = "Remove";
        remove.setAttribute("aria-label", `Remove ${item.display_name} from container`);
        actions.append(remove); name.append(label, actions);
        const quantity = document.createElement("td"); quantity.className = "inventory-count"; quantity.textContent = String(item.quantity);
        row.append(name, quantity); tbody.append(row);
      });
    }
    counterpart.querySelectorAll("tr.trade-inventory-row").forEach((row) => {
      row.hidden = row.dataset.containerParentObjectId !== id;
    });
    hydrateContainerState(counterpart);
    let close = counterpart.querySelector("[data-container-close]");
    if (!close) {
      close = document.createElement("button");
      close.type = "button";
      close.className = "inventory-container-close";
      close.dataset.containerClose = "true";
      close.textContent = "Close";
      close.setAttribute("aria-label", "Close container inventory");
      counterpart.prepend(close);
    }
    close.hidden = false;
    let water = counterpart.querySelector("[data-container-water-actions]");
    if (!water) {
      water = document.createElement("div"); water.dataset.containerWaterActions = "true";
      water.className = "inventory-container-water-actions";
      water.innerHTML = '<span data-container-tincture-status></span><button type="button" data-container-pour>Pour water in</button><button type="button" data-container-drain>Pour water out</button><button type="button" data-container-spirit>Pour tincture spirit</button><button type="button" data-container-tincture>Start tincture</button><button type="button" data-container-tincture-refresh>Refresh tincture</button><button type="button" data-container-tincture-dose>Take 10% dose</button>';
      close.after(water);
    }
    water.hidden = !/^\d+$/.test(id);
    const vessel = authoritativeContainerSnapshot?.presentations?.find((item) => String(item.object_id) === id)?.tincture_vessel === true;
    const tincture = authoritativeContainerSnapshot?.tinctures?.find((item) => String(item.container_object_id) === id);
    water.querySelector("[data-container-spirit]").hidden = !vessel || Boolean(tincture);
    water.querySelector("[data-container-tincture]").hidden = !vessel || Boolean(tincture);
    water.querySelector("[data-container-tincture-refresh]").hidden = !tincture || tincture.matured;
    water.querySelector("[data-container-tincture-dose]").hidden = !tincture?.matured;
    const tinctureStatus = water.querySelector("[data-container-tincture-status]");
    tinctureStatus.textContent = tincture ? (tincture.matured ? "Tincture ready" : "Tincture maturing") : "";
    counterpart.dataset.openContainerObjectId = id;
    if (!counterpart._containerDropBound) {
      counterpart._containerDropBound = true;
      counterpart.addEventListener("dragover", (event) => event.preventDefault());
      counterpart.addEventListener("drop", (event) => {
        const child = event.dataTransfer?.getData("application/x-adventuresim-inventory-object");
        const destination = event.target.closest?.("tr")?.querySelector?.("[data-container-open]");
        if (child && destination?.dataset.containerOpen && destination.dataset.containerOpen !== child) {
          event.preventDefault();
          event.stopImmediatePropagation();
          counterpart.dispatchEvent(new CustomEvent("inventory-container-move", {
            bubbles: true,
            detail: { child, parent: destination.dataset.containerOpen },
          }));
          return;
        }
        if (/^\d+$/.test(child || "")) {
          event.preventDefault();
          postContainer("/api/inventory/containers/remove", { child_object_id: child })
            .catch((error) => global.alert?.(error.message));
        }
      });
    }
    counterpart.querySelector("[data-inventory-search]")?.focus();
  }

  function closeContainer(browser) {
    const snapshot = browser._containerPanelStack?.pop();
    if (!snapshot) return;
    const tbody = browser.querySelector("tbody");
    if (tbody) {
      tbody.replaceChildren(...snapshot.rows);
      snapshot.rowHidden.forEach(([row, hidden]) => { row.hidden = hidden; });
    }
    if (snapshot.previousId) {
      browser.dataset.openContainerObjectId = snapshot.previousId;
      browser.querySelector("[data-container-close]").hidden = false;
      const water = browser.querySelector("[data-container-water-actions]");
      if (water) water.hidden = !/^\d+$/.test(snapshot.previousId);
      hydrateContainerState(browser);
    } else {
      browser.querySelector("[data-container-close]").hidden = true;
      const water = browser.querySelector("[data-container-water-actions]"); if (water) water.hidden = true;
      delete browser.dataset.openContainerObjectId;
    }
    snapshot.active?.focus?.();
    if (browser._containerHost && !browser._containerPanelStack.length) {
      browser._containerHost.hidden.forEach(([child, hidden]) => { child.hidden = hidden; });
      browser.remove();
    }
  }

  function mount(browser) {
    if (browser.dataset.inventoryMounted) { // live refresh may preserve the wrapper but replace rows
      const refreshed = parsePanelState(global.location.search, browser.dataset.inventoryBrowser, (browser.dataset.optionalColumns || "").split(",").filter(Boolean));
      Object.assign(browser._inventoryState, refreshed);
      const search = browser.querySelector("[data-inventory-search]"); if (search) search.value = refreshed.query;
      browser.querySelectorAll("[data-inventory-column-options] input").forEach((input) => { input.checked = refreshed.columns.includes(input.value); });
      apply(browser, browser._inventoryState);
      decorateContainers(browser);
      return;
    }
    browser.dataset.inventoryMounted = "true";
    const available = (browser.dataset.optionalColumns || "").split(",").filter(Boolean);
    let state = parsePanelState(global.location.search, browser.dataset.inventoryBrowser, available);
    browser._inventoryState = state;
    const search = browser.querySelector("[data-inventory-search]"); if (search) search.value = state.query;
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
    search?.addEventListener("input", () => { state.query = search.value; updateHistory(browser, state, browser._searchEditing === true); browser._searchEditing = true; apply(browser, state); });
    search?.addEventListener("blur", () => { browser._searchEditing = false; });
    browser.addEventListener("click", (event) => {
      const containerOpen = event.target.closest("[data-container-open]");
      if (containerOpen) { event.preventDefault(); openContainer(browser, containerOpen); return; }
      const containerClose = event.target.closest("[data-container-close]");
      if (containerClose) { event.preventDefault(); closeContainer(browser); return; }
      const containerRemove = event.target.closest("[data-container-remove]");
      if (containerRemove) {
        event.preventDefault();
        postContainer("/api/inventory/containers/remove", { child_object_id: containerRemove.dataset.containerRemove })
          .catch((error) => global.alert?.(error.message));
        return;
      }
      const waterAction = event.target.closest("[data-container-pour],[data-container-drain]");
      if (waterAction) {
        event.preventDefault();
        const requested = global.prompt?.("Milliliters", "1000");
        if (requested && Number(requested) > 0) postContainer(
          waterAction.matches("[data-container-pour]") ? "/api/inventory/containers/pour" : "/api/inventory/containers/drain",
          { container_object_id: browser.dataset.openContainerObjectId, requested_ml: requested },
        ).catch((error) => global.alert?.(error.message));
        return;
      }
      const tinctureAction = event.target.closest("[data-container-spirit],[data-container-tincture]");
      if (tinctureAction) {
        event.preventDefault();
        postContainer(
          tinctureAction.matches("[data-container-spirit]")
            ? "/api/inventory/containers/tincture-spirit"
            : "/api/inventory/containers/tincture-start",
          { container_object_id: browser.dataset.openContainerObjectId },
        ).catch((error) => global.alert?.(error.message));
        return;
      }
      const tinctureLifecycleAction = event.target.closest("[data-container-tincture-refresh],[data-container-tincture-dose]");
      if (tinctureLifecycleAction) {
        event.preventDefault();
        postContainer(
          tinctureLifecycleAction.matches("[data-container-tincture-dose]")
            ? "/api/inventory/containers/tincture-dose"
            : "/api/inventory/containers/tincture-refresh",
          { container_object_id: browser.dataset.openContainerObjectId, amount_milliunits: 100 },
        ).catch((error) => global.alert?.(error.message));
        return;
      }
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
      const containerToggle = event.target.closest("[data-container-toggle]");
      if (containerToggle) { event.preventDefault(); toggleExpanded(containerToggle.closest("tr.trade-inventory-row"), browser); return; }
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
    decorateContainers(browser);
  }

  function hydrateProceduralWeaponIcons(root = document) {
    const rows = root.matches?.("tr.trade-inventory-row")
      ? [root]
      : [...(root.querySelectorAll?.("tr.trade-inventory-row") || [])];
    rows.forEach((row) => {
      if (!row.querySelector('.inventory-item-label[data-item-melee="true"], .inventory-item-label[data-item-weapon-holder="true"]')) return;
      const scope = row.dataset.personalInventoryId ? "personal" : row.dataset.partyInventoryId ? "party" : "";
      const rowId = row.dataset.personalInventoryId || row.dataset.partyInventoryId || "";
      const icon = row.querySelector(".inventory-item-type .game-icon");
      if (!scope || !/^\d+$/.test(rowId) || !icon) return;
      const url = `/api/weapon-icons/${scope}/${rowId}.png`;
      if (icon.dataset.proceduralWeaponIcon === url) return;
      icon.dataset.proceduralWeaponIcon = url;
      const probe = new Image();
      probe.addEventListener("load", () => {
        if (icon.isConnected && icon.dataset.proceduralWeaponIcon === url) {
          icon.style.setProperty("--game-icon", `url("${url}")`);
        }
      }, { once: true });
      probe.addEventListener("error", () => {
        // The authored catalog SVG remains for legacy or non-instanced equipment.
      }, { once: true });
      probe.src = url;
    });
  }

  function mountAll(root = document) {
    root.querySelectorAll?.("[data-inventory-browser]").forEach(mount);
    hydrateProceduralWeaponIcons(root);
  }
  function refresh(scope = document) {
    const browsers = scope.matches?.("[data-inventory-browser]") ? [scope] : [...(scope.querySelectorAll?.("[data-inventory-browser]") || [])];
    const closest = scope.closest?.("[data-inventory-browser]"); if (closest && !browsers.includes(closest)) browsers.push(closest);
    browsers.forEach((browser) => {
      if (!browser.dataset.inventoryMounted) mount(browser);
      else apply(browser, browser._inventoryState || parsePanelState(global.location.search, browser.dataset.inventoryBrowser, (browser.dataset.optionalColumns || "").split(",").filter(Boolean)));
    });
    hydrateContainerState(scope);
    hydrateProceduralWeaponIcons(scope);
  }
  const api = { parsePanelState, serializePanelState, compareValues, normalizeSortValue, rowValue, groupCurrencyRows, groupFoodRows, decorateContainers, hydrateContainerState, hydrateProceduralWeaponIcons, openContainer, closeContainer, mountAll, refresh, syncPanelWidth };
  global.strategicInventoryBrowser = api;
  if (typeof module !== "undefined") module.exports = api;
  if (global.document) {
    global.addEventListener("DOMContentLoaded", () => { mountAll(); hydrateContainerState(); });
    global.addEventListener("popstate", () => mountAll());
    global.document.addEventListener("strategic-page-mounted", () => { mountAll(); hydrateContainerState(); });
    global.document.addEventListener("inventory-container-move", (event) => {
      const raw = String(event.detail.child);
      const legacy = raw.match(/^(personal|party):(\d+)$/);
      const parentRaw = String(event.detail.parent);
      const parentLegacy = parentRaw.match(/^(personal|party):(\d+)$/);
      postContainer("/api/inventory/containers/move", {
        child_object_id: legacy ? "0" : raw,
        child_scope: legacy?.[1] || "", child_row_id: legacy?.[2] || "0",
        parent_object_id: parentLegacy ? "0" : parentRaw,
        parent_scope: parentLegacy?.[1] || "", parent_row_id: parentLegacy?.[2] || "0",
      }).catch((error) => global.alert?.(error.message));
    });
  }
})(typeof window === "undefined" ? globalThis : window);
