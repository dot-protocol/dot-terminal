# Contributing

Start with a small issue describing a user-visible problem, a reproduction, and
the invariant the change should preserve. Discuss protocol changes in an issue
before implementing them. The current protocol is experimental, not a stable API.

Use a feature branch and pull request. Run formatting, Clippy with warnings denied,
and the full workspace tests. Tests should exercise behavior and failure cases.
Terminal fixtures must be generated from synthetic content, never captured from
someone's private terminal or agent history.

Keep protocol/core free of OS, renderer and model-runtime dependencies. Add platform
code behind explicit adapters. Document uncertain operation outcomes and migration
behavior. Don't add silent fallbacks that weaken authority or hide errors.

Sign commits with `git commit -s` to certify the Developer Certificate of Origin
at https://developercertificate.org/. This is an authorship/rights certification,
not a requirement to use a cryptographic signing key. Contributors retain copyright.

Preserve upstream attribution and inspect licenses before importing code. Large
features need a design note, tests and a rollback strategy. AI-assisted code is
welcome; the contributor is responsible for understanding and validating it.

Maintainers review through pull requests. Until a broader maintainer group is
established, repository administrators own release decisions. Proposed governance
changes are public issues. Funding does not grant automatic access to user data.
