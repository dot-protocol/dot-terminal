# Connected workspace

Android bundles the same `apps/desktop` UI, xterm renderer, input controller, themes,
activity layout and device/session navigation as the Mac/browser view. Its native
adapter authenticates with an Android Keystore key over pinned mutual TLS. The
web page never receives the private key or the desktop owner's bearer capability.

## Authority and setup

A node's `workspace` peer grant is separate from its existing one-terminal grant.
Old peer records default to **false**. Existing QR invitations still grant only their
original services; they do not silently grant the whole workspace. For a workspace,
the owner enrolls the device's public certificate with `pair --workspace`. This
grant allows listing, creating and controlling sessions exposed by the configured
hub, including its remote nodes. A shell grants the privileges of its host account;
this is not an agent sandbox. Use an appropriately restricted host account.

`dot-terminal-node serve --listen ADDRESS --workspace-config PRIVATE_FILE` serves a
workspace without depending on any single keeper. The private JSON config contains
`address` (a loopback socket address) and `capability` (the hub's 64-character key).
The node checks ownership/mode and refuses a non-loopback hub. Both node and hub
must run under the same owner. Never serve the loopback HTTP UI on a public/LAN port.

Only catalog and validated keeper routes pass the gateway. Stop, vault, iTerm,
resources, arbitrary paths/URLs, view automation and agent-log APIs are unavailable.
Revocation is checked on every request. Workspace UI preferences are local to the
phone and cannot overwrite the host window's preferences. A normal host browser
continues to use its existing loopback capability boundary.

## Build and use

`scripts/build-android.py --test-app` builds the shared web assets and Rust JNI
validator, packages `.uxlab`, and runs Android unit tests/lint. The regular build
updates `.dev`. Native updates still require an APK install; a web reload cannot
replace native code. Android's native settings/pairing screen has a Workspace button;
after using it, future app launches prefer that workspace. Back returns to settings.

A separate Mac instance is built with `scripts/build-desktop.py --test-app
--test-instance NAME --view-state-dir PRIVATE_DIRECTORY`. Its identity is independent
of another running test app; existing apps and keepers need not be restarted.

Tabs enumerate real sessions from the hub's device catalog. Each view selects its
own session; selection is not forced onto every device. Selecting a tab releases
that view's previous input lease, not the process. Operations carry the selected
device explicitly, and only the controller resizes the PTY. Observers follow its
reported geometry. Input remains ordered and is never replayed after uncertain ACKs.
Android IME composition commits through one native path; it does not also pass
through WebView's fallback interpretation. Delayed IME input is scoped to its session.

## Known limits

This transport still opens a TLS connection per RPC and polls output. It is not a
persistent multiplexed stream. Telemetry measures this view's requests/parser;
"caught up" does not prove simultaneous pixels or every peer's state. iPad/Windows
clients, phone-hosted shells and autonomous installation are not implemented here.
Old keepers have no presence or geometry epochs; their fallback is explicitly degraded.
Atomic styled snapshots are still needed for faithful old-history redraw after a gap.
The workspace enrollment experience is operator-assisted; a full workspace-grant QR
flow and user-facing peer management remain separate work.

## Shared names and usage

The workspace hub stores bounded session labels atomically under its private state
directory, scoped by device and session. The pencil action and double-click open the
same rename form. Paired workspace clients may rename; one-session grants may not.
Catalog refresh propagates names to other views (currently polling, up to 8 seconds).
CPU is summed across the shell and sampled descendants, with 100% representing one
core. Resident memory is summed RSS and may double-count shared pages; it is not a
physical-memory ownership claim. Samples carry a timestamp; unavailable/stale samples
show a dash. Local loopback legacy catalogs can use the host's collector; remote PID
numbers must never be matched against local processes. Old remote nodes remain unknown.

## One desktop view, one service

`build-desktop.py --workspace-config PRIVATE_FILE --identity SIGNING_IDENTITY` builds
a view of an existing loopback hub. The private file uses the same `{address, capability}`
shape as the TLS gateway config. The wrapper checks owner, mode, file type and loopback
address, and never starts or stops that hub. Keep the hub/node identities and labels in
a private persistent directory; supervise each service once. The current operator setup
uses macOS LaunchAgents and stable signed executable paths. Installation state and keys
are not repository content. LAN address rediscovery is still pending; durable pairing
does not by itself solve DHCP changes. Existing processes do not survive an OS reboot.
