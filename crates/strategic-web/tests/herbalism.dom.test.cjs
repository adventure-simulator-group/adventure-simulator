const assert = require("node:assert/strict");
const test = require("node:test");
const { parseHTML } = require("linkedom");

function fixture() {
  const { window, document } = parseHTML(`<html><body>
    <section data-herbalism-activity role="dialog" aria-labelledby="herbalism-dialog-title">
      <h2 id="herbalism-dialog-title">Herbalism</h2>
      <form data-herbalism-form>
        <input type="radio" name="inventory_item_id" value="42" data-herbal-ingredient
          data-item-id="comfrey_fine"
          data-dry-grind="Fine comfrey poultice|135|1|Supports tissue integrity|Topical use only|false"
          data-infuse-decoct="Spent herb waste|203|1|excessive heat destroys it|Ingredient becomes waste|true">
        <label>Dry and grind<input type="radio" name="method" value="dry_grind" data-herbal-method></label>
        <label>Infuse<input type="radio" name="method" value="infuse_decoct" data-herbal-method></label>
        <label>Tincture<input type="radio" name="method" value="tincture" data-herbal-method></label>
        <p data-herbal-preview></p>
        <button data-herbal-submit disabled>Prepare</button>
      </form>
    </section>
  </body></html>`);
  global.window = window;
  global.document = document;
  delete require.cache[require.resolve("../static/herbalism.js")];
  require("../static/herbalism.js");
  window.strategicHerbalism.mountAll();
  return { window, document };
}

test("selecting an ingredient disables incompatible authored methods", () => {
  const { window, document } = fixture();
  const ingredient = document.querySelector("[data-herbal-ingredient]");
  ingredient.checked = true;
  ingredient.dispatchEvent(new window.Event("change", { bubbles: true }));
  assert.equal(document.querySelector('[value="tincture"]').disabled, true);
  assert.equal(document.querySelector('[value="dry_grind"]').disabled, false);
});

test("compatible selection previews output, duration, units, risk, and degradation", () => {
  const { window, document } = fixture();
  const ingredient = document.querySelector("[data-herbal-ingredient]");
  const method = document.querySelector('[value="infuse_decoct"]');
  ingredient.checked = true;
  ingredient.dispatchEvent(new window.Event("change", { bubbles: true }));
  method.checked = true;
  method.dispatchEvent(new window.Event("change", { bubbles: true }));
  const preview = document.querySelector("[data-herbal-preview]").textContent;
  assert.match(preview, /Spent herb waste/);
  assert.match(preview, /203 minutes/);
  assert.match(preview, /Risk: Ingredient becomes waste/);
  assert.match(preview, /Degradation warning/);
  assert.equal(document.querySelector("[data-herbal-submit]").disabled, false);
});
