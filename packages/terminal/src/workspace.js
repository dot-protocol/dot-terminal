import {mountDeviceResources} from './resources.js';
import {mountTerminal} from './view.js';
/** A complete, host-contained workspace. No global page IDs, storage or navigation. */
export function mountWorkspace(element,{client,appearance={},onState=()=>{}}) {
 const root=document.createElement('section');root.className='dot-workspace';element.append(root);
 const make=(tag,text)=>{const e=document.createElement(tag);if(text)e.textContent=text;return e;};
 const toolbar=make('header'),devices=make('select'),sessions=make('select'),status=make('output','Choose a session');
 devices.setAttribute('aria-label','Device');sessions.setAttribute('aria-label','Session');status.setAttribute('aria-live','polite');
 const content=make('div');content.className='dot-workspace-content';
 const inventory=make('section');inventory.hidden=true;inventory.className='dot-resource-summary';
 let disposeResources=()=>{};let view=null,disposed=false,epoch=0,labels={},minimized=false,pending=Promise.resolve();
 const notice=e=>{if(!disposed)status.textContent=e.message||String(e);};
 const button=(label,action)=>{const b=make('button',label);b.type='button';b.onclick=()=>Promise.resolve().then(action).catch(notice);toolbar.append(b);return b;};
 toolbar.append(devices,sessions);
 const newButton=button('New',async()=>{const device=devices.value;const s=await client.create(device);if(device===devices.value)await loadSessions(s.session);});
 button('Rename',()=>{if(!sessions.value)return;rename.hidden=false;name.value=sessions.selectedOptions[0]?.textContent||'';name.focus();});
 button('Type here',()=>view?.takeControl());
 button('Watch',()=>view?.release());
 const minimize=button('Minimize',()=>{minimized=!minimized;view?.setPaused(minimized);content.hidden=minimized;minimize.textContent=minimized?'Restore':'Minimize';});
 const expand=button('Expand',()=>{root.classList.toggle('dot-expanded');expand.textContent=root.classList.contains('dot-expanded')?'Shrink':'Expand';});
 button('Resources',()=>{disposeResources();inventory.replaceChildren();inventory.hidden=!inventory.hidden;if(inventory.hidden)return;const id=devices.value;
  disposeResources=mountDeviceResources(inventory,{device:{id,name:devices.selectedOptions[0]?.textContent},sessionLabels:labels,request:path=>path.endsWith('resources')?client.resources(id):client.sessions(id)});
 });
 button('Refresh',()=>refresh());
 const rename=make('form');rename.hidden=true;const name=make('input');name.setAttribute('aria-label','Session name');name.maxLength=80;const save=make('button','Save name');save.type='submit';rename.append(name,save);rename.onsubmit=async e=>{e.preventDefault();try{await client.rename({device:devices.value,id:sessions.value,name:name.value.trim()});rename.hidden=true;await refresh();}catch(err){notice(err);}};
 root.append(toolbar,rename,status,inventory,content);
 async function select(){const own=++epoch;const old=view;view=null;if(old)await old.dispose().catch(notice);if(disposed||own!==epoch||!sessions.value)return;
  view=mountTerminal(content,{client,session:{device:devices.value,id:sessions.value},appearance,onState:s=>{if(own===epoch&&!disposed){status.textContent=s.message||s.state+(s.controller?' · Typing here':' · Watching');onState(s);}}});view.setPaused(minimized);
 }
 async function loadSessions(preferred){const selected=devices.value,own=++epoch;const previous=view;view=null;if(previous)await previous.dispose().catch(notice);if(disposed||own!==epoch)return;const list=await client.sessions(selected);if(disposed||own!==epoch||selected!==devices.value)return;
  sessions.replaceChildren(...list.map((s,i)=>{const o=make('option',labels[selected+'/'+s.id]||'Terminal '+(i+1));o.value=s.id;return o;}));if(list.some(s=>s.id===preferred))sessions.value=preferred;await select();
 }
 async function refresh(){const list=await client.devices();if(disposed)return;const previous=devices.value,session=sessions.value;labels=await client.labels().catch(()=>({}));if(disposed)return;
  devices.replaceChildren(...list.map(d=>{const o=make('option',d.name);o.value=d.id;o.disabled=d.state!=='connected';return o;}));if(list.some(d=>d.id===previous))devices.value=previous;
  newButton.disabled=!list.find(d=>d.id===devices.value)?.canCreate;await loadSessions(session);
 }
 devices.onchange=()=>{disposeResources();inventory.hidden=true;pending=loadSessions().catch(notice);};sessions.onchange=()=>{pending=select().catch(notice);};
 const ready=refresh().catch(e=>{notice(e);throw e;});ready.catch(()=>{});
 return {ready,refresh,setAppearance:options=>view?.setAppearance(options),async dispose(){disposeResources();disposed=true;++epoch;await pending.catch(()=>{});try{if(view)await view.dispose();}finally{root.remove();}}};
}
