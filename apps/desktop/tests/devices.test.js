import {test} from 'node:test';
import assert from 'node:assert/strict';
import {normalizeDevices, normalizeSessions, sessionsPath, sessionPath} from '../src/devices.js';

test('the local device comes first; junk, duplicates and unknown values are dropped or defaulted', () => {
  const list = normalizeDevices({devices: [
    {id: 'core', name: 'core VPS', kind: 'server', local: false, state: 'connected', can_create: true, capability: 'never-shown'},
    {id: 'local', name: 'MacBook', kind: 'laptop', local: true, state: 'connected', can_create: true},
    {id: 'core', name: 'dup'}, {id: '../etc', name: 'x'}, {id: 'pi', name: 'a\x1b[31mb', kind: 'toaster', state: 'weird'},
  ]});
  assert.deepEqual(list.map(d => d.id), ['local', 'core', 'pi']);
  assert.equal(list[1].capability, undefined);
  assert.deepEqual([list[2].name, list[2].kind, list[2].state, list[2].canCreate], ['a[31mb', 'laptop', 'offline', false]);
  assert.throws(() => normalizeDevices({devices: [{id: 'core', name: 'x'}]}), /no local device/);
  assert.throws(() => normalizeDevices({}));
});
test('a remote device cannot claim to be the local one', () => {
  const list = normalizeDevices({devices: [{id: 'local', local: true, name: 'Mac'}, {id: 'core', local: true, name: 'core'}]});
  assert.equal(list.find(d => d.id === 'core').local, false);
});
test('calls for a session go through its device', () => {
  assert.equal(sessionsPath('local'), 'sessions'); assert.equal(sessionPath('local', 'abcd1234'), 'sessions/abcd1234');
  assert.equal(sessionPath('core', 'abcd1234'), 'devices/core/sessions/abcd1234');
});
test('session lists are bounded and ids are checked', () => {
  const s = normalizeSessions({sessions: [{session: 'abcd1234ef', exited: false, pid: 7}, {session: '../../x'}, {session: 'short'}, null]});
  assert.deepEqual(s, [{id: 'abcd1234ef', exited: false, pid: 7}]);
  assert.deepEqual(normalizeSessions(null), []);
});
