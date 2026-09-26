import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createClient} from '../src/index.js';
test('same session ID on different hosts routes independently',async()=>{
 const paths=[];const c=createClient({request:async(path,body)=>{paths.push([path,body]);return {type:'ack'};}});
 await c.session({device:'local',id:'abcdef123456'}).operation({type:'status'});
 await c.session({device:'core',id:'abcdef123456'}).operation({type:'status'});
 assert.deepEqual(paths.map(p=>p[0]),['sessions/abcdef123456','devices/core/sessions/abcdef123456']);
 assert.throws(()=>c.session({device:'../vault',id:'abcdef123456'}));
});
test('keeper fences and uncertainty propagate without replay',async()=>{
 let calls=0;const c=createClient({request:async()=>{calls++;return {type:'error',message:'stale controller generation'};}});
 await assert.rejects(c.session({id:'abcdef123456'}).operation({type:'input'}),e=>e.code==='controller-fenced');assert.equal(calls,1);
});
test('clients do not share credentials or device catalogs',async()=>{
 const req=name=>async()=>({devices:[{id:'local',name,local:true,state:'connected',kind:'laptop'}]});
 const a=createClient({request:req('First')}),b=createClient({request:req('Second')});
 assert.equal((await a.devices())[0].name,'First');assert.equal((await b.devices())[0].name,'Second');
});
