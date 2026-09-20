import {mountDeviceResources} from './device-resources.js';
import {createInputGate} from './input-gate.js';
import {createTransport} from './transport.js';
import {installTrajectory} from './trajectory.js';
import {ActivityStore} from './activity-store.js';
import {newAgentState,foldAgent} from './agent-analysis.js';
import {buildSnapshot,Timeline,runAction} from './view-snapshot.js';
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
const mobileBridge=window.DotWorkspace;
const request=createTransport({bridge:mobileBridge,receive:fn=>{window.dotWorkspaceReply=fn;},storage:localStorage,fetcher:fetch.bind(window),capability});
const $ = s => document.querySelector(s);
$('#app').innerHTML = shellMarkup;
let active = null, generation = 0, sequence = 1, offset = 0, serial = 0, pollRunning = false, disposed = false, selecting = false;
let geometryUncertain=false;let selectionIdle=Promise.resolve();
// Per selected session: null = not known yet, true = keeper speaks read_frame, false = legacy sampling.
let frames=null,incarnation='';
// Opening a session replays what the keeper still holds (up to 1 MiB). That replay happens out of
// sight and back-to-back, then the view appears at the bottom: no watching it scroll from the top.
let catchingUp=false,catchStarted=0,replayed=0;
function caughtUp(){if(!catchingUp)return;catchingUp=false;$('#terminal').classList.remove('catching-up');term.scrollToBottom();timeline.mark('history shown',replayed);publishSoon();}
// Presence: who is on this session and who is typing. null support = not asked yet, false = older keeper.
const me=describeView(navigator.userAgent,(()=>{try{let v=sessionStorage.getItem('dot-view-id');if(!v){v=newViewId();sessionStorage.setItem('dot-view-id',v);}return v;}catch{return newViewId();}})());
let presence=null,presenceSupported=null,lastTapAt=0,acquiring=null;
let pollIdle=Promise.resolve(), finishPoll=()=>{};
let forceNext = false;
const term = new Terminal({fontFamily:'"SF Mono", Menlo, monospace',fontSize:13, lineHeight:1.25, cursorBlink:true, scrollback:6000, allowProposedApi:false, screenReaderMode:true, theme:{background:'#111519',foreground:'#d4dedc',cursor:'#adf4cf',selectionBackground:'#35554e',black:'#131c22',red:'#ef8f87',green:'#adf4cf',yellow:'#ead9a0',blue:'#92bce6',magenta:'#c8a6e3',cyan:'#95d7d8',white:'#e7eee8'}});
const fit = new FitAddon(); term.loadAddon(fit);
let opened = false, lastIterm = 0, lastItermScreen = null;
let appearance=applyAppearance(loadAppearance(localStorage),term,localStorage);
const copyIndex=indexShell($('#app'));
const activity=new ActivityStore();let controlSeen=false;
// What the agent in the selected session did, if the owner bound its log on this device.
const timeline=new Timeline();timeline.mark('page started');
let agentFeed=null;
// Interface state the backend keeps for us, because a view's own storage does not survive a restart.
let uiState={};let uiTimer=0;
function saveUi(patch){uiState={...uiState,...patch};clearTimeout(uiTimer);uiTimer=setTimeout(()=>{api('ui-state',uiState).catch(()=>{});},400);}
const activityPane=installTrajectory({onPrefs:p=>saveUi({activity_open:p.open,activity_width:p.width}),workspace:$('#workspace'),tabs:$('.view-tabs'),button:$('#activity'),store:activity,agent:()=>agentFeed,focusTerminal:()=>{if(opened)term.focus();}});
term.onResize(({cols,rows})=>activity.mark('resize',{cols,rows}));
installPlan($('#plan'));
$('#terminal').addEventListener('pointerdown',()=>{if(opened){term.textarea?.focus({preventScroll:true});window.dotRefreshInputFocus?.();}if(active&&!generation)tapControl();});
setInterval(()=>{if(!document.hidden)hello();},2500);
async function pullAgent(){
 if(mobileBridge||!active||active.kind!=='dot'||(active.device&&active.device!=='local')||disposed||document.hidden){return;}
 const target=active,epoch=serial,feed=agentFeed?.session===target.id?agentFeed:{session:target.id,state:newAgentState(),next:0,version:0,bound:null};
 if(feed.bound===false)return;
 try{const r=await api('sessions/'+encodeURIComponent(target.id)+'/events?after='+feed.next+(feed.next?'':'&tail=25000000'));if(epoch!==serial)return;
  feed.bound=r.bound===true;if(!feed.bound){agentFeed=null;return;}
  if(r.events.length){foldAgent(feed.state,r.events);feed.version++;}feed.next=r.next;agentFeed=feed;if(r.next<r.size)setTimeout(pullAgent,0);
 }catch(e){if(e.status===404||e.status===405){feed.bound=false;}}
}
setInterval(pullAgent,2000);
setInterval(()=>{if(!document.hidden&&!disposed)refresh().catch(()=>{});},8000);
// Version and refresh. The label is the build this view is RUNNING; a different build on disk turns
// the button into an update notice. Auto-reload only when nobody is typing here; sessions outlive views.
const uiVersion=(()=>{try{return parseVersion(__DOT_UI_VERSION__);}catch{return {build:'dev',commit:'',builtAt:''};}})();let updateReady=null;
const versionLabel=()=>{const b=$('#version');$('#version-text').textContent=updateReady?'Update ready':'Version '+(uiVersion.builtAt?new Date(uiVersion.builtAt).toLocaleString(undefined,{month:'short',day:'numeric',hour:'2-digit',minute:'2-digit'}):uiVersion.build);const rb=$('#reload');if(rb){rb.dataset.state=updateReady?'update':'current';rb.title=updateReady?'Update available · reload':'Reload this view';}b.dataset.state=updateReady?'update':'current';b.title=updateReady?'Running '+uiVersion.build+' · available '+updateReady.build:'Running build '+uiVersion.build+(uiVersion.builtAt?' · built '+new Date(uiVersion.builtAt).toLocaleString():'')+' · click to reload this view';};
const reloadView=async()=>{try{await release();}catch{/* the keeper fences a lost lease anyway */}location.reload();};
const reloadIfIdle=()=>{if(updateReady&&safeToReload({controlHeld:!!generation,queuedBytes:input.state().queuedBytes,dialogOpen:!!document.querySelector('dialog[open]')}))reloadView();};
$('#version').onclick=reloadView;$('#reload').onclick=reloadView;versionLabel();
if(uiVersion.build!=='dev')watchVersion({current:uiVersion,load:()=>fetch('version.json',{cache:'no-store'}).then(r=>{if(!r.ok)throw new Error('version unavailable');return r.json();}),onUpdate:next=>{updateReady=next;versionLabel();status('An update is ready · it loads when you pause typing');setTimeout(reloadIfIdle,3000);},paused:()=>document.hidden});
setInterval(reloadIfIdle,5000);
$('#menu').onclick=()=>{const shown=$('#app').classList.toggle('show-sessions');$('#menu').setAttribute('aria-expanded',String(shown));};
function status(s) { if(controlSeen!==!!generation){controlSeen=!!generation;activity.mark('control',{state:controlSeen?'taken':'ended'});}$('#state').textContent=s;$('#state').dataset.notice=String(!/^(Watching|Viewing|You are typing here|view-changed|released|Choose a session)/.test(s));$('#rename').hidden=!active;$('#input-state').title=s;$('#control').hidden=!!generation||!active;$('#detach').hidden=!generation;const badge=$('#input-state');if(badge&&!generation){badge.textContent='Watching';badge.dataset.state='view-only';}else if(badge&&badge.dataset.state==='view-only'){badge.textContent='Typing';badge.dataset.state='idle';} }
async function api(path, data) {
 const finish=health.begin(routeKey(path,data));
 try { const v=await request(path,data); if(v.type==='error'||v.error){const e=new Error(v.message||v.error);e.code=v.type!=='error'?'keeper-error':v.message==='stale controller generation'?'controller-fenced':String(v.message).startsWith('controller busy')?'controller-busy':'keeper-error';throw e;} finish(true);return v;
 }catch(error){finish(false);throw error;}
}
const deviceOf=new Map(); // session id -> device id, filled by refresh()
function operation(target, op) {return api(sessionPath(target.device||'local',target.id),op);}
function showError(e){status(e.message||String(e));}
function reveal(){if(!opened){$('#welcome').remove();term.open($('#terminal'));opened=true;try{const gpu=new WebglAddon();gpu.onContextLoss(()=>{gpu.dispose();rendererName='dom';});term.loadAddon(gpu);rendererName='webgl';}catch{rendererName='dom';}fit.fit();}term.focus();}
async function release(){const old=active,g=generation;generation=0;input.reset('released');if(old?.kind==='dot'&&g)await operation(old,{type:'release',generation:g});status('Viewing · input released');}
async function select(item){
 const own=++serial;selecting=true;let selected;selectionIdle=new Promise(resolve=>{selected=resolve;});
 try {
  try{await release();}catch(e){if(own===serial)showError(e);}
  if(own!==serial)return;
  active=null;input.reset('view-changed');forceNext=false;lastItermScreen=null;
  $('#app').classList.remove('show-sessions');$('#menu').setAttribute('aria-expanded','false');
  generation=0;sequence=1;reveal();await write('');if(own!==serial)return;
  term.reset();offset=0;signals.reset(item.kind);lastGeometry=0;frames=null;incarnation='';presence=null;presenceSupported=null;agentFeed=null;active=item;if(item.kind==='dot')saveUi({last_session:item.id,last_device:item.device||'local'});catchingUp=item.kind==='dot';catchStarted=performance.now();replayed=0;$('#terminal').classList.toggle('catching-up',catchingUp);timeline.mark('session selected',item.id.slice(0,8));controlSeen=false;activity.bind(item);
  $('#title').textContent=item.name;
  $('#details').textContent=item.kind==='dot'?'Session '+item.id.slice(0,8)+(item.device&&item.device!=='local'?' · shell runs on '+item.name.split(' / ')[0]+' · reached through this device':(mobileBridge?' · shell runs on the paired Mac':' · shell stays on this device')):'iTerm owns this shell · screen projection is text-only';
  status('Watching · tap the terminal or start typing');
  if(item.kind==='dot'&&sticky(item.id))setTimeout(()=>{if(active===item&&!generation)tapControl();},400);
  document.querySelectorAll('nav button').forEach(b=>b.classList.toggle('selected',b.dataset.id===item.id&&b.dataset.device===(item.device||'local')));
  if(item.kind==='dot'){
   const r=await operation(item,{type:'status'});if(own!==serial)return;$('#details').textContent+=' · PID '+r.pid;
   const screen=await operation(item,{type:'screen'});if(own!==serial)return;term.resize(screen.cols,screen.rows);
  }

 }catch(e){if(own===serial)showError(e);}finally{if(own===serial){selecting=false;window.dotRefreshInputFocus?.();}selected();}
}
let catalogSignature="", sessionLabels={};
async function refresh(){
 // Devices first, then each connected device's sessions. A backend without a catalog is one local device.
 let devices;try{devices=normalizeDevices(await api('devices'));}catch(e){if(e.status!==404&&e.status!==405)throw e;devices=LOCAL_ONLY;}
 const lists=await Promise.all(devices.map(async d=>{if(d.state!=='connected')return [];try{const v=await api(sessionsPath(d.id));if(d.local){$('#new').disabled=v.can_create===false;const start=$('#start');if(start)start.disabled=v.can_create===false;}return normalizeSessions(v);}catch{d.state='offline';return [];}}));
 const local=devices.find(d=>d.local);if(!mobileBridge&&local&&local.name!=='This device')me.label=local.name+' · '+(me.kind==='app'?'app':me.kind==='phone'?'phone':me.browser||'browser');
 try{sessionLabels=await api('session-labels');}catch{}
 for(const [i,d] of devices.entries())for(const s of lists[i]){const row=[...document.querySelectorAll('#sessions .session')].find(b=>b.dataset.id===s.id&&b.dataset.device===d.id);const u=s.usage;if(row&&u?.state==='sampled'&&Date.now()/1000-Number(u.at)<20)row.querySelector('.session-usage').textContent=Number(u.cpu).toFixed(0)+'% · '+Math.round(u.resident/1048576)+' MB';else if(row)row.querySelector('.session-usage').textContent='—';}
 const signature=JSON.stringify([devices,lists.map(ss=>ss.map(({usage,...s})=>s)),sessionLabels]);if(signature===catalogSignature)return;catalogSignature=signature;
 deviceOf.clear();const nav=$('#sessions');nav.replaceChildren();const tabs=$('#session-tabs');tabs.replaceChildren();
 devices.forEach((d,i)=>{
  const group=document.createElement('section');group.className='device';group.dataset.state=d.state;group.dataset.kind=d.kind;
  const head=document.createElement('div');head.className='device-head';const name=document.createElement('button');name.className='device-name';name.textContent=KIND_GLYPH[d.kind]+' '+d.name;name.setAttribute('aria-label','Resources for '+d.name);name.onclick=()=>showDeviceResources(d);
  const state=document.createElement('small');state.textContent=d.local?'This device':STATE_LABEL[d.state];name.title=d.name;head.append(name,state);
  if(d.canCreate&&d.state==='connected'){const add=document.createElement('button');add.className='device-add';add.textContent='+';add.setAttribute('aria-label','New terminal on '+d.name);add.title='New terminal on '+d.name;add.onclick=()=>create(d.id);head.append(add);}
  group.append(head);
  lists[i].sort((a,b)=>a.id.localeCompare(b.id));
  for(const [index,s] of lists[i].entries()){
   deviceOf.set(s.id,d.id);
   const label=sessionLabels[d.id+'/'+s.id]||'Terminal '+(index+1);
   if(active?.id===s.id&&active?.device===d.id){active.name=label;$('#title').textContent=label;}
   const b=document.createElement('button');b.dataset.id=s.id;b.dataset.device=d.id;b.className='session'+(active?.id===s.id&&active?.device===d.id?' selected':'');
   const dot=document.createElement('i');dot.className='session-dot';dot.dataset.state=s.exited?'ended':'running';dot.setAttribute('aria-label',s.exited?'Ended':'Running');
   const text=document.createElement('span');text.className='session-name';text.textContent=label;b.append(dot,text);
   const small=document.createElement('small');small.className='session-usage';
   const u=s.usage;const fresh=u?.state==='sampled'&&Date.now()/1000-Number(u.at)<20;
   small.textContent=fresh?Number(u.cpu).toFixed(0)+'% · '+Math.round(u.resident/1048576)+' MB':'—';
   small.title=fresh?'CPU (100% = one core) · summed resident memory of shell and descendants; shared pages may be counted twice':'Process usage unavailable';b.append(small);
   b.title=d.name+' · '+s.id;
   b.onclick=event=>{if(event?.detail===0&&(acquiring||selecting||input.state().queuedBytes))return;return select({kind:'dot',id:s.id,device:d.id,name:label});};
   b.ondblclick=()=>renameSession({id:s.id,device:d.id,name:label});group.append(b);
   const tab=b.cloneNode(true);tab.querySelector('.session-usage')?.remove();tab.className='session-tab'+(active?.id===s.id&&active?.device===d.id?' selected':'');tab.setAttribute('aria-label',d.name+' · '+label);tab.onclick=b.onclick;tab.ondblclick=b.ondblclick;tabs.append(tab);
  }
  if(!lists[i].length){const empty=document.createElement('p');empty.className='device-empty';empty.textContent=d.state==='connected'?'No sessions':d.state==='offline'?'Not reachable right now':'This device did not accept our key';group.append(empty);}
  nav.append(group);
 });
}
async function create(device='local'){if(typeof device!=='string')device='local';try{const s=await api(sessionsPath(device),{});await refresh();await select({kind:'dot',id:s.session,device,name:'Terminal / '+s.session.slice(0,8)});if(active?.id===s.session)await control();}catch(e){showError(e);}}
// "This is where I type." A view that had input control on a session takes it back by itself after a
// reload or a reselect. It stops doing that only when another view takes control (then THAT view
// is where the owner types). Per view, per session; nothing but a flag is stored.
function sticky(id,on){if(mobileBridge&&on===undefined)return false;const typing={...(uiState.typing||{})};if(on===undefined)return typing[id]===me.kind;if(on)typing[id]=me.kind;else delete typing[id];saveUi({typing});return on;}
function renderPresence(){
 const box=$('#presence');if(!box)return;box.replaceChildren();
 const chips=presenceChips(presence,me.view);
 box.textContent=presenceSupported===false?'':chips.length>1?'◉ '+chips.length:'';
 box.title=chips.map(c=>c.label+(c.you?' (this view)':'')+(c.typing?' · typing':' · watching')).join('\n');
}

