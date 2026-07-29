const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const root = path.join(__dirname, "..");

test("herbalism stage modal has accessible controls and a minimal authority payload", () => {
  const source = fs.readFileSync(path.join(root, "src/templates/settlement/trade.rs"), "utf8");
  assert.match(source, /aria-labelledby="herbalism-dialog-title"/);
  assert.match(source, /name="inventory_item_id"/);
  assert.match(source, /name="method"/);
  assert.doesNotMatch(source, /name="(?:output|potency|profile|route|dose)"/);
  assert.match(source, /data-herbal-preview role="status"/);
  assert.match(source, /data-strategic-tooltip/);
});

test("herbalism client disables incompatible methods and renders degradation preview", () => {
  const source = fs.readFileSync(path.join(root, "static/herbalism.js"), "utf8");
  assert.match(source, /choice\.disabled = !available/);
  assert.match(source, /Degradation warning/);
  assert.match(source, /strategicHerbalism/);
});

test("gateway handler uses a closed parse and the selected session actor", () => {
  const source = fs.readFileSync(
    path.join(root, "src/routes/settlements/party/herbalism.rs"),
    "utf8",
  );
  assert.match(source, /session\.character_id_u64\(\) != Some\(character_id\)/);
  assert.match(source, /"dry_grind"/);
  assert.match(source, /"infuse_decoct"/);
  assert.match(source, /"tincture"/);
  assert.match(source, /prepare_herbal_remedy/);
});
