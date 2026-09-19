import {test} from 'node:test';
import assert from 'node:assert/strict';
import {SessionSignals,LatestResize} from '../src/session-signals.js';
import {accessoryKey} from '../src/terminal-input.js';
import {probeControl} from '../src/control-state.js';
import {orderedResize} from '../src/render-flow.js';
test('sync distinguishes receipt, parsing, loss and stale views without content',()=>{
 let now=0;const s=new SessionSignals(()=>now);
 assert.equal(s.snapshot().state,'unknown');s.receive(20);s.apply(10);
 assert.equal(s.snapshot().pendingParseBytes,10);s.apply(20);
 assert.equal(s.snapshot().state,'caught-up-to-response');
 now=3001;assert.equal(s.snapshot().state,'stale');s.receive(40,true);s.apply(40);
 assert.equal(s.snapshot().state,'history-gap');s.fail();assert.equal(s.snapshot().state,'error');
 assert.equal(s.receive(39),false);assert.equal(s.receive(Infinity),false);
 assert.equal(s.snapshot().state,'error');
 s.reset('dot');assert.equal(s.snapshot().state,'unknown');assert.equal(s.snapshot().peerViews,'unknown');
 for(let i=0;i<200;i++)s.sample('read',i);
 assert.deepEqual(s.snapshot().latency.read,{count:120,p50:139,p95:193});
});
test('control probes retain lease on timeout and busy, but drop explicit fencing',async()=>{
 for(const error of [new Error('timeout'),Object.assign(new Error('busy'),{code:'keeper-error'})]){
  assert.deepEqual(await probeControl(async()=>{throw error;},9),{generation:9,state:'unconfirmed'});
 }
 assert.deepEqual(await probeControl(async()=>{throw Object.assign(new Error('fenced'),{code:'controller-fenced'});},9),{generation:0,state:'fenced'});
});
test('bad telemetry cannot throw into rendering and history count survives recovery',()=>{
 const s=new SessionSignals();s.receive(20,true);s.apply(20);s.receive(20);
 assert.equal(s.snapshot().state,'caught-up-to-response');assert.equal(s.snapshot().historyGaps,1);
 assert.equal(s.snapshot().historyComplete,false);
 assert.doesNotThrow(()=>s.receive(-1));assert.equal(s.snapshot().state,'measurement-error');
});
test('resize serializes pending operations and skips superseded intermediate sizes',async()=>{
 const seen=[];let unblock;
 const q=new LatestResize(async v=>{seen.push(v);if(v.cols===80)await new Promise(r=>unblock=r);});
 const done=q.request({cols:80});q.request({cols:90});q.request({cols:100});
 assert.deepEqual(seen,[{cols:80}]);unblock();await done;
 assert.deepEqual(seen,[{cols:80},{cols:100}]);await q.request({cols:100});assert.equal(seen.length,2);
});
test('accessory arrows follow application cursor mode without rewriting other input',()=>{
 assert.equal(accessoryKey('\x1b[A',true),'\x1bOA');
 assert.equal(accessoryKey('\x1b[A',false),'\x1b[A');
 assert.equal(accessoryKey('\x03',true),'\x03');
});
test('resize drains old output and prepares parser before remote redraw, rejects superseded view',async()=>{
 const events=[];let release;let current=true;
 const run=()=>orderedResize({drain:()=>new Promise(r=>{release=r;}),isCurrent:()=>current,prepareGrid:()=>events.push('grid'),send:async()=>events.push('remote-redraw')});
 const first=run();assert.deepEqual(events,[]);release();assert.equal(await first,true);
 assert.deepEqual(events,['grid','remote-redraw']);
 events.length=0;const second=run();current=false;release();assert.equal(await second,false);assert.deepEqual(events,[]);
});
