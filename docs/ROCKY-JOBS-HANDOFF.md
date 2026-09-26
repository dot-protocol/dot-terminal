# DOT Terminal checkpoint — 2026-09-26

Blaze's ownership decision: **Rocky executes; Jobs owns CTO decisions, independent
verification and code integration/merges.** Emmy stops implementation at this
checkpoint. Additional agents join only when Blaze assigns them.

## Source and scope

Continue PR #39 (`codex/vps-web-workspace`, now targeting main). Jobs reports #33–37
and #40 merged. Fetch and verify main/PR heads before integrating; this branch was
based on the earlier package stack. Do not reimplement the landed keeper work.
Read `CONTINUITY.md`, `vps-workspace.md`, `session-rendering.md` and AGENTS.md.
Runtime details, pairing state and rollback paths are in the private operator handoff.
Never publish that folder. Do not stop the named Mac terminal: Blaze launched Claude
there after it was created. Existing VPS agents must survive every upgrade.

DOT's keeper is THE backend. The AXXIS stream adapter is transitional for existing
processes only. New VPS work starts on DOT keepers; do not restart/adopt an agent to
make its process look like a new DOT session. Core Access becomes a DOT consumer.

## What this checkpoint delivers

- One VPS device section, retaining explicit internal adapter routing.
- Keyed named tabs; close-view versus stop-process distinction and confirmation.
- Toolbar intrinsic sizing/wrapping, measured at desktop and 412px phone width.
- Real native Mac app connected to the same hub/catalog as web. Fixed private hub
  address format; the shared configuration expects `IP:port`, not an HTTP URL.
- Android updated with the same UI and a narrow mTLS native stream transport.
- Rust stream views: random bound handles, ordered reads, bounded queues, expiry,
  no automatic input replay, close-view without stop-process.
- Hardware Android acceptance: paired catalog/replay, real disposable VPS command
  response, requested resize independently observed at 110x32, process alive after
  view-close. Fixture stopped; test instrumentation package removed.
- 75 JS tests, package tests/typecheck, web and independent-consumer builds, Rust
  fmt/strict clippy/tests; Android build/unit/lint and on-device transport acceptance.
  Physical keyboard/touch/IME visual acceptance is NOT established by that probe.

## First work, in order

1. **Mac Claude authentication diagnosis.** The visible error is specifically
   `Remote Control disconnected — OAuth token unavailable`, not proof that Claude's
   model/API login is absent. Existing iTerm and DOT Claude use the same OS user/home,
   executable version 2.1.283, and no differing credential/backend environment
   overrides. Separate read-only auth status reports loggedIn=true, claude.ai,
   firstParty, max. Do not export/copy OAuth tokens or bypass Keychain ACLs. Verify
   auth status from the actual DOT launch context and distinguish Remote Control
   from normal interactive Claude authentication. A new PTY is not an existing iTerm
   process; identical account access is expected, identical session memory is not.
2. **Resolve Rocky's #39 review before release.** R1: replace owner-wide upstream
   authority with scoped per-session authority. R2: observe by default plus explicit
   bounded control; server-enforced input fencing. R3: asynchronous stop/pending
   state with durable confirmation, not a three-second success/failure fiction.
   R4: move tab persistence off async executor; quarantine corrupt state safely.
   Constant-time auth comparison and reconnect cursor recovery remain open.
3. **Geometry ownership and actual rendering.** A separate owning-kernel patch
   `52c1b110` exists in the AXXIS Mac replica and private patch handoff. It is committed,
   NOT deployed or publicly pushed. It claims/releases resize authority by connection,
   supports explicit takeover and cleans up on disconnect. Real-PTY competing-socket
   and stale-owner tests pass. It is only a transitional resize fix, NOT global input
   fencing, ordered historical geometry or a replacement for DOT's keeper. Jobs must
   integrate it against the correct source/release; local AXXIS replica had two surface
   acceptance failures and differs from canonical/deployed heads. Never deploy the
   whole unrelated local development tree just to activate this patch.
4. **Complete desktop/web/physical Android acceptance.** Use a live redraw fixture
   and the same host session ID. Verify resize ownership transfer, alternate screen,
   unicode, keyboard/IME/paste, reconnect and no stale input replay. Keep terminal
   bytes intact; structured trajectory/accordions are a separate presentation layer.
5. **Continue keeper lane already agreed with Rocky:** orphan cleanup without killing
   detached user work, drain independent of viewers, push subscribe-after frames,
   and prompt revocation. Agree protocol changes before coding and run the conformance
   rig. Input dedup should retain bytes rather than relying on DefaultHasher equality.
6. **Release the reusable product.** Same substrate drives all clients; public package
   is alpha and not npm-published. Third-party auth adapters, release artifacts,
   persistent pairing/upgrade/rollback and consumer acceptance precede production
   Core Access replacement. No broad production-readiness claim from unit tests.

## Acceptance / monitoring

Jobs verifies exact SHA + installed artifact + live behavior independently before
merging. Keep process PID/session identity across view and gateway disconnects.
Close removes the shared view; Stop ends the owning process and peers reconcile it.
Current tab polling is two seconds, catalog polling eight; it is not instant sync.
Offline and missing states are not process death. Measure backlog, frame positions,
geometry and freshness separately; no invented synchronization percentage.

No logs, shared snapshots or public handoff may contain input, terminal transcripts,
credentials, bearer URLs or private configuration. Input admission is not proof of
command execution. Uncertain input must never be replayed automatically.
