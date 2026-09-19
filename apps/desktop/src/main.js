import {installTrajectory} from './trajectory.js';
import {ActivityStore} from './activity-store.js';
import {installPlan} from './plan.js';
import {parseVersion,watchVersion,safeToReload} from './version.js';
import {InputController} from './input-controller.js';
import {bindTerminalInput} from './terminal-input-binding.js';
import {orderedResize,framesUnsupported,framePlan} from './render-flow.js';
import {normalizeDevices,normalizeSessions,sessionsPath,sessionPath,LOCAL_ONLY,KIND_GLYPH,STATE_LABEL} from './devices.js';
import {describeView,newViewId,controlIntent,presenceChips,holderName} from './presence.js';
import {probeControl} from './control-state.js';
import {installKeyDock} from './key-dock.js';
import {shellMarkup} from './shell.js';
import { Terminal } from '@xterm/xterm';
import {WebglAddon} from '@xterm/addon-webgl';
import {SessionSignals,LatestResize} from './session-signals.js';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import './style.css';
import {themes, defaults, loadAppearance, applyAppearance} from './appearance.js';
import {HealthRegistry, routeKey, indexShell, stateEvent, stateFields} from './observability.js';
const health=new HealthRegistry();
const signals=new SessionSignals();
let rendererName="dom", resizeTimer, lastGeometry=0, activityAt=0, nextPollAt=0;

