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
          data-dry-grind="Fine comfrey poultice|135|1|No additional consumable|Supports tissue integrity|Safety warning: topical use only|false"
          data-infuse-decoct="Spent herb waste|203|1|No additional consumable|excessive heat destroys it|Safety warning: ingredient becomes waste|true">
        <input type="radio" name="inventory_item_id" value="43" data-herbal-ingredient
          data-item-id="poppy"
          data-tincture="Poppy tincture|396|1|Tincture spirit × 1|Strong relief|Safety warning: oxygenation hazard|false">
        <label tabindex="0" aria-describedby="dry-status" data-method-label="Dry and grind"
          data-method-description="Air-dry and grind.">Dry and grind
          <input type="radio" name="method" value="dry_grind" data-herbal-method>
          <span id="dry-status" data-herbal-method-status>Air-dry and grind.</span>
        </label>
        <label tabindex="0" aria-describedby="infuse-status" data-method-label="Infuse"
          data-method-description="Use authored heat.">Infuse
          <input type="radio" name="method" value="infuse_decoct" data-herbal-method>
          <span id="infuse-status" data-herbal-method-status>Use authored heat.</span>
        </label>
        <label tabindex="0" aria-describedby="tincture-status" data-method-label="Tincture"
          data-method-description="Steep in spirit.">Tincture
          <input type="radio" name="method" value="tincture" data-herbal-method>
          <span id="tincture-status" data-herbal-method-status>Steep in spirit.</span>
        </label>
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
  ingredient.setAttribute("checked", "");
  ingredient.dispatchEvent(new window.Event("change", { bubbles: true }));
  assert.equal(document.querySelector('[value="tincture"]').disabled, true);
  assert.equal(document.querySelector('[value="dry_grind"]').disabled, false);
  const wrapper = document.querySelector('[value="tincture"]').closest("label");
  assert.equal(wrapper.getAttribute("aria-disabled"), "true");
  assert.match(wrapper.getAttribute("data-strategic-tooltip"), /comfrey fine/);
  assert.match(wrapper.querySelector("[data-herbal-method-status]").textContent, /not an authored/);
});

test("compatible selection previews output, duration, units, risk, and degradation", () => {
  const { window, document } = fixture();
  const ingredient = document.querySelector("[data-herbal-ingredient]");
  const method = document.querySelector('[value="infuse_decoct"]');
  ingredient.setAttribute("checked", "");
  ingredient.dispatchEvent(new window.Event("change", { bubbles: true }));
  method.setAttribute("checked", "");
  method.dispatchEvent(new window.Event("change", { bubbles: true }));
  const preview = document.querySelector("[data-herbal-preview]").textContent;
  assert.match(preview, /Spent herb waste/);
  assert.match(preview, /203 minutes/);
  assert.match(preview, /No additional consumable/);
  assert.match(preview, /Safety warning:/);
  assert.match(preview, /Degradation warning/);
  assert.equal(document.querySelector("[data-herbal-submit]").disabled, false);
});

test("tincture preview names its bounded alcoholic consumable", () => {
  const { window, document } = fixture();
  const poppy = document.querySelector('[data-herbal-ingredient][value="43"]');
  const tincture = document.querySelector('[data-herbal-method][value="tincture"]');
  poppy.setAttribute("checked", "");
  poppy.dispatchEvent(new window.Event("change", { bubbles: true }));
  tincture.setAttribute("checked", "");
  tincture.dispatchEvent(new window.Event("change", { bubbles: true }));
  assert.match(document.querySelector("[data-herbal-preview]").textContent, /Tincture spirit × 1/);
});
