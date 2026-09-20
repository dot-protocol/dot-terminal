// Owner inventory is rendered locally, never indexed or emitted as telemetry.
export function sampleState(snapshot,now=Date.now()/1000) {
 if(!Number.isFinite(snapshot?.at))return 'unavailable';
 if(snapshot.at>now+5||now-snapshot.at>20)return 'stale';
 return snapshot.state==='ok'||snapshot.state==='ready'||snapshot.state==='alive'?'live':snapshot.state||'unknown';
}
export function bytes(n) {
 if(!Number.isFinite(n)||n<0)return '—';
 return n>=1073741824?(n/1073741824).toFixed(1)+' GB':(n/1048576).toFixed(0)+' MB';
}
export function mountDeviceResources(root,{request,device,sessionLabels={}}) {
 let disposed=false,timer;
 const el=(tag,text)=>{const n=document.createElement(tag);if(text!==undefined)n.textContent=text;return n;};
 const status=el('p','Loading measurements…');status.setAttribute('role','status');
 const cards=el('div');cards.className='resource-cards';
 const sessions=el('div');
 const details=el('details');details.append(el('summary','Processes'));
 const scroller=el('div');scroller.className='resource-table-scroll';details.append(scroller);
 const note=el('p','Host CPU is an average across cores. Process/session CPU: 100% is one core. Resident memory sums may count shared pages twice. Virtual memory is address space, not RAM consumed. Collection is read-only; API health requires explicit instrumentation.');note.className='resource-note';
 root.append(status,cards,sessions,details,note);
 function table(parent,headers,rows){parent.replaceChildren();const t=el('table');const head=el('thead');const hr=el('tr');headers.forEach(h=>hr.append(el('th',h)));head.append(hr);t.append(head);const body=el('tbody');for(const values of rows){const r=el('tr');values.forEach(v=>r.append(el('td',v)));body.append(r);}t.append(body);parent.append(t);}
 async function update(){
  if(disposed)return;
  if(document.hidden){timer=setTimeout(update,2000);return;}
  try{
   const path=device.id==='local'?'resources':'devices/'+encodeURIComponent(device.id)+'/resources';
   const [snapshot,list]=await Promise.all([request(path),request(device.id==='local'?'sessions':'devices/'+encodeURIComponent(device.id)+'/sessions').catch(()=>null)]);
   if(disposed)return;
   const state=sampleState(snapshot);status.textContent=state==='unavailable'?'Measurements unavailable on this device.':state+' · Sample '+new Date(snapshot.at*1000).toLocaleTimeString()+' · Updates every 5 seconds';
   cards.replaceChildren();
   if(state==='unavailable'){sessions.replaceChildren();scroller.replaceChildren();return;}
   for(const [label,value] of [['CPU',Number(snapshot.cpu).toFixed(1)+'% / '+snapshot.cores+' cores'],['Memory',bytes(snapshot.memory_used)+' / '+bytes(snapshot.memory_total)],['Swap',bytes(snapshot.swap_used)]]){const card=el('div');card.append(el('small',label),el('strong',value));cards.append(card);}
   const rows=Array.isArray(list)?list:list?.sessions;
   const srows=(rows||[]).map((s,i)=>{const u=s.usage;const valid=u?.state==='sampled'&&sampleState({at:u.at,state:'ok'})==='live';return [sessionLabels[device.id+'/'+s.id]||'Terminal '+(i+1),valid?Number(u.cpu).toFixed(1)+'%':'—',valid?bytes(u.resident):'—'];});
   table(sessions,['Session','CPU','Resident'],srows.length?srows:[['No measured sessions','—','—']]);
   const processes=[...(snapshot.processes||[])].sort((a,b)=>b.cpu-a.cpu||b.resident-a.resident).slice(0,200);
   table(scroller,['Process · PID','CPU','Resident','Virtual'],processes.map(p=>[p.name+' · '+p.pid,Number(p.cpu).toFixed(1)+'%',bytes(p.resident),bytes(p.virtual_memory)]));
   details.querySelector('summary').textContent='Processes · '+(snapshot.processes?.length||0)+(snapshot.processes?.length>200?' · top 200 by CPU':'');
  }catch{if(!disposed){status.textContent='Device measurements unavailable. Retrying…';cards.replaceChildren();sessions.replaceChildren();scroller.replaceChildren();}}
  finally{if(!disposed)timer=setTimeout(update,5000);}
 }
 update();return ()=>{disposed=true;clearTimeout(timer);};
}