const capability = location.hash.slice(1) || sessionStorage.getItem('dot-capability') || '';
if (capability) sessionStorage.setItem('dot-capability', capability);
history.replaceState(null, '', location.pathname);
const $ = s => document.querySelector(s);
$('#app').innerHTML = shellMarkup;
let active = null, generation = 0, sequence = 1, offset = 0, serial = 0, pollRunning = false, disposed = false, selecting = false;
let geometryUncertain=false;
// Per selected session: null = not known yet, true = keeper speaks read_frame, false = legacy sampling.
let frames=null,incarnation='';
// Presence: who is on this session and who is typing. null support = not asked yet, false = older keeper.
const me=describeView(navigator.userAgent,(()=>{try{let v=sessionStorage.getItem('dot-view-id');if(!v){v=newViewId();sessionStorage.setItem('dot-view-id',v);}return v;}catch{return newViewId();}})());
let presence=null,presenceSupported=null,lastTapAt=0,acquiring=null,pendingKeys=[];
let pollIdle=Promise.resolve(), finishPoll=()=>{};
let forceNext = false;
const term = new Terminal({fontFamily:'"SF Mono", Menlo, monospace',fontSize:13, lineHeight:1.25, cursorBlink:true, scrollback:6000, allowProposedApi:false, screenReaderMode:true, theme:{background:'#111519',foreground:'#d4dedc',cursor:'#adf4cf',selectionBackground:'#35554e',black:'#131c22',red:'#ef8f87',green:'#adf4cf',yellow:'#ead9a0',blue:'#92bce6',magenta:'#c8a6e3',cyan:'#95d7d8',white:'#e7eee8'}});
const fit = new FitAddon(); term.loadAddon(fit);
let opened = false, lastIterm = 0, lastItermScreen = null;
let appearance=applyAppearance(loadAppearance(localStorage),term,localStorage);
const copyIndex=indexShell($('#app'));
const activity=new ActivityStore();let controlSeen=false;
installTrajectory({workspace:$('#workspace'),tabs:$('.view-tabs'),button:$('#activity'),store:activity,focusTerminal:()=>{if(opened)term.focus();}});
term.onResize(({cols,rows})=>activity.mark('resize',{cols,rows}));
installPlan($('#plan'));
$('#terminal').addEventListener('pointerdown',()=>{if(active&&!generation)tapControl();});
setInterval(()=>{if(!document.hidden)hello();},2500);
// Version and refresh. The label is the build this view is RUNNING; a different build on disk turns
// the button into an update notice. Auto-reload only when nobody is typing here; sessions outlive views.
const uiVersion=(()=>{try{return parseVersion(__DOT_UI_VERSION__);}catch{return {build:'dev',commit:'',builtAt:''};}})();let updateReady=null;
const versionLabel=()=>{const b=$('#version');$('#version-text').textContent=updateReady?'New version ready · reload':'build '+uiVersion.build;b.dataset.state=updateReady?'update':'current';b.title=updateReady?'Running '+uiVersion.build+' · available '+updateReady.build:'Running build '+uiVersion.build+(uiVersion.builtAt?' · built '+new Date(uiVersion.builtAt).toLocaleString():'')+' · click to reload this view';};
const reloadView=async()=>{try{await release();}catch{/* the keeper fences a lost lease anyway */}location.reload();};
const reloadIfIdle=()=>{if(updateReady&&safeToReload({controlHeld:!!generation,queuedBytes:input.state().queuedBytes,dialogOpen:!!document.querySelector('dialog[open]')}))reloadView();};
$('#version').onclick=reloadView;versionLabel();
if(uiVersion.build!=='dev')watchVersion({current:uiVersion,load:()=>fetch('version.json',{cache:'no-store'}).then(r=>{if(!r.ok)throw new Error('version unavailable');return r.json();}),onUpdate:next=>{updateReady=next;versionLabel();status('A newer interface is ready · it loads when you are not typing');setTimeout(reloadIfIdle,3000);},paused:()=>document.hidden});
setInterval(reloadIfIdle,5000);
$('#menu').onclick=()=>{const shown=$('#app').classList.toggle('show-sessions');$('#menu').setAttribute('aria-expanded',String(shown));};
function status(s) { if(controlSeen!==!!generation){controlSeen=!!generation;activity.mark('control',{state:controlSeen?'taken':'ended'});}$('#state').textContent=s;$('#control').disabled=!!generation;$('#detach').disabled=!generation;const badge=$('#input-state');if(badge&&!generation){badge.textContent='VIEW ONLY';badge.dataset.state='view-only';}else if(badge&&badge.dataset.state==='view-only'){badge.textContent='INPUT · YOURS';badge.dataset.state='idle';} }
async function api(path, data) {
 const finish=health.begin(routeKey(path,data));
 try { const r=await fetch('/api/'+path,{method:data===undefined?'GET':'POST',headers:{Authorization:'Bearer '+capability,'Content-Type':'application/json'},body:data===undefined?undefined:JSON.stringify(data),signal:AbortSignal.timeout(6000)});
 if(!r.ok){const e=new Error(await r.text());e.status=r.status;throw e;}
 const v=await r.json(); if(v.type==='error'||v.error){const e=new Error(v.message||v.error);e.code=v.type!=='error'?'keeper-error':v.message==='stale controller generation'?'controller-fenced':String(v.message).startsWith('controller busy')?'controller-busy':'keeper-error';throw e;} finish(true);return v;
 }catch(error){finish(false);throw error;}
}
const deviceOf=new Map(); // session id -> device id, filled by refresh()
function operation(id, op) {return api(sessionPath(deviceOf.get(id)||'local',id),op);}
function showError(e){status(e.message||String(e));}
function reveal(){if(!opened){$('#welcome').remove();term.open($('#terminal'));opened=true;try{const gpu=new WebglAddon();gpu.onContextLoss(()=>{gpu.dispose();rendererName='dom';});term.loadAddon(gpu);rendererName='webgl';}catch{rendererName='dom';}fit.fit();}term.focus();}
async function release(){const old=active,g=generation;generation=0;input.reset('released');if(old?.kind==='dot'&&g)await operation(old.id,{type:'release',generation:g});status('Viewing · input released');}
async function select(item){
 const own=++serial;selecting=true;
 try {
  try{await release();}catch(e){if(own===serial)showError(e);}
  if(own!==serial)return;
  active=null;input.reset('view-changed');forceNext=false;$('#control').textContent='Take control';lastItermScreen=null;
  $('#app').classList.remove('show-sessions');$('#menu').setAttribute('aria-expanded','false');
  generation=0;sequence=1;reveal();await write('');if(own!==serial)return;
  term.reset();offset=0;signals.reset(item.kind);lastGeometry=0;frames=null;incarnation='';presence=null;presenceSupported=null;pendingKeys=[];active=item;controlSeen=false;activity.bind(item);
  $('#title').textContent=item.name;$('#mode').textContent=item.kind==='dot'?'DOT · SHARED PTY':'ITERM · SCREEN BRIDGE';
  $('#details').textContent=item.kind==='dot'?'Session '+item.id.slice(0,8)+(item.device&&item.device!=='local'?' · shell runs on '+item.name.split(' / ')[0]+' · reached through this device':' · shell stays on this device'):'iTerm owns this shell · screen projection is text-only';
  status('Viewing · take control to type');
  document.querySelectorAll('nav button').forEach(b=>b.classList.toggle('selected',b.dataset.id===item.id));
  if(item.kind==='dot'){
   const r=await operation(item.id,{type:'status'});if(own!==serial)return;$('#details').textContent+=' · PID '+r.pid;
   const screen=await operation(item.id,{type:'screen'});if(own!==serial)return;term.resize(screen.cols,screen.rows);
  }

 }catch(e){if(own===serial)showError(e);}finally{if(own===serial)selecting=false;}
}
async function refresh(){
 // Devices first, then each connected device's sessions. A backend without a catalog is one local device.
 let devices;try{devices=normalizeDevices(await api('devices'));}catch(e){if(e.status!==404&&e.status!==405)throw e;devices=LOCAL_ONLY;}
 const lists=await Promise.all(devices.map(async d=>{if(d.state!=='connected')return [];try{const v=await api(sessionsPath(d.id));if(d.local){$('#new').disabled=v.can_create===false;const start=$('#start');if(start)start.disabled=v.can_create===false;}return normalizeSessions(v);}catch{d.state='offline';return [];}}));
 const local=devices.find(d=>d.local);if(local&&local.name!=='This device')me.label=local.name+' · '+(me.kind==='app'?'app':me.kind==='phone'?'phone':me.browser||'browser');
 deviceOf.clear();const nav=$('#sessions');nav.replaceChildren();
 devices.forEach((d,i)=>{
  const group=document.createElement('section');group.className='device';group.dataset.state=d.state;group.dataset.kind=d.kind;
  const head=document.createElement('div');head.className='device-head';const name=document.createElement('span');name.className='device-name';name.textContent=KIND_GLYPH[d.kind]+' '+d.name;
  const state=document.createElement('small');state.textContent=d.local?'here':STATE_LABEL[d.state];state.title=d.local?'This device':STATE_LABEL[d.state];name.title=d.name;head.append(name,state);
  if(d.canCreate&&d.state==='connected'){const add=document.createElement('button');add.className='device-add';add.textContent='+';add.setAttribute('aria-label','New terminal on '+d.name);add.title='New terminal on '+d.name;add.onclick=()=>create(d.id);head.append(add);}
  group.append(head);
  for(const s of lists[i]){deviceOf.set(s.id,d.id);const b=document.createElement('button');b.dataset.id=s.id;b.className='session'+(active?.id===s.id?' selected':'');b.textContent=(s.exited?'○ ':'›_ ')+s.id.slice(0,8);const small=document.createElement('small');small.textContent=s.exited?'Ended':'Running';b.append(small);b.onclick=()=>select({kind:'dot',id:s.id,device:d.id,name:d.name+' / '+s.id.slice(0,8)});group.append(b);}
  if(!lists[i].length){const empty=document.createElement('p');empty.className='device-empty';empty.textContent=d.state==='connected'?'No sessions':d.state==='offline'?'Not reachable right now':'This device did not accept our key';group.append(empty);}
  nav.append(group);
 });
}
async function create(device='local'){if(typeof device!=='string')device='local';try{const s=await api(sessionsPath(device),{});await refresh();await select({kind:'dot',id:s.session,device,name:'Terminal / '+s.session.slice(0,8)});if(active?.id===s.session)await control();}catch(e){showError(e);}}
function renderPresence(){
 const box=$('#presence');if(!box)return;box.replaceChildren();
 if(presenceSupported===false){const c=document.createElement('span');c.className='chip';c.textContent='who is here: unknown (older session)';box.append(c);return;}
 for(const chip of presenceChips(presence,me.view)){const c=document.createElement('span');c.className='chip';c.dataset.kind=chip.kind;if(chip.typing)c.dataset.typing='true';if(chip.you)c.dataset.you='true';c.textContent=chip.label+(chip.you?' (you)':'');c.title=chip.typing?chip.label+' has input control':chip.label+' is watching';box.append(c);}
}
async function hello(){
 if(!active||active.kind!=='dot'||disposed||presenceSupported===false){renderPresence();return;}
 const target=active,epoch=serial;
 try{const r=await operation(target.id,{type:'hello',view:me.view,label:me.label,kind:me.kind});if(epoch!==serial)return;presenceSupported=true;presence=r;}
 catch(e){if(epoch!==serial)return;if(framesUnsupported(e)){presenceSupported=false;presence=null;}}
 renderPresence();
}
async function tapControl({viaKey=false}={}){
 if(!active)return false;if(generation)return true;if(acquiring)return acquiring;
 acquiring=(async()=>{
  if(active.kind==='dot')await hello();
  const intent=controlIntent({presence,self:me.view,held:!!generation,lastTapAgoMs:viaKey?Infinity:Date.now()-lastTapAt});
  if(!viaKey)lastTapAt=Date.now();
  if(intent==='confirm'){status(holderName(presence,me.view)+' is typing · '+(viaKey?'tap the terminal twice to take over':'tap again to take over'));return false;}
  forceNext=intent==='takeover';await control();return !!generation;
 })().finally(()=>{acquiring=null;});
 return acquiring;
}
async function control(){
 if(!active||generation)return;
 const target=active, epoch=serial;
 try{
  if(target.kind==='dot'){
   const r=await operation(target.id,presenceSupported?{type:'acquire_as',view:me.view,takeover:forceNext}:{type:'acquire',takeover:forceNext});
   if(epoch!==serial){await operation(target.id,{type:'release',generation:r.generation});return;}
   generation=r.generation;sequence=1;await resize();
  }else generation=1;
  if(epoch!==serial)return;
  forceNext=false;$('#control').textContent='Take control';status('You have input control');term.focus();hello();
 }catch(e){if(epoch!==serial)return;forceNext=true;$('#control').textContent='Take over input';showError(e);}
}
const resizes=new LatestResize(async v=>{
 if(v.epoch!==serial||v.generation!==generation||!generation)return;
 const start=performance.now();
 const applied=await orderedResize({drain:async()=>{await pollIdle;await write('');},
  isCurrent:()=>v.epoch===serial&&v.generation===generation&&generation!==0,
  prepareGrid:()=>{geometryUncertain=true;term.resize(v.cols,v.rows);},
  recover:async()=>{const screen=await operation(v.id,{type:"screen"});if(v.epoch===serial){term.resize(screen.cols,screen.rows);geometryUncertain=false;}},
  send:()=>operation(v.id,{type:'resize',generation:v.generation,cols:v.cols,rows:v.rows})});
 if(applied)geometryUncertain=false;
 if(applied)signals.sample('resize',performance.now()-start);
},(error,value)=>{if(value.epoch===serial)showError(error);});
async function resize(){
 if(!opened)return;
 // A viewer follows the host grid and scrolls. Only the controller changes it.
 if(active?.kind==='dot'){
  if(!generation)return;
  const d=fit.proposeDimensions();if(!d)return;
  await resizes.request({id:active.id,epoch:serial,generation,cols:Math.max(2,Math.min(240,d.cols)),rows:Math.max(1,Math.min(100,d.rows))});
 }else fit.fit();
}
const observer=new ResizeObserver(()=>{clearTimeout(resizeTimer);resizeTimer=setTimeout(()=>resize().catch(showError),120);});observer.observe($('#terminal'));
// One ordered, fenced input path. See input-controller.js for what it promises.
const inputLabels={'view-only':'Read-only view · choose Take control to type','too-large':'Too large to send at once · nothing was sent','busy':'Session busy · that input was not sent · try again','fenced':'Control changed · view only','unknown-outcome':'Input acknowledgement lost. Inspect the screen, then take control again; input was not retried.'};
const input=new InputController({
 canSend:()=>!!active&&!!generation,
 send:async bytes=>{
  const target=active,g=generation,start=performance.now();activityAt=Date.now();nextPollAt=0;
  // Control can be lost between queueing and sending (a gap, a takeover): those bytes must not go.
  if(!target||!g)throw Object.assign(new Error('input control is not held'),{code:'controller-fenced'});
  if(target.kind==='dot'){const n=sequence;await operation(target.id,{type:'input',generation:g,sequence:n,data:Array.from(bytes)});if(generation===g)sequence=n+1;}
  else await api('iterm',{action:'input',id:target.id,text:new TextDecoder().decode(bytes)});
  signals.sample('input',performance.now()-start);
 },
 onState:state=>{
  $('#input-state').textContent={idle:generation?'INPUT · YOURS':'VIEW ONLY',sending:'INPUT · SENDING',queued:'INPUT · QUEUED '+state.queuedBytes+' B',uncertain:'INPUT · CHECK SCREEN'}[state.condition];
  $('#input-state').dataset.state=generation?state.condition:'view-only';
  if(state.condition==='uncertain')activity.mark('input-stopped',{reason:state.refusal});if(state.refusal==='fenced'||state.refusal==='unknown-outcome'){generation=0;signals.fail();}
  if(state.refusal)status(inputLabels[state.refusal]||state.refusal);
 }});
