const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const path = require('node:path');
const { parseHTML } = require('linkedom');

const { createTooltipSystem } = require('../static/tooltips.js');
const styles = fs.readFileSync(path.join(__dirname, '../static/css/strategic.css'), 'utf8');

function fixture(markup = '<button title="Open inventory">Bag</button>') {
  const { window, document } = parseHTML(`<html><body>${markup}</body></html>`);
  Object.defineProperties(window, {
    innerWidth: { value: 320, configurable: true },
    innerHeight: { value: 200, configurable: true },
  });
  const system = createTooltipSystem(document, window);
  system.tooltip.getBoundingClientRect = () => ({ width: 100, height: 30 });
  return { window, document, system };
}

function dispatch(window, target, type, properties = {}) {
  const event = new window.Event(type, { bubbles: true });
  Object.entries(properties).forEach(([key, value]) => Object.defineProperty(event, key, { value }));
  target.dispatchEvent(event);
  return event;
}

test('title tooltips enhance immediately while preserving an accessible description', () => {
  const { window, document, system } = fixture();
  const button = document.querySelector('button');
  button.getBoundingClientRect = () => ({ left: 120, right: 160, top: 80, bottom: 100, width: 40, height: 20 });

  dispatch(window, button, 'pointerover', { pointerType: 'mouse' });

  assert.equal(button.hasAttribute('title'), false);
  assert.equal(button.dataset.strategicTooltip, 'Open inventory');
  assert.equal(button.getAttribute('aria-describedby'), 'strategic-tooltip');
  assert.equal(system.tooltip.textContent, 'Open inventory');
  assert.equal(system.tooltip.hidden, false);
  assert.equal(system.tooltip.getAttribute('aria-hidden'), 'false');
  assert.equal(system.tooltip.dataset.placement, 'top');
  assert.equal(system.tooltip.style.left, '90px');
  assert.equal(system.tooltip.style.top, '42px');

  dispatch(window, button, 'pointerout', { pointerType: 'mouse', relatedTarget: document.body });
  assert.equal(system.tooltip.hidden, true);
  assert.equal(button.hasAttribute('aria-describedby'), false);

  dispatch(window, button, 'pointerover', { pointerType: 'mouse' });
  dispatch(window, button, 'pointerleave', { pointerType: 'mouse' });
  assert.equal(system.tooltip.hidden, true);
});

test('an otherwise unnamed icon retains the title as its accessible name', () => {
  const { window, document } = fixture('<span title="Armour condition"></span>');
  const icon = document.querySelector('span');
  dispatch(window, icon, 'focusin');
  assert.equal(icon.getAttribute('aria-label'), 'Armour condition');
  assert.equal(icon.hasAttribute('data-strategic-tooltip-generated-label'), true);
});

test('delegation supports replacement content and Escape and blur dismiss it', () => {
  const { window, document, system } = fixture('<main></main>');
  const dynamic = document.createElement('button');
  dynamic.title = 'New live action';
  dynamic.textContent = 'Action';
  dynamic.getBoundingClientRect = () => ({ left: 5, right: 25, top: 3, bottom: 23, width: 20, height: 20 });
  document.querySelector('main').append(dynamic);

  dispatch(window, dynamic, 'focusin');
  assert.equal(system.tooltip.textContent, 'New live action');
  assert.equal(system.tooltip.dataset.placement, 'bottom');
  assert.equal(system.tooltip.style.left, '8px');
  assert.equal(system.tooltip.style.top, '31px');

  dispatch(window, document, 'keydown', { key: 'Escape' });
  assert.equal(system.tooltip.hidden, true);
  dispatch(window, dynamic, 'focusin');
  dispatch(window, dynamic, 'focusout', { relatedTarget: document.body });
  assert.equal(system.tooltip.hidden, true);
});

