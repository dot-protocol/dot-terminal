export const themes = {
 forest: {label:'Forest', bg:'#111519', surface:'#171d20', raised:'#22312b', text:'#d4dedc', muted:'#a1b3a9', accent:'#adf4cf', border:'#405349', selection:'#35554e'},
 midnight: {label:'Midnight', bg:'#101424', surface:'#191f34', raised:'#293452', text:'#e0e7ff', muted:'#a5b3d6', accent:'#9dbaff', border:'#435273', selection:'#344c79'},
 paper: {label:'Paper', bg:'#faf8f2', surface:'#eeece3', raised:'#e1e7dc', text:'#202d29', muted:'#52665b', accent:'#176544', border:'#a2b2a7', selection:'#bedacb'},
 contrast: {label:'High contrast', bg:'#000000', surface:'#111111', raised:'#262626', text:'#ffffff', muted:'#cccccc', accent:'#ffff70', border:'#aaaaaa', selection:'#555555'},
};
export const defaults = {theme:'forest', terminalSize:14, uiSize:14, lineHeight:1.25, font:'"SF Mono", Menlo, monospace', reducedNoise:false};
const number = (value, min, max, fallback) => Number.isFinite(Number(value)) && value !== '' ? Math.min(max,Math.max(min,Number(value))) : fallback;
export function normalize(value={}) {
 if(!value || typeof value!=='object') value={};
 return {theme:Object.hasOwn(themes,value.theme)?value.theme:defaults.theme,
 terminalSize:number(value.terminalSize,8,40,defaults.terminalSize), uiSize:number(value.uiSize,12,24,defaults.uiSize),
 lineHeight:number(value.lineHeight,1,2,defaults.lineHeight),
 font:typeof value.font==='string' && /^[\w\s,"'-]{1,120}$/.test(value.font.trim()) ? value.font.trim() : defaults.font,
 reducedNoise:value.reducedNoise===true};
}
export function terminalTheme(name) {const t=themes[name]||themes.forest;return {background:t.bg,foreground:t.text,cursor:t.accent,selectionBackground:t.selection,black:t.surface,red:name==='paper'?'#a02a23':'#ef8f87',green:name==='paper'?'#23653f':'#adf4cf',yellow:name==='paper'?'#785600':'#ead9a0',blue:name==='paper'?'#235c99':'#92bce6',magenta:name==='paper'?'#7940a0':'#c8a6e3',cyan:name==='paper'?'#24666b':'#95d7d8',white:t.text};}
export function loadAppearance(storage) {try{return normalize(JSON.parse(storage.getItem('dot-appearance-v1')||'{}'));}catch{return {...defaults};}}
export function applyAppearance(value, term, storage) {
 const settings=normalize(value),t=themes[settings.theme],root=document.documentElement;
 for(const [key,val] of Object.entries(t)) if(key!=='label')root.style.setProperty('--'+key,val);
 root.style.setProperty('--ui-size',settings.uiSize+'px');root.dataset.noise=settings.reducedNoise?'quiet':'full';root.dataset.theme=settings.theme;
 term.options.fontFamily=settings.font;term.options.fontSize=settings.terminalSize;term.options.lineHeight=settings.lineHeight;term.options.theme=terminalTheme(settings.theme);
 try{storage.setItem('dot-appearance-v1',JSON.stringify(settings));}catch{/* Settings remain usable if persistence is unavailable. */}
 return settings;
}
