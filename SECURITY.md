# Security policy

This is an early runtime foundation, not an audited security boundary.

The current keeper trusts the operating-system user. A different process running
as the same UID can control sessions. Private directories protect against other
ordinary users; they do not isolate agents from their owner or defend against root.
Do not run untrusted commands or expose this socket through an unauthenticated bridge.

There is no public network listener, provider credential store, encrypted durable
history, or remote enrollment in this version. History is bounded in RAM; ordinary
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
