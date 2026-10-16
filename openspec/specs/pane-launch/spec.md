# pane-launch

## Purpose

Action mode is the entry point herdr invokes for the plugin's `open` action
(declared in `herdr-plugin.toml`: plugin id `gitui`, version `0.1.0`, action
id `open`, pane id `open` with `placement = "tab"`, `min_herdr_version
"0.9.0"`, no keybindings declared). It reads the plugin context herdr passes
as JSON, resolves the repository directory, and calls back into herdr with
`herdr plugin pane open --placement tab` to launch the gitui pane in a new
tab, exporting the resolved directory and the caller pane/agent identities
to the pane through `HERDR_GITUI_*` environment variables. On success the
action prints the new pane id and exits immediately; pane setup and the
session itself continue in pane mode.

## Requirements

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

Action mode's herdr callbacks — pane open and the best-effort tab focus —
SHALL exec the binary at `HERDR_BIN_PATH` and SHALL fail with a contract
error when it is missing or blank. This requirement is the origin of that
contract: every other capability's herdr call cross-references it rather
than restating it, and one shared check guards every herdr call the plugin
makes in either mode (src/herdr.rs:8-13,28-36).

#### Scenario: Bin path absent at pane open

- **WHEN** action mode reaches the pane-open call with `HERDR_BIN_PATH` unset or whitespace-only
- **THEN** the action fails with "herdr did not provide HERDR_BIN_PATH (plugin API v1 contract); cannot call back into herdr"

#### Scenario: Tab focus passes through the same guard

- **WHEN** the best-effort tab-focus call runs with `HERDR_BIN_PATH` missing or blank
- **THEN** the shared check rejects that call too, producing the same contract error as its internal failure (which the best-effort caller ignores)

### Requirement: Pane open passes a fixed argv and requires a pane id in the reply

Action mode SHALL build the pane-open argv in the fixed order shown below;
conditional `--workspace` and caller `--env` pairs appear only when
resolved, while `HERDR_GITUI_DIR` and `--focus` are always present
(src/herdr.rs:60-100). It SHALL require a non-blank
`result.plugin_pane.pane.pane_id` in the reply, treat `tab_id` as optional,
and truncate raw reply snippets in errors at 200 characters
(src/herdr.rs:102-121,151-158).

#### Scenario: Full context

- **WHEN** workspace id `W`, caller pane `P`, and caller agent `A` are all resolved for directory `D`
- **THEN** argv is exactly `plugin pane open --plugin gitui --entrypoint open --placement tab --workspace W --cwd D --env HERDR_GITUI_DIR=D --env HERDR_GITUI_CALLER_PANE=P --env HERDR_GITUI_CALLER_AGENT=A --focus`

#### Scenario: Minimal context

- **WHEN** no workspace id, caller pane, or caller agent resolved for directory `D`
- **THEN** argv is exactly `plugin pane open --plugin gitui --entrypoint open --placement tab --cwd D --env HERDR_GITUI_DIR=D --focus`

#### Scenario: Reply without pane id

- **WHEN** the pane-open reply lacks a non-blank `result.plugin_pane.pane.pane_id` or is not valid JSON
- **THEN** the action fails with an error naming the missing path (or the parse error) and a raw snippet of at most 200 characters

#### Scenario: Reply without tab id

- **WHEN** the reply carries a valid `pane_id` but no non-blank `tab_id`
- **THEN** the open succeeds with no tab id reported

### Requirement: Success reports the pane and re-asserts tab focus best-effort

On a successful open, action mode SHALL print `herdr-gitui: opened gitui
pane {id} in tab {tab}` to stderr when a tab id was returned, or
`herdr-gitui: opened gitui pane {id}` without the tab part otherwise; when
a tab id exists it SHALL then run `herdr tab focus <tab>` best-effort,
ignoring any error, and return success so the process exits 0 without
holding (src/session.rs:118-128, src/main.rs:23-30).

#### Scenario: Open with tab

- **WHEN** pane open succeeds with a tab id
- **THEN** the message includes the pane id and tab id, a `tab focus` call is attempted, and its failure does not change the outcome

#### Scenario: Open without tab

- **WHEN** pane open succeeds without a tab id
- **THEN** the message shows only the pane id, no tab-focus call is made, and the process exits 0
