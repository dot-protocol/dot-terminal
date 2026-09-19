# Security policy

This is an early runtime foundation, not an audited security boundary.

The current keeper trusts the operating-system user. A different process running
as the same UID can control sessions. Private directories protect against other
ordinary users; they do not isolate agents from their owner or defend against root.
Do not run untrusted commands or expose this socket through an unauthenticated bridge.

The optional node gateway listens on an explicitly chosen IP address. It requires
mutual TLS and enrolled certificate pins; grants separate terminal access from
clipboard reads and writes. Enrollment is currently through authorized developer
ADB. A peer with terminal access can execute as the host user in the exposed
session: this is powerful access, not a sandbox. Such a shell may itself run
clipboard tools; API service grants do not isolate a terminal controller from the
host user's other privileges. Clipboard grants expose or replace
the host's current text clipboard on explicit phone action; Mac pbpaste cannot
reliably identify sensitive clipboard items. Pair only devices you control.
Revocation is checked before each service dispatch; an already authorized action
may finish. Disconnect does not revoke a device. Host identity files are permission
protected but not encrypted; Android private keys remain in Android Keystore.
There is no provider credential store or encrypted durable history. History is bounded in RAM; ordinary
process memory/swap can contain plaintext. Keeper diagnostic logs contain errors,
not an intentional terminal transcript, but command-start errors can reveal paths.

The diagnostic `read` command emits JSON. The interactive `attach` client forwards
raw terminal bytes and cannot yet filter hostile escape sequences or reconstruct
a complete screen after truncation. Treat output as trusted at this stage.

Input acknowledgements mean bytes were written to the PTY, not that a command ran
or its external effects completed. No exactly-once guarantee is made across keeper
crashes. Stopping kills the direct child through the PTY library; comprehensive
descendant containment needs the planned execution backend. Controller generations
are local fencing tokens, not cryptographic credentials or expiring distributed leases.

Report a vulnerability using GitHub's private vulnerability reporting for this
repository. If unavailable, open an issue requesting a private contact without
including exploit details, credentials, or private terminal content.