test('touch pointers neither open nor leave a focused tooltip trapped', () => {
  const { window, document, system } = fixture();
  const button = document.querySelector('button');
  dispatch(window, button, 'pointerover', { pointerType: 'touch' });
  assert.equal(system.tooltip.hidden, true);

  dispatch(window, button, 'pointerdown', { pointerType: 'touch' });
  dispatch(window, button, 'focusin');
  assert.equal(system.tooltip.hidden, true);
});

test('existing accessible names and descriptions are not overwritten', () => {
  const { window, document, system } = fixture(
    '<button aria-label="Bag" aria-describedby="inventory-help" title="Open inventory"></button>',
  );
  const button = document.querySelector('button');
  dispatch(window, button, 'pointerover', { pointerType: 'mouse' });
  assert.equal(button.getAttribute('aria-label'), 'Bag');
  assert.equal(button.getAttribute('aria-describedby'), 'inventory-help strategic-tooltip');
  system.hide();
  assert.equal(button.getAttribute('aria-describedby'), 'inventory-help');
});

test('tooltip text identical to the accessible name is not linked as a duplicate description', () => {
  const { window, document, system } = fixture(
    '<button aria-label="Zoom in" aria-describedby="map-help" data-strategic-tooltip="Zoom in"></button>',
  );
  const button = document.querySelector('button');
  dispatch(window, button, 'focusin');
  assert.equal(system.tooltip.textContent, 'Zoom in');
  assert.equal(system.tooltip.hidden, false);
  assert.equal(button.getAttribute('aria-describedby'), 'map-help');
  system.hide();
  assert.equal(button.getAttribute('aria-describedby'), 'map-help');
});

test('nested meter segments show their own multiline value tooltip', () => {
  const { window, document, system } = fixture(
    '<div data-strategic-tooltip="Filth system"><span data-strategic-tooltip="Blood\n24"></span></div>',
  );
  const segment = document.querySelector('span');
  segment.getBoundingClientRect = () => ({ left: 100, right: 140, top: 80, bottom: 90, width: 40, height: 10 });

  dispatch(window, segment, 'pointerover', { pointerType: 'mouse' });

  assert.equal(system.activeTarget, segment);
  assert.equal(system.tooltip.textContent, 'Blood\n24');
});

test('shared tooltips render above every popup overlay', () => {
  const tooltipZ = Number(styles.match(/#strategic-tooltip,[\s\S]*?z-index:\s*(\d+)/)[1]);
  const characterDialogZ = Number(styles.match(/\.character-action-overlay\s*\{[\s\S]*?z-index:\s*(\d+)/)[1]);
  const medicalDialogZ = Number(styles.match(/\.medical-examination-overlay\s*\{[\s\S]*?z-index:\s*(\d+)/)[1]);

  assert.ok(tooltipZ > characterDialogZ);
  assert.ok(tooltipZ > medicalDialogZ);
});

test('dynamic Bestiary chips use the viewport tooltip and accessible description', () => {
  const { window, document, system } = fixture('<main class="overflowing-chat"></main>');
  const chip = document.createElement('span');
  chip.tabIndex = 0;
  chip.setAttribute('aria-label', 'Werekin Bestiary result: 65%, supports.');
  chip.dataset.strategicTooltip = 'Typical signs: transformed tracks\nCommon strengths: speed';
  chip.textContent = 'Werekin — supports (65%)';
  chip.getBoundingClientRect = () => ({
    left: 280,
    right: 320,
    top: 4,
    bottom: 24,
    width: 40,
    height: 20,
  });
  document.querySelector('main').append(chip);

  dispatch(window, chip, 'focusin');

  assert.equal(system.tooltip.parentElement, document.body);
  assert.equal(chip.getAttribute('aria-describedby'), 'strategic-tooltip');
  assert.equal(system.tooltip.dataset.placement, 'bottom');
  assert.equal(system.tooltip.style.left, '212px');
  assert.equal(system.tooltip.textContent, chip.dataset.strategicTooltip);
});
