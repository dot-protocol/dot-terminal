# Session rendering and synchronization

## Layers and ownership

The keeper owns PTY lifetime, output ordering and controller fencing. The gateway
owns device authentication and service authorization. Clients own presentation,
input composition and local viewport. Shell navigation owns which session is shown;
it must not create terminal authority merely by selecting a view.

Desktop layout is in `shell.js`; appearance and metadata observation are separate
modules. `session-signals.js` owns bounded local measurements and serialized resize
scheduling. Android has a matching content-free `SessionSignals` foundation. This
separation is incremental: transport and session UI orchestration still reside in
main.js/MainActivity. It is not a finished plugin/module platform.

## Implemented measurements

Desktop tracks response and parsed offsets, pending parse bytes, history gaps,
response freshness, and the latest 120 read/parse/input-ACK/resize timing samples
with p50/p95. It emits `dot:session-state`, schema `dot.session-view.v1`. Values are
operational metadata; no session IDs, text, input, secret names or credentials.
Android tracks screen/input/resize request timing, UI-apply delay, errors and age.
Its measurements stay local to the app. The expandable key menu follows the theme.

A caught-up response is NOT proof of host-head equality, peer equality, or displayed
pixels. The xterm write callback means parsing completed. Android applying a snapshot
means UI state was set, not a completed display scanout. Network latency, parser
latency, input acknowledgement and input-to-visible-echo are different metrics.
No peer registry or fleet-wide synchronization percentage is implemented here.

A viewer uses the host grid rather than independently fitting a different width.
Only the controller requests remote resize; requests are coalesced and serialized.
The viewer may pan when the host grid exceeds its viewport. Geometry is sampled before nonempty follower output (and periodically while idle),
not atomically ordered with output; rapid cross-device resize remains a
known limitation. Byte replay after historical size changes cannot reconstruct all
TUI state. A full solution requires geometry events and styled checkpoints.

WebGL is optional and falls back to the standard xterm renderer if unavailable or
lost. xterm remains the terminal emulator; GPU rendering cannot repair bad ordering,
incorrect character widths, incomplete snapshots or transport latency.

## Target protocol (not yet implemented)

Each authorized view reports a scoped opaque view ID, session incarnation, renderer
capabilities, latest received/applied offsets, geometry epoch, input sequence ACK,
monotonic sample times and local backlog. The keeper publishes the stream head and
bounded, expiring view receipts. Reject stale incarnations, backwards offsets,
claims beyond the head, unauthorized observers and unbounded reporter cardinality.
A peer report is an attributed claim, not proof of honest pixels or execution.

A long-lived mTLS stream carries independently bounded output, input/control and
health channels. Slow viewers must not starve input or force fast viewers to pause;
use bounded replay, credits and explicit gap recovery. Disconnecting a viewer never
kills the PTY. No automatic replay of input with an uncertain acknowledgement.

Health is a vector: transport RTT, output backlog, response age, apply delay,
geometry agreement, controller generation, API success/freshness and protocol
compatibility. Unknown, background-paused, stale and degraded are first-class.
Cross-device one-way timing needs clock uncertainty; initially compare local RTTs
and sequence/offset differences, not wall-clock subtraction across devices.

## Frontier and evaluation

Keep xterm compatibility while measuring DOM versus WebGL. Investigate streaming
flow control, synchronized-output support, Unicode/grapheme width, styled snapshots,
resize epochs and native Android accessibility before considering a new emulator.
Benchmark a reproducible fixture: ANSI colors, alternate screen, cursor edits,
wide/combining characters, bounded floods and Ctrl-C; repeat under delay/disconnect.
Measure input-to-observed-echo separately from ACK. Publish device, transport,
viewport, sample counts, p50/p95, errors and test limits with each result.

Sources: [xterm flow control](https://xtermjs.org/docs/guides/flowcontrol/),
[xterm APIs](https://xtermjs.org/docs/api/terminal/classes/terminal/),
[WebGL addon](https://github.com/xtermjs/xterm.js/tree/master/addons/addon-webgl),
[Android input methods](https://developer.android.com/develop/ui/views/touch-and-input/creating-input-method),
[Gboard themes](https://support.google.com/gboard/answer/6102154?hl=en).
DOT controls its accessory/menu palette. The user's IME controls the main keyboard;
we do not change global keyboard settings silently or promise per-app Gboard colors.

## Initial evidence (2026-09-19)

On isolated loopback previews, browser read requests showed p50/p95 3/8 ms and
native WKWebView 7/12 ms in one rolling window (120 samples). Native input ACK p95
was 14 ms over 47 calls. These are observed local request timings, not a promised
SLO, comparative benchmark, phone measurement or input-to-pixel latency. Browser
ANSI red/green, Chinese characters and a combining accent were visible; native ANSI
blue and direct input were visible. Read-only font changes did not resize the host.
Phone physical verification is pending reconnection; build/unit/lint passed.

## Shared real-agent acceptance

A separate native lab bundle can be built with `--test-app --view-state-dir PATH`
when the owner explicitly requests viewing an existing session. This uses the same
keeper directory; it must not receive fixture commands or be cleaned up with session
stop. It does not replace the installed desktop. Test real agent redraw, streaming,
scrollback and control handoff with one controller and additional read-only viewers.
Do not equate a successful placeholder print/resize with a full agent TUI test.

Shared-view mode rejects session creation on the backend and disables vault access.
Controller resize drains parsing and sets the local grid before requesting host
resize, since the process can redraw before the acknowledgement arrives.
