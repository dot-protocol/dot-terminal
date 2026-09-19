import {orderedResize} from './render-flow.js';
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
let pollIdle=Promise.resolve(), finishPoll=()=>{};
let inputQueue = Promise.resolve(), queuedBytes = 0, forceNext = false;
const term = new Terminal({fontFamily:'"SF Mono", Menlo, monospace',fontSize:13, lineHeight:1.25, cursorBlink:true, scrollback:6000, allowProposedApi:false, screenReaderMode:true, theme:{background:'#111519',foreground:'#d4dedc',cursor:'#adf4cf',selectionBackground:'#35554e',black:'#131c22',red:'#ef8f87',green:'#adf4cf',yellow:'#ead9a0',blue:'#92bce6',magenta:'#c8a6e3',cyan:'#95d7d8',white:'#e7eee8'}});
const fit = new FitAddon(); term.loadAddon(fit);
let opened = false, lastIterm = 0, lastItermScreen = null;
let appearance=applyAppearance(loadAppearance(localStorage),term,localStorage);
const copyIndex=indexShell($('#app'));
$('#menu').onclick=()=>{const shown=$('#app').classList.toggle('show-sessions');$('#menu').setAttribute('aria-expanded',String(shown));};
function status(s) { $('#state').textContent=s;$('#control').disabled=!!generation;$('#detach').disabled=!generation; }
async function api(path, data) {
 const finish=health.begin(routeKey(path,data));
 try { const r=await fetch('/api/'+path,{method:data===undefined?'GET':'POST',headers:{Authorization:'Bearer '+capability,'Content-Type':'application/json'},body:data===undefined?undefined:JSON.stringify(data),signal:AbortSignal.timeout(6000)});
 if(!r.ok) throw new Error(await r.text());
 const v=await r.json(); if(v.type==='error'||v.error){const e=new Error(v.message||v.error);e.code=v.type==='error'&&v.message==='stale controller generation'?'controller-fenced':'keeper-error';throw e;} finish(true);return v;
 }catch(error){finish(false);throw error;}
}
function operation(id, op) {return api('sessions/'+encodeURIComponent(id),op);}
function showError(e){status(e.message||String(e));}
function reveal(){if(!opened){$('#welcome').remove();term.open($('#terminal'));opened=true;try{const gpu=new WebglAddon();gpu.onContextLoss(()=>{gpu.dispose();rendererName='dom';});term.loadAddon(gpu);rendererName='webgl';}catch{rendererName='dom';}fit.fit();}term.focus();}
async function release(){const old=active,g=generation;generation=0;if(old?.kind==='dot'&&g)await operation(old.id,{type:'release',generation:g});status('Viewing · input released');}
async function select(item){
 const own=++serial;selecting=true;
 try {
  try{await release();}catch(e){if(own===serial)showError(e);}
  if(own!==serial)return;
  active=null;forceNext=false;$('#control').textContent='Take control';lastItermScreen=null;
  $('#app').classList.remove('show-sessions');$('#menu').setAttribute('aria-expanded','false');
  generation=0;sequence=1;reveal();await write('');if(own!==serial)return;
  term.reset();offset=0;signals.reset(item.kind);lastGeometry=0;active=item;
  $('#title').textContent=item.name;$('#mode').textContent=item.kind==='dot'?'DOT · SHARED PTY':'ITERM · SCREEN BRIDGE';
  $('#details').textContent=item.kind==='dot'?'Session '+item.id.slice(0,8)+' · shell stays on this Mac':'iTerm owns this shell · screen projection is text-only';
  status('Viewing · take control to type');
  document.querySelectorAll('nav button').forEach(b=>b.classList.toggle('selected',b.dataset.id===item.id));
  if(item.kind==='dot'){
   const r=await operation(item.id,{type:'status'});if(own!==serial)return;$('#details').textContent+=' · PID '+r.pid;
   const screen=await operation(item.id,{type:'screen'});if(own!==serial)return;term.resize(screen.cols,screen.rows);
  }

 }catch(e){if(own===serial)showError(e);}finally{if(own===serial)selecting=false;}
}
async function refresh(){const v=await api('sessions');$('#new').disabled=v.can_create===false;const start=$('#start');if(start)start.disabled=v.can_create===false;$('#sessions').replaceChildren();for(const s of v.sessions){const b=document.createElement('button');b.dataset.id=s.session;b.className='session'+(active?.id===s.session?' selected':'');b.textContent=(s.exited?'○ ':'›_ ')+s.session.slice(0,8);const small=document.createElement('small');small.textContent=s.exited?'Ended':'Running';b.append(small);b.onclick=()=>select({kind:'dot',id:s.session,name:'Terminal / '+s.session.slice(0,8)});$('#sessions').append(b);}}
async function create(){try{const s=await api('sessions',{});await refresh();await select({kind:'dot',id:s.session,name:'Terminal / '+s.session.slice(0,8)});if(active?.id===s.session)await control();}catch(e){showError(e);}}
async function control(){
 if(!active||generation)return;
 const target=active, epoch=serial;
 try{
  if(target.kind==='dot'){
   const r=await operation(target.id,{type:'acquire',takeover:forceNext});
   if(epoch!==serial){await operation(target.id,{type:'release',generation:r.generation});return;}
   generation=r.generation;sequence=1;await resize();
  }else generation=1;
  if(epoch!==serial)return;
  forceNext=false;$('#control').textContent='Take control';status('You have input control');term.focus();
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
function sendInput(data){

 if(!active||!generation){status('Read-only view · choose Take control to type');return;}
 activityAt=Date.now();nextPollAt=0;
 const bytes=new TextEncoder().encode(data);if(queuedBytes+bytes.length>16384){status('Input queue full; paste was not sent');return;}
 const target=active,g=generation,epoch=serial;queuedBytes+=bytes.length;
 inputQueue=inputQueue.then(async()=>{
   if(epoch!==serial||generation!==g)return;
   try {const start=performance.now(); if(target.kind==='dot') { await operation(target.id,{type:'input',generation:g,sequence:sequence++,data:Array.from(bytes)}); }
   else await api('iterm',{action:'input',id:target.id,text:data});if(epoch===serial)signals.sample('input',performance.now()-start); }
   catch(e){if(epoch!==serial)return;generation=0;signals.fail();status('Input acknowledgement lost. Inspect the screen, then take control again; input was not retried.');}
 }).finally(()=>{queuedBytes-=bytes.length;});
}
term.onData(sendInput);
function write(data){return new Promise(resolve=>term.write(data,resolve));}
async function poll(){
 if(pollRunning||resizes.running||selecting||!active||disposed||document.hidden||Date.now()<nextPollAt)return;
 nextPollAt=Date.now()+(Date.now()-activityAt<1500?32:250);
 if(active.kind==='iterm'&&Date.now()-lastIterm<500)return;pollRunning=true;pollIdle=new Promise(resolve=>{finishPoll=resolve;});const target=active,epoch=serial;
 try {
  if(target.kind==='dot') {
   const start=performance.now();const r=await operation(target.id,{type:'read',after:offset});if(epoch!==serial)return;
   signals.sample('read',performance.now()-start);if(r.data.length){activityAt=Date.now();nextPollAt=0;}signals.receive(r.next,r.gap);
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
   if(geometryUncertain||r.gap||(!generation&&(r.data.length||geometryDue))){
    screen=await operation(target.id,{type:'screen'});if(epoch!==serial)return;
    if(term.cols!==screen.cols||term.rows!==screen.rows)term.resize(screen.cols,screen.rows);
    geometryUncertain=false;
   }
   if(geometryDue)lastGeometry=Date.now();
   const parseStart=performance.now();
   if(r.gap){generation=0;term.reset();await write('\r\n[Output history limit reached. Take control after checking the current screen.]\r\n');if(epoch!==serial)return;await write(screen.lines.join('\r\n'));if(epoch!==serial)return;offset=r.next;status('History gap · current text snapshot shown');}
   else {await write(new Uint8Array(r.data));if(epoch!==serial)return;offset=r.next;}
   signals.apply(offset);signals.sample('parse',performance.now()-parseStart);
   if(r.exited)status('Shell exited · output remains available');
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
window.addEventListener('keydown',e=>{if((e.metaKey||e.ctrlKey)&&e.key==='n'){e.preventDefault();create();}});
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
 const table=document.createElement('table');table.className='health-table';panel.append(table);
 const render=()=>{const sync=signals.snapshot();metrics.textContent='Sync: '+sync.state+' · Response age: '+(sync.responseAgeMs??'unknown')+' ms · Pending parse: '+sync.pendingParseBytes+' bytes · Read p50/p95: '+sync.latency.read.p50+'/'+sync.latency.read.p95+' ms · Input ACK p95: '+sync.latency.input.p95+' ms · Renderer: '+rendererName+' · Peer views: unknown · View: '+(active?.kind||'welcome')+' · Control: '+(generation?'held':'view only')+' · Input queue: '+queuedBytes+' bytes · Poll: '+(pollRunning?'in flight':'idle');table.replaceChildren();const head=document.createElement('tr');for(const label of ['API','State','Latency','Calls / errors']){const cell=document.createElement('th');cell.textContent=label;head.append(cell);}table.append(head);for(const r of health.snapshot()){const tr=document.createElement('tr');tr.dataset.health=r.state;for(const value of [r.id,r.state+(r.inflight?' · busy':''),r.last?r.latency+' ms':'—',r.calls+' / '+r.failures]){const td=document.createElement('td');td.textContent=value;tr.append(td);}table.append(tr);}};
 render();clearInterval(systemTimer);systemTimer=setInterval(()=>{if(!panel.open||document.hidden)return;render();},1000);
 const label=document.createElement('label');label.textContent='Search interface index';const search=document.createElement('input');search.type='search';search.placeholder='Try “terminal”, “primary”, or “dynamic”';label.append(search);panel.append(label);
 const results=document.createElement('pre');results.className='copy-index';panel.append(results);
 const show=()=>{const q=search.value.toLowerCase();results.textContent=copyIndex.filter(e=>[e.text,e.id,e.kind,e.role,e.signal].join(' ').toLowerCase().includes(q)).map(e=>e.id+' · '+e.kind+' / '+e.role+' / '+e.signal+'\n'+e.text).join('\n\n');};search.oninput=show;show();
 paragraph('Mapped state: '+stateFields.map(f=>f.id+' ('+f.type+(f.unit?', '+f.unit:'')+')').join(' · '));
 paragraph('Coverage: initial shell copy and seven dynamic slots. Owner-tool dialog content, terminal output and arbitrary program variables are excluded. This is a local registry foundation, not whole-system tracing.');
};

// Local subscribers may collaborate on operational state, never authentication or content.
setInterval(()=>{if(!disposed&&!document.hidden)window.dispatchEvent(new CustomEvent('dot:state',{detail:stateEvent({kind:active?.kind,controlHeld:!!generation,queuedBytes,pollRunning},health)}));},1000);

$('#sync').onclick=()=>$('#system').click();
setInterval(()=>{if(disposed)return;const s=signals.snapshot();$('#sync').textContent='Sync · '+(document.hidden?'paused':({'measurement-error':'measurement unavailable','unknown':'waiting','error':'check connection','stale':'stale','history-gap':'history missing','catching-up':'updating','caught-up-to-response':'current view'}[s.state]));if(s.historyGaps&&s.state!=='history-gap')$('#sync').textContent+=' · history missing';$('#sync').dataset.state=s.state;
 if(!document.hidden)window.dispatchEvent(new CustomEvent('dot:session-state',{detail:s}));},1000);
installKeyDock($('main'),term,sendInput,copyIndex);
