# Seeing and steering the real app

For whoever maintains the interface, a person or an agent on the same machine. The point is to look
at the app that is actually running (the Mac window, not a lab copy in a browser), cheaply.

## 1. The view's own snapshot (no pixels, no OS permissions)

Every view posts what it shows to its backend, which writes `<state-dir>/view-snapshot.json`
(private, replaced atomically, about 10 KB):

- window: inner size, scale, position, the screen it is on and that screen's origin, visible, focused
- layout boxes: sidebar, header, infobar, terminal, activity, footer
- terminal numbers: grid, line count, viewport top vs scrollback top, at bottom or scrolled, hidden while history loads
- text the person reads: title, status, input state, sync, version, the agent line, who is here
- devices and their session counts
- every operable control: role, accessible name, state (disabled, expanded, selected, checked), box
- accessibility summary: control count, controls WITHOUT an accessible name (a bug list), landmarks
- a timeline of the last start: page started → interface state → devices → session selected →
  typing here → history shown(bytes), each with milliseconds

It never contains terminal text, typed input, clipboard or capabilities.

```
scripts/dot-view.py <state-dir>            # a dozen lines: the whole picture
scripts/dot-view.py <state-dir> controls   # the control list (the accessibility tree that matters)
scripts/dot-view.py <state-dir> json
```

## 2. Steering, safely

`scripts/dot-view.py <state-dir> act <action>` appends to `<state-dir>/view-actions.json` (private).
The view collects it within ~1.5 s. The PAGE allowlists actions: `reload`, `refresh`,
`activity:open|close`, `select:<session-prefix>`, `snapshot`. Interface navigation only: nothing
can type into, create or stop a session. Anyone who can write that file already owns the state
directory.

## 3. Pixels, when fidelity matters

`scripts/capture-app.sh [out.png] [max-edge]` captures the DOT window itself with `screencapture -l`
(needs Screen Recording permission for the calling app). Captures can contain terminal text: keep
them private. To watch a transition, queue `act reload` and capture in a loop; PNG size alone shows
the blank/loading frames.

## Why not a generic computer-use tool

Tools that see pixels and post mouse events work on any app but cost an image per look and need
Screen Recording and Accessibility permissions; browser-extension tools cannot reach the Mac app's
web view at all. We own this app, so it reports its own state. Keep a pixel path for verification.
