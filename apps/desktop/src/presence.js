// Who is here and who is typing, and what a tap on the terminal should do about it. No DOM, no
// network. Presence is self-declared by paired views and is for people; the keeper's controller
// generations remain the only thing that decides whose bytes are accepted.

export const TYPING_GRACE_MS = 3000; // a holder who typed this recently is not interrupted by one tap
export const CONFIRM_WINDOW_MS = 5000; // a second tap within this takes over anyway

/** A short, honest name for this view. The keeper validates it again. */
export function describeView(userAgent = '', viewId = '') {
  const phone = /Android|iPhone|iPad/.test(userAgent);
  const embedded = /AppleWebKit/.test(userAgent) && !/Safari\//.test(userAgent) && /Macintosh/.test(userAgent);
  const browser = /Firefox\//.test(userAgent) ? 'Firefox' : /Edg\//.test(userAgent) ? 'Edge' : /Chrome\//.test(userAgent) ? 'Chrome' : /Safari\//.test(userAgent) ? 'Safari' : 'Browser';
  const kind = phone ? 'phone' : embedded ? 'app' : 'browser';
  const label = phone ? (/Android/.test(userAgent) ? 'Android phone' : 'iPhone / iPad') : embedded ? 'Mac app' : browser;
  return { view: viewId, kind, label };
}

export function newViewId(random = () => crypto.randomUUID()) { return 'view-' + random().replace(/[^a-zA-Z0-9-]/g, '').slice(0, 36); }

/**
 * What one tap (or the first key) in the terminal should do.
 *  'hold'     this view already has control
 *  'take'     nobody has it: just take it
 *  'takeover' someone has it but has gone quiet, or this is the confirming second tap
 *  'confirm'  someone typed moments ago: say who, and wait for a second tap
 */
export function controlIntent({ presence, self, held, lastTapAgoMs = Infinity }) {
  if (held) return 'hold';
  if (!presence) return 'take'; // an older keeper: try politely; a refusal is shown with a take-over option
  if (!presence.controller_known) return 'take';
  if (presence.controller === self) return 'takeover'; // a stale lease of our own (reload): reclaim it
  const idle = presence.controller_idle_ms ?? 0;
  if (idle >= TYPING_GRACE_MS || lastTapAgoMs <= CONFIRM_WINDOW_MS) return 'takeover';
  return 'confirm';
}

/** The people line: every live view, the typist marked, this view marked. */
export function presenceChips(presence, self) {
  if (!presence) return [];
  const chips = presence.views.map(v => ({ view: v.view, label: v.label, kind: v.kind, you: v.view === self, typing: presence.controller === v.view }));
  if (presence.controller_known && !chips.some(c => c.typing)) chips.push({ view: '', label: 'Another view', kind: 'cli', you: false, typing: true });
  return chips.sort((a, b) => Number(b.typing) - Number(a.typing) || Number(b.you) - Number(a.you) || a.label.localeCompare(b.label));
}

export function holderName(presence, self) {
  const holder = presenceChips(presence, self).find(c => c.typing);
  return holder ? (holder.you ? 'this view' : holder.label) : '';
}
