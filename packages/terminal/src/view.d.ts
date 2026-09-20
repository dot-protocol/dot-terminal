import type {ITerminalOptions} from '@xterm/xterm';
import type {Client,SessionRef} from './index.js';
export interface ViewState {schema:'dot.view.v1';state:string;controller:boolean;offset:number;message?:string}
export interface TerminalView {
 takeControl(options?:{takeover?:boolean}):Promise<void>;release():Promise<void>;
 setPaused(paused:boolean):void;setAppearance(options:ITerminalOptions):void;
 focus():void;paste(text:string):void;dispose():Promise<void>;
}
export function mountTerminal(element:HTMLElement,options:{client:Client;session:SessionRef;appearance?:ITerminalOptions;onState?:(state:ViewState)=>void}):TerminalView;
