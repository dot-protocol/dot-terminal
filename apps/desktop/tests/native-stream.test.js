import test from 'node:test';
import assert from 'node:assert/strict';
import {nativeStreamSocket} from '../src/native-stream.js';
test('native stream orders replay, sends no owner credential and closes only its view',async()=>{
 const calls=[];let reads=0;let done;
 const received=new Promise(r=>done=r);
 const Socket=nativeStreamSocket(async(path,body)=>{calls.push(body);if(body.op==='open')return {view:'test-view'};if(body.op==='read'){reads++;return {frames:reads===1?[{seq:1,frame:{data:'4142'}},{seq:2,frame:{control:'{"type":"replay_complete"}'}}]:[],closed:false};}return {queued:true};},'external/server/s_test/view');
 const socket=new Socket(),frames=[];
 socket.onopen=()=>socket.send('{"type":"auth","token":"must-not-be-sent"}');
 socket.onmessage=e=>{frames.push(e.data);if(frames.length===2)done();};
 await received;assert.deepEqual([...new Uint8Array(frames[0])],[65,66]);assert.match(frames[1],/replay_complete/);
 socket.send(new Uint8Array([67]));await socket.chain;socket.close();
 assert.ok(calls.some(c=>c.op==='send'&&c.data==='43'));assert.ok(calls.some(c=>c.op==='close'));
 assert.equal(JSON.stringify(calls).includes('must-not-be-sent'),false);assert.equal(calls.some(c=>c.op==='stop'),false);
});
test('native stream maps control to its own operations and reports an observer refusal without disconnecting',async()=>{
 const calls=[];
 const Socket=nativeStreamSocket(async(path,body)=>{calls.push(body);
  if(body.op==='open')return {view:'v1'};
  if(body.op==='read')return new Promise(()=>{});
  if(body.op==='take_control'){if(!body.takeover){const e=new Error('another view is typing in this session; take over to type here');e.status=409;throw e;}return {control:true,generation:3};}
  if(body.op==='send'&&body.data){const e=new Error('observing: take control to type into this session');e.status=403;throw e;}
  return {};
 });
 const socket=new Socket(),messages=[];let closed=false;
 socket.onmessage=e=>messages.push(JSON.parse(e.data));socket.onclose=()=>{closed=true;};
 await new Promise(r=>{socket.onopen=r;});
 socket.send(new Uint8Array([65]));await socket.chain;
 socket.send('{"type":"take_control","takeover":false}');await socket.chain;
 socket.send('{"type":"take_control","takeover":true}');await socket.chain;
 assert.equal(closed,false,'a refused keystroke must not tear down the view');
 assert.deepEqual(messages.map(m=>m.state),['observing','refused','granted']);
 assert.equal(messages[2].generation,3);
 assert.deepEqual(calls.filter(c=>c.op==='take_control').map(c=>c.takeover),[false,true]);
 socket.close();
});
