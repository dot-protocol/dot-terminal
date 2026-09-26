> Latest active work: `codex/vps-web-workspace` builds on reusable package head
> `78748b4`. Read `docs/vps-workspace.md` and the latest continuity entry first.
> Native desktop verification and ordered legacy replay/control are still open.
> Shared workspace tabs are implemented with two-second synchronization polling.

# DOT Terminal — full continuation handoff

Owner: Blaze. Incoming implementation collaborator: Rocky. Date: 2026-09-19.
Read this first, then AGENTS.md and docs/CONTINUITY.md. This is the public source
handoff; device addresses, process IDs, credentials and session transcripts are
intentionally kept in a separate private operator folder.

## Source and operational truth

Public repository: https://github.com/dot-protocol/dot-terminal.
PR #11 merged as ac6ee2067973175b5823fc9fce53c060a3cef9bc.
PR #20 merged as 2c62145acc23ea3e03f5baf72e8a0ab1b8a19792.
All four CI checks passed on each exact reviewed head before merge.
A standalone local clone now replaces the generated conversation folder for future
source development. The old checkout remains a live runtime dependency. Do not delete,
move, rebuild or clean it until a separately planned, owner-approved runtime migration.
A copied private operator archive contains old absolute paths: verify every hint live.

No source checkout or agent name grants credentials. Do not read or use another agent's
identity keys. Preserve existing input ownership, private node identities and phone
pairing. The handoff authorizes continuation of DOT work, not unrelated production changes.

## Product ambition

Build an open-source commons: one terminal workspace across Mac, Linux VPS, Android,
eventually Windows and iPad/web. Durable sessions outlive views; agents may be local,
remote or API-backed. Strong defaults, excellent mobile input and rendering, explicit
ownership, observable progress and recovery are foundational rather than later extras.
Rust is the core. Use native platform integrations and proven upstream engines where
appropriate, preserve licenses, and do not rewrite xterm/WireGuard for branding alone.

Longer-term, the terminal is a device endpoint for sovereign identity, private data,
agent work and collaboration. The discussion also covered personal/family/company
CRM/ERP, open commerce and delivery coordination, account linking, sovereign sign-in,
selective life-history sharing, signed receipts, formal claims and confidential compute.
These are requirements/research directions, NOT current terminal features. A hash or
signature proves attribution/integrity, not truth, delivery, physical presence or complete
capture. A terminal is not itself a sandbox/firewall, and cannot bypass mobile OS limits.

## Implemented vs still missing

| Area | Implemented and evidence | Limits / next gate |
|---|---|---|
| PTY core | Per-session Rust keepers; creator exit survival; controller generations; ordered input; 1 MiB ring and explicit gaps; macOS/Linux tests | No durable multi-node session registry or process migration; arbitrary old iTerm PTYs are not adopted |
| Desktop/web | Swift WKWebView shell, xterm.js/WebGL fallback, compact actions, floating terminal keys, exact typography/themes, iTerm text bridge | Installed app differs from source; tabs/splits/accessibility/IME corpus and full styled reconnect still needed |
| Real Rocky TUI | Native input, browser followers, shrinking/expanding while working; same keeper/shell/agent survived; ordering bug fixed | Legacy byte stream and geometry are not atomic; full-screen queries are expensive; no cross-device pixel-equivalence proof |
| Android | ARM64 app, Rust JNI, Keystore device identity, pinned mutual TLS, QR pairing, scoped grants, handoff, direct input, monochrome screen | Latest compact controls built/unit/lint checked but not installed this turn; physical test blocked by disconnected wireless ADB; no local shell/model yet |
| Pairing/clipboard | Physical Wi-Fi handoff in both directions previously tested; explicit text send/get, revoke and one-level Android undo | No silent universal background clipboard, files/AirDrop, roaming mesh or internet relay |
| Vault | Keychain-backed encrypted-at-rest vault, audited plaintext env disclosure at process launch | No encryption during ordinary process use; no comprehensive use monitoring, recovery/export or operation broker |
| Authority | Experimental owner/subject signed scoped grants, replay/budget reservation journal, outcome states | Synthetic tests only; not wired into live vault/gateway; journal not encrypted; not an OS security boundary |
| Resource manager | Reused read-only host measurements | No CPU/energy/network/firewall enforcement |
| Observability | Local API timing/freshness, received/applied offsets, gaps; explicit unknown coverage | Not key-to-pixel latency or global synchronization; no authenticated peer receipt registry |
| Trajectory | Metadata-only Claude JSONL exporter; local imported rail, grouping/filter/details/pagination, tested against real history | Snapshot only; not live, not cryptographically PTY-bound; no semantic success inference or raw ANSI line folding |
| iPad/Windows | Planned clients | No verified iPad mesh or Windows runtime |

## Immediate next sequence

1. Preserve the live session. Read private operator handoff, verify current processes,
   browser control owner, gateway and installed Android artifact. Do not restart for tidiness.
2. Recover phone debugging connectivity using its current main Wireless debugging IP:port.
   ADB is developer deployment/control, separate from normal DOT TLS traffic. Pairing port
   and connection port differ; old codes/ports expire. Do not expose loopback UI capability.
3. Build isolated .uxlab, attach an explicitly authorized gateway to the same real PTY,
   verify native/web/phone concurrently with actual agent streaming. Test keyboard show/hide,
   rotation, app background/resume, reconnect, fast output, Unicode, paste, lease handoff,
   resize and lost acknowledgements. Record transport and renderer honestly.
4. Replace legacy geometry sampling with stream incarnation, ordered resize epochs and
   atomic styled checkpoints. Add credits/backpressure and incremental authenticated streams.
   Measure key-to-ACK separately from parsed/drawn frame latency, including p50/p95 and loss.
