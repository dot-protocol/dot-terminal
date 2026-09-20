import {createClient, createTransport} from '../src/index.js';
import {mountTerminal} from '../src/view.js';
import {mountWorkspace} from '../src/workspace.js';
const client=createClient({request:createTransport({fetcher:fetch,capability:'test-only'})});
const container=document.createElement('div');
const workspace=mountWorkspace(container,{client});
const view=mountTerminal(container,{client,session:{device:'core',id:'test-session'},onState:s=>console.log(s.state)});
void workspace.ready;void view.release();void view.dispose();