// Typing or tapping in the terminal IS asking for control. Keys pressed while control is being
// acquired were never sent, so delivering them afterwards is not a replay. A key never confirms a
// takeover from someone who is typing; only a deliberate second tap does.
const sendInput=text=>{
 if(generation||!active)return input.submit(text);
 if(pendingKeys.length<64)pendingKeys.push(text);
 if(pendingKeys.length===1)tapControl({viaKey:true}).then(ok=>{const keys=pendingKeys;pendingKeys=[];if(ok)for(const k of keys)input.submit(k);});
 return true;
};
bindTerminalInput({submit:sendInput,term,surface:$('#terminal'),controller:input,canDrop:()=>!!active&&!!generation,notify:status});
function write(data){return new Promise(resolve=>term.write(data,resolve));}
async function poll(){
 if(pollRunning||resizes.running||selecting||!active||disposed||document.hidden||Date.now()<nextPollAt)return;
 nextPollAt=Date.now()+(Date.now()-activityAt<1500?32:250);
 if(active.kind==='iterm'&&Date.now()-lastIterm<500)return;pollRunning=true;pollIdle=new Promise(resolve=>{finishPoll=resolve;});const target=active,epoch=serial;
 try {
  if(target.kind==='dot') {
   const start=performance.now();let r;
   if(frames!==false){
    try{r=await operation(target.id,{type:'read_frame',after:offset});if(epoch!==serial)return;frames=true;}
    catch(e){if(epoch!==serial)return;if(frames===true||!framesUnsupported(e))throw e;frames=false;signals.note?.('legacy-geometry');}
   }
   if(frames===false){r=await operation(target.id,{type:'read',after:offset});if(epoch!==serial)return;}
   if(frames){
    const plan=framePlan({frame:r,knownIncarnation:incarnation,controller:!!generation,cols:term.cols,rows:term.rows});
    if(plan.restart){incarnation=r.incarnation;offset=0;generation=0;term.reset();signals.reset(target.kind);activity.mark('gap');status('Session stream restarted · replaying');return;}
    incarnation=r.incarnation;if(plan.resize)term.resize(plan.resize.cols,plan.resize.rows);
   }
   signals.sample('read',performance.now()-start);if(r.data.length){activityAt=Date.now();nextPollAt=0;activity.output(r.data.length);}if(r.gap)activity.mark('gap');signals.receive(r.next,r.gap);
   const geometryDue=Date.now()-lastGeometry>1000;
   if(geometryDue&&generation){
    const checked=generation;const result=await probeControl(g=>operation(target.id,{type:'check_control',generation:g}),checked);
    if(epoch!==serial)return;
    if(generation===checked){if(result.state==='fenced'){generation=0;status('Control changed · view only');}
     else if(result.state==='unconfirmed')status('Connection uncertain · control check will retry');}
   }
   // Legacy keepers cannot label byte chunks with geometry. Sample BEFORE applying
   // output, never after a redraw has already been parsed using the old grid.
   let screen;
   if(r.gap||(!frames&&(geometryUncertain||(!generation&&(r.data.length||geometryDue))))){
    screen=await operation(target.id,{type:'screen'});if(epoch!==serial)return;
    if(term.cols!==screen.cols||term.rows!==screen.rows)term.resize(screen.cols,screen.rows);
    geometryUncertain=false;
   }
   if(geometryDue)lastGeometry=Date.now();
   const parseStart=performance.now();
   if(r.gap){generation=0;term.reset();await write('\r\n[Output history limit reached. Take control after checking the current screen.]\r\n');if(epoch!==serial)return;await write(screen.lines.join('\r\n'));if(epoch!==serial)return;offset=r.next;status('History gap · current text snapshot shown');}
   else {await write(new Uint8Array(r.data));if(epoch!==serial)return;offset=r.next;}
   signals.apply(offset);signals.sample('parse',performance.now()-parseStart);
   if(r.exited)activity.mark('exited');if(r.exited)status('Shell exited · output remains available');
  } else {
   lastIterm=Date.now();const r=await api('iterm',{action:'screen',id:target.id});if(epoch!==serial)return;
   // External application screen text must never be interpreted as escape commands.
   const safe=r.lines.map(l=>l.replace(/[\x00-\x1f\x7f-\x9f]/g,''));
   const text=safe.join('\r\n')+'\x1b['+(Math.max(0,Math.min(term.rows-1,r.cursor_row||0))+1)+';'+(Math.max(0,Math.min(term.cols-1,r.cursor_col||0))+1)+'H';
   if(text!==lastItermScreen){lastItermScreen=text;await write('\x1b[H\x1b[2J'+text);}
  }
 }catch(e){if(epoch===serial){signals.fail();showError(e);}}finally{pollRunning=false;finishPoll();}
}
setInterval(poll,32);
$('#new').onclick=create;$('#start').onclick=create;$('#refresh').onclick=()=>refresh().catch(showError);$('#control').onclick=control;$('#detach').onclick=()=>release().catch(showError);
$('#iterm').onclick=async()=>{try{const r=await api('iterm',{action:'list'});$('#iterm-list').replaceChildren();for(const s of r.sessions){const b=document.createElement('button');b.textContent=s.name||'iTerm session';b.dataset.id=s.id;b.onclick=()=>select({kind:'iterm',id:s.id,name:s.name||'iTerm session'});$('#iterm-list').append(b);}status(r.sessions.length+' iTerm sessions available');}catch(e){showError(e);}};
$('#browser').onclick=()=>window.open(location.origin+'/#'+capability,'_blank','noopener,noreferrer');
window.addEventListener('keydown',e=>{if(e.metaKey&&!e.ctrlKey&&!e.altKey&&e.key==='n'&&!document.querySelector('dialog[open]')){e.preventDefault();create();}});
window.addEventListener('pagehide',()=>{disposed=true;if(active?.kind==='dot'&&generation)fetch('/api/sessions/'+active.id,{method:'POST',headers:{Authorization:'Bearer '+capability,'Content-Type':'application/json'},body:JSON.stringify({type:'release',generation}),keepalive:true}).catch(()=>{});});
status('Choose a session or start a new shell');
refresh().catch(showError);

