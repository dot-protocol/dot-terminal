# Session trajectory

The desktop trajectory button opens a narrow right-hand rail beside the PTY. Import
an explicit local `dot.trajectory.v1` snapshot. Consecutive returned tools of the same
category within two minutes group into expandable runs (up to 50); errors, missing
results, inputs and compactions remain boundaries. Show earlier paginates the history.
The filter preserves errors and compactions. This is an observer feature and never
acquires terminal control or sends input. A controller's normal resize observer may
resize its PTY when opening the rail; a follower retains the host grid and pans.

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

Module state is exposed locally as `#trajectory[data-state=empty|loaded|error]`.
Static copy: trajectory.title, trajectory.import, trajectory.filter, trajectory.earlier.
Dynamic data is primary operational signal with no content indexing; it does not enter
the general UI telemetry or clipboard. UI styling uses existing theme tokens.

Next: explicitly owner-bound agent/run adapters with incremental event offsets,
correlated tool groups, turn boundaries and verified effect receipts. Do not infer
semantic completion from ANSI text or strip arbitrary terminal lines: cursor-addressed
TUIs rely on those cells. Folding tool output in the primary view requires a structured
agent projection with the raw terminal retained as an escape hatch. Live ingestion,
PTY/session binding, semantic milestones and on-demand raw content are not implemented.
