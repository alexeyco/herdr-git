## herdr-git

![herdr-git](assets/banner.svg)

herdr plugin that opens [gitui](https://github.com/gitui-org/gitui) or [lazygit](https://github.com/jesseduffield/lazygit) in a herdr pane and reports what the session changed to the agent.

[herdr](https://herdr.dev) is a terminal workspace manager and agent multiplexer. This plugin bridges it with gitui and lazygit: one keypress opens the available Git TUI rooted at the repository, and when you quit it after committing, pushing, or switching branches, a short English summary of what changed is sent to the focused agent as a prompt. It targets the herdr plugin API v1 and runs on Linux and macOS (Windows untested). The pane layout — tab, split, overlay, zoom, or a centered popup — is configured in `config.toml`.

## Demo

One keypress opens gitui or lazygit rooted at the repository; when you quit after a commit, a branch switch, or a push, the changes arrive in the focused agent as a single prompt.

<!-- TODO: record a short demo GIF or asciinema cast (keypress → Git TUI → summary delivered) and embed here; keep it under ~15s -->

## Other herdr plugins

- [herdr-review](https://github.com/alexeyco/herdr-review) — review diffs with the human review TUI of your choice and deliver your annotations to the agent in the focused pane.
- [herdr-open-in](https://github.com/alexeyco/herdr-open-in) — open the workspace or worktree in your editor.

## Install

```
herdr plugin install alexeyco/herdr-git
```

The installer fetches the repository, runs `cargo build --release`, and registers the plugin.

## Quickstart

Bind a key in your herdr config:

```toml
[[keys.command]]
key = "prefix+shift+g"
type = "plugin_action"
command = "alexeyco.git.open"
description = "Open Git TUI"
```

Or invoke the action from the CLI:

```
herdr plugin action invoke alexeyco.git.open
```

## Why

- **Instant TUI.** Plugin pane commands are argv-exec, not send-text, so there is no shell layer and no echoed command line before the chosen TUI appears.
- **Read-only observation.** The plugin takes before/after git snapshots with read-only probes; it never writes to the git index or tree.
- **Agent-agnostic.** The summary goes to whichever agent herdr reports in the focused pane; no agent kind is special-cased.
- **Configurable layout.** Tab, split beside the caller pane, overlay, zoom, or a centered popup — through `config.toml`.
- **Kept summaries are pruned.** Session summaries that cannot be delivered stay on disk and are pruned automatically after 7 days.
- **Linux and macOS, ARM included.** Release binaries cover `x86_64` and `aarch64` on both platforms.

## Requirements

- herdr >= 0.9.0
- [gitui](https://github.com/gitui-org/gitui), [lazygit](https://github.com/jesseduffield/lazygit), or both on `PATH`; install either program yourself (no minimum version is enforced)
- Rust toolchain 1.96.1 (the plugin builds itself on install)

Release binaries are unsigned. macOS may quarantine them on first run; `xattr -d com.apple.quarantine <path>` clears the quarantine flag until the binaries are signed and notarized.

## Usage

Any chord works; see the [herdr keyboard docs](https://herdr.dev/docs/keyboard/) for picking one that your terminal doesn't swallow.

The plugin discovers executable `gitui` and `lazygit` programs on `PATH`; install the programs yourself. If neither is found, a setup popup links to both projects. If exactly one is found, the plugin opens it directly without a picker. If both are found, a picker appears on every invocation and preselects your last explicit choice. Use the arrow keys or `k`/`j` to move, Enter to choose, or Esc/`q` to cancel. Your choice opens in the configured herdr pane placement.

Bind the action as `alexeyco.git.open`. Users must update their own keybindings and scripts; the plugin does not migrate the old `alexeyco.gitui.open` identifier automatically.

## Comparison

How herdr-git compares with other Git TUI plugins for herdr and one adjacent Git workflow plugin, using the four selection criteria below as documented in their public sources.

✅ documented in the linked sources · ⚠️ partial or implemented differently · ❌ not found in the public sources checked

|                                                                                        | [herdr-git](https://github.com/alexeyco/herdr-git) | [Crokily](https://github.com/Crokily/herdr-lazygit) | [Jacques](https://github.com/JacquesvanWyk/herdr-lazygit) | [beomjungil](https://github.com/beomjungil/herdr-lazygit-overlay) | [herdr-sidebar](https://github.com/alexarthurs/herdr-sidebar) |
| -------------------------------------------------------------------------------------- | -------------------------------------------------- | --------------------------------------------------- | --------------------------------------------------------- | ----------------------------------------------------------------- | ------------------------------------------------------------- |
| Flexible launch placement                                                              | ✅[^1]                                             | ✅[^2]                                              | ✅[^3]                                                    | ⚠️[^4]                                                            | ⚠️[^5]                                                        |
| Supports `gitui`                                                                       | ✅                                                 | ❌                                                  | ❌                                                        | ❌                                                                | ❌[^6]                                                        |
| Supports `lazygit`                                                                     | ✅                                                 | ✅                                                  | ✅                                                        | ✅                                                                | ❌[^6]                                                        |
| Feedback to the agent (a Git-change summary is sent to the caller after the TUI exits) | ✅[^7]                                             | ❌                                                  | ❌                                                        | ❌                                                                | ❌                                                            |

[^1]: herdr-git reads pane placement from `config.toml` — [configuration](https://github.com/alexeyco/herdr-git#configuration).

[^2]: Crokily documents configurable sidebar or expanded split/tab layouts — [source](https://github.com/Crokily/herdr-lazygit).

[^3]: Jacques provides separate actions for split and tab placement — [source](https://github.com/JacquesvanWyk/herdr-lazygit).

[^4]: beomjungil opens a fixed session-modal popup over the active pane — [source](https://github.com/beomjungil/herdr-lazygit-overlay).

[^5]: herdr-sidebar provides a dockable Git workflow pane; its README does not describe choosing a launch placement for a Git TUI — [source](https://github.com/alexarthurs/herdr-sidebar).

[^6]: herdr-sidebar documents built-in Source Control, not launching an external Git TUI — [source](https://github.com/alexarthurs/herdr-sidebar).

[^7]: After the selected TUI exits, herdr-git sends an English summary of Git changes to the caller agent.

Feature cells checked 2026-10-21 against the linked public READMEs.

<details>
<summary>Detailed comparison</summary>

|                    | [herdr-git](https://github.com/alexeyco/herdr-git)                  | [Crokily](https://github.com/Crokily/herdr-lazygit)                    | [Jacques](https://github.com/JacquesvanWyk/herdr-lazygit)                                   | [beomjungil](https://github.com/beomjungil/herdr-lazygit-overlay)  | [herdr-sidebar](https://github.com/alexarthurs/herdr-sidebar)                |
| ------------------ | ------------------------------------------------------------------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ | ---------------------------------------------------------------------------- |
| Flexible placement | Configured Herdr pane placement                                     | Sidebar, expanded layout, split, or tab                                | Split pane or tab                                                                           | Fixed session-modal popup over the active pane                     | Dockable Explorer and Source Control pane                                    |
| Supports `gitui`   | Yes                                                                 | No documented support                                                  | No documented support                                                                       | No documented support                                              | No documented support                                                        |
| Supports `lazygit` | Yes                                                                 | Yes                                                                    | Yes                                                                                         | Yes                                                                | No documented support                                                        |
| Agent feedback     | Sends a Git-change summary to the caller agent after the TUI exits  | Does not document sending a Git-change summary to the caller agent     | Does not document sending a Git-change summary to the caller agent                          | Does not document sending a Git-change summary to the caller agent | Does not document sending a Git-change summary to the caller agent           |
| Latest release     | [v0.2.0](https://github.com/alexeyco/herdr-git/releases/tag/v0.2.0) | [v0.4.0](https://github.com/Crokily/herdr-lazygit/releases/tag/v0.4.0) | [v0.1.0 tag](https://github.com/JacquesvanWyk/herdr-lazygit/tree/v0.1.0); no GitHub release | No GitHub release published                                        | [v0.15.1](https://github.com/alexarthurs/herdr-sidebar/releases/tag/v0.15.1) |

</details>

Among these plugins, herdr-git is the only one that documents support for both `gitui` and `lazygit` and sends a Git-change summary to the caller agent after the TUI exits.

When you quit the selected TUI, the plugin compares read-only before/after git snapshots and builds an English summary of what changed:

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

Nothing changed → nothing is sent. Closing the pane early cancels the session (no summary is delivered). Failure paths hold the pane until you press Enter so the error message stays readable.

## Configuration

An optional flat-key `config.toml` in the plugin's config directory controls where the selected Git TUI pane opens:

```toml
placement = "split"
direction = "down"
```

or, for a centered popup sized in outer terminal cells or percentages of the terminal area:

```toml
placement = "popup"
width = 80
height = "80%"
```

| Key         | Values                                       | Default   | Notes                                                 |
| ----------- | -------------------------------------------- | --------- | ----------------------------------------------------- |
| `placement` | `tab`, `split`, `overlay`, `zoomed`, `popup` | `tab`     | How the Git TUI pane is placed in the layout          |
| `direction` | `right`, `down`, `auto`                      | `right`   | Split direction; used only when `placement = "split"` |
| `width`     | Positive integer or `"N%"` string            | _(unset)_ | Popup width; used only when `placement = "popup"`     |
| `height`    | Positive integer or `"N%"` string            | _(unset)_ | Popup height; used only when `placement = "popup"`    |

`split` and `zoomed` open beside or over the caller pane — the plugin passes `--target-pane` with the id of the pane that triggered the action. If no caller pane resolves, a configured `split`/`zoomed` degrades to `tab` with a warning instead of failing the open. `popup` opens the selected Git TUI as a centered session-modal window: it leaves the tiled layout untouched, receives all terminal input including Escape, and closes when the TUI quits. `width`/`height` apply only to the popup placement; values below herdr's popup minimum are clamped by the daemon.

Settings are read once per action launch, so an edit takes effect on the next launch. The plugin never fails over its settings: a missing `config.toml` silently means defaults, while a corrupt file, an unknown key, or an invalid value degrades to the defaults for that key with a warning you can see via `herdr plugin log list`. Focus is not configurable: the pane always opens focused.

The file lives in the plugin's config directory: herdr injects its location as `HERDR_PLUGIN_CONFIG_DIR`, and `herdr plugin config-dir alexeyco.git` prints the same directory for you to edit in a shell.

## How it works

The full detection rules and lifecycle are in [docs/how-it-works.md](docs/how-it-works.md).

## Building

```
git clone https://github.com/alexeyco/herdr-git
cd herdr-git
cargo build --release
herdr plugin link .
```

To run your own build inside herdr panes, keep the plugin linked; a linked plugin does not rebuild itself.

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE)
