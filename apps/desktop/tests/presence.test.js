import {test} from 'node:test';
import assert from 'node:assert/strict';
import {describeView, newViewId, controlIntent, presenceChips, holderName, TYPING_GRACE_MS} from '../src/presence.js';

const views = [{view: 'view-mac', label: 'Mac app', kind: 'app', age_ms: 10}, {view: 'view-phone', label: 'Android phone', kind: 'phone', age_ms: 900}];
const p = (o = {}) => ({views, controller: null, controller_known: false, controller_idle_ms: null, ...o});
test('a tap just takes control when nobody has it, and is a no-op when this view has it', () => {
  assert.equal(controlIntent({presence: p(), self: 'view-mac', held: false}), 'take');
  assert.equal(controlIntent({presence: p({controller: 'view-mac', controller_known: true}), self: 'view-mac', held: true}), 'hold');
});
test('someone typing moments ago is not interrupted by one tap; a second tap takes over', () => {
  const busy = p({controller: 'view-phone', controller_known: true, controller_idle_ms: 400});
  assert.equal(controlIntent({presence: busy, self: 'view-mac', held: false}), 'confirm');
  assert.equal(controlIntent({presence: busy, self: 'view-mac', held: false, lastTapAgoMs: 1200}), 'takeover');
  assert.equal(controlIntent({presence: busy, self: 'view-mac', held: false, lastTapAgoMs: 60_000}), 'confirm');
});
test('a holder who has gone quiet, or our own stale lease, is taken over at once', () => {
  assert.equal(controlIntent({presence: p({controller: 'view-phone', controller_known: true, controller_idle_ms: TYPING_GRACE_MS}), self: 'view-mac', held: false}), 'takeover');
  assert.equal(controlIntent({presence: p({controller: 'view-mac', controller_known: true, controller_idle_ms: 0}), self: 'view-mac', held: false}), 'takeover');
  assert.equal(controlIntent({presence: null, self: 'view-mac', held: false}), 'take', 'older keeper: polite attempt');
});
test('the people line marks the typist and this view, and admits an unnamed holder', () => {
  const chips = presenceChips(p({controller: 'view-phone', controller_known: true}), 'view-mac');
  assert.deepEqual(chips.map(c => [c.label, c.typing, c.you]), [['Android phone', true, false], ['Mac app', false, true]]);
  assert.equal(holderName(p({controller: 'view-phone', controller_known: true}), 'view-mac'), 'Android phone');
  assert.equal(holderName(p({controller: null, controller_known: true}), 'view-mac'), 'Another view');
  assert.equal(holderName(p(), 'view-mac'), '');
});
test('views name themselves plainly and ids fit the keeper rule', () => {
  assert.deepEqual(describeView('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)', 'v').label, 'Mac app');
  assert.equal(describeView('Mozilla/5.0 (Macintosh) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36').label, 'Chrome');
  assert.equal(describeView('Mozilla/5.0 (Linux; Android 15; moto g67) AppleWebKit/537.36 Chrome/140 Mobile Safari/537.36').kind, 'phone');
  assert.match(newViewId(() => '123e4567-e89b-12d3-a456-426614174000'), /^[a-zA-Z0-9-]{8,64}$/);
});
