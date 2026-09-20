import {normalizeDevices,normalizeSessions,sessionsPath,sessionPath} from './devices.js';
export {createTransport} from './transport.js';
export {InputController} from './input-controller.js';

/** Host-owned authenticated transport. No global credentials, storage or browser required. */
export function createClient({request}) {
 if(typeof request!=='function')throw new TypeError('An authenticated request function is required');
 async function call(path,body){
  const value=await request(path,body);
  if(value?.type==='error'||value?.error){const e=new Error(value.message||value.error);e.code=value.message==='stale controller generation'?'controller-fenced':String(value.message).startsWith('controller busy')?'controller-busy':'keeper-error';throw e;}
  return value;
 }
 const valid=(s)=>{if(typeof s!=='string'||!/^[a-zA-Z0-9-]{1,64}$/.test(s))throw new TypeError('Invalid resource identifier');return s;};
 return Object.freeze({
  devices:async()=>normalizeDevices(await call('devices')),
  sessions:async(device='local')=>normalizeSessions(await call(sessionsPath(valid(device)))),
  create:async(device='local')=>call(sessionsPath(valid(device)),{}),
  labels:()=>call('session-labels'),
  rename:({device='local',id,name})=>call('session-labels',{device:valid(device),id:valid(id),name}),
  resources:(device='local')=>call(device==='local'?'resources':'devices/'+valid(device)+'/resources'),
  session:({device='local',id})=>{const path=sessionPath(valid(device),valid(id));return Object.freeze({device,id,operation:op=>call(path,op)});},
 });
}
