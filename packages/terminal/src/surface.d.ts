import {Terminal,ITerminalOptions} from '@xterm/xterm';
import {FitAddon} from '@xterm/addon-fit';
export function createTerminalSurface(options?:ITerminalOptions):{terminal:Terminal;fit:FitAddon;mount(element:HTMLElement):void;dispose():void};
