# VPS workspace connection — development status

DOT's own Rust keeper remains the native session engine. Existing processes owned by
AXXIS cannot be adopted simply by knowing their PID: the existing holder owns the PTY
master, replay buffer and lifecycle. The desktop gateway now supports an explicit
compatibility connection to that holder. It neither uses tmux as DOT's backend nor
restarts existing agents.

## Connection boundary

The private gateway state directory may contain owner-only `external-hosts.json`:

```json
[{"id":"external-server","name":"Existing VPS sessions","url":"http://127.0.0.1:57431/","token":"REPLACE_WITH_EXISTING_PRIVATE_SERVICE_TOKEN","ssh_alias":"my-server","remote_port":7431}]
```

The token must be 64 hexadecimal characters; this example is deliberately invalid.
The file must be owned by the gateway user and inaccessible to group/other users.
Never put real configuration in the repository. Use an existing authorized service
credential; do not enroll a new identity or disable SSH host verification.

`ssh_alias` is optional. When supplied, the gateway supervises a dedicated SSH tunnel
using the owner's existing SSH configuration. It uses batch authentication, normal
known-host verification, loopback-only forwarding, keepalives and a retry delay capped
at 30 seconds. Only the transport reconnects; input is never automatically retried.
Without the alias, the operator must supervise the tunnel separately.

The browser authenticates to DOT in the first WebSocket frame. The upstream credential
stays in the gateway; it is not sent to JavaScript or placed in a URL. Host and Origin
checks precede the upgrade. Failed authentication never attaches an upstream session.

Desktop and web must point at the SAME loopback hub (`--workspace-config` for the Mac
wrapper). Opening a second gateway produces a different workspace. This is an owner-local
connection, not a public website authentication adapter or a LAN-access mechanism.

## User-visible behavior

- Existing live sessions appear under the configured device name; ended sessions are
  excluded from this compatibility catalog.
- Opening a view waits for retained-output replay before allowing input. Rendering is
  serialized; queues are bounded and a failed/lagged view disables input.
- Observers start with the reported PTY grid. Typing views can resize the upstream PTY.
- Workspace tabs are persisted in the Rust gateway. Atomic open/close mutations are
  idempotent, preserve concurrent additions and carry a monotonic revision. Visible
  views poll membership every two seconds; hidden views catch up when active again.
- Close tab removes the workspace tab from connected views and clears selection; it
  sends no process-stop request. The process remains available in the device catalog.
- Stop process is separate. DOT-owned sessions require the shell PID and keeper socket
  to disappear. Compatibility sessions require the upstream owner's `exited` state;
  this is not independent proof that every descendant has exited.
- Session aliases persist in the DOT hub. They do not rename upstream agent identities.

## Verification performed

- Disposable VPS shell: two independent WebSocket clients received real process output;
  after one view disconnected, the other could still type and receive output.
- Explicit VPS stop returned the owner's ended state. No existing agent received test input.
- Wrong WebSocket credentials and cross-origin requests were rejected.
- Interrupting the gateway-owned test SSH tunnel led to automatic transport recovery.
- A separate API client opened and closed a workspace tab; the real browser reflected
  both changes and detached on close. Duplicate open preserved the revision. Gateway
  restart preserved tab state and the Mac keeper; graceful termination removed its SSH child.
- Real web UI: a disposable VPS shell accepted keyboard input and rendered its response.
  The in-app Stop dialog passed cancel and confirm tests; closing the same disposable
  session tab preserved its process. A separate owner-side stop was reconciled into
  the browser catalog and persisted shared tabs.
- Real web UI: Rocky's retained/live output displayed at its reported grid; no input or
  resize was sent to Rocky. A disposable Mac session accepted browser keyboard input,
  displayed command output, survived Close tab, cleared persisted selection, and stopped
  with shell/socket disappearance confirmed by the gateway.
- Workspace Rust tests and strict clippy, desktop UI tests, package tests/type checking,
  desktop web build and independent consumer build passed during this change. A final
  bounded verification after edits is recorded in continuity.

- Real agent rendering: explicitly fitting its shared grid expanded it from 93 to 179
  columns. No test commands were sent. Other legacy controllers may still resize it.
- Tabs retain keyed controls through refresh; close controls name their session and
  keep processes alive. Stop names the session and device before ending it.

## Open release gates

This is not a claim of a completed cross-device release.

1. Native Mac now opens the same hub and its real agent catalog was observed. Full
   native input/resize acceptance remains open; do not infer it from catalog parity.
2. Compatibility output is a raw retained byte stream. It has no historical geometry
   epochs/checkpoints or exactly-once input acknowledgements. Old full-screen replay may
   render incorrectly. Current geometry is sampled, not an ordered resize event stream.
3. Compatibility input has no global controller fencing: other Core Access clients can
   still type or resize. The interface explicitly labels this weaker connection.
4. Shared tabs use two-second polling, not a push channel. Active selection is per
   view. Ordering, offline mutation reconciliation, multiple workspaces and sustained
   multi-client acceptance remain open. Older hubs fall back to per-view tabs.
5. DOT remote hosts running older binaries cannot yet return the new confirmed-stop
   result. The UI reports that uncertainty rather than claiming success.
6. This adapter is currently in the desktop gateway/UI. The public package requires a
   documented transport capability contract before third-party use of this adapter.
7. Android, external hosted web auth, reboot persistence, sustained latency/load, IME,
   microphone/key-release support and recovery from complete history loss remain open.

Next: finish desktop/web acceptance against the same hub, then implement push-based workspace
updates and ordered geometry/replay/control capabilities in DOT's Rust
engine. Replace Core Access only after its live acceptance gates pass.

## Native stream projection

`POST /api/external/{device}/{session}/view` supports open/read/send/close for paired
workspace clients. Handles are random, bound to the configured device and session,
and expire after 30 seconds without requests. Each view has bounded queues and
ordered frame sequence numbers. Close destroys the view, never the PTY. No upstream
credential is returned. Input is queued without automatic retry; this adapter still
does not claim process-consumed input acknowledgement or styled history checkpoints.

Optional `device_id` in a private external-host entry groups that adapter's sessions
under an existing device. It changes presentation, never authority or routing.
Android transport was tested on hardware through its paired mTLS identity, including
a real disposable VPS command response and independently confirmed resize. Physical
keyboard/touch rendering acceptance and VPS deployment of negotiated resize ownership
remain open.
