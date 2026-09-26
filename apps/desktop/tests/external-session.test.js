import test from 'node:test';
import assert from 'node:assert/strict';
import {ExternalSession} from '../src/external-session.js';
class Socket {
 constructor(){this.readyState=1;this.bufferedAmount=0;this.sent=[];}
 send(v){this.sent.push(v);}
 close(){this.readyState=3;this.onclose?.();}
 receive(v){this.onmessage({data:v});}
}
const tick=()=>new Promise(r=>setImmediate(r));
test('replay barrier waits for rendering; disconnect rejects input and dispose only closes view',async()=>{
 let finish;const rendered=[];
 const v=new ExternalSession({url:'ws://localhost',token:'test',WebSocketClass:Socket,onState:()=>{},write:b=>new Promise(r=>{rendered.push([...b]);finish=r;})});
 v.ws.onopen();assert.equal(JSON.parse(v.ws.sent[0]).type,'auth');
 v.ws.receive(new Uint8Array([65]).buffer);v.ws.receive('{"type":"replay_complete"}');await tick();assert.equal(v.ready,false);assert.throws(()=>v.input(new Uint8Array([66])));
 finish();await tick();assert.equal(v.ready,true);assert.deepEqual(rendered,[[65]]);v.input(new Uint8Array([66]));v.dispose();assert.equal(v.ready,false);assert.throws(()=>v.input(new Uint8Array([67])));assert.equal(v.ws.sent.length,2);
});
test('oversized queued output closes instead of silently losing bytes',()=>{
 const v=new ExternalSession({url:'ws://localhost',token:'test',WebSocketClass:Socket,onState:()=>{},write:async()=>{}});
 v.ws.receive(new Uint8Array(4*1024*1024+1).buffer);assert.equal(v.ws.readyState,3);assert.equal(v.ready,false);
});

test('socket loss while replay is rendering cannot re-enable input',async()=>{
 let finish;
 const v=new ExternalSession({url:'ws://localhost',token:'test',WebSocketClass:Socket,onState:()=>{},write:()=>new Promise(r=>{finish=r;})});
 v.ws.receive(new Uint8Array([65]).buffer);v.ws.receive('{"type":"replay_complete"}');await tick();v.ws.close();finish();await tick();assert.equal(v.ready,false);
});
