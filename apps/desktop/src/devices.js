// The sidebar's model: devices, and each device's sessions. No DOM, no network.
// A session belongs to exactly one device; every call for it goes through that device's route.
const KINDS = ['laptop', 'server', 'phone'], STATES = ['connected', 'offline', 'refused'];
const text = (v, max) => (typeof v === 'string' ? v.replace(/[\x00-\x1f\x7f]/g, '').slice(0, max) : '');

/** Backends older than the device catalog have no /api/devices: they are one local device. */
export const LOCAL_ONLY = [{ id: 'local', name: 'This device', kind: 'laptop', local: true, state: 'connected', canCreate: true }];

export function normalizeDevices(data) {
  if (!Array.isArray(data?.devices)) throw new Error('Unsupported device list');
  const seen = new Set(), out = [];
  for (const d of data.devices.slice(0, 32)) {
    const id = text(d?.id, 32);
    if (!/^[a-z0-9-]{1,32}$/.test(id) || seen.has(id)) continue;
    seen.add(id);
    out.push({ id, name: text(d.name, 40) || id, kind: KINDS.includes(d.kind) ? d.kind : 'laptop', local: d.local === true && id === 'local',
      state: STATES.includes(d.state) ? d.state : 'offline', canCreate: d.can_create === true });
  }
  if (!out.some(d => d.local)) throw new Error('Device list has no local device');
  return out.sort((a, b) => Number(b.local) - Number(a.local));
}

/** Where a device's session calls go. The local device keeps the original routes. */
export const sessionsPath = device => (device === 'local' ? 'sessions' : 'devices/' + encodeURIComponent(device) + '/sessions');
export const sessionPath = (device, id) => sessionsPath(device) + '/' + encodeURIComponent(id);

export function normalizeSessions(data) {
  return (Array.isArray(data?.sessions) ? data.sessions : []).filter(s => /^[a-zA-Z0-9-]{8,64}$/.test(s?.session ?? '')).slice(0, 200)
    .map(s => ({ id: s.session, exited: s.exited === true, pid: Number.isSafeInteger(s.pid) ? s.pid : null, usage: s.usage ?? null }));
}

export const KIND_GLYPH = { laptop: '▭', server: '▤', phone: '▯' };
export const STATE_LABEL = { connected: 'Connected', offline: 'Offline', refused: 'Refused this device' };