async function hello(){
 if(!active||active.kind!=='dot'||disposed||presenceSupported===false){renderPresence();return;}
 const target=active,epoch=serial;
 try{const r=await operation(target,{type:'hello',view:me.view,label:me.label,kind:me.kind});if(epoch!==serial)return;presenceSupported=true;presence=r;}
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
   let r;try{r=await operation(target,presenceSupported?{type:'acquire_as',view:me.view,takeover:forceNext}:{type:'acquire',takeover:forceNext});}
   catch(e){if(presenceSupported||forceNext||!/already controlled/.test(String(e.message)))throw e;r=await operation(target,{type:'acquire',takeover:true});}
   if(epoch!==serial){await operation(target,{type:'release',generation:r.generation});return;}
   generation=r.generation;sequence=1;await resize();
  }else generation=1;
  if(epoch!==serial)return;
  timeline.mark('typing here');forceNext=false;status('You are typing here');term.focus();hello();sticky(target.id,true);
 }catch(e){if(epoch!==serial)return;forceNext=true;$('#control').textContent='Take over typing';showError(e);}
}
const resizes=new LatestResize(async v=>{
 if(v.epoch!==serial||v.generation!==generation||!generation)return;
 const start=performance.now();
 const applied=await orderedResize({drain:async()=>{await pollIdle;await write('');},
  isCurrent:()=>v.epoch===serial&&v.generation===generation&&generation!==0,
  prepareGrid:()=>{geometryUncertain=true;term.resize(v.cols,v.rows);},
  recover:async()=>{const screen=await operation(v,{type:"screen"});if(v.epoch===serial){term.resize(screen.cols,screen.rows);geometryUncertain=false;}},
  send:()=>operation(v,{type:'resize',generation:v.generation,cols:v.cols,rows:v.rows})});
 if(applied)geometryUncertain=false;
 if(applied)signals.sample('resize',performance.now()-start);
},(error,value)=>{if(value.epoch===serial)showError(error);});
async function resize(){
 if(!opened)return;
 // A viewer follows the host grid and scrolls. Only the controller changes it.
 if(active?.kind==='dot'){
  if(!generation)return;
  const d=fit.proposeDimensions();if(!d)return;
  await resizes.request({id:active.id,device:active.device,epoch:serial,generation,cols:Math.max(2,Math.min(240,d.cols)),rows:Math.max(1,Math.min(100,d.rows))});
 }else fit.fit();
}
const observer=new ResizeObserver(()=>{clearTimeout(resizeTimer);resizeTimer=setTimeout(()=>resize().catch(showError),120);});observer.observe($('#terminal'));
// One ordered, fenced input path. See input-controller.js for what it promises.
const inputLabels={'view-only':'You are watching · tap the terminal to type here','too-large':'Too large to send at once · nothing was sent','busy':'Session busy · that input was not sent · try again','fenced':'Control changed · view only','unknown-outcome':'Input acknowledgement lost. Inspect the screen, then take control again; input was not retried.'};
const input=new InputController({
 canSend:()=>!!active&&!!generation,
 send:async bytes=>{
  const target=active,g=generation,start=performance.now();activityAt=Date.now();nextPollAt=0;
  // Control can be lost between queueing and sending (a gap, a takeover): those bytes must not go.
  if(!target||!g)throw Object.assign(new Error('input control is not held'),{code:'controller-fenced'});
  if(target.kind==='dot'){const n=sequence;await operation(target,{type:'input',generation:g,sequence:n,data:Array.from(bytes)});if(generation===g)sequence=n+1;}
  else await api('iterm',{action:'input',id:target.id,text:new TextDecoder().decode(bytes)});
  signals.sample('input',performance.now()-start);
 },
 onState:state=>{
  $('#input-state').textContent={idle:generation?'Typing':'Watching',sending:'Sending',queued:'Sending · '+state.queuedBytes+' bytes waiting',uncertain:'Stopped · check the screen'}[state.condition];
  $('#input-state').dataset.state=generation?state.condition:'view-only';
  if(state.condition==='uncertain')activity.mark('input-stopped',{reason:state.refusal});if(state.refusal==='fenced'||state.refusal==='unknown-outcome'){generation=0;signals.fail();}
  if(state.refusal)status(inputLabels[state.refusal]||state.refusal);
 }});
