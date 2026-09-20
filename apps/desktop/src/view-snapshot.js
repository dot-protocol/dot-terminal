// What this view is showing, as compact data: for whoever maintains the interface to SEE the real
// app (Mac window, browser, phone) without pixels or OS permissions. Layout boxes, every control
// with its role/name/state, terminal viewport numbers, and a timeline of the last start.
// It never includes terminal text, typed input, clipboard or capabilities.
const box = el => { if (!el) return null; const r = el.getBoundingClientRect(); return r.width || r.height ? [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)] : null; };
const visible = el => { const r = el.getBoundingClientRect(); if (!r.width || !r.height) return false; const s = getComputedStyle(el); return s.visibility !== 'hidden' && s.display !== 'none'; };
const clip = (v, n) => String(v ?? '').replace(/\s+/g, ' ').trim().slice(0, n);

/** The accessible name a screen reader would announce, in the order browsers resolve it. */
export function accessibleName(el) {
  const labelled = el.getAttribute('aria-labelledby'); if (labelled) { const t = labelled.split(/\s+/).map(id => el.ownerDocument.getElementById(id)?.textContent ?? '').join(' '); if (t.trim()) return clip(t, 80); }
  const aria = el.getAttribute('aria-label'); if (aria?.trim()) return clip(aria, 80);
  if (el.labels?.length) { const t = [...el.labels].map(l => l.textContent).join(' '); if (t.trim()) return clip(t, 80); }
  const text = clip(el.textContent, 80); if (text) return text;
  return clip(el.getAttribute('title') || el.getAttribute('placeholder') || '', 80);
}

/** Every operable control that is on screen. A control without a name is an accessibility bug. */
export function controlList(root) {
  const out = [];
  for (const el of root.querySelectorAll('button,[role=tab],[role=separator],input,select,textarea,summary,a[href]')) {
    if (!visible(el) || el.closest('.xterm')) continue;
    const role = el.getAttribute('role') || ({ BUTTON: 'button', INPUT: el.type === 'checkbox' ? 'checkbox' : 'textbox', SELECT: 'combobox', TEXTAREA: 'textbox', SUMMARY: 'disclosure', A: 'link' })[el.tagName] || 'generic';
    const name = accessibleName(el), c = { role, name, box: box(el) };
    if (el.id) c.id = el.id; if (el.disabled) c.disabled = true; if (!name) c.unnamed = true;
    for (const [attr, key] of [['aria-expanded', 'expanded'], ['aria-selected', 'selected'], ['aria-checked', 'checked']]) if (el.hasAttribute(attr)) c[key] = el.getAttribute(attr) === 'true';
    if (el.type === 'checkbox') c.checked = el.checked; if (el.tagName === 'SUMMARY') c.expanded = el.parentElement.open; if (el.classList.contains('selected')) c.selected = true;
    out.push(c); if (out.length >= 120) break;
  }
  return out;
}

export class Timeline {
  constructor(clock = () => performance.now(), limit = 60) { Object.assign(this, { clock, limit }); this.events = []; this.zero = clock(); }
  mark(name, detail) { const e = { t: Math.round(this.clock() - this.zero), name: clip(name, 40) }; if (detail !== undefined) e.detail = typeof detail === 'number' ? detail : clip(detail, 60); this.events.push(e); if (this.events.length > this.limit) this.events.shift(); return e; }
}

export function buildSnapshot({ doc, win, term, facts, timeline }) {
  const $ = s => doc.querySelector(s), controls = controlList(doc), b = term?.buffer?.active;
  return {
    schema: 'dot.view-snapshot.v1', at: new Date().toISOString(), ...facts,
    window: { inner: [win.innerWidth, win.innerHeight], dpr: win.devicePixelRatio, position: [win.screenX, win.screenY], screen: [win.screen.width, win.screen.height], screenOrigin: [win.screen.availLeft ?? 0, win.screen.availTop ?? 0], visible: !doc.hidden, focused: doc.hasFocus() },
    layout: { sidebar: box($('aside')), header: box($('header')), infobar: box($('.infobar')), terminal: box($('#terminal')), activity: box($('#trajectory')), footer: box($('footer')) },
    terminal: term && b ? { cols: term.cols, rows: term.rows, lines: b.length, viewportTop: b.viewportY, scrollbackTop: b.baseY, atBottom: b.viewportY >= b.baseY, hidden: $('#terminal')?.classList.contains('catching-up') === true } : null,
    text: { title: clip($('#title')?.textContent, 80), status: clip($('#state')?.textContent, 120), input: clip($('#input-state')?.textContent, 40), sync: clip($('#sync')?.textContent, 40), version: clip($('#version-text')?.textContent, 60), agentNow: clip($('.agent-now')?.textContent, 120), presence: [...doc.querySelectorAll('#presence .chip')].map(c => clip(c.textContent, 50)) },
    devices: [...doc.querySelectorAll('nav#sessions .device')].map(d => ({ name: clip(d.querySelector('.device-name')?.textContent, 50), state: d.dataset.state, sessions: d.querySelectorAll('.session').length })),
    controls, accessibility: { controls: controls.length, unnamed: controls.filter(c => c.unnamed).map(c => c.id || c.role), landmarks: ['aside', 'header', 'main', 'footer', 'nav'].filter(t => $(t)) },
    timeline: timeline.events,
  };
}

/** Interface navigation only. Nothing here can type into, create or stop a session. */
export function runAction(action, handlers) {
  const [name, arg = ''] = String(action).split(':');
  const allowed = { reload: () => handlers.reload(), refresh: () => handlers.refresh(), activity: () => (arg === 'open' || arg === 'close') && handlers.activity(arg === 'open'), select: () => /^[a-zA-Z0-9-]{8,64}$/.test(arg) && handlers.select(arg), snapshot: () => handlers.snapshot() };
  if (!Object.hasOwn(allowed, name)) return false; allowed[name](); return true;
}
