import {test} from 'node:test';
import assert from 'node:assert/strict';
import {newAgentState, foldAgent, analyze, turnSummary, runSummary, toolLabel} from '../src/agent-analysis.js';

const T = s => new Date(Date.UTC(2026, 8, 19, 10, 0, s)).toISOString();
const start = (id, tool, s, sig = id) => ({kind: 'tool', phase: 'start', id, tool, category: 'shell', sig: sig.padEnd(12, '0').slice(0, 12).replace(/[^0-9a-f]/g, 'a'), at: T(s)});
const end = (id, s, error = false) => ({kind: 'tool', phase: 'end', id, error, at: T(s)});
test('a turn says how many calls ran, failed, were repeated, and how long it took', () => {
  const st = foldAgent(newAgentState(), [{kind: 'input', at: T(0)}, start('a', 'Bash', 1, 'aaa'), end('a', 3), start('b', 'Bash', 4, 'aaa'), end('b', 5, true), start('c', 'Read', 6, 'ccc'), end('c', 7),
    {kind: 'input', at: T(20)}, start('d', 'Edit', 21, 'ddd')]);
  const [now, first] = analyze(st, Date.parse(T(25)));
  assert.equal(turnSummary(first), '3 tool calls · 1 failed · 1 repeated · 7 s');
  assert.deepEqual(first.runs.map(runSummary), ['Read · 1 s', 'Bash ×2 · 1 failed · 1 repeated · 4 s']);
  assert.equal(turnSummary(now), '1 tool call · 1 running now · 1 s'); assert.equal(now.active, true);
  assert.deepEqual(first.longest, {tool: 'Bash', ms: 2000});
});
test('a repeat is the same tool with the same input in the same turn, not across turns', () => {
  const st = foldAgent(newAgentState(), [{kind: 'input', at: T(0)}, start('a', 'Bash', 1, 'abc'), end('a', 2), {kind: 'input', at: T(10)}, start('b', 'Bash', 11, 'abc'), end('b', 12)]);
  assert.deepEqual(analyze(st).map(t => t.repeated), [0, 0]);
});
test('folding is incremental, ignores duplicates and junk, and stays bounded', () => {
  const st = newAgentState();
  foldAgent(st, [start('a', 'Bash', 1)]); foldAgent(st, [start('a', 'Bash', 1), end('a', 2), end('a', 9, true), {kind: 'tool', phase: 'start', id: '<x>', tool: 'Bash', at: T(3)}, {kind: 'note', at: T(3), text: 'secret'}, null, {kind: 'input', at: 'never'}]);
  const [t] = analyze(st); assert.equal(t.calls, 1); assert.equal(t.failed, 0); assert.equal(JSON.stringify(st.items).includes('secret'), false);
  const many = []; for (let i = 0; i < 7000; i++) many.push(start('m' + i, 'Read', 5));
  foldAgent(st, many); assert.ok(st.items.length <= 6000 && st.calls.size <= 6000);
});
test('tool names read like words', () => {
  assert.equal(toolLabel('mcp__claude-in-chrome__javascript_tool'), 'chrome · javascript tool'); assert.equal(toolLabel('Bash'), 'Bash');
  assert.equal(runSummary({kind: 'compact', at: 0, before: 180000, after: 40000}), 'Context compacted · 180k → 40k tokens');
});
