# Design

## Architecture

Use `src/adapters/` for operating-system and Herdr integrations, and
`src/features/` for plugin workflows. Keep serialization/configuration types
in the feature that owns them. Define feature-facing traits in
`src/features/session/ports.rs` and `src/features/action/ports.rs`; implement
them in adapters and generate test doubles with `mockall`.

Target module layout:

- `src/adapters/git.rs`: read-only Git snapshot commands.
- `src/adapters/herdr.rs`: Herdr CLI argv, environment, and reply handling.
- `src/adapters/process.rs`: selected TUI process execution and exit status.
- `src/adapters/state.rs`: session files, preference storage, and pruning.
- `src/adapters/terminal.rs`: Ratatui/Crossterm terminal lifecycle.
- `src/features/action/`: context resolution, tool discovery and action
  orchestration.
- `src/features/picker.rs`: picker state and rendering, independent of process
  launch and preference persistence.
- `src/features/session/`: session orchestration, summary and outcome policy,
  delivery, and teardown.
- `src/features/setup.rs`: setup guidance.
- `src/main.rs`: mode and pane-entrypoint dispatch only.
- `src/settings.rs`: existing placement settings parsing and configuration.

Keep error text construction explicit to preserve the existing messages.
`anyhow` was evaluated and removed because contextual formatting did not
simplify these paths. Use `tempfile` in state adapter tests, replacing shared
or manually cleaned temporary paths. Keep
`serde`, `serde_json`, and `toml` for their existing formats. Configure
`ratatui` with `default-features = false` and its `crossterm` feature. Add no
unjustified dependencies beyond `ratatui`, `mockall`, and `tempfile`.

## Behavior and size verification

Treat the baseline OpenSpec capabilities as the behavior contract. Preserve
all exact messages, argv order, environment names and values, tool choice,
snapshot timing, state-file naming and cleanup, and exit codes. Run the
existing E2E suite unchanged; rewrite unit tests around ports and mocks where
that reduces setup or duplication. Record representative pre-refactor setup
for boundary tests, then report which bespoke fakes, manual fixture setup, or
shared temporary paths the refactor removes or simplifies. Do not use test
LOC as a success metric.

Before implementation, add `scripts/count-production-loc.py` as the single
counting script. It SHALL enumerate every Rust source file under `src/`,
exclude blank lines and lines containing only Rust comments, and exclude the
full source span of every complete Rust item annotated `#[cfg(test)]`,
including inline test modules and external files declared only by
`#[cfg(test)] mod tests;`. It SHALL report one total
for production LOC. Run this same script against the pre-change `HEAD` tree
and the final working tree; the script reads the baseline source with `git
show` and the final source from disk. Record both invocations and totals as a
refactoring diagnostic, not a success threshold: the user accepted additional
production LOC for explicit adapter and feature boundaries. Review extracted
duplicate logic in the diff and keep the shared implementation in one adapter
or feature helper. `ratatui` is included for terminal reliability even if the
picker itself grows.

## README banner identity

Refresh the existing `assets/banner.svg` text to identify `herdr-git` and the
supported Git TUI workflow. Update its accessible label and visible title and
description while preserving the existing banner artwork, dimensions, and
visual style.

## Specification heading readability

Use readable top-level headings in every tracked specification file under
`openspec/**/specs/`. Convert kebab-case capability labels such as
`# agent-delivery` to `# Agent delivery`; preserve specification paths,
capability IDs, and requirement content.

## Trade-offs

- The extra adapter and feature modules add production LOC, but allow tests
  to isolate integrations and keep workflow decisions readable.
- Ratatui adds a rendering abstraction and dependency; its value is guaranteed
  terminal restoration, not picker LOC reduction.
- `anyhow` was removed because it did not simplify error propagation while
  preserving exact messages.
- `mockall` and `tempfile` are test-only dependencies and should replace
  bespoke mocks and fragile filesystem setup rather than add parallel test
  machinery.
- Weekly Dependabot updates for `rust-toolchain` belong to the separate
  subsequent `rust-toolchain-updates` change; MSRV remains separately managed.
