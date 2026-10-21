# Session outcomes

## Purpose

This capability decides and executes what happens after the selected Git TUI exits inside the plugin pane. The plugin maps its exit status, the built session summary, and the presence of a calling agent to exactly one of five outcomes — Killed, Failed, NoChanges, NoAgent, Deliver — each with a defined effect on session state files, user-visible messages, pane teardown or hold, and process exit code. It also owns the on-disk session summary files (location, naming, creation, removal) and the best-effort pruning of kept summaries older than seven days.

## Requirements

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

### Requirement: Error outcomes report, keep evidence, and hold the pane

On the Killed and Failed outcomes the plugin SHALL report the corresponding
fixed error message, hold the pane per the mode-dispatch hold contract, and
exit with code 1. On Failed the plugin first preserves evidence: a non-blank
summary SHALL be written to a session file before the error is returned, and
that file is kept (src/session.rs:296-312; src/main.rs:22-31).

#### Scenario: Killed path

- **WHEN** the outcome is Killed
- **THEN** stderr shows `herdr-gitui: gitui terminated without an exit code (killed?)`, no session file is written by this path, the pane is held per the mode-dispatch contract, and the process exits 1

#### Scenario: Failed path keeps a non-empty summary

- **WHEN** the outcome is Failed(rc) and the summary is non-blank
- **THEN** the summary is written to a session file, stderr shows `herdr-gitui: gitui failed with exit code {rc}`, the pane is held, and the process exits 1 with the file kept

#### Scenario: Failed with a blank summary writes no file

- **WHEN** the outcome is Failed(rc) and the summary is empty or whitespace-only
- **THEN** no session file is written by this path, and the same failure message, hold, and exit 1 follow (src/session.rs:308-311)

#### Scenario: Failed write failure is silent

- **WHEN** the outcome is Failed(rc) and writing the session file fails
- **THEN** the write error is ignored and the same failure message, hold, and exit 1 follow

### Requirement: NoChanges and NoAgent outcome effects

On the NoChanges outcome the plugin SHALL print
`herdr-gitui: no repository changes detected` to stderr, run best-effort pane
teardown, write no session file, and exit with code 0. On the NoAgent outcome
the plugin SHALL keep the summary in a session file — a failure to produce or
write that file aborts the path as a hard error — print the kept-file
message, hold the pane, and exit with code 0 on success
(src/session.rs:313-326; src/main.rs:22-31).

#### Scenario: NoChanges closes quietly

- **WHEN** the outcome is NoChanges
- **THEN** stderr shows the no-changes message, the pane teardown is attempted, no session file exists, and the process exits 0 without holding

#### Scenario: NoAgent keeps the summary for the user

- **WHEN** the outcome is NoAgent and the session file write succeeds
- **THEN** stderr shows `herdr-gitui: no agent in focused pane; summary kept at {path}`, the pane is held per the mode-dispatch contract, and the process exits 0 with the file kept

#### Scenario: NoAgent write failure is fatal

- **WHEN** the outcome is NoAgent and producing and writing the session file fails — either the timestamp step (`system clock error: {e}`) or the write step (`failed to write session file {path}: {e}`)
- **THEN** the error is reported with the `herdr-gitui: ` prefix, the pane is held per the mode-dispatch contract, and the process exits 1

### Requirement: Best-effort pane teardown

The plugin's pane teardown SHALL attempt `herdr plugin pane close
$HERDR_PANE_ID` via the herdr binary and, when that id is missing or the call
fails, fall back to `herdr tab close $HERDR_TAB_ID`; all failures are logged
to stderr and never propagated (src/session.rs:359-373,370-382). When the
session runs as a popup — the pane process sees `HERDR_GITUI_PLACEMENT=popup`
in its environment, exported by the action — the teardown SHALL make no herdr
call at all: neither `plugin pane close` nor the tab-close fallback. A popup
is a singleton session resource rather than a Herdr pane: its process does
not receive `HERDR_PANE_ID` for itself, and the plugin context its
environment carries points at the underlying tiled caller pane, so any
close-the-own-pane call would target the user's agent pane; the popup closes
naturally when the session process exits (herdr-side behavior). Summary
delivery is unaffected by the popup gate because it addresses
`HERDR_GITUI_CALLER_PANE`, not the session's own pane id. The pane hold its
outcomes invoke is specified by the mode-dispatch capability (see "Hold waits
for one Enter and never hangs on closed stdin"); the hold mechanism is not
re-specified here.

#### Scenario: Pane close succeeds

- **WHEN** a non-popup teardown runs with a non-blank `HERDR_PANE_ID` and the close call succeeds
- **THEN** no tab close is attempted and no message is logged

#### Scenario: Pane close falls back to tab close

- **WHEN** a non-popup `HERDR_PANE_ID` is missing or the pane close fails
- **THEN** `herdr tab close $HERDR_TAB_ID` is attempted when the tab id is present, and any failure of either call is logged (e.g. `herdr-gitui: failed to close pane {pane}: {e}`) without failing the session

#### Scenario: No ids at all is a silent no-op

- **WHEN** teardown runs with both `HERDR_PANE_ID` and `HERDR_TAB_ID` missing or blank
- **THEN** no herdr call is attempted, nothing is logged, and the session outcome is unchanged (src/session.rs:362-372)

#### Scenario: Close on process exit is herdr-side behavior

- **WHEN** the plugin pane process exits, whether or not teardown ran
- **THEN** this capability requires nothing further of the plugin: closing the pane on process exit is herdr-side behavior, not a plugin call

#### Scenario: Popup session makes no close call

- **WHEN** a session with `HERDR_GITUI_PLACEMENT=popup` ends with a summary that is delivered to the caller agent
- **THEN** delivery happens exactly as today (agent prompt to `HERDR_GITUI_CALLER_PANE`, success message, session file removed) and the plugin makes zero `herdr plugin pane close` and zero `herdr tab close` calls; the popup closes on its own when the process exits

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

### Requirement: Pruning of kept session files

At every pane start the plugin SHALL prune the sessions directory by deleting
only `*.txt` entries whose modification time, relative to the current system
time, is at least 7 days old (`SESSION_MAX_AGE`; exactly 7 days counts as
old). Pruning SHALL be best-effort and never fail the session
(src/session.rs:29,393-418).

#### Scenario: Old txt removed, everything else kept

- **WHEN** the sessions directory contains a .txt file with mtime 8 days old, a fresh .txt file, and a non-.txt file
- **THEN** only the old .txt file is removed; a .txt file with mtime exactly at the 7-day boundary is also removed

#### Scenario: Entries that must not be touched are skipped

- **WHEN** a directory entry is not a `.txt` file, has unreadable metadata or mtime, or has a future mtime
- **THEN** pruning leaves it untouched and logs nothing (src/session.rs:399-409)

#### Scenario: Pruning never blocks the session

- **WHEN** the sessions directory cannot be read, or a removal fails
- **THEN** pruning returns without aborting the session; a read failure is skipped silently and a failed removal only logs `herdr-gitui: warning: cannot remove {path}: {e}`

#### Scenario: Pruning logs even a vanished file

- **WHEN** an old `.txt` passes the age test but the removal fails because the file is already gone (`NotFound`)
- **THEN** the removal warning is logged anyway — pruning calls `remove_file` raw with no `NotFound` exemption, unlike the deliver path's `remove()` helper — and the session continues (src/session.rs:411-416 vs src/session.rs:442-452)
