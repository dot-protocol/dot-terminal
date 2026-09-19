// The project plan, shown in the sidebar. Read-only: a static file shipped with the UI. It holds
// no session data, and everything from it is rendered as text with fixed state labels.
export const STATES = ['done', 'running', 'open', 'blocked', 'planned'];
const LABEL = { done: 'Done', running: 'In progress', open: 'Next', blocked: 'Blocked', planned: 'Planned' };
const text = (value, max) => (typeof value === 'string' ? value.slice(0, max) : '');

export function validatePlan(data) {
  if (data?.schema !== 'dot.plan.v1' || !Array.isArray(data.tasks) || data.tasks.length > 200) throw new Error('Unsupported plan');
  const seen = new Set();
  const tasks = data.tasks.map(t => {
    const id = text(t?.id, 60), title = text(t?.title, 80);
    if (!/^[a-z0-9][a-z0-9-]*$/.test(id) || seen.has(id) || !title || !STATES.includes(t.state)) throw new Error('Invalid plan task');
    seen.add(id);
    return { id, title, state: t.state, note: text(t.note, 400), ref: /^#\d{1,6}$/.test(t.ref ?? '') ? t.ref : '' };
  });
  return { updated: /^\d{4}-\d{2}-\d{2}$/.test(data.updated ?? '') ? data.updated : '', tasks };
}

/** Done first is history; what a person wants on top is what is moving, then what is next. */
export function orderPlan(tasks) {
  const rank = { running: 0, blocked: 1, open: 2, planned: 3, done: 4 };
  return tasks.map((t, i) => [t, i]).sort((a, b) => rank[a[0].state] - rank[b[0].state] || a[1] - b[1]).map(([t]) => t);
}

export function summarizePlan(tasks) {
  const count = state => tasks.filter(t => t.state === state).length;
  return { total: tasks.length, done: count('done'), running: count('running'), blocked: count('blocked') };
}

export function installPlan(root, { load = () => fetch('plan.json', { cache: 'no-store' }).then(r => { if (!r.ok) throw new Error('Plan unavailable'); return r.json(); }), everyMs = 60000 } = {}) {
  root.innerHTML = '<div class="section"><span data-copy-id="plan.title">Plan</span><span id="plan-progress" class="plan-progress"></span></div><div class="plan-bar" role="img"><i></i></div><ol id="plan-list" class="plan-list"></ol>';
  const list = root.querySelector('ol'), progress = root.querySelector('#plan-progress'), bar = root.querySelector('.plan-bar'), open = new Set();
  let last = '';
  function draw(plan) {
    const s = summarizePlan(plan.tasks);
    progress.textContent = s.done + ' / ' + s.total; bar.firstElementChild.style.width = (s.total ? Math.round(s.done / s.total * 100) : 0) + '%';
    bar.setAttribute('aria-label', s.done + ' of ' + s.total + ' tasks done' + (s.blocked ? ', ' + s.blocked + ' blocked' : ''));
    list.replaceChildren();
    for (const t of orderPlan(plan.tasks)) {
      const li = document.createElement('li'), details = document.createElement('details'), summary = document.createElement('summary'), p = document.createElement('p'), state = document.createElement('span');
      li.dataset.state = t.state; details.open = open.has(t.id); details.ontoggle = () => { if (details.open) open.add(t.id); else open.delete(t.id); };
      summary.textContent = t.title; state.className = 'plan-state'; state.textContent = LABEL[t.state] + (t.ref ? ' · ' + t.ref : '');
      p.textContent = t.note; details.append(summary, state, p); li.append(details); list.append(li);
    }
    root.dataset.state = 'loaded';
  }
  async function refresh() {
    try { const raw = await load(), key = JSON.stringify(raw); if (key === last) return; draw(validatePlan(raw)); last = key; }
    catch (e) { if (!last) { root.dataset.state = 'error'; progress.textContent = 'unavailable'; } }
  }
  refresh(); const timer = setInterval(() => { if (!document.hidden) refresh(); }, everyMs);
  return { refresh, dispose() { clearInterval(timer); } };
}
