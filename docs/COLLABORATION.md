# Collaborating on DOT Terminal

Start with `AGENTS.md`, `docs/CONTINUITY.md`, and `docs/session-rendering.md`.
Public repository: https://github.com/dot-protocol/dot-terminal

A running shell is user work, never a test fixture. Preserve live desktop, browser,
node gateway and keeper processes. Do not type probes into another agent's terminal.
Private runtime hints live outside this repository in the operator's `work/` folder;
verify them, never commit them. No identity or credentials transfer with this handoff.

## Work ownership

The session-signals-and-shell branch owns desktop rendering/resize, local view
measurements and compact Android controls. Coordinate before editing those files.
Other contributors should use their own branch and worktree from verified main,
with a separate runtime directory and ephemeral ports. Do not share index/staging
or reuse another contributor's test app/keeper. Public issues and PRs carry durable
ownership; Oracle dialogue can notify collaborators but does not grant access.

## Resume and acceptance

1. Inspect git status, HEAD, open PRs and exact-head CI. Do not discard unfinished work.
2. Read component docs and compare source SHA with installed artifact and live process.
3. Run desktop unit tests/build and the required workspace checks in AGENTS.md.
4. For Android use `scripts/build-android.py`; use `-PdotTestApp` for device experiments.
5. Verify on disposable PTYs: ANSI/TUI output, keyboard show/hide, resize ownership,
   two viewers, control transfer, delayed input ACK, history loss, stale/disconnected
   state, and background/resume. Record transport and renderer actually tested.
6. Signed-off explicit-path commit, PR, exact-head green checks, then merge. Update
   continuity with deployment differences. Never restart the user's app just to
   make its version match source.

## Ordered next work

- Persistent authenticated multiplexed streams, bounded per-view credits, observer
  expiry and reconnect negotiation. Keep input fencing and no uncertain input retry.
- Host stream incarnation + geometry epochs + atomic styled snapshots. Replay must
  preserve the original geometry; a text-only fallback is explicitly degraded.
- Authorized peer-view registry showing head/received/applied offsets, freshness,
  renderer capability and measurement coverage. Discovery never grants authority.
- Android styled-cell renderer and Unicode/IME/TUI conformance, then multi-session
  navigation backed by the real gateway catalog. No fake tabs for absent sessions.
