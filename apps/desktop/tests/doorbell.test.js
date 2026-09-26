import test from 'node:test';
import assert from 'node:assert/strict';
import {createDoorbell} from '../src/doorbell.js';
class Socket{constructor(url){this.url=url;this.sent=[];Socket.last=this;}send(v){this.sent.push(JSON.parse(v));}close(){this.closed=true;this.onclose?.();}}
const say=(ws,v)=>ws.onmessage({data:JSON.stringify(v)});
test('the doorbell authenticates, subscribes, and rings on output without carrying bytes',()=>{
 const rings=[];const bell=createDoorbell({url:'ws://h/api/sessions/x/doorbell',token:'T',after:0,onOutput:v=>rings.push(v.type),WebSocketClass:Socket});
 const ws=Socket.last;ws.onopen();
 assert.deepEqual(ws.sent,[{type:'auth',token:'T'},{type:'subscribe',after:0}]);assert.equal(bell.live,true);
 say(ws,{type:'output',next:12,epoch:0});say(ws,{type:'heartbeat'});say(ws,{type:'output',next:40,epoch:1});
 assert.deepEqual(rings,['output','output'],'a heartbeat is not output');
 say(ws,{type:'exited'});assert.deepEqual(rings,['output','output','exited']);assert.equal(bell.live,false);
});
test('an older keeper makes the doorbell step aside so the view keeps polling',()=>{
 const changes=[];const bell=createDoorbell({url:'u',token:'T',after:0,onOutput:()=>{},onChange:b=>changes.push(b.live),WebSocketClass:Socket});
 const ws=Socket.last;ws.onopen();say(ws,{type:'unsupported',reason:'this keeper predates push; polling'});
 assert.equal(bell.live,false);assert.equal(bell.supported,false);assert.equal(ws.closed,true);assert.deepEqual(changes,[true,false]);
});
test('a dropped link is not live, so the idle timer takes over at once',()=>{
 const bell=createDoorbell({url:'u',token:'T',after:0,onOutput:()=>{},WebSocketClass:Socket});
 const ws=Socket.last;ws.onopen();assert.equal(bell.live,true);ws.onclose();assert.equal(bell.live,false);
});
test('a doorbell opened after the replay subscribes from the view offset, not from the start',()=>{
 createDoorbell({url:'u',token:'T',after:1048576,onOutput:()=>{},WebSocketClass:Socket});
 const ws=Socket.last;ws.onopen();assert.deepEqual(ws.sent[1],{type:'subscribe',after:1048576});
});
