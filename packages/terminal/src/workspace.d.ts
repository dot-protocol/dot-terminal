import type {ITerminalOptions} from '@xterm/xterm';
import type {Client} from './index.js';
import type {ViewState} from './view.js';
export function mountWorkspace(element:HTMLElement,options:{client:Client;appearance?:ITerminalOptions;onState?:(state:ViewState)=>void}):{ready:Promise<void>;refresh():Promise<void>;setAppearance(options:ITerminalOptions):void;dispose():Promise<void>};
