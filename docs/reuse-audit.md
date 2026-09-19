# Reuse audit — 19 September 2026

Scope: source-first audit of the existing dot collection, concentrating on terminal,
protocol and execution candidates. This is not a production deployment audit or
an exhaustive review of every business project in the collection.

The collection contains nested repositories, dirty working trees and unrelated
private data. No source tree, credential store or runtime data was bulk imported.
Original working trees were left unchanged.

| Candidate | Evidence | Decision |
|---|---|---|
| AXXIS independent PTY holder | `apps/axxisd/src/ptyd.rs`; separate process, private socket, bounded history | Adopt the process-ownership principle; implement a smaller independent keeper |
| AXXIS terminal protocol | `crates/axxis-terminal-protocol/src/lib.rs`; strict v1 plus documented legacy fallback | Reuse strictness/portability lessons; fresh protocol rejects unsupported versions without fallback |
| AXXIS portable domain types | `crates/axxis-protocol/src/`; session/events/resources | Reference for boundaries; avoid product-specific event and resource coupling |
| AXXIS terminal corpus | `apps/axxisd/benchmarks/README.md`; generated streams and sidecar events | Reuse the synthetic-fixture method; import specific fixtures only after compatibility/provenance review |
| Piano / Alacritty fork | Alacritty workspace plus modified rendering/event files; extra Tauri shell | Evaluate upstream terminal library directly; do not inherit the whole dirty fork |
| Piano shell | `piano-shell/src-tauri/src/lib.rs`; UI, PTY, browser and logging in one module | Reference UX only; split execution from presentation in the new product |
| DOT signed observations | `dot-protocol/README.md`; signed/hash-linked observation format | Possible later evidence adapter; not the terminal transport or a substitute for reviewed cryptography |
| Human layer | `human-layer/README.md`; platform adapters and explicit interaction evidence | Later computer-use integration; not a base dependency |
| Kernel HTML, browser tools, archives, business apps | Directory inventory | Defer; unrelated to the first session runtime, no data imported |

AXXIS inspected checkout baseline: `e5b87ba44e37e22c7f69cf67937c0e42b6ff7423`.
Piano inspected checkout baseline: `d692748d3f61253ebe9f5094320120d22f6a046f`.
Working-tree modifications mean these hashes alone do not reproduce all observed
files. This audit is evidence about the local snapshot, not a claim about upstream HEAD.

A scoped AST graph of AXXIS's 39 daemon source files contained 1,141 nodes and 2,791
edges (36 communities). No semantic extraction model calls were made. The graph
located the holder/framing cluster; direct source inspection established the findings.
The pre-existing graph directory did not contain a completed graph.

Specific reasons to rewrite the keeper boundary: the older broadcast path iterates
client writes while holding the client-list mutex (`ptyd.rs`, broadcast); raw replay
is not a screen snapshot; old protocol compatibility is not required for a new repo.
The new keeper uses bounded pull reads, independent input ownership and explicit
failure responses. It still needs the larger architecture's renderer, auth and storage.

License evidence: AXXIS root MIT text; Piano root Apache-2.0 and MIT texts. We have
not assumed root licenses cover every transitive dependency or model weight.
AXXIS attribution is preserved in NOTICE and licenses/AXXIS-MIT.txt.

Upstream reference: https://github.com/axxis-world/axxis
Terminal reference: https://github.com/alacritty/alacritty
PTY reference: https://github.com/wezterm/wezterm/tree/main/pty
