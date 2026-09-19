# Appearance and observable UI state

## Available now

Desktop/web Appearance offers Forest, Midnight, Paper and High contrast palettes;
terminal size 8–40 px (including fractional values), interface size 12–24 px,
line height 1–2, and an explicit local font/fallback list. CSS semantic colors and
xterm colors change together. Preferences are local, versioned and validated;
they do not yet synchronize across devices. Reduce decorative text hides ambient copy.
Fonts must already exist on the device; no network font request is made.

At phone width the web session sidebar becomes a drawer and controls use larger targets.
Android has matching named palettes, a saved 8–32 sp terminal size and local font-family
field. Terminal text no longer shrinks to fit an entire snapshot; drag to pan around it.
Devices/clipboard controls collapse to leave more screen for the terminal. Android still
uses monochrome screen snapshots, not the desktop xterm renderer. Palette values are
currently mirrored in Java and JS; generated cross-platform tokens remain future work.

## Text and state indexing

The initial web shell copy is indexed locally with text tokens, static/dynamic kind,
primary/secondary role and signal/ambient classification. Six dynamic slots expose only
metadata. The index does not inspect terminal output, user text, secret names/values,
API payloads, arbitrary program variables or owner-tool dialog contents. Current static
copy IDs are build-local; stable authored localization IDs are a follow-up before external
consumers persist references. This is explicit coverage, not every word in every process.

System shows observed request counts, failures, latest latency, in-flight count and
freshness for every allowlisted desktop API operation. Unknown means not observed;
stale means no response for 15 seconds; neither implies a failed server. Observed errors
carry no raw response body. There are no extra network health polls.

The visible view emits `dot:state` CustomEvents once per second while foregrounded:
`dot.ui.state.v1`, module `desktop-view`, timestamp, view kind, control-held flag,
input-queue bytes, output-poll status and bounded API summaries. State field types,
units and operational privacy labels live in `observability.js`. This is in-process
telemetry, not a mesh event bus, signed evidence or comprehensive server health.
Further modules must declare their state/privacy contracts rather than dump their heap.

## Safe preview and wireless updates

The desktop server has `--disable-vault` for isolated previews. Use separate private
runtime state with a short path (macOS Unix-domain socket paths are length-limited),
ephemeral loopback listeners and disposable shells. Never expose the owner's loopback
capability on LAN. Existing desktop/native bundle and user sessions stay running.

Android development installs can travel over paired wireless ADB. The pairing port and
connection port differ; pairing may not auto-connect when multicast discovery is blocked.
This updates a signed APK and reloads its Activity, not live native-code replacement.
DOT device pairing is independent of ADB authorization. Production signed OTA delivery,
rollback protection and consent UI are not implemented. Do not put pairing codes or
private addresses in commits. Debug connections may need reconnecting after network changes.

## Verification performed

- JS tests: corrupt settings, precise sizes/bounds, font validation, endpoint normalization,
  unknown/error/stale state, concurrent requests and telemetry field exclusion.
- Web production build; Rust format, workspace clippy and workspace tests passed.
- Android build and lint passed; APK installed successfully over wireless ADB.
- In-app browser: Paper theme with 17.5 px terminal and 16 px interface persisted across
  reload; a 390 px viewport had no document overflow. Drawer and System index were checked.
- A disposable PTY was created in a short isolated state directory; changing to Midnight
  and 18 px updated the visible terminal while preserving the shell PID. Automated text
  entry through the browser tool failed before visible input; do not call that an input test.
- Real Android UI: Appearance opens and a 16.5 sp setting remains after Apply and reopening.

No native desktop bundle was replaced. Browser preview is the updated desktop/web
surface; the already-running installed app adopts these assets only through a later
explicit deployment/reload. No iPad deployment or whole-system telemetry was tested.
