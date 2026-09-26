// The same UI runs in a loopback browser or a bundled Android WebView. The native
// transport holds the device identity; no host capability or private key enters JS.
export function createTransport({bridge, receive, storage, fetcher, capability, timeout=12000}) {
 const pending=new Map();let sequence=0;
 if(bridge)receive((id,result)=>{const p=pending.get(id);if(!p)return;pending.delete(id);clearTimeout(p.timer);result.error?p.reject(new Error(result.error)):p.resolve(result);});
 return async (path,body)=>{
  if(bridge&&path==='ui-state') {if(body!==undefined)storage.setItem('dot-workspace-state',JSON.stringify(body));return JSON.parse(storage.getItem('dot-workspace-state')||'{}');}
  let status,value;
  if(bridge){
   if(!/^(devices|sessions|session-labels)(\/|$)/.test(path)){const e=new Error('This service is available only on the host');e.status=404;throw e;}
   const result=await new Promise((resolve,reject)=>{const id=String(++sequence);const timer=setTimeout(()=>{pending.delete(id);reject(new Error('Device connection timed out; input was not retried'));},timeout);pending.set(id,{resolve,reject,timer});try{bridge.request(id,JSON.stringify({path,body:body??null}));}catch(e){clearTimeout(timer);pending.delete(id);reject(e);}});
   status=result.status;value=result.body;
  }else{
   const r=await fetcher('/api/'+path,{method:body===undefined?'GET':'POST',headers:{Authorization:'Bearer '+capability,'Content-Type':'application/json'},body:body===undefined?undefined:JSON.stringify(body),signal:AbortSignal.timeout(timeout)});
   status=r.status;const text=await r.text();try{value=JSON.parse(text);}catch{value={error:text};}
  }
  // The host answered, so this is never "connection refused". A 401 means this page was opened without
  // its access link (a bare URL), which only DOT itself can hand out.
  if(status<200||status>=300){const e=new Error(status===401?'This page needs its access link. Open it from DOT on this Mac.':value?.error||`The host refused the request (${status})`);e.status=status;throw e;}
  return value;
 };
}
