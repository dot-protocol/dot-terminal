# Panda and mobile control references

Reviewed 2026-09-19. The installed app identifies as Panda, package
`com.blurr.voice`, version 1.1.47 (111), installed through Google Play.
Its initial screen describes voice accessibility. Google sign-in repeatedly returned
to the account chooser; the user asked to stop configuration and research online.
No signed-in task execution, accessibility grant, microphone use or autonomous action
was established. The cause of the sign-in failure was not diagnosed.

## What it does

The [Panda repository](https://github.com/Ayush0Chaudhary/blurr) describes a Kotlin
agent using Android Accessibility to read UI hierarchy and perform gestures, with
LLMs planning actions. That is a device operator, not a terminal emulator or a PTY.
Its public README and current store binary should not be assumed to be identical.

The [current company site and policy summary](https://www.heypanda.org/) identify
Steepwind Labs and describe cloud account data and AI providers. Relevant screen
content, commands and voice can be processed as features require. Calling an agent
"on-device" does not establish that all inference or data processing stays local.
The repository README also marks local memory temporarily disabled; do not infer the
installed release's memory behavior from older demonstrations.

The [current repository license](https://github.com/Ayush0Chaudhary/blurr/blob/main/LICENSE)
restricts use to personal, educational and noncommercial purposes and requires separate
commercial licensing. This is source-available with restrictions, not an unrestricted
open-source foundation for DOT. No Panda code was imported.

## Alternatives to evaluate

| Reference | Role in DOT research | Root license inspected |
|---|---|---|
| [Droidrun / Mobilerun](https://github.com/droidrun/mobilerun) | Model-independent mobile operator framework; local machine runs agent, phone exposes control; useful adapter reference | [MIT](https://github.com/droidrun/mobilerun/blob/main/LICENSE) |
| [Mobile MCP](https://github.com/mobile-next/mobile-mcp) | Structured mobile inspection/action tools for agents, Android and iOS | [Apache-2.0](https://github.com/mobile-next/mobile-mcp/blob/main/LICENSE) |
| [AndroidWorld](https://github.com/google-research/android_world) | Repeatable mobile task evaluation rather than a consumer assistant | Evaluate separately before reuse |
| [TalkBack](https://github.com/google/talkback) | Native accessibility semantics and navigation; not an LLM operator | Evaluate separately before reuse |

These are research candidates, not installed integrations. Verify exact revisions,
component licenses, permission models and platform constraints before reuse. Root
licenses alone do not settle every dependency or hosted-service term.

## Design implication

Keep device operation separate from terminal control. A terminal capability grants
input to a selected PTY, not permission to inspect banking apps or send messages.
A mobile operator should have an explicit task, allowed apps/actions, expiry and a
visible stop control. Screen text is untrusted input, not authorization. Prefer
structured application APIs when available, with accessibility as a separately
consented fallback. Record action outcomes and uncertainty without indiscriminately
recording screens, passwords or personal app content. Do not infer successful delivery
or purchase from a tap or model narration.
