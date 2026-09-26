// Same native mTLS request bridge as DOT keepers. No capability enters the phone UI.
export function nativeStreamSocket(request,path){
 return class NativeStream {
  constructor(){this.readyState=0;this.bufferedAmount=0;this.after=0;this.chain=Promise.resolve();this.open();}
  async open(){try{const r=await request(path,{op:'open'});this.view=r.view;if(this.readyState===3){await request(path,{op:'close',view:this.view});return;}this.readyState=1;this.onopen?.();this.poll();}catch{this.fail();}}
  async poll(){while(this.readyState===1){try{const r=await request(path,{op:'read',view:this.view,after:this.after});for(const {seq,frame} of r.frames){if(this.readyState!==1)return;if(seq!==this.after+1)throw Error('Stream gap');this.after=seq;const data=frame.data?Uint8Array.from(frame.data.match(/.{2}/g)||[],h=>parseInt(h,16)).buffer:frame.control;this.onmessage?.({data});}if(r.closed){this.fail();return;}await new Promise(resolve=>setTimeout(resolve,120));}catch{this.fail();return;}}}
  send(value){if(this.readyState!==1)throw Error('Disconnected');let op;
   if(typeof value==='string'){const control=JSON.parse(value);if(control.type==='auth')return;op={control};}
   else {const bytes=value instanceof Uint8Array?value:new Uint8Array(value);op={data:Array.from(bytes,b=>b.toString(16).padStart(2,'0')).join('')};}
   const size=typeof value==='string'?value.length:value.byteLength;this.bufferedAmount+=size;
   this.chain=this.chain.then(async()=>{if(this.readyState!==1)throw Error('Disconnected');await request(path,{op:'send',view:this.view,...op});}).catch(()=>this.fail()).finally(()=>{this.bufferedAmount-=size;});
  }
  fail(){if(this.readyState===3)return;this.close();this.onerror?.();this.onclose?.();}
  close(){if(this.readyState===3)return;this.readyState=3;if(this.view)request(path,{op:'close',view:this.view}).catch(()=>{});}
 };
}
