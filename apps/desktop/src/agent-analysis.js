// Turns an agent's metadata events into what a person wants to know: per request ("turn"), how many
// tool calls ran, which failed, which were the same call made again, how long it took, and what is
// still running. No DOM, no network. Input is already reduced by the backend: tool names, times,
// failure flags and input fingerprints. There is no command text or output in here to leak.
const CATEGORIES = ['shell', 'files', 'browser', 'tasks', 'agent', 'oracle', 'other'];
const ms = at => { const t = Date.parse(at); return Number.isFinite(t) ? t : null; };
const clean = (v, max, pattern) => (typeof v === 'string' && v.length <= max && pattern.test(v) ? v : '');

export function newAgentState() { return { calls: new Map(), items: [], seen: 0 }; }

/** Fold a batch of backend events into the state. Unknown shapes are skipped, never stored. */
export function foldAgent(state, events) {
  for (const e of Array.isArray(events) ? events : []) {
    const at = ms(e?.at); if (at === null) continue;
    if (e.kind === 'input') state.items.push({ kind: 'input', at });
    else if (e.kind === 'compact') state.items.push({ kind: 'compact', at, before: Number.isFinite(e.before) ? e.before : null, after: Number.isFinite(e.after) ? e.after : null });
    else if (e.kind === 'tool' && e.phase === 'start') {
      const id = clean(e.id, 80, /^[\w-]+$/), tool = clean(e.tool, 48, /^[\w.:-]+$/); if (!id || !tool || state.calls.has(id)) continue;
      const call = { kind: 'call', id, at, tool, category: CATEGORIES.includes(e.category) ? e.category : 'other', sig: clean(e.sig, 12, /^[0-9a-f]+$/), endAt: null, error: false };
      state.calls.set(id, call); state.items.push(call);
    } else if (e.kind === 'tool' && e.phase === 'end') {
      const call = state.calls.get(e.id); if (call && call.endAt === null) { call.endAt = at; call.error = e.error === true; }
    }
    state.seen++;
  }
  if (state.items.length > 6000) { for (const old of state.items.splice(0, state.items.length - 5000)) if (old.kind === 'call') state.calls.delete(old.id); }
  return state;
}

/** Short tool names for people: mcp__claude-in-chrome__computer → chrome · computer */
export function toolLabel(tool) {
  const m = /^mcp__(.+?)__(.+)$/.exec(tool); if (!m) return tool;
  return m[1].replace(/^claude[-_]in[-_]/, '').replace(/^claude_ai_/, '').replace(/[_-]+/g, ' ') + ' · ' + m[2].replace(/_/g, ' ');
}

/**
 * Newest first. A turn is everything between two of the person's messages. Inside a turn,
 * consecutive calls to the same tool are one run. "Repeated" = the same tool with the same input
 * was already made earlier in this turn: usually a retry or a loop worth looking at.
 */
export function analyze(state, now = Date.now()) {
  const turns = []; let turn = null;
  const open = at => { turn = { kind: 'turn', at, endAt: at, calls: 0, failed: 0, repeated: 0, running: 0, runs: [], sigs: new Set(), compactions: 0, longest: null }; turns.push(turn); };
  for (const item of state.items) {
    if (item.kind === 'input' || !turn) open(item.at);
    if (item.kind === 'input') continue;
    turn.endAt = Math.max(turn.endAt, item.endAt ?? item.at);
    if (item.kind === 'compact') { turn.compactions++; turn.runs.push({ kind: 'compact', at: item.at, before: item.before, after: item.after }); continue; }
    const repeated = !!item.sig && turn.sigs.has(item.sig); if (item.sig) turn.sigs.add(item.sig);
    const running = item.endAt === null, took = running ? null : item.endAt - item.at;
    turn.calls++; if (item.error) turn.failed++; if (repeated) turn.repeated++; if (running) turn.running++;
    if (took !== null && (!turn.longest || took > turn.longest.ms)) turn.longest = { tool: item.tool, ms: took };
    const last = turn.runs.at(-1);
    if (last?.kind === 'run' && last.tool === item.tool) { last.count++; last.endAt = item.endAt ?? item.at; if (item.error) last.failed++; if (repeated) last.repeated++; if (running) last.running++; }
    else turn.runs.push({ kind: 'run', at: item.at, endAt: item.endAt ?? item.at, tool: item.tool, category: item.category, count: 1, failed: item.error ? 1 : 0, repeated: repeated ? 1 : 0, running: running ? 1 : 0 });
  }
  for (const t of turns) { delete t.sigs; t.runs.reverse(); t.active = t === turns.at(-1) && (t.running > 0 || now - t.endAt < 60000); }
  return turns.reverse();
}

const plural = (n, word) => n + ' ' + word + (n === 1 ? '' : 's');
export const took = span => { const s = Math.max(0, Math.round(span / 1000)); return s < 60 ? s + ' s' : s < 3600 ? Math.floor(s / 60) + ' min ' + (s % 60 ? (s % 60) + ' s' : '').trim() : Math.floor(s / 3600) + ' h ' + Math.floor(s % 3600 / 60) + ' min'; };

/** One sentence for a turn: what a person reads first. */
export function turnSummary(t) {
  if (!t.calls) return t.compactions ? 'Context was compacted' : 'No tool calls';
  const parts = [plural(t.calls, 'tool call')];
  if (t.failed) parts.push(t.failed + ' failed'); if (t.repeated) parts.push(t.repeated + ' repeated'); if (t.running) parts.push(t.running + ' running now');
  parts.push(took(t.endAt - t.at).trim());
  return parts.join(' · ');
}
export function runSummary(r) {
  if (r.kind === 'compact') return 'Context compacted' + (r.before && r.after ? ' · ' + Math.round(r.before / 1000) + 'k → ' + Math.round(r.after / 1000) + 'k tokens' : '');
  const parts = [toolLabel(r.tool) + (r.count > 1 ? ' ×' + r.count : '')];
  if (r.failed) parts.push(r.failed === r.count ? 'failed' : r.failed + ' failed'); if (r.repeated) parts.push(r.repeated === r.count && r.count === 1 ? 'same call again' : r.repeated + ' repeated');
  if (r.running) parts.push('running'); else if (r.endAt - r.at >= 1000) parts.push(took(r.endAt - r.at).trim());
  return parts.join(' · ');
}
