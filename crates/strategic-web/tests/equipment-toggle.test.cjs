const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { readRustModuleSource } = require('./rust-module-source.cjs');

const { exactEquipmentError } = require('../static/equipment-toggle.js');

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
  assert.match(client, /inventory_item_id: checkbox\.dataset\.inventoryItemId/);
  assert.match(client, /equipped: String\(checkbox\.checked\)/);
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
