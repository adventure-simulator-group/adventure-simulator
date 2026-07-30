const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { parseHTML } = require('linkedom');
const { readRustModuleSource } = require('./rust-module-source.cjs');

const {
  attachmentTargetsForPlacement,
  chooseSlot,
  exactEquipmentError,
  normalizedEquipmentInput,
  parseInputMap,
  parsePlacementOptions,
  parsePlacements,
  selectionForInput,
  showSlotPreview,
  hideSlotPreview,
} = require('../static/equipment-toggle.js');

test('equipment errors preserve the exact reducer response', async () => {
  const response = {
    status: 400,
    statusText: 'Bad Request',
    text: async () => 'Goslar restricts arms; present a recognized organization',
  };
  assert.equal(
    await exactEquipmentError(response),
    'Goslar restricts arms; present a recognized organization',
  );
});

test('authored placement alternatives remain atomic and ordered', () => {
  assert.deepEqual(
    parsePlacements('LeftArm|RightArm|Chest, Stomach'),
    ['LeftArm', 'RightArm', 'Chest, Stomach'],
  );
  assert.deepEqual(parsePlacements(''), []);
});

test('attachment placements preserve parent target identity', () => {
  assert.deepEqual(
    parsePlacementOptions(JSON.stringify([{
      placementIndex: 2,
      label: 'contained',
      requirements: [{
        requirementIndex: 0,
        channel: 'Contents',
        targets: [
          { parentInventoryItemId: 41, attachmentPointId: 'blade', label: 'Sword sheath / blade' },
          { parentInventoryItemId: 9, attachmentPointId: 'contents', label: 'Leather satchel / contents' },
        ],
      }],
    }])),
    [
      {
        placementIndex: 2,
        label: 'contained',
        requirements: [{
          requirementIndex: 0,
          channel: 'Contents',
          targets: [
            { parentInventoryItemId: 41, attachmentPointId: 'blade', label: 'Sword sheath / blade' },
            { parentInventoryItemId: 9, attachmentPointId: 'contents', label: 'Leather satchel / contents' },
          ],
        }],
      },
    ],
  );
  assert.deepEqual(parsePlacementOptions('{bad json'), []);
});

test('multi-point placements submit one selected target per requirement', () => {
  const placement = {
    placementIndex: 4,
    requirements: [
      {
        requirementIndex: 0,
        targets: [
          { parentInventoryItemId: 11, attachmentPointId: 'left' },
          { parentInventoryItemId: 12, attachmentPointId: 'right' },
        ],
      },
      {
        requirementIndex: 1,
        targets: [
          { parentInventoryItemId: 21, attachmentPointId: 'upper' },
          { parentInventoryItemId: 22, attachmentPointId: 'lower' },
        ],
      },
    ],
  };
  assert.deepEqual(attachmentTargetsForPlacement(placement, { 0: 1, 1: 0 }), [
    {
      requirement_index: 0,
      parent_inventory_item_id: 12,
      attachment_point_id: 'right',
    },
    {
      requirement_index: 1,
      parent_inventory_item_id: 21,
      attachment_point_id: 'upper',
    },
  ]);
});

test('slot input chooses the outermost compatible attachment target', () => {
  const placements = [{
    placementIndex: 2,
    inputRanks: {},
    requirements: [{
      requirementIndex: 0,
      targets: [
        {
          parentInventoryItemId: 11,
          attachmentPointId: 'inner',
          inputRanks: { q: 60000 },
        },
        {
          parentInventoryItemId: 12,
          attachmentPointId: 'outer',
          inputRanks: { q: 160000 },
        },
      ],
    }],
  }];
  assert.deepEqual(selectionForInput(placements, 'q'), {
    placementIndex: 2,
    attachmentTargets: [{
      requirement_index: 0,
      parent_inventory_item_id: 12,
      attachment_point_id: 'outer',
    }],
  });
  assert.equal(selectionForInput(placements, 'e'), null);
});

