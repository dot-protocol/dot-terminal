import {test} from 'node:test';
import assert from 'node:assert/strict';
import {InputController, chunkUtf8, readDrop, CHUNK_BYTES, MIN_CHUNK_BYTES, MAX_QUEUED_BYTES} from '../src/input-controller.js';

const decode = parts => new TextDecoder('utf-8', {fatal: true}).decode(Buffer.concat(parts.map(p => Buffer.from(p))));
const busy = () => Object.assign(new Error('controller busy; input has not been accepted'), {code: 'controller-busy'});
function rig({fail = () => null, held = true} = {}) {
  const sent = [], states = []; let calls = 0, gate = null;
  const c = new InputController({
    canSend: () => held, onState: s => states.push(s), sleep: async () => {},
    send: async bytes => { const n = calls++; if (gate) await gate; const e = fail(n, bytes); if (e) throw e; sent.push(Uint8Array.from(bytes)); },
  });
  const idle = async () => { while (c.running || c.queue.length) await new Promise(r => setImmediate(r)); };
  return {c, sent, states, idle, hold: () => { let open; gate = new Promise(r => open = r); return () => { gate = null; open(); }; }};
}

test('chunks never split a character, and reassemble to exactly the input', () => {
  const text = 'a€😀é漢字'.repeat(700) + '\x1b[200~multi\nline\x1b[201~';
  const bytes = new TextEncoder().encode(text);
  for (const max of [4, 5, 7, 64, CHUNK_BYTES]) {
    const parts = chunkUtf8(bytes, max);
    assert.ok(parts.every(p => p.length <= max && p.length > 0));
    for (const p of parts) new TextDecoder('utf-8', {fatal: true}).decode(p); // each piece is valid on its own
    assert.equal(decode(parts), text);
  }
  assert.throws(() => chunkUtf8(bytes, 3));
});

test('typing goes out one key at a time when idle, in order, as the bytes the emulator produced', async () => {
  const {c, sent, idle} = rig();
  for (const key of ['l', 's', ' ', '\x1b[A', '\r']) { c.submit(key); await idle(); }
  assert.deepEqual(sent.map(b => decode([b])), ['l', 's', ' ', '\x1b[A', '\r']);
  assert.equal(c.snapshot().requests, 5);
});

test('a burst behind a slow request is coalesced in order, never reordered or duplicated', async () => {
  const {c, sent, idle, hold} = rig();
  const release = hold();
  c.submit('a'); for (const k of 'bcdefg') c.submit(k); // 'a' is in flight; the rest wait
  assert.equal(c.state().condition, 'queued');
  release(); await idle();
  assert.deepEqual(sent.map(b => decode([b])), ['a', 'bcdefg']);
  assert.equal(c.state().condition, 'idle');
});

test('a large multiline unicode paste arrives whole, in chunks under the keeper limit', async () => {
  const {c, sent, idle} = rig();
  const paste = '\x1b[200~' + 'line one 😀\nline two 漢字\n'.repeat(3000) + '\x1b[201~';
  assert.equal(c.submit(paste), true); await idle();
  assert.ok(sent.length > 1 && sent.every(b => b.length <= CHUNK_BYTES));
  assert.equal(decode(sent), paste, 'byte order and bracketed-paste delimiters survive chunking');
});

test('over the bound the whole submission is refused and reported; nothing is truncated', async () => {
  const {c, sent, states, idle} = rig();
  assert.equal(c.submit('x'.repeat(MAX_QUEUED_BYTES + 1)), false); await idle();
  assert.equal(sent.length, 0);
  assert.equal(states.at(-1).refusal, 'too-large');
  assert.equal(c.submit('ok'), true); await idle();
  assert.equal(decode(sent), 'ok');
});

test('a view without control sends nothing and says so', async () => {
  const {c, sent, states, idle} = rig({held: false});
  assert.equal(c.submit('rm -rf /\r'), false); await idle();
  assert.equal(sent.length, 0);
  assert.equal(states.at(-1).refusal, 'view-only');
});

test('busy means not accepted: the SAME bytes are retried, once, in place', async () => {
  const {c, sent, idle} = rig({fail: n => (n < 2 ? busy() : null)});
  c.submit('pay'); c.submit(' now'); await idle();
  assert.deepEqual(sent.map(b => decode([b])), ['pay', ' now']);
  assert.equal(c.snapshot().busyRetries, 2);
});

test('an unknown outcome is never retried, and what waited behind it is discarded', async () => {
  const {c, sent, states, idle, hold} = rig({fail: n => (n === 0 ? new Error('timeout') : null)});
  const release = hold();
  c.submit('pay\r'); c.submit('again\r'); release(); await idle();
  assert.deepEqual(sent, [], 'neither the ambiguous bytes nor the queued ones were sent again');
  assert.deepEqual([states.at(-1).condition, states.at(-1).refusal], ['uncertain', 'unknown-outcome']);
  assert.equal(c.snapshot().discardedBytes, 6);
  c.reset(); c.submit('fresh'); await idle();
  assert.equal(decode(sent), 'fresh', 'a deliberate recovery starts clean');
});

