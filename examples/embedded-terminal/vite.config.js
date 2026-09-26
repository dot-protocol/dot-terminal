import {defineConfig} from 'vite';
// Development only: fixed owner-selected loopback hub; rewrite Origin/Host for its
// existing exact-origin gate. Never deploy this development proxy as a public service.
const target=process.env.DOT_PREVIEW_HUB||'http://127.0.0.1:58984';
const url=new URL(target);if(url.protocol!=='http:'||url.hostname!=='127.0.0.1')throw new Error('Preview hub must be literal loopback');
export default defineConfig({server:{proxy:{'/api':{target,changeOrigin:true,configure(proxy){proxy.on('proxyReq',req=>{req.setHeader('origin',url.origin);});}}}}});
