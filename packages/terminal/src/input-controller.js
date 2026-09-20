// The ONE ordered, fenced path from a view to a PTY. Keyboard, IME composition, paste, drops and
// accessory keys all arrive here as text the terminal emulator has already encoded (xterm's onData);
// nothing else may write input. This file has no DOM and no network: the caller supplies `send`.
//
// Promises it keeps:
//   - bytes leave in the order they arrived, one request in flight at a time;
//   - a chunk never splits a UTF-8 character, so a slow link cannot corrupt text;
//   - "not accepted" (keeper busy) retries the SAME bytes; an ambiguous failure is never retried —
//     what was queued behind it is discarded and the caller is told, because replaying input into a
//     shell after an unknown outcome can run a command twice;
//   - nothing is dropped silently: over the bound, the whole submission is refused and reported;
//   - measurements are counts and timings only. No text, no bytes, no clipboard content is kept.

// A shell's line editor can take a paste at only a few KB/s, and the keeper's write blocks until
// the PTY accepts it. Measured: 8 KiB into zsh took 1.1 s p50 / 1.8 s p95, too close to the 3 s
// socket timeout — and crossing it is an ambiguous outcome that (rightly) stops input. So a request
// starts small and its size follows the measured round trip.
export const CHUNK_BYTES = 2 * 1024; // the largest request; far under the keeper's 16 KiB limit
export const MIN_CHUNK_BYTES = 256;
const SLOW_RTT_MS = 600, FAST_RTT_MS = 120;
export const MAX_QUEUED_BYTES = 1024 * 1024;
const BUSY_RETRIES = 6, BUSY_BACKOFF_MS = 15, SAMPLES = 120;

/** Split `bytes` into pieces of at most `max`, never inside a UTF-8 character. */
export function chunkUtf8(bytes, max = CHUNK_BYTES) {
  if (max < 4) throw new Error('chunk size must hold one UTF-8 character');
  const out = [];
  for (let at = 0; at < bytes.length;) {
    let end = Math.min(bytes.length, at + max);
    // Back up over continuation bytes (10xxxxxx) so the cut lands on a character boundary.
    if (end < bytes.length) while (end > at && (bytes[end] & 0xc0) === 0x80) end--;
    if (end === at) end = Math.min(bytes.length, at + max); // malformed run: make progress anyway
    out.push(bytes.subarray(at, end));
    at = end;
  }
  return out;
}

/**
 * What a drop means for a terminal. Links and text are inserted like a paste; files are refused
 * until there is an explicit transfer contract — a browser cannot give a portable path, and a
 * remote shell could not read it anyway. HTML is ignored: only the literal link or text counts.
 */
export function readDrop(transfer) {
  const types = Array.from(transfer?.types ?? []);
  const get = type => (types.includes(type) ? transfer.getData(type) : '');
  const uris = get('text/uri-list').split(/\r?\n/).map(l => l.trim()).filter(l => l && !l.startsWith('#'));
  // A dragged link carries both a uri-list and Files in some browsers; the link wins.
  if (uris.length) return { kind: 'text', text: uris.join(' ') };
  // Never end a drop with a newline: in a shell without bracketed paste that would run it.
  const plain = get('text/plain').replace(/[\r\n]+$/, '');
  if (plain) return { kind: 'text', text: plain };
  if (types.includes('Files') || (transfer?.files?.length ?? 0) > 0) return { kind: 'files', count: transfer.files?.length ?? 0 };
  return { kind: 'none' };
}

const percentile = (values, q) => {
  if (!values.length) return null;
  const a = [...values].sort((x, y) => x - y);
  return a[Math.ceil(a.length * q) - 1];
};

export class InputController {
  /**
   * @param send     async (Uint8Array) => void. Reject with `{code:'controller-busy'}` when the
   *                 keeper did not accept the bytes; any other rejection is an unknown outcome.
   * @param canSend  () => boolean: does this view hold input control right now?
   * @param onState  (state) => void, called whenever `state()` changes meaningfully.
   */
  constructor({ send, canSend, onState = () => {}, clock = () => performance.now(), sleep = ms => new Promise(r => setTimeout(r, ms)) }) {
    Object.assign(this, { send, canSend, onState, clock, sleep });
    this.encoder = new TextEncoder();
    this.queue = []; this.queued = 0; this.running = false; this.epoch = 0;
    this.condition = 'idle'; this.refusal = null; this.chunk = CHUNK_BYTES;
    this.resetMeasurements();
  }

  resetMeasurements() {
    this.m = { submissions: 0, requests: 0, bytes: 0, busyRetries: 0, refused: 0, uncertain: 0, discardedBytes: 0,
      repeats: 0, highWater: 0, lastAt: null, gaps: [], rtts: [], perRequest: [] };
  }

  /** A view switch, a lost lease, a takeover: whatever was waiting belongs to the past. */
  reset(reason = 'reset') {
    this.epoch++;
    this.m.discardedBytes += this.queued;
    this.queue = []; this.queued = 0;
    this.set('idle', reason === 'reset' ? null : reason);
  }

  /** A held key, seen by a passive listener. Counted, never written. */
  sawRepeat() { this.m.repeats++; }

