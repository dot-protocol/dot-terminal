import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import './style.css';

const capability = location.hash.slice(1) || sessionStorage.getItem('dot-capability') || '';
if (capability) sessionStorage.setItem('dot-capability', capability);
history.replaceState(null, '', location.pathname);
const $ = s => document.querySelector(s);
$('#app').innerHTML = `
<aside><div class="brand"><b class="mark">●</b> DOT <span>TERMINAL</span></div>
<div class="workspace">PERSONAL WORKSPACE <span class="online">●</span></div>
<button id="new" class="primary">＋ New terminal <kbd>⌘ N</kbd></button>
<div class="section">DOT SESSIONS <button id="refresh" aria-label="Refresh sessions">↻</button></div><nav id="sessions"></nav>
<div class="section">CONNECTED APPS <span>LOCAL</span></div><button id="iterm">▣ iTerm sessions</button><nav id="iterm-list"></nav>
<div class="bottom"><div class="identity">◈ <div>This Mac<small>Local session owner</small></div><i></i></div><p>Your shells keep running when this window closes.</p></div></aside>
<main><header><div><span class="eyebrow">ONE SESSION. ANY SCREEN.</span><h1 id="title">Your command center</h1></div><div class="header-actions"><button id="browser">Open in browser ↗</button><button id="control">Take control</button><button id="detach">Release</button></div></header>
<div class="infobar"><span id="state">Choose a session or start a new shell</span><span id="mode">LOCAL · PRIVATE</span></div>
<div id="terminal"><div id="welcome"><div class="orb">●</div><h2>A home for your work.</h2><p>Persistent shells. Connected devices.<br>Pick up exactly where you left off.</p><button id="start">Start a terminal →</button><small>Existing iTerm sessions are available in the sidebar.</small></div></div>
<footer><span id="details">DOT / development preview</span><span>Rust session core · xterm.js renderer</span></footer></main>`;
let active = null, generation = 0, sequence = 1, offset = 0, serial = 0, pollRunning = false, disposed = false;
let inputQueue = Promise.resolve(), queuedBytes = 0, forceNext = false;
const term = new Terminal({fontFamily:'"SF Mono", Menlo, monospace',fontSize:13, lineHeight:1.25, cursorBlink:true, scrollback:6000, allowProposedApi:false, screenReaderMode:true, theme:{background:'#111519',foreground:'#d4dedc',cursor:'#adf4cf',selectionBackground:'#35554e',black:'#131c22',red:'#ef8f87',green:'#adf4cf',yellow:'#ead9a0',blue:'#92bce6',magenta:'#c8a6e3',cyan:'#95d7d8',white:'#e7eee8'}});
const fit = new FitAddon(); term.loadAddon(fit);
let opened = false, lastIterm = 0, lastItermScreen = null;
function status(s) { $('#state').textContent=s; }
async function api(path, data) {
 const r=await fetch('/api/'+path,{method:data===undefined?'GET':'POST',headers:{Authorization:'Bearer '+capability,'Content-Type':'application/json'},body:data===undefined?undefined:JSON.stringify(data),signal:AbortSignal.timeout(6000)});
 if(!r.ok) throw new Error(await r.text());
 const v=await r.json(); if(v.type==='error'||v.error) throw new Error(v.message||v.error); return v;
}
function operation(id, op) {return api('sessions/'+encodeURIComponent(id),op);}
function showError(e){status(e.message||String(e));}
function reveal(){if(!opened){$('#welcome').remove();term.open($('#terminal'));opened=true;fit.fit();}term.focus();}
async function release(){const old=active,g=generation;generation=0;if(old?.kind==='dot'&&g)await operation(old.id,{type:'release',generation:g});status('Viewing · input released');}
async function select(item){
 try {await release();} catch(e){showError(e);}
 forceNext=false; $('#control').textContent='Take control';
 lastItermScreen=null;
 active=item; const own=++serial; generation=0; sequence=1;offset=0;reveal();term.reset();
 $('#title').textContent=item.name;$('#mode').textContent=item.kind==='dot'?'DOT · SHARED PTY':'ITERM · SCREEN BRIDGE';
 $('#details').textContent=item.kind==='dot'?'Session '+item.id.slice(0,8)+' · shell stays on this Mac':'iTerm owns this shell · screen projection is text-only';
 status('Viewing · take control to type');
 if(item.kind==='dot') { try {let r=await operation(item.id,{type:'status'});if(own===serial)$('#details').textContent+=' · PID '+r.pid;}catch(e){showError(e);} }
 document.querySelectorAll('nav button').forEach(b=>b.classList.toggle('selected',b.dataset.id===item.id));
}
async function refresh(){const v=await api('sessions');$('#sessions').replaceChildren();for(const s of v.sessions){const b=document.createElement('button');b.dataset.id=s.session;b.className='session'+(active?.id===s.session?' selected':'');b.textContent=(s.exited?'○ ':'›_ ')+s.session.slice(0,8);const small=document.createElement('small');small.textContent=s.exited?'Ended':'Running';b.append(small);b.onclick=()=>select({kind:'dot',id:s.session,name:'Terminal / '+s.session.slice(0,8)});$('#sessions').append(b);}}
async function create(){try{const s=await api('sessions',{});await refresh();await select({kind:'dot',id:s.session,name:'Terminal / '+s.session.slice(0,8)});await control();}catch(e){showError(e);}}
async function control(){if(!active)return;try{if(active.kind==='dot'){const r=await operation(active.id,{type:'acquire',takeover:forceNext});generation=r.generation;sequence=1;await resize();}else{generation=1;}forceNext=false;$('#control').textContent='Take control';status('You have input control');term.focus();}catch(e){forceNext=true;$('#control').textContent='Take over input';showError(e);}}
async function resize(){if(opened){fit.fit();if(active?.kind==='dot'&&generation)await operation(active.id,{type:'resize',generation,cols:Math.min(240,term.cols),rows:Math.min(100,term.rows)});}}
const observer=new ResizeObserver(()=>{resize().catch(showError);});observer.observe($('#terminal'));
term.onData(data=>{
 if(!active||!generation){status('Read-only view · choose Take control to type');return;}
 const bytes=new TextEncoder().encode(data);if(queuedBytes+bytes.length>16384){status('Input queue full; paste was not sent');return;}
 const target=active,g=generation,epoch=serial;queuedBytes+=bytes.length;
 inputQueue=inputQueue.then(async()=>{
   if(epoch!==serial||generation!==g)return;
   try { if(target.kind==='dot') { await operation(target.id,{type:'input',generation:g,sequence:sequence++,data:Array.from(bytes)}); }
   else await api('iterm',{action:'input',id:target.id,text:data}); }
   catch(e){generation=0;status('Input acknowledgement lost. Inspect the screen, then take control again; input was not retried.');}
 }).finally(()=>{queuedBytes-=bytes.length;});
});
function write(data){return new Promise(resolve=>term.write(data,resolve));}
async function poll(){
 if(pollRunning||!active||disposed||document.hidden)return;
 if(active.kind==='iterm'&&Date.now()-lastIterm<500)return;pollRunning=true;const target=active,epoch=serial;
 try {
  if(target.kind==='dot') {
   const r=await operation(target.id,{type:'read',after:offset});if(epoch!==serial)return;
   if(r.gap){generation=0;term.reset();await write('\r\n[Output history limit reached. Take control after checking the current screen.]\r\n');const screen=await operation(target.id,{type:'screen'});if(epoch!==serial)return;await write(screen.lines.join('\r\n'));offset=r.next;status('History gap · current text snapshot shown');}
   else {await write(new Uint8Array(r.data));if(epoch!==serial)return;offset=r.next;}
   if(r.exited)status('Shell exited · output remains available');
  } else {
   lastIterm=Date.now();const r=await api('iterm',{action:'screen',id:target.id});if(epoch!==serial)return;
   // External application screen text must never be interpreted as escape commands.
   const safe=r.lines.map(l=>l.replace(/[\x00-\x1f\x7f-\x9f]/g,''));
   const text=safe.join('\r\n')+'\x1b['+(Math.max(0,Math.min(term.rows-1,r.cursor_row||0))+1)+';'+(Math.max(0,Math.min(term.cols-1,r.cursor_col||0))+1)+'H';
   if(text!==lastItermScreen){lastItermScreen=text;await write('\x1b[H\x1b[2J'+text);}
  }
 }catch(e){if(epoch===serial)showError(e);}finally{pollRunning=false;}
}
setInterval(poll,90);
$('#new').onclick=create;$('#start').onclick=create;$('#refresh').onclick=()=>refresh().catch(showError);$('#control').onclick=control;$('#detach').onclick=()=>release().catch(showError);
$('#iterm').onclick=async()=>{try{const r=await api('iterm',{action:'list'});$('#iterm-list').replaceChildren();for(const s of r.sessions){const b=document.createElement('button');b.textContent=s.name||'iTerm session';b.dataset.id=s.id;b.onclick=()=>select({kind:'iterm',id:s.id,name:s.name||'iTerm session'});$('#iterm-list').append(b);}status(r.sessions.length+' iTerm sessions available');}catch(e){showError(e);}};
$('#browser').onclick=()=>window.open(location.origin+'/#'+capability,'_blank','noopener,noreferrer');
window.addEventListener('keydown',e=>{if((e.metaKey||e.ctrlKey)&&e.key==='n'){e.preventDefault();create();}});
window.addEventListener('pagehide',()=>{disposed=true;if(active?.kind==='dot'&&generation)fetch('/api/sessions/'+active.id,{method:'POST',headers:{Authorization:'Bearer '+capability,'Content-Type':'application/json'},body:JSON.stringify({type:'release',generation}),keepalive:true}).catch(()=>{});});
refresh().catch(showError);

// Owner tools use the same authenticated loopback boundary as terminal operations.
const tools=document.createElement('div');tools.className='owner-tools';
tools.innerHTML='<div class="section">OWNER TOOLS</div><button id="resources">◷ Resources</button><button id="vault">◇ Vault & audit</button>';
$('aside').insertBefore(tools,$('.bottom'));
const panel=document.createElement('dialog');panel.id='owner-panel';document.body.append(panel);
function panelBase(title){panel.replaceChildren();const top=document.createElement('div');top.className='panel-top';const h=document.createElement('h2');h.textContent=title;const close=document.createElement('button');close.textContent='Close';close.onclick=()=>panel.close();top.append(h,close);panel.append(top);if(!panel.open)panel.showModal();}
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
