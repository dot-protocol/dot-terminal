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
