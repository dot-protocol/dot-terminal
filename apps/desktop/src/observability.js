// Bounded, local metadata only. Never store request/response bodies, errors,
// authorization headers, session IDs, user input, vault names or terminal output.
export const routes=['sessions.list','sessions.create','sessions.status','sessions.check_control','sessions.read','sessions.screen','sessions.acquire','sessions.release','sessions.resize','sessions.input','iterm.list','iterm.screen','iterm.input','resources','vault.list','vault.put','vault.delete','vault.run'];
export function routeKey(path,data) {
 if(path==='sessions')return data===undefined?'sessions.list':'sessions.create';
 if(path.startsWith('sessions/'))return routes.includes('sessions.'+data?.type)?'sessions.'+data.type:'unknown';
 if(path==='iterm'||path==='vault')return routes.includes(path+'.'+data?.action)?path+'.'+data.action:'unknown';
 return path==='resources'?'resources':'unknown';
}
export class HealthRegistry {
 constructor(){this.values=new Map(routes.map(id=>[id,{id,calls:0,failures:0,inflight:0,last:0,latency:0,state:'unknown'}]));}
 begin(id){const r=this.values.get(id);if(!r)return()=>{};r.calls++;r.inflight++;const start=performance.now();return ok=>{r.inflight--;r.last=Date.now();r.latency=Math.round(performance.now()-start);r.state=ok?'healthy':'error';if(!ok)r.failures++;};}
 snapshot(now=Date.now()){return [...this.values.values()].map(r=>({...r,state:r.last&&now-r.last>15000?'stale':r.state}));}
}
export function indexShell(root) {
 const entries=[];const walker=document.createTreeWalker(root,NodeFilter.SHOW_TEXT);
 const dynamic=new Set(['title','state','input-state','mode','details','sessions','iterm-list','sync','plan-progress','plan-list']);
 let node;while((node=walker.nextNode())){
  const parent=node.parentElement, text=(parent.matches('button[aria-label]')?parent.getAttribute('aria-label'):node.textContent).trim();if(!text||parent.closest('#terminal:not(:has(#welcome)),input,textarea,script,style'))continue;
  if([...dynamic].some(id=>parent.closest('#'+id)))continue;
  const container=parent.closest('[id]');
  entries.push({id:parent.matches('button[id]')?'action.'+parent.id:'copy.'+entries.length,element:container?.id||parent.tagName.toLowerCase(),kind:'static',role:parent.closest('button,h1,h2')?'primary':'secondary',signal:parent.closest('.eyebrow,footer,.bottom p')?'ambient':'signal',text,words:text.toLowerCase().match(/[\p{L}\p{N}]+/gu)||[]});
 }
 for(const id of dynamic)entries.push({id:'slot.'+id,element:id,kind:'dynamic',role:id==='state'?'primary':'secondary',signal:'signal',text:'[value excluded]',words:[]});
 return entries;
}

// Explicit publication contract. Values outside this allowlist never enter telemetry.
export const stateFields = [
 {id:'view.kind',type:'enum',unit:null,privacy:'operational'},
 {id:'view.controlHeld',type:'boolean',unit:null,privacy:'operational'},
 {id:'input.queuedBytes',type:'number',unit:'bytes',privacy:'operational'},
 {id:'output.pollRunning',type:'boolean',unit:null,privacy:'operational'},
];
export function stateEvent(values,registry,now=Date.now()) {
 return {schema:'dot.ui.state.v1',module:'desktop-view',at:now,
 state:{kind:['dot','iterm'].includes(values.kind)?values.kind:'welcome',controlHeld:values.controlHeld===true,
 queuedBytes:Number.isFinite(values.queuedBytes)?Math.min(16384,Math.max(0,values.queuedBytes)):0,pollRunning:values.pollRunning===true},
 apis:registry.snapshot(now)};
}
