import {test} from 'node:test';import assert from 'node:assert/strict';import {createInputGate} from '../src/input-gate.js';
const tick=()=>new Promise(r=>setImmediate(r));
test('keys cannot overtake the first burst between lease grant and resize ACK',async()=>{
 let lease=false,done,epoch=1;const sent=[];const send=createInputGate({ready:()=>lease,acquire:()=>new Promise(r=>done=r),submit:t=>sent.push(t),context:()=>epoch});
 send('p');await tick();lease=true;send('w');send('d');send('\r');assert.deepEqual(sent,[]);done(true);await tick();assert.deepEqual(sent,['pwd\r']);send('x');assert.deepEqual(sent,['pwd\r','x']);
});
test('switching tabs discards old pending input instead of injecting it into the new session',async()=>{
 let epoch=1;const done=[],sent=[];const send=createInputGate({ready:()=>false,acquire:()=>new Promise(r=>done.push(r)),submit:t=>sent.push(t),context:()=>epoch});send('old');await tick();epoch=2;send('new');await tick();done[0](true);await tick();assert.deepEqual(sent,[]);done[1](true);await tick();assert.deepEqual(sent,['new']);
});
