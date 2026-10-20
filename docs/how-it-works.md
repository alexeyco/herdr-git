# How it works

The `alexeyco.gitui.open` action is a small Rust program (sources under `src/`) that runs in two modes.

- **Action mode** runs when herdr invokes the `alexeyco.gitui.open` action. herdr sets `HERDR_PLUGIN_ACTION_ID` and `HERDR_PLUGIN_CONTEXT_JSON`; the binary resolves a repository directory from that JSON, reads the pane settings from `config.toml`, and opens a `gitui` plugin pane (`herdr plugin pane open --placement <placement>`, default `tab`), then exits immediately.
- **Pane mode** runs inside the opened pane. herdr sets `HERDR_PLUGIN_ENTRYPOINT_ID`, spawns `command = ["./target/release/herdr-gitui"]` from the manifest as a process attached to the pane pty, with the pane's tty and `--cwd` set to the repository. The binary takes read-only before/after git snapshots around a gitui session and delivers an English summary of what changed to the agent pane that requested the session.

Plugin pane commands are argv-exec, not send-text: the manifest `command` array is spawned directly, so there is no shell layer, no echoed command line, and the gitui TUI appears the instant the pane opens. Plugin panes also auto-close when the pane process exits, so the error and no-agent paths block on Enter before exiting.

## Dispatch

`main` picks the mode from the environment, in this order:

| Condition                        | Mode                                                   |
| -------------------------------- | ------------------------------------------------------ |
| `HERDR_PLUGIN_ENTRYPOINT_ID` set | Pane mode                                              |
| `HERDR_PLUGIN_ACTION_ID` set     | Action mode                                            |
| neither                          | Error: `invoked outside herdr plugin context` (exit 1) |

For both variables, a blank or whitespace-only value counts as unset.

## Context

