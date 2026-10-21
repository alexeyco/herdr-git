# Design

## Action-mode tool selection

Add a small tool-selection layer that resolves executable `gitui` and `lazygit` candidates from `PATH` in action mode. Preserve the existing first-executable and canonical absolute path behavior for each candidate. When both resolve, show the interactive picker on every action invocation. The saved id selects the initial highlighted option when it names an installed tool; otherwise use the existing deterministic option order, with gitui first. When only one resolves, pass it directly to the pane-open flow without showing a selector.

When neither resolves, request the `setup` popup and end the action without opening a tool pane. When both resolve, request the compact `picker` popup and pass both canonical executable paths plus the saved preference to it. For configured popup placement, the action creates a one-shot selection ticket and waits while the picker is open. On confirmation, the picker saves the preference and publishes the selected tool id; on cancellation it publishes cancellation. The action calls Herdr's `popup.close`, then opens the selected tool with configured popup dimensions. Other placements retain the current picker-to-session-pane handoff. The picker declares compact dimensions (52 columns by 7 rows) in the manifest. Its first row is blank, the tool list follows, one blank row separates the list from the help, and the help occupies the final row without horizontal padding. The plugin does not download or install either program.

## Entrypoint dispatch and popup flows

Declare three panes in `herdr-plugin.toml`: `open` with `placement = "tab"`, `picker` with `placement = "popup"`, and `setup` with `placement = "popup"`. Keep the existing action id `open`. Dispatch `HERDR_PLUGIN_ENTRYPOINT_ID=open` to the session pane, `picker` to `session::run_picker()`, and `setup` directly to install guidance. Action mode requests `picker` or `setup` through `herdr plugin pane open --plugin alexeyco.git --entrypoint <id> --placement popup`; setup remains separate from the session and picker flow.

Pass `HERDR_GITUI_PATH` and `HERDR_LAZYGIT_PATH` only for detected tools, and pass `HERDR_GIT_LAST_TOOL` when the saved choice is valid. Pass the resolved repository directory as `--cwd` and forward existing workspace, caller pane, and caller agent context through plugin-owned environment values. `picker::run()` returns the confirmed tool or cancellation without opening a pane. `session::run_picker()` saves confirmed choices. For non-popup placements it opens the selected session pane itself; for popup placement it atomically publishes the selected id or cancellation through the one-shot ticket so the action can close the picker and open the configured TUI popup. The setup popup gives install links for gitui and lazygit and exits after the user closes it.

## Plugin identity

Set the manifest plugin id to `alexeyco.git` while retaining action and pane ids `open`, producing the sole qualified action `alexeyco.git.open`. Update Herdr callback references that require the plugin id, including the config-directory lookup, to `alexeyco.git`. Users configure the new action name themselves; the plugin does not add or document an alias or perform configuration migration.

## Saved explicit choice

Use `$HERDR_PLUGIN_STATE_DIR/last-tool` for the preference file. Its contents are a bare tool id (`gitui` or `lazygit`) followed by a newline. Reads are forgiving: missing, unreadable, empty, or unknown values act as no preference. Save only after an explicit picker confirmation. Write through a temporary file and rename it into place; ignore save failures. Automatic launch with one available tool never changes the saved explicit choice.

## Pane session

Pass the selected tool id and canonical binary path from action mode to pane mode through plugin-owned environment values, alongside the existing repository directory and caller context. Pane mode dispatches to the selected TUI in the same repository directory and captures its exit status. Keep snapshots before and after the child process and use the existing outcome and agent-delivery pipeline. Prefix the summary with `gitui session summary:` or `lazygit session summary:` according to the selected tool.

For all non-popup placements, preserve the initiating context's `tab_id` as `HERDR_GITUI_CALLER_TAB` through the picker and session-pane handoff. Before clean pane teardown, focus that caller tab best-effort; on held error or no-agent paths, do so after Enter releases the hold. Popup sessions do not restore a caller tab.

## Files to touch

- `src/gitui.rs` — shared executable lookup for the supported tools.
- `src/session.rs` — action availability routing, picker wrapper, selected-tool handoff, tool pane execution, and tool-specific summary labels.
- `src/herdr.rs` — action requests for the `picker` and `setup` popup entrypoints.
- `src/main.rs` — entrypoint-id dispatch for `open`, `picker`, and `setup`, with the picker routed through `session::run_picker()`, setup kept separate, and unknown-id errors.
- `src/picker.rs` — picker popup UI and selected-tool/cancellation result.
- `src/setup.rs` — install guidance popup for both tools.
- `src/state.rs` — best-effort saved explicit tool preference.
- `herdr-plugin.toml` — plugin id `alexeyco.git`, action id `open`, and pane entrypoints `open`, `picker`, and `setup`.
- `README.md` — describe both tools, availability routing, picker controls, and the new action identifier.
- `openspec/specs/gitui-integration/spec.md` — sync the approved integration behavior after implementation.
- `openspec/specs/session-outcomes/spec.md` — sync the preserved outcome behavior after implementation.
- `openspec/specs/pane-launch/spec.md` — sync the qualified action identity after implementation.
- `openspec/specs/pane-settings/spec.md` — sync the config-directory callback id after implementation.
- `openspec/specs/tool-selection/spec.md` — create the baseline capability spec from the approved `tool-selection` delta after implementation.

The action id and session pane entrypoint id remain `open`; pane settings, read-only snapshots, summary delivery, and existing outcome mapping stay unchanged. The picker and setup UI use the standard library and add no crate dependency.
