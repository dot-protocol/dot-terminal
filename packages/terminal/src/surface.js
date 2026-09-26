import {Terminal} from '@xterm/xterm';
import {FitAddon} from '@xterm/addon-fit';
/** Renderer ownership without host navigation, global selectors or network authority. */
export function createTerminalSurface(options={}) {
 const terminal=new Terminal({fontSize:13,cursorBlink:true,scrollback:6000,screenReaderMode:true,...options});
 const fit=new FitAddon();terminal.loadAddon(fit);
 let mounted=false,disposed=false;
 return Object.freeze({terminal,fit,
  mount(element){if(disposed||mounted)throw new Error('Surface cannot be mounted twice');terminal.open(element);mounted=true;},
  dispose(){if(disposed)return;disposed=true;terminal.dispose();},
 });
}
