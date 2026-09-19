# Android native core test

The native device probe executes the real `dot-terminal-core` and
`dot-terminal-protocol` crates on an ARM64 Android device. It checks binary input
roundtrips, malformed-frame rejection, bounded history, controller fencing,
deduplication and uncertain-input handling. A failed check exits nonzero.

## Run

Requirements: Python 3, adb, Rust's `aarch64-linux-android` target, an Android NDK,
and an authorized ARM64 phone with USB debugging. The runner supports macOS/Linux
build hosts. NDK 28.2.13676358 is the initial development toolchain; the binary
links against API 28. It is not an APK and requires no application installation.

```sh
rustup target add aarch64-linux-android
export ANDROID_NDK_HOME=/path/to/ndk/28.2.13676358
python3 scripts/android-smoke.py
```

If several devices are connected, set `ANDROID_SERIAL` to the chosen device.
The script builds a release executable, pushes it into a new temporary directory,
verifies its SHA-256 on the device, runs it, emits a JSON receipt, and removes the
temporary device directory. It does not collect the serial number in the receipt,
read personal application data, or use the microphone, network, or a model.

The receipt contains the source commit, whether the checkout was dirty, artifact
hash, model/OS, execution context and individual passing checks. It provides
bounded evidence about that binary on that device; it is not a general security
or performance certification.

## What this establishes

Actual native ARM64 Android execution of shared core/protocol behavior. CI executes
the same probe on desktop hosts; device execution is a separate manual validation.

## What remains

Execution currently uses the Android debugging shell. It does **not** establish
app-sandbox operation, background survival, a graphical terminal, PTY permissions,
model performance, or battery/thermal behavior. Those require a packaged Android
application and the architecture's real-device lifecycle tests.

## Recorded device run

On 19 September 2026, all 13 checks passed natively on a Moto G67 Power 5G
running Android 16 / API 36. The transferred executable's SHA-256 matched the
local artifact. The [receipt](validation/moto-g67-power-android16.json) records
the clean source revision and execution boundary; no device serial is published.
The temporary device executable was removed after the run.
