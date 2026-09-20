export interface SessionRef {device?:string;id:string}
export interface Device {id:string;name:string;kind:string;local:boolean;state:string;canCreate:boolean}
export interface Session {id:string;exited:boolean;pid:number|null;usage:unknown}
export interface Client {
 devices():Promise<Device[]>; sessions(device?:string):Promise<Session[]>;
 create(device?:string):Promise<{session:string}>;
 labels():Promise<Record<string,string>>;
 rename(value:SessionRef & {name:string}):Promise<unknown>;
 resources(device?:string):Promise<Record<string,unknown>>;
 session(ref:SessionRef):{readonly device:string;readonly id:string;operation(op:Record<string,unknown>):Promise<any>};
}
export type Request = (path:string,body?:unknown)=>Promise<any>;
export function createClient(options:{request:Request}):Client;
export function createTransport(options:{fetcher?:typeof fetch;capability?:string;timeout?:number;bridge?:{request(id:string,data:string):void};receive?:(fn:(id:string,result:any)=>void)=>void;storage?:Storage}):Request;
export {InputController} from './input-controller.js';
