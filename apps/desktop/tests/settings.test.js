import {test} from 'node:test';
import assert from 'node:assert/strict';
import {normalize,loadAppearance,defaults,terminalTheme} from '../src/appearance.js';
import {routeKey,HealthRegistry,stateEvent} from '../src/observability.js';
test('appearance handles corrupt preferences and bounds exact numeric settings',()=>{
 assert.deepEqual(loadAppearance({getItem:()=>'{bad'}),defaults);
 assert.deepEqual(normalize(null),defaults);
 const v=normalize({theme:'paper',terminalSize:17.5,uiSize:16,lineHeight:1.35,font:'JetBrains Mono, monospace'});
 assert.equal(v.terminalSize,17.5);assert.equal(v.lineHeight,1.35);
 assert.equal(normalize({terminalSize:500}).terminalSize,40);
 assert.equal(normalize({font:'url(https://example.invalid)'}).font,defaults.font);
 assert.equal(terminalTheme('paper').background,'#faf8f2');
});
test('route telemetry never contains IDs or arbitrary request properties',()=>{
 assert.equal(routeKey('sessions/private-session',{type:'read',secret:'hidden'}),'sessions.read');
 assert.equal(routeKey('vault',{action:'put',value:'hidden'}),'vault.put');
 assert.equal(routeKey('sessions/private-session',{type:'arbitrary-secret'}),'unknown');
 assert.equal(routeKey('private-token',{}),'unknown');
});
test('health distinguishes unknown, error, stale and concurrent requests',()=>{
 const h=new HealthRegistry();assert.equal(h.snapshot().find(r=>r.id==='sessions.list').state,'unknown');
 const a=h.begin('sessions.list'),b=h.begin('sessions.list');assert.equal(h.snapshot().find(r=>r.id==='sessions.list').inflight,2);
 a(true);b(false);const v=h.snapshot().find(r=>r.id==='sessions.list');assert.equal(v.state,'error');assert.equal(v.failures,1);assert.equal(v.inflight,0);
 assert.equal(h.snapshot(v.last+16000).find(r=>r.id==='sessions.list').state,'stale');
 const e=stateEvent({kind:'private-session',queuedBytes:Infinity,token:'do-not-publish',controlHeld:true},h);
 assert.equal(e.state.kind,'welcome');assert.equal(e.state.queuedBytes,0);assert.ok(!JSON.stringify(e).includes('do-not-publish'));
});
