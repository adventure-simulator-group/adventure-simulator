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

test('skill tooltips render generated details with aligned correlation percentages', () => {
  const { window, document, system } = fixture(`
    <div tabindex="0"
      data-strategic-tooltip="Human&#10;Governed by Intelligence&#10;500.0 effective hours trained&#10;250.0 hours from correlated skills:&#10;Wildmen | 65%&#10;Fey | 30%"
      data-skill-tooltip='{"name":"Human","governed_by":"Intelligence","trained_hours":500,"correlated_hours":250,"correlations":[{"name":"Wildmen","percent":65},{"name":"Fey","percent":30}]}'></div>
  `);
  const skill = document.querySelector('[data-skill-tooltip]');

  dispatch(window, skill, 'focusin');

  assert.equal(system.tooltip.querySelector('.strategic-tooltip-title').textContent, 'Human');
  assert.deepEqual(
    [...system.tooltip.querySelectorAll('.strategic-skill-tooltip-line')].map((line) => line.textContent),
    [
      'Governed by Intelligence',
      '500.0 effective hours trained',
      '250.0 hours from correlated skills:',
    ],
  );
  assert.deepEqual(
    [...system.tooltip.querySelectorAll('.strategic-skill-tooltip-correlations tr')]
      .map((row) => [...row.querySelectorAll('td')].map((cell) => cell.textContent)),
    [['Wildmen', '65%'], ['Fey', '30%']],
  );
  assert.equal(skill.getAttribute('aria-describedby'), 'strategic-tooltip');
});

