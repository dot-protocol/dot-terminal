import {test} from 'node:test';
import assert from 'node:assert/strict';
import {framePlan, framesUnsupported} from '../src/render-flow.js';

const frame = (o = {}) => ({incarnation: 'a', cols: 100, rows: 30, ...o});
test('a follower parses bytes on the grid they were produced on', () => {
  assert.deepEqual(framePlan({frame: frame(), knownIncarnation: 'a', controller: false, cols: 80, rows: 24}), {restart: false, resize: {cols: 100, rows: 30}});
  assert.deepEqual(framePlan({frame: frame(), knownIncarnation: 'a', controller: false, cols: 100, rows: 30}), {restart: false, resize: null});
});
test('the controller is never bounced back by old-grid bytes still in flight', () => {
  assert.equal(framePlan({frame: frame({cols: 80, rows: 24}), knownIncarnation: 'a', controller: true, cols: 120, rows: 40}).resize, null);
});
test('a different incarnation restarts the stream; the first frame of a view does not', () => {
  assert.deepEqual(framePlan({frame: frame({incarnation: 'b'}), knownIncarnation: 'a', controller: true, cols: 1, rows: 1}), {restart: true, resize: null});
  assert.equal(framePlan({frame: frame(), knownIncarnation: '', controller: false, cols: 100, rows: 30}).restart, false);
});
test('a nonsense grid is ignored rather than applied', () => {
  assert.equal(framePlan({frame: frame({cols: 0, rows: 0}), knownIncarnation: 'a', controller: false, cols: 80, rows: 24}).resize, null);
});
test('fallback happens only for "does not know read_frame", never for an ordinary failure', () => {
  assert.equal(framesUnsupported(Object.assign(new Error('unknown variant `read_frame`, expected one of `status`, `read`'), {code: 'keeper-error'})), true);
  assert.equal(framesUnsupported(Object.assign(new Error('Failed to deserialize'), {status: 422})), true);
  assert.equal(framesUnsupported(Object.assign(new Error('session exited'), {code: 'keeper-error'})), false);
  assert.equal(framesUnsupported(new Error('The operation timed out')), false);
  assert.equal(framesUnsupported(Object.assign(new Error('stale controller generation'), {code: 'controller-fenced'})), false);
});
