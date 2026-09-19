# Pair by QR; hand off a running session

Both devices need an IP route (for example the same Wi-Fi or an existing private VPN).
USB is for installation/debugging, not normal DOT transport. This does not yet provide
NAT traversal, device discovery, a relay, signed OTA updates or multi-session routing.
The node gateway currently targets one keeper. The device identity and grants are
independent of the selected network address.

## Pairing

On the host, initialize a node identity and serve the selected keeper as described in
android-app.md. Create a short-lived invitation on a separate temporary port:

```sh
./target/debug/dot-terminal-node --state-dir "$NODE_STATE" invite \
  --listen 0.0.0.0:17846 --host "$REACHABLE_HOST" --service-port 17843 \
  --name phone --qr pairing.svg --terminal --clipboard-read --clipboard-write
```

Open the private SVG on the host. On Android choose **Scan QR**, scan it, review the
grants and choose **Request pairing**. Compare the 16-character confirmation code on
both displays and type that code into the host prompt. Do not approve a code supplied
by an unknown remote party. A matching code binds the invitation, server key, client
certificate, requested grants and service address. The phone signs a domain-separated
transcript with its existing non-exportable Android Keystore key. The host verifies
possession before asking for approval. An unauthenticated invitation endpoint cannot
execute terminal or clipboard operations. Normal service traffic still requires mTLS.

The `.invite` sidecar contains exactly the QR payload, for the **Pair link** field.
Both it and the SVG are private capabilities (0600); never commit or publicly share
them. They are removed after normal approval/decline/expiry. A killed host process may
leave expired files; delete them. The default lifetime is 120 seconds, maximum 300.
A bounded request deadline and limited attempts constrain malformed clients.
If the approval response is lost, the host may have enrolled the device while the
phone retained its previous profile. Inspect/revoke the named peer or repeat an
invitation for that same peer/key. New enrollment never silently replaces a different
key with the same name. Device replacement/recovery is not implemented.

## Handoff

Android **Handoff** offers the current session for two minutes. The sender retains
control until a recipient accepts. The ticket is consumed atomically and the old
generation is immediately fenced. Closing the offer cancels it; a new controller,
release, expiry or successful acceptance invalidates it. Pairing grants do not change.
The capsule binds the host node identity, session ID and high-entropy one-use ticket.
Android refuses a capsule for a different paired host or session.

A local CLI can produce an offer:

```sh
./target/debug/dot-terminal handoff "$SESSION" --node-id "$NODE_ID" --qr handoff.svg
# If another controller is active, use --takeover only deliberately.
./target/debug/dot-terminal attach "$SESSION" --handoff-file handoff.handoff
```

The local CLI already has owner access to its private keeper socket. Android reaches
that same keeper through the paired service. The shell is never moved to the phone:
its execution stays on the owner machine and input authority moves between views.
