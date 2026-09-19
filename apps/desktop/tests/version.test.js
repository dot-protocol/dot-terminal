import {test} from 'node:test';
import assert from 'node:assert/strict';
import {parseVersion, watchVersion, safeToReload} from '../src/version.js';

const v = build => ({schema: 'dot.ui-version.v1', build, commit: 'abc12345', builtAt: '2026-09-19T17:00:00Z'});
const timers = {setInterval: () => 1, clearInterval: () => {}};
test('a version file is strict: schema, plain build id, nothing else trusted', () => {
  assert.equal(parseVersion(v('abc12345.20260919T1700')).build, 'abc12345.20260919T1700');
  assert.throws(() => parseVersion({...v('x'), schema: 'other'})); assert.throws(() => parseVersion(v('<script>')));
  assert.equal(parseVersion({...v('x'), commit: 'a b', builtAt: 'never'}).commit, '');
});
test('a new build is announced once; the same build and a failed check are not updates', async () => {
  let served = v('one'), fail = false; const updates = [], checks = [];
  const w = watchVersion({current: parseVersion(v('one')), load: async () => { if (fail) throw new Error('offline'); return served; }, onUpdate: n => updates.push(n.build), onCheck: ok => checks.push(ok), timers});
  await w.check(); assert.deepEqual(updates, []);
  fail = true; await w.check(); assert.deepEqual(updates, []); assert.equal(checks.at(-1), false);
  fail = false; served = v('two'); await w.check(); await w.check(); assert.deepEqual(updates, ['two']);
  served = v('one'); await w.check(); assert.deepEqual(updates, ['two'], 'rolling back to the running build is not an update');
  served = v('three'); w.stop(); await w.check(); assert.deepEqual(updates, ['two']);
});
test('never reload under someone who is typing', () => {
  assert.equal(safeToReload({controlHeld: false, queuedBytes: 0, dialogOpen: false}), true);
  for (const busy of [{controlHeld: true}, {queuedBytes: 3}, {dialogOpen: true}]) assert.equal(safeToReload({controlHeld: false, queuedBytes: 0, dialogOpen: false, ...busy}), false);
});
