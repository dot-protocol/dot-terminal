import {test} from 'node:test';
import assert from 'node:assert/strict';
import {SessionSignals,LatestResize} from '../src/session-signals.js';
test('sync distinguishes receipt, parsing, loss and stale views without content',()=>{
 let now=0;const s=new SessionSignals(()=>now);
 assert.equal(s.snapshot().state,'unknown');s.receive(20);s.apply(10);
 assert.equal(s.snapshot().pendingParseBytes,10);s.apply(20);
 assert.equal(s.snapshot().state,'caught-up-to-response');
 now=3001;assert.equal(s.snapshot().state,'stale');s.receive(40,true);s.apply(40);
 assert.equal(s.snapshot().state,'history-gap');s.fail();assert.equal(s.snapshot().state,'error');
 assert.throws(()=>s.receive(39));assert.throws(()=>s.receive(Infinity));
 s.reset('dot');assert.equal(s.snapshot().state,'unknown');assert.equal(s.snapshot().peerViews,'unknown');
 for(let i=0;i<200;i++)s.sample('read',i);
 assert.deepEqual(s.snapshot().latency.read,{count:120,p50:139,p95:193});
});
test('resize serializes pending operations and skips superseded intermediate sizes',async()=>{
 const seen=[];let unblock;
 const q=new LatestResize(async v=>{seen.push(v);if(v.cols===80)await new Promise(r=>unblock=r);});
 const done=q.request({cols:80});q.request({cols:90});q.request({cols:100});
 assert.deepEqual(seen,[{cols:80}]);unblock();await done;
 assert.deepEqual(seen,[{cols:80},{cols:100}]);await q.request({cols:100});assert.equal(seen.length,2);
});
