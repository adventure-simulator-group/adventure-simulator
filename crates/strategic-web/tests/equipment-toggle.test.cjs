const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { readRustModuleSource } = require('./rust-module-source.cjs');

const {
  attachmentTargetsForPlacement,
  exactEquipmentError,
  normalizedEquipmentInput,
  parseInputMap,
  parsePlacementOptions,
  parsePlacements,
  selectionForInput,
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
  assert.match(routes, /definition\.slot/);
});
