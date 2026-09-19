// Explicit local import only. Never reads the PTY, uploads content, or enters telemetry.
export function validateTrajectory(data) {
 if(data?.schema!=='dot.trajectory.v1'||!Array.isArray(data.events)||data.events.length>50000)throw new Error('Unsupported activity snapshot');
 const events=data.events.map(e=>{
  if(!e||!['tool','compact','input'].includes(e.kind)||!Number.isFinite(Date.parse(e.at)))throw new Error('Invalid activity event');
  const v={kind:e.kind,at:e.at};
  if(e.kind==='tool'){
   if(!['shell','files','browser','tasks','agent','oracle','other'].includes(e.category)||!['returned','error','unresolved'].includes(e.state))throw new Error('Invalid tool event');
   Object.assign(v,{category:e.category,state:e.state,batchSteps:Number.isSafeInteger(e.batchSteps)&&e.batchSteps>=0?Math.min(10000,e.batchSteps):0});
  }
  if(e.kind==='compact')for(const key of ['before','after','durationMs'])if(Number.isFinite(e[key])&&e[key]>=0)v[key]=e[key];
  return v;
 });
 return events.sort((a,b)=>Date.parse(a.at)-Date.parse(b.at));
}
export function groupTrajectory(events) {
 const groups=[];
 for(const e of events){const last=groups.at(-1);if(e.kind==='tool'&&e.state==='returned'&&last?.kind==='tool'&&last.state==='returned'&&last.category===e.category&&Date.parse(e.at)-Date.parse(last.lastAt)<120000&&last.count<50){last.count++;last.lastAt=e.at;last.batchSteps+=e.batchSteps;}
 else groups.push({...e,count:1,lastAt:e.at});}
 return groups;
}
export function installTrajectory(root,button) {
 let events=[],page=0;const limit=60;
 const panel=document.createElement('section');panel.id='trajectory';panel.dataset.state='empty';panel.hidden=true;panel.setAttribute('aria-label','Session activity snapshot');
 panel.innerHTML='<div class="trajectory-head"><strong data-copy-id="trajectory.title">Trajectory</strong><button aria-label="Close trajectory">×</button></div><label class="trajectory-import" data-copy-id="trajectory.import">Import activity snapshot<input type="file" accept="application/json,.json" aria-label="Import activity snapshot"></label><p class="trajectory-note">Local snapshot · independent of the selected PTY. No live agent feed connected.</p><output aria-live="polite"></output><div class="trajectory-filters" data-copy-id="trajectory.filter"><label><input type="checkbox"> Errors & compactions only</label></div><ol></ol><button class="trajectory-more" data-copy-id="trajectory.earlier">Show earlier</button>';
 root.append(panel);
 const output=panel.querySelector('output'),list=panel.querySelector('ol'),filter=panel.querySelector('input[type=checkbox]'),more=panel.querySelector('.trajectory-more');
 function toggle(open){panel.hidden=!open;button.setAttribute('aria-expanded',String(open));root.classList.toggle('trajectory-open',open);}
 button.onclick=()=>toggle(panel.hidden);panel.querySelector('.trajectory-head button').onclick=()=>toggle(false);
 function render(){
  const grouped=groupTrajectory(events).filter(e=>!filter.checked||e.kind==='compact'||e.state==='error');const tail=grouped.slice(Math.max(0,grouped.length-(page+1)*limit));list.replaceChildren();
  for(const g of tail){const li=document.createElement('li');li.dataset.state=g.state||g.kind;const details=document.createElement('details'),summary=document.createElement('summary');
   const time=document.createElement('time');time.textContent=new Date(g.at).toLocaleString(undefined,{month:'short',day:'numeric',hour:'2-digit',minute:'2-digit'});summary.append(time);
   const label=g.kind==='compact'?'Context compacted':g.kind==='input'?'Input recorded':`${g.category} ×${g.count} · ${g.state==='returned'?'returned':g.state==='error'?'error':'no result recorded'}`;
   summary.append(document.createTextNode(label));details.append(summary);const p=document.createElement('p');
   p.textContent=g.kind==='compact'?`${g.before?.toLocaleString()??'?'} → ${g.after?.toLocaleString()??'?'} tokens${g.durationMs!==undefined?' · '+Math.round(g.durationMs/1000)+'s':''}`:g.kind==='tool'?`${g.batchSteps?g.batchSteps+' explicit batch steps. ':''}Tool return does not prove the intended change succeeded. Raw arguments and output are excluded.`:'Message contents excluded. This marker may include harness-injected input.';
   details.append(p);li.append(details);list.append(li);
  }
  more.hidden=tail.length===grouped.length;
 }
 filter.onchange=()=>{page=0;render();};more.onclick=()=>{page++;render();};
 panel.querySelector('input[type=file]').onchange=async ev=>{try{const file=ev.target.files[0];if(!file)return;if(file.size>12000000)throw new Error('Snapshot exceeds 12 MB');const next=validateTrajectory(JSON.parse(await file.text()));events=next;page=0;panel.dataset.state='loaded';output.textContent=`${events.filter(e=>e.kind==='tool').length.toLocaleString()} tools · ${events.filter(e=>e.kind==='compact').length} compactions · ${events.filter(e=>e.state==='error').length} errors`;render();}catch(e){panel.dataset.state='error';output.textContent=e.message;}finally{ev.target.value='';}};
 render();
}
