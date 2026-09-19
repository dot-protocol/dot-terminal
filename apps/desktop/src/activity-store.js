// What this view has observed about ONE selected PTY, newest first. No DOM, no network.
//
// Live entries come only from things the view already does with the keeper: bytes it read,
// grid changes it applied, control it gained or lost, history it missed. They are counts, sizes
// and times — never terminal text, never input. An imported agent snapshot can sit in the same
// list, but it stays labelled as a snapshot: nothing binds that file to this PTY.

export const QUIET_MS = 1500;   // output pauses longer than this end a burst
export const LIVE_LIMIT = 400;  // bounded retention; the oldest live entries fall off
const CONTROL = ['taken', 'ended'], STOPPED = ['fenced', 'busy', 'unknown-outcome'];
const whole = (n, max) => (Number.isSafeInteger(n) && n >= 0 ? Math.min(max, n) : 0);

export class ActivityStore {
  constructor({ clock = () => Date.now(), limit = LIVE_LIMIT, onChange = () => {} } = {}) {
    Object.assign(this, { clock, limit, onChange });
    this.session = null; this.live = []; this.snapshot = []; this.version = 0; this.serial = 0;
  }

  /** A different PTY (or none). What was observed about the last one does not carry over. */
  bind(session) {
    this.session = session ? { id: String(session.id), kind: session.kind === 'iterm' ? 'iterm' : 'dot' } : null;
    this.live = [];
    if (this.session) this.push({ kind: 'attached' }); else this.changed();
  }

  /** Bytes just read from the bound PTY. Extends the open burst or starts a new one. */
  output(bytes) {
    const n = whole(bytes, Number.MAX_SAFE_INTEGER); if (!n || !this.session) return;
    const now = this.clock(), top = this.live[0];
    if (top?.kind === 'output' && top.open) { top.lastAt = now; top.bytes += n; top.reads++; this.changed(); }
    else this.push({ kind: 'output', lastAt: now, bytes: n, reads: 1, open: true });
  }

  /** A discrete observation. Anything outside the known shapes is dropped, not stored. */
  mark(kind, detail = {}) {
    if (!this.session) return;
    if (kind === 'resize') {
      const cols = whole(detail.cols, 1000), rows = whole(detail.rows, 1000);
      if (cols && rows) this.push({ kind, cols, rows });
    } else if (kind === 'control' && CONTROL.includes(detail.state)) this.push({ kind, state: detail.state });
    else if (kind === 'input-stopped' && STOPPED.includes(detail.reason)) this.push({ kind, reason: detail.reason });
    else if (kind === 'gap') this.push({ kind });
    else if (kind === 'exited' && !this.live.some(e => e.kind === 'exited')) this.push({ kind });
  }

  /** Once a second: a burst that has gone quiet is closed. Returns true when something changed. */
  tick() {
    const top = this.live.find(e => e.kind === 'output' && e.open);
    if (!top || this.clock() - top.lastAt < QUIET_MS) return false;
    top.open = false; this.changed(); return true;
  }

  /** Already-validated, already-grouped snapshot entries (see trajectory.js). */
  importSnapshot(groups) { this.snapshot = groups.map(g => ({ ...g, source: 'snapshot', atMs: Date.parse(g.lastAt ?? g.at), key: 's' + g.kind + g.at + (g.category ?? '') })); this.changed(); }

  push(entry) {
    const closing = this.live.find(e => e.kind === 'output' && e.open);
    if (closing && entry.kind !== 'output') closing.open = false; // an event is a boundary
    this.live.unshift({ ...entry, source: 'live', atMs: this.clock(), key: 'l' + (++this.serial) });
    if (this.live.length > this.limit) this.live.length = this.limit;
    this.changed();
  }

  changed() { this.version++; this.onChange(); }

  /** Newest first. `attention` keeps only what a person should look at. */
  entries({ attention = false } = {}) {
    const all = [...this.live, ...this.snapshot].sort((a, b) => b.atMs - a.atMs);
    return attention ? all.filter(e => ['gap', 'input-stopped', 'exited', 'compact'].includes(e.kind) || e.state === 'error' || e.state === 'ended') : all;
  }

  /** The one-line answer to "is it doing anything right now?" */
  summary() {
    if (!this.session) return { state: 'unbound' };
    const last = this.live.find(e => e.kind === 'output');
    if (!last) return { state: 'waiting' };
    return last.open ? { state: 'streaming', forMs: this.clock() - last.atMs, bytes: last.bytes } : { state: 'quiet', forMs: this.clock() - last.lastAt };
  }
}
