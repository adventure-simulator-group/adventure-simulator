const test = require("node:test");
const assert = require("node:assert/strict");
const { parseHTML } = require("linkedom");

function currencyRow(id, name, quantity, target = 0) {
  return `<tr class="trade-inventory-row" data-inventory-quantity="${quantity}" data-item-key="${id}">
    <td class="inventory-item-type"><span class="game-icon" role="img" aria-label="Item type: ${name}" title="Item type: ${name}"></span></td>
    <td class="inventory-item-name"><span data-item-name="Coin" data-item-kind="currency" data-currency-name="${name}">Coin</span>
      <span class="inventory-row-actions"><button type="button" class="trade-transfer" data-dynamic-transfer data-item-name="${name}" data-transfer-mode="one" data-count="${quantity}" data-target="${target}" data-label-one="Transfer one ${name}" data-label-target="Transfer ${name} to target" data-label-all="Transfer all ${name}" aria-label="Transfer one ${name}" title="Transfer one ${name}">→</button></span>
    </td>
    <td class="inventory-count"><span data-target-control data-quantity="${quantity}"><span data-target-value>${target}</span></span></td>
    <td class="inventory-weight">0.01</td><td class="inventory-gold">1</td>
  </tr>`;
}

function ordinaryRow(id, name, quantity) {
  return `<tr class="trade-inventory-row" data-inventory-quantity="${quantity}" data-item-key="${id}">
    <td class="inventory-item-type"></td>
    <td class="inventory-item-name"><span data-item-name="${name}" data-item-kind="supply">${name}</span></td>
    <td class="inventory-count">${quantity}</td>
    <td class="inventory-weight">1</td><td class="inventory-gold">999</td>
  </tr>`;
}

function alcoholRow(id, name, quantity, weight, value) {
  return `<tr class="trade-inventory-row" data-inventory-quantity="${quantity}" data-item-key="${id}">
    <td class="inventory-item-type"><span class="game-icon" role="img" aria-label="Item type: ${name}" title="Item type: ${name}"></span></td>
    <td class="inventory-item-name"><span data-item-name="${name}" data-item-kind="supply" data-item-group="alcohol" data-group-name="Alcohol">${name}</span>
      <span class="inventory-row-actions"><button type="button" class="trade-transfer" aria-label="Transfer one ${name}">→</button></span>
    </td>
    <td class="inventory-count">${quantity}</td>
    <td class="inventory-weight">${weight}</td><td class="inventory-gold">${value}</td>
  </tr>`;
}

function fixture() {
  const { window, document } = parseHTML(`<html><body>
    <div data-inventory-browser="test" data-optional-columns="">
      <input data-inventory-search><div data-inventory-column-options></div>
      <table class="trade-inventory-table"><thead><tr><th>Name</th><th class="inventory-column-count">#</th></tr></thead><tbody>
        ${ordinaryRow("apple", "Apple", 999)}
        ${alcoholRow("small_beer", "Small beer", 2, 0.5, 2)}
        ${alcoholRow("table_wine", "Table wine", 1, 0.25, 3)}
        ${currencyRow("lubeck_mark", "Lübeck mark", 3, 1)}
        ${currencyRow("danish_mark", "Danish mark", 2, 0)}
      </tbody></table>
    </div></body></html>`);
  window.location = { search: "", pathname: "/", hash: "" };
  window.history = { pushState() {}, replaceState() {} };
  window.getComputedStyle = () => ({ paddingLeft: "0", paddingRight: "0" });
  global.window = window;
  global.document = document;
  return { window, document, browser: document.querySelector("[data-inventory-browser]") };
}

