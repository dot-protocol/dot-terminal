# Identity, worldlines and the personal mesh

Design proposal, 2026-09-19. These contracts are **not implemented** by this document.
The current vault protects data at rest and audits environment disclosure; it does
not keep a secret encrypted inside arbitrary programs or intercept every later use.

## Trust boundary

A directory is storage and an OS access-control boundary, never an identity or grant.
No absolute security guarantee survives a compromised trusted OS, malicious approved
operation, lost recovery authority, implementation flaw or hardware attack. Minimize
what each compromise can do, and make recovery an explicit part of the design.

Separate four identities: owner, device, agent workload, and individual run. Keep
owner recovery authority offline. Give each device its own hardware-backed key where
available; replacement enrolls a new key rather than cloning the old private key.
Use separate algorithm-tagged signing and encryption keys: existing Ed25519 software
identities cannot simply become P-256 Secure Enclave/Android keys by changing storage.
Android hardware security level must be measured per key. The current Mac vault uses
an ordinary Keychain item, not a Secure Enclave signing key.

A trusted launcher binds a run's ephemeral proof-of-possession key to its sandbox and
OS caller identity. An agent's name, model, prompt, executable hash or claimed PID is
not sufficient attestation. A shared unsandboxed OS account is a weak isolation boundary.
SPIFFE/SPIRE is a useful workload-identity model; policy still decides authorization.

## Permission and secret broker

Owner-authorized grants bind issuer, subject key, workload/run, audience, resource,
actions, expiry, delegation parent, policy epoch and spending/resource limits. Define
one versioned canonical encoding with algorithm and signature-domain separation;
reject unknown versions and ambiguous encodings. Do not invent a cryptographic primitive.

For each operation:

1. Caller proves possession over method, target, body digest and broker challenge.
2. Broker verifies caller binding, signature, scope, audience, expiry, revocation,
   delegation attenuation and replay state; atomically reserves budget/idempotency key.
3. It durably records authorization and intent before releasing the operation.
4. It performs the narrowly defined API/signing operation without returning the secret.
5. It records outcome, service receipt or explicitly unknown outcome. A crash after a
   remote side effect needs reconciliation; a local journal cannot promise exactly once.

Example: allow one run to invoke one model provider with a daily spend cap, but forbid
reading/exporting the provider key or contacting arbitrary destinations with it.
An HTTP proxy requires real destination/redirect policy and sandbox egress enforcement;
otherwise an agent can bypass it. Raw environment injection remains an explicitly
weaker compatibility mode with disclosure logging, not complete usage accounting.

Hardware non-exportability reduces extraction risk; compromised authorized software
may still request operations. High-risk operations can require owner presence or a
second approver. Never log secret values, authorization headers or raw sensitive inputs.

Revocation must have an explicit offline rule: short leases bound exposure; high-risk
operations fail closed without fresh policy. A disconnected device cannot learn a new
revocation instantly. Persist replay/budget state across restarts and fence concurrent
controllers. Encrypt backups and wrap data keys for approved recovery recipients;
include offline recovery, lost-device revocation, key rotation, schema migration,
anti-rollback checkpoints and a tested restore on a fresh device. Previously disclosed
plaintext cannot be cryptographically recalled.

## Worldline

Use a signed causal event DAG, with per-actor sequence chains and explicit fork/merge
edges. Logical agent continuity survives a model or device change through authorized
handoff receipts; it does not imply that two model processes are the same person.

An event commits to version, domain, parent IDs, actor/device/run IDs, sequence,
policy/grant references, operation ID, event kind, encrypted payload digest and outcome.
Wall-clock time is an assertion, not causal ordering. Keep private payloads separately
encrypted; do not put raw secrets in immutable logs. Minimize exposed metadata.
Independent devices can witness signed checkpoints and gossip conflicting heads.

Signatures prove key attribution and byte integrity, not truthful execution, complete
capture or an unerasable history. Local chains alone cannot detect a hidden fork or
withheld tail. Merkle inclusion proves membership under a particular committed root;
it does not prove the root contains someone's entire life. Selective disclosure needs
an explicit committed scope and count, plus proofs appropriate to the actual claim.

## Transport and synchronization

