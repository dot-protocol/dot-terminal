// Which UI build is this view running, and has a newer one been put in place? Compares the build
// compiled into the bundle with /version.json next to it. No session data is involved.
export function parseVersion(data) {
  if (data?.schema !== 'dot.ui-version.v1' || typeof data.build !== 'string' || !/^[\w.+-]{1,64}$/.test(data.build)) throw new Error('Unsupported version file');
  return { build: data.build, commit: /^[\w]{1,40}$/.test(data.commit ?? '') ? data.commit : '', builtAt: Number.isFinite(Date.parse(data.builtAt)) ? data.builtAt : '' };
}

/**
 * Polls for a different build. `onUpdate(next)` fires once per new build seen. A failed check is
 * reported through `onCheck(false)` and never treated as "no update".
 */
export function watchVersion({ current, load, onUpdate, onCheck = () => {}, everyMs = 15000, timers = globalThis, paused = () => false }) {
  let announced = current.build, stopped = false;
  async function check() {
    if (stopped) return null;
    try {
      const next = parseVersion(await load());
      onCheck(true);
      if (next.build !== announced) { announced = next.build; if (next.build !== current.build) onUpdate(next); }
      return next;
    } catch { onCheck(false); return null; }
  }
  const timer = timers.setInterval(() => { if (!paused()) check(); }, everyMs);
  return { check, stop() { stopped = true; timers.clearInterval(timer); } };
}

/** Reloading is safe for sessions (keepers outlive views) but not for typing in flight. */
export function safeToReload({ controlHeld, queuedBytes, dialogOpen }) { return !controlHeld && !queuedBytes && !dialogOpen; }