// Files dropped on the Mac app arrive here as paths (the host reads them; a browser cannot). They are
// inserted like a paste, quoted for a shell, never submitted. Only for a session on this device.
window.dotDropFiles=async paths=>{
 if(!Array.isArray(paths)||!paths.length||!active)return;
 if(active.kind!=='dot'||(active.device&&active.device!=='local')){status('Files can only be dropped into a session on this device');return;}
 if(!await tapControl())return;
 const quoted=paths.filter(p=>typeof p==='string'&&p.startsWith('/')&&p.length<=1024&&!/[\x00-\x1f\x7f]/.test(p)).slice(0,32).map(p=>p.replace(/[^A-Za-z0-9_.\/\-+@%:,=]/g,c=>'\\'+c));
 if(!quoted.length)return;term.paste(quoted.join(' ')+' ');term.focus();status(quoted.length===1?'File path inserted · not sent':quoted.length+' file paths inserted · not sent');
};
// Typing or tapping in the terminal IS asking for control. Keys pressed while control is being
// acquired were never sent, so delivering them afterwards is not a replay. A key never confirms a
// takeover from someone who is typing; only a deliberate second tap does.
const sendInput=createInputGate({ready:()=>!!generation&&!acquiring&&!selecting,acquire:async()=>{const epoch=serial;await selectionIdle;return epoch===serial?tapControl({viaKey:true}):false;},submit:text=>input.submit(text),context:()=>serial,notify:status});
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
    try{r=await operation(target,{type:'read_frame',after:offset});if(epoch!==serial)return;frames=true;}
    catch(e){if(epoch!==serial)return;if(frames===true||!framesUnsupported(e))throw e;frames=false;signals.note?.('legacy-geometry');}
   }
   if(frames===false){r=await operation(target,{type:'read',after:offset});if(epoch!==serial)return;}
   if(frames){
    const plan=framePlan({frame:r,knownIncarnation:incarnation,controller:!!generation,cols:term.cols,rows:term.rows});
    if(plan.restart){incarnation=r.incarnation;offset=0;generation=0;term.reset();signals.reset(target.kind);activity.mark('gap');status('Session stream restarted · replaying');return;}
    incarnation=r.incarnation;if(plan.resize)term.resize(plan.resize.cols,plan.resize.rows);
   }
   signals.sample('read',performance.now()-start);if(r.data.length){activityAt=Date.now();nextPollAt=0;activity.output(r.data.length);}const opening=offset===0;if(r.gap&&!opening)activity.mark('gap');signals.receive(r.next,r.gap&&!opening);
   const geometryDue=Date.now()-lastGeometry>1000;
   if(geometryDue&&generation){
    const checked=generation;const result=await probeControl(g=>operation(target,{type:'check_control',generation:g}),checked);
    if(epoch!==serial)return;
    if(generation===checked){if(result.state==='fenced'){generation=0;sticky(target.id,false);status('Another view is typing now · tap the terminal to type here again');}
     else if(result.state==='unconfirmed')status('Connection uncertain · control check will retry');}
   }
   // Legacy keepers cannot label byte chunks with geometry. Sample BEFORE applying
   // output, never after a redraw has already been parsed using the old grid.
   let screen;
   if(r.gap||(!frames&&(geometryUncertain||(!generation&&(r.data.length||geometryDue))))){
    screen=await operation(target,{type:'screen'});if(epoch!==serial)return;
    if(term.cols!==screen.cols||term.rows!==screen.rows)term.resize(screen.cols,screen.rows);
    geometryUncertain=false;
   }
   if(geometryDue)lastGeometry=Date.now();
   const parseStart=performance.now();
   if(r.gap){term.reset();if(!opening)await write('\r\n[This view was away and missed some output. Showing the current screen.]\r\n');if(epoch!==serial)return;await write(screen.lines.join('\r\n'));if(epoch!==serial)return;offset=r.next;if(!opening)status('Caught up · some earlier output was missed');}
   else {await write(new Uint8Array(r.data));if(epoch!==serial)return;offset=r.next;}
   // Caught up means the keeper had nothing more, not "this read was short": frames end at every
   // resize mark, so short reads happen in the middle of history. A safety limit covers a session that never pauses.
   if(catchingUp){replayed+=r.data.length;if(!r.data.length||performance.now()-catchStarted>5000)caughtUp();else{nextPollAt=0;setTimeout(poll,0);}}
   signals.apply(offset);signals.sample('parse',performance.now()-parseStart);
   if(r.exited)activity.mark('exited');if(r.exited)status('Shell exited · output remains available');
  } else {
   lastIterm=Date.now();const r=await api('iterm',{action:'screen',id:target.id});if(epoch!==serial)return;
   // External application screen text must never be interpreted as escape commands.
   const safe=r.lines.map(l=>l.replace(/[\x00-\x1f\x7f-\x9f]/g,''));
   const text=safe.join('\r\n')+'\x1b['+(Math.max(0,Math.min(term.rows-1,r.cursor_row||0))+1)+';'+(Math.max(0,Math.min(term.cols-1,r.cursor_col||0))+1)+'H';
   if(text!==lastItermScreen){lastItermScreen=text;await write('\x1b[H\x1b[2J'+text);}
  }
 }catch(e){if(epoch===serial){signals.fail();showError(e);caughtUp();}}finally{pollRunning=false;finishPoll();}
}
setInterval(poll,32);
$('#new').onclick=create;$('#start').onclick=create;$('#refresh').onclick=()=>refresh().catch(showError);$('#control').onclick=control;$('#detach').onclick=()=>{if(active?.kind==='dot')sticky(active.id,false);release().catch(showError);};