// Owner tools use the same authenticated loopback boundary as terminal operations.
const tools=document.createElement('div');tools.className='owner-tools';
tools.innerHTML='<div class="section">OWNER TOOLS</div><button id="resources">◷ Resources</button><button id="vault">◇ Vault & audit</button>';
$('aside').insertBefore(tools,$('.bottom'));
const panel=document.createElement('dialog');panel.id='owner-panel';document.body.append(panel);
function panelBase(title){clearInterval(auditTimer);clearInterval(systemTimer);panel.replaceChildren();const top=document.createElement('div');top.className='panel-top';const h=document.createElement('h2');h.textContent=title;const close=document.createElement('button');close.textContent='Close';close.onclick=()=>panel.close();top.append(h,close);panel.append(top);if(!panel.open)panel.showModal();}
function paragraph(text){const p=document.createElement('p');p.textContent=text;panel.append(p);return p;}
const fmtBytes=n=>(n/1024/1024/1024).toFixed(1)+' GB';
$('#resources').onclick=async()=>{panelBase('Your machine, in view');paragraph('Read-only measurements from the AXXIS Resource Manager collector. CPU is summed across cores; energy and traffic enforcement are not implemented.');try{const r=await api('resources');if(!r.groups){paragraph('Measurements are starting. Reopen this panel in a few seconds.');return;}paragraph('CPU '+Number(r.cpu).toFixed(1)+'% · Memory '+fmtBytes(r.memory_used)+' / '+fmtBytes(r.memory_total)+' · Swap '+fmtBytes(r.swap_used));const table=document.createElement('table');for(const g of [...r.groups].sort((a,b)=>b.cpu-a.cpu).slice(0,20)){const tr=document.createElement('tr');for(const value of [g.name,Number(g.cpu).toFixed(1)+'% CPU',(g.footprint==null||!g.measured)?'Footprint unavailable':fmtBytes(g.footprint),g.count+' processes']){const td=document.createElement('td');td.textContent=value;tr.append(td);}table.append(tr);}panel.append(table);paragraph('Sample time: '+new Date(r.at*1000).toLocaleTimeString());}catch(e){paragraph(e.message);}};
let auditTimer;
async function vaultPanel(){panelBase('Vault & provenance');paragraph('Encrypted at rest · master key in macOS Keychain. Secret values are never returned by the list API.');
 try{const v=await api('vault',{action:'list'});
 paragraph(v.limitation);
 const form=document.createElement('form');form.className='vault-form';form.innerHTML='<label>Environment name<input name="name" placeholder="MY_API_KEY" pattern="[A-Za-z0-9_]+" maxlength="128" required autocomplete="off"></label><label>Secret value<input name="value" type="password" required autocomplete="new-password"></label><button>Store encrypted</button>';
 form.onsubmit=async e=>{e.preventDefault();const name=form.elements.name.value,value=form.elements.value.value;form.elements.value.value='';try{await api('vault',{action:'put',name,value});await vaultPanel();}catch(err){paragraph(err.message);}};panel.append(form);
 const list=document.createElement('div');list.className='secret-list';for(const name of v.secrets){const row=document.createElement('div');const label=document.createElement('label');const check=document.createElement('input');check.type='checkbox';check.value=name;check.className='secret-choice';label.append(check,document.createTextNode(name));const del=document.createElement('button');del.textContent='Delete';del.onclick=async()=>{try{await api('vault',{action:'delete',name});vaultPanel();}catch(e){paragraph(e.message);}};row.append(label,del);list.append(row);}panel.append(list);
 const launch=document.createElement('form');launch.className='vault-form';launch.innerHTML='<label>Executable (absolute path)<input name="command" placeholder="/bin/zsh" required></label><label>Arguments (one per line)<textarea name="args" autocorrect="off" autocapitalize="off" spellcheck="false"></textarea></label><button>Run with selected secrets</button>';
 launch.onsubmit=async e=>{e.preventDefault();try{const secrets=[...panel.querySelectorAll('.secret-choice:checked')].map(x=>x.value);const args=launch.elements.args.value ? launch.elements.args.value.split('\n') : [];const s=await api('vault',{action:'run',command:launch.elements.command.value,args,secrets});panel.close();await refresh();await select({kind:'dot',id:s.session,name:'Vault process / '+s.session.slice(0,8)});await control();}catch(err){paragraph(err.message);}};panel.append(launch);
 paragraph('Selected secrets become plaintext environment variables in the new keeper and child process. Use only trusted executables.');
 const title=document.createElement('h3');title.textContent='Live audit';panel.append(title);const audit=document.createElement('pre');audit.className='audit';panel.append(audit);
 const render=v=>{audit.textContent=v.audit.slice(-30).reverse().map(e=>new Date(e.at*1000).toLocaleTimeString()+' · '+e.action+' · '+JSON.stringify(e.details)).join('\n');};render(v);
 clearInterval(auditTimer);auditTimer=setInterval(async()=>{if(!panel.open){clearInterval(auditTimer);return;}try{render(await api('vault',{action:'list'}));}catch{}},2000);
 }catch(e){paragraph(e.message);}}
