import {test} from 'node:test';
import assert from 'node:assert/strict';
import {validateTrajectory,groupTrajectory} from '../src/trajectory.js';
const tool=(at,state='returned')=>({kind:'tool',at,category:'shell',state,batchSteps:0});
test('trajectory rejects invalid records and strips arbitrary content',()=>{
 const [e]=validateTrajectory({schema:'dot.trajectory.v1',events:[{...tool('2026-01-01'),command:'secret',output:'private'}]});
 assert.equal(e.command,undefined);assert.equal(e.output,undefined);
 assert.throws(()=>validateTrajectory({schema:'dot.trajectory.v1',events:[tool('bad')]}));
 assert.throws(()=>validateTrajectory({schema:'dot.trajectory.v1',events:[{...e,category:'<script>'}]}));
});
test('grouping preserves errors and compaction boundaries; does not call missing results running',()=>{
 const events=[tool('2026-01-01T00:00:00Z'),tool('2026-01-01T00:00:01Z'),tool('2026-01-01T00:00:02Z','error'),{kind:'compact',at:'2026-01-01T00:00:03Z'},tool('2026-01-01T00:00:04Z','unresolved')];
 const groups=groupTrajectory(events);assert.equal(groups.length,4);assert.equal(groups[0].count,2);assert.equal(groups[1].state,'error');assert.equal(groups[3].state,'unresolved');
});
