# DOT Terminal continuity

Updated 2026-09-19. Public baseline: PR #5, merge
`09b6850b0b0308d5005f4544fee54fc15d3053e5`.

## Verified baseline

- Rust per-session PTY keepers, controller generation fencing, bounded byte replay.
- macOS desktop and local browser views with xterm.js; existing iTerm screen/input bridge.
- Android app with device Keystore identity, pinned mutual TLS, QR enrollment,
  explicit clipboard grants and one-use handoff. Both handoff directions were tested
  on a physical Android phone with preserved PID/environment, replay rejection and
  stale-controller fencing. Network route was Wi-Fi; no ADB reverse mappings.
- macOS vault encrypted at rest with a Keychain master key; audited plaintext env
  disclosure to launched processes. No recovery/export or secret-operation broker.
- Reused MIT resource collector; read-only measurements, no resource/firewall enforcement.
- Four CI jobs passed on the PR #5 head. See the checked-in validation receipt for
  test boundaries. Passing CI is not proof of a particular running binary.

## Live-service protection

The desktop and web version are in active use. Keep them running. Keepers survive
view closure, but the current browser service still belongs to the desktop lifetime.
The installed app can differ from the already-running executable; verify before
claiming that a change is live. Do not restart merely to reconcile that mismatch.
The existing node gateway routes one keeper; mobile multi-session selection is absent.
The browser capability grants local-owner access and is unsuitable for LAN exposure.

## Current requested work

Investigate older DOT FS, assembly/VM, identity, receipt and transfer work; define
hardware-backed identities, constrained agent permissions and causally linked worldlines;
plan a Mac + Android + iPad + VPS mesh; study LocalSend; preserve dependable agent handoff.
Research results and next experiments belong in `docs/identity-and-mesh.md` and
`docs/dot-reuse-review.md` when available. Keep implementation claims separate from plans.

## Resume procedure

1. Read AGENTS.md and this file; inspect Git status/HEAD and any open PR.
2. Locate private operator handoff outside this repository if available; it is a hint,
   not authority. Never copy live hostnames, tokens, serials or user session IDs into Git.
3. Verify live process identities/ports without reading secret values or killing processes.
4. Use a fresh branch from verified main and isolated test state. Follow component
   instructions; do not rebuild another owner's service or impersonate an Oracle agent.
5. Complete meaningful checks, update evidence and continuity, and merge the exact
   checked head. Leave explicit remaining work if a device/tool prerequisite is missing.

## Latest handoff: identity/mesh review

The identity/mesh and reuse documents now contain source-grounded design, limitations
and ordered experiments. This change adds documentation and operating instructions only;
it does not deploy a broker, mesh, iPad client or hardware-backed Mac vault. Found a
private-seed sidecar export in the old DOTFS CLI; do not import that export behavior.
No old DOT files or live services were changed. Next bounded implementation is the
portable capability verifier plus isolated broker journal, followed by a separate
authenticated multi-session gateway. Read the rejection criteria before implementation.

Validation for this documentation change: local `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`, and
`cargo test --workspace --locked` passed. Older-source tests were not run.
CI status must be checked against the PR head before merging.
