# Roadmap and acceptance gates

The scope is a durable public product. Small implementation stages are review
boundaries, not a decision to retain prototype shortcuts forever.

| Stage | Status | Required evidence |
|---|---|---|
| Local execution foundation | Implemented, early | Real PTY survives creator exit; fencing; malformed-client resilience; macOS/Linux tests |
| Terminal model and snapshots | Monochrome VT snapshots implemented | Correct alternate screen, Unicode, modes, resize and reconnect corpus |
| Durable node registry | Planned | Keeper discovery, honest failure states, controller restart survival |
| Secure device connections | Enrolled IP/TLS gateway implemented; mesh discovery/relay planned | Mac/VPS/phone enrollment, revocation, end-to-end transport and fault tests |
| Encrypted storage and recovery | Planned | Clean-device export/restore, key rotation, migration rollback |
| Desktop terminal | Planned | Native input, accessibility, GPU rendering, windows/tabs/splits |
| Android client and local model | Wireless development client and explicit clipboard implemented; inference planned | Real-device remote handoff, offline inference, lifecycle/memory/battery measurements |
| Agent context and execution policy | Planned | Permission-filtered retrieval, bounded tools, explicit cloud egress, sandbox provider |
| Public alpha | Not released | Complete three-device workflow, signed artifacts, documented support/security boundaries |

## First community workstreams

1. Specify engine-independent terminal snapshots and construct a synthetic VT corpus.
2. Test keeper behavior on Linux and macOS under child exit, disconnect and saturation.
3. Define device-scoped input grants and controller lease expiry without relying on
   a synchronized client clock.
4. Extend Android JNI bindings with proper cell styling, IME composition and a measured offline small-model harness.

Read the complete architecture before making irreversible wire or storage choices.
Do not add separate renderers or provider backends without a demonstrated need.
