# DOT Terminal development

Keep changes reviewable and preserve user work. Do not publish secrets, live terminal
captures, private configuration, or local machine paths. Original code is Apache-2.0;
preserve imported licenses and attribution.

Current implemented scope is documented in README. Do not present planned features
as working. Protocol/core must remain independent of OS, renderer, and model runtime.

Before handoff: cargo fmt --all --check; cargo clippy --workspace --all-targets
--locked -- -D warnings; cargo test --workspace --locked. Behavior changes need
meaningful regression coverage. Publishing/installing is separate from passing tests.
