const test = require('node:test');
const assert = require('node:assert/strict');

const {
  activityKind, clock, signed, professionReward, wrappedFocusTarget, FOCUSABLE_SELECTOR,
} = require('../static/immediate-activity.js');

test('activity duration preserves the current minute and wraps midnight', () => {
  assert.equal(clock(23 * 60 + 37 + 60), '00:37');
  assert.equal(clock(12 * 60 + 7 + 24 * 60), '12:07');
});

test('allocation names map to authoritative activity discriminators', () => {
  assert.equal(activityKind('combat_training_minutes'), 'combat_training');
  assert.equal(activityKind('profession_practice_minutes'), 'profession_practice');
  assert.equal(activityKind('prayer_minutes'), 'prayer');
});

test('preview formatting shows signed rounded outcomes', () => {
  assert.equal(signed('gold', 0.49), '0');
  assert.equal(signed('gold', 0.51), '+1');
  assert.equal(signed('morale', -0.14), '-0.1');
});

test('profession rewards use persisted accrual and authoritative tier thresholds', () => {
  const eightHours = 8 * 60 * 1440;
  assert.deepEqual(professionReward({
    accrued: eightHours - 60 * 1440, threshold: eightHours, sign: -1, reward: 'gold',
  }, 60), { gold: -1, virtue: 0 });
  assert.deepEqual(professionReward({
    accrued: 0, threshold: eightHours, sign: 1, reward: 'gold',
  }, 7 * 60), { gold: 0, virtue: 0 });
  assert.deepEqual(professionReward({
    accrued: 0, threshold: 2 * 60 * 1440, sign: 1, reward: 'virtue',
  }, 4 * 60), { gold: 0, virtue: 2 });
});

test('focus wrapping excludes hidden controls and wraps in both directions', () => {
  assert.match(FOCUSABLE_SELECTOR, /input:not\(\[type="hidden"\]\)/);
  const first = {};
  const middle = {};
  const last = {};
  assert.equal(wrappedFocusTarget(first, [first, middle, last], true), last);
  assert.equal(wrappedFocusTarget(last, [first, middle, last], false), first);
  assert.equal(wrappedFocusTarget(middle, [first, middle, last], false), null);
});

test('activity dialogs remount after strategic soft navigation', () => {
  const source = require('node:fs').readFileSync(
    require('node:path').join(__dirname, '../static/immediate-activity.js'),
    'utf8',
  );
  assert.match(source, /strategic-page-mounted/);
  assert.match(source, /\(\) => mountAll\(\)/);
});
