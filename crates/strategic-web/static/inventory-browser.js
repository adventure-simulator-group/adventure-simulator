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
  function rowValue(row, key) {
    const label = row.querySelector?.("[data-item-name]");
    if (key === "name") return label?.dataset.itemName || label?.textContent.trim() || "";
    if (key === "type") return label?.dataset.itemKind || "";
    if (OPTIONAL_COLUMNS[key]) return textNumber(label?.dataset[`stat${OPTIONAL_COLUMNS[key][1][0].toUpperCase()}${OPTIONAL_COLUMNS[key][1].slice(1)}`]);
    const selectors = { quantity: ".inventory-count", target: ".inventory-target-value", equipped: ".inventory-equipped input", durability: ".inventory-durability", weight: ".inventory-weight", value: ".inventory-gold" };
    if (key === "equipped") return row.querySelector(selectors[key])?.checked ? 1 : 0;
    return textNumber(row.querySelector(selectors[key])?.textContent);
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
    row.append(cell);
    return cell;
  }

  function updateHistory(browser, state, replace = false) {
    const query = serializePanelState(global.location.search, browser.dataset.inventoryBrowser, state);
    const url = `${global.location.pathname}${query}${global.location.hash}`;
    global.history[replace ? "replaceState" : "pushState"]({}, "", url);
  }

  function apply(browser, state) {
    const body = browser.querySelector("tbody");
    if (!body) return;
    const rows = [...body.querySelectorAll(":scope > tr.trade-inventory-row:not(.inventory-detail-row)")];
    rows.forEach(ensureQuantityTargetSplit);
    rows.forEach((row) => {
      row.tabIndex = 0;
      if (!row.hasAttribute("aria-expanded")) row.setAttribute("aria-expanded", "false");
      const name = String(rowValue(row, "name")).toLocaleLowerCase();
      row.hidden = !name.includes(state.query.toLocaleLowerCase());
      Object.keys(OPTIONAL_COLUMNS).forEach((column) => optionalCell(row, column).hidden = !state.columns.includes(column));
      const details = row.nextElementSibling?.classList.contains("inventory-detail-row") ? row.nextElementSibling : null;
      if (details) details.hidden = row.hidden || row.getAttribute("aria-expanded") !== "true";
    });
    browser.querySelectorAll("thead [data-inventory-column]").forEach((header) => { header.hidden = !state.columns.includes(header.dataset.inventoryColumn); });
    rows.map((row, index) => ({ row, index })).sort((a, b) => compareValues(rowValue(a.row, state.sort), rowValue(b.row, state.sort), state.direction) || a.index - b.index).forEach(({ row }) => {
      body.append(row);
      const detail = row._inventoryDetail;
      if (detail) body.append(detail);
    });
    browser.querySelectorAll("[data-inventory-sort]").forEach((button) => {
      const active = button.dataset.inventorySort === state.sort;
      button.closest("th").setAttribute("aria-sort", active ? (state.direction === "asc" ? "ascending" : "descending") : "none");
      button.querySelector(".inventory-sort-indicator").textContent = active ? (state.direction === "asc" ? "▲" : "▼") : "";
    });
  }

  function toggleExpanded(row, browser) {
    const open = row.getAttribute("aria-expanded") === "true";
    row.setAttribute("aria-expanded", String(!open));
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
          const term = document.createElement("dt"); term.textContent = name;
          const description = document.createElement("dd"); description.textContent = value;
          group.append(term, description); list.append(group);
        });
        cell.append(list);
      } else cell.textContent = "No additional details.";
      detail.append(cell); row._inventoryDetail = detail; row.after(detail);
    }
    row._inventoryDetail.hidden = open;
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
      browser.querySelector("thead tr").append(th);
    });
    search.addEventListener("input", () => { state.query = search.value; updateHistory(browser, state, browser._searchEditing === true); browser._searchEditing = true; apply(browser, state); });
    search.addEventListener("blur", () => { browser._searchEditing = false; });
    browser.addEventListener("click", (event) => {
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
  const api = { parsePanelState, serializePanelState, compareValues, rowValue, mountAll };
  global.strategicInventoryBrowser = api;
  if (typeof module !== "undefined") module.exports = api;
  if (global.document) { global.addEventListener("DOMContentLoaded", () => mountAll()); global.addEventListener("popstate", () => mountAll()); }
})(typeof window === "undefined" ? globalThis : window);
