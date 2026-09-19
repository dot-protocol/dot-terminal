# Android terminal input and control

The Android screen now prioritizes the terminal, with a compact heading, explicit
View/Control/Disconnect actions, collapsed tools and an optional command composer.
Tap the terminal to open the keyboard. The horizontally scrollable special-key row
provides Esc, Tab, one-shot Ctrl, arrows, backspace, Enter and keyboard access.
Appearance retains exact font size and family preferences.

View polls snapshots without acquiring control. Control first requests a free lease;
if unavailable, takeover requires a separate explicit confirmation. Session provides
release-to-view and explains that the current gateway exposes one session. It is not
a multi-session catalog. Disconnect releases control and stops viewing; the host
process continues. Reconnection is explicit and uncertain input is not retried.

The input connection holds uncommitted IME composition locally and sends committed
text. Multi-character pasted text containing line breaks or Escape requires review.
This is not a comprehensive hostile-paste filter. Hardware special keys and Ctrl/Alt
characters are translated to conventional terminal bytes; application-cursor/keypad
modes, dead-key composition and all IME combinations are not fully supported.
Input is briefly batched and fenced to its observed controller generation.

The view measures font cells and keyboard-adjusted bounds, debounces resize, and only
sends dimensions while controlling. Dimensions respect protocol limits. Read-only
viewers pan without changing the PTY. Snapshots remain monochrome and do not provide
full scrollback/selection, styled rendering or a native local shell.

## UI state/copy contract

| Stable conceptual ID | Kind | Role | Importance | Privacy |
|---|---|---|---|---|
| mobile.session.title | dynamic | primary | signal | local session identifier; never telemetry |
| mobile.connection.status | dynamic | primary | signal | local operational state only |
| mobile.action.view | static | primary | signal | public UI copy |
| mobile.action.control | static | primary | signal | public UI copy |
| mobile.action.disconnect | static | secondary | signal | public UI copy |
| mobile.tools | static | secondary | ambient | public UI copy |
| mobile.input.modifier | dynamic | primary | signal | local modifier state only |
| mobile.terminal.viewport | dynamic | secondary | signal | dimensions only |
| mobile.terminal.content | dynamic | primary | signal | private; excluded from indexing/telemetry |

This documents coverage; it does not add Android-to-mesh telemetry or index commands.
The terminal accessibility description remains local Android accessibility data.

## Test isolation

Gradle `-PdotTestApp` builds the debug application as a separate `.uxlab` installation,
with its own device identity/preferences. Production paired settings are not copied.
Use a disposable keeper and development-only loopback bridge. Remove the test install
and forwarding rules afterwards. The standard build helper includes Java unit tests.

## Validation (2026-09-19)

On the physical Android device, the isolated debug app demonstrated: view without
acquiring control; cancelled takeover preserving the other controller; direct command
input and visible response; Ctrl-C interrupt; measured remote resize from 22 to 13
rows when the keyboard appeared and back to 22 when hidden; release-to-view allowing
another controller; read-only viewing preserving that controller's 80×24 dimensions;
and disconnect freeing control without terminating the keeper. Tests used a temporary
wireless-debugging bridge to a disposable Mac shell, not production mesh traffic.

The test found stale HTTP connection reuse in the development bridge. Requests and
responses now explicitly close that HTTP connection. Input remains non-retrying.
Java key/geometry unit tests, Android build/lint, Rust workspace checks and Python
bridge tests passed. IME composition code is implemented but CJK, dictation, complex
Unicode, full-screen TUI modes and prolonged roaming remain unverified on real devices.
No benchmark or latency percentile is claimed.
