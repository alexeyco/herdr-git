# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - 2026-10-13

### Added

- `gitui.open` action that opens gitui in a new herdr tab rooted at the repository, straight into the TUI (the pane process execs gitui directly, with no shell layer and no echoed command).
- Read-only before/after git snapshots taken around the gitui session (branch, HEAD, upstream, ahead count, staged/unstaged/untracked counts). The probes never write the git index or tree.
- English session summary delivered to the agent in the focused pane: created commits (up to 10, with short hash and subject), amend/rebase detection, reset detection, branch switch (including detached HEAD), push via ahead-count decrease, and staging deltas (increase in staged, unstaged, or untracked file counts). Nothing is sent when nothing changed.
- Session state kept as `<pid>-<millis>.txt` under `HERDR_PLUGIN_STATE_DIR/sessions/`, written only when a summary exists, removed on successful delivery, and pruned after 7 days. Delivered prompts capped at 64 KiB on a UTF-8 boundary.
- gitui resolved from the first executable `gitui` on `PATH`, canonicalized to an absolute path, with an install hint when not found. No minimum version is enforced.
- Release binaries for Linux (`x86_64`, `aarch64`) and macOS (`x86_64`, `aarch64`), built by a release pipeline that runs the full CI suite before publishing. The macOS binaries are unsigned.
- Documentation: README, the behavioral spec in `docs/how-it-works.md`, a development guide, a contributing guide, and a security policy.