5. Add real gateway multi-session catalog and authenticated per-view receipts: incarnation,
   host head, received/applied offsets, geometry epoch, age and renderer capability. Never
   show one percentage that conflates API health, history fidelity and rendered equivalence.
6. Make input ownership obvious: which view controls, view-only badge, explicit handoff,
   no silent takeover on click. The user could not type because he was in a follower tab.
7. Bind structured agent events to authorized runs and PTYs. Stream tool start/result,
   true batch membership, compaction, waits, corrections, decisions and verified effects.
   Keep speculative interpretations separate. Collapse tool runs in the structured view;
   retain the raw PTY, whose cursor operations cannot safely be removed line-by-line.

## End-to-end planned workstreams

- UI shell: session/device/workspace navigation, named tabs/splits/windows, compact icons,
  keyboard/accessory palette, exact font values, semantic theme tokens, screen-reader support,
  Android IME/composition, voice/STT. App can theme its controls; Gboard owns its keyboard.
- Mesh: durable device identity, QR bootstrap, trust confirmation, scoped services, revocation,
  direct LAN first, address changes, NAT traversal/relay/VPN adapters, explicit device handoff.
  Research Headscale/NetBird/Tailscale/WireGuard and LocalSend through contracts. Bluetooth/NFC
  may bootstrap proximity; neither is promised as a general PTY transport or proof of receipt.
- Transfer: explicit cross-device clipboard and AirDrop-like file sharing with encrypted
  transfer, consent, checksums, resumability and safe filenames; mobile background constraints.
- Sovereign data: versioned encrypted event/blob format, migrations, backups, replication,
  conflict-aware live sync, export/recovery, lost-device replacement, account linking and
  delegated family/company access. Test clean-device restoration before claiming resilience.
- Secret broker: keys stay outside agent context; scoped operations instead of raw env secrets,
  per-use provenance, revocation, budgets, reconciliation after uncertain results, owner-visible
  monitor. Hardware backing narrows key extraction risk but does not protect a compromised
  authorized caller or eliminate endpoint/metadata attacks. Integrate authority only after review.
- Agent runtime: capability-scoped context retrieval, permission-filtered notebooks/memory,
  local or cloud providers, explicit cloud egress, optional small local routing model. Android
  local inference needs a measured RAM/thermal/battery harness before model promises.
- Isolation/resources: sandbox adapters, filesystem/network policies and metered workloads;
  OS-specific enforcement with clear threat model. Read-only telemetry is not enforcement.
- Worldlines: causal signed events with concurrent branches, declared hidden elements and
  selective disclosure. Formalization means precise propositions/rules and checkable evidence,
  not proving arbitrary natural-language claims by labelling them formal or using Lean.
- Extensions: Solid/personal data, sovereign auth/passkeys/recovery, commerce and company/life
  records via adapters. Universal login requires relying-party adoption; no promise to replace
  Google sign-in on arbitrary websites. Physical handover needs consent and a defined evidence
  model; proximity alone is insufficient. Blockchain/confidential compute remain research.
- Commons: Apache-2.0 original code, upstream notices, reproducible builds, signed releases,
  SBOM/advisory checks, clear supported platforms, contribution guide, accessible demo/docs,
  useful public issues and an actual three-device alpha workflow before marketing broad claims.

## Code and documentation map

- crates/session: keeper + CLI; crates/protocol: wire types; crates/terminal: VT model.
- crates/node: device TLS gateway, pairing, clipboard/handoff.
- crates/authority: experimental capability verifier and journal.
- crates/desktop: loopback owner API, vault, bridge; crates/resources: host metrics.
- crates/android + apps/android: JNI and Android UI; apps/desktop: web UI and Swift host.
- scripts/build-desktop.py, build-android.py, pair-android.py, usb-bridge.py: dev tooling.
- docs/architecture.md + ROADMAP.md: base architecture and acceptance gates.
- docs/protocol.md, session-rendering.md, mobile-input.md: PTY/input/rendering contracts.
- docs/android-app.md, qr-pairing-handoff.md, mesh-and-continuity.md: connectivity.
- docs/identity-and-mesh.md, authority.md, desktop.md: identity, secrets, resources.
- docs/appearance-and-state.md, trajectory.md: UI/observability scope.
- docs/reuse-audit.md, dot-reuse-review.md: older Documents/dot and Movies/Kin research.
  Do not import old private-seed sidecar exports; validate each license and adapter.
- docs/mobile-terminal-study.md, panda-and-mobile-control.md: Termius/mobile/Panda research.

## Build / validate / merge

Use a fresh branch/worktree and isolated state. A source clone does not carry ignored
SDKs, build outputs, signing material or active runtime. Rebuild dependencies deliberately.

```
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cd apps/desktop && npm ci && npm test && npm run build
```

Python: `python3 -m unittest discover -s scripts -p test_session_trajectory.py`.
Android: Java 21, platform 36, NDK 28.2.13676358, ARM64 Rust target; use
`scripts/build-android.py`, then Gradle `-PdotTestApp` for isolated installation.
Mac: `scripts/build-desktop.py --test-app`; shared observer additionally takes
`--view-state-dir PRIVATE_RUNTIME`. Do not replace an open lab bundle. Shared mode
must never create/stop keepers or access vaults; old running binaries can predate guards.

Latest code passed local Rust checks, desktop tests/build, exporter tests and four CI
jobs. Android build/unit/lint and Mac lab builds passed earlier; phone deployment remains
separate. Commit explicit paths with sign-off, update continuity, open PR, verify exact
head checks, merge that head. No uncommitted source was left at this handoff.
