# DOT Terminal

**An open Rust terminal workspace for people and agents, across devices.**

[![CI](https://github.com/dot-protocol/dot-terminal/actions/workflows/ci.yml/badge.svg)](https://github.com/dot-protocol/dot-terminal/actions/workflows/ci.yml)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

Start work on one machine. Keep the session alive. Return from another interface.
DOT Terminal is building a portable execution and session foundation for macOS,
Linux servers, and Android, with explicit ownership and agent permissions.

**Status: early implementation, not a finished terminal emulator.** The current
release is a local Unix PTY keeper and diagnostic/interactive CLI. The graphical
terminal, remote device pairing, Android app, encrypted durable storage, and
agent context engine are planned. Do not use this version for untrusted code.

## Try the working foundation

Requires a current stable Rust toolchain and macOS or Linux. Windows runtime and
Android packaging are not available yet. No cloud account or API key required.

```sh
git clone https://github.com/dot-protocol/dot-terminal.git
cd dot-terminal
cargo build --locked

# The shell survives after this command exits.
SESSION=$(./target/debug/dot-terminal new -- /bin/sh)
./target/debug/dot-terminal list
./target/debug/dot-terminal attach "$SESSION"
# Press Ctrl-] to detach. Attach again to return.
./target/debug/dot-terminal attach "$SESSION"
./target/debug/dot-terminal stop "$SESSION"
```

An existing CLI agent can be started in place of `/bin/sh`. It retains its normal
host permissions: this keeper is not a sandbox. Sessions started elsewhere are
not automatically adopted.

For explicit handoff between two local terminal windows:

```sh
./target/debug/dot-terminal attach "$SESSION" --takeover
```

The previous controller's subsequent writes and resizes are rejected. A detached
CLI releases control; an abruptly killed client requires explicit takeover.

`read ID --after OFFSET` returns byte history as JSON without rendering escape
sequences. `attach` replays raw bytes through your existing terminal. Full-screen
snapshot reconstruction, advanced keyboard modes, mouse forwarding, and terminal
clipboard filtering are not implemented. Use only trusted commands and output.

## What works today

- A detached keeper per session owns the PTY independently of the launching CLI.
- Strict versioned, length-limited control messages over private Unix sockets.
- Controller generations fence stale input and resize requests.
- Ordered input batches deduplicate the last acknowledged request and reject
  conflicting/out-of-order requests. Ambiguous writes are never automatically retried.
- A bounded 1 MiB output ring exposes byte offsets and explicit truncation gaps.
- Exited sessions retain output until stopped; no terminal content is written to disk.
- Private runtime directory (0700), socket permissions (0600), bounded connections,
  and request deadlines. Idle keepers block on I/O rather than poll continuously.
- Unit and real-process integration tests; macOS and Linux CI.

## Architecture

```mermaid
flowchart LR
  Client[CLI now / graphical clients next] --> Protocol[Versioned protocol]
  Protocol --> Keeper[Independent session keeper]
  Keeper --> PTY[PTY and shell or agent]
  Keeper --> Ownership[Controller fencing]
  Keeper --> History[Bounded output history]
```

A reproducible [Android native probe](docs/android.md) exercises the portable core
on a connected ARM64 phone, without claiming a finished Android app.

The [architecture](docs/architecture.md) defines the larger system. The
[roadmap](ROADMAP.md) distinguishes implemented behavior from release gates.
The [protocol](docs/protocol.md) describes current failure semantics.

The design draws on AXXIS's independent PTY-holder work and mature upstream
terminal projects. See [reuse decisions](docs/reuse-audit.md) and [NOTICE](NOTICE).
We reuse components where their contracts fit; old product boundaries do not
constrain the new design.

## Build with us

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Contributions with reproductions and measured results are especially valuable:
terminal snapshot correctness, Unicode/input methods, Android lifecycle handling,
slow-client backpressure, and cross-platform process supervision.

Read [CONTRIBUTING](CONTRIBUTING.md), [SECURITY](SECURITY.md), and the
[community guidelines](CODE_OF_CONDUCT.md). No claims of production security,
universal device support, or performance superiority are made by this prototype.

Apache-2.0 for original work; upstream dependencies retain their own licenses.