$('#browser').onclick=()=>window.open(location.origin+'/#'+capability,'_blank','noopener,noreferrer');
window.addEventListener('keydown',e=>{if(e.metaKey&&!e.ctrlKey&&!e.altKey&&e.key==='n'&&!document.querySelector('dialog[open]')){e.preventDefault();create();}});
window.addEventListener('pagehide',()=>{disposed=true;if(active?.kind==='dot'&&generation){const path=sessionPath(active.device||'local',active.id);const op={type:'release',generation};if(mobileBridge)request(path,op).catch(()=>{});else fetch('/api/'+path,{method:'POST',headers:{Authorization:'Bearer '+capability,'Content-Type':'application/json'},body:JSON.stringify(op),keepalive:true}).catch(()=>{});}});
status('Choose a session or start a new shell');
// Publish what this view shows, and run interface actions left for it. See view-snapshot.js.
let lastPublished='',publishTimer=0;
async function publish(){
 if(disposed||mobileBridge)return;
 try{const snap=buildSnapshot({doc:document,win:window,term:opened?term:null,timeline,facts:{build:uiVersion.build,view:{id:me.view.slice(-6),kind:me.kind,label:me.label},session:active?{id:active.id.slice(0,8),device:active.device||'local',typing:!!generation,frames,presence:presenceSupported}:null}});
  const key=JSON.stringify({...snap,at:0});if(key===lastPublished)return;lastPublished=key;await api('view-snapshot',snap);}catch{/* an older backend has no snapshot route */}
}
function publishSoon(){clearTimeout(publishTimer);publishTimer=setTimeout(publish,150);}
setInterval(publish,2000);
setInterval(async()=>{if(disposed||mobileBridge)return;try{const r=await api('view-actions');for(const a of r.actions||[]){timeline.mark('action',a);runAction(a,{reload:reloadView,refresh:()=>refresh().catch(()=>{}),activity:open=>activityPane.toggle(open),select:id=>{const b=[...document.querySelectorAll('nav#sessions .session')].find(x=>x.dataset.id.startsWith(id));if(b)b.click();},snapshot:publish});}}catch{/* older backend */}},1500);
// Start where the owner left off: the same session and the same Activity pane. Older backends have no
// interface state (404) and simply start on the welcome screen.
(async()=>{
 try{uiState=await api('ui-state');timeline.mark('interface state loaded');}catch{uiState={};}
 try{await refresh();timeline.mark('devices loaded',deviceOf.size);}catch(e){showError(e);}
 activityPane.restore({open:uiState.activity_open===true,width:uiState.activity_width||0});
 const last=uiState.last_session;if(last&&!active&&deviceOf.has(last)){const device=deviceOf.get(last);const label=[...document.querySelectorAll('nav#sessions .session')].find(b=>b.dataset.id===last);if(label)label.click();else select({kind:'dot',id:last,device,name:'Terminal / '+last.slice(0,8)});}
})();

