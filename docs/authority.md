# Experimental operation authority

`dot-terminal-authority` is a Rust library, not a running broker, sandbox or secret store.
Nothing calls a provider, exposes a listener or reads a vault key. Use synthetic data only.

## Implemented

- Direct owner-signed Ed25519 grants bind subject key, run, exact audience/resource/action,
  time interval, policy epoch, unique grant ID and operation-count budget.
- Subject signatures bind the exact request fields, nonce and SHA-256 body digest.
- Versioned domain-separated signing bytes use typed compact serde JSON with fixed field
  order, no maps/floats, and unknown fields rejected. Interoperating languages need byte
  conformance vectors before this becomes a stable public wire format.
- `Journal::authorize` checks trusted policy, signatures and scope, then atomically reserves
  an operation and budget in SQLite before returning an opaque permit. Duplicate nonces,
  exhausted grants and changed terms under an already-used grant ID are rejected.
- SQLite immediate transactions serialize competing reservations across connections.
  Replay rejection rolls back the budget increment. Reservation persists across reopen.
- Completion records succeeded, failed or unknown. Unfinished reservations stay pending
  and consume budget. No automatic retry after an ambiguous external side effect.

`verify` alone is not permission to dispatch: it does not reserve budget or replay state.
The trusted adapter must use `authorize`, dispatch precisely the signed operation/body,
and then finish the returned permit. A library cannot prevent its embedding process
from ignoring these rules. The agent must eventually run outside that trusted process.

## Explicit limits before production integration

The journal is **not encrypted or cryptographically authenticated**. It stores grant and
request hashes/IDs, counts and outcomes, not request bodies or secret values. Its filesystem
location, access control and encryption are the embedding broker's responsibility. Disk
rollback/tampering, untrusted schema files and hostile administrators are not resisted.
No production authority database is created by tests or desktop startup.

Trusted caller supplies owner trust root, current audience, revocations, policy epoch and
clock. Keep policy stable throughout authorization and enforce rollback-resistant policy
updates in the future broker. Network inputs need bounded parsing and rate limits before
this API. There is no OS workload attestation, hardware key integration, grant issuance UI,
delegation, currency budget, provider connector, denial audit, signed worldline, recovery,
metadata encryption, or pending-operation reconciliation interface yet. A nonce is
replay protection, not proof of human presence or a complete challenge-response protocol.

This is an internal version-1 experiment. Maximum uses means reservations, not successful
operations or provider spend. Newly issued grant IDs create separate budgets; shared
account-level budgets require another authoritative transaction boundary.

## Validation

Synthetic tests cover wrong owner/subject signatures, modified grants and requests,
wrong resource/action/run/audience/body, future/expired grants, policy epoch/revocation,
unknown fields, nonce replay, close/reopen with unfinished intent, budget exhaustion,
changed grant terms, and two connections racing for one remaining use. These tests do
not establish power-loss behavior, real provider idempotency or production security.

Next: add a trusted broker process with private encrypted state, OS caller binding,
bounded transport parsing, policy/revocation management and a synthetic operation
adapter. Only then integrate a narrowly scoped real secret operation with user-visible
permissions and a tested recovery path. Keep the existing desktop and gateway running.
