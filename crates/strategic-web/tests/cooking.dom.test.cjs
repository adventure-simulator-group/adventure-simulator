const assert = require("node:assert/strict");
const test = require("node:test");
const { parseHTML } = require("linkedom");

function fixture() {
  const { window, document } = parseHTML(`<html><body>
    <div data-cooking-activity>
      <aside class="left-sidebar"><p data-cooking-pot-empty></p>
        <div data-inventory-browser="cooking-pot-left"><table><tbody></tbody></table></div>
      </aside>
      <main>
        <input type="radio" data-cooking-method value="roast" checked>
        <form><input data-cooking-ids><input data-cooking-amounts>
          <p data-cooking-preview></p>
          <button type="submit" data-cook-submit disabled>Cook</button></form>
      </main>
      <aside class="right-sidebar">
        <div data-inventory-browser="cooking-inventory-right"><table><tbody>
          <tr class="trade-inventory-row trade-row-player" data-cooking-source="42">
            <td class="inventory-item-name"><span data-item-name="Carrot" data-item-kind="food" data-food-lot="true">Carrot</span>
              <span class="inventory-row-actions"><button type="button" data-cooking-stage="42" data-cooking-name="Carrot" data-count="1000000" data-mass="0.2" data-safety="5" data-transfer-mode="one">left</button></span>
            </td>
            <td class="inventory-count">1</td><td class="inventory-weight">0.2</td><td class="inventory-gold">2</td>
          </tr>
        </tbody></table></div>
      </aside>
    </div>
  </body></html>`);
  window.strategicInventoryBrowser = { refresh() {} };
  window.strategicTradeUi = { mountInventoryBulkControls() {} };
  global.window = window;
  global.document = document;
  global.CSS = { escape: (value) => String(value) };
  delete require.cache[require.resolve("../static/cooking.js")];
  require("../static/cooking.js");
  window.strategicCooking.mount(document);
  return { window, document, form: document.querySelector("[data-cooking-activity]") };
}

test("ingredients transfer between inventory and pot and drive the cooking form", () => {
  const { window, form } = fixture();
  const stage = form.querySelector("[data-cooking-stage]");
  stage.dispatchEvent(new window.Event("click", { bubbles: true }));

  assert.equal(form.querySelector("[data-cooking-ids]").value, "42");
  assert.equal(form.querySelector("[data-cooking-amounts]").value, "250000");
  assert.equal(form.querySelector("[data-cooking-pot-id='42'] .inventory-count").textContent, "0.25");
  assert.match(form.querySelector("[data-cooking-source='42'] .inventory-count").textContent, /-0.25/);
  assert.equal(form.querySelector("[data-cook-submit]").disabled, false);
  assert.match(form.querySelector("[data-cooking-preview]").textContent, /flavor score 0\/5/);
  assert.match(form.querySelector("[data-cooking-preview]").textContent, /15% calories lost/);
  assert.equal(form.querySelector("[data-cooking-pot-empty]").hidden, true);

  const unstage = form.querySelector("[data-cooking-unstage]");
  unstage.dispatchEvent(new window.Event("click", { bubbles: true }));
  assert.equal(form.querySelector("[data-cooking-pot-id='42']"), null);
  assert.equal(form.querySelector("[data-cooking-ids]").value, "");
  assert.equal(form.querySelector("[data-cook-submit]").disabled, true);
});
