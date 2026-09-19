// The Activity pane. Live rows come from an ActivityStore (sizes and times this view observed);
// agent rows come only from an explicit local snapshot import. Nothing is uploaded or persisted.
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
const WIDTH_KEY='dot.activity.width',OPEN_KEY='dot.activity.open',MIN_WIDTH=240,MAX_WIDTH=560,DEFAULT_WIDTH=320;
const recall=(key,fallback)=>{try{return localStorage.getItem(key)??fallback;}catch{return fallback;}};
const remember=(key,value)=>{try{localStorage.setItem(key,value);}catch{/* private window: the pane still works */}};
const size=n=>n<1024?n+' B':n<1048576?(n/1024).toFixed(n<10240?1:0)+' KB':(n/1048576).toFixed(1)+' MB';
const span=ms=>{const s=Math.max(0,Math.round(ms/1000));return s<60?s+' s':s<3600?Math.floor(s/60)+' min':Math.floor(s/3600)+' h '+Math.floor(s%3600/60)+' min';};
export const ago=(ms)=>ms<2000?'now':span(ms)+' ago';
/** Fixed labels only. Nothing from a terminal or a snapshot is ever rendered as markup. */
export function describe(e,now){
 switch(e.kind){
  case 'output':return [e.open?'Output · streaming':'Output',size(e.bytes)+' in '+e.reads+(e.reads===1?' read':' reads')+' over '+span(e.lastAt-e.atMs)+'. Sizes and times only; the text is in the terminal, not here.'];
  case 'resize':return ['Grid '+e.cols+' × '+e.rows,'The terminal grid this view applied.'];
  case 'control':return [e.state==='taken'?'You took input control':'Input control ended','Control is fenced by the keeper. Only one view can type at a time.'];
  case 'input-stopped':return ['Input stopped · '+(e.reason==='unknown-outcome'?'delivery unknown':e.reason),'Nothing was replayed. Check the screen before typing again.'];
  case 'gap':return ['Output history missed','This view fell behind the keeper’s retained history. The screen was reset to the current state.'];
  case 'attached':return ['View attached','Observation of this PTY starts here. Earlier activity was not seen by this view.'];
  case 'exited':return ['Session exited','The process in this PTY ended.'];
  case 'compact':return ['Context compacted',`${e.before?.toLocaleString()??'?'} → ${e.after?.toLocaleString()??'?'} tokens${e.durationMs!==undefined?' · '+Math.round(e.durationMs/1000)+'s':''}`];
  case 'input':return ['Input recorded','Message contents excluded. This marker may include harness-injected input.'];
  default:return [`${e.category} ×${e.count} · ${e.state==='returned'?'returned':e.state==='error'?'error':'no result recorded'}`,`${e.batchSteps?e.batchSteps+' explicit batch steps. ':''}Tool return does not prove the intended change succeeded. Raw arguments and output are excluded.`];
 }
}
/**
 * The Activity pane: a sibling of the terminal inside `workspace`, never on top of it.
 * `store` is an ActivityStore. Returns {toggle, dispose}.
 */
