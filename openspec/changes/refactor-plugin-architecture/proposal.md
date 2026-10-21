# Refactor plugin architecture

## Why now

`src/session.rs` and `src/herdr.rs` combine several independent workflows and
have become the largest production modules. The sibling `herdr-review` project
now has adapter and feature boundaries, injected ports, and focused tests that
make these workflows easier to change. This change applies that architecture
to `herdr-git`, reports the production LOC change, and reduces duplicated
logic.

## What changes

Split external integrations into adapters and user-facing workflows into
features. Introduce ports for process, filesystem, terminal, and Herdr
boundaries where injection removes hand-written test plumbing. Use `mockall`
for boundary mocks and `tempfile` for isolated filesystem tests. Replace the hand-written terminal picker with
`ratatui` to make terminal setup and cleanup reliable. Retain existing
dependencies and add no other dependency unless its use is justified by a
specific simplification in the design and review.

Do not retain `anyhow`: its contextual formatting did not simplify this
plugin's error paths while the existing exact error strings must remain stable.

The refactor preserves the plugin's observable behavior: Herdr argv and
environment, pane and popup lifecycle, git snapshots and summaries, state
file semantics, user-visible message text, and process exit codes. The success
criteria are less duplicated production logic, simpler boundary-focused
tests, and clear adapter/feature ownership. The user accepts higher production
LOC for these explicit boundaries; report the baseline comparison without
using it as a completion gate.

## Capabilities

- Adds `plugin-architecture` requirements for module boundaries, injected
  integrations, and behavior-preserving workflow extraction.
- Dependabot's weekly Rust toolchain update is out of scope here. Plan it as a
  separate subsequent change named `rust-toolchain-updates`, with capability
  `rust-toolchain-updates`; keep MSRV updates separate as the user requested.
- Refresh the README banner's accessible label and visible text to use the
  current `herdr-git` identity and supported Git TUI workflow.
- Replace raw kebab-case top-level labels in tracked OpenSpec specification
  files with readable headings while preserving capability IDs and paths.

## Assumptions and rejected risks

ASSUMPTION: The current `make check` suite plus focused adapter and feature
tests can verify behavior preservation without adding new end-to-end behavior.

REJECTED-RISK: Ratatui and explicit architecture boundaries may add production
LOC — user accepts this cost for terminal UI reliability and clearer module
ownership.
