import {test} from 'node:test';
import assert from 'node:assert/strict';
import {sampleState,bytes} from '../src/device-resources.js';
test('missing, stale and clock-skewed samples never appear live',()=>{
 assert.equal(sampleState({},100),'unavailable');
 assert.equal(sampleState({at:70,state:'ok'},100),'stale');
 assert.equal(sampleState({at:120,state:'ok'},100),'stale');
 assert.equal(sampleState({at:99,state:'ok'},100),'live');
 assert.equal(sampleState({at:99,state:'degraded'},100),'degraded');
});
test('absent memory is not presented as zero',()=>{
 assert.equal(bytes(null),'—');assert.equal(bytes(undefined),'—');assert.equal(bytes(0),'0 MB');
});
