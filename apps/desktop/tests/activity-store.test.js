import {test} from 'node:test';
import assert from 'node:assert/strict';
import {ActivityStore, QUIET_MS} from '../src/activity-store.js';
import {describe, ago} from '../src/trajectory.js';

function rig(options = {}) { let now = 1_000_000, changes = 0; const s = new ActivityStore({clock: () => now, onChange: () => changes++, ...options}); return {s, at: ms => { now += ms; }, changes: () => changes}; }

test('nothing is recorded until a PTY is bound, and a new PTY starts a clean list', () => {
  const {s} = rig();
  s.output(10); s.mark('gap'); assert.equal(s.entries().length, 0); assert.equal(s.summary().state, 'unbound');
  s.bind({id: 'a', kind: 'dot'}); s.output(10); assert.deepEqual(s.entries().map(e => e.kind), ['output', 'attached']);
  s.bind({id: 'b', kind: 'dot'}); assert.deepEqual(s.entries().map(e => e.kind), ['attached'], 'the last PTY’s activity does not follow you');
});

test('reads inside a burst extend one row; quiet closes it; the newest row is first', () => {
  const {s, at} = rig(); s.bind({id: 'a'});
  s.output(100); at(30); s.output(50); at(30); s.output(25);
  let [top] = s.entries(); assert.deepEqual([top.kind, top.bytes, top.reads, top.open], ['output', 175, 3, true]);
  assert.equal(s.summary().state, 'streaming');
  at(QUIET_MS - 1); assert.equal(s.tick(), false); at(1); assert.equal(s.tick(), true);
  assert.equal(s.entries()[0].open, false); assert.equal(s.summary().state, 'quiet');
  at(5000); s.output(7);
  assert.deepEqual(s.entries().map(e => [e.kind, e.bytes]), [['output', 7], ['output', 175], ['attached', undefined]]);
});

test('an event is a boundary: output after a resize is a new row, in time order', () => {
  const {s, at} = rig(); s.bind({id: 'a'});
  s.output(1); at(10); s.mark('resize', {cols: 105, rows: 28}); at(10); s.output(2);
  assert.deepEqual(s.entries().map(e => e.kind), ['output', 'resize', 'output', 'attached']);
});

test('only known shapes are stored; free text can never enter', () => {
  const {s} = rig(); s.bind({id: 'a'}); const before = s.entries().length;
  s.mark('resize', {cols: 'ls -la', rows: 2}); s.mark('control', {state: '<b>'}); s.mark('input-stopped', {reason: 'rm -rf'}); s.mark('note', {text: 'secret'}); s.output('many');
  assert.equal(s.entries().length, before);
  s.mark('control', {state: 'taken', text: 'secret'}); assert.equal(s.entries()[0].text, undefined);
  s.mark('exited'); s.mark('exited'); assert.equal(s.entries().filter(e => e.kind === 'exited').length, 1);
});

test('retention is bounded and drops the oldest', () => {
  const {s, at} = rig({limit: 5}); s.bind({id: 'a'});
  for (let i = 1; i <= 9; i++) { at(1); s.mark('resize', {cols: i, rows: 1}); }
  assert.deepEqual(s.entries().map(e => e.cols), [9, 8, 7, 6, 5]);
});

test('a snapshot merges by time, stays labelled, and the attention filter keeps what matters', () => {
  const {s, at} = rig(); s.bind({id: 'a'}); at(1000); s.mark('control', {state: 'ended'});
  s.importSnapshot([{kind: 'tool', category: 'shell', state: 'returned', count: 2, batchSteps: 0, at: new Date(1_000_500).toISOString(), lastAt: new Date(1_000_500).toISOString()},
    {kind: 'tool', category: 'files', state: 'error', count: 1, batchSteps: 0, at: new Date(1_002_000).toISOString(), lastAt: new Date(1_002_000).toISOString()}]);
  assert.deepEqual(s.entries().map(e => e.source + ':' + (e.category ?? e.kind)), ['snapshot:files', 'live:control', 'snapshot:shell', 'live:attached']);
  assert.deepEqual(s.entries({attention: true}).map(e => e.category ?? e.kind), ['files', 'control']);
});

test('labels are fixed text with sizes and times only', () => {
  const [label, detail] = describe({kind: 'output', open: true, bytes: 48_000, reads: 12, atMs: 0, lastAt: 12_000});
  assert.equal(label, 'Output · streaming'); assert.match(detail, /^47 KB in 12 reads over 12 s\./);
  assert.equal(ago(500), 'now'); assert.equal(ago(65_000), '1 min ago');
});
