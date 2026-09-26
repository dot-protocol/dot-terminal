// Same native mTLS request bridge as DOT keepers. No capability enters the phone UI.
export function nativeStreamSocket(request,path){
 return class NativeStream {
  constructor(){this.readyState=0;this.bufferedAmount=0;this.after=0;this.chain=Promise.resolve();this.open();}
  async open(){try{const r=await request(path,{op:'open'});this.view=r.view;if(this.readyState===3){await request(path,{op:'close',view:this.view});return;}this.readyState=1;this.onopen?.();this.poll();}catch{this.fail();}}
  async poll(){while(this.readyState===1){try{const r=await request(path,{op:'read',view:this.view,after:this.after});for(const {seq,frame} of r.frames){if(this.readyState!==1)return;if(seq!==this.after+1)throw Error('Stream gap');this.after=seq;const data=frame.data?Uint8Array.from(frame.data.match(/.{2}/g)||[],h=>parseInt(h,16)).buffer:frame.control;this.onmessage?.({data});}if(r.closed){this.fail();return;}await new Promise(resolve=>setTimeout(resolve,120));}catch{this.fail();return;}}}
  send(value){if(this.readyState!==1)throw Error('Disconnected');let op;
   if(typeof value==='string'){const control=JSON.parse(value);if(control.type==='auth')return;
    // Control is its own operation on the poll transport; answer with the same message a socket would.
    if(control.type==='take_control'||control.type==='release_control'){
     const take=control.type==='take_control';
     this.chain=this.chain.then(async()=>{let reply;
      try{const r=await request(path,{op:control.type,view:this.view,...(take?{takeover:control.takeover===true}:{})});reply=take?{type:'control',state:'granted',generation:r.generation}:{type:'control',state:'observing'};}
      catch(e){if(e?.status!==409&&e?.status!==403)throw e;reply={type:'control',state:take?'refused':'observing',reason:e.message};}
      this.onmessage?.({data:JSON.stringify(reply)});}).catch(()=>this.fail());
     return;
    }
    op={control};}
   else {const bytes=value instanceof Uint8Array?value:new Uint8Array(value);op={data:Array.from(bytes,b=>b.toString(16).padStart(2,'0')).join('')};}
   const size=typeof value==='string'?value.length:value.byteLength;this.bufferedAmount+=size;
   this.chain=this.chain.then(async()=>{if(this.readyState!==1)throw Error('Disconnected');
    // An observer's input is refused (403), not a broken stream: report it as a control message.
    try{await request(path,{op:'send',view:this.view,...op});}catch(e){if(e?.status!==403)throw e;this.onmessage?.({data:JSON.stringify({type:'control',state:'observing',reason:e.message})});}
   }).catch(()=>this.fail()).finally(()=>{this.bufferedAmount-=size;});
  }
  fail(){if(this.readyState===3)return;this.close();this.onerror?.();this.onclose?.();}
  close(){if(this.readyState===3)return;this.readyState=3;if(this.view)request(path,{op:'close',view:this.view}).catch(()=>{});}
 };
}
