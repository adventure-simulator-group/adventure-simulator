const test = require('node:test');
const assert = require('node:assert/strict');

const { activityKind, clock, signed } = require('../static/immediate-activity.js');

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
