# herdr-gitui

[![CI](https://github.com/alexeyco/herdr-gitui/actions/workflows/ci.yml/badge.svg)](https://github.com/alexeyco/herdr-gitui/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 1.96.1+](https://img.shields.io/badge/rust-1.96.1%2B-blue.svg)](https://www.rust-lang.org)

![herdr-gitui](assets/banner.svg)

herdr plugin that opens [gitui](https://github.com/gitui-org/gitui) in a new tab and reports what the session changed to the agent in the focused pane.

[herdr](https://herdr.dev) is a terminal workspace manager and agent multiplexer; [gitui](https://github.com/gitui-org/gitui) is a blazing-fast terminal UI for git. This plugin bridges them: one keypress opens a tab straight into the gitui TUI rooted at the repository, and when you quit gitui after making changes, a short English summary of what changed is sent to the focused agent as a prompt. It targets the herdr plugin API v1 and runs on Linux and macOS (Windows untested).

## Demo

One keypress opens a `gitui` tab rooted at the repository; when you quit gitui after committing, switching branches, or pushing, the changes arrive in the focused agent as a single prompt.

<!-- TODO: record a short demo GIF or asciinema cast (keypress → gitui TUI → summary delivered) and embed here; keep it under ~15s -->

## Quickstart

```
herdr plugin install alexeyco/herdr-gitui
```

Bind a key in your herdr config:

```toml
[[keys.command]]
key = "prefix+shift+g"
type = "plugin_action"
command = "gitui.open"
description = "Open gitui"
```

Or invoke the action from the CLI:

```
herdr plugin action invoke gitui.open
```

## Why

- **Instant TUI.** Plugin pane commands are argv-exec, not send-text, so there is no shell layer and no echoed command line before gitui appears.
- **Read-only observation.** The plugin takes before/after git snapshots with read-only probes; it never writes to the git index or tree.
- **Agent-agnostic.** The summary goes to whichever agent herdr reports in the focused pane; no agent kind is special-cased.
- **Kept summaries are pruned.** Session summaries that cannot be delivered stay on disk and are pruned automatically after 7 days.
- **Linux and macOS, ARM included.** Release binaries cover `x86_64` and `aarch64` on both platforms.

## Requirements

- herdr >= 0.9.0
- [gitui](https://github.com/gitui-org/gitui) on `PATH` (no minimum version is enforced)
- Rust toolchain 1.96.1 (the plugin builds itself on install)

Release binaries are unsigned. macOS may quarantine them on first run; `xattr -d com.apple.quarantine <path>` clears the quarantine flag until the binaries are signed and notarized.

## Usage

Any chord works; see the [herdr keyboard docs](https://herdr.dev/docs/keyboard/) for picking one that your terminal doesn't swallow.

The action resolves the repository directory from the plugin context and opens a `gitui` tab whose pane runs gitui directly, so the tab opens straight into the TUI (no shell echo). When you quit gitui, the plugin compares read-only before/after git snapshots and builds an English summary of what changed:

| Situation                           | What is reported                                       |
| ----------------------------------- | ------------------------------------------------------ |
| New commits                         | Each commit (up to 10) with short hash and subject     |
| Amend or rebase                     | HEAD moved from one commit to another (non-linear)     |
| Reset                               | HEAD moved back to an ancestor                         |
| Branch switch (incl. detached HEAD) | New branch name, or detached HEAD short sha            |
| Push (ahead-count decrease)         | Number of commits pushed to the upstream               |
| Staging deltas                      | Increase in staged, unstaged, or untracked file counts |

Example prompt delivered to the agent:

```
gitui session summary:

- switched to branch feature-x
- created commit abc1234: fix: handle edge case
- pushed 2 commit(s) to origin/feature-x
```

Nothing changed → nothing is sent. Closing the tab early cancels the session (no summary is delivered). Failure paths hold the pane until you press Enter so the error message stays readable. The full detection rules and lifecycle are in [docs/how-it-works.md](docs/how-it-works.md).

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/development.md](docs/development.md).

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

[MIT](LICENSE)
