# Mobile terminal UX study

Reviewed 2026-09-19. This is a ranking of usefulness to DOT's design, not market
share or a claim that one product is best for every user. Only Termius received
hands-on testing in this study. Other assessments are based on primary sources.

## Selection

| Rank within platform | Product | Why study it | Evidence / limits |
|---|---|---|---|
| Android 1 | Termius | Remote connection workflow, terminal-first layout, special-key strip, connection tabs and visual session cards | Play Store installation and physical-device tests below; commercial product, no source reuse proposed |
| Android 2 | Termux | Local development environment and terminal input; most relevant complement to remote sessions | [Official project](https://github.com/termux/termux-app); not installed/tested here |
| Android 3 | ConnectBot | Small, open SSH client and approachable connection model | [Official project](https://github.com/connectbot/connectbot); not installed/tested here |
| iPad 1 | Blink Shell | Touch gestures, keyboard customization and SSH/Mosh workflows | [Official project](https://github.com/blinksh/blink); no iPad test |
| iPad 2 | Prompt 3 | Native Apple connection and keyboard experience | [Panic documentation](https://help.panic.com/prompt/prompt3/); no iPad test |
| iPad 3 | a-Shell | Local command execution within Apple's application constraints | [Official project](https://github.com/holzschu/a-shell); no iPad test |

Blink documents two-finger new-shell gestures, swiping between shells, pinch text
size and configurable keys. These are useful benchmarks, but DOT should expose
visible alternatives for gestures. Prompt provides another commercial Apple UX
reference. a-Shell serves a different local-runtime need from an SSH client.
[iSH](https://ish.app/) is also worth tracking for iOS Linux compatibility, rather
than treating all local shells as equivalent to native host access.

JuiceSSH is a historical reference; its present installation availability was not
verified on the test device, so it is not ranked as a current recommendation.

Termux's Google Play distribution is active again and is maintained separately
from the main distribution. Consult the [Play maintainers](https://github.com/termux-play-store)
and [main installation guidance](https://github.com/termux/termux-app#installation)
before selecting packages or mixing installation channels. The blanket statement
that all Play Store Termux releases are obsolete is not a sound current assumption.

## Physical Android test

Installed Termius 7.10.0, version code 937, package `com.server.auditor.ssh.client`,
from Google Play (`com.android.vending`) on a Motorola g67 power 5G.
[Official listing](https://play.google.com/store/apps/details?id=com.server.auditor.ssh.client).
Selected **Continue without Sync and AI**. No Termius account, paid subscription,
cloud key upload or existing private-key import was used. This observes the chosen
workflow; it is not an audit of all application network traffic or storage security.

Temporary loopback SSH fixtures were reachable through wireless Android debugging
port forwarding. One fixture emitted synthetic output. The other created fresh Mac
shells or fresh SSH sessions to an authorized Linux VPS using credentials retained
on the Mac. The phone authenticated only to the temporary fixture. Host fingerprints
were compared with the fixture's generated keys before acceptance.

This tested real phone input and rendering against real Mac/VPS processes. It did
**not** test direct phone-to-VPS authentication, mobile-data traversal, Mosh roaming,
DOT mesh transport, or persistent access independent of the Mac/debugging connection.
The temporary shells, servers and two test forwarding rules were removed afterwards.
Termius remains installed; its saved fixture profiles are no longer usable endpoints.
No production shell, DOT keeper, gateway, desktop or browser service was replaced.

| Case | Observed result |
|---|---|
| Onboarding without vendor account | Reached local vault and connection UI |
| Saved host / quick connection | Both worked; quick connection accepts SSH-style address syntax |
| Host identity | Fingerprint approval precedes initial connection |
| Mac input/output | `uname -s` returned Darwin; shell accepted typed commands |
| VPS input/output | Separate VPS tab returned Linux; also exercised an SSH hop from the disposable Mac shell |
| Terminal geometry | Remote `stty size` reported 42 rows by 88 columns with phone keyboard visible |
| Interrupt | On-screen Ctrl followed by C interrupted `sleep 30` on the Mac shell |
| ANSI rendering | Distinct basic foreground colors observed in fixture and VPS output |
| Longer output | Numbered 60-line output displayed; keyboard-hidden view exposed more content |
| Sessions | Synthetic, Mac and VPS tabs existed together; switching restored their visible contents |
| App navigation | Returning from Home exposed connections through a separate card-based overview |
| Keyboard resize propagation | Not established end-to-end; expanded viewport observed, but no reliable second remote-size measurement |
| Network loss / same-process recovery | Not tested; app navigation is not a network recovery test |
| Unicode, IME composition, alternate screen, paste safety | Not yet tested |

Screenshots and accessibility dumps remain private. Accessibility terminal text
sometimes lagged actual rendered output; visual inspection was necessary. Some
animated sheets did not produce an idle accessibility snapshot. Early automated
form entry lost initial characters during focus transitions; this is not evidence
that normal human typing is broken. Re-read state after focus/keyboard changes.

## What DOT should learn

The terminal should occupy the main screen. Devices, pairing, clipboard and diagnostics
belong in drawers or secondary screens, while identity, session name and control state
remain visible. Termius's keyboard-adjacent special keys avoid a separate command form;
its connection cards offer recognition and fast return to work. Small tabs truncate
names quickly: DOT needs good session labels and an accessible full session list.

The next bounded implementation should provide:

1. A real terminal input surface: IME composition, modifier state, Esc/Tab/arrows,
   Ctrl-C, backspace and hardware keyboard handling. Keep a multiline composer as an
   optional tool, with explicit paste/execute behavior.
2. Session list and tabs showing device, session identity, connection freshness and
   view/control mode. Viewing must not silently take over another controller.
3. Measured viewport and keyboard insets, exact font sizing and explicit size policy.
   Multiple viewers cannot all dictate one PTY's dimensions simultaneously; the current
   controller owns resize, while viewers pan or fit without mutating the remote size.
4. Reconnection to stable server-side session identity, with replay position and
   visible gap/error states. Separate **disconnect view**, **release control** and
   **terminate process**. A saved host, SSH channel and durable PTY are different objects.
5. A measured renderer decision: compare an Android WebView using the existing xterm
   path with a maintained native terminal view. Require color, Unicode, selection,
   scrollback, alternate-screen and accessibility tests before replacing the renderer.

Acceptance should include Mac and VPS sessions open simultaneously, 20 tab switches
without duplicate shells, keyboard show/hide with confirmed server resize, long input
and IME composition, predictable interrupt/paste behavior, and network interruption
followed by reattachment to the same process. Record latency distributions rather than
claiming low latency from one successful command. Test genuine Wi-Fi/mesh transport
separately from debugging tunnels.

## Reuse boundary

No third-party code was imported. Termius and Prompt are UX references, not proposed
forks. ConnectBot publishes an [Apache-2.0 license](https://github.com/connectbot/connectbot/blob/main/LICENSE).
Termux's [license](https://github.com/termux/termux-app/blob/master/LICENSE.md) is GPLv3-only
with documented exceptions for terminal libraries and shared code; inspect the exact
files and dependency licenses before importing anything. Blink's repository identifies
GPL-3.0. UX observations do not grant rights to copy code, branding or assets. For any
reuse, pin the actual revision and preserve applicable notices and obligations.
