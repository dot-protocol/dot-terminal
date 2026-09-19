import {accessoryKey} from './terminal-input.js';
export function installKeyDock(root,term,sendInput,copyIndex){
// Accessible, compact controls keep terminal keystrokes on the same fenced input path.
const dock=document.createElement('div');dock.className='key-dock';
const toggle=document.createElement('button');toggle.className='key-dot';toggle.textContent='●';toggle.setAttribute('aria-label','Terminal keys');toggle.setAttribute('aria-expanded','false');
const keyMenu=document.createElement('div');keyMenu.className='key-menu';keyMenu.hidden=true;keyMenu.setAttribute('role','group');keyMenu.setAttribute('aria-label','Terminal keys');
for(const [label,title,value] of [['Esc','Escape','\x1b'],['⇥','Tab','\t'],['^C','Interrupt','\x03'],['^D','End of input','\x04'],['↑','Arrow up','\x1b[A'],['↓','Arrow down','\x1b[B'],['←','Arrow left','\x1b[D'],['→','Arrow right','\x1b[C']]){
 const b=document.createElement('button');b.textContent=label;b.title=title;b.setAttribute('aria-label',title);b.onclick=()=>{sendInput(accessoryKey(value,term.modes.applicationCursorKeysMode));term.focus();};keyMenu.append(b);copyIndex.push({id:'action.keys.'+title.toLowerCase().replaceAll(' ','-'),kind:'static',role:'primary',signal:'signal',privacy:'operational',text:title,words:title.toLowerCase().split(' ')});
}
toggle.onclick=()=>{keyMenu.hidden=!keyMenu.hidden;toggle.setAttribute('aria-expanded',String(!keyMenu.hidden));};dock.append(keyMenu,toggle);root.append(dock);
}
