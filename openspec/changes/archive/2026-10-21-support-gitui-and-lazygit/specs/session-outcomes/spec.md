# Session outcomes

## MODIFIED Requirements

### Requirement: Outcome decision precedence

The plugin SHALL decide the session outcome from the selected TUI's exit code, the summary text, and agent availability, in the existing fixed precedence: Killed first, then Failed, then NoChanges, then Deliver or NoAgent. Agent presence SHALL continue to require both `HERDR_GITUI_CALLER_PANE` and `HERDR_GITUI_CALLER_AGENT` to be non-blank.

#### Scenario: Missing exit code wins over everything else

- **WHEN** either supported TUI terminates without an exit code, regardless of summary content or caller environment variables
- **THEN** the outcome is Killed

#### Scenario: Non-zero exit code beats summary and agent checks

- **WHEN** either supported TUI exits with a non-zero code, even with a non-empty summary and both caller variables set
- **THEN** the outcome is Failed carrying that exit code

#### Scenario: Clean exit with empty summary is NoChanges

- **WHEN** either supported TUI exits with code 0 and the summary is empty or whitespace-only, even when both caller variables are set
- **THEN** the outcome is NoChanges

#### Scenario: Clean exit with summary and caller decides Deliver vs NoAgent

- **WHEN** either supported TUI exits with code 0 and the summary is non-empty
- **THEN** the outcome is Deliver when both caller variables are non-blank, and NoAgent when either is missing or blank

### Requirement: Session completion restores the caller tab

When the session has a non-blank `HERDR_GITUI_CALLER_TAB`, the plugin SHALL
best-effort focus that tab before closing the session pane on clean
`NoChanges` and `Deliver` outcomes. On held `NoAgent`, `Killed`, and `Failed`
paths, it SHALL attempt to focus the caller tab after the user releases the
hold and before the process exits. A missing tab id or failed focus call SHALL
not change the session outcome. Popup sessions SHALL NOT attempt tab focus.
Popup sessions SHALL close when their TUI process exits and SHALL NOT issue a
pane-close callback.

#### Scenario: Clean completion returns focus before pane teardown

- **WHEN** a tab-placement session ends with `NoChanges` or successfully delivers its summary and `HERDR_GITUI_CALLER_TAB` is `T`
- **THEN** `herdr tab focus T` is attempted before the session pane is closed

#### Scenario: Held session returns focus after Enter

- **WHEN** a tab-placement session is held for an error or missing caller agent and the user presses Enter
- **THEN** `herdr tab focus T` is attempted before the process exits

#### Scenario: Missing caller tab or popup skips restoration

- **WHEN** the caller tab id is absent or the session runs in a popup
- **THEN** no caller-tab focus call is made and the session outcome is unchanged

#### Scenario: Selected TUI popup closes on exit

- **WHEN** a TUI runs in the configured popup and exits
- **THEN** its popup closes as the process exits without a pane-close callback

### Requirement: Session file location, naming, and content

Session summaries SHALL continue to be stored directly under `<state-dir>/sessions/`, where state-dir is the non-blank `HERDR_PLUGIN_STATE_DIR` or the system temp directory when absent or blank. Pane start SHALL create that directory recursively and treat failure as fatal. Each file SHALL be named `<pid>-<unix millis>.txt`; its content SHALL begin with the selected tool label (`gitui session summary:` or `lazygit session summary:`) and then contain the summary produced from the read-only snapshots. The plugin SHALL never read session file contents back after writing them.

#### Scenario: File name carries pid and timestamp

- **WHEN** a summary is written for a process with pid 4242 at unix milliseconds 1700000000000
- **THEN** the file is named `4242-1700000000000.txt` in the sessions directory and its content begins with the selected tool label followed by the summary

#### Scenario: State dir override and fallback

- **WHEN** `HERDR_PLUGIN_STATE_DIR` is blank or unset
- **THEN** the sessions directory is created under the system temp directory; when it is set non-blank, the sessions directory is created under it

#### Scenario: Unusable state dir fails the session

- **WHEN** creating `<state-dir>/sessions/` fails at pane start
- **THEN** the session aborts with `herdr-gitui: failed to create state dir {path}: {e}`

#### Scenario: Contents are never read back

- **WHEN** the plugin works with session files after writing one
- **THEN** the plugin never reads a session file's contents back — the file is a backup copy; only the sessions directory is listed, and only for pruning

#### Scenario: gitui summary identifies its tool

- **WHEN** gitui produces a non-empty session summary
- **THEN** the stored and delivered summary begins with `gitui session summary:`

#### Scenario: lazygit summary identifies its tool

- **WHEN** lazygit produces a non-empty session summary
- **THEN** the stored and delivered summary begins with `lazygit session summary:`
