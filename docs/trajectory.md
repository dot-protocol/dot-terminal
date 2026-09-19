# Session activity

The Activity button opens a pane beside the terminal. The terminal and the pane are siblings in
one grid row (`#workspace`): nothing is positioned over the terminal. A splitter between them
drags, takes arrow keys / Home / End, resets on double-click or Enter, is clamped to 240–560 px
and at most half the workspace, and is remembered on this device along with open/closed. Under
680 px the pane and the terminal share one cell and labelled Terminal / Activity tabs choose
which is visible; both keep their box, so switching views never resizes the PTY. Opening the
pane never takes control, resets or replays the terminal, and does not move focus; closing it
returns focus to the terminal only if focus was inside the pane. On a wide screen a controller's
normal resize observer resizes its PTY to the narrower terminal; a follower keeps the host grid.

## Live rows (bound to the selected PTY)

Newest first, updated as it happens. `ActivityStore` (`apps/desktop/src/activity-store.js`, no DOM,
no network) records only what this view itself observes about the selected session: output bursts
(bytes, reads, duration; a pause over 1.5 s ends a burst), grid changes applied, input control
gained or ended, input stopped after an uncertain delivery, missed history, exit. Sizes, counts
and times only: never terminal text, never input. Selecting another session clears the list;
retention is 400 rows.
Storage: the list lives in memory only. It is never written to disk, a log, telemetry or
`localStorage` (only the pane width and open/closed are remembered), so it cannot grow on the
device however long a session streams; a streaming burst is one row that is updated, not a row per read. The header line answers "is it doing anything now": streaming for N s, or
quiet for N s. Coverage is this view only: it starts at attach, pauses while the tab is hidden
(the view does not poll then), and says nothing about what the process is doing, only that
bytes moved. "Needs attention only" keeps stops, gaps, exits, lost control, errors, compactions.

## Agent snapshot rows (not bound, not live)

Optional, under "Add an agent snapshot": import an explicit local `dot.trajectory.v1` file. Its
rows merge into the same newest-first list by time and carry a `snapshot` tag. Consecutive
returned tools of the same category within two minutes group into runs (up to 50); errors,
missing results, inputs and compactions remain boundaries.

Generate a private snapshot with:

```
python3 scripts/session-trajectory.py /private/session.jsonl /private/activity.json
```

The adapter streams Claude JSONL, deduplicates UUIDs/tool IDs, pairs tool results and
exports only category, timestamp, outcome flag, batch step count and compaction
sizes/duration. It never exports prompts, reasoning, command arguments, results,
paths, credentials or agent identity. Output must not exist and is created mode 0600.
The UI imports locally into memory, with a 12 MB / 50,000-event cap; it performs no
upload or persistence. Refresh loses the import. Do not commit real activity exports.

Scope: this is a snapshot of the available file, independently imported and not
cryptographically bound to the selected PTY. Earlier history, branches and child-agent
logs may be absent. Returned tools are not verified task success; unresolved results
are not labelled running. Input markers can include harness-injected messages.
Compaction duration is provider-reported, not a measurement of human waiting time.
The UI uses fixed category labels and textContent, not provider HTML.

Module state is exposed locally as `#trajectory[data-state=empty|loaded]`,
`.trajectory-live[data-state=unbound|waiting|streaming|quiet]` and `#workspace[data-view]`.
Static copy: trajectory.title, trajectory.import, trajectory.filter, trajectory.earlier.
Dynamic data is primary operational signal with no content indexing; it does not enter
the general UI telemetry or clipboard. UI styling uses existing theme tokens.

Next: explicitly owner-bound agent/run adapters with incremental event offsets,
correlated tool groups, turn boundaries and verified effect receipts. Do not infer
semantic completion from ANSI text or strip arbitrary terminal lines: cursor-addressed
TUIs rely on those cells. Folding tool output in the primary view requires a structured
agent projection with the raw terminal retained as an escape hatch. A live AGENT feed (tool
events as they happen), cryptographic PTY binding, semantic milestones and on-demand raw content
are not implemented: the live rows above are transport observations, not agent events.
