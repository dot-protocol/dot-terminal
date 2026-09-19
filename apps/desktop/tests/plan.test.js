import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {validatePlan, orderPlan, summarizePlan, STATES} from '../src/plan.js';

const task = (id, state = 'open', extra = {}) => ({id, title: 'T ' + id, state, ...extra});
test('the shipped plan is valid and says plainly what is not built', () => {
  const plan = validatePlan(JSON.parse(readFileSync(new URL('../public/plan.json', import.meta.url))));
  assert.ok(plan.tasks.length >= 8); assert.ok(plan.tasks.every(t => STATES.includes(t.state)));
  assert.equal(plan.tasks.find(t => t.id === 'long-term').state, 'planned');
});
test('unknown states, duplicate ids, markup-ish ids and foreign fields are rejected or dropped', () => {
  const bad = tasks => assert.throws(() => validatePlan({schema: 'dot.plan.v1', tasks}));
  bad([task('a', 'shipped')]); bad([task('a'), task('a')]); bad([task('<b>')]); bad([{id: 'a', state: 'open'}]);
  assert.throws(() => validatePlan({schema: 'other', tasks: []}));
  const [t] = validatePlan({schema: 'dot.plan.v1', tasks: [task('a', 'done', {ref: 'javascript:1', html: '<i>', note: 'x'.repeat(900)})]}).tasks;
  assert.equal(t.ref, ''); assert.equal(t.html, undefined); assert.equal(t.note.length, 400);
});
test('moving work is on top, history at the bottom, file order kept within a state', () => {
  const ordered = orderPlan([task('d1', 'done'), task('o1'), task('p1', 'planned'), task('r1', 'running'), task('b1', 'blocked'), task('o2')]).map(t => t.id);
  assert.deepEqual(ordered, ['r1', 'b1', 'o1', 'o2', 'p1', 'd1']);
  assert.deepEqual(summarizePlan([task('a', 'done'), task('b', 'blocked'), task('c')]), {total: 3, done: 1, running: 0, blocked: 1});
});
