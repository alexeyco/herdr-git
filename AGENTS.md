# AGENTS.md

See README.md for what this plugin does; docs/development.md for the dev workflow. Only agent-relevant gotchas live here.

## Gotchas

- The binary is dual-mode: `HERDR_PLUGIN_ACTION_ID` set = action mode (opens the pane, exits), `HERDR_PLUGIN_ENTRYPOINT_ID` set = pane mode (runs gitui on the pane tty). Same binary, dispatched in `main`. Pane mode wins when both are set.
- The version appears in `Cargo.toml` and `herdr-plugin.toml`; keep both in sync, including the root package entry in `Cargo.lock`. Before creating any release tag, run `make release-check TAG=v<version>` from the target commit. If the check fails for an explicitly approved release version, prepare a PR that updates all version entries and rerun the preflight after it merges; never create a tag that fails the check. The release workflow runs the same check and then runs the full CI suite via `workflow_call` before artifacts.
- Build the release binary before `herdr plugin link` testing; a linked plugin does not rebuild itself.
- Call back into herdr through `HERDR_BIN_PATH`, never a bare `herdr`.
- `min_herdr_version` is checked against the running herdr binary; an older local herdr refuses to link.
- gitui is resolved from the first executable `gitui` on `$PATH` only — there is no override environment variable. The path is canonicalized to an absolute path; a canonicalization failure is a loud error. When no `gitui` is found, the plugin fails with an install hint pointing at <https://github.com/gitui-org/gitui>. No minimum version is checked.
- The manifest declares both `[[actions]]` (keybinding target, id `open`) and `[[panes]]` (id `open`, `placement = "tab"`, `command = ["./target/release/herdr-gitui"]`). Plugin pane commands are argv-exec, not send-text, so there is no shell layer and no echoed command. The pane-open `--placement` comes from `config.toml` (default `tab`) and overrides the manifest value at open time; the manifest intentionally stays `placement = "tab"` (drift acknowledged in `src/herdr.rs`).
- Runtime commands may start in the plugin directory, so the action exports `HERDR_GITUI_DIR=<repo>` to the pane; pane mode prefers it over its own cwd and runs gitui with `.current_dir(dir)`. `HERDR_GITUI_*` is not a protected `HERDR_*` key.
- Plugin panes auto-close when the pane process exits (`RuntimeExitAction::ClosePane`, no respawn). Success paths still call `plugin pane close` (best-effort); error and no-agent paths print their message then block on stdin via `hold_pane` before returning, otherwise the message vanishes.
- Session state is written under `HERDR_PLUGIN_STATE_DIR/sessions/` as `<pid>-<unix millis>.txt` (pid keeps same-millisecond sessions collision-safe); pane start prunes kept `.txt` files older than 7 days, best-effort with logged warnings. It is written only when a summary exists (rc 0 with a non-empty summary, or rc ≠ 0 with a summary); removed on successful delivery; kept when delivery is skipped (no agent caller, or delivery failed) or when gitui fails. On a clean no-change exit and on the killed/signal path, no file is written in the first place.
- The delivered agent prompt is capped at 64 KiB (UTF-8 boundary) with a `(output truncated; full summary kept at <path>)` note; unreadable summaries are a hard error when gitui exited non-zero (file kept).
- Git snapshots are strictly read-only — the plugin never mutates the index or tree. It only compares before/after state.
- The summary rules are heuristic: amend vs. rebase is indistinguishable (both produce "amended or rebased branch"); pushes are detected only via ahead-count decrease with an upstream (a push from another clone is invisible); staging deltas are counts, not file names.
- herdr CLI calls (`tab`, `pane`, `agent`) have no timeout in plugin API v1, so a hung herdr daemon blocks the action.
- Never push directly to `master` and never merge a PR yourself: every change goes through a branch and a PR the maintainer merges.
- The release version is the maintainer's call: never pick, bump, or assume a version, and never create or push a tag on your own — ask which version is being released first.