export function installTrajectory({workspace,tabs,button,store,focusTerminal=()=>{}}) {
 let page=0,drawn=-1;const limit=60,open=new Set(),narrow=matchMedia('(max-width:680px)');
 const splitter=document.createElement('div');splitter.id='splitter';splitter.hidden=true;splitter.tabIndex=0;
 for(const [k,v] of Object.entries({role:'separator','aria-orientation':'vertical','aria-label':'Resize activity pane','aria-controls':'trajectory','aria-valuemin':MIN_WIDTH,'aria-valuemax':MAX_WIDTH}))splitter.setAttribute(k,v);
 const panel=document.createElement('section');panel.id='trajectory';panel.dataset.state='empty';panel.hidden=true;panel.setAttribute('aria-label','Session activity');
 panel.innerHTML='<div class="trajectory-head"><strong data-copy-id="trajectory.title">Activity</strong><button aria-label="Close activity">×</button></div><p class="trajectory-live" role="status"><span class="dot" aria-hidden="true"></span><span class="live-text"></span></p><div class="trajectory-filters" data-copy-id="trajectory.filter"><label><input type="checkbox" id="activity-attention"> Needs attention only</label></div><ol aria-label="Newest first"></ol><button class="trajectory-more" data-copy-id="trajectory.earlier">Show earlier</button><details class="trajectory-snapshot"><summary data-copy-id="trajectory.import">Add an agent snapshot</summary><label>Import activity snapshot<input type="file" id="activity-import" accept="application/json,.json"></label><p class="trajectory-note">A snapshot is a file you chose. It is not bound to this PTY and does not update.</p><output aria-live="polite"></output></details>';
 workspace.append(splitter,panel);
 const live=panel.querySelector('.trajectory-live'),liveText=panel.querySelector('.live-text'),output=panel.querySelector('output'),list=panel.querySelector('ol'),filter=panel.querySelector('#activity-attention'),more=panel.querySelector('.trajectory-more');
 const width=value=>{const room=Math.max(MIN_WIDTH,Math.floor(workspace.clientWidth/2)||MAX_WIDTH);const w=Math.round(Math.min(MAX_WIDTH,room,Math.max(MIN_WIDTH,Number(value)||DEFAULT_WIDTH)));workspace.style.setProperty('--activity-width',w+'px');splitter.setAttribute('aria-valuenow',w);return w;};
 let current=width(recall(WIDTH_KEY,DEFAULT_WIDTH));
 const view=name=>{workspace.dataset.view=name;for(const tab of tabs.querySelectorAll('[role=tab]')){const on=tab.dataset.view===name;tab.setAttribute('aria-selected',String(on));tab.tabIndex=on?0:-1;}};
 function toggle(show,{user=true}={}){
  const hadFocus=panel.contains(document.activeElement)||splitter===document.activeElement;
  panel.hidden=splitter.hidden=!show;tabs.hidden=!show;button.setAttribute('aria-expanded',String(show));workspace.classList.toggle('trajectory-open',show);
  view(show&&narrow.matches?'activity':'terminal');if(user)remember(OPEN_KEY,show?'1':'0');
  if(show){current=width(current);draw(true);}else if(hadFocus)focusTerminal(); // focus goes back only if it was ours
 }
 button.onclick=()=>toggle(panel.hidden);panel.querySelector('.trajectory-head button').onclick=()=>toggle(false);
 for(const tab of tabs.querySelectorAll('[role=tab]'))tab.onclick=()=>{view(tab.dataset.view);if(tab.dataset.view==='terminal')focusTerminal();};
 tabs.onkeydown=e=>{if(e.key!=='ArrowLeft'&&e.key!=='ArrowRight')return;e.preventDefault();const next=workspace.dataset.view==='terminal'?'activity':'terminal';view(next);tabs.querySelector('[aria-selected=true]').focus();};
 const onNarrow=()=>{if(!panel.hidden)view(narrow.matches?workspace.dataset.view:'terminal');};narrow.addEventListener('change',onNarrow);
 // Splitter: drag, arrow keys, Home/End, double-click to reset. Width is clamped and remembered.
 const commit=value=>{current=width(value);remember(WIDTH_KEY,String(current));};
 splitter.onpointerdown=e=>{e.preventDefault();splitter.setPointerCapture(e.pointerId);const right=workspace.getBoundingClientRect().right;splitter.onpointermove=m=>{current=width(right-m.clientX);};splitter.onpointerup=()=>{splitter.onpointermove=splitter.onpointerup=null;commit(current);};};
 splitter.ondblclick=()=>commit(DEFAULT_WIDTH);
 splitter.onkeydown=e=>{const step={ArrowLeft:16,ArrowRight:-16}[e.key];if(step)commit(current+step);else if(e.key==='Home')commit(MAX_WIDTH);else if(e.key==='End')commit(MIN_WIDTH);else if(e.key==='Enter')commit(DEFAULT_WIDTH);else return;e.preventDefault();};
 function header(){
  const s=store.summary();live.dataset.state=s.state;
  liveText.textContent=s.state==='unbound'?'No session selected':s.state==='waiting'?'Live · attached · no output seen yet':s.state==='streaming'?'Live · streaming '+span(s.forMs)+' · '+size(s.bytes):'Live · quiet for '+span(s.forMs);
 }
 function draw(force){
  if(panel.hidden)return;header();const now=Date.now();
  if(!force&&drawn===store.version){for(const t of list.querySelectorAll('time'))t.textContent=ago(now-Number(t.dataset.at));return;}
  drawn=store.version;const all=store.entries({attention:filter.checked}),shown=all.slice(0,(page+1)*limit);list.replaceChildren();
  for(const e of shown){const li=document.createElement('li');li.dataset.state=e.state||e.kind;li.dataset.source=e.source;if(e.open)li.dataset.open='true';
   const details=document.createElement('details'),summary=document.createElement('summary'),time=document.createElement('time'),[label,detail]=describe(e,now);
   details.open=open.has(e.key);details.ontoggle=()=>{if(details.open)open.add(e.key);else open.delete(e.key);};
   time.dataset.at=e.lastAt??e.atMs;time.dateTime=new Date(e.atMs).toISOString();time.title=new Date(e.atMs).toLocaleString();time.textContent=ago(now-(e.lastAt??e.atMs));
   summary.append(time,document.createTextNode(label));if(e.source==='snapshot'){const tag=document.createElement('span');tag.className='tag';tag.textContent='snapshot';summary.append(tag);}
   const p=document.createElement('p');p.textContent=detail;details.append(summary,p);li.append(details);list.append(li);}
  panel.dataset.state=all.length?'loaded':'empty';more.hidden=shown.length===all.length;
 }
 filter.onchange=()=>{page=0;draw(true);};more.onclick=()=>{page++;draw(true);};
 panel.querySelector('#activity-import').onchange=async ev=>{try{const file=ev.target.files[0];if(!file)return;if(file.size>12000000)throw new Error('Snapshot exceeds 12 MB');const events=validateTrajectory(JSON.parse(await file.text()));store.importSnapshot(groupTrajectory(events));page=0;output.textContent=`${events.filter(e=>e.kind==='tool').length.toLocaleString()} tools · ${events.filter(e=>e.kind==='compact').length} compactions · ${events.filter(e=>e.state==='error').length} errors`;}catch(e){output.textContent=e.message;}finally{ev.target.value='';}};
 // A streaming PTY changes the store on every read; the pane redraws at most four times a second.
 let pending=0;const previous=store.onChange;store.onChange=()=>{previous();if(!pending)pending=setTimeout(()=>{pending=0;draw(false);},250);};
 const timer=setInterval(()=>{store.tick();draw(false);},1000);
 if(recall(OPEN_KEY,'0')==='1')toggle(true,{user:false});else view('terminal');
 return {toggle,dispose(){clearInterval(timer);clearTimeout(pending);narrow.removeEventListener('change',onNarrow);store.onChange=previous;splitter.remove();panel.remove();}};
}