test('fencing and persistent busy are told apart from an unknown outcome', async () => {
  const fenced = rig({fail: () => Object.assign(new Error('stale controller generation'), {code: 'controller-fenced'})});
  fenced.c.submit('x'); await fenced.idle();
  assert.equal(fenced.states.at(-1).refusal, 'fenced');
  const stuck = rig({fail: () => busy()});
  stuck.c.submit('x'); await stuck.idle();
  assert.equal(stuck.states.at(-1).refusal, 'busy');
  assert.equal(stuck.sent.length, 0);
});

test('switching view while a request is in flight drops the queue and lets nothing through afterwards', async () => {
  const {c, sent, idle, hold} = rig();
  const release = hold();
  c.submit('old-1'); c.submit('old-2');
  c.reset('view-changed'); release(); await idle();
  assert.deepEqual(sent.map(b => decode([b])), ['old-1'], 'only the request already on the wire completed');
  c.submit('new'); await idle();
  assert.equal(decode([sent.at(-1)]), 'new');
});

test('measurements are counts and timings only', async () => {
  let now = 0; const sent = [];
  const c = new InputController({canSend: () => true, clock: () => now, send: async b => { now += 2; sent.push(b); }});
  for (let i = 0; i < 5; i++) { now += 83; c.submit(' '); c.sawRepeat(); await new Promise(r => setImmediate(r)); }
  const s = c.snapshot();
  assert.equal(s.keyRepeatsSeen, 5); assert.equal(s.submissions, 5); assert.equal(s.arrivalGapMs.p50, 85);
  assert.equal(s.requestRttMs.p50, 2);
  assert.ok(!JSON.stringify(s).includes(' "'), 'no input text in the snapshot');
  assert.deepEqual(Object.keys(s).filter(k => /text|data|content|clip/i.test(k)), []);
});

test('a drop is a link or text, never html, never a file path, never a trailing newline', () => {
  const transfer = (data, files = []) => ({types: [...Object.keys(data), ...(files.length ? ['Files'] : [])], getData: t => data[t] ?? '', files});
  assert.deepEqual(readDrop(transfer({'text/uri-list': '# comment\r\nhttps://example.com/a?b=1&c=$(rm)\r\n', 'text/html': '<a href="x">x</a>', 'text/plain': 'ignored'})),
    {kind: 'text', text: 'https://example.com/a?b=1&c=$(rm)'}, 'the literal link, not reinterpreted');
  assert.deepEqual(readDrop(transfer({'text/uri-list': 'https://a.example\nhttps://b.example'})), {kind: 'text', text: 'https://a.example https://b.example'});
  assert.deepEqual(readDrop(transfer({'text/plain': 'echo hi\n\n'})), {kind: 'text', text: 'echo hi'});
  assert.deepEqual(readDrop(transfer({'text/html': '<b>only html</b>'})), {kind: 'none'});
  assert.deepEqual(readDrop(transfer({}, [{name: 'secret.pdf'}])), {kind: 'files', count: 1});
  assert.deepEqual(readDrop(transfer({'text/uri-list': 'https://dragged.example'}, [{name: 'x.webloc'}])), {kind: 'text', text: 'https://dragged.example'});
  assert.deepEqual(readDrop(null), {kind: 'none'});
});

test('keys typed right after a reset, while the old request is still out, are not left stranded', async () => {
  const {c, sent, idle, hold} = rig();
  const release = hold();
  c.submit('old'); c.reset('view-changed'); c.submit('new'); // typed before the old request returns
  release(); await idle();
  assert.deepEqual(sent.map(b => decode([b])), ['old', 'new']);
  assert.equal(c.state().condition, 'idle');
});

test('a slow consumer shrinks the next request and a fast one grows it back; bytes stay whole', async () => {
  const sent = []; let now = 0, cost = 900;
  const c = new InputController({ canSend: () => true, clock: () => now, sleep: async () => {},
    send: async bytes => { now += cost; sent.push(Uint8Array.from(bytes)); } });
  const idle = async () => { while (c.running || c.queue.length) await new Promise(r => setImmediate(r)); };
  const paste = '漢字 😀 line\n'.repeat(2000);
  c.submit(paste); await idle();
  const sizes = sent.map(b => b.length);
  assert.ok(sizes[0] > CHUNK_BYTES - 4 && sizes[0] <= CHUNK_BYTES, 'starts at the full request size');
  assert.ok(sizes[1] <= CHUNK_BYTES / 2 && sizes[2] <= CHUNK_BYTES / 4, 'each slow round trip halves the next request');
  assert.equal(c.snapshot().chunkBytes, MIN_CHUNK_BYTES, 'the request budget stops at the floor');
  assert.ok(sizes.every(n => n <= CHUNK_BYTES));
  assert.equal(decode(sent), paste);
  cost = 5; const from = sent.length;
  c.submit(paste); await idle();
  assert.ok(sent.slice(from).some(b => b.length > CHUNK_BYTES / 2), 'fast round trips grow it back');
  assert.equal(c.snapshot().chunkBytes, CHUNK_BYTES);
});
