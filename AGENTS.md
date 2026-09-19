# DOT Terminal development

Keep changes reviewable and preserve user work. Do not publish secrets, live terminal
captures, private configuration, or local machine paths. Original code is Apache-2.0;
preserve imported licenses and attribution.

Current implemented scope is documented in README. Do not present planned features
as working. Protocol/core must remain independent of OS, renderer, and model runtime.

Before handoff: cargo fmt --all --check; cargo clippy --workspace --all-targets
--locked -- -D warnings; cargo test --workspace --locked. Behavior changes need
meaningful regression coverage. Publishing/installing is separate from passing tests.

## Resume safely

Read `docs/CONTINUITY.md` first, then README and the relevant component docs. Verify
`git status`, branch, HEAD and the latest PR/check state before editing. A previous
agent's report is evidence to re-check, not proof of current deployment.

The user actively uses the desktop and browser. Do not quit/restart the desktop,
kill a keeper/gateway, disconnect a browser, change live bindings, take over an active
PTY or overwrite a running app bundle as part of routine development. Test with
isolated directories, ephemeral ports and disposable sessions. Never type tests into
an existing user shell. Do not expose the desktop's loopback authority token on LAN.
A reload/restart is an operational change, not a build step. Keep both UI services up.

## Commit and merge discipline

- One bounded concern per branch/commit. Stage explicit paths, never all changes in
  a shared checkout. Inspect staged diffs and scan for private data before pushing.
- Preserve unrelated edits and nested repositories. Never reset/clean/stash someone
  else's work. Do not import private keys, real receipts, terminal output or inventories.
- Record actual checks and their scope. Use signed-off commits. Open a PR; wait for
  required checks on the exact head; merge only that head. Do not force-push main.
- Update continuity before long experiments and at every handoff. Record known gaps,
  next bounded task, public source SHA and deployment mismatch; never write credentials.
- If usage ends mid-task, the next agent must be able to resume from files alone.
  Do not mark unfinished behavior as implemented or merge solely to leave a clean tree.

## Identity and security invariants

A workspace folder, model name, prompt, self-asserted agent ID, IP address or discovery
advertisement grants no authority. Person/owner, device, agent workload and run IDs are
separate. Permissions must be owner-issued, scoped, short-lived, revocable and bound
to the actual caller or proof-of-possession key. Discovery does not enroll a device.
Do not retrieve/copy/sign with another agent's private identity (including Oracle seats).
Do not claim encryption during ordinary environment-variable use. Never place master
keys in agent context. A signed event proves attribution/integrity, not truthful execution,
complete capture or immutable global history. Preserve concurrent worldline branches.

## Reuse and integration

Older DOT repositories are reference material until their location, license, revision,
working-tree state and behavior are checked. Prefer protocol adapters and conformance
fixtures to merging unrelated trees. Specifications and mocks are not runtime support.
Keep private local inventory and live connection details outside the public repository.
