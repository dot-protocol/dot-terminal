# Rocky: integrating DOT into Core.access

Blaze asked DOT to own terminal infrastructure and Core.access to own the surrounding
rooms/Oracle interface. Read PRODUCT-BOUNDARY.md and session-rendering.md first.
Source baseline: PR #34, e21412f; resource/audit branch: codex/device-resources-contract.
Do not merge this branch into an unrelated Core checkout or copy main.js wholesale.

The local AXXIS `apps/axxisd/ui/index.html` currently constructs xterm and owns fit and
resize observers. This read-only audit does not establish its deployed version.
Use that boundary as an adapter seam, not a reason to replace rooms or the desktop.

1. Agree on client/view interfaces, extract an independently mountable view in DOT,
   and demonstrate two instances with no global selectors or shared focus state.
2. Core.access supplies authenticated transport and existing session references.
   Preserve Core's remote execution. A local projection must not spawn duplicate
   shells/agent seats, create a parallel kernel or move VPS work to the Mac.
3. Bind visible tab identity to device + session incarnation. UI labels are metadata.
   Never infer identity from list index, display title or a reused PID.
4. Host navigation owns layout. DOT owns terminal input/render lifecycle. The activity
   pane gets its own layout column/row; it never overlays terminal cells or captures
   terminal shortcuts. Compact icons need accessible names and deliberate hit targets.
5. Exactly one view controls input AND PTY geometry. Preview/focus/tab selection does
   not grant authority. Dock/maximize uses container sizing; observer grids follow the
   controller. Input queued for a former tab must never reach the newly selected tab.
6. Keep uncertain input ACKs visible. Reconnect/detach must not replay commands or
   terminate a process. Use generation fencing and explicit gap/checkpoint recovery.
7. Add a feature-flagged adapter and preserve the current path until real TUI testing
   passes: ANSI, alternate screen, wide/combining text, IME, paste, Ctrl-C, background,
   two viewers, control transfer, resize, network delay/drop and history gaps.
8. Keep per-view transport/parser/ACK measurements distinct from input-to-pixel latency.
   Resource views show host-owned samples; browser requests cannot represent all APIs.

Do not fold raw ANSI with regex. Build an optional responsive event view from an
authorized adapter with expandable originals. Shell commands, tool batches, output
and compactions can fold there. Decisions/learnings need attributed annotations.
The current replay ring is bounded; lossless historical retention is a separate gate.

Operational rule: never build/restart old live runtime paths or send probes into a
user/agent terminal. Use a separate worktree, disposable PTY and isolated ports.
Record source SHA, installed SHA, actual tests and remaining gaps at every handoff.

## Package now available (2026-09-20)

Use `packages/terminal` on `codex/terminal-package`, version 0.1.0-alpha.1.
`npm pack ./packages/terminal` produces an installable artifact. Start with
`createClient({request})` and `mountWorkspace(container,{client})`; Core can instead
mount `mountTerminal` inside its own navigation. See package README and the independent
consumer. DOT's existing app now shares the packaged client, renderer and input paths.
Keep the integration feature-flagged: new view requires current keeper read_frame,
and history-gap recovery/hosted relay are incomplete. Supply Core's authenticated
server/native adapter; never pass a Mac owner bearer through the public website.
