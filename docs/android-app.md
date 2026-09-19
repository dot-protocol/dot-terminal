# Android development client

This is an installable ARM64 development app, not the production cross-device
mesh or a phone-local shell. The Android platform view is Java; all wire
requests and responses pass through the shared Rust protocol using JNI. The Mac
or Linux keeper owns the shell and Alacritty VT state. Closing or force-stopping
the phone app does not stop the remote shell.

## Wireless connection and clipboard

Build/install below, then initialize a host identity and explicitly grant services:

```sh
cargo build --locked -p dot-terminal -p dot-terminal-node
SESSION=$(./target/debug/dot-terminal new -- /bin/sh)
mkdir -m 700 .node-dev
./target/debug/dot-terminal-node --state-dir .node-dev init
python3 scripts/pair-android.py --serial YOUR_DEVICE --node-dir .node-dev \
  --host YOUR_REACHABLE_HOST_IP --name phone \
  --terminal --clipboard-read --clipboard-write
./target/debug/dot-terminal-node --state-dir .node-dev serve \
  --listen 0.0.0.0:17843 --socket "/tmp/dot-terminal-$(id -u)/$SESSION.sock"
```

Keep `.node-dev` private and outside version control. Use an interface-specific
listen address when possible. USB enrolls public certificates; the private phone
key stays in Android Keystore. Subsequent application requests use TLS 1.3 over
Wi-Fi or another reachable IP route, without adb forwarding. No Google account,
DOT account, VPN slot, or hosted coordinator is required on the same LAN.
Cross-network NAT traversal, relay, changing-address discovery and unattended
updates are not implemented. An existing VPN may provide the route.

**Send clipboard** explicitly reads foreground Android text and replaces Mac text.
**Get clipboard** reads Mac text and makes it available for ordinary Android paste.
**Undo** restores the previous Android clipboard while the app process remains
alive, unless it changed again. Contents are never executed as terminal input.
Text is limited to 64 KiB; Android items marked sensitive are refused on send.
Neither automatic background sync nor an encrypted clipboard history is provided.

Revoke a lost device with `dot-terminal-node --state-dir .node-dev revoke --name phone`.
Re-enrollment after app data loss requires explicit revocation and authorized
bootstrap; there is no automatic identity replacement or recovery service.

## Build

Requires stable Rust with the `aarch64-linux-android` target, Python 3, Java 21,
Android SDK platform 36, and NDK 28.2.13676358. Set `ANDROID_HOME`,
`ANDROID_NDK_HOME`, and `JAVA_HOME` for your installation. The build script uses
the checked-in Gradle wrapper and checksum, and builds the native Rust library
before packaging. The APK targets ARM64 Android 11+; only the documented Motorola
Android 16 device has been exercised. Native ELF alignment is 16 KiB.

```sh
rustup target add aarch64-linux-android
python3 scripts/build-android.py
adb -s YOUR_DEVICE install -r apps/android/app/build/outputs/apk/debug/app-debug.apk
cargo build --locked -p dot-terminal
SESSION=$(./target/debug/dot-terminal new -- /bin/sh)
python3 scripts/usb-bridge.py --serial YOUR_DEVICE \
  --socket "/tmp/dot-terminal-$(id -u)/$SESSION.sock"
```

On the phone, tap **Take control**. Type a command and press the return button.
Esc, Tab, Ctrl C, and arrow buttons send immediate terminal keys. Command entry
sends one complete line; full interactive IME editing is not implemented. Taking
control explicitly fences any previous controller, including a Mac CLI client.
Disconnect releases control and retains the shell. After connection loss or app
restart, tap Take control again. Ambiguous input is not automatically retried.

Stop the bridge with Ctrl-C to revoke its capability and remove USB forwarding.
Stop the keeper separately with `dot-terminal stop "$SESSION"`. A new bridge run
generates a new capability and pairs the debug app again.

## Boundary and remaining work

The bridge binds only `127.0.0.1:17842`, forwards to exactly one owned private Unix
socket, excludes stop/new/list, limits messages, and requires a random 256-bit
bearer capability. Android reaches it using adb reverse. The capability is never
printed or committed. Same-user Mac processes and the authorized adb connection
are trusted; same-user process inspection can see the pairing argument. Other
phone apps can reach the forwarded port but cannot act without the capability.

The optional legacy USB bridge below uses development HTTP. The paired IP route
above uses mutual TLS and never silently falls back to that bridge. Do not
publish the port or treat it as device enrollment. The release variant disables
the development connection entirely. App backup/device transfer exclude its
private files. Debug screenshots are allowed for testing; release windows are
marked secure. Debug APKs are locally signed and are not a public release channel.

The current view is monochrome, fixed at 48x24, with no scrollback browsing or
selection. Alacritty maintains cursor/erase/alternate-screen state; style/mode
metadata, terminal query replies, full Unicode layout and IME composition remain
work. Foreground polling stops in the background. No local inference, phone PTY,
VPS transport, encrypted persistence, or automatic recovery is claimed.

Next gates are styled snapshots, persistent event-driven connections, proper IME
composition, production pairing/discovery, and recovery. Each current request
performs a TLS handshake; battery, latency and concurrency are not yet optimized.
