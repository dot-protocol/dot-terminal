# Current continuation handoff — 2026-09-19

Read `../HANDOFF.md` for the complete scope, implementation matrix, next actions and
build/merge rules. PR #20 is merged as `2c62145`; PR #11 as `ac6ee20`, both after
four exact-head CI checks passed. A standalone local source checkout is now the
continuation location. The old generated checkout remains a live runtime dependency;
never move/delete/rebuild it as cleanup. Private operator material was copied separately
with restricted access. Source and runtime are deliberately not assumed identical.
Rocky is the requested incoming owner. Preserve the active browser's input ownership.
The older entries below are chronological evidence and may describe superseded states.

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

## Authority implementation handoff

PR #6 merged after all four checks passed, main `bab32c2`. The next branch adds
`dot-terminal-authority`: direct signed grants, request proof of possession, policy
checks and transactional SQLite replay/count reservations. Read `docs/authority.md`
for exact limits. The database metadata is not encrypted; synthetic tests only. This
is a library with no live vault/gateway integration and no deployed service change.
Pending intents consume budget after reopen; there is no automatic external retry.
Next: isolated trusted broker process and synthetic adapter, policy/caller binding and
encrypted private state before any valuable credential is used.

Validation: format check, workspace clippy with warnings denied, and all workspace
tests passed locally. Authority tests use fresh temporary SQLite files and fixed
synthetic signing keys. CI must pass on the exact PR head before merging.

## Theme and observability handoff

Authority PR #7 merged after all four checks passed (`76d4044`). New work adds
web themes/exact typography, responsive drawer, bounded local copy/state index and
API observations, plus Android appearance/pan/collapsible controls. Read
`docs/appearance-and-state.md` for coverage and validation boundaries.
Android APK was installed wirelessly and preferences checked on the real device.
The native desktop bundle and active user services were not replaced or restarted.
An isolated, vault-disabled web preview is open in Codex; private connection details
are in the outer work directory. Keep it separate from user sessions.
Next: authored stable copy IDs and generated cross-platform theme tokens, native
accessibility/viewport work, then module state contracts and authenticated mesh events.
There is no production OTA updater and no automatic whole-process variable indexing.

PR #8 carries the UI work. Physical Paper-theme inspection led to a contrast correction
for native controls/status icons; preserve that correction when continuing Android UX.

Known preference gap: web localStorage is scoped to the backend origin; a random-port
server restart can reset web appearance. Add authenticated persistent settings before
claiming preferences survive desktop service restarts. Android persistence is separate.

## Mobile terminal research handoff

Theme PR #8 merged as `87fd049` after exact-head checks passed. The next documentation
branch records a primary-source mobile terminal comparison and physical Termius 7.10.0
Play Store testing in `docs/mobile-terminal-study.md`. Real disposable Mac and VPS
shells accepted phone input; colors, Ctrl-C and connection tabs were observed. The
transport was a temporary wireless-debugging SSH bridge, not direct mesh/VPS access.
Temporary test shells/servers/forwarding rules were removed; Termius remains installed
with inactive fixture profiles. Private evidence is outside the repository.
No DOT runtime changes or live service restarts occurred. Next bounded work is terminal
input/viewport/session UX, with view-only access and explicit control transitions.
Keyboard resize propagation, network recovery, Unicode/IME and full-screen TUI tests
remain unproven. Other ranked applications were researched, not installed or tested.

Validation for the study: workspace format check, clippy with warnings denied, and
workspace tests passed locally. Documentation only; these checks do not expand the
physical-device evidence. CI must pass on the exact PR head before merge.

## Android input and Panda research handoff

Study PR #9 merged after all four checks passed (`5d6a664`). Next work is on
`codex/mobile-input-and-panda`: direct terminal input, keyboard strip, measured controller
resize, explicit view/control/release and takeover confirmation, compact tools and a
session panel. The gateway still exposes only one session; no fake multi-session tabs
were added. See `docs/mobile-input.md` for the real-device tests and remaining IME,
renderer and networking limits. Temporary `.uxlab` app, keeper and development bridge
were used; normal desktop/browser/gateway sessions were not restarted or controlled.

Panda 1.1.47 sign-in did not reach a signed-in screen. At the user's direction,
configuration was stopped and research continued online. Its current personal-use
license is not suitable for direct open-source reuse; no Panda source was imported.
See `docs/panda-and-mobile-control.md`. Termius review now includes vault/keychain forms,
forwarding, snippets, known hosts, logs and settings; paid/team/hardware-key workflows
remain untested. Private screenshots and account information stay outside Git.

Validation: Android build, unit tests and lint; workspace format, clippy and tests;
and five bridge tests passed. Exact-head CI must pass before merge. This is not proof
of unrestricted phone automation, confidential computation or full terminal emulation.

The standard Android debug APK was installed wirelessly with existing app data kept,
and the new main screen was verified. It was left disconnected; no input was sent to
its normally paired session. The isolated app/keeper/bridge and forwarding rule were
removed. Desktop and browser remained running. Private artifact hash and connection
hints live in the outer work directory, not this public repository.

