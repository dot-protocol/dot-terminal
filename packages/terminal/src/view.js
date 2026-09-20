import {createTerminalSurface} from './surface.js';
import {framesUnsupported} from './render-flow.js';
import {InputController} from './input-controller.js';
import {bindTerminalInput} from './terminal-input-binding.js';

/** One view, one immutable session reference. Dispose never stops the process. */
export function mountTerminal(element,{client,session,appearance={},onState=()=>{}}) {
 if(!element?.append)throw new TypeError('A mount element is required');
 const remote=client.session(session),surface=createTerminalSurface(appearance),term=surface.terminal;
 const root=document.createElement('div');root.className='dot-terminal-surface';element.append(root);surface.mount(root);
 let disposed=false,paused=false,generation=0,sequence=1,offset=0,incarnation='',timer,blocked=false,retry=100;
 let chain=Promise.resolve(),resizing=false,resizeAgain=false;
 const emit=(state,extra={})=>{if(!disposed)onState({schema:'dot.view.v1',state,controller:!!generation,offset,...extra});};
 const serial=fn=>{const task=chain.then(()=>disposed?undefined:fn());chain=task.catch(()=>{});return task;};
 const write=data=>new Promise(resolve=>term.write(data,resolve));
 const input=new InputController({canSend:()=>!disposed&&!paused&&!document.hidden&&!!generation,
  send:async data=>{const g=generation,n=sequence;await remote.operation({type:'input',generation:g,sequence:n,data:Array.from(data)});if(g===generation)sequence=n+1;},
  onState:s=>{if(s.condition==='uncertain'){generation=0;emit('input-uncertain');}}
 });
 const unbind=bindTerminalInput({term,surface:root,controller:input,canDrop:()=>!!generation&&!paused,notify:message=>emit('notice',{message})});
 async function resize(){
  if(resizing){resizeAgain=true;return;}
  if(disposed||paused||!generation||root.clientWidth<1||root.clientHeight<1)return;
  resizing=true;
  try{await serial(async()=>{if(!generation||paused)return;const d=surface.fit.proposeDimensions();if(!d)return;
   const cols=Math.max(2,Math.min(240,d.cols)),rows=Math.max(1,Math.min(100,d.rows));if(cols===term.cols&&rows===term.rows)return;
   await write('');if(disposed)return;term.resize(cols,rows);
   try{await remote.operation({type:'resize',generation,cols,rows});}catch(e){generation=0;input.reset('resize-failed');throw e;}
  });}catch(e){emit('error',{message:e.message});}finally{resizing=false;if(resizeAgain){resizeAgain=false;void resize();}}
 }
 const observer=new ResizeObserver(()=>{void resize();});observer.observe(root);
 async function poll(){
  if(disposed||blocked)return;
  try{if(!paused&&!document.hidden)await serial(async()=>{
   const r=await remote.operation({type:'read_frame',after:offset});if(disposed)return;
   if(incarnation&&incarnation!==r.incarnation){generation=0;input.reset('stream-restarted');offset=0;term.reset();incarnation=r.incarnation;emit('restarted');return;}
   incarnation=r.incarnation;
   if(r.gap){blocked=true;generation=0;input.reset('history-gap');emit('history-gap',{message:'Retained output is incomplete. A styled checkpoint is required; this view will not fabricate a screen.'});return;}
   if(!generation&&r.cols>0&&r.rows>0&&(term.cols!==r.cols||term.rows!==r.rows))term.resize(r.cols,r.rows);
   await write(new Uint8Array(r.data));if(disposed)return;offset=r.next;
   if(generation){const c=await remote.operation({type:'check_control',generation});if(c.type==='error')throw new Error(c.message);}
   retry=100;emit('connected');
  });}catch(e){retry=Math.min(5000,retry*2);if(framesUnsupported(e)){blocked=true;emit('upgrade-required',{message:'This view requires a current DOT keeper.'});return;}if(e.code==='controller-fenced'){generation=0;input.reset('fenced');}emit('disconnected',{message:e.message});}
  finally{if(!disposed&&!blocked)timer=setTimeout(poll,paused||document.hidden?1000:retry);}
 }
 poll();
 return Object.freeze({
  async takeControl({takeover=false}={}){if(disposed||blocked)throw new Error('View unavailable');await serial(async()=>{if(generation)return;const r=await remote.operation({type:'acquire',takeover});if(disposed){await remote.operation({type:'release',generation:r.generation});return;}generation=r.generation;sequence=1;input.reset();emit('connected');});await resize();if(!disposed)term.focus();},
  async release(){const g=generation;generation=0;input.reset('released');if(g)await remote.operation({type:'release',generation:g});emit('connected');},
  setPaused(value){paused=!!value;root.hidden=paused;if(paused)input.reset('paused');else void resize();emit(paused?'paused':'connected');},
  setAppearance(options){Object.assign(term.options,options);void resize();},
  focus(){if(!disposed&&!paused)term.focus();},
  paste(text){if(!generation)throw new Error('Read-only view');term.paste(text);},
  async dispose(){if(disposed)return;disposed=true;clearTimeout(timer);observer.disconnect();unbind();input.reset('disposed');const g=generation;generation=0;
   await chain;surface.dispose();root.remove();if(g)await remote.operation({type:'release',generation:g});},
 });
}
