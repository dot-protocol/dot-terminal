// A session's doorbell: the gateway says the moment new output lands, and the view pulls it through
// read_frame at once instead of waiting for its next timer. Bytes never arrive here; read_frame stays the
// only path that applies them. `live` is true only while notes can arrive, so the caller can relax its
// idle polling then and fall back to it the moment the doorbell is gone or unsupported.
export function createDoorbell({url,token,after,onOutput,onChange=()=>{},WebSocketClass=WebSocket}){
 const bell={live:false,supported:true,closed:false};
 const ws=new WebSocketClass(url);
 const set=(live)=>{if(bell.live!==live){bell.live=live;onChange(bell);}};
 ws.onopen=()=>{ws.send(JSON.stringify({type:'auth',token}));ws.send(JSON.stringify({type:'subscribe',after}));set(true);};
 ws.onmessage=e=>{
  let v;try{v=JSON.parse(e.data);}catch{return;}
  if(v.type==='output')onOutput(v);
  else if(v.type==='unsupported'){bell.supported=false;set(false);ws.close();}
  else if(v.type==='exited'){onOutput(v);set(false);}
 };
 ws.onclose=()=>{set(false);};
 ws.onerror=()=>{set(false);};
 bell.close=()=>{bell.closed=true;set(false);try{ws.close();}catch{}};
 return bell;
}