test('shared tooltips render above every popup overlay', () => {
  const tooltipZ = Number(styles.match(/#strategic-tooltip,[\s\S]*?z-index:\s*(\d+)/)[1]);
  const characterDialogZ = Number(styles.match(/\.character-action-overlay\s*\{[\s\S]*?z-index:\s*(\d+)/)[1]);

  assert.ok(tooltipZ > characterDialogZ);
});

test('Bestiary tooltips separate main and secondary types, pin, and show enemy facts', () => {
  const { window, document, system } = fixture(
    '<span tabindex="0" role="button" aria-pressed="false" aria-label="Human knowledge" data-tooltip-pinnable data-strategic-tooltip="Human" data-bestiary-name="Human"></span>',
  );
  const skill = document.querySelector('span');
  skill.setAttribute('data-bestiary-enemies', JSON.stringify([
    {
      id: 'skeleton',
      name: 'Skeleton',
      is_primary: false,
      strengths: ['150 J innate resistance reduces edged force'],
      weaknesses: ['No innate padding against blunt force'],
    },
    {
      id: 'ghoul',
      name: 'Ghoul',
      is_primary: false,
      strengths: ['15 J innate padding absorbs blunt force'],
      weaknesses: ['No implemented resistance against edged force'],
    },
    {
      id: 'bandit',
      name: 'Bandit',
      is_primary: true,
      strengths: [],
      weaknesses: [],
    },
  ]));

  dispatch(window, skill, 'focusin');

  assert.equal(system.tooltip.querySelector('.strategic-tooltip-title').textContent, 'Human');
  assert.equal(system.tooltip.getAttribute('role'), 'dialog');
  assert.equal(system.tooltip.getAttribute('aria-modal'), 'false');
  assert.equal(system.tooltip.getAttribute('aria-label'), 'Human');
  assert.deepEqual(
    [...system.tooltip.querySelectorAll('.strategic-tooltip-enemy-group-heading')]
      .map((item) => item.textContent),
    ['Main type', 'Secondary type'],
  );
  assert.deepEqual(
    [...system.tooltip.querySelectorAll('[role="group"]')]
      .map((group) => group.getAttribute('aria-label')),
    ['Main type', 'Secondary type'],
  );
  assert.deepEqual(
    [...system.tooltip.querySelectorAll('.strategic-tooltip-enemy-group.is-primary .strategic-tooltip-enemy')]
      .map((item) => item.textContent),
    ['Bandit'],
  );
  assert.deepEqual(
    [...system.tooltip.querySelectorAll('.strategic-tooltip-enemy-group.is-secondary .strategic-tooltip-enemy')]
      .map((item) => item.textContent),
    ['Skeleton', 'Ghoul'],
  );
  assert.deepEqual(
    [...system.tooltip.querySelectorAll('.strategic-tooltip-enemy')].map((item) => item.textContent),
    ['Bandit', 'Skeleton', 'Ghoul'],
  );
  assert.equal(system.tooltip.querySelectorAll('.strategic-tooltip-section').length, 0);

  dispatch(window, skill, 'click', { detail: 1 });
  dispatch(window, skill, 'pointerout', { pointerType: 'mouse', relatedTarget: document.body });
  assert.equal(system.pinnedTarget, skill);
  assert.equal(skill.getAttribute('aria-pressed'), 'true');
  assert.equal(system.tooltip.hidden, false);

  const skeleton = system.tooltip.querySelector(
    '.strategic-tooltip-enemy-group.is-secondary .strategic-tooltip-enemy',
  );
  dispatch(window, skeleton, 'pointerover', { pointerType: 'mouse' });
  assert.deepEqual(
    [...system.tooltip.querySelectorAll('.strategic-tooltip-strength')].map((item) => item.textContent),
    ['150 J innate resistance reduces edged force'],
  );
  assert.deepEqual(
    [...system.tooltip.querySelectorAll('.strategic-tooltip-weakness')].map((item) => item.textContent),
    ['No innate padding against blunt force'],
  );

  dispatch(window, skill, 'click', { detail: 1 });
  assert.equal(system.pinnedTarget, null);
  assert.equal(skill.getAttribute('aria-pressed'), 'false');
  assert.equal(system.tooltip.hidden, true);
});

test('keyboard pinning enters enemy lore, Escape restores focus, and removed pins clear', () => {
  const { window, document, system } = fixture(`
    <span tabindex="0" role="button" aria-pressed="false"
      data-tooltip-pinnable data-strategic-tooltip="Undead" data-bestiary-name="Undead"
      data-bestiary-enemies='[{"id":"skeleton","name":"Skeleton","is_primary":true,"strengths":[],"weaknesses":[]}]'>
      Undead
    </span>
  `);
  const skill = document.querySelector('[data-tooltip-pinnable]');
  skill.getBoundingClientRect = () => ({
    left: 120, right: 160, top: 80, bottom: 100, width: 40, height: 20,
  });
  dispatch(window, skill, 'focusin');
  let enteredEnemyLore = false;
  let restoredTriggerFocus = false;
  window.HTMLElement.prototype.focus = function focus() {
    if (this.classList?.contains('strategic-tooltip-enemy')) enteredEnemyLore = true;
    if (this === skill) {
      restoredTriggerFocus = true;
      dispatch(window, skill, 'focusin');
    }
  };

  dispatch(window, skill, 'keydown', { key: 'Enter' });

  assert.equal(system.pinnedTarget, skill);
  assert.equal(enteredEnemyLore, true);

  const enemy = system.tooltip.querySelector('.strategic-tooltip-enemy');
  dispatch(window, enemy, 'keydown', { key: 'Escape' });
  assert.equal(system.pinnedTarget, null);
  assert.equal(system.tooltip.hidden, true);
  assert.equal(restoredTriggerFocus, true);

  dispatch(window, skill, 'click');
  skill.remove();
  system.position();
  assert.equal(system.pinnedTarget, null);
  assert.equal(system.tooltip.hidden, true);
});

test('dynamic Bestiary chips use the viewport tooltip and accessible description', () => {
  const { window, document, system } = fixture('<main class="overflowing-chat"></main>');
  const chip = document.createElement('span');
  chip.tabIndex = 0;
  chip.setAttribute('aria-label', 'Werekin Bestiary result: 65%, supports.');
  chip.dataset.strategicTooltip = 'Werekin';
  chip.dataset.bestiaryName = 'Werekin';
  chip.dataset.bestiaryEnemies = JSON.stringify([
    { id: 'werewolf', name: 'Werewolf', is_primary: true, strengths: [], weaknesses: [] },
  ]);
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
  assert.equal(system.tooltip.querySelector('.strategic-tooltip-enemy').textContent, 'Werewolf');
});