## Session signals and shell handoff

Android input PR #10 merged as `d9976d7` with four passing checks. Current work is
`codex/session-signals-and-shell`. Read `docs/session-rendering.md` and
`docs/COLLABORATION.md`. Adds optional WebGL with fallback, corrected fit padding,
serialized/coalesced controller resize, host-sized read-only views, compact shell
icons, expandable terminal keys and bounded per-view latency/freshness measurements.
Polling adapts between an active 32 ms scheduling interval and a 250 ms idle gate;
background browser views do not poll. This is still polling, not a persistent stream.
Android gets themed expandable keys and local request/apply timing; no styled-cell
renderer or peer receipt registry yet. Gboard controls its own main keyboard theme.

Rocky was onboarded through his existing idle terminal at Blaze's explicit request,
read the repo instructions and returned review risks. He did not receive credentials
or authority to overwrite this work. He recommends incarnation/geometry/styled
snapshots before a peer receipt registry, and separate DOT/Oracle identity adapters.
He is available to review the PR. Use separate branches/worktrees for new ownership.

Source changes are NOT automatically the live app: the protected browser and main
native app remain on their earlier assets/runtime. Existing preview keepers can run
from `target/debug`; rebuilding that path does not establish their live version.
The macOS lab build now uses a separate bundle identity, isolated state, disabled
vault and no iTerm integration (`scripts/build-desktop.py --test-app`). Do not use
`--install` to replace the active desktop. Private preview bindings are outside Git.

Validation so far: desktop tests/build, Android build/unit/lint, workspace format,
clippy and tests passed. Isolated browser and native WKWebView accepted shell input,
rendered ANSI colors, and used WebGL. A read-only browser view retained the shared
177-column grid after its local font changed to 20 px. Corrected browser layout had
no horizontal/vertical overflow at the tested desktop size. Native window resize changed the disposable PTY from 117×31 to 85×25;
subsequent shell input succeeded. Android wireless debugging disappeared
before installation; the new physical-phone UI is not yet verified. Full CJK IME,
roaming, renderer-loss recovery and end-to-end pixel timing remain unproven.

Rocky's review of `7ebad79` caught a transient-control-check regression and replay
selection race. Follow-up distinguishes explicit keeper fencing from network/busy
failures, suppresses polling while selection/reset is in progress, and makes invalid
telemetry non-throwing. Historical gaps remain a separate fidelity flag/count after
freshness recovers. Accessory arrows honor application cursor mode. Lab builds now
use their own Cargo target directory and mkdtemp-created private runtime. Full-screen
geometry sampling remains a cost limitation; a versioned geometry/epoch contract is
next. Positional non-action copy IDs are still not a complete authored copy registry.

Blaze clarified acceptance: test Rocky's actual running agent UI across views, not
only sample shell output. New browser/native views may attach to the existing session
without restarting its keeper. The explicit lab-only `--view-state-dir` build option
supports that requested shared view; it is not an isolated fixture and must never be
used for test commands. Do not stop sessions when cleaning up such a view. Phone
attachment remains blocked by the unavailable debugging connection/profile setup.

Real-agent testing found redraw fragments when a resize acknowledgement raced with
new output: the controller parsed a redraw before changing its local grid, and a
follower sampled geometry after parsing. Follow-up drains in-flight parsing and
prepares the controller grid before requesting SIGWINCH; followers sample geometry
before applying nonempty output. This improves the legacy path but does not make
geometry and bytes atomic. Sampling full screens now costs one extra request per
nonempty follower chunk; replace this with versioned geometry metadata, not a claim
of complete renderer equivalence. Regression tests cover the resize ordering and
superseded-view guard. Further real-agent retesting is in progress.

Shared native views now use backend `--attach-only`: creating keepers is rejected
server-side and the vault is disabled. This prevents a shared-view lab rebuild from
replacing a new keeper's executable path. Ordinary installed desktop behavior is
unchanged. Rebuilding an isolated lab still requires first closing its test app and
stopping only keepers that the experiment created. Never stop an attached live keeper.
Rocky re-reviewed `cd1783f` and cleared the prior blocker; the subsequent live-render
ordering change still needs its own review/CI. Private reviews remain outside Git.

Real-agent retest of the ordering fix: a substantive review request was sent through
native terminal input to the existing Rocky process. While it worked, the native
controller was shrunk and expanded; the browser follower showed matching line wraps,
intact prompt/status rows and continuing output at both sizes. The original keeper,
shell and agent processes remained alive. This is bounded visual evidence, not
atomic stream correctness or pixel-latency measurement. The final review is pending.
The original browser was refreshed for this explicitly requested test; its backend
and the main installed native app were not restarted. A separate shared native lab
window and browser view remain attached. The lab bundle uses the equivalent inline
ordering implementation built before extraction into the tested render-flow helper.
Android still has no debugging connection, so same-session phone rendering and the
new mobile controls remain unverified on hardware. All four CI checks passed for
`c35ebdc`; desktop nine tests/build and required Rust checks passed locally.

