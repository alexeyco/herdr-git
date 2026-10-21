# Pane launch

## Purpose

Action mode is the entry point herdr invokes for the plugin's `open` action (declared in `herdr-plugin.toml`: plugin id `alexeyco.git`, version `0.1.0`, action id `open`, session pane id `open`, with popup `picker` and `setup` entrypoints, `min_herdr_version "0.9.0"`, and no keybindings declared). It reads the plugin context herdr passes as JSON, resolves the repository directory, and calls back into herdr with `herdr plugin pane open --plugin alexeyco.git` to launch a tool pane, exporting the resolved directory and caller identities through `HERDR_GITUI_*` environment variables. On success the action prints the new pane id and exits immediately; pane setup and the session itself continue in pane mode.

## Requirements

### Requirement: Manifest identity defines the qualified action name

The manifest SHALL declare plugin id `alexeyco.git`, action id `open`, and
session pane entrypoint id `open`, making `alexeyco.git.open` its only action
identifier. It SHALL provide separate `picker` and `setup` popup entrypoints,
without legacy aliases or automatic user-configuration migration.

#### Scenario: Qualified action is invoked

- **WHEN** a Herdr setting binds `command = "alexeyco.git.open"` or a user runs `herdr plugin action invoke alexeyco.git.open`
- **THEN** Herdr invokes the plugin's `open` action

#### Scenario: Old action name is not an alias

- **WHEN** a setting or invocation uses `gitui.open` or `alexeyco.gitui.open`
- **THEN** this plugin does not resolve that name as an alias for `alexeyco.git.open`

#### Scenario: Explicit configuration is user-owned

- **WHEN** a user's external Herdr settings still name the previous qualified action
- **THEN** the plugin neither rewrites those settings nor resolves the previous qualified action as an alias; the user must configure `alexeyco.git.open`

### Requirement: Action mode requires a non-blank plugin context JSON

Action mode SHALL read `HERDR_PLUGIN_CONTEXT_JSON` and treat it as required:
when the variable is absent or its value is blank, it SHALL fail with
`HERDR_PLUGIN_CONTEXT_JSON is not set; invoke this action from herdr`, and
invalid JSON SHALL fail as an invalid-context error.
(src/session.rs:96-102, src/context.rs:21-22)

#### Scenario: Context variable missing or blank

- **WHEN** action mode runs with `HERDR_PLUGIN_CONTEXT_JSON` unset, empty, or whitespace-only
- **THEN** the action fails with "HERDR_PLUGIN_CONTEXT_JSON is not set; invoke this action from herdr" and no herdr callback is attempted

#### Scenario: Malformed context JSON

- **WHEN** `HERDR_PLUGIN_CONTEXT_JSON` contains text that is not valid JSON
- **THEN** the action fails with an "invalid plugin context JSON" error

### Requirement: Repository directory resolves along a fixed fallback chain

Action mode SHALL resolve the repository directory as the first non-blank
string among `worktree.checkout_path`, `workspace_cwd`, and
`focused_pane_cwd` in that order; a non-string or whitespace-only value
counts as absent; when none is present it SHALL fail with
`no directory in plugin context; invoke this action inside a workspace`.
(src/context.rs:8-16,23-25,37-53)

#### Scenario: Checkout path wins

- **WHEN** the context has a non-blank `worktree.checkout_path` alongside other directory fields
- **THEN** the resolved directory is the checkout path

#### Scenario: Later chain entries fill in blanks

- **WHEN** `worktree.checkout_path` is blank or non-string and `workspace_cwd` is non-blank
- **THEN** the resolved directory is `workspace_cwd`; with only `focused_pane_cwd` non-blank, it is used instead

#### Scenario: Empty chain

- **WHEN** all three directory fields are absent, blank, or non-string
- **THEN** the action fails with "no directory in plugin context; invoke this action inside a workspace"

### Requirement: Workspace and caller pane ids fall back to environment; caller agent does not

Action mode SHALL use `workspace_id` and `focused_pane_id` from the context,
falling back to `HERDR_WORKSPACE_ID` and `HERDR_PANE_ID` respectively when
the context value is absent or blank; `focused_pane_agent` SHALL be taken
from the context only, with no environment fallback.
(src/session.rs:104-110, src/context.rs:56-66)

#### Scenario: Context values preferred

- **WHEN** the context provides non-blank `workspace_id` and `focused_pane_id`
- **THEN** those values are used even if the fallback variables are set