test("mixed currency DOM stays one aggregate through normalization, staging, and live insertion", () => {
  const { document, browser } = fixture();
  delete require.cache[require.resolve("../static/inventory-browser.js")];
  const inventory = require("../static/inventory-browser.js");
  inventory.mountAll(document);

  let parents = browser.querySelectorAll(".currency-parent-row");
  assert.equal(parents.length, 1);
  assert.equal(parents[0].querySelector(".inventory-count").textContent, "5");
  assert.equal(parents[0].dataset.inventoryQuantity, "5");
  assert.equal(parents[0].querySelector(".inventory-weight").textContent, "0.05");
  assert.equal(parents[0].querySelector(".game-icon").getAttribute("aria-label"), "Item type: Coin");
  assert.equal(parents[0].querySelector(".trade-transfer").dataset.labelAll, "Transfer all Coin");
  assert.equal(parents[0].querySelector("[data-item-name]").nextElementSibling, parents[0].querySelector("[data-coin-toggle]"));
  assert.doesNotMatch(parents[0].outerHTML, /Lübeck|Danish/);
  assert.equal(browser.querySelector("tbody > tr:not(.currency-component-row)"), parents[0]);

  browser._inventoryState.sort = "quantity";
  browser._inventoryState.direction = "desc";
  inventory.refresh(browser);
  parents = browser.querySelectorAll(".currency-parent-row");
  assert.equal(browser.querySelector("tbody > tr:not(.currency-component-row)"), parents[0]);

  parents[0].querySelector("[data-coin-toggle]").click();
  const children = [...browser.querySelectorAll(".currency-component-row")];
  assert.deepEqual(children.map((row) => row.querySelector(".inventory-count").textContent), ["3", "2"]);
  assert.deepEqual(children.map((row) => row.querySelector("[data-item-name]").textContent), ["Lübeck mark", "Danish mark"]);

  const firstCount = children[0].querySelector(".inventory-count");
  firstCount.dataset.base = "3";
  firstCount.dataset.tradeDraftChange = "-1";
  firstCount.innerHTML = "3 <span>-1</span>";
  inventory.refresh(browser);
  parents = browser.querySelectorAll(".currency-parent-row");
  assert.equal(parents.length, 1);
  assert.equal(parents[0].querySelector(".inventory-count").textContent, "4");
  assert.equal(browser.querySelectorAll(".inventory-detail-row").length, 0);

  browser.querySelector("tbody").insertAdjacentHTML("beforeend", currencyRow("saxon_thaler", "Saxon thaler", 4));
  inventory.refresh(browser);
  parents = browser.querySelectorAll(".currency-parent-row");
  assert.equal(parents.length, 1);
  assert.equal(parents[0].querySelector(".inventory-count").textContent, "8");
});

test("alcohol types collapse into a non-fungible aggregate and retain component actions", () => {
  const { document, browser } = fixture();
  delete require.cache[require.resolve("../static/inventory-browser.js")];
  const inventory = require("../static/inventory-browser.js");
  inventory.mountAll(document);

  const parents = browser.querySelectorAll(".alcohol-parent-row");
  assert.equal(parents.length, 1);
  const parent = parents[0];
  assert.equal(parent.querySelector("[data-item-name]").textContent, "Alcohol");
  assert.equal(parent.querySelector(".inventory-count").textContent, "3");
  assert.equal(parent.querySelector(".inventory-weight").textContent, "1.25");
  assert.equal(parent.querySelector(".inventory-gold").textContent, "7");
  assert.equal(parent.querySelector(".trade-transfer"), null);
  assert.equal(parent.querySelector("[data-alcohol-toggle]").getAttribute("aria-label"), "Show alcohol types");

  const components = browser.querySelectorAll(".alcohol-component-row");
  assert.equal(components.length, 2);
  assert.ok([...components].every((row) => row.hidden));
  parent.querySelector("[data-alcohol-toggle]").click();
  assert.ok([...components].every((row) => !row.hidden));
  assert.equal(components[0].querySelector(".trade-transfer").getAttribute("aria-label"), "Transfer one Small beer");
});

test("merchant Alcohol parent does not present aggregate stock as one unit", () => {
  const { document, browser } = fixture();
  browser.querySelectorAll('[data-item-group="alcohol"]').forEach((label) => {
    label.closest("tr").dataset.groupSummary = "catalog";
  });
  delete require.cache[require.resolve("../static/inventory-browser.js")];
  const inventory = require("../static/inventory-browser.js");
  inventory.mountAll(document);

  const parent = browser.querySelector(".alcohol-parent-row");
  assert.equal(parent.querySelector(".inventory-weight").textContent, "—");
  assert.equal(parent.querySelector(".inventory-gold").textContent, "—");
});

test("aggregate Coin one, all, and target actions route coherently to component rows", () => {
  const { document, browser } = fixture();
  delete require.cache[require.resolve("../static/inventory-browser.js")];
  const inventory = require("../static/inventory-browser.js");
  inventory.mountAll(document);
  const calls = [];
  browser.querySelectorAll(".currency-component-row .trade-transfer").forEach((button) => {
    button.addEventListener("click", () => calls.push({
      name: button.closest("tr").querySelector("[data-item-name]").textContent,
      mode: button.dataset.transferMode,
      target: button.dataset.target,
    }));
  });

  browser.querySelector(".currency-parent-row .trade-transfer").click();
  assert.deepEqual(calls.map((call) => call.name), ["Lübeck mark"]);

  calls.length = 0;
  let action = browser.querySelector(".currency-parent-row .trade-transfer");
  action.dataset.transferMode = "all";
  action.click();
  assert.equal(calls.length, 2);
  assert.ok(calls.every((call) => call.mode === "all"));

  calls.length = 0;
  action = browser.querySelector(".currency-parent-row .trade-transfer");
  action.dataset.transferMode = "target";
  action.click();
  assert.equal(calls.length, 2);
  assert.ok(calls.every((call) => call.mode === "target"));
});
