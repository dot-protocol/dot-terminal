# Devices and nodes

Goal: every terminal session on any of the owner's devices is usable from any of the owner's
views. This document states what is built, exactly.

## Model

A **node** is the desktop backend (`dot-terminal-desktop`) plus the keepers it starts. Every device
that hosts sessions runs one. A **view** (Mac app, browser, later the phone app) talks only to its
own node. That node lists the owner's other nodes from `<state-dir>/devices.json` and relays
session calls to them, so a remote node's capability never reaches page JavaScript.

```
view ──loopback──▶ local node ──overlay HTTP + capability──▶ remote node ──unix socket──▶ keeper ─▶ PTY
```

- `GET /api/devices` → this device and the configured remotes with `connected | offline | refused`.
- `GET|POST /api/devices/{device}/sessions`, `POST /api/devices/{device}/sessions/{id}`: relayed.
- The sidebar is Devices → their sessions; a session belongs to one device and every call for it
  goes through that device's route. Backends without the catalog appear as one local device.

## Transport and its limits

Node-to-node traffic is plain HTTP carrying a bearer capability. It is accepted ONLY on loopback or
on 100.64.0.0/10, the range used by WireGuard overlays (Tailscale now; a self-hosted Headscale
later needs no DOT change). Both `--listen` and device URLs are checked; anything else is refused
at start-up. The overlay provides encryption and peer authentication; DOT adds the capability.
Consequences, stated plainly:
- Every machine on the same overlay network can reach the port. Without the capability they get
  401. Use overlay ACLs to narrow who can connect.
- The capability is a long-lived shared secret in two private files (`--capability-file` on the
  node, `devices.json` on the hub, both required to be owner-only). Rotating it means editing both.
  This is NOT the device-identity, mutual-TLS pairing the phone path uses; unifying the two is open.
- `devices.json` is written by hand today. There is no pairing UI for nodes.

## Running a node on a server

Build `dot-terminal` and `dot-terminal-desktop`, copy a built UI (`apps/desktop/dist`), then run as
an unprivileged user that owns a 0700 state directory:

```
dot-terminal-desktop --assets <ui> --session-binary <dot-terminal> --state-dir <dir> \
  --disable-vault --listen <overlay-ip>:7420 --capability-file <file> --name "core VPS" --kind server
```

Under systemd use `KillMode=process` (keepers must outlive a gateway restart) and send stdout to
null (the start-up line contains the capability). A session on that node is a shell as that user.

## Verified / not verified

Verified (2026-09-19): a node on a Linux VPS as a systemd service under an unprivileged user,
listening only on its Tailscale address; a Mac node with that VPS in `devices.json`; from a Mac
browser view, the VPS appeared under Devices, a session was created on it, and typing in the view
ran a command there. Presence showed the Mac's device name. Observed once: the first outbound
connection from a newly built binary on macOS stalled ~5 s, then all later ones were instant, so a
device can show "offline" for one refresh after an update.
Not verified: the Mac app (WKWebView), Android as a view or as a node, latency over a poor link
(output is still polled, not pushed), capability rotation, more than one remote.
