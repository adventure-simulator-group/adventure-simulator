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

function fixture() {
  const { window, document } = parseHTML(`<html><body>
    <div data-inventory-browser="test" data-optional-columns="">
      <input data-inventory-search><div data-inventory-column-options></div>
      <table class="trade-inventory-table"><thead><tr><th>Name</th><th class="inventory-column-count">#</th></tr></thead><tbody>
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
  assert.doesNotMatch(parents[0].outerHTML, /Lübeck|Danish/);

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
