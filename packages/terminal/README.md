# DOT Terminal

An embeddable device/session workspace and a headless client for the DOT Rust node.
Apache-2.0. Version `0.1.0-alpha.1`: installable alpha, not an unrestricted public
remote-access service. Rendering uses xterm under the package boundary (MIT).

## Install from this repository

```sh
npm ci --prefix packages/terminal
npm pack ./packages/terminal
# In your application:
npm install /path/to/dot-protocol-terminal-0.1.0-alpha.1.tgz
```

The package is not yet published to npm. It contains only public source, types and
licenses, with pinned renderer dependencies. No operator files, machine inventory,
credentials, native binaries or running sessions are embedded in the tarball.

## Embed the whole workspace

```js
import {createClient} from '@dot-protocol/terminal';
import {mountWorkspace} from '@dot-protocol/terminal/workspace';
import '@dot-protocol/terminal/style.css';

const client = createClient({
  request: async (path, body) => {
    // Your authenticated backend/native adapter enforces this user's node grants.
    const response = await fetch('/my-dot-api/' + path, {
      method: body === undefined ? 'GET' : 'POST',
      headers: {'Content-Type': 'application/json'},
      credentials: 'same-origin',
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (!response.ok) throw new Error('DOT request refused');
    return response.json();
  },
});
const workspace = mountWorkspace(document.querySelector('#terminal'), {client});
await workspace.ready;
// When the host component unmounts:
await workspace.dispose(); // Detaches views, does NOT terminate processes.
```

Give the container an explicit height. Mount multiple independent workspaces in the
same document. No global IDs, page navigation, storage, document-wide drop handlers
or implicit access to device credentials. Theme the outer shell using `--dot-bg` and
`--dot-fg`; pass xterm-compatible `appearance` options for the terminal.

The supplied workspace includes devices, session selection, creation, rename,
explicit Type/Watch controls, minimize/restore, expand/shrink and host-granted resource
summary. Refresh fetches shared labels/catalog; live catalog push is not implemented.
Applications can provide different navigation around the same client and terminal view.

## Embed only a session, retaining your own UI

```js
import {mountTerminal} from '@dot-protocol/terminal/view';
const view = mountTerminal(container, {
  client, session: {device: 'core', id: sessionId},
  onState: state => renderConnectionStatus(state),
});
await view.takeControl(); // Refuses another controller; no automatic takeover.
view.setPaused(true);     // Minimize without stopping the process.
view.setPaused(false);
await view.release();
await view.dispose();
```

Only the controller resizes the PTY. Observers follow its grid and can scroll/pan.
A host wanting an intentional takeover can call `takeControl({takeover:true})` after
its own explicit confirmation. Focus, mounting and selecting do not acquire control.
Input is fenced and ordered; uncertain acknowledgements are not automatically retried.
Data events are operational state only, not terminal text or API bodies.

## Backend and security contract

`request(path, body?)` resolves the existing DOT JSON API. Required operations:
`devices`, `sessions`, `devices/{id}/sessions`, `session-labels`, and keeper operations
at `sessions/{id}` or `devices/{device}/sessions/{id}`. Resource routes are optional,
separately authorized owner APIs. The current paired Android grant excludes inventory.
Use the existing native `createTransport` bridge for Android; keys stay in native code.

A hosted website cannot magically connect to a private Mac. Your service must provide
an authenticated connection to an enrolled node (or a native bridge). Never forward
an owner bearer to an arbitrary website, expose the loopback hub publicly, or turn the
example's development proxy into a production relay. Authentication and node enrollment
remain enforced by DOT and the hosting service, not a frontend identifier.

The session view requires a current keeper with `read_frame` and `check_control`.
Legacy keepers are still supported by the existing DOT app, not this new view.
A history gap stops this view with an explicit `history-gap` state: styled checkpoint
recovery is not finished, and replaying partial ANSI is not a truthful reconstruction.
The existing keeper ring is bounded. This is not a lossless lifetime recording system.

## Independent consumer / acceptance

`examples/embedded-terminal` imports the public package, not desktop source. Start a
separate loopback test hub with a SHORT private state path and a disposable session,
then run `DOT_PREVIEW_HUB=http://127.0.0.1:PORT npm run dev` there. Enter that isolated
hub's token in the local form. Nothing is saved to browser storage.

Verified locally: package installed/imported outside the repository; two independent
workspaces showing one live alternate-screen/color/Unicode process; controller expand
while streaming. Automated client and existing shared-input tests run in CI. These
checks do not establish Android IME, arbitrary third-party SSO/relay deployment,
Windows execution or complete reconnect fidelity. See repository continuity for the
exact tested revision and remaining release gates.