#### Scenario: Environment fallback

- **WHEN** the context omits `workspace_id` but `HERDR_WORKSPACE_ID` is non-blank, or omits `focused_pane_id` but `HERDR_PANE_ID` is non-blank
- **THEN** the environment value is used

#### Scenario: Agent without context value

- **WHEN** the context has no non-blank `focused_pane_agent`
- **THEN** no caller agent is passed on, regardless of any environment variable

### Requirement: Herdr callbacks execute only the HERDR_BIN_PATH binary

Action mode's herdr callbacks — the settings-layer `plugin config-dir` query,
pane open, and the best-effort tab focus — SHALL exec the binary at
`HERDR_BIN_PATH` and SHALL fail with a contract error when it is missing or
blank. This requirement is the origin of that contract: every other
capability's herdr call cross-references it rather than restating it, and one
shared check guards every herdr call the plugin makes in either mode
(src/herdr.rs:8-13,28-36).

#### Scenario: Bin path absent at pane open

- **WHEN** action mode reaches the pane-open call with `HERDR_BIN_PATH` unset or whitespace-only
- **THEN** the action fails with "herdr did not provide HERDR_BIN_PATH (plugin API v1 contract); cannot call back into herdr"

#### Scenario: Tab focus passes through the same guard

- **WHEN** the best-effort tab-focus call runs with `HERDR_BIN_PATH` missing or blank
- **THEN** the shared check rejects that call too, producing the same contract error as its internal failure (which the best-effort caller ignores)

#### Scenario: Config-dir query passes through the same guard

- **WHEN** the settings layer attempts the `plugin config-dir alexeyco.git` fallback query with `HERDR_BIN_PATH` missing or blank
- **THEN** the shared check rejects that call as well, and the settings layer treats the rejection as "config directory unavailable" (a warning, not an action failure)

### Requirement: Pane open passes a fixed argv and requires a pane id in the reply

Action mode SHALL open session panes with the established argv ordering,
settings-driven placement, caller context, and reply handling, using
`--plugin alexeyco.git`. Picker and setup requests SHALL use their matching
popup entrypoints, not the session pane flow. The picker SHALL be borderless,
24 columns by 6 rows, with blank rows above and below the tool list, no help
line, reset colors, and reverse-video selection.

#### Scenario: Manifest declares all pane entrypoints

- **WHEN** the manifest is loaded
- **THEN** it declares pane ids `open`, `picker`, and `setup`, with `open` at tab placement and `picker` and `setup` at popup placement

#### Scenario: Pane mode dispatches by entrypoint id

- **WHEN** Herdr starts the binary with `HERDR_PLUGIN_ENTRYPOINT_ID` set to `open`, `picker`, or `setup`
- **THEN** `src/main.rs` dispatches respectively to the session flow, picker entrypoint, or separate setup flow

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
- **THEN** the picker uses its 24-by-6 dimensions, closes before a second popup opens, and the selected tool popup receives the configured size flags

#### Scenario: Picker declares compact dimensions

- **WHEN** the plugin manifest is loaded
- **THEN** the `picker` popup has width `24` and height `6`, independently of user settings for the selected TUI popup

#### Scenario: Picker has no help line and reverses the selected row

- **WHEN** the picker renders its tool rows
- **THEN** it shows no help line or border, uses terminal reset colors, and highlights the active row with reverse video across the popup width

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

### Requirement: Success reports the pane and re-asserts tab focus best-effort

After a successful non-popup open, action mode SHALL report the pane and any
returned tab id, then best-effort focus that tab. After a successful popup
open, it SHALL report the popup without identifiers and SHALL NOT focus a
tab. Both paths SHALL return success without holding the process.

#### Scenario: Open with tab

- **WHEN** a non-popup pane open succeeds with a tab id
- **THEN** the message includes the pane id and tab id, a `tab focus` call is attempted, and its failure does not change the outcome

#### Scenario: Open without tab

- **WHEN** a non-popup pane open succeeds without a tab id
- **THEN** the message shows only the pane id, no tab-focus call is made, and the process exits 0

#### Scenario: Popup opens without pane id reply

- **WHEN** the effective placement is `popup` and the reply is `{"id":"cli:plugin","result":{"type":"ok"}}` — no pane id, no tab id
- **THEN** the action treats the open as successful (it is not a missing-pane-id failure), prints `herdr-gitui: opened gitui popup` on stderr, makes no `tab focus` call, and exits 0 without holding