  /** Text from the terminal emulator. Returns false when it was refused (and says why via state). */
  submit(text) {
    if (!text) return true;
    if (!this.canSend()) { this.m.refused++; this.set(this.condition, 'view-only'); return false; }
    const bytes = this.encoder.encode(text);
    if (this.queued + bytes.length > MAX_QUEUED_BYTES) {
      this.m.refused++;
      this.set(this.condition, 'too-large');
      return false;
    }
    const now = this.clock();
    if (this.m.lastAt !== null) { this.m.gaps.push(Math.round(now - this.m.lastAt)); if (this.m.gaps.length > SAMPLES) this.m.gaps.shift(); }
    this.m.lastAt = now; this.m.submissions++;
    this.queue.push(bytes); this.queued += bytes.length;
    this.m.highWater = Math.max(this.m.highWater, this.queued);
    // Typed while a request is still out: say so, rather than looking idle-but-unresponsive.
    if (this.running) this.set('queued', null); else this.refusal = null;
    this.drain();
    return true;
  }

  async drain() {
    if (this.running) return;
    this.running = true;
    const epoch = this.epoch;
    try {
      while (this.queue.length && epoch === this.epoch) {
        // Everything that piled up while the last request was in flight goes out together, in
        // order. One key at a time when idle; one request for a burst or a paste chunk.
        let size = 0, take = 0;
        while (take < this.queue.length && size + this.queue[take].length <= this.chunk) size += this.queue[take++].length;
        let batch;
        if (take === 0) { // a single submission larger than a chunk: cut it on character boundaries
          const [first, ...rest] = chunkUtf8(this.queue[0], this.chunk);
          this.queue.splice(0, 1, first, ...rest); batch = first;
        } else {
          batch = new Uint8Array(size);
          let at = 0; for (const part of this.queue.slice(0, take)) { batch.set(part, at); at += part.length; }
          this.queue.splice(0, take, batch);
        }
        this.set(this.queued > batch.length ? 'queued' : 'sending', null);
        const outcome = await this.deliver(batch, epoch);
        if (epoch !== this.epoch) return; // the view moved on while we waited; reset() already accounted
        if (outcome !== 'sent') {
          this.m.uncertain++;
          // The batch itself has an unknown outcome — it may have reached the shell. Only what
          // waited behind it is known to be unsent, and that is what gets discarded.
          this.m.discardedBytes += this.queued - batch.length;
          this.queue = []; this.queued = 0; this.epoch++;
          this.set('uncertain', outcome);
          return;
        }
        this.queue.shift(); this.queued -= batch.length;
      }
      if (epoch === this.epoch) this.set('idle', null);
    } finally {
      this.running = false;
      // A reset can arrive, and new keys after it, while the old request is still out. Whoever
      // finishes must not leave them waiting for the next keypress.
      if (this.queue.length) this.drain();
    }
  }

  async deliver(batch, epoch) {
    for (let attempt = 0; ; attempt++) {
      const start = this.clock();
      try {
        await this.send(batch);
        const rtt = this.clock() - start;
        this.m.requests++; this.m.bytes += batch.length;
        for (const [list, value] of [[this.m.rtts, rtt], [this.m.perRequest, batch.length]]) { list.push(Math.round(value * 10) / 10); if (list.length > SAMPLES) list.shift(); }
        // Slow consumer: halve the next request. Fast again: grow back. Never below one screenful of typing.
        if (rtt > SLOW_RTT_MS) this.chunk = Math.max(MIN_CHUNK_BYTES, this.chunk >> 1);
        else if (rtt < FAST_RTT_MS) this.chunk = Math.min(CHUNK_BYTES, this.chunk << 1);
        return 'sent';
      } catch (error) {
        // Busy means the keeper refused before touching the PTY: the same bytes are still unsent.
        if (error?.code !== 'controller-busy' || attempt >= BUSY_RETRIES || epoch !== this.epoch) return error?.code === 'controller-fenced' ? 'fenced' : error?.code === 'controller-busy' ? 'busy' : 'unknown-outcome';
        this.m.busyRetries++;
        await this.sleep(BUSY_BACKOFF_MS * (attempt + 1));
      }
    }
  }

  set(condition, refusal) {
    const changed = condition !== this.condition || refusal !== this.refusal;
    this.condition = condition; this.refusal = refusal;
    if (changed) this.onState(this.state());
  }

  /** What a person needs to see about their typing right now. */
  state() { return { condition: this.condition, refusal: this.refusal, queuedBytes: this.queued }; }

  /** Content-free measurements for the System panel and for a held-key diagnosis. */
  snapshot() {
    const m = this.m;
    return { schema: 'dot.input-view.v1', coverage: 'local-view', ...this.state(),
      submissions: m.submissions, requests: m.requests, bytesSent: m.bytes, keyRepeatsSeen: m.repeats,
      busyRetries: m.busyRetries, refused: m.refused, unknownOutcomes: m.uncertain, discardedBytes: m.discardedBytes,
      queueHighWaterBytes: m.highWater, chunkBytes: this.chunk,
      arrivalGapMs: { count: m.gaps.length, p50: percentile(m.gaps, .5), p95: percentile(m.gaps, .95) },
      requestRttMs: { count: m.rtts.length, p50: percentile(m.rtts, .5), p95: percentile(m.rtts, .95) },
      bytesPerRequest: { count: m.perRequest.length, p50: percentile(m.perRequest, .5), p95: percentile(m.perRequest, .95) } };
  }
}
