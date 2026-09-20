import {test} from 'node:test';
import assert from 'node:assert/strict';
import {runAction, Timeline} from '../src/view-snapshot.js';

test('actions are interface navigation only; anything else is refused', () => {
  const did = []; const h = {reload: () => did.push('reload'), refresh: () => did.push('refresh'), activity: o => did.push('activity ' + o), select: id => did.push('select ' + id), snapshot: () => did.push('snapshot')};
  for (const a of ['reload', 'refresh', 'activity:open', 'activity:close', 'select:a600d7f7cafe', 'snapshot']) assert.equal(runAction(a, h), true, a);
  assert.deepEqual(did, ['reload', 'refresh', 'activity true', 'activity false', 'select a600d7f7cafe', 'snapshot']);
  for (const bad of ['input:rm -rf', 'type:ls', 'stop:a600d7f7', 'create', 'control', '__proto__', 'constructor', 'toString']) assert.equal(runAction(bad, h), false, bad);
  runAction('select:../../etc', h); runAction('activity:maybe', h); assert.equal(did.length, 6);
});
test('the timeline is bounded, relative and short', () => {
  let now = 1000; const t = new Timeline(() => now, 3);
  now = 1250; t.mark('devices'); now = 3000; t.mark('revealed', 1048576); t.mark('x'.repeat(200), 'y'.repeat(200)); t.mark('last');
  assert.equal(t.events.length, 3); assert.deepEqual(t.events[0], {t: 2000, name: 'revealed', detail: 1048576});
  assert.equal(t.events[1].name.length, 40); assert.equal(t.events[1].detail.length, 60);
});
