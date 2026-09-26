// Existing AXXIS sessions retain their original process owner. This view never
// claims DOT sequence acknowledgements or full replay. Input is fenced by the DOT
// gateway: a view observes until it takes control, and loses it when another view
// takes over (the gateway drops its keystrokes and says so).
export class ExternalSession {
 constructor({url,token,write,onState,onControlLost=()=>{},WebSocketClass=WebSocket}) {
  this.write=write;this.onState=onState;this.ready=false;this.pending=0;this.disposed=false;this.chain=Promise.resolve();
  const ws=this.ws=new WebSocketClass(url);ws.binaryType='arraybuffer';
  ws.onopen=()=>{ws.send(JSON.stringify({type:'auth',token}));onState('Connecting to existing session…');};
  ws.onmessage=e=>{
   if(typeof e.data==='string') {
    let v;try{v=JSON.parse(e.data);}catch{return;}
    if(v.type==='replay_complete')this.chain.then(()=>{if(!this.disposed&&ws.readyState===1){this.ready=true;onState('Watching · existing host controls this session');}});
    if(v.type==='capabilities')this.resizeControl=v.resize_control===true;
    if(v.type==='resize_control'){this.claimReply?.(v.granted===true);this.claimReply=null;if(!v.granted)onControlLost();}
    if(v.type==='upstream_unavailable'){this.ready=false;onState('Existing terminal service unavailable');}
    if(v.type==='control'){
     if(this.grantReply){this.grantReply(v);this.grantReply=null;}
     else if(v.state==='observing'&&this.controlling){this.controlling=false;onControlLost(v.reason||'Another view is typing in this session');}
    }
    return;
   }
   const bytes=new Uint8Array(e.data);this.pending+=bytes.length;
   if(this.pending>4*1024*1024){this.ready=false;ws.close();onState('View fell behind · reopen to recover available history');return;}
   if(this.pending>256*1024&&!this.paused){this.paused=true;this.control({type:'pause'});}
   this.chain=this.chain.then(()=>this.disposed?undefined:write(bytes)).finally(()=>{
    this.pending-=bytes.length;if(this.paused&&this.pending<64*1024){this.paused=false;this.control({type:'resume'});}
   }).catch(()=>{this.ready=false;ws.close();onState('Rendering failed · input disabled · reopen this view');});
  };
  ws.onclose=()=>{this.ready=false;if(!this.disposed)onState('Disconnected · process may still be running · reopen to reconnect');};
  ws.onerror=()=>{this.ready=false;if(!this.disposed)onState('Connection failed · input disabled');};
 }
 control(message){if(this.ws.readyState===1)this.ws.send(JSON.stringify(message));}
 input(bytes){if(!this.ready||this.ws.readyState!==1||this.ws.bufferedAmount>64*1024)throw new Error('Input not sent: existing session is not ready');this.ws.send(bytes);}
 /** Ask the gateway for input control, then the owner for resize control. Rejects with the reason. */
 acquire(takeover=false){
  return new Promise((resolve,reject)=>{
   const timer=setTimeout(()=>{this.grantReply=null;reject(new Error('Control was not acknowledged'));},3000);
   this.grantReply=v=>{clearTimeout(timer);if(v.state==='granted'){this.controlling=true;resolve();}else reject(new Error(v.reason||'Another view is typing in this session. Take over to type here.'));};
   this.control({type:'take_control',takeover});
  }).then(()=>this.acquireResize(takeover));
 }
 release(){this.controlling=false;this.control({type:'release_control'});this.releaseResize();}
 acquireResize(takeover=false){if(!this.resizeControl)return Promise.resolve();return new Promise((resolve,reject)=>{const timer=setTimeout(()=>{this.claimReply=null;reject(new Error('Resize ownership was not acknowledged'));},3000);this.claimReply=ok=>{clearTimeout(timer);ok?resolve():reject(new Error('Another view controls the terminal size. Take over to resize.'));};this.control({type:'claim_resize',takeover});});}
 releaseResize(){if(this.resizeControl)this.control({type:'release_resize'});}
 resize(cols,rows){if(this.ready)this.control({type:'resize',cols,rows});}
 dispose(){this.disposed=true;this.ready=false;this.ws.close();}
}
