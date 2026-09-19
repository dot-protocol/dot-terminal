#!/bin/sh
# Capture the DOT Terminal window exactly as it is on screen, without a browser or an extension.
# For whoever is working on the UI (a person or an agent) to SEE the real app, not a lab copy.
# Usage: scripts/capture-app.sh [out.png] [max-edge-px]   (needs macOS Screen Recording permission
# for the app that runs this). Prints the path. The image can contain terminal text: keep it private.
set -eu
out="${1:-${TMPDIR:-/tmp}/dot-terminal-window.png}"; edge="${2:-1600}"
id=$(/usr/bin/swift - <<'SWIFT'
import CoreGraphics
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly,.excludeDesktopElements], kCGNullWindowID) as! [[String:Any]]
for w in list where (w["kCGWindowOwnerName"] as? String ?? "").hasPrefix("DOT Terminal") && (w["kCGWindowLayer"] as? Int ?? 1) == 0 { print(w["kCGWindowNumber"]!); break }
SWIFT
)
[ -n "$id" ] || { echo "no DOT Terminal window on screen" >&2; exit 1; }
screencapture -x -o -l "$id" "$out"
sips -Z "$edge" "$out" >/dev/null
echo "$out"
