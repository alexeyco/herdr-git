# Pane launch

## MODIFIED Requirements

### Requirement: Manifest identity defines the qualified action name

The plugin manifest SHALL declare plugin id `alexeyco.git` and action id
`open`, producing the sole qualified action identifier
`alexeyco.git.open`. The action id and session pane entrypoint id SHALL remain
`open`; the manifest SHALL also declare popup pane entrypoints `picker` and
`setup`, each handled by a distinct pane-mode flow;
the plugin SHALL NOT expose `alexeyco.gitui.open` or `gitui.open` as aliases.
Users SHALL configure the new action name themselves; the plugin SHALL NOT
rewrite user configuration or provide migration behavior.

#### Scenario: Qualified action is invoked

- **WHEN** a Herdr setting binds `command = "alexeyco.git.open"` or a user runs `herdr plugin action invoke alexeyco.git.open`
- **THEN** Herdr invokes the plugin's `open` action

#### Scenario: Old action name is not an alias

- **WHEN** a setting or invocation uses `gitui.open` or `alexeyco.gitui.open`
- **THEN** this plugin does not resolve that name as an alias for `alexeyco.git.open`

#### Scenario: Explicit configuration is user-owned

- **WHEN** a user's external Herdr settings still name the previous qualified action
- **THEN** the plugin neither rewrites those settings nor resolves the previous qualified action as an alias; the user must configure `alexeyco.git.open`

### Requirement: Pane open passes a fixed argv and requires a pane id in the reply

Action mode SHALL retain the existing session pane-open argv ordering,
placement and reply handling, while using `--plugin alexeyco.git` for every
pane-open call. The `--entrypoint open`, settings-driven placement, caller
context, popup handling, pane-id validation, and optional tab-id behavior
for the session pane SHALL remain unchanged. When action mode requests the
tool picker or setup guidance, it SHALL open the corresponding `picker` or
`setup` entrypoint with popup placement and SHALL NOT route those entrypoints
through the `open` session pane flow.

#### Scenario: Manifest declares all pane entrypoints

- **WHEN** the manifest is loaded
- **THEN** it declares pane ids `open`, `picker`, and `setup`, with `open` at tab placement and `picker` and `setup` at popup placement

#### Scenario: Pane mode dispatches by entrypoint id

- **WHEN** Herdr starts the binary with `HERDR_PLUGIN_ENTRYPOINT_ID` set to `open`, `picker`, or `setup`
- **THEN** `src/main.rs` dispatches respectively to the session flow, `session::run_picker()` wrapper, or separate setup flow

#### Scenario: Unknown pane entrypoint fails

- **WHEN** pane mode starts with an entrypoint id other than `open`, `picker`, or `setup`
- **THEN** it returns an error that identifies the unknown id and expected entrypoints

#### Scenario: Picker and setup are requested as popups

- **WHEN** action mode needs a picker or install guidance
- **THEN** it invokes the `picker` popup with the canonical tool paths, saved preference, repository directory, and caller context, or invokes the `setup` popup when no tool is installed; both requests use `--plugin alexeyco.git`, their corresponding `--entrypoint`, and `--placement popup`

#### Scenario: Picker opens the chosen session pane

- **WHEN** the user confirms a tool in the picker popup
- **THEN** the picker publishes the confirmed tool id and closes its compact popup; the action then opens the `open` pane with the chosen tool id and canonical executable path using configured placement and popup dimensions

#### Scenario: Popup picker preserves configured dimensions

- **WHEN** configured placement is `popup` and width or height is set
- **THEN** the picker uses its compact manifest dimensions, closes before a second popup opens, and the selected tool popup receives the configured size flags

#### Scenario: Picker declares compact dimensions

- **WHEN** the plugin manifest is loaded
- **THEN** the `picker` popup has width `52` and height `7`, independently of user settings for the selected TUI popup

#### Scenario: Picker preserves the caller tab

- **WHEN** the action context includes caller tab id `T`, the user selects a tool, and configured placement is not `popup`
- **THEN** the picker forwards `T` to the selected session pane as `HERDR_GITUI_CALLER_TAB`

#### Scenario: Full context

- **WHEN** workspace id `W`, caller pane `P`, and caller agent `A` are resolved for directory `D` with default settings
- **THEN** the argv is `plugin pane open --plugin alexeyco.git --entrypoint open --placement tab --workspace W --cwd D --env HERDR_GITUI_DIR=D --env HERDR_GITUI_CALLER_PANE=P --env HERDR_GITUI_CALLER_AGENT=A --focus`

#### Scenario: Minimal context

- **WHEN** no workspace id, caller pane, or caller agent resolves for directory `D` with default settings
- **THEN** the argv is `plugin pane open --plugin alexeyco.git --entrypoint open --placement tab --cwd D --env HERDR_GITUI_DIR=D --focus`

#### Scenario: Split placement with explicit direction

- **WHEN** placement is `split`, direction is `down`, and caller pane `P` resolves
- **THEN** argv contains `--plugin alexeyco.git --entrypoint open --placement split --direction down --target-pane P` and no `--workspace` token

#### Scenario: Split placement with auto direction

- **WHEN** placement is `split`, direction is `auto`, and caller pane `P` resolves
- **THEN** argv contains `--plugin alexeyco.git --entrypoint open --placement split --target-pane P` with no `--direction` or `--workspace` token

#### Scenario: Direction ignored for non-split placements

- **WHEN** placement is `zoomed`, `overlay`, or `tab`, with direction `down`
- **THEN** argv uses `--plugin alexeyco.git`, omits `--direction`, and retains the existing target-pane and workspace rules for each placement

#### Scenario: Split targets the caller pane

- **WHEN** placement is `split`, direction is `down`, and caller pane `P` resolves
- **THEN** argv includes `--plugin alexeyco.git --placement split --direction down --target-pane P` and no `--workspace` token

#### Scenario: Zoomed targets the caller pane

- **WHEN** placement is `zoomed` and caller pane `P` resolves
- **THEN** argv includes `--plugin alexeyco.git --placement zoomed --target-pane P`, without `--direction` or `--workspace`

#### Scenario: Overlay omits workspace targeting

- **WHEN** placement is `overlay` and workspace id `W` resolves
- **THEN** argv includes `--plugin alexeyco.git --placement overlay` and omits `--workspace` and `--target-pane`

#### Scenario: Unresolvable caller pane falls back to tab with warning

- **WHEN** placement is `split` or `zoomed` and no caller pane resolves
- **THEN** the action warns and uses the existing tab argv shape with `--plugin alexeyco.git`, no `--direction` or `--target-pane`, and `--workspace` when resolved

#### Scenario: Popup passes size flags

- **WHEN** placement is `popup` with width `80`, height `80%`, directory `D`, and caller pane `P`
- **THEN** argv is `plugin pane open --plugin alexeyco.git --entrypoint open --placement popup --width 80 --height 80% --cwd D --env HERDR_GITUI_DIR=D --env HERDR_GITUI_PLACEMENT=popup --env HERDR_GITUI_CALLER_PANE=P --focus`, with no workspace, target-pane, or direction flags

#### Scenario: Reply without pane id

- **WHEN** a non-popup reply lacks a pane id or is invalid JSON
- **THEN** the action reports the existing missing-id or parse error; popup reply handling remains unchanged

#### Scenario: Reply without tab id

- **WHEN** the reply includes a valid pane id but no tab id
- **THEN** the open succeeds without reporting a tab id
