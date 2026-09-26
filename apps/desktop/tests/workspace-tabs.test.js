import test from 'node:test';
import assert from 'node:assert/strict';
import {renderWorkspaceTabs} from '../src/workspace-tabs.js';
class Element {
 constructor(){this.children=[];this.dataset={};this.attrs={};this.className='';this.classList={toggle:(name,on)=>{const c=new Set(this.className.split(' '));if(on)c.add(name);else c.delete(name);this.className=[...c].join(' ');}};}
 append(...nodes){for(const node of nodes){node.remove();node.parent=this;this.children.push(node);}}
 insertBefore(node,before){node.remove();node.parent=this;const i=before?this.children.indexOf(before):this.children.length;this.children.splice(i,0,node);}
 remove(){if(this.parent){this.parent.children=this.parent.children.filter(n=>n!==this);this.parent=null;}}
 setAttribute(k,v){this.attrs[k]=v;}
 querySelector(selector){const cls=selector.slice(1);for(const c of this.children){if(c.className.split(' ').includes(cls))return c;const nested=c.querySelector(selector);if(nested)return nested;}return null;}
}
test('refresh preserves tab hit targets; close addresses exactly its own session',()=>{
 const old=globalThis.document;globalThis.document={createElement:()=>new Element()};
 try{
  const root=new Element(),calls=[];
  const entries=[{id:'a',device:'vps',name:'Bobby',deviceName:'Server'},{id:'b',device:'vps',name:'Taylor',deviceName:'Server'}];
  const options={activeKey:'vps/a',onSelect:i=>calls.push(['select',i.id]),onClose:i=>calls.push(['close',i.id]),onRename:()=>{}};
  renderWorkspaceTabs(root,entries,options);const first=root.children[0],second=root.children[1],close=first.children[1];
  renderWorkspaceTabs(root,[{...entries[0],name:'Bobby renamed'},entries[1]],options);
  assert.equal(root.children[0],first);assert.equal(root.children[1],second);assert.equal(first.children[1],close);
  close.onclick({stopPropagation(){}});assert.deepEqual(calls,[['close','a']]);assert.match(close.attrs['aria-label'],/Bobby renamed.*keep process running/);
  second.children[0].onclick();assert.deepEqual(calls[1],['select','b']);
  renderWorkspaceTabs(root,[entries[1]],options);assert.deepEqual(root.children,[second]);
 }finally{globalThis.document=old;}
});
