# Desktop, browser, vault and resources

## Session ownership

`dot-terminal-desktop` is a Rust service bound only to a random loopback port.
The Swift/AppKit shell embeds its bundled web assets in a nonpersistent WKWebView.
No CDN, remote JavaScript, analytics, or downloaded extension code is loaded.
The same assets run in a local browser. A random 256-bit capability is passed through
the native parent's private stdout pipe and a URL fragment. The page immediately
removes the fragment and retains the capability in that tab's session storage for
reload. API calls require it in an Authorization header. Host and Origin checks,
a restrictive CSP, no-store, no-referrer and frame restrictions defend the web boundary.
Browser extensions or compromised same-user processes are outside this boundary.

The keepers own the PTYs. A window owns only a controller generation and output offset.
The renderer fetches at most one 16 KiB output chunk at a time, waits for xterm's
write callback, then advances the offset. Input is serialized, capped at 16 KiB queued,
and never retried after an ambiguous acknowledgement. This first transport uses
bounded HTTP polling; it is not a WebSocket streaming claim. Hidden tabs stop polling.
A closing view attempts to release its lease. After an abrupt close, use the explicit
**Take over input** action. Opening a second view does not create a second shell.

The current 1 MiB raw replay cannot fully reconstruct styles and terminal modes
across history truncation or different historical window sizes. A gap is visible and
falls back to an owner-side text snapshot. Snapshot/output atomicity and complete
styled rehydration are release blockers for a claim of seamless arbitrary-TUI continuity.
The native wrapper is macOS-only; Rust HTTP/session components target Unix. Android
is an independent paired client. Windows, a native Linux wrapper and an Internet relay
are not implemented. Remote access must use the authenticated node service, not public
exposure of this loopback UI.

## Existing iTerm sessions

The optional Python bridge enumerates sessions and retrieves screen text through
iTerm's documented API. Input is sent only to the selected session with broadcasting
suppressed. DOT does not own those PTYs. Other iTerm views remain able to type; DOT's
input toggle is not an exclusive cross-application lock. Color, scrollback and full
terminal modes are not preserved in this text projection. DOT-managed sessions use
xterm.js directly and have generation fencing. The bridge is a separate GPL-2.0-or-later
component; see the root NOTICE, licenses/GPL-2.0.txt and requirements-iterm.txt.
The build installs the optional library from PyPI into a separate local application-support environment (not the signed bundle); public source does not bundle it.
Redistribution of a complete binary bundle must include all applicable notices and
corresponding source for that optional component.

## Vault boundary

On macOS, the default vault uses a random AES-256-GCM key stored as a Keychain generic
password. Encrypted records include secret values, names, and audit metadata. Each
write uses a fresh random 96-bit nonce, a version-bound AAD, a 0600 temporary file,
file sync, atomic rename and directory sync. A process lock prevents simultaneous
vault writers. Keychain errors do not cause generation of a replacement key for an
existing vault. The current Keychain key is not a Secure Enclave non-exportable key.

The UI can store, list names, delete and launch a process with selected environment
variables. It cannot retrieve secret values from a list endpoint. Release is logged
and persisted before process launch; a later entry records the resulting session ID.
The observed executable path and SHA-256, number of arguments, selected secret names,
local-owner authority and operation time are retained. Argument values are not logged.
The digest is an observation, not a code-signing guarantee or protection against an
executable changing between hashing and execution. There is a bounded 10,000-entry
local audit; reaching it fails closed. There is no audit rotation/export yet.

Environment variables are plaintext in the keeper and target process. Their later
copies, output, network use and descendant processes are not comprehensively audited.
A malicious child can print or exfiltrate a supplied secret. AES-GCM protects stored
bytes against modification, but a local owner or equivalent malware can access keys,
replace the app, restore an older vault, or rewrite history. This is neither a global
firewall nor an immutable provenance ledger. Recovery/export, revocation of already
released secrets, per-agent grants and a credential-operation broker remain unfinished.
Do not store valuable credentials in this preview. The local vault has not received
an independent security audit.

## Resources and policy roadmap

`dot-terminal-resources` reuses AXXIS Resource Manager's MIT collector with its
license and a pre-edit hash manifest. It records private local measurements under
DOT's own data directory, not the existing Resource Manager database. Process command
arguments and environment values are not collected. The panel shows CPU, memory,
swap and grouped process attribution. Group footprints are distinct from physical
system RAM and may include compressed/swapped accounting; unavailable platform
measurements must not be interpreted as zero. This is read-only visibility.

The next policy boundary is managed-workload launch: explicit executable identity,
secret scopes, filesystem/network grants, and platform-specific enforcement adapters.
Linux cgroups and namespaces, macOS sandbox/network extension capabilities and Android
app/OS limits differ; none is implemented here. No whole-machine traffic, CPU, power
or memory enforcement is claimed. Oracle integration should consume its published
contracts, keep person/seat identity separate from device keys and leave the active
Oracle migration owner in control; no Oracle deployment is modified by this app.
