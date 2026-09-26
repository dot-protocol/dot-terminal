# DOT Terminal: product boundary and acceptance plan

Audit: 2026-09-20. Source baseline e21412f; device resource work is on
`codex/device-resources-contract`. Source, preview, installed applications and old
keepers are separate versions. This document updates the original architecture;
it is not a claim that every target below is implemented.

## Ethos

A durable, owner-controlled execution workspace that any application can present.
A session belongs to its execution device, not its tab, browser or AI provider.
Views can disappear without killing work. Authentication, recovery, accessibility,
privacy and honest operational state are part of the foundation. Use proven engines,
contribute upstream and preserve licenses. Public source must build without private
operator files. No vendor identity is necessary to operate one's own node.

DOT is the infrastructure and reference interface. Core.access owns rooms, Oracle,
people, tasks and application navigation. Other applications can supply other UI.
Personal ERP, commerce, identity federation and selective life-history sharing are
future applications of these primitives, not dependencies of a usable terminal.

## Verified source map versus original ambition

| Layer | Present | Still required |
| --- | --- | --- |
| Rust keeper (`crates/session`) | Real Unix PTYs, independent lifetime, fenced input/controller, bounded replay and explicit gaps | Styled atomic checkpoints, durable recording, hardened crash/reconnect matrix, Windows ConPTY |
| Protocol/parser (`protocol`, `terminal`) | Versioned operations, VT model, geometry-aware newer read frames | Complete modes/styles/Unicode compatibility corpus; negotiation with old keepers; snapshot plus ordered deltas |
| Node (`node`) | Pinned mutual TLS, enrollment/QR, scoped grants, workspace catalog proxy, clipboard/handoff primitives | Discovery/address changes, relay fallback, recovery/rotation/revocation drills, persistent streams and bounded flow control |
| Workspace hub (`desktop`) | Device/session catalog, labels, authenticated owner APIs, remote routing | Durable registry/event subscriptions, capability-scoped SDK authentication, fleet receipts |
| Rendering (`apps/desktop`) | xterm 6 with WebGL fallback, controller-aware resizing, input queue/generation fencing, shared Android web UI | Independently mountable package; two-view lifecycle; full IME/accessibility/mouse/paste/TUI acceptance matrix |
| Reference shell | Device navigation, named tabs, appearance, compact keys, activity component | Predictable mobile navigation, splits/window lifecycle, state migrations, clearer reconnect and ownership |
| Resources (`resources`) | Backend collector, host/process CPU/RSS, footprint coverage, session descendant attribution | Explicit phone inventory grant, upgraded VPS collector, process identity races, cross-platform coverage, opt-in API instrumentation |
| Vault (`desktop/vault`, `authority`) | Local encrypted-at-rest vault and audit foundations; experimental authority library | Production integration, workload-bound grants, brokered use, rotation/recovery/export, independent security review |
| Agent/event adapters | Owner-bound Claude log analysis and trajectory UI | Portable adapters, permissioned remote feed, evidence links and branch-preserving durable events |
| Inference/speech/sandbox | Architecture candidates | Provider adapters, hardware/battery benchmarks, sandbox policy, speech draft UX |
| Release/commons | Public repository, CI, upstream attribution, handoff discipline | Clean-machine installer, published SDK, signed releases/update policy, support matrix, maintainers/security process |

Do not infer a completion percentage by counting files. Acceptance is an end-to-end
workflow: install -> pair -> start -> observe elsewhere -> control transfer ->
background/disconnect -> resume -> revoke -> recover, without losing or duplicating input.

## Embedding contract (target, not a published SDK)

Keep one repository while extracting versioned packages. Proposed boundaries:

- `terminal-client`: authenticated transport supplied by the host, session references,
  negotiation, ordered output, ownership, input acknowledgements and scoped telemetry.
- `terminal-view`: mount into a caller-owned element; theme/typography, selection,
  accessibility, keyboard/composition and renderer lifecycle. No global DOM IDs,
  sidebar, singleton terminal, routing, owner bearer or hard-coded backend address.
- `workspace-model`: device/session catalog, labels, subscriptions and layout references.
  Shared session identity is distinct from each view's selected tab and pixel layout.