Rocky's final review of `c35ebdc` found no blockers, with three should-fix edges.
Follow-up reconciles host geometry after an uncertain resize (and keeps retrying
geometry reads before parsing if reconciliation fails), rejects Stop in attach-only
backends, and makes attach-only independently disable/reject vault access. Regression
coverage exercises failure recovery, superseded views and refusal before spawning/RPC.
Desktop ten tests/build and workspace format/clippy/tests passed. These last failure
path protections are source-tested, not loaded into the running shared lab/backend.
The visible browser follower was intentionally read-only; when Blaze reported he
could not type, control was explicitly transferred there and input focused. Avoid
moving it back implicitly. The input-owner UX needs clearer cross-view identification.

## Activity trajectory (2026-09-19)

PR #11 merged as `ac6ee20`. Follow-up branch `codex/activity-trajectory` adds an
explicit local Claude JSONL metadata exporter and desktop right-hand trajectory rail.
Read `docs/trajectory.md` for privacy and coverage. No live feed or terminal-content
folding is claimed. The observed session's private records and metadata export remain
outside Git. A separate attach-only preview observes the same live PTY; original
browser input ownership and native apps remain untouched. Snapshot import, grouping,
compaction detail and live PTY observer view were checked in the in-app browser.
Source tests cover stripping content, invalid records, tool-result pairing, duplicate
records, unresolved results and group boundaries. The rail does not prove task effects.

## Dependable input (2026-09-19)

Branch `rocky/dependable-input`. One `InputController` (`apps/desktop/src/input-controller.js`,
no DOM, no network) is now the only path from a desktop view to a PTY: xterm `onData` (keyboard,
composition, paste) and drops all pass through it. `terminal-input-binding.js` is the only DOM
wiring. What it guarantees and what was measured:

- Ordered, one request in flight, bursts coalesced. UTF-8 is never split across requests.
- `controller busy` (keeper refused before touching the PTY) retries the same bytes; any other
  failure is an unknown outcome: nothing is replayed, bytes queued behind it are discarded, the
  view says so and gives up control. Still detected by the keeper's English string (open: a
  machine-readable code needs a new protocol operation, the wire is strict both ways).
- Paste: the old path refused anything over 16 KiB. Now up to 1 MiB, cut on character
  boundaries. Measured in an isolated lab: zsh's line editor accepts roughly 6 KB/s and the
  keeper's PTY write blocks, so 8 KiB requests took 1.1 s p50 / 1.8 s p95 against a 3 s socket
  timeout, and one run crossed it (ambiguous, input stopped, as designed). Requests now start at
  2 KiB and follow the measured round trip (halve above 600 ms, grow back under 120 ms, floor
  256 B): same paste, 252 ms p50 / 483 ms p95, no unknown outcomes. A 60,000-byte multiline
  unicode paste into a raw `cat` sink arrived with an identical SHA-256.
- Drops: a link or plain text is inserted like a paste, never submitted, trailing newlines
  stripped; files get an explicit "not supported" message; the page never navigates away. A
  read-only view refuses the drop and says why.
- The infobar shows input state (`VIEW ONLY`, `INPUT · YOURS`, sending, queued bytes, stopped).
  System panel has content-free input measurements: counts and timings only, never text.
- The session shortcut is Cmd-N only. It used to also take Ctrl-N from the shell.

Not done, not claimed: hold-Space voice is NOT diagnosed. The keeper half of the per-key path was
measured fast (0.25 ms p50, zero busy in 3,000 inputs), so the earlier busy-collision theory is
dropped. Still open: whether the web view emits repeats for a held Space and at what cadence (the
new `held-key repeats seen` counter exists for this, it needs a physical key hold), and which
process macOS holds responsible for the microphone (the bundle has no
`NSMicrophoneUsageDescription`). Swift host drag types, Android, and keeper-side non-blocking
writes are untouched. Deployed state: source only. No running app, backend or keeper was replaced.

## Activity pane and workspace layout (2026-09-19)

Branch `rocky/activity-pane`, stacked on `rocky/dependable-input`. The trajectory rail's
`position:absolute; top:112px` and the terminal's `margin-right` are gone. `#workspace` is a grid:
terminal, splitter, Activity pane as siblings; tabs under 680 px. The pane is now live and newest
first, fed by `ActivityStore` from what the view observes about the selected PTY (output bursts,
grid, control, stops, gaps, exit; sizes and times only). Snapshot import remains, tagged, merged by
time. Read `docs/trajectory.md`. Checked in a browser against an isolated lab backend: wide, the
terminal box ended at x=1593 and the pane began at x=1600 (no intersection), focus stayed out of
the pane, a bursty lab session showed streaming/quiet live; narrow (606 px), the terminal box was
identical in both tab views and the page did not scroll sideways. Not checked: WKWebView, Android,
a controller's live TUI redraw after the pane-driven resize. Not built: a live agent/tool feed;
it needs an owner-bound adapter in the backend and is the next step for this pane.
Deployed state: source only.
