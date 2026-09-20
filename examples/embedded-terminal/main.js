import {createClient,createTransport} from '@dot-protocol/terminal';
import {mountWorkspace} from '@dot-protocol/terminal/workspace';
import '@dot-protocol/terminal/style.css';
let views=[];
document.querySelector('#connect').onsubmit=async e=>{e.preventDefault();try{
 await Promise.all(views.map(v=>v.dispose()));views=[];
 const field=document.querySelector('#token');const capability=field.value;field.value='';
 const client=createClient({request:createTransport({fetcher:fetch.bind(window),capability})});
 views=['first','second'].map(id=>mountWorkspace(document.getElementById(id),{client,appearance:{theme:{background:'#111519',foreground:'#d4dedc'}}}));
 await Promise.all(views.map(v=>v.ready));document.querySelector('#connect').hidden=true;document.querySelector('#disconnect').hidden=false;
 document.querySelector('#message').textContent='Connected. Processes stay on their devices when a view is closed.';
 }catch(error){document.querySelector('#message').textContent=error.message;}};

document.querySelector('#disconnect').onclick=async()=>{await Promise.all(views.map(v=>v.dispose()));views=[];document.querySelector('#connect').hidden=false;document.querySelector('#disconnect').hidden=true;document.querySelector('#message').textContent='Views disconnected. Reconnect to see the same surviving sessions.';};
