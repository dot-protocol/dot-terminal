// Keep the entire burst behind control acquisition, including the period after
// the lease arrives but before its resize ACK. Newer keys must not overtake it.
export function createInputGate({ready,acquire,submit,context,notify=()=>{}}){
 let pending=null;
 return text=>{
  if(!text)return true;
  const epoch=context();
  if(pending?.epoch!==epoch)pending=null;
  if(!pending&&ready())return submit(text);
  if(!pending)pending={epoch,values:[],bytes:0,started:false};
  const batch=pending,bytes=new TextEncoder().encode(text).length;
  if(batch.bytes+bytes>1024*1024){notify('Input too large; nothing more was queued');return false;}
  batch.values.push(text);batch.bytes+=bytes;
  if(!batch.started){batch.started=true;Promise.resolve().then(acquire).then(ok=>{
   if(pending!==batch||context()!==batch.epoch)return;
   pending=null;if(ok)submit(batch.values.join(''));else notify('Typing was not sent; take control and try again');
  }).catch(()=>{if(pending===batch){pending=null;notify('Typing was not sent; connection unavailable');}});}
  return true;
 };
}
