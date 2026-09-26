// Keyed tab controls retain their hit targets while the catalog refreshes.
export function renderWorkspaceTabs(root, entries, {activeKey,onSelect,onClose,onRename}) {
 const existing=new Map([...root.children].map(el=>[el.dataset.key,el]));
 for(const [index,item] of entries.entries()){
  const key=`${item.device}/${item.id}`;let group=existing.get(key);
  if(!group){
   group=document.createElement('div');group.className='terminal-tab-group';group.dataset.key=key;
   const select=document.createElement('button');select.className='session-tab';
   const dot=document.createElement('i');dot.className='session-dot';
   const label=document.createElement('span');label.className='tab-label';
   const name=document.createElement('span');name.className='tab-name';
   const device=document.createElement('small');device.className='tab-device';label.append(name,device);select.append(dot,label);
   const close=document.createElement('button');close.className='tab-close';close.textContent='×';
   group.append(select,close);
  }
  existing.delete(key);group.classList.toggle('selected',key===activeKey);
  const [select,close]=group.children;
  select.setAttribute('aria-label',`Open ${item.name} on ${item.deviceName}`);
  select.setAttribute('aria-pressed',String(key===activeKey));
  select.querySelector('.tab-name').textContent=item.name;select.querySelector('.tab-device').textContent=item.deviceName;
  select.querySelector('.session-dot').dataset.state=item.exited?'ended':'running';
  select.title=`${item.name} · ${item.deviceName}`;
  select.onclick=()=>onSelect(item);select.ondblclick=()=>onRename(item);
  close.setAttribute('aria-label',`Close ${item.name} tab; keep process running`);
  close.title=`Close ${item.name} tab in connected views. Its process keeps running.`;
  close.onclick=e=>{e.stopPropagation();onClose(item);};
  if(root.children[index]!==group)root.insertBefore(group,root.children[index]||null);
 }
 for(const group of existing.values())group.remove();
}