$('#vault').onclick=vaultPanel;
panel.addEventListener('close',()=>clearInterval(auditTimer));

// Settings never contain authentication material. Changes affect this view only.
let systemTimer;
panel.addEventListener('close',()=>clearInterval(systemTimer));
$('#appearance').onclick=()=>{
 panelBase('Make it yours');
 paragraph('Exact sizes, local fonts and a shared color palette for the interface and terminal. Preferences stay on this device.');
 const form=document.createElement('form');form.className='appearance-form';
 form.innerHTML='<label>Theme<select name="theme"></select></label><label>Terminal size (px)<input name="terminalSize" type="number" min="8" max="40" step="0.5" required></label><label>Interface size (px)<input name="uiSize" type="number" min="12" max="24" step="0.5" required></label><label>Line height<input name="lineHeight" type="number" min="1" max="2" step="0.05" required></label><label class="wide">Font family / fallback list<input name="font" maxlength="120" required list="font-options"></label><datalist id="font-options"><option value="SF Mono, Menlo, monospace"><option value="JetBrains Mono, monospace"><option value="Fira Code, monospace"><option value="monospace"></datalist><label class="wide"><span><input name="reducedNoise" type="checkbox"> Reduce decorative text</span></label><div class="settings-preview wide">Aa Bb Cc · 0123456789 · {} [] =&gt;</div><div class="wide"><button type="submit" class="primary">Apply appearance</button> <button type="button" id="reset-appearance">Reset</button></div>';
 for(const [id,t] of Object.entries(themes)){const o=new Option(t.label,id);form.elements.theme.add(o);}
 const fill=()=>{for(const [key,value] of Object.entries(appearance)){const el=form.elements.namedItem(key);if(el.type==='checkbox')el.checked=value;else el.value=value;}};
 fill();form.oninput=()=>{const preview=form.querySelector('.settings-preview');preview.style.fontFamily=form.elements.font.value;preview.style.fontSize=Math.min(40,Math.max(8,Number(form.elements.terminalSize.value)||14))+'px';};
 form.onsubmit=e=>{e.preventDefault();const values=Object.fromEntries(new FormData(form));values.reducedNoise=form.elements.reducedNoise.checked;appearance=applyAppearance(values,term,localStorage);fill();resize().catch(showError);status('Appearance saved on this device');};
 form.querySelector('#reset-appearance').onclick=()=>{appearance=applyAppearance(defaults,term,localStorage);fill();resize().catch(showError);};
 panel.append(form);paragraph('Fonts must be installed on this device; otherwise the fallback is used. No remote font downloads.');
};
$('#system').onclick=()=>{
 panelBase('System · live signals');paragraph('Observed in this view only. Unknown means not called; stale means no response observed for 15 seconds. No terminal text, secret values, user input or API bodies are indexed.');
 const metrics=document.createElement('p');panel.append(metrics);
 const inputMetrics=document.createElement('p');inputMetrics.id='input-metrics';panel.append(inputMetrics);
 const table=document.createElement('table');table.className='health-table';panel.append(table);
 const renderInput=()=>{const d=input.snapshot();inputMetrics.textContent='Input (counts and timings only): '+d.condition+' · held-key repeats seen: '+d.keyRepeatsSeen+' · submissions: '+d.submissions+' · requests: '+d.requests+' · gap between keys p50/p95: '+(d.arrivalGapMs.p50??'—')+'/'+(d.arrivalGapMs.p95??'—')+' ms · request p50/p95: '+(d.requestRttMs.p50??'—')+'/'+(d.requestRttMs.p95??'—')+' ms · bytes per request p95: '+(d.bytesPerRequest.p95??'—')+' · busy retries: '+d.busyRetries+' · refused: '+d.refused+' · unknown outcomes: '+d.unknownOutcomes+' · discarded bytes: '+d.discardedBytes;};
 const render=()=>{const sync=signals.snapshot();metrics.textContent='Sync: '+sync.state+' · Response age: '+(sync.responseAgeMs??'unknown')+' ms · Pending parse: '+sync.pendingParseBytes+' bytes · Read p50/p95: '+sync.latency.read.p50+'/'+sync.latency.read.p95+' ms · Input ACK p95: '+sync.latency.input.p95+' ms · Renderer: '+rendererName+' · Peer views: unknown · View: '+(active?.kind||'welcome')+' · Control: '+(generation?'held':'view only')+' · Input queue: '+input.state().queuedBytes+' bytes · Poll: '+(pollRunning?'in flight':'idle');table.replaceChildren();const head=document.createElement('tr');for(const label of ['API','State','Latency','Calls / errors']){const cell=document.createElement('th');cell.textContent=label;head.append(cell);}table.append(head);for(const r of health.snapshot()){const tr=document.createElement('tr');tr.dataset.health=r.state;for(const value of [r.id,r.state+(r.inflight?' · busy':''),r.last?r.latency+' ms':'—',r.calls+' / '+r.failures]){const td=document.createElement('td');td.textContent=value;tr.append(td);}table.append(tr);}};
 const renderAll=()=>{for(const draw of [renderInput,render]){try{draw();}catch(error){showError(error);}}};
 renderAll();clearInterval(systemTimer);systemTimer=setInterval(()=>{if(!panel.open||document.hidden)return;renderAll();},1000);
 const label=document.createElement('label');label.textContent='Search interface index';const search=document.createElement('input');search.type='search';search.placeholder='Try “terminal”, “primary”, or “dynamic”';label.append(search);panel.append(label);
 const results=document.createElement('pre');results.className='copy-index';panel.append(results);
 const show=()=>{const q=search.value.toLowerCase();results.textContent=copyIndex.filter(e=>[e.text,e.id,e.kind,e.role,e.signal].join(' ').toLowerCase().includes(q)).map(e=>e.id+' · '+e.kind+' / '+e.role+' / '+e.signal+'\n'+e.text).join('\n\n');};search.oninput=show;show();
 paragraph('Mapped state: '+stateFields.map(f=>f.id+' ('+f.type+(f.unit?', '+f.unit:'')+')').join(' · '));
 paragraph('Coverage: initial shell copy and seven dynamic slots. Owner-tool dialog content, terminal output and arbitrary program variables are excluded. This is a local registry foundation, not whole-system tracing.');
};

// Local subscribers may collaborate on operational state, never authentication or content.
setInterval(()=>{if(!disposed&&!document.hidden)window.dispatchEvent(new CustomEvent('dot:state',{detail:stateEvent({kind:active?.kind,controlHeld:!!generation,queuedBytes:input.state().queuedBytes,pollRunning},health)}));},1000);

$('#sync').onclick=()=>$('#system').click();
setInterval(()=>{if(disposed)return;const s=signals.snapshot();$('#sync').textContent='Sync · '+(document.hidden?'paused':({'measurement-error':'measurement unavailable','unknown':'waiting','error':'check connection','stale':'stale','history-gap':'history missing','catching-up':'updating','caught-up-to-response':'current view'}[s.state]));if(s.historyGaps&&s.state!=='history-gap')$('#sync').textContent+=' · history missing';$('#sync').dataset.state=s.state;
 if(!document.hidden)window.dispatchEvent(new CustomEvent('dot:session-state',{detail:s}));},1000);
installKeyDock($('main'),term,sendInput,copyIndex);