test('automatic target selection respects attachment-point capacity', () => {
  const shared = {
    parentInventoryItemId: 12,
    attachmentPointId: 'single',
    freeCapacity: 1,
    inputRanks: { q: 160000 },
  };
  const alternate = {
    parentInventoryItemId: 12,
    attachmentPointId: 'second',
    freeCapacity: 1,
    inputRanks: { q: 150000 },
  };
  const selection = selectionForInput([{
    placementIndex: 3,
    requirements: [
      { requirementIndex: 0, targets: [shared, alternate] },
      { requirementIndex: 1, targets: [shared, alternate] },
    ],
  }], 'q');
  assert.deepEqual(selection.attachmentTargets.map((target) => target.attachment_point_id), [
    'single',
    'second',
  ]);
});

test('root placements expose every applicable key and preserve authored alternatives', () => {
  const placements = [
    { placementIndex: 0, inputRanks: { w: 40000, s: 40000 }, requirements: [] },
    { placementIndex: 1, inputRanks: { v: 40000 }, requirements: [] },
  ];
  assert.equal(selectionForInput(placements, 'w').placementIndex, 0);
  assert.equal(selectionForInput(placements, 's').placementIndex, 0);
  assert.equal(selectionForInput(placements, 'v').placementIndex, 1);
  assert.equal(selectionForInput(placements, 'b'), null);
});

test('occupied eligible root slots remain selectable and expose their item icon', () => {
  const occupant = {
    inventoryItemId: 19,
    itemName: 'Padded skirt',
    icon: '/static/icons/game/skirt.svg',
  };
  assert.deepEqual(selectionForInput([{
    placementIndex: 3,
    inputRanks: { y: 20000 },
    inputOccupants: { y: occupant },
    requirements: [],
  }], 'y'), {
    placementIndex: 3,
    attachmentTargets: [],
    occupant,
  });
});

test('a full compatible attachment point remains selectable as a swap', () => {
  const occupant = {
    inventoryItemId: 24,
    itemName: 'Arming sword',
    icon: '/static/icons/game/broadsword.svg',
  };
  assert.deepEqual(selectionForInput([{
    placementIndex: 1,
    inputRanks: {},
    requirements: [{
      requirementIndex: 0,
      targets: [{
        parentInventoryItemId: 12,
        attachmentPointId: 'sheath',
        freeCapacity: 0,
        occupants: [occupant],
        inputRanks: { q: 160000 },
      }],
    }],
  }], 'q'), {
    placementIndex: 1,
    attachmentTargets: [{
      requirement_index: 0,
      parent_inventory_item_id: 12,
      attachment_point_id: 'sheath',
    }],
    occupant,
  });
});

test('mixed body and attachment placements require the same valid key', () => {
  const placement = {
    placementIndex: 4,
    hasBody: true,
    inputRanks: { w: 50000 },
    requirements: [{
      requirementIndex: 0,
      targets: [{
        parentInventoryItemId: 12,
        attachmentPointId: 'mount',
        freeCapacity: 1,
        inputRanks: { w: 160000, q: 160000 },
      }],
    }],
  };
  assert.equal(selectionForInput([placement], 'q'), null);
  assert.equal(selectionForInput([placement], 'w').placementIndex, 4);
});

test('keyboard input is normalized only while the slot chooser handles it', () => {
  assert.equal(normalizedEquipmentInput({ key: 'Q' }), 'q');
  assert.equal(normalizedEquipmentInput({ key: 'Tab' }), 'tab');
  assert.equal(normalizedEquipmentInput({ key: 'Escape' }), '');
  assert.deepEqual(
    parseInputMap('[{"input":"w","label":"W","row":1,"column":2}]'),
    [{ input: 'w', label: 'W', row: 1, column: 2 }],
  );
});