Environment variables set by herdr (see the [plugin docs](https://herdr.dev/docs/plugins/)):

| Variable                                              | Used for                                                                                             |
| ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `HERDR_PLUGIN_ACTION_ID`                              | Dispatch: action mode                                                                                |
| `HERDR_PLUGIN_ENTRYPOINT_ID`                          | Dispatch: pane mode                                                                                  |
| `HERDR_PLUGIN_CONTEXT_JSON`                           | Action mode: source of the repository directory and the caller pane/agent; must be present           |
| `HERDR_BIN_PATH`                                      | Every callback (`plugin pane open`, `plugin pane close`, `tab close`, `tab focus`, `agent prompt`)   |
| `HERDR_PLUGIN_STATE_DIR`                              | Base directory for session state (`sessions/`)                                                       |
| `HERDR_PLUGIN_CONFIG_DIR`                             | Action mode: directory holding `config.toml`; preferred over the `plugin config-dir` query           |
| `HERDR_WORKSPACE_ID`, `HERDR_PANE_ID`                 | Action mode: fallbacks for `workspace_id` / `focused_pane_id` when the context JSON omits them       |
| `HERDR_GITUI_DIR`                                     | Pane mode: repository directory, injected by the action via `--env` (preferred over the process cwd) |
| `HERDR_GITUI_PLACEMENT`                               | Pane mode: set to `popup` by the action for popup sessions only; gates the teardown (no close calls) |
| `HERDR_GITUI_CALLER_PANE`, `HERDR_GITUI_CALLER_AGENT` | Pane mode: delivery target, injected by the action via `--env`                                       |
| `HERDR_TAB_ID`, `HERDR_PANE_ID`                       | Pane mode: pane teardown (close pane, then tab)                                                      |
| `HERDR_PLUGIN_ID`, `HERDR_PLUGIN_ROOT`                | Set by herdr but unused by this plugin                                                               |

Fields read from the context JSON in action mode (all optional; absent when null or blank):

| Field                    | Meaning                                                      |
| ------------------------ | ------------------------------------------------------------ |
| `worktree.checkout_path` | Checkout directory of the worktree the workspace is based on |
| `workspace_cwd`          | Workspace working directory (may be a subdirectory)          |
| `focused_pane_cwd`       | Working directory of the focused pane                        |
| `workspace_id`           | Workspace id, used to open the pane in the right workspace   |
| `focused_pane_id`        | Pane id that receives the summary on delivery                |
| `focused_pane_agent`     | Agent kind in the focused pane, when any; delivery needs it  |

The caller pane and agent are captured at action time and passed to the pane as `HERDR_GITUI_CALLER_PANE` / `HERDR_GITUI_CALLER_AGENT`; pane mode never reads the context JSON itself.

## Directory resolution

```
worktree.checkout_path > workspace_cwd > focused_pane_cwd > error
```

`worktree.checkout_path` wins because `workspace_cwd` may be a subdirectory the user has `cd`-ed into; running gitui there would root the TUI at a subdirectory instead of the repository. A field counts only if it is a present, non-blank string; a non-string or whitespace-only value counts as absent and the chain falls through to the next candidate.

If no field yields a directory, the action exits 1 with `no directory in plugin context`. This happens when the action is invoked outside a workspace context.

## Snapshot probes

In pane mode, the plugin takes a **before** snapshot, runs gitui, then takes an **after** snapshot. All probes are read-only (`git -C <dir>` / `current_dir(dir)`); the index and working tree are never mutated.

| Field       | Command                                                       | Parse                                  |
| ----------- | ------------------------------------------------------------- | -------------------------------------- |
| `in_repo`   | `git rev-parse --is-inside-work-tree`                         | exit 0 and stdout `true`               |
| `head`      | `git rev-parse HEAD`                                          | exit 0 → full sha, else None           |
| `branch`    | `git symbolic-ref --short -q HEAD`                            | non-empty stdout; detached → None      |
| `upstream`  | `git rev-parse --abbrev-ref --symbolic-full-name @{upstream}` | exit 0 → e.g. `origin/main`, else None |
| `ahead`     | `git rev-list --count @{upstream}..HEAD`                      | exit 0 → parse u64, else None          |
| `staged`    | `git diff --cached --name-only`                               | count non-empty lines                  |
| `unstaged`  | `git diff --name-only`                                        | count non-empty lines                  |
| `untracked` | `git ls-files --others --exclude-standard`                    | count non-empty lines                  |

Not a work tree → snapshot with `in_repo=false`, everything else None/0, and the summary is always empty (gitui in a non-repo dir: nothing to report). Probe failures degrade to `None`/defaults with one stderr warning `herdr-gitui: warning: git probe failed (<cmd>): <err>` — never fatal.

## Summary rules

The summary is a pure function of the before/after snapshots and the directory. Output is a string (empty = nothing to report), lines joined with `\n`, each line `- ` prefixed, in this order:

1. **Either snapshot not `in_repo`** → empty summary.
2. **Branch switch**: `before.branch != after.branch` (compare Option values; detached HEAD counts as `None`) → `- switched to branch <after>` or `- switched to a detached HEAD at <short after.head>` when after.branch is None. Short sha = first 7 chars. When both `after.branch` and `after.head` are `None` (no commits yet and detached), the fallback text is `unknown`. When the branch switched, rules 3–5 are skipped (they are same-branch only); rules 6–7 still apply. On branch switch, up to 5 commits unique to the new HEAD are listed via `git log --no-merges --format=%h%x09%s -n 5 <after.head>` as `- branch <name> is at commit <h>: <subject>` lines.
3. **Created commits** (same branch, both heads present, `before.head` is an ancestor of `after.head` — probe `git merge-base --is-ancestor <before> <after>`, before != after): list `git log --no-merges --reverse --format=%h%x09%s <before>..<after>`, cap 10 lines: `- created commit <h>: <subject>` each, then `- ... and <N> more commits` when truncated. If the new HEAD commit itself is a merge (`git rev-list --merges <before>..<after>` non-empty), add one line `- merged into branch <after.branch>` after the commit lines.
4. **Amend/rebase** (same branch, heads differ, before.head NOT an ancestor of after.head, after.head not ancestor of before.head): `- amended or rebased branch <branch>: HEAD moved from <short before> to <short after>`.
5. **Reset** (same branch, after.head is an ancestor of before.head, heads differ): `- reset branch <branch>: HEAD moved back from <short before> to <short after>`.
6. **Push** (same upstream present on both sides, before.ahead and after.ahead present, after < before): `- pushed <before.ahead - after.ahead> commit(s) to <upstream>`.
7. **Staging changes**: for each of staged/unstaged/untracked where after > before: `- staged <delta> more file(s)` / `- unstaged <delta> more file(s)` / `- <delta> more untracked file(s)`. Decreases are not reported.

Rationale: the summary is heuristic — it infers what happened from read-only git state, not from intercepting gitui's internal operations. This keeps the plugin simple and resilient to gitui version changes.

Caveats:

- **Amend vs. rebase** — both produce the same git state (HEAD moved non-linearly on the same branch); the plugin cannot distinguish them and reports "amended or rebased".
- **Push detection** — only catches ahead-count decrease with an upstream. A push from another clone, or a push that does not change the local ahead count (e.g. `git push` when already up to date), is invisible.
- **Staging deltas** — counts only, not file names. A staging decrease is not reported.
- **Branch switch** — skips commit-range rules (3–5) because the before/after heads are on different branches.

Any git probe failure inside summary building degrades to omitting that line (warn once).

## Launch and lifecycle

### Action mode

1. Parse the context JSON, resolve the repository directory and the caller pane/agent (see [Context](#context)).
2. Load the pane settings once from `<config-dir>/config.toml`. The config directory is `HERDR_PLUGIN_CONFIG_DIR` when it is set to a non-blank value; otherwise the action queries `herdr plugin config-dir alexeyco.gitui` through `HERDR_BIN_PATH` and takes the bare path line it prints. If neither yields a directory, the action proceeds with all defaults (`placement = tab`, `direction = right`, `width`/`height` unset) and warns on stderr. Settings failures are never fatal: a missing `config.toml` is silent defaults; corrupt or unreadable TOML is all defaults with one warning; an unknown key warns while known keys keep their effect; an invalid value falls back to that key's default with a warning while the other keys keep theirs (for the popup size keys that default is "unset": no flag). A loaded placement can still degrade at open time: `split`/`zoomed` without a resolvable caller pane falls back to `tab` with a warning (step 3). Warnings go to stderr and are visible via `herdr plugin log list --plugin alexeyco.gitui`. The file is read once per action launch, so an edit applies to the next launch, not to a running session. Pane mode reads no config.
3. Open the gitui pane:

   ```
   herdr plugin pane open --plugin alexeyco.gitui --entrypoint open --placement tab \
     [--workspace <workspace_id>] --cwd <dir> --env HERDR_GITUI_DIR=<dir> \
     [--env HERDR_GITUI_CALLER_PANE=<focused_pane_id>] \
     [--env HERDR_GITUI_CALLER_AGENT=<focused_pane_agent>] --focus
   ```

   The template shows the default placement. `--placement` takes its value from the settings loaded in step 2 (`tab`, `split`, `overlay`, `zoomed`, or `popup`), `--direction right|down` is inserted immediately after the placement value only when the effective placement is `split` and the configured direction is not `auto` — for any other placement a configured `direction` is ignored, and `auto` omits the flag. For `popup` the optional `--width <v>` / `--height <v>` pairs sit immediately after the placement value, each emitted only when its settings key resolved, and the action additionally exports `--env HERDR_GITUI_PLACEMENT=popup` right after the `HERDR_GITUI_DIR` pair — the only popup-only argv addition, so every non-popup argv stays byte-identical and pane mode can gate its teardown on it (step 9). `split` and `zoomed` open beside or over the caller pane: `--target-pane <focused_pane_id>` follows the direction position (or the placement value when no direction is passed), reusing the same caller pane id the action exports as `HERDR_GITUI_CALLER_PANE`; `tab`, `overlay`, and `popup` never carry the flag. Anchoring is mutually exclusive per placement: `--workspace <workspace_id>` (from the context or `HERDR_WORKSPACE_ID`) is passed only for `tab`, while `split`, `zoomed`, `overlay`, and `popup` anchor to a pane — the caller pane, or the daemon-derived active pane for `overlay` and `popup` (which get no `--target-pane` from the plugin) — instead of a workspace, so the daemon rejects any `--workspace` on those opens. When `split`/`zoomed` is configured but no caller pane resolves, the action warns on stderr (`herdr-gitui: warning: ...`) and opens with the `tab` shape — with `--workspace` back when a workspace id resolved — rather than letting herdr fail the open. `--cwd <dir>` roots the pane at the repository resolved above, and `--env HERDR_GITUI_DIR=<dir>` exports the same directory so pane mode does not have to trust its own cwd. Each caller `--env` pair is added only for the caller values that are present. `--focus` stays unconditional: focus is not a setting.

4. Parse `result.plugin_pane.pane.pane_id` (required) / `.tab_id` (optional) from the JSON reply. Log the pane and tab id to stderr (visible via `herdr plugin log list`) and exit 0. When the reply carries a tab id, the action then re-asserts focus with a best-effort `herdr tab focus <tab_id>` (errors ignored) — purely defensive against a client-side focus race, since herdr already applies `--focus` at open. A `popup` open is the one exempt reply: the daemon answers `{"id":"cli:plugin","result":{"type":"ok"}}` with no pane id and no tab id (live-verified on herdr 0.9.5 — a popup is a singleton session resource, not a Herdr pane), so the action treats it as success, logs `herdr-gitui: opened gitui popup` without identifiers, and makes no `tab focus` call because there is no tab; a popup reply that is invalid JSON or carries a non-`ok` result (e.g. `ui_busy` while another Herdr modal is active) fails through the unchanged pane-open error path. The action does no tab management, no polling, and no snapshot work: all of that happens in the pane process.

### Pane mode

The pane command is `["./target/release/herdr-gitui"]`, spawned by herdr as a process attached to the pane pty. Its working directory is the `--cwd` passed to `pane open` (the repository). Because herdr runtime commands may default to the plugin directory as cwd, pane mode prefers `HERDR_GITUI_DIR` (exported by the action) over its own process cwd, and launches gitui with that directory as the child's working directory.

1. `require_bin()` — verify `HERDR_BIN_PATH` is present before anything else.
2. Dir = `HERDR_GITUI_DIR` (non-blank) else process cwd.
3. State dir: `HERDR_PLUGIN_STATE_DIR/sessions/` (temp dir when unset); create it; prune `*.txt` files older than 7 days (best-effort, warn `herdr-gitui: warning: cannot remove <path>: <err>`).
4. Resolve gitui binary: first executable `gitui` on `$PATH`, canonicalized to an absolute path (canonicalize failure = loud error `found gitui at <path> but cannot resolve it to an absolute path: <err>`). None found → error `gitui not found on PATH; install it from https://github.com/gitui-org/gitui`. No minimum version is checked.
5. Take the **before** git snapshot in `dir`. All probes are read-only; probe failures degrade to `None`/defaults with one stderr warning.
6. Print `herdr-gitui: starting gitui in <dir>` to stdout (immediately covered by the TUI).
7. Spawn gitui as a **direct child**, bare argv `[<gitui_bin>]` with NO arguments and `current_dir(dir)` — gitui discovers the repository from its working directory. It takes no positional path (gitui 0.28 rejects one with `error: unexpected argument`), and its `-d/--directory` flag is the _git dir_ (GIT_DIR), not the repo root, so no flags are passed. tty inherited (`.status()`). Wait; read exit code.
8. Take the **after** snapshot; build the session summary.
9. Act on the outcome (see [Outcome mapping](#outcome-mapping)). Plugin panes auto-close when the pane process exits, so on the quiet-success paths — summary delivered, or no changes — the plugin still closes the pane explicitly (best-effort; the process exit is the backup):

   ```
   herdr plugin pane close <HERDR_PANE_ID>
   ```

   with a best-effort fallback to `herdr tab close <HERDR_TAB_ID>` when the pane id is missing or the close fails. A close failure is logged, never fatal. On the error and no-agent paths the plugin instead prints its message and then blocks on a stdin read (`herdr-gitui: press Enter to close`) until you press Enter, so the message stays readable before the pane auto-closes. Popup sessions skip the teardown entirely: when the pane process sees `HERDR_GITUI_PLACEMENT=popup` (the popup-only export from step 3), `close_pane()` returns without making any herdr call — the popup process carries the plugin context of the underlying tiled caller pane, so a close-the-own-pane call would close the user's agent pane, and the popup closes naturally when gitui exits (herdr-side behavior). Summary delivery is unaffected: the prompt addresses `HERDR_GITUI_CALLER_PANE`, never the session's own id.

If you close the tab yourself while gitui is open, the pane process is killed: nothing is delivered, and whatever state existed stays on disk (pruned after 7 days on a later pane start). This is the intended cancel path.

## Outcome mapping

| gitui exit       | Summary   | Outcome                                                                                                                                 |
| ---------------- | --------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `0`              | empty     | No changes; nothing is sent, no file written, pane closed, exit 0.                                                                      |
| `0`              | non-empty | Summary delivered to the caller agent pane.                                                                                             |
| `!= 0`           | (any)     | gitui failed; summary file kept if written, error logged, hold for Enter — the pane never flash-closes before the user reads the error. |
| no code (signal) | (any)     | Treated as a failure; error logged, hold for Enter.                                                                                     |

## Delivery

The summary is delivered only when gitui exited 0 with a non-empty summary and the pane was opened with both `HERDR_GITUI_CALLER_PANE` and `HERDR_GITUI_CALLER_AGENT`. Delivery calls:

```
herdr agent prompt <HERDR_GITUI_CALLER_PANE> <text>
```

with the summary text prefixed by `gitui session summary:\n\n`. The plugin is agent-agnostic: it does not inspect the agent kind, so any agent herdr supports receives the prompt.

The delivered prompt is capped at 64 KiB (65536 bytes), well under `ARG_MAX` since the text travels as a single argv element. The cut is made on a UTF-8 boundary, and when it truncates the text the plugin appends `\n\n(output truncated; full summary kept at <path>)`, so the full summary stays available on disk.

Fallbacks and edge behavior:

- **No agent caller value** — the session file is kept on disk, `herdr-gitui: no agent in focused pane; summary kept at <path>` is logged, and the pane blocks on `herdr-gitui: press Enter to close` until you press Enter.
- **gitui error (non-zero exit or signal)** — if a summary exists it is written to the session file first (kept); the error is logged, and the pane holds until Enter regardless of whether a summary exists, so the pane never flash-closes before gitui's own error output and the plugin message have been read.
- **Exit 0 with empty summary** — treated as "no changes"; no file is written and the pane is closed.
- **Delivery call fails** — the error is logged, the session file is kept, and the pane holds until Enter.
- **Tab closed early** — the pane process is killed; nothing is sent, and whatever state existed stays on disk (pruned after 7 days on a later pane start).

The `<pid>-<unix millis>.txt` file is written only when a summary exists (rc 0 with a non-empty summary, or rc ≠ 0 with a summary); it is removed on successful delivery and kept when delivery is skipped (no agent), when delivery fails, or when gitui fails. On a clean no-change exit (rc 0, empty summary) and on the killed/signal path, no file is written in the first place.

## Binary and state

- gitui lookup: the first executable `gitui` on `$PATH`, canonicalized to an absolute path since the pane process may have a different `$PATH` than the action process. There is no override environment variable. A candidate that cannot be canonicalized is a loud error — `found gitui at <path> but cannot resolve it to an absolute path: <err>` — never a cwd-relative guess. When no binary is found, the plugin fails with an install hint pointing at <https://github.com/gitui-org/gitui>.
- State lives under `HERDR_PLUGIN_STATE_DIR/sessions/`: a single `<pid>-<unix millis>.txt` file per session (the pid makes the name collision-safe across concurrent sessions). `HERDR_PLUGIN_STATE_DIR` is expected to be an absolute path and is used as given; when it is unset, the system temp directory is used. Kept files older than 7 days are pruned at pane start (best-effort, failures logged as warnings).
- Every herdr callback uses `HERDR_BIN_PATH`, never a bare `herdr`.

## JSON parsing

The context is parsed by [`serde_json`](https://crates.io/crates/serde_json) into a generic `Value`. The plugin then takes the first present, non-blank string along the resolution chain (`worktree.checkout_path` → `workspace_cwd` → `focused_pane_cwd`). Standard JSON escaping, including `\uXXXX` with surrogate pairs, survives, so non-ASCII repository paths are preserved.

## Troubleshooting

- **`no directory in plugin context`** — the action ran without workspace context. Invoke it from inside a herdr workspace (e.g. via the keybinding), not from a bare shell.
- **`invoked outside herdr plugin context`** — the binary was run directly, without `HERDR_PLUGIN_ACTION_ID` or `HERDR_PLUGIN_ENTRYPOINT_ID`. Invoke it through herdr.
- **`gitui not found on PATH`** — install [gitui](https://github.com/gitui-org/gitui) and make sure the `gitui` binary is on `PATH`.
- **Nothing delivered** — nothing changed during the gitui session. The plugin logs `no repository changes detected` and closes the pane. Make changes in gitui (commit, switch branch, push, stage files) and quit again.
- **gitui fails to start** — the plugin resolves the gitui binary before printing anything, so a missing gitui fails with `gitui not found on PATH; install it from …` before `starting gitui` is printed. A spawn failure after the binary was found is printed alongside the `starting gitui` line. Either way the pane waits for Enter before auto-closing. Fix the cause and re-run the action; check the action side with `herdr plugin log list --plugin alexeyco.gitui`.
- **Summary not delivered** — the caller pane had no agent, or delivery failed. The session summary is kept under `HERDR_PLUGIN_STATE_DIR/sessions/`, and the pane waits for Enter before auto-closing. The kept path is printed. Focus an agent pane and re-run the action.
- **`herdr did not provide HERDR_BIN_PATH (plugin API v1 contract)`** — the process was not given the herdr binary path it needs for callbacks. Run the plugin through herdr rather than directly, and check the herdr install.
- **The action appears hung** — herdr CLI calls (`plugin pane`, `tab`, `agent`) have no timeout by design in plugin API v1, so a hung herdr daemon blocks the action; check the daemon first.
- Action logs: `herdr plugin log list --plugin alexeyco.gitui`.
