import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createTransport} from '../src/transport.js';
test('native responses correlate even when completed out of order; no bearer is passed',async()=>{
 let reply;const calls=[];const bridge={request:(...args)=>calls.push(args)};
 const request=createTransport({bridge,receive:fn=>reply=fn,storage:{},capability:'PRIVATE'});
 const a=request('sessions'),b=request('devices/core/sessions');
 assert.equal(JSON.stringify(calls).includes('PRIVATE'),false);
 reply(calls[1][0],{status:200,body:{sessions:['remote']}});
 reply(calls[0][0],{status:200,body:{sessions:['local']}});
 assert.deepEqual(await a,{sessions:['local']});assert.deepEqual(await b,{sessions:['remote']});
 await assert.rejects(request('vault',{action:'list'}),/host/);
});
test('uncertain input times out without automatic retry',async()=>{
 let calls=0;const request=createTransport({bridge:{request:()=>calls++},receive:()=>{},storage:{},timeout:10});
 await assert.rejects(request('sessions/a',{type:'input'}),/not retried/);assert.equal(calls,1);
});
import {routeKey} from '../src/observability.js';
test('remote telemetry uses bounded route names without recording device or session identifiers',()=>{assert.equal(routeKey('devices/private-server/sessions/private-session',{type:'input',data:[1,2,3]}),'sessions.input');assert.equal(routeKey('devices/private-server/sessions'),'sessions.list');assert.equal(routeKey('devices'),'devices.list');});

test('session labels travel through the native workspace without owner credentials',async()=>{
 let reply;let call;const request=createTransport({bridge:{request:(...args)=>call=args},receive:fn=>reply=fn,storage:{},capability:'PRIVATE'});
 const pending=request('session-labels',{device:'local',id:'abcdef0123456789abcdef0123456789',name:'Research'});
 assert.equal(JSON.stringify(call).includes('PRIVATE'),false);reply(call[0],{status:200,body:{saved:true}});assert.equal((await pending).saved,true);
 assert.equal(routeKey('session-labels',{name:'Never index this'}),'sessions.labels.write');
});
test('a refusal from the host never reads as a connection failure; a 401 names the access link',async()=>{
 const fetcher=async(_,init)=>({status:init?.method==='POST'?500:401,text:async()=>''});
 const request=createTransport({fetcher,storage:{},capability:''});
 await assert.rejects(request('devices'),e=>e.status===401&&/needs its access link/.test(e.message)&&!/Connection refused/.test(e.message));
 await assert.rejects(request('sessions',{}),e=>e.status===500&&/refused the request \(500\)/.test(e.message));
});