test('slot chooser uses an icon cell, an X close control, and red invalid-key feedback', async () => {
  const { window, document } = parseHTML('<html><body><button id="equip"></button></body></html>');
  global.window = window;
  global.document = document;
  window.HTMLElement.prototype.showModal = function showModal() {
    this.setAttribute('open', '');
  };
  window.HTMLElement.prototype.close = function close() {
    this.removeAttribute('open');
    this.dispatchEvent(new window.Event('close'));
  };
  const control = document.querySelector('#equip');
  control.dataset.inventoryItemId = '7';
  control.dataset.equipmentInputMap = JSON.stringify([
    { input: 'q', label: 'Q', row: 0, column: 0, locations: ['Left belt'] },
    { input: 'y', label: 'Y', row: 1, column: 1, locations: ['Stomach'] },
  ]);
  control.dataset.equipmentPlacementOptions = JSON.stringify([{
    placementIndex: 0,
    inputRanks: { y: 20000 },
    inputOccupants: {
      y: {
        inventoryItemId: 7,
        itemName: 'Padded skirt',
        icon: '/static/icons/game/skirt.svg',
      },
    },
    requirements: [],
  }]);

  const result = chooseSlot(control);
  const dialog = document.querySelector('.equipment-slot-modal');
  assert.equal(dialog.querySelector('h2'), null);
  assert.equal(dialog.querySelector('p'), null);
  assert.equal(dialog.querySelector('.equipment-slot-cancel'), null);
  assert.equal(
    dialog.querySelector('.equipment-slot-close').getAttribute('aria-label'),
    'Close equipment slot picker',
  );
  const current = dialog.querySelector('[data-equipment-input="y"]');
  assert.ok(current.querySelector('.equipment-slot-choice-icon[role="img"]'));
  assert.equal(current.classList.contains('is-current-placement'), true);
  assert.equal(current.getAttribute('aria-current'), 'true');
  assert.match(current.getAttribute('aria-label'), /current placement/);

  const invalidKey = new window.Event('keydown', { bubbles: true, cancelable: true });
  invalidKey.key = 'q';
  dialog.dispatchEvent(invalidKey);
  assert.equal(
    dialog.querySelector('[data-equipment-input="q"]').classList.contains('is-invalid-input'),
    true,
  );

  dialog.querySelector('.equipment-slot-close').click();
  assert.equal(await result, null);
  delete global.document;
  delete global.window;
});

test('slot preview mirrors the keyboard map without creating a modal', () => {
  const { window, document } = parseHTML('<html><body><button id="equip"></button></body></html>');
  global.window = window;
  global.document = document;
  const control = document.querySelector('#equip');
  control.dataset.equipmentToggle = '';
  control.dataset.equipmentEquipped = 'false';
  control.dataset.equipmentInputMap = JSON.stringify([
    { input: 'q', label: 'Q', row: 0, column: 0, locations: ['Left belt'] },
    { input: 'g', label: 'G', row: 1, column: 1, locations: ['Chest'] },
  ]);
  control.dataset.equipmentPlacementOptions = JSON.stringify([{
    placementIndex: 0,
    inputRanks: { g: 20000 },
    inputOccupants: {},
    requirements: [],
  }]);

  const preview = showSlotPreview(control);
  assert.equal(preview.getAttribute('aria-hidden'), 'true');
  assert.equal(preview.querySelectorAll('.equipment-slot-choice').length, 2);
  assert.equal(preview.querySelector('[data-equipment-input="q"]').disabled, true);
  assert.equal(preview.querySelector('[data-equipment-input="g"]').disabled, false);
  assert.equal(document.querySelector('.equipment-slot-modal'), null);

  hideSlotPreview();
  assert.equal(document.querySelector('.equipment-slot-preview'), null);
  delete global.document;
  delete global.window;
});

