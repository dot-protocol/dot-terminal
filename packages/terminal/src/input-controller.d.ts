export interface InputState {condition:string;refusal:string|null;queuedBytes:number}
export class InputController {
 constructor(options:{send:(bytes:Uint8Array)=>Promise<void>;canSend:()=>boolean;onState?:(state:InputState)=>void;clock?:()=>number;sleep?:(ms:number)=>Promise<void>});
 submit(text:string):boolean;reset(reason?:string):void;state():InputState;sawRepeat():void;snapshot():Record<string,unknown>;
}
export function readDrop(transfer:DataTransfer):{kind:string;text?:string;count?:number};
export function chunkUtf8(bytes:Uint8Array,max:number):Uint8Array[];
