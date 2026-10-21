# Support gitui and lazygit

## Why now

The plugin currently launches only gitui, so users who use lazygit cannot use the same Herdr session workflow. Supporting both tools lets each user launch the TUI already installed on their system while preserving the plugin's existing repository snapshot and summary delivery behavior.

## What changes

Action mode detects `gitui` and `lazygit` on `PATH`. It launches the only available tool directly, opens a setup popup when neither is available, and opens a picker popup on every invocation when both are available. The picker starts on the last explicitly selected tool and saves the new explicit choice under `$HERDR_PLUGIN_STATE_DIR/last-tool`. If that remembered tool is no longer installed and exactly one tool remains, the plugin launches the remaining tool without showing the picker or changing the saved choice. Pane mode launches the selected tool and labels its session summary with that tool's name.

The picker follows herdr-open-in controls: arrows or `k`/`j` move selection, Enter confirms, and Esc or `q` cancels. The plugin gives install guidance for both gitui and lazygit when neither is available; users install and configure the tools themselves.

The plugin id and qualified action become `alexeyco.git` and `alexeyco.git.open`; the action and session pane entrypoint ids remain `open`. The manifest also declares `picker` and `setup` popup entrypoints, and `src/main.rs` dispatches them to distinct flows. Users configure the new action name themselves. Existing pane settings, read-only git snapshots, summary delivery, and outcome rules remain in effect. Tab-placement sessions return focus to the tab that invoked the action after the session ends.

The README competitor comparison remains a separate change; this change updates only the installation and usage documentation needed for the two supported tools.

## Impact

- Modifies `gitui-integration` to cover detection, selection, and launching of both supported TUIs.
- Modifies `session-outcomes` to retain the existing outcome behavior for either TUI.
- Adds `tool-selection` for action-mode detection, picker behavior, and saved explicit preference.
- Modifies `pane-launch` for the new plugin/action identity, popup requests, and declared pane entrypoints; modifies `pane-settings` for the new config-directory callback id.

ASSUMPTION: Users install gitui and lazygit themselves; the plugin detects them on `PATH` and provides installation guidance but does not install them.
ASSUMPTION: The picker uses herdr-open-in controls (arrows or `k`/`j`, Enter, Esc or `q`) and preselects the last explicitly chosen tool.
ASSUMPTION: The selector appears only when both tools are installed; neither installed shows setup guidance, and exactly one installed launches directly.
ASSUMPTION: If the remembered tool disappears and exactly one tool remains, the plugin launches the remaining tool without a picker and leaves the saved explicit choice unchanged.
ASSUMPTION: The `picker` popup receives paths, saved preference, repository directory, and caller context through plugin-owned `HERDR_GIT*` environment values; `src/main.rs` dispatches it through `session::run_picker()`, which consumes the picker result and opens the `open` session pane (untested — this would be disproved if Herdr cannot forward those values through `--env` to popup entrypoints).
ASSUMPTION: The picker and setup terminal interfaces can use the standard library without adding a crate dependency (untested — this would be disproved if the chosen UI implementation requires an external crate).