test('hover reassigns equipped or unequipped items while keyboard focus waits for Space', async () => {
  const { window, document } = parseHTML(
    '<html><body><main id="strategic-page"><button id="equip"></button></main></body></html>',
  );
  global.window = window;
  global.document = document;
  window.HTMLElement.prototype.showModal = function showModal() {
    this.setAttribute('open', '');
  };
  window.HTMLElement.prototype.close = function close() {
    this.removeAttribute('open');
    this.dispatchEvent(new window.Event('close'));
  };
  const mutations = [];
  window.strategicSubmitMutation = async (path, options) => {
    mutations.push({ path, body: options.body });
  };
  const control = document.querySelector('#equip');
  control.dataset.inventoryItemId = '7';
  control.dataset.equipmentToggle = '';
  control.dataset.equipmentEquipped = 'false';
  control.dataset.equipmentInputMap = JSON.stringify([
    { input: 'g', label: 'G', row: 1, column: 1, locations: ['Chest'] },
  ]);
  control.dataset.equipmentPlacementOptions = JSON.stringify([{
    placementIndex: 0,
    inputRanks: { g: 20000 },
    inputOccupants: {},
    requirements: [],
  }]);

  const modulePath = require.resolve('../static/equipment-toggle.js');
  delete require.cache[modulePath];
  require(modulePath);

  control.focus();
  const focusIn = new window.Event('focusin', { bubbles: true });
  control.dispatchEvent(focusIn);
  assert.ok(document.querySelector('.equipment-slot-preview'));
  const focusedKey = new window.Event('keydown', { bubbles: true, cancelable: true });
  focusedKey.key = 'g';
  control.dispatchEvent(focusedKey);
  await Promise.resolve();
  assert.equal(mutations.length, 0);

  control.click();
  assert.ok(document.querySelector('.equipment-slot-modal'));
  document.querySelector('.equipment-slot-close').click();

  control.blur();
  const hover = new window.Event('mouseover', { bubbles: true });
  hover.relatedTarget = null;
  control.dispatchEvent(hover);
  assert.ok(document.querySelector('.equipment-slot-preview'));
  const hoveredKey = new window.Event('keydown', { bubbles: true, cancelable: true });
  hoveredKey.key = 'g';
  document.dispatchEvent(hoveredKey);
  await Promise.resolve();
  assert.equal(mutations.length, 1);
  assert.equal(mutations[0].path, '/api/equipment');
  assert.equal(mutations[0].body.get('placement_index'), '0');

  control.disabled = false;
  control.dataset.equipmentEquipped = 'true';
  control.dispatchEvent(hover);
  assert.ok(document.querySelector('.equipment-slot-preview'));
  const equippedHoverKey = new window.Event('keydown', {
    bubbles: true,
    cancelable: true,
  });
  equippedHoverKey.key = 'g';
  document.dispatchEvent(equippedHoverKey);
  await Promise.resolve();
  assert.equal(mutations.length, 2);
  assert.equal(mutations[1].body.get('equipped'), 'true');

  control.disabled = false;
  control.focus();
  control.dispatchEvent(focusIn);
  assert.ok(document.querySelector('.equipment-slot-preview'));
  const space = new window.Event('keydown', { bubbles: true, cancelable: true });
  space.key = ' ';
  space.code = 'Space';
  control.dispatchEvent(space);
  assert.ok(document.querySelector('.equipment-slot-modal'));
  document.querySelector('.equipment-slot-close').click();

  delete require.cache[modulePath];
  delete global.document;
  delete global.window;
});

test('equipment errors have an HTTP fallback when the reducer body is empty', async () => {
  const response = {
    status: 503,
    statusText: 'Service Unavailable',
    text: async () => '',
  };
  assert.equal(await exactEquipmentError(response), '503 Service Unavailable');
});

test('medication checkbox submits no browser-selected medical parameters', () => {
  const client = fs.readFileSync(
    path.join(__dirname, '..', 'static', 'equipment-toggle.js'),
    'utf8',
  );
  assert.match(client, /inventory_item_id: control\.dataset\.inventoryItemId/);
  assert.match(client, /await equipmentMutation\(checkbox, checkbox\.checked\)/);
  for (const parameter of ['patient_id', 'route', 'amount_milliunits', 'region']) {
    assert.doesNotMatch(client, new RegExp(`${parameter}:`));
  }
});

test('parameterized preparation form and browser route are absent', () => {
  const client = fs.readFileSync(
    path.join(__dirname, '..', 'static', 'equipment-toggle.js'),
    'utf8',
  );
  const health = fs.readFileSync(
    path.join(__dirname, '..', 'src', 'templates', 'settlement', 'character_health.rs'),
    'utf8',
  );
  const routes = readRustModuleSource(
    path.join(__dirname, '..', 'src', 'routes', 'settlements', 'mod.rs'),
  );
  const healthProduction = health.split('#[cfg(test)]')[0];
  const removedHeading = ['Administer', ' preparation'].join('');
  const removedEmptyState = ['No prepared', ' interventions'].join('');
  assert.equal(healthProduction.includes(removedHeading), false);
  assert.equal(healthProduction.includes(removedEmptyState), false);
  assert.doesNotMatch(healthProduction, /name="(?:route|amount_milliunits|region)"/);
  const removedRoute = ['physiology', '/administer'].join('');
  assert.equal(routes.includes(removedRoute), false);
  assert.match(routes, /standard_medication_administration/);
  assert.match(routes, /"equip_item"/);
  assert.match(routes, /"replace_item_at_placement"/);
  assert.match(client, /replace_occupied: 'true'/);
  assert.match(routes, /definition\.slot/);
});
