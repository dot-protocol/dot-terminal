# Existing DOT reuse review

Source inspection, 2026-09-19. Paths below are relative to the older Kin checkout;
that checkout is private and these paths are inventory, not claims of published packages.
Kin HEAD observed: `4011e6c3aae346e05263c89ba478a3ba7cff7dec`; the checkout has unrelated
changes. Nothing there was edited, committed, reset, deployed or copied into this repo.
This is a bounded review, not a security audit or exhaustive repository inventory.

| Source | Evidence read | Reuse decision |
| --- | --- | --- |
| `dot/protocol/dot-protocol/packages/crypto` | `src/index.ts` session derivation, handoff creation/verification; `src/tests/session-worldline.test.ts` | Reuse public handoff format ideas and conformance vectors. A valid self-signed handoff still needs a trusted current key and authorization. Deterministic keys from a master do not provide forward secrecy after master compromise. |
| `dot/storage/dotfs-core` | `log.py`, `test_dotfs_core.py`, license | Useful typed, signed, hash-linked event log and materialized-view design. Current imports assume a sibling `dna-fs` path that moved. Repair packaging in an isolated adapter before integration. |
| `dot/protocol/dot-protocol/spec/16-dotfs.md` and `packages/dotfs/src/cli.ts` | Container/key-wrap/QR design; sidecar serialization | Explore encrypted artifact manifests and selective disclosure. **Do not reuse sidecar export as-is:** it serializes `seedHex`. A transfer manifest must never carry an unwrapped private seed. |
| `tree/crates/tree-core` and `tree/Cargo.toml` | Observation/observer/proof types and six-crate Rust workspace | Candidate schema adapter; not evidence of a sandboxed DOT instruction executor. Historical migration audit calls it a DOT VM prototype. No production Oracle claim inferred from these local files. |
| `docs/superpowers/specs/2026-05-08-piperchat-v2-design.md` | DOT Assembly references: SEED, ENCAP, SIGN/VERIFY, EMIT | Instruction vocabulary/specification found, not a verified assembler/runtime. Keep executable instruction semantics, resource metering and sandbox guarantees as separate future work. |
| `dot/transfer/localsend-fork` | Historical NOTES, clean nested upstream checkout at `22d6f2b`, upstream protocol | Reuse research and test interoperability through an adapter. Historical notes about TLS pinning/builds are not a fresh upstream vulnerability finding or current build result. |

## License boundary

The DOT protocol root has an MIT license; crypto package metadata also declares MIT.
DOTFS core has an Apache-2.0 license and source headers. The LocalSend nested checkout
contains Apache-2.0 license text. `tree/Cargo.toml` declares MIT but no `tree/LICENSE`
was present: resolve applicable parent license, authorship and notices before copying.
Check each selected file/dependency and preserve notices at import time. No code was
imported by this review; a package's use of audited crypto dependencies does not make
its surrounding protocol or permission model audited.

## Integration order

1. Specify a versioned Rust capability/worldline contract and rejection vectors.
2. Compare synthetic DOT Crypto handoff fixtures; trusted-key and grant checks must
   surround signature verification. Hardware-backed P-256 requires an explicit algorithm
   adapter, not a silent rewrite of Ed25519 wire formats.
3. Adapt DOTFS encrypted blobs/events behind storage interfaces, without private-seed
   exports. Define migration, crash consistency, concurrent heads and restore behavior.
4. Add LocalSend file compatibility separately from trusted DOT pairing. Untrusted LAN
   discovery cannot authorize terminal input, clipboard access or secret use.
5. Evaluate Tree/Assembly only for bounded execution needs after defining sandbox and
   resource limits. Do not merge its server or change Oracle ownership/runtime.

Tests in older sources were inspected, not executed in this review. Live device mesh,
recovery and hardware security remain unverified beyond the baseline in CONTINUITY.md.