Stable identity is independent of changing IP addresses. Treat discovery as untrusted.
Use authenticated, encrypted sessions after explicit enrollment. Reuse the existing
QR comparison and pinned mutual-TLS model rather than exposing the desktop owner token.
The next LAN/browser gateway must issue narrow view/control credentials and enforce
Origin checks, replay rules and per-session grants. Keep the current loopback server private.

| Link | Role | Limitation |
| --- | --- | --- |
| Local Wi-Fi/Ethernet | Primary PTY, clipboard and file transport | Guest isolation/multicast filtering; direct-address fallback needed |
| QR | Bind invitation and peer key during enrollment | Expiring, single-use; discovery is not approval |
| Bluetooth LE | Optional discovery/bootstrap/small control messages | Capability and background limits; not bulk compute transport |
| NFC | Optional invitation transfer on supported hardware | Do not assume availability on the attached iPad or phone |
| Internet relay/overlay | Reach peers off LAN | Relay cannot receive end-to-end plaintext; still enforce grants |
| USB / ADB | Development, installation and diagnosis | Never required for normal mesh authority |

LocalSend's discovery and receiver-consent flow are useful models and its file protocol
could be an optional compatibility adapter. It is not a workload authorization service
or compute scheduler. Apple-specific nearby frameworks are optional adapters, not the
cross-platform wire contract. Consult the current Network framework migration guidance.

PTY output needs resumable sequence cursors, bounded buffering, gap detection and an
atomic terminal-state snapshot. Input uses one controller lease/generation with fencing;
never merge concurrent keystrokes as a CRDT. Clipboard should be opt-in per peer with
size/TTL limits, no silent secret propagation, clear source and undo. Files need encrypted
chunks, verified manifests, resumable transfer and atomic completion. Independent notes
may use CRDTs; permissions, secret grants and financial limits require authoritative rules.

## Device lab and implementation sequence

Observed this session: authorized Android debugging connection and an iPad on USB.
Previous validation established Mac-to-Android Wi-Fi terminal handoff. No iPad DOT app,
mesh enrollment, hardware key level, Bluetooth/NFC transport or distributed compute was
verified here. Full Xcode/device deployment tooling is absent on this Mac.

1. Portable capability verifier and durable broker journal, with rejection tests for
   wrong caller/audience, expired/revoked grants, replay, widened delegation and budget races.
2. Separate authenticated multi-session gateway from the desktop lifetime, using an
   isolated listener during development. Add a paired browser/iPad foreground projection;
   never repurpose the loopback owner capability for LAN use.
3. LAN discovery + reconnect using stable peer identity; stream cursors and snapshots;
   then consent-based clipboard/file transfer. Keep direct-address/QR fallback.
4. Hardware-backed device enrollment and recovery drill; witness checkpoints across devices.
5. Opt-in compute worker advertisements: CPU/GPU/RAM, OS, battery, temperature, availability
   and allowed workload classes. Claims are hints until tested, not attestation.

First compute demo: split independent synthetic jobs between Mac and Android, record
input/output hashes and signed receipts, and use iPad for observation/approval. Add VPS
as durable coordinator only after its current owner/runtime is checked. Do not execute
untrusted native jobs without a sandbox. Budget CPU, memory, time, network and battery.
Wi-Fi does not combine separate RAM/GPU memory into one fast machine; distributed model
layers are a later measured experiment, not an initial promise.

Test sleep/resume, router change, blocked multicast, duplicate/out-of-order packets,
lost acknowledgments, reconnect after replay truncation, revocation during partition,
worker loss, corrupted results and recovery from backup. iPad apps are normally suspended
in the background, so it is initially a foreground endpoint rather than an always-on server.
All tests use disposable sessions/state; active desktop/browser sessions stay running.

## Primary references (reviewed 2026-09-19)

- [Android Keystore](https://developer.android.com/privacy-and-security/keystore)
- [SPIFFE overview](https://spiffe.io/docs/latest/spiffe-about/overview/)
- [LocalSend protocol](https://github.com/localsend/protocol)
- [ADB](https://developer.android.com/tools/adb)
- [Apple background execution](https://developer.apple.com/documentation/uikit/extending-your-app-s-background-execution-time)
- [Apple Network framework migration](https://developer.apple.com/documentation/technotes/tn3213-moving-from-multipeer-connectivity-to-network-framework)