// Owner tools use the same authenticated loopback boundary as terminal operations.
const tools=document.createElement('div');tools.className='owner-tools';
tools.innerHTML='<div class="section">Tools</div><button id="resources">◷ Resources</button><button id="vault">◇ Vault & audit</button>';
$('aside').insertBefore(tools,$('.bottom'));
const panel=document.createElement('dialog');panel.id='owner-panel';document.body.append(panel);
function renameSession(target=active){
 if(!target)return;
 const selected={...target};panelBase('Rename session');
 const form=document.createElement('form');const field=document.createElement('input');field.type='text';field.maxLength=80;field.value=selected.name;field.setAttribute('aria-label','Session name');
 const save=document.createElement('button');save.type='submit';save.textContent='Save';save.className='primary';
 const error=document.createElement('p');error.setAttribute('role','alert');
 form.append(field,save,error);form.onsubmit=async e=>{e.preventDefault();save.disabled=true;try{await api('session-labels',{device:selected.device,id:selected.id,name:field.value.trim()});catalogSignature='';await refresh();panel.close();}catch(e){error.textContent='Could not save the name. '+e.message;}finally{save.disabled=false;}};
 panel.append(form);field.focus();field.select();
}
$('#rename').onclick=()=>renameSession();
let disposeResources=()=>{};
panel.addEventListener('close',()=>disposeResources());
function panelBase(title){disposeResources();clearInterval(auditTimer);clearInterval(systemTimer);panel.replaceChildren();const top=document.createElement('div');top.className='panel-top';const h=document.createElement('h2');h.textContent=title;const close=document.createElement('button');close.textContent='Close';close.onclick=()=>panel.close();top.append(h,close);panel.append(top);if(!panel.open)panel.showModal();}
function paragraph(text){const p=document.createElement('p');p.textContent=text;panel.append(p);return p;}
function showDeviceResources(device={id:'local',name:'This device'}) {
 panelBase(device.name+' · Resources');
 if(mobileBridge){paragraph('Process inventory needs a separate device permission. Terminal pairing does not grant access to private machine inventory. This view is currently available on the host.');return;}
 const content=document.createElement('section');content.className='resource-view';panel.append(content);
 disposeResources=mountDeviceResources(content,{request:api,device,sessionLabels});
}
$('#resources').onclick=()=>showDeviceResources();
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
setInterval(()=>{if(disposed)return;const s=signals.snapshot();$('#sync').textContent=(document.hidden?'Paused':({'measurement-error':'Status unavailable','unknown':'Connecting','error':'Connection problem','stale':'Not updating','history-gap':'Missed some output','catching-up':'Catching up','caught-up-to-response':'Live'}[s.state]));$('#sync').dataset.state=s.state;
 if(!document.hidden)window.dispatchEvent(new CustomEvent('dot:session-state',{detail:s}));},1000);
installKeyDock($('main'),term,sendInput,copyIndex);

if(mobileBridge){document.documentElement.dataset.platform='android';for(const id of ['browser','resources','vault'])$('#'+id).hidden=true;$('.identity').textContent='◈ Android · paired workspace';}

if(mobileBridge){
 const compose=document.createElement('output');compose.className='ime-composition';compose.hidden=true;$('#workspace').append(compose);window.dotComposition=text=>{compose.textContent=typeof text==='string'?text:'';compose.hidden=!compose.textContent;};
 const inputTarget=()=>active?.kind==='dot'?(active.device||'local')+'/'+active.id+'/'+serial:'';
 window.dotNativeInput=(text,target)=>{if(typeof text!=='string')return;if(target!==inputTarget()){status('Unsent input from the previous tab was discarded');return;}if(text.length>1024*1024){status('Input too large; nothing was sent');return;}sendInput(text);};
 const reportFocus=()=>mobileBridge.terminalFocus(document.activeElement===term.textarea,inputTarget());window.dotRefreshInputFocus=reportFocus;
 document.addEventListener('focusin',reportFocus);document.addEventListener('focusout',()=>setTimeout(reportFocus,0));
}