- Reference DOT shell: composes the above with navigation, pairing and device details.
- Optional activity/worldline package: structured evidence above the terminal stream.

The host gives a transport and session reference; the view exposes mount, attach,
setPresentation, detach and dispose with versioned state/error events. Disposing a
view detaches only. Stopping a process is an explicit separately authorized operation.
Input is never replayed automatically when its acknowledgement is uncertain.

Minimized means suspend drawing and bounded buffering, not terminate. Restoring must
negotiate a current checkpoint if retained output is insufficient. Panel/expanded
sizes come from the host container. ResizeObserver only requests PTY resize when this
view holds the controller lease. Observers pan or scale the shared grid; they cannot
independently reflow a full-screen TUI. Hidden/zero-size containers never send 0x0.

First package gate: an independent example outside the DOT shell mounts TWO views,
resizes/minimizes/restores/disposes each, has no cross-view input leakage, survives a
transport break and rejects stale control. Do not publish a thin wrapper around the
current global main.js as a finished SDK. Extract transport/lifecycle before exporting
an API and maintain a conformance example used by Core.access and CI.

## Resource contract

One read-only collector belongs to the device backend, independent of open views.
The hub now discovers a bundled sibling collector unless explicitly configured.
Selecting a device opens totals, session attribution and expandable processes, with
freshness and retry state. Virtual memory is address space, not additional RAM usage.
Host CPU is averaged across cores; process/session CPU uses 100% per core. Summed RSS
can double-count shared pages. Old peers/absent metrics are unknown, never zeros.
Literal loopback peers share a process namespace; remote PIDs must never be matched
against the Mac's inventory. PID plus start time should underpin future attribution.

Current inventory API is OWNER-ONLY. Paired terminal grants do not expose cwd,
executable paths, project names, ports or full process inventory. Add a separate
resource-observe grant with an allowlisted, bounded projection before phone sharing.
Resource control (kill, quota, energy, traffic/firewall) is separate authority.
Discovery of a listening port is not proof that an API is healthy. Instrumented
services must publish declared health/freshness/coverage without bodies or secrets.
No portable service can truthfully promise visibility into every sandboxed process,
GPU workload, battery cost or API on every operating system.

## Structured activity without corrupting the terminal

Raw ANSI output remains canonical for terminal rendering. Never remove/reorder lines
or run regex folding over a cursor-addressed TUI. Claude already folds some tool output;
its controls are part of that application's own screen and may require controller input.

A separate, responsive activity view can fold tool calls/results, repeated progress,
test/build logs, diffs, retrieval batches and explicit context compactions. Retain
original event references, expandable payloads, timestamps, source and sequence.
Decisions, mistakes and learnings are agent/user annotations with provenance, not
facts deduced reliably from terminal colors. Permission-filter before indexing/sharing.

The current 1 MiB ring is NOT a full historical archive. Lossless lifetime history
requires an encrypted append log, geometry/mode events, checkpoints, quotas, export,
backup/restore and an explicit retention policy. Disk full must become visible, not
silently lose history. Redacted or expired data must not be described as complete.

Phone observation in this audit: Rocky was actively producing tool/progress output,
phone remained Watching, desktop-width lines exceeded the viewport. This establishes
an actual usability problem, not measured network latency or pixel equivalence.
No input was sent to Rocky. Prioritize readable observer navigation and an optional
structured view; do not seize control to make a screenshot fit.

## Delivery order and release gates

1. Correctness: atomic styled recovery, ordered geometry/output, flow control,
   lifecycle/IME/paste and controller-only resize on real TUIs.
2. Component: separate client/view/model; independent two-view embedding example;
   minimized/panel/maximized and accessible mobile acceptance.
3. Device operations: live resource view, explicit remote resource grants, collector
   packaging and supported metrics matrix; persistent discovery and recovery.
4. Core.access adapter: keep remote execution on Core; reuse rooms/navigation. Verify
   session identity, authorization, detach semantics and backward-compatible rollout.
5. Public alpha: signed reproducible artifacts, clean-machine three-device exercise,
   documented limitations/security reporting and pinned dependency/license inventory.
6. Optional structured worldline, context/model/speech, sandbox and sovereign services.

No rewrite of xterm or WireGuard is on the critical path without a demonstrated
failure that upstream integration cannot address.
